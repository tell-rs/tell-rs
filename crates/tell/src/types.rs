//! Queued entry types shared by the client and the worker, plus wire enums.

use std::borrow::Cow;

/// Re-export encoding types used in the public API.
pub use tell_encoding::{
    EventType, HistogramParams, LogEventType, LogLevel, MetricType, SchemaType, Temporality,
};

/// Queued event ready to be encoded and sent.
///
/// `session_id` is `None` when the caller has not opted in to session stamping.
/// See [`crate::TellConfigBuilder::enable_session`].
#[derive(Debug)]
pub(crate) struct QueuedEvent {
    pub event_type: EventType,
    pub timestamp: u64,
    pub device_id: [u8; 16],
    pub session_id: Option<[u8; 16]>,
    /// Borrowed for `'static` names (constants, `track_static`), owned otherwise.
    pub event_name: Option<Cow<'static, str>>,
    pub payload: Option<Vec<u8>>,
}

/// Queued log entry ready to be encoded and sent.
///
/// `session_id` is `None` when the caller has not opted in to session stamping.
/// See [`crate::TellConfigBuilder::enable_session`].
#[derive(Debug)]
pub(crate) struct QueuedLog {
    pub level: LogLevel,
    pub timestamp: u64,
    pub session_id: Option<[u8; 16]>,
    pub component: Option<String>,
    /// Per-entry service override. Falls back to config-level service if None.
    pub service: Option<String>,
    pub payload: Option<Vec<u8>>,
}

/// A label key-value pair that avoids allocation for static strings.
///
/// When callers pass `&'static str` literals (the common case), both key and value
/// are `Cow::Borrowed` — zero heap allocation. Dynamic strings use `Cow::Owned`.
pub(crate) type MetricLabel = (Cow<'static, str>, Cow<'static, str>);

/// Queued metric entry ready to be encoded and sent.
///
/// Uses `Cow<'static, str>` for name and labels to avoid heap allocation
/// when callers pass string literals (the overwhelmingly common case).
#[derive(Debug)]
pub(crate) struct QueuedMetric {
    pub metric_type: MetricType,
    /// Nanoseconds since the Unix epoch (events and logs use milliseconds).
    pub timestamp: u64,
    pub name: Cow<'static, str>,
    pub value: f64,
    pub labels: Vec<MetricLabel>,
    pub temporality: Temporality,
    pub histogram: Option<HistogramParams>,
}
