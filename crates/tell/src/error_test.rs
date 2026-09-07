use crate::error::TellError;

#[test]
fn test_buffer_error_display() {
    let err = TellError::buffer("disk full");
    assert_eq!(format!("{err}"), "buffer error: disk full");
}

#[test]
fn test_queue_full_display() {
    let err = TellError::QueueFull { dropped: 42 };
    assert_eq!(format!("{err}"), "queue full: 42 messages dropped so far");
}

#[test]
fn test_validation_display() {
    let err = TellError::validation("userId", "is required");
    assert_eq!(format!("{err}"), "validation error: userId is required");
}

#[test]
fn test_io_error_converts() {
    let io = std::io::Error::other("boom");
    let err: TellError = io.into();
    assert!(matches!(err, TellError::Io(_)));
}
