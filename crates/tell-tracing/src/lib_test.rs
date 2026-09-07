use super::*;

use std::time::Duration;

use tell::TellConfig;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing_subscriber::layer::SubscriberExt;

/// Read one length-prefixed frame: 4-byte big-endian length, then payload.
async fn read_frame(stream: &mut TcpStream) -> Vec<u8> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.unwrap();
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await.unwrap();
    payload
}

/// Bind a local collector that captures the first frame it receives.
async fn spawn_collector() -> (String, JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_frame(&mut stream).await
    });
    (addr, server)
}

/// A client pointed at `addr` that only sends on `close()`.
fn client_for(addr: &str) -> Tell {
    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint(addr)
        .batch_size(100)
        .flush_interval(Duration::from_secs(60))
        .max_retries(0)
        .build()
        .unwrap();
    Tell::new(config).unwrap()
}

/// Emit `f`'s events through a `TellLayer` only, scoped to this thread.
fn with_layer(layer: TellLayer, f: impl FnOnce()) {
    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, f);
}

async fn collect_frame(server: JoinHandle<Vec<u8>>) -> String {
    let frame = timeout(Duration::from_secs(5), server)
        .await
        .expect("collector timed out")
        .expect("collector task panicked");
    String::from_utf8_lossy(&frame).into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn test_on_event_forwards_message_target_and_fields() {
    let (addr, server) = spawn_collector().await;
    let client = client_for(&addr);

    with_layer(TellLayer::new(client.clone()), || {
        tracing::info!(user = "u1", count = 3, "hello world");
    });
    client.close().await.unwrap();

    let frame = collect_frame(server).await;
    assert!(frame.contains("hello world"), "message missing: {frame}");
    assert!(
        frame.contains("tell_tracing::lib_test"),
        "target missing: {frame}"
    );
    assert!(frame.contains(r#""user":"u1""#), "str field missing");
    assert!(frame.contains(r#""count":3"#), "int field missing");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_on_event_without_message_field_uses_metadata_name() {
    let (addr, server) = spawn_collector().await;
    let client = client_for(&addr);

    with_layer(TellLayer::new(client.clone()), || {
        tracing::warn!(retries = 2u64);
    });
    client.close().await.unwrap();

    let frame = collect_frame(server).await;
    // `metadata.name()` for a macro-generated event is "event <file>:<line>".
    assert!(
        frame.contains("event crates/tell-tracing/src/lib_test.rs"),
        "event name missing: {frame}"
    );
    assert!(frame.contains(r#""retries":2"#), "u64 field missing");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_on_event_records_bool_float_and_debug_fields() {
    let (addr, server) = spawn_collector().await;
    let client = client_for(&addr);

    with_layer(TellLayer::new(client.clone()), || {
        tracing::error!(ok = false, ratio = 1.5, list = ?vec![1, 2], "boom");
    });
    client.close().await.unwrap();

    let frame = collect_frame(server).await;
    assert!(frame.contains(r#""ok":false"#), "bool field missing");
    assert!(frame.contains(r#""ratio":1.5"#), "float field missing");
    assert!(
        frame.contains(r#""list":"[1, 2]""#),
        "debug field missing: {frame}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_with_component_from_target_disabled_omits_target() {
    let (addr, server) = spawn_collector().await;
    let client = client_for(&addr);

    let layer = TellLayer::new(client.clone()).with_component_from_target(false);
    with_layer(layer, || {
        tracing::info!("no component");
    });
    client.close().await.unwrap();

    let frame = collect_frame(server).await;
    assert!(frame.contains("no component"), "message missing: {frame}");
    assert!(
        !frame.contains("tell_tracing::lib_test"),
        "target should be omitted: {frame}"
    );
}

#[test]
fn test_level_to_tell_maps_every_tracing_level() {
    assert_eq!(level_to_tell(&Level::ERROR), LogLevel::Error);
    assert_eq!(level_to_tell(&Level::WARN), LogLevel::Warning);
    assert_eq!(level_to_tell(&Level::INFO), LogLevel::Info);
    assert_eq!(level_to_tell(&Level::DEBUG), LogLevel::Debug);
    assert_eq!(level_to_tell(&Level::TRACE), LogLevel::Trace);
}
