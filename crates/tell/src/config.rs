//! Client configuration: `TellConfig`, its builder, and presets.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::buffer::DEFAULT_BUFFER_MAX_BYTES;
use crate::error::TellError;
use crate::validation::validate_and_decode_api_key;

/// Default collector endpoint.
pub const DEFAULT_ENDPOINT: &str = "collect.tell.rs:50000";

/// Default localhost endpoint for development.
pub const DEV_ENDPOINT: &str = "localhost:50000";

/// Default in-memory queue capacity (messages).
pub const DEFAULT_QUEUE_CAPACITY: usize = 10_000;

/// Default interval between samples of the registered instruments.
pub const DEFAULT_METRICS_INTERVAL: Duration = Duration::from_secs(15);

/// Configuration for the Tell SDK.
#[derive(Clone)]
pub struct TellConfig {
    /// Decoded 16-byte API key.
    pub(crate) api_key_bytes: [u8; 16],
    /// Service name stamped on every event and log.
    pub(crate) service: Option<String>,
    /// Source hostname/instance stamped on every metric, and the fallback
    /// log source when a log entry has no component.
    pub(crate) source: Option<String>,
    /// Collector host:port.
    pub(crate) endpoint: String,
    /// Max events per batch before flush. Also the maximum frame size in entries.
    pub(crate) batch_size: usize,
    /// Time between automatic flushes.
    pub(crate) flush_interval: Duration,
    /// Retry attempts per failed batch (ignored when a disk buffer is configured).
    pub(crate) max_retries: u32,
    /// Graceful shutdown deadline.
    pub(crate) close_timeout: Duration,
    /// TCP connect and per-frame write timeout.
    pub(crate) network_timeout: Duration,
    /// Error callback.
    pub(crate) on_error: Option<Arc<dyn Fn(TellError) + Send + Sync>>,
    /// Directory for the disk buffer (WAL). `None` disables disk buffering.
    pub(crate) buffer_path: Option<PathBuf>,
    /// Maximum bytes for the disk buffer. Default: 3 GiB when path is set.
    pub(crate) buffer_max_bytes: u64,
    /// Whether to auto-generate and stamp a process-wide session id on
    /// `track`, `revenue`, and log calls. Identity messages (`identify`,
    /// `alias`, `group`) never stamp regardless of this flag.
    pub(crate) enable_session: bool,
    /// In-memory queue capacity between callers and the worker.
    pub(crate) queue_capacity: usize,
    /// How often the worker samples the registered instruments
    /// ([`Tell::metrics`](crate::Tell::metrics)).
    pub(crate) metrics_interval: Duration,
}

impl std::fmt::Debug for TellConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TellConfig")
            .field("endpoint", &self.endpoint)
            .field("batch_size", &self.batch_size)
            .field("flush_interval", &self.flush_interval)
            .field("max_retries", &self.max_retries)
            .field("close_timeout", &self.close_timeout)
            .field("network_timeout", &self.network_timeout)
            .field("buffer_path", &self.buffer_path)
            .field("buffer_max_bytes", &self.buffer_max_bytes)
            .field("enable_session", &self.enable_session)
            .field("queue_capacity", &self.queue_capacity)
            .field("metrics_interval", &self.metrics_interval)
            .finish()
    }
}

/// Builder for constructing a `TellConfig`.
#[must_use = "call .build() to produce a TellConfig"]
pub struct TellConfigBuilder {
    api_key: String,
    service: Option<String>,
    source: Option<String>,
    endpoint: Option<String>,
    batch_size: Option<usize>,
    flush_interval: Option<Duration>,
    max_retries: Option<u32>,
    close_timeout: Option<Duration>,
    network_timeout: Option<Duration>,
    on_error: Option<Arc<dyn Fn(TellError) + Send + Sync>>,
    buffer_path: Option<PathBuf>,
    buffer_max_bytes: Option<u64>,
    enable_session: bool,
    queue_capacity: Option<usize>,
    metrics_interval: Option<Duration>,
}

