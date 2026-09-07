use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};

use crate::client::Tell;
use crate::config::TellConfig;
use crate::error::TellError;

const VALID_KEY: &str = "feed1e11feed1e11feed1e11feed1e11";

async fn read_frame(stream: &mut TcpStream) -> Option<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.ok()?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await.ok()?;
    Some(payload)
}

fn builder(addr: &str) -> crate::config::TellConfigBuilder {
    TellConfig::builder(VALID_KEY)
        .endpoint(addr.to_string())
        .flush_interval(Duration::from_secs(60))
        .max_retries(0)
        .network_timeout(Duration::from_secs(5))
}

#[tokio::test]
async fn test_close_behind_flush_stops_worker() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let client = Tell::new(builder(&addr.to_string()).batch_size(100).build().unwrap()).unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_frame(&mut stream).await
    });

    client.track("u1", "Before", ());
    let flusher = {
        let c = client.clone();
        tokio::spawn(async move { c.flush().await })
    };
    let closer = {
        let c = client.clone();
        tokio::spawn(async move { c.close().await })
    };

    flusher.await.unwrap().unwrap();
    closer.await.unwrap().unwrap();
    assert!(server.await.unwrap().is_some(), "batch sent before close");

    // Worker is gone: a second close must report Closed rather than hang.
    let err = tokio::time::timeout(Duration::from_secs(2), client.close())
        .await
        .expect("close after shutdown must not hang")
        .unwrap_err();
    assert!(matches!(err, TellError::Closed), "got {err:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn test_close_on_full_queue_current_thread_does_not_deadlock() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let client = Tell::new(
        builder(&addr.to_string())
            .queue_capacity(4)
            .batch_size(1000)
            .close_timeout(Duration::from_secs(3))
            .build()
            .unwrap(),
    )
    .unwrap();

    // The worker has not run yet on this single thread, so the queue fills.
    for i in 0..8 {
        client.track("u1", &format!("E{i}"), ());
    }
    assert!(client.dropped() > 0, "queue should overflow");

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_frame(&mut stream).await
    });

    tokio::time::timeout(Duration::from_secs(5), client.close())
        .await
        .expect("close must not deadlock")
        .unwrap();
    assert!(server.await.unwrap().is_some());
}

#[tokio::test]
async fn test_queue_full_reported_once_and_counted() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let reports = Arc::new(AtomicUsize::new(0));
    let r = reports.clone();
    let client = Tell::new(
        builder(&addr.to_string())
            .queue_capacity(2)
            .batch_size(1000)
            .on_error(move |e| {
                if matches!(e, TellError::QueueFull { .. }) {
                    r.fetch_add(1, Ordering::SeqCst);
                }
            })
            .build()
            .unwrap(),
    )
    .unwrap();

    // Fill synchronously before the worker gets scheduled by burst-sending;
    // the multi-thread worker may drain some, so just assert consistency.
    let mut refused = 0u64;
    for _ in 0..50 {
        if !client.try_track("u1", "Burst", ()) {
            refused += 1;
        }
    }
    assert_eq!(client.dropped(), refused);
    if refused > 0 {
        assert_eq!(
            reports.load(Ordering::SeqCst),
            1,
            "one report per full episode"
        );
    }
    client.close().await.ok();
}

#[tokio::test]
async fn test_batches_are_chunked_at_batch_size() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let client = Tell::new(
        builder(&addr.to_string())
            .batch_size(10)
            .queue_capacity(1000)
            .build()
            .unwrap(),
    )
    .unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut frames = 0;
        while read_frame(&mut stream).await.is_some() {
            frames += 1;
        }
        frames
    });

    for i in 0..35 {
        client.track("u1", &format!("E{i}"), ());
    }
    client.close().await.unwrap();

    let frames = server.await.unwrap();
    assert!(
        frames >= 4,
        "35 events at batch_size 10 need >= 4 frames, got {frames}"
    );
}

#[tokio::test]
async fn test_super_properties_spliced_and_overridden() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let client = Tell::new(builder(&addr.to_string()).batch_size(100).build().unwrap()).unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_frame(&mut stream).await.unwrap()
    });

    client.register(crate::props! { "app" => "web", "plan" => "free" });
    client.unregister("missing");
    client.track("u1", "Evt", crate::props! { "plan" => "pro" });
    client.close().await.unwrap();

    let bytes = server.await.unwrap();
    let needle = br#"{"user_id":"u1","app":"web","plan":"free","plan":"pro"}"#;
    assert!(
        bytes.windows(needle.len()).any(|w| w == needle),
        "payload not found in {:?}",
        String::from_utf8_lossy(&bytes)
    );
}

#[test]
fn test_works_without_tokio_runtime() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        use std::io::Read;
        let (mut stream, _) = listener.accept().unwrap();
        let mut len = [0u8; 4];
        stream.read_exact(&mut len).unwrap();
        let mut payload = vec![0u8; u32::from_be_bytes(len) as usize];
        stream.read_exact(&mut payload).unwrap();
        payload
    });

    let client = Tell::new(builder(&addr.to_string()).batch_size(100).build().unwrap()).unwrap();
    client.track_static("u1", "NoRuntime", ());
    client.flush_blocking().unwrap();
    client.close_blocking().unwrap();

    let bytes = server.join().unwrap();
    assert!(bytes.windows(9).any(|w| w == b"NoRuntime"));
}

#[tokio::test]
async fn test_blocking_close_inside_runtime_is_refused() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let client = Tell::new(builder(&addr.to_string()).build().unwrap()).unwrap();
    let err = client.close_blocking().unwrap_err();
    assert!(matches!(err, TellError::Configuration(_)));
    client.close().await.ok();
}
