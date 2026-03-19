use std::borrow::Cow;
use std::sync::{Arc, LazyLock};

use parking_lot::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossfire::MTx;
use serde_json::Value;
use tokio::sync::oneshot;

use crate::config::TellConfig;
use crate::error::{Result, TellError};
use crate::props::IntoPayload;
use crate::types::{
    EventType, HistogramParams, LogLevel, MetricType, QueuedEvent, QueuedLog, QueuedMetric,
    Temporality,
};
use crate::validation::{validate_event_name, validate_log_message, validate_user_id};
use crate::worker::{WorkerMessage, spawn_worker};

/// The Tell analytics client.
///
/// `Tell` is `Clone + Send + Sync`. Internally it wraps an `Arc<Inner>`,
/// so cloning is cheap and all clones share the same connection.
///
/// # Example
///
/// ```no_run
/// use tell::{Tell, TellConfig};
/// use serde_json::json;
///
/// #[tokio::main]
/// async fn main() {
///     let client = Tell::new(
///         TellConfig::production("feed1e11feed1e11feed1e11feed1e11").unwrap()
///     ).unwrap();
///
///     client.track("user_123", "Page Viewed", Some(json!({"url": "/home"})));
///     client.identify("user_123", Some(json!({"name": "Jane"})));
///
///     client.close().await.ok();
/// }
/// ```
#[derive(Clone)]
pub struct Tell {
    inner: Arc<Inner>,
}

struct Inner {
    device_id: [u8; 16],
    session_id: RwLock<[u8; 16]>,
    super_properties: RwLock<Arc<serde_json::Map<String, Value>>>,
    on_error: Option<Arc<dyn Fn(TellError) + Send + Sync>>,
    tx: MTx<crossfire::mpsc::Array<WorkerMessage>>,
    close_timeout: Duration,
}

/// Fast millisecond timestamp using quanta's rdtsc clock.
/// Anchored to system time once at startup — subsequent calls use CPU timestamp
/// counter deltas (~2ns) instead of SystemTime::now() syscall (~20ns).
fn now_ms() -> u64 {
    static CLOCK: LazyLock<quanta::Clock> = LazyLock::new(quanta::Clock::new);
    static ANCHOR_SYSTEM: LazyLock<u64> = LazyLock::new(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    });
    static ANCHOR_RAW: LazyLock<u64> = LazyLock::new(|| CLOCK.raw());

    let delta_ns = CLOCK.delta_as_nanos(*ANCHOR_RAW, CLOCK.raw());
    *ANCHOR_SYSTEM + delta_ns / 1_000_000
}

fn new_uuid_bytes() -> [u8; 16] {
    *uuid::Uuid::new_v4().as_bytes()
}

/// Build a JSON object as bytes by merging a key:value pair with optional pre-serialized properties.
///
/// Avoids building a `serde_json::Map` intermediate DOM. `key_colon` is a byte literal
/// like `b"\"user_id\":"`, and `value` is JSON-escaped directly into the output buffer.
/// If `props` is a serialized JSON object, its fields are merged flat.
fn merge_json_payload(key_colon: &[u8], value: &str, props: Option<&[u8]>) -> Vec<u8> {
    let props_inner = props.and_then(|p| {
        if p.len() > 2 && p[0] == b'{' {
            Some(&p[1..]) // inner content including closing '}'
        } else {
            None
        }
    });

    let cap = 2 + key_colon.len() + value.len() + 2 + props_inner.map_or(0, |p| p.len());
    let mut buf = Vec::with_capacity(cap);

    buf.push(b'{');
    buf.extend_from_slice(key_colon);
    // Write JSON-escaped string directly into buf (no intermediate Vec)
    let _ = serde_json::to_writer(&mut buf, &value);

    if let Some(inner) = props_inner {
        buf.push(b',');
        buf.extend_from_slice(inner);
    } else {
        buf.push(b'}');
    }

    buf
}

