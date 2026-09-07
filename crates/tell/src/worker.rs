//! Background worker: drains the queue, batches, encodes, and sends.
//!
//! Runs on the caller's Tokio runtime when one exists, otherwise on a
//! dedicated thread with its own current-thread runtime.

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crossfire::{AsyncRx, MTx};
use tell_encoding::{
    BatchParams, DEFAULT_VERSION, EventParams, LabelParam, LogEntryParams, MetricEntryParams,
    SchemaType, encode_batch_into, encode_event_data_into, encode_log_data_into,
    encode_metric_data_into,
};
use tokio::sync::oneshot;

use crate::buffer::DiskBuffer;
use crate::clock;
use crate::config::TellConfig;
use crate::error::TellError;
use crate::transport::TcpTransport;
use crate::types::{QueuedEvent, QueuedLog, QueuedMetric};

/// Sender half handed to the client.
pub(crate) type Tx = MTx<crossfire::mpsc::Array<WorkerMessage>>;
type Rx = AsyncRx<crossfire::mpsc::Array<WorkerMessage>>;
type ErrorCallback = Arc<dyn Fn(TellError) + Send + Sync>;

/// Messages sent to the background worker.
pub(crate) enum WorkerMessage {
    Event(QueuedEvent),
    Log(QueuedLog),
    Metric(QueuedMetric),
    Flush(oneshot::Sender<()>),
    Close(oneshot::Sender<()>),
}

static BATCH_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_batch_id() -> u64 {
    BATCH_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// How often the worker re-anchors the fast clock against `SystemTime`.
const CLOCK_RESYNC_INTERVAL: Duration = Duration::from_secs(1);

fn report(cb: &Option<ErrorCallback>, err: TellError) {
    if let Some(cb) = cb {
        cb(err);
    }
}

/// Spawn the background worker and return the sender for queuing messages.
///
/// Uses the current Tokio runtime when called from inside one. Otherwise
/// spawns a `tell-worker` thread running a current-thread runtime.
pub(crate) fn spawn_worker(config: TellConfig) -> Result<Tx, TellError> {
    crossfire::detect_backoff_cfg();
    let (tx, rx) = crossfire::mpsc::bounded_blocking_async::<WorkerMessage>(config.queue_capacity);

    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(worker_loop(config, rx));
        }
        Err(_) => spawn_dedicated_thread(config, rx)?,
    }

    Ok(tx)
}

fn spawn_dedicated_thread(config: TellConfig, rx: Rx) -> Result<(), TellError> {
    std::thread::Builder::new()
        .name("tell-worker".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match runtime {
                Ok(rt) => rt.block_on(worker_loop(config, rx)),
                Err(e) => report(&config.on_error, TellError::Io(e)),
            }
        })
        .map(drop)
        .map_err(TellError::Io)
}

/// Pending entries, one queue per schema.
#[derive(Default)]
struct Queues {
    events: Vec<QueuedEvent>,
    logs: Vec<QueuedLog>,
    metrics: Vec<QueuedMetric>,
}

impl Queues {
    fn len(&self) -> usize {
        self.events.len() + self.logs.len() + self.metrics.len()
    }
}

/// Acknowledgements collected while draining the channel.
#[derive(Default)]
struct Acks {
    flush: Vec<oneshot::Sender<()>>,
    close: Vec<oneshot::Sender<()>>,
}

impl Acks {
    fn send_all(self) {
        for ack in self.flush.into_iter().chain(self.close) {
            // The caller may have stopped waiting; that is not an error here.
            let _ = ack.send(());
        }
    }
}

fn absorb(queues: &mut Queues, acks: &mut Acks, msg: WorkerMessage) {
    match msg {
        WorkerMessage::Event(e) => queues.events.push(e),
        WorkerMessage::Log(l) => queues.logs.push(l),
        WorkerMessage::Metric(m) => queues.metrics.push(m),
        WorkerMessage::Flush(ack) => acks.flush.push(ack),
        WorkerMessage::Close(ack) => acks.close.push(ack),
    }
}

