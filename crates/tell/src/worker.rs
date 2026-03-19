use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crossfire::{AsyncRx, MTx};
use tell_encoding::{
    BatchParams, EventParams, LabelParam, LogEntryParams, MetricEntryParams, SchemaType,
    encode_batch_into, encode_event_data_into, encode_log_data_into, encode_metric_data_into,
};
use tokio::sync::oneshot;

use crate::buffer::DiskBuffer;
use crate::config::TellConfig;
use crate::error::TellError;
use crate::transport::TcpTransport;
use crate::types::{QueuedEvent, QueuedLog, QueuedMetric};

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

/// Mutable state owned by the worker loop, avoiding long parameter lists.
struct WorkerState {
    transport: TcpTransport,
    disk_buffer: Option<DiskBuffer>,
    event_queue: Vec<QueuedEvent>,
    log_queue: Vec<QueuedLog>,
    metric_queue: Vec<QueuedMetric>,
    data_buf: Vec<u8>,
    batch_buf: Vec<u8>,
    api_key: [u8; 16],
    service: Option<String>,
    source: Option<String>,
    batch_size: usize,
    max_retries: u32,
    on_error: Option<Arc<dyn Fn(TellError) + Send + Sync>>,
}

impl WorkerState {
    fn new(config: &TellConfig) -> Self {
        let disk_buffer = config.buffer_path.as_ref().and_then(|path| {
            match DiskBuffer::open(path, config.buffer_max_bytes) {
                Ok(buf) => Some(buf),
                Err(e) => {
                    if let Some(ref cb) = config.on_error {
                        cb(TellError::buffer(format!(
                            "failed to open disk buffer: {e}"
                        )));
                    }
                    None
                }
            }
        });

        Self {
            transport: TcpTransport::new(config.endpoint.clone(), config.network_timeout),
            disk_buffer,
            event_queue: Vec::new(),
            log_queue: Vec::new(),
            metric_queue: Vec::new(),
            data_buf: Vec::with_capacity(64 * 1024),
            batch_buf: Vec::with_capacity(64 * 1024),
            api_key: config.api_key_bytes,
            service: config.service.clone(),
            source: config.source.clone(),
            batch_size: config.batch_size,
            max_retries: config.max_retries,
            on_error: config.on_error.clone(),
        }
    }
}

async fn worker_loop(config: TellConfig, rx: AsyncRx<crossfire::mpsc::Array<WorkerMessage>>) {
    let flush_interval = config.flush_interval;
    let mut state = WorkerState::new(&config);

    let mut interval = tokio::time::interval(flush_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip the first immediate tick
    interval.tick().await;

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(WorkerMessage::Event(event)) => {
                        state.event_queue.push(event);
                    }
                    Ok(WorkerMessage::Log(log)) => {
                        state.log_queue.push(log);
                    }
                    Ok(WorkerMessage::Metric(metric)) => {
                        state.metric_queue.push(metric);
                    }
                    Ok(WorkerMessage::Flush(ack)) => {
                        drain_channel(
                            &rx, &mut state.event_queue, &mut state.log_queue, &mut state.metric_queue,
                        ).into_iter().for_each(|a| { let _ = a.send(()); });
                        flush_all(&mut state).await;
                        let _ = ack.send(());
                        continue;
                    }
                    Ok(WorkerMessage::Close(ack)) => {
                        drain_channel(
                            &rx, &mut state.event_queue, &mut state.log_queue, &mut state.metric_queue,
                        ).into_iter().for_each(|a| { let _ = a.send(()); });
                        shutdown(&mut state).await;
                        let _ = ack.send(());
                        return;
                    }
                    Err(_) => {
                        shutdown(&mut state).await;
                        return;
                    }
                }

                // Bulk drain: grab all available messages without blocking.
                // Low-throughput: try_recv returns nothing, zero cost.
                // High-throughput: amortises select!/recv overhead across
                // thousands of messages instead of one at a time.
                // Stop if we hit a Flush/Close — those need flush-then-ack.
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        WorkerMessage::Event(e) => state.event_queue.push(e),
                        WorkerMessage::Log(l) => state.log_queue.push(l),
                        WorkerMessage::Metric(m) => state.metric_queue.push(m),
                        WorkerMessage::Flush(ack) => {
                            flush_all(&mut state).await;
                            let _ = ack.send(());
                            break;
                        }
                        WorkerMessage::Close(ack) => {
                            shutdown(&mut state).await;
                            let _ = ack.send(());
                            return;
                        }
                    }
                }

                // Flush any queues that reached batch_size.
                if state.event_queue.len() >= state.batch_size {
                    flush_events(&mut state).await;
                }
                if state.log_queue.len() >= state.batch_size {
                    flush_logs(&mut state).await;
                }
                if state.metric_queue.len() >= state.batch_size {
                    flush_metrics(&mut state).await;
                }
            }
            _ = interval.tick() => {
                drain_disk_buffer(&mut state).await;
                flush_all_nonempty(&mut state).await;
            }
        }
    }
}