impl Tell {
    /// Create a new Tell client and spawn the background worker.
    ///
    /// # Errors
    ///
    /// Returns `TellError::Configuration` if the config is invalid.
    pub fn new(config: TellConfig) -> Result<Self> {
        let on_error = config.on_error.clone();
        let close_timeout = config.close_timeout;
        let tx = spawn_worker(config);

        Ok(Self {
            inner: Arc::new(Inner {
                device_id: new_uuid_bytes(),
                session_id: RwLock::new(new_uuid_bytes()),
                super_properties: RwLock::new(Arc::new(serde_json::Map::new())),
                on_error,
                tx,
                close_timeout,
            }),
        })
    }

    // --- Super Properties ---

    /// Register properties that will be merged into every track/group/revenue event.
    ///
    /// Accepts `props!{..}`, `Props::new()`, `json!({..})`, or any `Serialize` type.
    pub fn register(&self, properties: impl IntoPayload) {
        if let Some(bytes) = properties.into_payload()
            && let Ok(Value::Object(map)) = serde_json::from_slice(&bytes)
        {
            let mut sp = self.inner.super_properties.write();
            let inner = Arc::make_mut(&mut sp);
            inner.extend(map);
        }
    }

    /// Remove a super property by key.
    pub fn unregister(&self, key: &str) {
        let mut sp = self.inner.super_properties.write();
        let inner = Arc::make_mut(&mut sp);
        inner.remove(key);
    }

    // --- Events ---

    /// Track a user action.
    ///
    /// Accepts [`Props`](crate::Props), `props!{..}`, `Some(json!({..}))`,
    /// any `Option<impl Serialize>`, or `None::<serde_json::Value>`.
    ///
    /// Never blocks, never panics. Invalid input is reported via `onError`.
    pub fn track(&self, user_id: &str, event_name: &str, properties: impl IntoPayload) {
        if let Err(e) = validate_user_id(user_id) {
            self.report_error(e);
            return;
        }
        if let Err(e) = validate_event_name(event_name) {
            self.report_error(e);
            return;
        }

        let prop_bytes = properties.into_payload();
        let payload = self.build_track_payload_bytes(user_id, prop_bytes);

        let _ = self.inner.tx.try_send(WorkerMessage::Event(QueuedEvent {
            event_type: EventType::Track,
            timestamp: now_ms(),
            device_id: self.inner.device_id,
            session_id: self.read_session_id(),
            event_name: Some(event_name.into()),
            payload,
        }));
    }

    /// Identify a user with optional traits.
    pub fn identify(&self, user_id: &str, traits: impl IntoPayload) {
        if let Err(e) = validate_user_id(user_id) {
            self.report_error(e);
            return;
        }

        // Build {"user_id":"...", ...traits} directly as bytes (flat payload)
        let trait_bytes = traits.into_payload();
        let traits_inner = trait_bytes.as_deref().and_then(|t| {
            if t.len() > 2 && t[0] == b'{' {
                Some(&t[1..])
            } else {
                None
            }
        });
        let cap = 24 + user_id.len() + traits_inner.map_or(0, |t| t.len());
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(b"{\"user_id\":");
        let _ = serde_json::to_writer(&mut buf, &user_id);
        if let Some(inner) = traits_inner {
            buf.push(b',');
            buf.extend_from_slice(inner);
        } else {
            buf.push(b'}');
        }

        let _ = self.inner.tx.try_send(WorkerMessage::Event(QueuedEvent {
            event_type: EventType::Identify,
            timestamp: now_ms(),
            device_id: self.inner.device_id,
            session_id: self.read_session_id(),
            event_name: None,
            payload: Some(buf),
        }));
    }

