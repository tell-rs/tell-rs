use std::collections::HashSet;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

use tell::{Tell, TellConfig};

const VALID_KEY: &str = "feed1e11feed1e11feed1e11feed1e11";

/// Read exactly one length-prefixed frame.
async fn read_frame(stream: &mut tokio::net::TcpStream) -> Vec<u8> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.unwrap();
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await.unwrap();
    buf
}

/// Concurrent correctness: N tokio tasks share one Tell client and each calls
/// `track_with_session` with its own distinct 16-byte session id. After flush,
/// all N session ids must appear in the outbound frame (one batch).
///
/// This proves the per-call path does not race on shared state — each event
/// carries the sid supplied by its caller, not some other task's sid.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_with_session_each_carries_own_sid() {
    const N: usize = 8;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let client = Tell::new(
        TellConfig::builder(VALID_KEY)
            .endpoint(addr.to_string())
            .batch_size(N * 2) // ensure all N events fit in one batch
            .flush_interval(Duration::from_secs(60))
            .max_retries(0)
            .network_timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
    )
    .unwrap();

    // Generate N distinct 16-byte session ids; each byte is the task index.
    let sids: Vec<[u8; 16]> = (0..N).map(|i| [(i as u8) | 0x80; 16]).collect();

    // Spawn N tasks that share the Tell client.
    let mut handles = Vec::with_capacity(N);
    for i in 0..N {
        let client_clone = client.clone();
        let sid = sids[i];
        handles.push(tokio::spawn(async move {
            client_clone.track_with_session(
                &sid,
                "user_concurrent",
                "Concurrent Event",
                None::<serde_json::Value>,
            );
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    // Drain the single batch.
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_frame(&mut stream).await
    });

    client.flush().await.ok();
    let frame = server.await.unwrap();
    client.close().await.ok();

    // Every session id must appear in the frame.
    let missing: Vec<usize> = sids
        .iter()
        .enumerate()
        .filter(|(_, sid)| !frame.windows(16).any(|w| w == *sid))
        .map(|(i, _)| i)
        .collect();

    assert!(
        missing.is_empty(),
        "concurrent track_with_session: session ids for tasks {:?} were not found in the frame",
        missing
    );

    // All N session ids must be distinct (sanity on test setup).
    let unique: HashSet<[u8; 16]> = sids.iter().copied().collect();
    assert_eq!(unique.len(), N, "test setup: all sids must be distinct");
}