impl TellConfigBuilder {
    /// Start building a config with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            service: None,
            source: None,
            endpoint: None,
            batch_size: None,
            flush_interval: None,
            max_retries: None,
            close_timeout: None,
            network_timeout: None,
            on_error: None,
            buffer_path: None,
            buffer_max_bytes: None,
            enable_session: false,
            queue_capacity: None,
            metrics_interval: None,
        }
    }

    /// Set the service name stamped on every event and log. No auto-detect for server SDKs.
    pub fn service(mut self, name: impl Into<String>) -> Self {
        self.service = Some(name.into());
        self
    }

    /// Set the source hostname/instance stamped on every metric.
    ///
    /// Also used as the log `source` when a log entry has no component.
    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Set the collector endpoint (`host:port`). Default: `collect.tell.rs:50000`.
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Max entries per batch. Default: `100`.
    ///
    /// Reaching this many queued entries triggers a flush, and no frame ever
    /// carries more than this many entries.
    pub fn batch_size(mut self, size: usize) -> Self {
        self.batch_size = Some(size);
        self
    }

    /// Interval between automatic flushes. Default: `10s`.
    pub fn flush_interval(mut self, interval: Duration) -> Self {
        self.flush_interval = Some(interval);
        self
    }

    /// Retry attempts per failed batch send. Default: `3`.
    ///
    /// Ignored when [`buffer_path`](Self::buffer_path) is set: a failed send
    /// goes straight to the disk buffer and is retried from there on the next
    /// flush tick, so retries never stall ingestion.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = Some(retries);
        self
    }

    /// Deadline for the worker's final flush during [`close`](crate::Tell::close). Default: `5s`.
    ///
    /// `close` may take up to twice this long in the worst case: once waiting
    /// for a full queue to accept the close request, once for the flush.
    pub fn close_timeout(mut self, timeout: Duration) -> Self {
        self.close_timeout = Some(timeout);
        self
    }

    /// TCP connect and per-frame write timeout. Default: `5s`.
    pub fn network_timeout(mut self, timeout: Duration) -> Self {
        self.network_timeout = Some(timeout);
        self
    }

    /// Callback invoked on non-fatal errors (validation failures, send errors, queue full).
    pub fn on_error(mut self, f: impl Fn(TellError) + Send + Sync + 'static) -> Self {
        self.on_error = Some(Arc::new(f));
        self
    }

    /// Set the directory for the disk buffer (WAL).
    ///
    /// When set, failed TCP sends are persisted to disk and retried on subsequent
    /// flush ticks. When `None` (the default), disk buffering is disabled.
    pub fn buffer_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.buffer_path = Some(path.into());
        self
    }

    /// Set the maximum bytes for the disk buffer. Default: 3 GiB when path is set.
    ///
    /// Oldest frames are evicted (FIFO) when the buffer exceeds this limit.
    pub fn buffer_max_bytes(mut self, max_bytes: u64) -> Self {
        self.buffer_max_bytes = Some(max_bytes);
        self
    }

    /// Opt in to process-wide session stamping.
    ///
    /// When enabled, [`Tell::new`](crate::Tell::new) generates one UUID v4 and
    /// stamps it on every `track`, `revenue`, and log call. Identity-control
    /// messages (`identify`, `alias`, `group`) are never stamped — they describe
    /// who the actor is, not what they did.
    ///
    /// Default: off. Without this opt-in, all outbound events and logs carry
    /// `session_id = None`. Use the per-call `_with_session` variants on
    /// [`Tell`](crate::Tell) when sessions belong to upstream actors instead.
    pub fn enable_session(mut self) -> Self {
        self.enable_session = true;
        self
    }

    /// In-memory queue capacity in messages. Default: `10_000`.
    ///
    /// When the queue is full, new messages are dropped and counted; see
    /// [`Tell::dropped`](crate::Tell::dropped) and [`TellError::QueueFull`].
    pub fn queue_capacity(mut self, capacity: usize) -> Self {
        self.queue_capacity = Some(capacity);
        self
    }

    /// How often the worker samples the registered instruments
    /// ([`Tell::metrics`](crate::Tell::metrics)). Default: 15 s. Must be
    /// non-zero; sampling only happens once an instrument is registered.
    pub fn metrics_interval(mut self, interval: Duration) -> Self {
        self.metrics_interval = Some(interval);
        self
    }

    /// Build the config, validating the API key.
    pub fn build(self) -> Result<TellConfig, TellError> {
        let api_key_bytes = validate_and_decode_api_key(&self.api_key)?;

        if let Some(ref s) = self.service
            && s.is_empty()
        {
            return Err(TellError::validation("service", "must not be empty"));
        }
        if self.batch_size == Some(0) {
            return Err(TellError::configuration("batch_size must be at least 1"));
        }
        if self.queue_capacity == Some(0) {
            return Err(TellError::configuration(
                "queue_capacity must be at least 1",
            ));
        }
        if self.metrics_interval == Some(Duration::ZERO) {
            return Err(TellError::configuration(
                "metrics_interval must be non-zero",
            ));
        }

        Ok(TellConfig {
            api_key_bytes,
            service: self.service,
            source: self.source,
            endpoint: self
                .endpoint
                .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string()),
            batch_size: self.batch_size.unwrap_or(100),
            flush_interval: self.flush_interval.unwrap_or(Duration::from_secs(10)),
            max_retries: self.max_retries.unwrap_or(3),
            close_timeout: self.close_timeout.unwrap_or(Duration::from_secs(5)),
            network_timeout: self.network_timeout.unwrap_or(Duration::from_secs(5)),
            on_error: self.on_error,
            buffer_path: self.buffer_path,
            buffer_max_bytes: self.buffer_max_bytes.unwrap_or(DEFAULT_BUFFER_MAX_BYTES),
            enable_session: self.enable_session,
            queue_capacity: self.queue_capacity.unwrap_or(DEFAULT_QUEUE_CAPACITY),
            metrics_interval: self.metrics_interval.unwrap_or(DEFAULT_METRICS_INTERVAL),
        })
    }
}

impl TellConfig {
    /// Start building a config.
    pub fn builder(api_key: impl Into<String>) -> TellConfigBuilder {
        TellConfigBuilder::new(api_key)
    }

    /// Development preset: localhost:50000, batch=10, flush=2s.
    pub fn development(api_key: impl Into<String>) -> Result<Self, TellError> {
        Self::builder(api_key)
            .endpoint(DEV_ENDPOINT)
            .batch_size(10)
            .flush_interval(Duration::from_secs(2))
            .build()
    }

    /// Production preset: default endpoint, batch=100, flush=10s.
    pub fn production(api_key: impl Into<String>) -> Result<Self, TellError> {
        Self::builder(api_key).build()
    }
}