    /// Associate a user with a group.
    pub fn group(&self, user_id: &str, group_id: &str, properties: impl IntoPayload) {
        if let Err(e) = validate_user_id(user_id) {
            self.report_error(e);
            return;
        }
        if group_id.is_empty() {
            self.report_error(TellError::validation("groupId", "is required"));
            return;
        }

        let prop_bytes = properties.into_payload();
        let sp = self.inner.super_properties.read();
        if sp.is_empty() {
            drop(sp);
            // Fast path: build {"group_id":"...","user_id":"...",...props} as bytes
            let props_inner = prop_bytes.as_deref().and_then(|p| {
                if p.len() > 2 && p[0] == b'{' {
                    Some(&p[1..])
                } else {
                    None
                }
            });
            let cap = 40 + group_id.len() + user_id.len() + props_inner.map_or(0, |p| p.len());
            let mut buf = Vec::with_capacity(cap);
            buf.extend_from_slice(b"{\"group_id\":");
            let _ = serde_json::to_writer(&mut buf, &group_id);
            buf.extend_from_slice(b",\"user_id\":");
            let _ = serde_json::to_writer(&mut buf, &user_id);
            if let Some(inner) = props_inner {
                buf.push(b',');
                buf.extend_from_slice(inner);
            } else {
                buf.push(b'}');
            }

            let _ = self.inner.tx.try_send(WorkerMessage::Event(QueuedEvent {
                event_type: EventType::Group,
                timestamp: now_ms(),
                device_id: self.inner.device_id,
                session_id: self.read_session_id(),
                event_name: None,
                payload: Some(buf),
            }));
        } else {
            // Slow path: clone and merge with super properties
            let mut map = (**sp).clone();
            drop(sp);
            map.insert("group_id".to_string(), Value::String(group_id.to_string()));
            map.insert("user_id".to_string(), Value::String(user_id.to_string()));
            if let Some(bytes) = prop_bytes
                && let Ok(Value::Object(props)) = serde_json::from_slice(&bytes)
            {
                for (k, v) in props {
                    map.insert(k, v);
                }
            }

            let _ = self.inner.tx.try_send(WorkerMessage::Event(QueuedEvent {
                event_type: EventType::Group,
                timestamp: now_ms(),
                device_id: self.inner.device_id,
                session_id: self.read_session_id(),
                event_name: None,
                payload: serde_json::to_vec(&map).ok(),
            }));
        }
    }

    /// Track a revenue event.
    pub fn revenue(
        &self,
        user_id: &str,
        amount: f64,
        currency: &str,
        order_id: &str,
        properties: impl IntoPayload,
    ) {
        if let Err(e) = validate_user_id(user_id) {
            self.report_error(e);
            return;
        }
        if amount <= 0.0 {
            self.report_error(TellError::validation("amount", "must be positive"));
            return;
        }
        if currency.is_empty() {
            self.report_error(TellError::validation("currency", "is required"));
            return;
        }
        if order_id.is_empty() {
            self.report_error(TellError::validation("orderId", "is required"));
            return;
        }

        let prop_bytes = properties.into_payload();
        let sp = self.inner.super_properties.read();
        if sp.is_empty() {
            drop(sp);
            // Fast path: build JSON directly as bytes
            let props_inner = prop_bytes.as_deref().and_then(|p| {
                if p.len() > 2 && p[0] == b'{' {
                    Some(&p[1..])
                } else {
                    None
                }
            });
            let cap = 80
                + user_id.len()
                + currency.len()
                + order_id.len()
                + props_inner.map_or(0, |p| p.len());
            let mut buf = Vec::with_capacity(cap);
            buf.extend_from_slice(b"{\"user_id\":");
            let _ = serde_json::to_writer(&mut buf, &user_id);
            buf.extend_from_slice(b",\"amount\":");
            let _ = serde_json::to_writer(&mut buf, &amount);
            buf.extend_from_slice(b",\"currency\":");
            let _ = serde_json::to_writer(&mut buf, &currency);
            buf.extend_from_slice(b",\"order_id\":");
            let _ = serde_json::to_writer(&mut buf, &order_id);
            if let Some(inner) = props_inner {
                buf.push(b',');
                buf.extend_from_slice(inner);
            } else {
                buf.push(b'}');
            }

            let _ = self.inner.tx.try_send(WorkerMessage::Event(QueuedEvent {
                event_type: EventType::Track,
                timestamp: now_ms(),
                device_id: self.inner.device_id,
                session_id: self.read_session_id(),
                event_name: Some("Order Completed".into()),
                payload: Some(buf),
            }));
        } else {
            // Slow path: clone and merge with super properties
            let mut map = (**sp).clone();
            drop(sp);
            map.insert("user_id".to_string(), Value::String(user_id.to_string()));
            map.insert("amount".to_string(), serde_json::json!(amount));
            map.insert("currency".to_string(), Value::String(currency.to_string()));
            map.insert("order_id".to_string(), Value::String(order_id.to_string()));
            if let Some(bytes) = prop_bytes
                && let Ok(Value::Object(props)) = serde_json::from_slice(&bytes)
            {
                for (k, v) in props {
                    map.insert(k, v);
                }
            }

            let _ = self.inner.tx.try_send(WorkerMessage::Event(QueuedEvent {
                event_type: EventType::Track,
                timestamp: now_ms(),
                device_id: self.inner.device_id,
                session_id: self.read_session_id(),
                event_name: Some("Order Completed".into()),
                payload: serde_json::to_vec(&map).ok(),
            }));
        }
    }

