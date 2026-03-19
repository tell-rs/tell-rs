use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use serde::Serialize;
use serde_json::json;
use tell::{Tell, TellConfig, props};
use tokio::runtime::Runtime;

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

fn make_tell_client(rt: &Runtime) -> Tell {
    rt.block_on(async {
        let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
            .endpoint("192.0.2.1:50000") // non-routable — sends fail fast
            .batch_size(10_000) // large batch to avoid flush during bench
            .flush_interval(Duration::from_secs(3600))
            .network_timeout(Duration::from_millis(1)) // fail fast on connect
            .max_retries(0) // don't retry — just drop failed batches
            .build()
            .unwrap();

        Tell::new(config).unwrap()
    })
}

fn init_flashlog() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let guard = flashlog::Logger::initialize()
            .with_console_report(false)
            .with_msg_buffer_size(1_000) // flush often to bound memory
            .with_msg_flush_interval(100_000_000) // 100ms
            .launch();
        // Keep the logger thread alive for the duration of the benchmark process
        std::mem::forget(guard);
    });
}

// ── Tell benchmarks first (bounded channel — no OOM risk) ──────────────

fn bench_tell_hot_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison");
    group.throughput(Throughput::Elements(1));
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(3));
    group.warm_up_time(Duration::from_secs(1));

    let rt = Runtime::new().unwrap();
    let client = make_tell_client(&rt);

    group.bench_function("tell/track_no_props", |b| {
        b.iter(|| {
            client.track("user_bench_123", "Page Viewed", None::<serde_json::Value>);
        });
    });

    group.bench_function("tell/track_small_props", |b| {
        b.iter(|| {
            client.track(
                "user_bench_123",
                "Page Viewed",
                Some(json!({"url": "/home", "referrer": "google"})),
            );
        });
    });

    group.bench_function("tell/log_no_data", |b| {
        b.iter(|| {
            client.log_error("Connection refused", Some("api"), None::<serde_json::Value>);
        });
    });

    group.bench_function("tell/log_with_data", |b| {
        b.iter(|| {
            client.log_error(
                "Connection refused",
                Some("api"),
                Some(json!({"host": "db.internal", "port": 5432})),
            );
        });
    });

    // Matching flashlog payload types for direct comparison
    // ── Props (skip json!() DOM allocation) ─────────────────────────
    group.bench_function("tell/track_props", |b| {
        b.iter(|| {
            client.track(
                "user_bench_123",
                "Page Viewed",
                props! {"url" => "/home", "referrer" => "google"},
            );
        });
    });

    group.bench_function("tell/log_props", |b| {
        b.iter(|| {
            client.log_error(
                "Connection refused",
                Some("api"),
                props! {"host" => "db.internal", "port" => 5432},
            );
        });
    });

    // ── Matching flashlog payload types for direct comparison ──────
    group.bench_function("tell/log_i32", |b| {
        b.iter(|| {
            client.log_error("Bench", Some("bench"), Some(json!({"log_int": 42})));
        });
    });

    let log_struct = LogStruct::default();
    group.bench_function("tell/log_struct", |b| {
        b.iter(|| {
            client.log_error("Bench", Some("bench"), Some(&log_struct));
        });
    });

    group.finish();
}

// ── FlashLog benchmarks (unbounded channel — keep iterations bounded) ──

fn bench_flashlog_hot_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison");
    group.throughput(Throughput::Elements(1));
    // Short timing to limit queue buildup — flashlog's unbounded crossbeam
    // channel accumulates ~50M msg/sec at 20ns/send, worker processes ~10M/sec.
    group.sample_size(30);
    group.measurement_time(Duration::from_millis(500));
    group.warm_up_time(Duration::from_millis(300));

    init_flashlog();

    group.bench_function("flashlog/i32", |b| {
        let i: i32 = 42;
        b.iter(|| {
            flashlog::flash_error_ct!(log_int = i);
        });
    });

    // Drain: let worker catch up between benchmarks
    std::thread::sleep(Duration::from_secs(1));

    let log_struct = LogStruct::default();
    group.bench_function("flashlog/80byte_struct", |b| {
        b.iter(|| {
            flashlog::flash_error_ct!(LogStruct = log_struct);
        });
    });

    std::thread::sleep(Duration::from_secs(1));

    group.bench_function("flashlog/message_only", |b| {
        b.iter(|| {
            flashlog::flash_error_ct!("benchmark"; "Connection refused");
        });
    });

    std::thread::sleep(Duration::from_secs(1));

    group.bench_function("flashlog/message_with_data", |b| {
        let i: i32 = 42;
        b.iter(|| {
            flashlog::flash_error_ct!("benchmark"; "Connection refused"; port = i);
        });
    });

    group.finish();

    // Final drain before burst benchmarks
    std::thread::sleep(Duration::from_secs(2));
}

// ── Burst benchmarks ───────────────────────────────────────────────────

fn bench_comparison_burst(c: &mut Criterion) {
    let mut group = c.benchmark_group("comparison_burst");
    group.throughput(Throughput::Elements(1000));
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(1));
    group.warm_up_time(Duration::from_millis(300));

    // Tell bursts first (bounded channel)
    let rt = Runtime::new().unwrap();
    let client = make_tell_client(&rt);

    group.bench_function("tell/1000_tracks", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                client.track(
                    "user_bench_123",
                    "Page Viewed",
                    Some(json!({"url": "/home"})),
                );
            }
        });
    });

    group.bench_function("tell/1000_logs", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                client.log_error(
                    "Connection refused",
                    Some("api"),
                    Some(json!({"port": 5432})),
                );
            }
        });
    });

    // FlashLog bursts (keep short)
    init_flashlog();
    std::thread::sleep(Duration::from_secs(1));

    let log_struct = LogStruct::default();
    group.bench_function("flashlog/1000_structs", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                flashlog::flash_error_ct!(LogStruct = log_struct);
            }
        });
    });

    std::thread::sleep(Duration::from_secs(1));

    group.bench_function("flashlog/1000_i32s", |b| {
        let i: i32 = 42;
        b.iter(|| {
            for _ in 0..1000 {
                flashlog::flash_error_ct!(log_int = i);
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_tell_hot_path,
    bench_flashlog_hot_path,
    bench_comparison_burst,
);
criterion_main!(benches);
