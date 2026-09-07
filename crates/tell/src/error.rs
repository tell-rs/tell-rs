//! Error type and result alias for the SDK.

use thiserror::Error;

/// Errors that can occur in the Tell SDK.
///
/// Marked `#[non_exhaustive]`: new variants may be added in minor releases.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TellError {
    /// Configuration is invalid (thrown at construction time).
    #[error("configuration error: {0}")]
    Configuration(String),

    /// A validation error for a specific field (reported via onError callback).
    #[error("validation error: {field} {reason}")]
    Validation {
        /// Field that failed validation.
        field: String,
        /// Why it failed.
        reason: String,
    },

    /// A network/transport error.
    #[error("network error: {0}")]
    Network(String),

    /// The SDK has been closed and cannot accept new events.
    #[error("client is closed")]
    Closed,

    /// An IO error from the transport layer.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// A disk buffer (WAL) error.
    #[error("buffer error: {0}")]
    Buffer(String),

    /// The in-memory queue is full and messages are being dropped.
    ///
    /// Reported once when the queue transitions to full. `dropped` is the
    /// total number of messages dropped since the client was created; see
    /// [`Tell::dropped`](crate::Tell::dropped) for the live counter.
    #[error("queue full: {dropped} messages dropped so far")]
    QueueFull {
        /// Total messages dropped since the client was created.
        dropped: u64,
    },
}

impl TellError {
    /// Build a [`TellError::Configuration`].
    pub fn configuration(msg: impl Into<String>) -> Self {
        Self::Configuration(msg.into())
    }

    /// Build a [`TellError::Validation`].
    pub fn validation(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Validation {
            field: field.into(),
            reason: reason.into(),
        }
    }

    /// Build a [`TellError::Network`].
    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }

    /// Build a [`TellError::Buffer`].
    pub fn buffer(msg: impl Into<String>) -> Self {
        Self::Buffer(msg.into())
    }
}

/// Result alias using [`TellError`].
pub type Result<T> = std::result::Result<T, TellError>;