    /// Link two user identities.
    pub fn alias(&self, previous_id: &str, user_id: &str) {
        if previous_id.is_empty() {
            self.report_error(TellError::validation("previousId", "is required"));
            return;
        }
        if let Err(e) = validate_user_id(user_id) {
            self.report_error(e);
            return;
        }

        // Build {"previous_id":"...","user_id":"..."} directly as bytes
        let mut buf = Vec::with_capacity(40 + previous_id.len() + user_id.len());
        buf.extend_from_slice(b"{\"previous_id\":");
        let _ = serde_json::to_writer(&mut buf, &previous_id);
        buf.extend_from_slice(b",\"user_id\":");
        let _ = serde_json::to_writer(&mut buf, &user_id);
        buf.push(b'}');

        let _ = self.inner.tx.try_send(WorkerMessage::Event(QueuedEvent {
            event_type: EventType::Alias,
            timestamp: now_ms(),
            device_id: self.inner.device_id,
            session_id: self.read_session_id(),
            event_name: None,
            payload: Some(buf),
        }));
    }

    // --- Logging ---

    /// Send a structured log entry.
    ///
    /// `component` is an optional label for the module or subsystem that produced
    /// the log (e.g. `"auth"`, `"cache"`, `"db"`). The app-level `service` name
    /// is taken from [`TellConfig`] and stamped automatically.
    ///
    /// Fire-and-forget: silently drops the entry if the channel is full.
    /// Use [`try_log`](Self::try_log) when the caller needs backpressure.
    pub fn log(
        &self,
        level: LogLevel,
        message: &str,
        component: Option<&str>,
        data: impl IntoPayload,
    ) {
        let _ = self.try_log(level, message, component, data);
    }

    /// Send a structured log entry, returning `false` if the channel is full.
    ///
    /// Same as [`log`](Self::log) but lets the caller react to backpressure
    /// (e.g. stop reading a file and retry on the next poll).
    pub fn try_log(
        &self,
        level: LogLevel,
        message: &str,
        component: Option<&str>,
        data: impl IntoPayload,
    ) -> bool {
        if let Err(e) = validate_log_message(message) {
            self.report_error(e);
            return true; // validation error, not channel pressure
        }

        let data_bytes = data.into_payload();
        let payload = merge_json_payload(b"\"message\":", message, data_bytes.as_deref());

        self.inner
            .tx
            .try_send(WorkerMessage::Log(QueuedLog {
                level,
                timestamp: now_ms(),
                session_id: self.read_session_id(),
                component: component.map(|s| s.to_string()),
                payload: Some(payload),
            }))
            .is_ok()
    }

