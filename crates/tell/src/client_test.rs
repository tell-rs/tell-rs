use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

use crate::client::Tell;
use crate::config::TellConfig;
use crate::types::LogLevel;

const VALID_KEY: &str = "feed1e11feed1e11feed1e11feed1e11";

/// Read one length-prefixed frame from a TCP stream.
async fn read_frame(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.unwrap();
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await.unwrap();
    payload
}

fn bytes_contain(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Helper: bind a listener, build a Tell client pointed at it, return both.
async fn setup(service: Option<&str>) -> (TcpListener, Tell) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let mut builder = TellConfig::builder(VALID_KEY)
        .endpoint(addr.to_string())
        .batch_size(100)
        .flush_interval(Duration::from_secs(60))
        .max_retries(0)
        .network_timeout(Duration::from_secs(5));

    if let Some(svc) = service {
        builder = builder.service(svc);
    }

    let client = Tell::new(builder.build().unwrap()).unwrap();
    (listener, client)
}

#[tokio::test]
async fn per_entry_service_override() {
    let (listener, client) = setup(Some("default-svc")).await;

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_frame(&mut stream).await
    });

    client.try_log_with_service(
        LogLevel::Info,
        "hello",
        Some("comp"),
        Some("override-svc"),
        None::<serde_json::Value>,
    );
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        bytes_contain(&frame, b"override-svc"),
        "per-entry service override should appear in the frame"
    );

    client.close().await.ok();
}

#[tokio::test]
async fn config_service_used_by_try_log() {
    let (listener, client) = setup(Some("cfg-svc")).await;

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_frame(&mut stream).await
    });

    client.try_log(
        LogLevel::Info,
        "hello",
        Some("comp"),
        None::<serde_json::Value>,
    );
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        bytes_contain(&frame, b"cfg-svc"),
        "config-level service should appear when try_log sends service: None"
    );

    client.close().await.ok();
}

#[tokio::test]
async fn none_service_falls_back_to_config() {
    let (listener, client) = setup(Some("fallback-svc")).await;

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_frame(&mut stream).await
    });

    client.try_log_with_service(
        LogLevel::Info,
        "hello",
        Some("comp"),
        None, // explicit None
        None::<serde_json::Value>,
    );
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        bytes_contain(&frame, b"fallback-svc"),
        "None service should fall back to config service"
    );

    client.close().await.ok();
}

#[tokio::test]
async fn empty_service_normalized_to_config() {
    let (listener, client) = setup(Some("norm-svc")).await;

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_frame(&mut stream).await
    });

    client.try_log_with_service(
        LogLevel::Info,
        "hello",
        Some("comp"),
        Some(""), // empty string — should normalize to None
        None::<serde_json::Value>,
    );
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        bytes_contain(&frame, b"norm-svc"),
        "empty service should normalize to None and fall back to config service"
    );

    client.close().await.ok();
}

#[tokio::test]
async fn no_service_anywhere() {
    let (listener, client) = setup(None).await; // no config service

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_frame(&mut stream).await
    });

    // Use a unique marker so we can verify it's absent
    client.try_log_with_service(
        LogLevel::Info,
        "hello",
        Some("comp"),
        None,
        None::<serde_json::Value>,
    );
    client.flush().await.ok();

    let frame = server.await.unwrap();
    // Frame should still be valid (contains the log), just no service field
    assert!(
        bytes_contain(&frame, b"hello"),
        "log message should be present"
    );

    client.close().await.ok();
}

#[tokio::test]
async fn log_with_service_fires_and_forgets() {
    let (listener, client) = setup(Some("default-svc")).await;

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_frame(&mut stream).await
    });

    client.log_with_service(
        LogLevel::Error,
        "boom",
        Some("comp"),
        Some("fire-forget-svc"),
        None::<serde_json::Value>,
    );
    client.flush().await.ok();

    let frame = server.await.unwrap();
    assert!(
        bytes_contain(&frame, b"fire-forget-svc"),
        "log_with_service should send the per-entry service"
    );

    client.close().await.ok();
}
