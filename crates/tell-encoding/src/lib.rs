mod batch;
mod event;
mod helpers;
mod log;
mod metric;

#[cfg(test)]
mod batch_test;
#[cfg(test)]
mod event_test;
#[cfg(test)]
mod log_test;
#[cfg(test)]
mod metric_test;

pub use batch::{encode_batch, encode_batch_into};
pub use event::{encode_event, encode_event_data, encode_event_data_into};
pub use log::{encode_log_data, encode_log_data_into, encode_log_entry};
pub use metric::{encode_metric_data, encode_metric_data_into, encode_metric_entry};

/// API key length in bytes (16 bytes = 32 hex chars).
pub const API_KEY_LENGTH: usize = 16;

/// UUID length in bytes.
pub const UUID_LENGTH: usize = 16;

/// IPv6 address length in bytes.
pub const IPV6_LENGTH: usize = 16;

/// Default protocol version (v1.0 = 100).
pub const DEFAULT_VERSION: u8 = 100;

/// Schema type for routing batches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SchemaType {
    Unknown = 0,
    Event = 1,
    Log = 2,
    Metric = 3,
}

impl SchemaType {
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Event type for analytics events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum EventType {
    #[default]
    Unknown = 0,
    Track = 1,
    Identify = 2,
    Group = 3,
    Alias = 4,
    Enrich = 5,
    Context = 6,
}

impl EventType {
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Log event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum LogEventType {
    Unknown = 0,
    #[default]
    Log = 1,
    Enrich = 2,
}

impl LogEventType {
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Log severity levels following RFC 5424 + trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum LogLevel {
    Emergency = 0,
    Alert = 1,
    Critical = 2,
    Error = 3,
    Warning = 4,
    Notice = 5,
    #[default]
    Info = 6,
    Debug = 7,
    Trace = 8,
}

impl LogLevel {
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Metric type — determines how to interpret the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum MetricType {
    #[default]
    Unknown = 0,
    /// Point-in-time value (cpu %, memory bytes, temperature).
    Gauge = 1,
    /// Cumulative or delta count (requests_total, bytes_sent).
    Counter = 2,
    /// Distribution with buckets (latency, request size).
    Histogram = 3,
}

impl MetricType {
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Aggregation temporality — how counter/histogram values relate to time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Temporality {
    #[default]
    Unspecified = 0,
    /// Total since process start (Prometheus-style).
    Cumulative = 1,
    /// Change since last report (StatsD-style).
    Delta = 2,
}

impl Temporality {
    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Parameters for encoding a single event.
pub struct EventParams<'a> {
    pub event_type: EventType,
    pub timestamp: u64,
    pub service: Option<&'a str>,
    pub device_id: Option<&'a [u8; UUID_LENGTH]>,
    pub session_id: Option<&'a [u8; UUID_LENGTH]>,
    pub event_name: Option<&'a str>,
    pub payload: Option<&'a [u8]>,
}

/// Parameters for encoding a single log entry.
pub struct LogEntryParams<'a> {
    pub event_type: LogEventType,
    pub session_id: Option<&'a [u8; UUID_LENGTH]>,
    pub level: LogLevel,
    pub timestamp: u64,
    pub source: Option<&'a str>,
    pub service: Option<&'a str>,
    pub payload: Option<&'a [u8]>,
}

/// Parameters for encoding a single metric entry.
pub struct MetricEntryParams<'a> {
    pub metric_type: MetricType,
    pub timestamp: u64,
    pub name: &'a str,
    pub value: f64,
    pub source: Option<&'a str>,
    pub service: Option<&'a str>,
    pub labels: &'a [LabelParam<'a>],
    pub temporality: Temporality,
    pub histogram: Option<&'a HistogramParams>,
    pub session_id: Option<&'a [u8; UUID_LENGTH]>,
}

/// A string key-value label for metric dimensions.
pub struct LabelParam<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

/// Histogram data for encoding.
#[derive(Debug, Clone)]
pub struct HistogramParams {
    pub count: u64,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    /// Bucket boundaries and cumulative counts, sorted by `upper_bound`.
    /// Use `f64::INFINITY` for the final catch-all bucket.
    pub buckets: Vec<(f64, u64)>,
}

/// Parameters for encoding a batch.
pub struct BatchParams<'a> {
    pub api_key: &'a [u8; API_KEY_LENGTH],
    pub schema_type: SchemaType,
    pub version: u8,
    pub batch_id: u64,
    pub data: &'a [u8],
}
