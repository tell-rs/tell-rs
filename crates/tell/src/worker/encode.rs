//! FlatBuffer encoding of one queue chunk into the worker's batch buffer.

use std::ops::Range;

use tell_encoding::{
    BatchParams, DEFAULT_VERSION, EventParams, LabelParam, LogEntryParams, MetricEntryParams,
    SchemaType, encode_batch_into, encode_event_data_into, encode_log_data_into,
    encode_metric_data_into,
};

use super::{Sender, next_batch_id};
use crate::types::{QueuedEvent, QueuedLog, QueuedMetric};

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

pub(super) fn encode_events(sender: &mut Sender, chunk: &[QueuedEvent]) {
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

pub(super) fn encode_logs(sender: &mut Sender, chunk: &[QueuedLog]) {
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

pub(super) fn encode_metrics(sender: &mut Sender, chunk: &[QueuedMetric]) {
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