/// Graceful shutdown: try to flush everything over TCP within a deadline.
/// If the deadline expires (e.g. network is down), save remaining queues to WAL.
async fn shutdown(state: &mut WorkerState) {
    let deadline = std::time::Duration::from_secs(5);

    match tokio::time::timeout(deadline, flush_all(state)).await {
        Ok(()) => {}
        Err(_) => {
            // Deadline expired — TCP is likely down. Save whatever is left to WAL.
            if let Some(ref cb) = state.on_error {
                cb(TellError::network(
                    "shutdown flush timed out — saving pending data to disk buffer",
                ));
            }
            save_queues_to_wal(state);
        }
    }
    state.transport.close().await;
}

/// Flush all three queues unconditionally.
async fn flush_all(state: &mut WorkerState) {
    drain_disk_buffer(state).await;
    flush_events(state).await;
    flush_logs(state).await;
    flush_metrics(state).await;
}

/// Flush only non-empty queues (used on tick to avoid unnecessary work).
async fn flush_all_nonempty(state: &mut WorkerState) {
    if !state.event_queue.is_empty() {
        flush_events(state).await;
    }
    if !state.log_queue.is_empty() {
        flush_logs(state).await;
    }
    if !state.metric_queue.is_empty() {
        flush_metrics(state).await;
    }
}

/// Drain pending messages from the channel without blocking.
/// Returns any Flush/Close oneshot senders that were found (so callers can ack them).
fn drain_channel(
    rx: &AsyncRx<crossfire::mpsc::Array<WorkerMessage>>,
    events: &mut Vec<QueuedEvent>,
    logs: &mut Vec<QueuedLog>,
    metrics: &mut Vec<QueuedMetric>,
) -> Vec<oneshot::Sender<()>> {
    let mut acks = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        match msg {
            WorkerMessage::Event(e) => events.push(e),
            WorkerMessage::Log(l) => logs.push(l),
            WorkerMessage::Metric(m) => metrics.push(m),
            WorkerMessage::Flush(ack) | WorkerMessage::Close(ack) => {
                acks.push(ack);
            }
        }
    }
    acks
}

/// Try to drain all pending frames from the disk buffer, sending each over TCP.
///
/// Stops on the first send failure (the frames remain on disk for the next tick).
async fn drain_disk_buffer(state: &mut WorkerState) {
    let buf = match state.disk_buffer.as_mut() {
        Some(b) if !b.is_empty() => b,
        _ => return,
    };

    loop {
        let frame = match buf.drain_next() {
            Ok(Some(frame)) => frame,
            Ok(None) => return,
            Err(e) => {
                if let Some(ref cb) = state.on_error {
                    cb(TellError::buffer(format!("disk buffer read error: {e}")));
                }
                return;
            }
        };

        if let Err(send_err) = state.transport.send_frame(&frame).await {
            // Send failed — put the frame back and stop draining.
            // We re-append because the cursor already advanced past it.
            if let Err(write_err) = buf.append(&frame)
                && let Some(ref cb) = state.on_error
            {
                cb(TellError::buffer(format!(
                    "failed to re-buffer frame: {write_err}"
                )));
            }
            if let Some(ref cb) = state.on_error {
                cb(send_err);
            }
            return;
        }
    }
}

