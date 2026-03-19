use std::time::{Duration, Instant};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use serde_json::json;
use tell::{Tell, TellConfig};
use tell_bench::SCENARIOS;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;

/// Spawn a null TCP server that accepts one connection and discards all data.
/// Returns the server address.
async fn spawn_null_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let handle = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {} // discard
                    }
                }
            });
        }
    });

    (addr, handle)
}

fn bench_pipeline_flush(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_flush");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));

    let rt = Runtime::new().unwrap();

    let (addr, server_handle) = rt.block_on(spawn_null_server());

    for scenario in SCENARIOS {
        group.throughput(Throughput::Elements(scenario.events_per_batch as u64));
        group.bench_with_input(
            BenchmarkId::new("events", scenario.name),
            scenario,
            |b, scenario| {
                b.to_async(&rt).iter_custom(|iters| {
                    let addr = addr.clone();
                    async move {
                        let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
                            .endpoint(&addr)
                            .batch_size(scenario.events_per_batch)
                            .flush_interval(Duration::from_secs(3600))
                            .build()
                            .unwrap();

                        let client = Tell::new(config).unwrap();

                        // Warm up connection
                        client.track("warmup", "Warmup", None::<serde_json::Value>);
                        client.flush().await.unwrap();

                        let payload =
                            json!({"data": "x".repeat(scenario.payload_size.saturating_sub(30))});

                        let start = Instant::now();
                        for _ in 0..iters {
                            for _ in 0..scenario.events_per_batch {
                                client.track("user_bench_123", "Page Viewed", Some(&payload));
                            }
                            client.flush().await.unwrap();
                        }
                        let elapsed = start.elapsed();

                        client.close().await.ok();
                        elapsed
                    }
                });
            },
        );
    }

    rt.block_on(async { server_handle.abort() });
    group.finish();
}

fn bench_pipeline_log_flush(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline_log_flush");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));

    let rt = Runtime::new().unwrap();

    let (addr, server_handle) = rt.block_on(spawn_null_server());

    for scenario in &SCENARIOS[..2] {
        // realtime_small and typical only
        group.throughput(Throughput::Elements(scenario.events_per_batch as u64));
        group.bench_with_input(
            BenchmarkId::new("logs", scenario.name),
            scenario,
            |b, scenario| {
                b.to_async(&rt).iter_custom(|iters| {
                    let addr = addr.clone();
                    async move {
                        let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
                            .endpoint(&addr)
                            .batch_size(scenario.events_per_batch)
                            .flush_interval(Duration::from_secs(3600))
                            .build()
                            .unwrap();

                        let client = Tell::new(config).unwrap();

                        // Warm up
                        client.log_info("warmup", Some("bench"), None::<serde_json::Value>);
                        client.flush().await.unwrap();

                        let data = json!({"context": "x".repeat(scenario.payload_size.saturating_sub(30))});

                        let start = Instant::now();
                        for _ in 0..iters {
                            for _ in 0..scenario.events_per_batch {
                                client.log_error("Connection failed", Some("api"), Some(&data));
                            }
                            client.flush().await.unwrap();
                        }
                        let elapsed = start.elapsed();

                        client.close().await.ok();
                        elapsed
                    }
                });
            },
        );
    }

    rt.block_on(async { server_handle.abort() });
    group.finish();
}

criterion_group!(benches, bench_pipeline_flush, bench_pipeline_log_flush);
criterion_main!(benches);
