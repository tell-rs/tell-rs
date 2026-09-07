//! Full TellConfig builder — all available options with defaults.
//!
//!   cargo run -p tell --example config

use std::time::Duration;
use tell::{Tell, TellConfig};

#[tokio::main]
async fn main() {
    let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
        .endpoint("collect.tell.rs:50000") // default: collect.tell.rs:50000
        .service("my-api") // app-level service name for filtering
        .batch_size(100) // default: 100 events per batch
        .flush_interval(Duration::from_secs(10)) // default: 10s between flushes
        .max_retries(3) // default: 3 retry attempts (ignored when buffer_path is set)
        .close_timeout(Duration::from_secs(5)) // default: 5s graceful shutdown
        .network_timeout(Duration::from_secs(5)) // default: 5s connect + write timeout
        .queue_capacity(10_000) // default: 10,000 messages in flight
        // .buffer_path("/var/lib/my-api/tell") // default: off; WAL for failed sends
        .on_error(|e| eprintln!("[Tell] {e}")) // default: errors are silent
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.track("user_1", "Test", ());
    println!("dropped: {}", client.dropped());
    client.close().await.ok();
}
