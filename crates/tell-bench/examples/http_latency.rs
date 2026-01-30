//! Measures the floor cost of a localhost HTTP POST round-trip.
//!
//! This is the minimum latency any HTTP-based analytics SDK (PostHog, Mixpanel,
//! RudderStack) pays per call — before JSON serialization, before SDK logic.
//!
//! Run: cargo run -p tell-bench --example http_latency --release

use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const HTTP_200: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n1";
const PORT: u16 = 19876;
const WARMUP: usize = 50;
const ITERATIONS: usize = 1000;

/// Minimal TCP server that reads a request and returns 200 OK.
async fn mock_http_server() {
    let listener = TcpListener::bind(("127.0.0.1", PORT)).await.unwrap();
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            let _ = socket.read(&mut buf).await;
            let _ = socket.write_all(HTTP_200).await;
        });
    }
}

#[tokio::main]
async fn main() {
    // Start mock server
    tokio::spawn(mock_http_server());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let url = format!("http://127.0.0.1:{PORT}/capture");
    let client = reqwest::Client::new();

    // JSON payload similar to what PostHog sends
    let payload = serde_json::json!({
        "api_key": "phc_test",
        "event": "Page Viewed",
        "$distinct_id": "user_123",
        "properties": {
            "url": "/home",
            "referrer": "google",
            "$lib": "posthog-rs",
            "$lib_version": "0.3.7"
        }
    });
    let body = serde_json::to_string(&payload).unwrap();

    // Warmup
    for _ in 0..WARMUP {
        let _ = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body.clone())
            .send()
            .await;
    }

    // Measure individual calls
    let mut latencies = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        let _ = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(body.clone())
            .send()
            .await;
        latencies.push(start.elapsed());
    }

    latencies.sort();
    let total: std::time::Duration = latencies.iter().sum();
    let avg = total / ITERATIONS as u32;
    let p50 = latencies[ITERATIONS / 2];
    let p99 = latencies[ITERATIONS * 99 / 100];
    let min = latencies[0];
    let max = latencies[ITERATIONS - 1];

    println!("Localhost HTTP POST (reqwest → mock server)");
    println!("  Iterations: {ITERATIONS}");
    println!("  Payload:    {} bytes JSON", body.len());
    println!();
    println!("  avg: {:>10?}", avg);
    println!("  p50: {:>10?}", p50);
    println!("  p99: {:>10?}", p99);
    println!("  min: {:>10?}", min);
    println!("  max: {:>10?}", max);
    println!();
    println!("This is the FLOOR — the minimum cost of any HTTP-based SDK.");
    println!("PostHog/Mixpanel/RudderStack add JSON serialization, HashMap");
    println!("allocation, base64 encoding, and SDK logic on top of this.");
    println!();
    println!("Tell track() with props!: ~84 ns (channel send, zero I/O)");
    println!("Ratio: ~{}x", avg.as_nanos() / 84);
}
