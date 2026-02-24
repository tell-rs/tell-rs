/// Re-export encoding types used in the public API.
pub use tell_encoding::{EventType, LogEventType, LogLevel, SchemaType};

/// Queued event ready to be encoded and sent.
#[derive(Debug)]
pub(crate) struct QueuedEvent {
    pub event_type: EventType,
    pub timestamp: u64,
    pub device_id: [u8; 16],
    pub session_id: [u8; 16],
    pub event_name: Option<Box<str>>,
    pub payload: Option<Vec<u8>>,
}

/// Queued log entry ready to be encoded and sent.
#[derive(Debug)]
pub(crate) struct QueuedLog {
    pub level: LogLevel,
    pub timestamp: u64,
    pub session_id: [u8; 16],
    pub component: Option<String>,
    pub payload: Option<Vec<u8>>,
}
