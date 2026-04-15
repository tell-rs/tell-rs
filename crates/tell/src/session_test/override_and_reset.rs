use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::error::TellError;
use crate::types::LogLevel;

// --- R4: Per-call _with_session overrides any builder default ---

#[tokio::test]
async fn test_track_with_session_no_builder_default() {
    let (listener, client) = setup_no_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.track_with_session(&SID_A, "user_1", "Test", None::<serde_json::Value>);
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        bytes_contain(&frame, &SID_A),
        "track_with_session must embed the caller-supplied session id in the frame"
    );
    client.close().await.ok();
}

#[tokio::test]
async fn test_track_with_session_overrides_builder_default() {
    let (listener, client) = setup_with_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.track_with_session(&SID_B, "user_1", "Override", None::<serde_json::Value>);
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        bytes_contain(&frame, &SID_B),
        "track_with_session must embed the explicit sid (SID_B) over the builder default"
    );
    client.close().await.ok();
}

#[tokio::test]
async fn test_revenue_with_session_no_builder_default() {
    let (listener, client) = setup_no_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.revenue_with_session(
        &SID_A,
        "user_1",
        19.99,
        "USD",
        "order_rev_001",
        None::<serde_json::Value>,
    );
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        bytes_contain(&frame, &SID_A),
        "revenue_with_session must embed the caller-supplied session id"
    );
    client.close().await.ok();
}

#[tokio::test]
async fn test_revenue_with_session_overrides_builder_default() {
    let (listener, client) = setup_with_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.revenue_with_session(
        &SID_B,
        "user_1",
        19.99,
        "USD",
        "order_rev_002",
        None::<serde_json::Value>,
    );
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        bytes_contain(&frame, &SID_B),
        "revenue_with_session must embed the explicit sid over the builder default"
    );
    client.close().await.ok();
}

// --- R5: log_with_session collapsed helper ---

#[tokio::test]
async fn test_log_with_session_error_level() {
    let (listener, client) = setup_no_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.log_with_session(
        &SID_A,
        LogLevel::Error,
        "crash",
        None,
        None::<serde_json::Value>,
    );
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        bytes_contain(&frame, &SID_A),
        "log_with_session must embed the caller-supplied session id"
    );
    assert!(
        bytes_contain(&frame, b"crash"),
        "log_with_session must embed the log message"
    );
    client.close().await.ok();
}

#[tokio::test]
async fn test_log_with_session_info_level() {
    let (listener, client) = setup_no_session().await;
    let server = tokio::spawn(recv_one_frame(listener));

    client.log_with_session(
        &SID_A,
        LogLevel::Info,
        "heartbeat_info",
        None,
        None::<serde_json::Value>,
    );
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        bytes_contain(&frame, &SID_A),
        "log_with_session Info must embed the session id"
    );
    assert!(
        bytes_contain(&frame, b"heartbeat_info"),
        "log_with_session must embed the log message"
    );
    client.close().await.ok();
}

// --- R6: reset_session rotates only when builder-enabled ---

#[tokio::test]
async fn test_reset_session_rotates_id() {
    let (listener, client) = setup_with_session().await;

    let sid_before = client
        .current_session_id()
        .expect("enable_session must produce a non-None session id");

    client.reset_session();

    let sid_after = client
        .current_session_id()
        .expect("session id must remain non-None after reset");

    assert_ne!(
        sid_before, sid_after,
        "reset_session must produce a different UUID"
    );

    let server = tokio::spawn(recv_one_frame(listener));
    client.track("user_1", "After Reset", None::<serde_json::Value>);
    client.flush().await.ok();
    let frame = server.await.unwrap();

    assert!(
        bytes_contain(&frame, &sid_after),
        "track after reset must stamp the new session id"
    );
    assert!(
        !bytes_contain(&frame, &sid_before),
        "track after reset must NOT stamp the old session id"
    );

    client.close().await.ok();
}

#[tokio::test]
async fn test_reset_session_disabled_invokes_on_error() {
    let errors: Arc<Mutex<Vec<TellError>>> = Arc::new(Mutex::new(Vec::new()));
    let errors_clone = errors.clone();

    let config = TellConfig::builder(VALID_KEY)
        .endpoint("127.0.0.1:19999")
        .batch_size(100)
        .flush_interval(Duration::from_secs(60))
        .max_retries(0)
        .on_error(move |e| errors_clone.lock().unwrap().push(e))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.reset_session();

    let captured = errors.lock().unwrap();
    assert!(
        !captured.is_empty(),
        "reset_session on disabled config must invoke on_error"
    );
    let first = captured[0].to_string().to_lowercase();
    assert!(
        first.contains("session"),
        "on_error message must mention 'session', got: {first}"
    );
}
