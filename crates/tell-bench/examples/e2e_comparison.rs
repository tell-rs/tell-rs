//! End-to-end comparison: FlashLog (disk) vs Tell SDK (TCP).
//!
//! Run: cargo run -p tell-bench --example e2e_comparison --release
//!
//! FlashLog is a global singleton — finalize() joins the worker thread and
//! can only be called once per process. So we measure flashlog at each N by
//! accumulating messages, then finalize once at the end. Tell creates a fresh
//! client per scenario and measures close().await for each.

use std::time::{Duration, Instant};

use serde::Serialize;
use tell::{Tell, TellConfig};
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

/// 80-byte struct matching flashlog's benchmark payload.
#[derive(Debug, Clone, Serialize)]
struct LogStruct {
    data: [u64; 10],
}

impl Default for LogStruct {
    fn default() -> Self {
        LogStruct {
            data: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        }
    }
}

const N: usize = 100_000;

fn main() {
    println!();
    println!("============================================================");
    println!("  E2E Pipeline Comparison: FlashLog (disk) vs Tell (TCP)");
    println!("  N = {} messages, 80-byte struct payload", format_count(N));
    println!("============================================================");
    println!();

    // --- FlashLog: write to disk ---
    run_flashlog_benchmark();

    println!();

    // --- Tell: send over TCP ---
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(run_tell_benchmark());

    println!();
    println!("------------------------------------------------------------");
    println!("  Architecture comparison:");
    println!();
    println!("  FlashLog path to collector:");
    println!("    caller → channel → worker serialize JSON → disk write");
    println!("    → [Vector/Filebeat reads file] → [network to collector]");
    println!("    = 2+ hops, external infrastructure required");
    println!();
    println!("  Tell SDK path to collector:");
    println!("    caller serialize → channel → worker FlatBuffer encode");
    println!("    → batch → TCP send → collector");
    println!("    = 1 hop, zero extra infrastructure");
    println!("------------------------------------------------------------");
    println!();
}

fn run_flashlog_benchmark() {
    let tmpdir = tempfile::tempdir().expect("failed to create tmpdir");
    let guard = flashlog::Logger::initialize()
        .with_file(tmpdir.path().to_str().unwrap(), "bench")
        .expect("failed to init flashlog file")
        .with_console_report(false)
        .with_msg_buffer_size(1_000_000) // large — only flush on finalize
        .with_msg_flush_interval(u64::MAX) // never auto-flush
        .launch();

    // Warmup: ensure worker thread is spun up
    flashlog::flash_info_ct!("warmup");
    flashlog::flush!();
    std::thread::sleep(Duration::from_millis(200));

    let log_struct = LogStruct::default();

    // Phase 1: Enqueue all messages (caller thread time)
    let enqueue_start = Instant::now();
    for _ in 0..N {
        flashlog::flash_error_ct!(LogStruct = log_struct);
    }
    let enqueue_elapsed = enqueue_start.elapsed();

    // Phase 2: Finalize — joins worker thread, guarantees:
    //   serialize all messages → write to disk → flush → fsync
    let finalize_start = Instant::now();
    drop(guard);
    let finalize_elapsed = finalize_start.elapsed();

    let total = enqueue_elapsed + finalize_elapsed;

    println!("  FlashLog → Disk (tmpdir)");
    println!("  ├─ Caller enqueue:  {:>12}  ({:.0} ns/msg)",
        format_duration(enqueue_elapsed),
        enqueue_elapsed.as_nanos() as f64 / N as f64);
    println!("  ├─ Finalize (join): {:>12}  (serialize + write + fsync)",
        format_duration(finalize_elapsed));
    println!("  └─ Total pipeline:  {:>12}  ({:.0} ns/msg)",
        format_duration(total),
        total.as_nanos() as f64 / N as f64);
}

async fn run_tell_benchmark() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    // Null TCP server — accept connections, discard all data
    let server = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
            });
        }
    });

    let log_struct = LogStruct::default();

    let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
        .endpoint(&addr)
        .batch_size(10_000) // flush in batches of 10K
        .flush_interval(Duration::from_secs(3600))
        .build()
        .unwrap();

    let client = Tell::new(config).unwrap();

    // Warmup connection
    client.log_info("warmup", Some("bench"), None::<serde_json::Value>);
    client.flush().await.unwrap();

    // Phase 1: Enqueue all messages (caller thread time)
    let enqueue_start = Instant::now();
    for _ in 0..N {
        client.log_error("Bench", Some("bench"), Some(&log_struct));
    }
    let enqueue_elapsed = enqueue_start.elapsed();

    // Phase 2: close() — flushes all pending → encodes → sends over TCP → closes
    let close_start = Instant::now();
    client.close().await.ok();
    let close_elapsed = close_start.elapsed();

    let total = enqueue_elapsed + close_elapsed;

    println!("  Tell SDK → TCP (localhost null server)");
    println!("  ├─ Caller enqueue:  {:>12}  ({:.0} ns/msg)",
        format_duration(enqueue_elapsed),
        enqueue_elapsed.as_nanos() as f64 / N as f64);
    println!("  ├─ Close (flush):   {:>12}  (encode + batch + TCP send)",
        format_duration(close_elapsed));
    println!("  └─ Total pipeline:  {:>12}  ({:.0} ns/msg)",
        format_duration(total),
        total.as_nanos() as f64 / N as f64);

    server.abort();
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

fn format_duration(d: Duration) -> String {
    let us = d.as_micros();
    if us >= 1_000_000 {
        format!("{:.2} s", d.as_secs_f64())
    } else if us >= 1_000 {
        format!("{:.1} ms", us as f64 / 1_000.0)
    } else {
        format!("{} us", us)
    }
}
