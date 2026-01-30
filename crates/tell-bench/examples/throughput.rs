//! Sustained delivery throughput benchmark.
//!
//! Enqueues events continuously and lets the worker pipeline naturally
//! (no explicit flush between batches — batch_size triggers auto-flush).
//! Measures actual sustained events/sec delivered over TCP.
//!
//! Run: cargo run -p tell-bench --example throughput --release

use std::time::{Duration, Instant};

use serde_json::json;
use tell::{props, Tell, TellConfig};
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

const TOTAL_EVENTS: usize = 10_000_000;
const BATCH_SIZES: &[usize] = &[10, 100, 500];

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 256 * 1024];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
            });
        }
    });

    let payload = json!({"url": "/home", "referrer": "google", "screen": "1920x1080"});

    println!();
    println!("  Sustained delivery throughput — {} events over TCP", format_count(TOTAL_EVENTS));
    println!("  Null TCP server on localhost, ~200B payload per event");
    println!();
    println!("  {:>10}  {:>12}  {:>12}", "Batch", "Events/sec", "Total");
    println!("  {:>10}  {:>12}  {:>12}", "---", "---", "---");

    for &batch_size in BATCH_SIZES {
        let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
            .endpoint(&addr)
            .batch_size(batch_size)
            .flush_interval(Duration::from_secs(3600)) // only batch_size triggers flush
            .build()
            .unwrap();

        let client = Tell::new(config).unwrap();

        // Warmup connection
        client.track("warmup", "Warmup", None::<serde_json::Value>);
        client.flush().await.unwrap();

        let start = Instant::now();
        for _ in 0..TOTAL_EVENTS {
            client.track("user_bench_123", "Page Viewed", Some(&payload));
        }
        // close() flushes remaining events and waits for TCP delivery
        client.close().await.ok();
        let elapsed = start.elapsed();

        let events_per_sec = TOTAL_EVENTS as f64 / elapsed.as_secs_f64();

        println!(
            "  {:>10}  {:>12}  {:>12}",
            batch_size,
            format_rate(events_per_sec),
            format_duration(elapsed),
        );
    }

    // Same test with Props builder (skip json!() DOM)
    println!();
    println!("  Props builder (~200B payload):");
    println!("  {:>10}  {:>12}  {:>12}", "Batch", "Events/sec", "Total");
    println!("  {:>10}  {:>12}  {:>12}", "---", "---", "---");

    for &batch_size in BATCH_SIZES {
        let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
            .endpoint(&addr)
            .batch_size(batch_size)
            .flush_interval(Duration::from_secs(3600))
            .build()
            .unwrap();

        let client = Tell::new(config).unwrap();
        client.track("warmup", "Warmup", None::<serde_json::Value>);
        client.flush().await.unwrap();

        let start = Instant::now();
        for _ in 0..TOTAL_EVENTS {
            client.track(
                "user_bench_123",
                "Page Viewed",
                props! {"url" => "/home", "referrer" => "google", "screen" => "1920x1080"},
            );
        }
        client.close().await.ok();
        let elapsed = start.elapsed();

        let events_per_sec = TOTAL_EVENTS as f64 / elapsed.as_secs_f64();

        println!(
            "  {:>10}  {:>12}  {:>12}",
            batch_size,
            format_rate(events_per_sec),
            format_duration(elapsed),
        );
    }

    // Same test with no properties (minimal event)
    println!();
    println!("  No properties (minimal event):");
    println!("  {:>10}  {:>12}  {:>12}", "Batch", "Events/sec", "Total");
    println!("  {:>10}  {:>12}  {:>12}", "---", "---", "---");

    for &batch_size in BATCH_SIZES {
        let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
            .endpoint(&addr)
            .batch_size(batch_size)
            .flush_interval(Duration::from_secs(3600))
            .build()
            .unwrap();

        let client = Tell::new(config).unwrap();
        client.track("warmup", "Warmup", None::<serde_json::Value>);
        client.flush().await.unwrap();

        let start = Instant::now();
        for _ in 0..TOTAL_EVENTS {
            client.track("user_bench_123", "Page Viewed", None::<serde_json::Value>);
        }
        client.close().await.ok();
        let elapsed = start.elapsed();

        let events_per_sec = TOTAL_EVENTS as f64 / elapsed.as_secs_f64();

        println!(
            "  {:>10}  {:>12}  {:>12}",
            batch_size,
            format_rate(events_per_sec),
            format_duration(elapsed),
        );
    }

    server.abort();
    println!();
}

fn format_count(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{}M", n / 1_000_000)
    } else if n >= 1_000 {
        format!("{}K", n / 1_000)
    } else {
        n.to_string()
    }
}

fn format_rate(eps: f64) -> String {
    if eps >= 1_000_000.0 {
        format!("{:.1}M", eps / 1_000_000.0)
    } else if eps >= 1_000.0 {
        format!("{:.0}K", eps / 1_000.0)
    } else {
        format!("{:.0}", eps)
    }
}

fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms >= 1000 {
        format!("{:.2}s", d.as_secs_f64())
    } else {
        format!("{}ms", ms)
    }
}