    /// Log at **Emergency** level (RFC 5424 severity 0). System is unusable.
    pub fn log_emergency(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Emergency, message, component, data);
    }

    /// Log at **Alert** level (RFC 5424 severity 1). Immediate action required.
    pub fn log_alert(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Alert, message, component, data);
    }

    /// Log at **Critical** level (RFC 5424 severity 2). Critical failure.
    pub fn log_critical(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Critical, message, component, data);
    }

    /// Log at **Error** level (RFC 5424 severity 3). Runtime error.
    pub fn log_error(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Error, message, component, data);
    }

    /// Log at **Warning** level (RFC 5424 severity 4). Potential issue.
    pub fn log_warning(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Warning, message, component, data);
    }

    /// Log at **Notice** level (RFC 5424 severity 5). Normal but significant.
    pub fn log_notice(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Notice, message, component, data);
    }

    /// Log at **Info** level (RFC 5424 severity 6). Informational.
    pub fn log_info(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Info, message, component, data);
    }

    /// Log at **Debug** level (RFC 5424 severity 7). Debug-level detail.
    pub fn log_debug(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Debug, message, component, data);
    }

    /// Log at **Trace** level (RFC 5424 severity 8). Finest-grained detail.
    pub fn log_trace(&self, message: &str, component: Option<&str>, data: impl IntoPayload) {
        self.log(LogLevel::Trace, message, component, data);
    }

    // --- Metrics ---

    /// Send a gauge metric (point-in-time value).
    ///
    /// Labels are string key-value pairs for metric dimensions
    /// (e.g. `&[("core", "0"), ("host", "web-01")]`).
    ///
    /// Zero heap allocation when name and labels are string literals.
    /// Never blocks, never panics.
    pub fn gauge(&self, name: &'static str, value: f64, labels: &[(&'static str, &'static str)]) {
        self.send_metric(
            MetricType::Gauge,
            name,
            value,
            labels,
            Temporality::Unspecified,
        );
    }

    /// Send a counter metric (cumulative or delta count).
    ///
    /// Uses delta temporality by default (change since last report).
    pub fn counter(&self, name: &'static str, value: f64, labels: &[(&'static str, &'static str)]) {
        self.send_metric(MetricType::Counter, name, value, labels, Temporality::Delta);
    }

    /// Send a counter metric with explicit temporality.
    pub fn counter_with_temporality(
        &self,
        name: &'static str,
        value: f64,
        labels: &[(&'static str, &'static str)],
        temporality: Temporality,
    ) {
        self.send_metric(MetricType::Counter, name, value, labels, temporality);
    }

    /// Send a histogram metric (distribution with explicit buckets).
    ///
    /// `buckets` is a list of `(upper_bound, cumulative_count)` sorted by upper_bound.
    /// Use `f64::INFINITY` for the final catch-all bucket.
    pub fn histogram(
        &self,
        name: &'static str,
        histogram: HistogramParams,
        labels: &[(&'static str, &'static str)],
    ) {
        if name.is_empty() {
            self.report_error(TellError::validation("name", "metric name is required"));
            return;
        }

        let _ = self.inner.tx.try_send(WorkerMessage::Metric(QueuedMetric {
            metric_type: MetricType::Histogram,
            timestamp: now_ms() * 1_000_000,
            name: Cow::Borrowed(name),
            value: 0.0,
            labels: labels
                .iter()
                .map(|&(k, v)| (Cow::Borrowed(k), Cow::Borrowed(v)))
                .collect(),
            temporality: Temporality::Cumulative,
            histogram: Some(histogram),
        }));
    }

    // --- Dynamic label variants (for runtime-generated label values) ---

    /// Send a gauge with dynamic (non-static) label values. Allocates per call.
    pub fn gauge_dyn(&self, name: &'static str, value: f64, labels: &[(&'static str, &str)]) {
        self.send_metric_dyn(
            MetricType::Gauge,
            name,
            value,
            labels,
            Temporality::Unspecified,
        );
    }

    /// Send a counter with dynamic label values. Allocates per call.
    pub fn counter_dyn(&self, name: &'static str, value: f64, labels: &[(&'static str, &str)]) {
        self.send_metric_dyn(MetricType::Counter, name, value, labels, Temporality::Delta);
    }

    /// Send a counter with dynamic label values and explicit temporality.
    pub fn counter_dyn_with_temporality(
        &self,
        name: &'static str,
        value: f64,
        labels: &[(&'static str, &str)],
        temporality: Temporality,
    ) {
        self.send_metric_dyn(MetricType::Counter, name, value, labels, temporality);
    }

    /// Zero-allocation path for fully static names and labels.
    fn send_metric(
        &self,
        metric_type: MetricType,
        name: &'static str,
        value: f64,
        labels: &[(&'static str, &'static str)],
        temporality: Temporality,
    ) {
        if name.is_empty() {
            self.report_error(TellError::validation("name", "metric name is required"));
            return;
        }

        let _ = self.inner.tx.try_send(WorkerMessage::Metric(QueuedMetric {
            metric_type,
            timestamp: now_ms() * 1_000_000,
            name: Cow::Borrowed(name),
            value,
            labels: labels
                .iter()
                .map(|&(k, v)| (Cow::Borrowed(k), Cow::Borrowed(v)))
                .collect(),
            temporality,
            histogram: None,
        }));
    }

    /// Allocation path for dynamic label values (static keys, dynamic values).
    fn send_metric_dyn(
        &self,
        metric_type: MetricType,
        name: &'static str,
        value: f64,
        labels: &[(&'static str, &str)],
        temporality: Temporality,
    ) {
        if name.is_empty() {
            self.report_error(TellError::validation("name", "metric name is required"));
            return;
        }

        let _ = self.inner.tx.try_send(WorkerMessage::Metric(QueuedMetric {
            metric_type,
            timestamp: now_ms() * 1_000_000,
            name: Cow::Borrowed(name),
            value,
            labels: labels
                .iter()
                .map(|&(k, v)| (Cow::Borrowed(k), Cow::Owned(v.to_string())))
                .collect(),
            temporality,
            histogram: None,
        }));
    }

    // --- Lifecycle ---

    /// Rotate the session ID.
    pub fn reset_session(&self) {
        let mut session = self.inner.session_id.write();
        *session = new_uuid_bytes();
    }

    /// Flush all queued events and logs, waiting for completion.
    pub async fn flush(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.inner
            .tx
            .send_timeout(WorkerMessage::Flush(tx), self.inner.close_timeout)
            .map_err(|_| TellError::Closed)?;
        tokio::time::timeout(self.inner.close_timeout, rx)
            .await
            .map_err(|_| TellError::network("flush timed out"))?
            .map_err(|_| TellError::Closed)
    }

    /// Flush and shut down the background worker.
    ///
    /// Waits up to `close_timeout` for the Close message to enter the channel
    /// (handles backpressure when the ring buffer is full), then waits again
    /// for the worker to finish flushing.
    pub async fn close(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.inner
            .tx
            .send_timeout(WorkerMessage::Close(tx), self.inner.close_timeout)
            .map_err(|_| TellError::Closed)?;
        tokio::time::timeout(self.inner.close_timeout, rx)
            .await
            .map_err(|_| TellError::network("close timed out"))?
            .map_err(|_| TellError::Closed)
    }

    // --- Internal ---

    fn read_session_id(&self) -> [u8; 16] {
        *self.inner.session_id.read()
    }

    fn build_track_payload_bytes(
        &self,
        user_id: &str,
        prop_bytes: Option<Vec<u8>>,
    ) -> Option<Vec<u8>> {
        let sp = self.inner.super_properties.read();
        if sp.is_empty() {
            drop(sp);
            Some(merge_json_payload(
                b"\"user_id\":",
                user_id,
                prop_bytes.as_deref(),
            ))
        } else {
            // Slow path: clone and merge with super properties
            let map_clone = (**sp).clone();
            drop(sp);
            let mut map = map_clone;
            map.insert("user_id".into(), Value::String(user_id.to_string()));
            if let Some(bytes) = prop_bytes
                && let Ok(Value::Object(props)) = serde_json::from_slice(&bytes)
            {
                for (k, v) in props {
                    map.insert(k, v);
                }
            }
            serde_json::to_vec(&map).ok()
        }
    }

    fn report_error(&self, err: TellError) {
        if let Some(ref cb) = self.inner.on_error {
            cb(err);
        }
    }
}
