use thiserror::Error;

/// Errors that can occur in the Tell SDK.
#[derive(Debug, Error)]
pub enum TellError {
    /// Configuration is invalid (thrown at construction time).
    #[error("configuration error: {0}")]
    Configuration(String),

    /// A validation error for a specific field (reported via onError callback).
    #[error("validation error: {field} {reason}")]
    Validation { field: String, reason: String },

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
}

impl TellError {
    pub fn configuration(msg: impl Into<String>) -> Self {
        Self::Configuration(msg.into())
    }

    pub fn validation(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Validation {
            field: field.into(),
            reason: reason.into(),
        }
    }

    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, TellError>;