/// Everything needed to encode and deliver a batch.
struct Sender {
    transport: TcpTransport,
    disk_buffer: Option<DiskBuffer>,
    data_buf: Vec<u8>,
    batch_buf: Vec<u8>,
    api_key: [u8; 16],
    service: Option<String>,
    source: Option<String>,
    batch_size: usize,
    max_retries: u32,
    close_timeout: Duration,
    on_error: Option<ErrorCallback>,
}

impl Sender {
    fn new(config: &TellConfig) -> Self {
        let disk_buffer = config.buffer_path.as_ref().and_then(|path| {
            match DiskBuffer::open(path, config.buffer_max_bytes) {
                Ok(buf) => Some(buf),
                Err(e) => {
                    report(
                        &config.on_error,
                        TellError::buffer(format!("failed to open disk buffer: {e}")),
                    );
                    None
                }
            }
        });

        Self {
            transport: TcpTransport::new(config.endpoint.clone(), config.network_timeout),
            disk_buffer,
            data_buf: Vec::with_capacity(64 * 1024),
            batch_buf: Vec::with_capacity(64 * 1024),
            api_key: config.api_key_bytes,
            service: config.service.clone(),
            source: config.source.clone(),
            batch_size: config.batch_size.max(1),
            max_retries: config.max_retries,
            close_timeout: config.close_timeout,
            on_error: config.on_error.clone(),
        }
    }

    /// Send `batch_buf`, falling back to the disk buffer or the error callback.
    ///
    /// Without a disk buffer: up to `max_retries` retries with exponential
    /// backoff (100ms, 200ms, 400ms, ...). With a disk buffer: one attempt,
    /// then append to the WAL, which the flush tick drains. Retries never
    /// stall ingestion for longer than one network timeout.
    async fn send_with_fallback(&mut self) {
        let attempts = if self.disk_buffer.is_some() {
            1
        } else {
            self.max_retries + 1
        };

        let mut last_err = None;
        for attempt in 0..attempts {
            match self.transport.send_frame(&self.batch_buf).await {
                Ok(()) => return,
                Err(e) => {
                    last_err = Some(e);
                    if attempt + 1 < attempts {
                        tokio::time::sleep(backoff(attempt)).await;
                    }
                }
            }
        }

        if self.disk_buffer.is_some() {
            self.append_to_wal();
        } else if let Some(e) = last_err {
            report(&self.on_error, e);
        }
    }

    /// Append `batch_buf` to the disk buffer, reporting eviction and failure.
    fn append_to_wal(&mut self) {
        let Some(buf) = self.disk_buffer.as_mut() else {
            return;
        };
        match buf.append(&self.batch_buf) {
            Ok(evicted) if evicted > 0 => report(
                &self.on_error,
                TellError::buffer(format!(
                    "disk buffer full — evicted {evicted} bytes of oldest data to make room"
                )),
            ),
            Ok(_) => {}
            Err(e) => report(
                &self.on_error,
                TellError::buffer(format!("failed to buffer batch: {e}")),
            ),
        }
    }
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(100u64 << attempt.min(10))
}

