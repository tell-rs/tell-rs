//! The `Tell` client: lifecycle, super properties, and queueing.
//!
//! Event, log, and metric methods live in the sibling modules.

mod events;
mod logs;
mod metrics;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use crossfire::{MAsyncTx, SendTimeoutError};
use parking_lot::RwLock;
use serde_json::Value;
use tokio::sync::oneshot;

use crate::config::TellConfig;
use crate::error::{Result, TellError};
use crate::payload::object_inner;
use crate::props::IntoPayload;
use crate::worker::{Tx, WorkerMessage, spawn_worker};

/// Extra time allowed for the worker's acknowledgement after its own deadline.
const ACK_GRACE: Duration = Duration::from_secs(1);

/// The Tell analytics client.
///
/// `Tell` is `Clone + Send + Sync`. Internally it wraps an `Arc<Inner>`,
/// so cloning is cheap and all clones share the same connection.
///
/// # Example
///
/// ```no_run
/// use tell::{Tell, TellConfig, props};
///
/// #[tokio::main]
/// async fn main() {
///     let client = Tell::new(
///         TellConfig::production("feed1e11feed1e11feed1e11feed1e11").unwrap()
///     ).unwrap();
///
///     client.track("user_123", "Page Viewed", props! { "url" => "/home" });
///     client.identify("user_123", props! { "name" => "Jane" });
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
    /// The process-wide auto session id, present only when
    /// `TellConfigBuilder::enable_session()` was called.
    session_id: RwLock<Option<[u8; 16]>>,
    super_props: RwLock<SuperProps>,
    on_error: Option<Arc<dyn Fn(TellError) + Send + Sync>>,
    /// Blocking sender for the hot path (`try_send` never blocks).
    tx: Tx,
    /// Async sender for control messages from async contexts.
    atx: MAsyncTx<crossfire::mpsc::Array<WorkerMessage>>,
    close_timeout: Duration,
    /// Messages dropped because the queue was full or closed.
    dropped: AtomicU64,
    /// Set while the queue is full; cleared on the next successful send.
    queue_full: AtomicBool,
}

/// Super properties kept both as a map (for mutation) and as a pre-serialized
/// fragment `"k":v,"k2":v2` (spliced into payloads without re-parsing).
#[derive(Default)]
struct SuperProps {
    map: serde_json::Map<String, Value>,
    fragment: Arc<[u8]>,
}

impl SuperProps {
    fn rebuild(&mut self) {
        let bytes = serde_json::to_vec(&self.map).unwrap_or_default();
        self.fragment = match object_inner(Some(&bytes)) {
            Some(inner) => Arc::from(inner),
            None => Arc::from(&[][..]),
        };
    }
}

pub(crate) fn new_uuid_bytes() -> [u8; 16] {
    *uuid::Uuid::new_v4().as_bytes()
}

fn control_error<T>(e: SendTimeoutError<T>) -> TellError {
    match e {
        SendTimeoutError::Timeout(_) => {
            TellError::network("queue full: control message could not be enqueued")
        }
        SendTimeoutError::Disconnected(_) => TellError::Closed,
    }
}

impl Tell {
    /// Create a new Tell client and spawn the background worker.
    ///
    /// Inside a Tokio runtime the worker is spawned on it. Outside one, a
    /// dedicated `tell-worker` thread is started; use
    /// [`close_blocking`](Self::close_blocking) to shut it down.
    ///
    /// # Errors
    ///
    /// Returns `TellError::Io` if the worker thread cannot be spawned.
    pub fn new(config: TellConfig) -> Result<Self> {
        let on_error = config.on_error.clone();
        let close_timeout = config.close_timeout;
        let session_id = config.enable_session.then(new_uuid_bytes);
        let tx = spawn_worker(config)?;
        let atx = MAsyncTx::from(tx.clone());

        Ok(Self {
            inner: Arc::new(Inner {
                device_id: new_uuid_bytes(),
                session_id: RwLock::new(session_id),
                super_props: RwLock::new(SuperProps::default()),
                on_error,
                tx,
                atx,
                close_timeout,
                dropped: AtomicU64::new(0),
                queue_full: AtomicBool::new(false),
            }),
        })
    }

    // --- Super Properties ---

    /// Register properties that will be merged into every track/group/revenue event.
    ///
    /// Accepts `props!{..}`, `Props::new()`, `json!({..})`, or any `Serialize` type.
    /// Per-call properties override super properties with the same key.
    pub fn register(&self, properties: impl IntoPayload) {
        if let Some(bytes) = properties.into_payload()
            && let Ok(Value::Object(map)) = serde_json::from_slice(&bytes)
        {
            let mut sp = self.inner.super_props.write();
            sp.map.extend(map);
            sp.rebuild();
        }
    }

    /// Remove a super property by key.
    pub fn unregister(&self, key: &str) {
        let mut sp = self.inner.super_props.write();
        if sp.map.remove(key).is_some() {
            sp.rebuild();
        }
    }