async fn flush_events(state: &mut WorkerState) {
    if state.event_queue.is_empty() {
        return;
    }

    let events: Vec<QueuedEvent> = std::mem::take(&mut state.event_queue);

    let params: Vec<EventParams<'_>> = events
        .iter()
        .map(|e| EventParams {
            event_type: e.event_type,
            timestamp: e.timestamp,
            service: state.service.as_deref(),
            device_id: Some(&e.device_id),
            session_id: Some(&e.session_id),
            event_name: e.event_name.as_deref(),
            payload: e.payload.as_deref(),
        })
        .collect();

    state.data_buf.clear();
    let range = encode_event_data_into(&mut state.data_buf, &params);

    state.batch_buf.clear();
    encode_batch_into(
        &mut state.batch_buf,
        &BatchParams {
            api_key: &state.api_key,
            schema_type: SchemaType::Event,
            version: 100,
            batch_id: next_batch_id(),
            data: &state.data_buf[range],
        },
    );

    send_with_fallback(state).await;
}

async fn flush_logs(state: &mut WorkerState) {
    if state.log_queue.is_empty() {
        return;
    }

    let logs: Vec<QueuedLog> = std::mem::take(&mut state.log_queue);

    let params: Vec<LogEntryParams<'_>> = logs
        .iter()
        .map(|l| LogEntryParams {
            event_type: tell_encoding::LogEventType::Log,
            session_id: Some(&l.session_id),
            level: l.level,
            timestamp: l.timestamp,
            source: l.component.as_deref(),
            service: state.service.as_deref(),
            payload: l.payload.as_deref(),
        })
        .collect();

    state.data_buf.clear();
    let range = encode_log_data_into(&mut state.data_buf, &params);

    state.batch_buf.clear();
    encode_batch_into(
        &mut state.batch_buf,
        &BatchParams {
            api_key: &state.api_key,
            schema_type: SchemaType::Log,
            version: 100,
            batch_id: next_batch_id(),
            data: &state.data_buf[range],
        },
    );

    send_with_fallback(state).await;
}

async fn flush_metrics(state: &mut WorkerState) {
    if state.metric_queue.is_empty() {
        return;
    }

    let metrics: Vec<QueuedMetric> = std::mem::take(&mut state.metric_queue);

    let label_vecs: Vec<Vec<LabelParam<'_>>> = metrics
        .iter()
        .map(|m| {
            m.labels
                .iter()
                .map(|(k, v)| LabelParam { key: k, value: v })
                .collect()
        })
        .collect();

    let params: Vec<MetricEntryParams<'_>> = metrics
        .iter()
        .zip(label_vecs.iter())
        .map(|(m, labels)| MetricEntryParams {
            metric_type: m.metric_type,
            timestamp: m.timestamp,
            name: &m.name,
            value: m.value,
            source: state.source.as_deref(),
            service: state.service.as_deref(),
            labels,
            temporality: m.temporality,
            histogram: m.histogram.as_ref(),
            session_id: None,
        })
        .collect();

    state.data_buf.clear();
    let range = encode_metric_data_into(&mut state.data_buf, &params);

    state.batch_buf.clear();
    encode_batch_into(
        &mut state.batch_buf,
        &BatchParams {
            api_key: &state.api_key,
            schema_type: SchemaType::Metric,
            version: 100,
            batch_id: next_batch_id(),
            data: &state.data_buf[range],
        },
    );

    send_with_fallback(state).await;
}

