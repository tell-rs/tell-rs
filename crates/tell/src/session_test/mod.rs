use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

use crate::client::Tell;
use crate::config::TellConfig;

mod default_and_identity;
mod override_and_reset;

pub(super) const VALID_KEY: &str = "feed1e11feed1e11feed1e11feed1e11";
pub(super) const SID_A: [u8; 16] = [0x55u8; 16];
pub(super) const SID_B: [u8; 16] = [0xAAu8; 16];

/// Build a Tell client aimed at the given listener. No session by default.
pub(super) async fn setup_no_session() -> (TcpListener, Tell) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let client = Tell::new(
        TellConfig::builder(VALID_KEY)
            .endpoint(addr.to_string())
            .batch_size(100)
            .flush_interval(Duration::from_secs(60))
            .max_retries(0)
            .network_timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
    )
    .unwrap();
    (listener, client)
}

/// Build a Tell client aimed at the given listener with enable_session on.
pub(super) async fn setup_with_session() -> (TcpListener, Tell) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let client = Tell::new(
        TellConfig::builder(VALID_KEY)
            .endpoint(addr.to_string())
            .batch_size(100)
            .flush_interval(Duration::from_secs(60))
            .max_retries(0)
            .network_timeout(Duration::from_secs(5))
            .enable_session()
            .build()
            .unwrap(),
    )
    .unwrap();
    (listener, client)
}

/// Read exactly one length-prefixed frame from a TCP stream.
pub(super) async fn recv_one_frame(listener: TcpListener) -> Vec<u8> {
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.unwrap();
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await.unwrap();
    payload
}

pub(super) fn bytes_contain(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