    /// The pre-serialized super property fragment, or `None` when empty.
    pub(crate) fn super_fragment(&self) -> Option<Arc<[u8]>> {
        let sp = self.inner.super_props.read();
        if sp.fragment.is_empty() {
            None
        } else {
            Some(Arc::clone(&sp.fragment))
        }
    }

    // --- Lifecycle ---

    /// Rotate the process-wide session id.
    ///
    /// Only meaningful when the builder opted in via
    /// [`TellConfigBuilder::enable_session`](crate::TellConfigBuilder::enable_session).
    /// When session stamping is disabled, this is a no-op that reports a
    /// validation error through the configured `on_error` callback. It never
    /// panics and never retroactively enables session stamping.
    pub fn reset_session(&self) {
        let mut session = self.inner.session_id.write();
        match *session {
            Some(_) => *session = Some(new_uuid_bytes()),
            None => {
                drop(session);
                self.report_error(TellError::validation(
                    "session",
                    "is disabled; call .enable_session() on the builder",
                ));
            }
        }
    }

    /// Total messages dropped because the queue was full (or closed).
    ///
    /// Increase [`queue_capacity`](crate::TellConfigBuilder::queue_capacity)
    /// or lower the flush interval if this grows.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.inner.dropped.load(Ordering::Relaxed)
    }

    /// Flush all queued events, logs, and metrics, waiting for completion.
    ///
    /// Waits up to `close_timeout` for a full queue to accept the request,
    /// then up to `close_timeout` plus a small grace period for the worker.
    pub async fn flush(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send_control(WorkerMessage::Flush(tx)).await?;
        self.await_ack(rx, "flush").await
    }

    /// Flush and shut down the background worker.
    ///
    /// Everything queued before the call is flushed. The worker's own flush
    /// deadline is `close_timeout`; on expiry, pending data goes to the disk
    /// buffer if one is configured.
    pub async fn close(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send_control(WorkerMessage::Close(tx)).await?;
        self.await_ack(rx, "close").await
    }

    /// Blocking variant of [`flush`](Self::flush) for programs without a Tokio runtime.
    ///
    /// Returns `TellError::Configuration` when called from inside a Tokio
    /// runtime, where it would block a worker thread; use `flush().await` there.
    pub fn flush_blocking(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send_control_blocking(WorkerMessage::Flush(tx))?;
        rx.blocking_recv().map_err(|_| TellError::Closed)
    }

    /// Blocking variant of [`close`](Self::close) for programs without a Tokio runtime.
    ///
    /// Returns `TellError::Configuration` when called from inside a Tokio
    /// runtime, where it would block a worker thread; use `close().await` there.
    pub fn close_blocking(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send_control_blocking(WorkerMessage::Close(tx))?;
        rx.blocking_recv().map_err(|_| TellError::Closed)
    }

    // --- Internal ---

    async fn send_control(&self, msg: WorkerMessage) -> Result<()> {
        self.inner
            .atx
            .send_timeout(msg, self.inner.close_timeout)
            .await
            .map_err(control_error)
    }

    fn send_control_blocking(&self, msg: WorkerMessage) -> Result<()> {
        if tokio::runtime::Handle::try_current().is_ok() {
            return Err(TellError::configuration(
                "blocking flush/close called inside a Tokio runtime; use the async variant",
            ));
        }
        self.inner
            .tx
            .send_timeout(msg, self.inner.close_timeout)
            .map_err(control_error)
    }

    async fn await_ack(&self, rx: oneshot::Receiver<()>, what: &str) -> Result<()> {
        tokio::time::timeout(self.inner.close_timeout + ACK_GRACE, rx)
            .await
            .map_err(|_| TellError::network(format!("{what} timed out")))?
            .map_err(|_| TellError::Closed)
    }

    /// Enqueue without blocking. Returns `false` and counts the drop when the queue is full.
    ///
    /// `on_error` receives one [`TellError::QueueFull`] per full episode.
    #[inline]
    pub(crate) fn enqueue(&self, msg: WorkerMessage) -> bool {
        match self.inner.tx.try_send(msg) {
            Ok(()) => {
                if self.inner.queue_full.load(Ordering::Relaxed) {
                    self.inner.queue_full.store(false, Ordering::Relaxed);
                }
                true
            }
            Err(_) => {
                let dropped = self.inner.dropped.fetch_add(1, Ordering::Relaxed) + 1;
                // Load first: a swap on every dropped message is a needless RMW.
                if !self.inner.queue_full.load(Ordering::Relaxed)
                    && !self.inner.queue_full.swap(true, Ordering::Relaxed)
                {
                    self.report_error(TellError::QueueFull { dropped });
                }
                false
            }
        }
    }

    pub(crate) fn device_id(&self) -> [u8; 16] {
        self.inner.device_id
    }

    pub(crate) fn read_session_id(&self) -> Option<[u8; 16]> {
        *self.inner.session_id.read()
    }

    /// Expose the current auto-session id for testing only.
    #[cfg(test)]
    pub(crate) fn current_session_id(&self) -> Option<[u8; 16]> {
        *self.inner.session_id.read()
    }

    pub(crate) fn report_error(&self, err: TellError) {
        if let Some(ref cb) = self.inner.on_error {
            cb(err);
        }
    }
}
