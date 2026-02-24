//! Full TellConfig builder — all available options with defaults.
//!
//!   cargo run -p tell --example config

use std::time::Duration;
use tell::{Tell, TellConfig};

#[tokio::main]
async fn main() {
    let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
        .endpoint("collect.tell.rs:50000")          // default: collect.tell.rs:50000
        .service("my-api")                           // app-level service name for filtering
        .batch_size(100)                             // default: 100 events per batch
        .flush_interval(Duration::from_secs(10))     // default: 10s between flushes
        .max_retries(3)                              // default: 3 retry attempts
        .close_timeout(Duration::from_secs(5))       // default: 5s graceful shutdown
        .network_timeout(Duration::from_secs(30))    // default: 30s TCP timeout
        .on_error(|e| eprintln!("[Tell] {e}"))       // default: errors are silent
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    client.track("user_1", "Test", None::<serde_json::Value>);
    client.close().await.ok();
}