async fn worker_loop(config: TellConfig, rx: Rx) {
    let mut sender = Sender::new(&config);
    let mut queues = Queues::default();
    let drain_limit = config.queue_capacity;

    let mut flush_tick = tokio::time::interval(config.flush_interval);
    flush_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    flush_tick.tick().await; // skip the immediate first tick

    let mut resync_tick = tokio::time::interval(CLOCK_RESYNC_INTERVAL);
    resync_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    resync_tick.tick().await;

    loop {
        tokio::select! {
            msg = rx.recv() => {
                let Ok(msg) = msg else {
                    // All clients dropped: flush what we have and exit.
                    shutdown(&mut sender, &mut queues, Acks::default()).await;
                    return;
                };
                let mut acks = Acks::default();
                absorb(&mut queues, &mut acks, msg);
                // Bulk drain amortises select! overhead under load. Bounded so
                // a saturated producer cannot starve the timers.
                for _ in 0..drain_limit {
                    let Ok(m) = rx.try_recv() else { break };
                    absorb(&mut queues, &mut acks, m);
                }

                if !acks.close.is_empty() {
                    shutdown(&mut sender, &mut queues, acks).await;
                    return;
                }
                if !acks.flush.is_empty() {
                    flush_all(&mut sender, &mut queues).await;
                    acks.send_all();
                    continue;
                }
                flush_full_queues(&mut sender, &mut queues).await;
            }
            _ = flush_tick.tick() => {
                flush_all(&mut sender, &mut queues).await;
            }
            _ = resync_tick.tick() => {
                clock::resync();
            }
        }
    }
}

/// Graceful shutdown: flush everything within `close_timeout`, then ack.
///
/// If the deadline expires (e.g. network is down), save remaining queues to WAL.
async fn shutdown(sender: &mut Sender, queues: &mut Queues, acks: Acks) {
    let deadline = sender.close_timeout;
    if tokio::time::timeout(deadline, flush_all(sender, queues))
        .await
        .is_err()
    {
        report(
            &sender.on_error,
            TellError::network("shutdown flush timed out — saving pending data to disk buffer"),
        );
        save_queues_to_wal(sender, queues);
    }
    sender.transport.close().await;
    acks.send_all();
}

/// Drain the disk buffer, then flush every queue.
async fn flush_all(sender: &mut Sender, queues: &mut Queues) {
    drain_disk_buffer(sender).await;
    flush_queue(sender, &mut queues.events, encode_events).await;
    flush_queue(sender, &mut queues.logs, encode_logs).await;
    flush_queue(sender, &mut queues.metrics, encode_metrics).await;
}

/// Flush only queues that reached `batch_size`.
async fn flush_full_queues(sender: &mut Sender, queues: &mut Queues) {
    let n = sender.batch_size;
    if queues.events.len() >= n {
        flush_queue(sender, &mut queues.events, encode_events).await;
    }
    if queues.logs.len() >= n {
        flush_queue(sender, &mut queues.logs, encode_logs).await;
    }
    if queues.metrics.len() >= n {
        flush_queue(sender, &mut queues.metrics, encode_metrics).await;
    }
}

/// Send a queue in chunks of at most `batch_size`, keeping the Vec's capacity.
async fn flush_queue<T>(sender: &mut Sender, queue: &mut Vec<T>, encode: fn(&mut Sender, &[T])) {
    while !queue.is_empty() {
        let n = queue.len().min(sender.batch_size);
        encode(sender, &queue[..n]);
        sender.send_with_fallback().await;
        queue.drain(..n);
    }
}

/// Encode a queue in chunks straight into the WAL. Synchronous — no network I/O.
fn save_queue<T>(sender: &mut Sender, queue: &mut Vec<T>, encode: fn(&mut Sender, &[T])) {
    while !queue.is_empty() {
        let n = queue.len().min(sender.batch_size);
        encode(sender, &queue[..n]);
        sender.append_to_wal();
        queue.drain(..n);
    }
}

/// Emergency save: encode remaining in-memory queues directly to WAL.
fn save_queues_to_wal(sender: &mut Sender, queues: &mut Queues) {
    if sender.disk_buffer.is_none() {
        let total = queues.len();
        if total > 0 {
            report(
                &sender.on_error,
                TellError::buffer(format!(
                    "no disk buffer configured — dropping {total} unsent items on shutdown"
                )),
            );
        }
        return;
    }
    save_queue(sender, &mut queues.events, encode_events);
    save_queue(sender, &mut queues.logs, encode_logs);
    save_queue(sender, &mut queues.metrics, encode_metrics);
}

