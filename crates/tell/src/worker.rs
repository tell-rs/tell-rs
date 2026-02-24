use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crossfire::{AsyncRx, MTx};
use tell_encoding::{
    encode_batch_into, encode_event_data_into, encode_log_data_into,
    BatchParams, EventParams, LogEntryParams, SchemaType,
};
use tokio::sync::oneshot;

use crate::config::TellConfig;
use crate::error::TellError;
use crate::transport::TcpTransport;
use crate::types::{QueuedEvent, QueuedLog};

/// Messages sent to the background worker.
pub(crate) enum WorkerMessage {
    Event(QueuedEvent),
    Log(QueuedLog),
    Flush(oneshot::Sender<()>),
    Close(oneshot::Sender<()>),
}

static BATCH_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_batch_id() -> u64 {
    BATCH_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Default channel capacity — pre-allocated ring buffer slots.
/// 10,000 events at ~160 bytes each ≈ 1.6 MB.
const CHANNEL_CAPACITY: usize = 10_000;

/// Spawn the background worker task.
///
/// Returns the sender for queuing messages.
pub(crate) fn spawn_worker(config: TellConfig) -> MTx<crossfire::mpsc::Array<WorkerMessage>> {
    crossfire::detect_backoff_cfg();
    let (tx, rx) = crossfire::mpsc::bounded_blocking_async::<WorkerMessage>(CHANNEL_CAPACITY);

    tokio::spawn(worker_loop(config, rx));

    tx
}

async fn worker_loop(config: TellConfig, rx: AsyncRx<crossfire::mpsc::Array<WorkerMessage>>) {
    let mut transport = TcpTransport::new(config.endpoint.clone(), config.network_timeout);

    let mut event_queue: Vec<QueuedEvent> = Vec::new();
    let mut log_queue: Vec<QueuedLog> = Vec::new();
    let mut data_buf: Vec<u8> = Vec::with_capacity(64 * 1024);
    let mut batch_buf: Vec<u8> = Vec::with_capacity(64 * 1024);

    let flush_interval = config.flush_interval;
    let batch_size = config.batch_size;
    let max_retries = config.max_retries;
    let api_key = config.api_key_bytes;
    let service = config.service.clone();
    let on_error = config.on_error.clone();

    let mut interval = tokio::time::interval(flush_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip the first immediate tick
    interval.tick().await;

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(WorkerMessage::Event(event)) => {
                        event_queue.push(event);
                        if event_queue.len() >= batch_size {
                            flush_events(&mut transport, &api_key, &service, &mut event_queue, max_retries, &on_error, &mut data_buf, &mut batch_buf).await;
                        }
                    }
                    Ok(WorkerMessage::Log(log)) => {
                        log_queue.push(log);
                        if log_queue.len() >= batch_size {
                            flush_logs(&mut transport, &api_key, &service, &mut log_queue, max_retries, &on_error, &mut data_buf, &mut batch_buf).await;
                        }
                    }
                    Ok(WorkerMessage::Flush(ack)) => {
                        // Drain any remaining messages from the channel first
                        let extra_acks = drain_channel(&rx, &mut event_queue, &mut log_queue);
                        flush_events(&mut transport, &api_key, &service, &mut event_queue, max_retries, &on_error, &mut data_buf, &mut batch_buf).await;
                        flush_logs(&mut transport, &api_key, &service, &mut log_queue, max_retries, &on_error, &mut data_buf, &mut batch_buf).await;
                        let _ = ack.send(());
                        for a in extra_acks {
                            let _ = a.send(());
                        }
                    }
                    Ok(WorkerMessage::Close(ack)) => {
                        let extra_acks = drain_channel(&rx, &mut event_queue, &mut log_queue);
                        flush_events(&mut transport, &api_key, &service, &mut event_queue, max_retries, &on_error, &mut data_buf, &mut batch_buf).await;
                        flush_logs(&mut transport, &api_key, &service, &mut log_queue, max_retries, &on_error, &mut data_buf, &mut batch_buf).await;
                        transport.close().await;
                        let _ = ack.send(());
                        for a in extra_acks {
                            let _ = a.send(());
                        }
                        return;
                    }
                    Err(_) => {
                        // Channel closed — flush remaining and exit
                        flush_events(&mut transport, &api_key, &service, &mut event_queue, max_retries, &on_error, &mut data_buf, &mut batch_buf).await;
                        flush_logs(&mut transport, &api_key, &service, &mut log_queue, max_retries, &on_error, &mut data_buf, &mut batch_buf).await;
                        transport.close().await;
                        return;
                    }
                }
            }
            _ = interval.tick() => {
                if !event_queue.is_empty() {
                    flush_events(&mut transport, &api_key, &service, &mut event_queue, max_retries, &on_error, &mut data_buf, &mut batch_buf).await;
                }
                if !log_queue.is_empty() {
                    flush_logs(&mut transport, &api_key, &service, &mut log_queue, max_retries, &on_error, &mut data_buf, &mut batch_buf).await;
                }
            }
        }
    }
}

/// Drain pending messages from the channel without blocking.
/// Returns any Flush/Close oneshot senders that were found (so callers can ack them).
fn drain_channel(
    rx: &AsyncRx<crossfire::mpsc::Array<WorkerMessage>>,
    events: &mut Vec<QueuedEvent>,
    logs: &mut Vec<QueuedLog>,
) -> Vec<oneshot::Sender<()>> {
    let mut acks = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        match msg {
            WorkerMessage::Event(e) => events.push(e),
            WorkerMessage::Log(l) => logs.push(l),
            WorkerMessage::Flush(ack) | WorkerMessage::Close(ack) => {
                acks.push(ack);
            }
        }
    }
    acks
}