/// Try sending the batch in `state.batch_buf` over TCP.
///
/// Retries up to `max_retries` times with exponential backoff (100ms, 200ms, 400ms, ...).
/// The transport auto-reconnects on each attempt (`ensure_connected()` redials after failure).
///
/// After all retries are exhausted: if a disk buffer is configured, append to the WAL.
/// Otherwise, invoke the error callback and the data is lost.
async fn send_with_fallback(state: &mut WorkerState) {
    let mut last_err = None;

    for attempt in 0..=state.max_retries {
        match state.transport.send_frame(&state.batch_buf).await {
            Ok(()) => return,
            Err(e) => {
                last_err = Some(e);
                if attempt < state.max_retries {
                    let delay_ms = 100u64 << attempt.min(10);
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    // All retries exhausted — fall back to disk buffer or drop.
    let send_err = last_err.expect("loop ran at least once");
    if let Some(ref mut buf) = state.disk_buffer {
        match buf.append(&state.batch_buf) {
            Ok(evicted) if evicted > 0 => {
                if let Some(ref cb) = state.on_error {
                    cb(TellError::buffer(format!(
                        "disk buffer full — evicted {evicted} bytes of oldest data to make room"
                    )));
                }
            }
            Err(e) => {
                if let Some(ref cb) = state.on_error {
                    cb(TellError::buffer(format!("failed to buffer batch: {e}")));
                }
            }
            _ => {}
        }
    } else if let Some(ref cb) = state.on_error {
        cb(send_err);
    }
}

/// Emergency save: encode remaining in-memory queues directly to WAL.
/// Called when the shutdown TCP flush times out. Synchronous — no network I/O.
fn save_queues_to_wal(state: &mut WorkerState) {
    let Some(ref mut buf) = state.disk_buffer else {
        // No disk buffer — data is lost.
        if let Some(ref cb) = state.on_error {
            let total = state.event_queue.len() + state.log_queue.len() + state.metric_queue.len();
            if total > 0 {
                cb(TellError::buffer(format!(
                    "no disk buffer configured — dropping {total} unsent items on shutdown"
                )));
            }
        }
        return;
    };

    // Save pending events
    if !state.event_queue.is_empty() {
        let events: Vec<QueuedEvent> = std::mem::take(&mut state.event_queue);
        let params: Vec<EventParams<'_>> = events
            .iter()
            .map(|e| EventParams {
                event_type: e.event_type,
                timestamp: e.timestamp,
                service: state.service.as_deref(),
                device_id: Some(&e.device_id),
                session_id: Some(&e.session_id),
                event_name: e.event_name.as_deref(),
                payload: e.payload.as_deref(),
            })
            .collect();

        state.data_buf.clear();
        let range = encode_event_data_into(&mut state.data_buf, &params);
        state.batch_buf.clear();
        encode_batch_into(
            &mut state.batch_buf,
            &BatchParams {
                api_key: &state.api_key,
                schema_type: SchemaType::Event,
                version: 100,
                batch_id: next_batch_id(),
                data: &state.data_buf[range],
            },
        );
        let _ = buf.append(&state.batch_buf);
    }

    // Save pending logs
    if !state.log_queue.is_empty() {
        let logs: Vec<QueuedLog> = std::mem::take(&mut state.log_queue);
        let params: Vec<LogEntryParams<'_>> = logs
            .iter()
            .map(|l| LogEntryParams {
                event_type: tell_encoding::LogEventType::Log,
                session_id: Some(&l.session_id),
                level: l.level,
                timestamp: l.timestamp,
                source: l.component.as_deref(),
                service: state.service.as_deref(),
                payload: l.payload.as_deref(),
            })
            .collect();

        state.data_buf.clear();
        let range = encode_log_data_into(&mut state.data_buf, &params);
        state.batch_buf.clear();
        encode_batch_into(
            &mut state.batch_buf,
            &BatchParams {
                api_key: &state.api_key,
                schema_type: SchemaType::Log,
                version: 100,
                batch_id: next_batch_id(),
                data: &state.data_buf[range],
            },
        );
        let _ = buf.append(&state.batch_buf);
    }

    // Save pending metrics
    if !state.metric_queue.is_empty() {
        let metrics: Vec<QueuedMetric> = std::mem::take(&mut state.metric_queue);
        let label_vecs: Vec<Vec<LabelParam<'_>>> = metrics
            .iter()
            .map(|m| {
                m.labels
                    .iter()
                    .map(|(k, v)| LabelParam { key: k, value: v })
                    .collect()
            })
            .collect();

        let params: Vec<MetricEntryParams<'_>> = metrics
            .iter()
            .zip(label_vecs.iter())
            .map(|(m, labels)| MetricEntryParams {
                metric_type: m.metric_type,
                timestamp: m.timestamp,
                name: &m.name,
                value: m.value,
                source: state.source.as_deref(),
                service: state.service.as_deref(),
                labels,
                temporality: m.temporality,
                histogram: m.histogram.as_ref(),
                session_id: None,
            })
            .collect();

        state.data_buf.clear();
        let range = encode_metric_data_into(&mut state.data_buf, &params);
        state.batch_buf.clear();
        encode_batch_into(
            &mut state.batch_buf,
            &BatchParams {
                api_key: &state.api_key,
                schema_type: SchemaType::Metric,
                version: 100,
                batch_id: next_batch_id(),
                data: &state.data_buf[range],
            },
        );
        let _ = buf.append(&state.batch_buf);
    }
}
