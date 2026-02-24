mod helpers;
mod batch;
mod event;
mod log;

#[cfg(test)]
mod batch_test;
#[cfg(test)]
mod event_test;
#[cfg(test)]
mod log_test;

pub use batch::{encode_batch, encode_batch_into};
pub use event::{encode_event, encode_event_data, encode_event_data_into};
pub use log::{encode_log_entry, encode_log_data, encode_log_data_into};

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

/// Parameters for encoding a batch.
pub struct BatchParams<'a> {
    pub api_key: &'a [u8; API_KEY_LENGTH],
    pub schema_type: SchemaType,
    pub version: u8,
    pub batch_id: u64,
    pub data: &'a [u8],
}