async fn flush_events(
    transport: &mut TcpTransport,
    api_key: &[u8; 16],
    service: &Option<String>,
    queue: &mut Vec<QueuedEvent>,
    max_retries: u32,
    on_error: &Option<std::sync::Arc<dyn Fn(TellError) + Send + Sync>>,
    data_buf: &mut Vec<u8>,
    batch_buf: &mut Vec<u8>,
) {
    if queue.is_empty() {
        return;
    }

    let events: Vec<QueuedEvent> = std::mem::take(queue);

    // Build params borrowing from the queued events
    let params: Vec<EventParams<'_>> = events
        .iter()
        .map(|e| EventParams {
            event_type: e.event_type,
            timestamp: e.timestamp,
            service: service.as_deref(),
            device_id: Some(&e.device_id),
            session_id: Some(&e.session_id),
            event_name: e.event_name.as_deref(),
            payload: e.payload.as_deref(),
        })
        .collect();

    // Reusable buffers: data_buf for EventData, batch_buf for final Batch
    data_buf.clear();
    let range = encode_event_data_into(data_buf, &params);

    batch_buf.clear();
    encode_batch_into(batch_buf, &BatchParams {
        api_key,
        schema_type: SchemaType::Event,
        version: 100,
        batch_id: next_batch_id(),
        data: &data_buf[range],
    });

    send_or_spawn_retry(transport, batch_buf, max_retries, on_error).await;
}

async fn flush_logs(
    transport: &mut TcpTransport,
    api_key: &[u8; 16],
    service: &Option<String>,
    queue: &mut Vec<QueuedLog>,
    max_retries: u32,
    on_error: &Option<std::sync::Arc<dyn Fn(TellError) + Send + Sync>>,
    data_buf: &mut Vec<u8>,
    batch_buf: &mut Vec<u8>,
) {
    if queue.is_empty() {
        return;
    }

    let logs: Vec<QueuedLog> = std::mem::take(queue);

    // Build params borrowing from the queued logs
    // service  → config-level app name (same as events)
    // component → per-log module label, mapped to wire `source` field
    let params: Vec<LogEntryParams<'_>> = logs
        .iter()
        .map(|l| LogEntryParams {
            event_type: tell_encoding::LogEventType::Log,
            session_id: Some(&l.session_id),
            level: l.level,
            timestamp: l.timestamp,
            source: l.component.as_deref(),
            service: service.as_deref(),
            payload: l.payload.as_deref(),
        })
        .collect();

    // Reusable buffers: data_buf for LogData, batch_buf for final Batch
    data_buf.clear();
    let range = encode_log_data_into(data_buf, &params);

    batch_buf.clear();
    encode_batch_into(batch_buf, &BatchParams {
        api_key,
        schema_type: SchemaType::Log,
        version: 100,
        batch_id: next_batch_id(),
        data: &data_buf[range],
    });

    send_or_spawn_retry(transport, batch_buf, max_retries, on_error).await;
}

/// Try sending once inline (fast path). On failure, spawn a retry task
/// so the worker select loop is never blocked by backoff.
async fn send_or_spawn_retry(
    transport: &mut TcpTransport,
    batch: &[u8],
    max_retries: u32,
    on_error: &Option<std::sync::Arc<dyn Fn(TellError) + Send + Sync>>,
) {
    match transport.send_frame(batch).await {
        Ok(()) => {}
        Err(first_err) => {
            if max_retries > 0 {
                let endpoint = transport.endpoint().to_string();
                let timeout = transport.connect_timeout();
                let on_error = on_error.clone();
                let owned = batch.to_vec(); // only allocate on retry path
                tokio::spawn(async move {
                    retry_send(endpoint, timeout, owned, max_retries, on_error).await;
                });
            } else if let Some(cb) = on_error {
                cb(first_err);
            }
        }
    }
}

/// Retry sending a batch on a fresh TCP connection with exponential backoff.
/// Runs as a spawned task so the main worker loop is never blocked.
async fn retry_send(
    endpoint: String,
    connect_timeout: Duration,
    data: Vec<u8>,
    max_retries: u32,
    on_error: Option<std::sync::Arc<dyn Fn(TellError) + Send + Sync>>,
) {
    let mut transport = TcpTransport::new(endpoint, connect_timeout);
    let mut last_err = None;

    for attempt in 1..=max_retries {
        let base = 1000.0 * 1.5_f64.powi(attempt as i32 - 1);
        let jitter = base * 0.2 * rand_f64();
        let delay = (base + jitter).min(30_000.0);
        tokio::time::sleep(Duration::from_millis(delay as u64)).await;

        match transport.send_frame(&data).await {
            Ok(()) => return,
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    if let (Some(err), Some(cb)) = (last_err, on_error) {
        cb(err);
    }
}

/// Simple pseudo-random float in [0, 1) for jitter.
/// Not cryptographic — just for backoff jitter.
fn rand_f64() -> f64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let mut hasher = DefaultHasher::new();
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .hash(&mut hasher);
    std::thread::current().id().hash(&mut hasher);
    // Mix in the batch counter for extra entropy between calls
    BATCH_COUNTER.load(Ordering::Relaxed).hash(&mut hasher);
    let hash = hasher.finish();
    (hash as f64) / (u64::MAX as f64)
}