/// Send pending WAL frames over TCP, stopping at the first failure.
///
/// The cursor is committed once at the end of the pass.
async fn drain_disk_buffer(sender: &mut Sender) {
    let Some(buf) = sender.disk_buffer.as_mut() else {
        return;
    };
    if buf.is_empty() {
        return;
    }

    loop {
        let frame = match buf.drain_next() {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(e) => {
                report(
                    &sender.on_error,
                    TellError::buffer(format!("disk buffer read error: {e}")),
                );
                break;
            }
        };

        if let Err(send_err) = sender.transport.send_frame(&frame).await {
            // Put the frame back (cursor already advanced past it) and stop.
            if let Err(write_err) = buf.append(&frame) {
                report(
                    &sender.on_error,
                    TellError::buffer(format!("failed to re-buffer frame: {write_err}")),
                );
            }
            report(&sender.on_error, send_err);
            break;
        }
    }

    if let Err(e) = buf.commit() {
        report(
            &sender.on_error,
            TellError::buffer(format!("disk buffer commit error: {e}")),
        );
    }
}

/// Wrap `data_buf[range]` in a Batch envelope into `batch_buf`.
fn finish_batch(sender: &mut Sender, schema_type: SchemaType, range: Range<usize>) {
    sender.batch_buf.clear();
    encode_batch_into(
        &mut sender.batch_buf,
        &BatchParams {
            api_key: &sender.api_key,
            schema_type,
            version: DEFAULT_VERSION,
            batch_id: next_batch_id(),
            data: &sender.data_buf[range],
        },
    );
}

fn encode_events(sender: &mut Sender, chunk: &[QueuedEvent]) {
    let service = sender.service.as_deref();
    let params: Vec<EventParams<'_>> = chunk
        .iter()
        .map(|e| EventParams {
            event_type: e.event_type,
            timestamp: e.timestamp,
            service,
            device_id: Some(&e.device_id),
            session_id: e.session_id.as_ref(),
            event_name: e.event_name.as_deref(),
            payload: e.payload.as_deref(),
        })
        .collect();

    sender.data_buf.clear();
    let range = encode_event_data_into(&mut sender.data_buf, &params);
    finish_batch(sender, SchemaType::Event, range);
}

fn encode_logs(sender: &mut Sender, chunk: &[QueuedLog]) {
    let service = sender.service.as_deref();
    let source = sender.source.as_deref();
    let params: Vec<LogEntryParams<'_>> = chunk
        .iter()
        .map(|l| LogEntryParams {
            event_type: tell_encoding::LogEventType::Log,
            session_id: l.session_id.as_ref(),
            level: l.level,
            timestamp: l.timestamp,
            source: l.component.as_deref().or(source),
            service: l.service.as_deref().or(service),
            payload: l.payload.as_deref(),
        })
        .collect();

    sender.data_buf.clear();
    let range = encode_log_data_into(&mut sender.data_buf, &params);
    finish_batch(sender, SchemaType::Log, range);
}

fn encode_metrics(sender: &mut Sender, chunk: &[QueuedMetric]) {
    let service = sender.service.as_deref();
    let source = sender.source.as_deref();
    let label_vecs: Vec<Vec<LabelParam<'_>>> = chunk
        .iter()
        .map(|m| {
            m.labels
                .iter()
                .map(|(k, v)| LabelParam { key: k, value: v })
                .collect()
        })
        .collect();

    let params: Vec<MetricEntryParams<'_>> = chunk
        .iter()
        .zip(label_vecs.iter())
        .map(|(m, labels)| MetricEntryParams {
            metric_type: m.metric_type,
            timestamp: m.timestamp,
            name: &m.name,
            value: m.value,
            source,
            service,
            labels,
            temporality: m.temporality,
            histogram: m.histogram.as_ref(),
            session_id: None,
        })
        .collect();

    sender.data_buf.clear();
    let range = encode_metric_data_into(&mut sender.data_buf, &params);
    finish_batch(sender, SchemaType::Metric, range);
}
