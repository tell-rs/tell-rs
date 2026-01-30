use std::time::Duration;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use serde_json::json;
use tell::{props, Props, Tell, TellConfig};
use tokio::runtime::Runtime;

fn make_client(rt: &Runtime) -> Tell {
    // Non-routable endpoint — worker spawns but never connects.
    // Unbounded channel absorbs all sends without backpressure.
    rt.block_on(async {
        let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
            .endpoint("192.0.2.1:50000")
            .batch_size(100_000) // never auto-flush
            .flush_interval(Duration::from_secs(3600)) // never timer-flush
            .build()
            .unwrap();

        Tell::new(config).unwrap()
    })
}

fn bench_track_no_props(c: &mut Criterion) {
    let mut group = c.benchmark_group("track");
    group.throughput(Throughput::Elements(1));

    let rt = Runtime::new().unwrap();
    let client = make_client(&rt);

    group.bench_function("no_props", |b| {
        b.iter(|| {
            client.track("user_bench_123", "Page Viewed", None::<serde_json::Value>);
        });
    });

    group.finish();
}

fn bench_track_small_props(c: &mut Criterion) {
    let mut group = c.benchmark_group("track");
    group.throughput(Throughput::Elements(1));

    let rt = Runtime::new().unwrap();
    let client = make_client(&rt);

    group.bench_function("small_props", |b| {
        b.iter(|| {
            client.track(
                "user_bench_123",
                "Page Viewed",
                Some(json!({"url": "/home", "referrer": "google"})),
            );
        });
    });

    group.finish();
}

fn bench_track_small_props_builder(c: &mut Criterion) {
    let mut group = c.benchmark_group("track");
    group.throughput(Throughput::Elements(1));

    let rt = Runtime::new().unwrap();
    let client = make_client(&rt);

    group.bench_function("small_props_builder", |b| {
        b.iter(|| {
            client.track(
                "user_bench_123",
                "Page Viewed",
                props! {"url" => "/home", "referrer" => "google"},
            );
        });
    });

    group.finish();
}

fn bench_track_large_props_builder(c: &mut Criterion) {
    let mut group = c.benchmark_group("track");
    group.throughput(Throughput::Elements(1));

    let rt = Runtime::new().unwrap();
    let client = make_client(&rt);

    group.bench_function("large_props_builder", |b| {
        b.iter(|| {
            client.track(
                "user_bench_123",
                "Page Viewed",
                Props::new()
                    .add("url", "/dashboard/analytics/overview")
                    .add("referrer", "https://www.google.com/search?q=analytics+platform")
                    .add("user_agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
                    .add("screen_width", 1920)
                    .add("screen_height", 1080)
                    .add("viewport_width", 1440)
                    .add("viewport_height", 900)
                    .add("color_depth", 24)
                    .add("language", "en-US")
                    .add("timezone", "America/New_York")
                    .add("session_count", 42)
                    .add("page_load_time_ms", 1234)
                    .add("dom_ready_ms", 890)
                    .add("first_paint_ms", 456),
            );
        });
    });

    group.finish();
}

fn bench_track_large_props(c: &mut Criterion) {
    let mut group = c.benchmark_group("track");
    group.throughput(Throughput::Elements(1));

    let rt = Runtime::new().unwrap();
    let client = make_client(&rt);
    let large_payload = json!({
        "url": "/dashboard/analytics/overview",
        "referrer": "https://www.google.com/search?q=analytics+platform",
        "user_agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
        "screen_width": 1920,
        "screen_height": 1080,
        "viewport_width": 1440,
        "viewport_height": 900,
        "color_depth": 24,
        "language": "en-US",
        "timezone": "America/New_York",
        "session_count": 42,
        "page_load_time_ms": 1234,
        "dom_ready_ms": 890,
        "first_paint_ms": 456,
    });

    group.bench_function("large_props", |b| {
        b.iter(|| {
            client.track("user_bench_123", "Page Viewed", Some(&large_payload));
        });
    });

    group.finish();
}

fn bench_track_with_super_props(c: &mut Criterion) {
    let mut group = c.benchmark_group("track");
    group.throughput(Throughput::Elements(1));

    let rt = Runtime::new().unwrap();
    let client = make_client(&rt);
    client.register(json!({
        "app_version": "3.1.0",
        "env": "production",
        "platform": "web",
        "sdk_version": "0.1.0",
        "deployment_id": "deploy_abc123"
    }));

    group.bench_function("with_super_props", |b| {
        b.iter(|| {
            client.track(
                "user_bench_123",
                "Page Viewed",
                Some(json!({"url": "/home", "referrer": "google", "page_type": "landing"})),
            );
        });
    });

    group.finish();
}

fn bench_track_burst(c: &mut Criterion) {
    let mut group = c.benchmark_group("track_burst");

    let rt = Runtime::new().unwrap();
    let client = make_client(&rt);

    for &count in &[100u64, 1000, 10000] {
        group.throughput(Throughput::Elements(count));
        group.bench_with_input(
            BenchmarkId::new("events", count),
            &count,
            |b, &count| {
                b.iter(|| {
                    for _ in 0..count {
                        client.track(
                            "user_bench_123",
                            "Page Viewed",
                            Some(json!({"url": "/home"})),
                        );
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_log(c: &mut Criterion) {
    let mut group = c.benchmark_group("log");
    group.throughput(Throughput::Elements(1));

    let rt = Runtime::new().unwrap();
    let client = make_client(&rt);

    group.bench_function("log_error", |b| {
        b.iter(|| {
            client.log_error(
                "Connection refused",
                Some("api"),
                Some(json!({"host": "db.internal", "port": 5432})),
            );
        });
    });

    group.finish();
}

fn bench_identify(c: &mut Criterion) {
    let mut group = c.benchmark_group("identify");
    group.throughput(Throughput::Elements(1));

    let rt = Runtime::new().unwrap();
    let client = make_client(&rt);

    group.bench_function("with_traits", |b| {
        b.iter(|| {
            client.identify(
                "user_bench_123",
                Some(json!({"name": "Jane Doe", "email": "jane@example.com", "plan": "pro"})),
            );
        });
    });

    group.finish();
}

fn bench_revenue(c: &mut Criterion) {
    let mut group = c.benchmark_group("revenue");
    group.throughput(Throughput::Elements(1));

    let rt = Runtime::new().unwrap();
    let client = make_client(&rt);

    group.bench_function("with_props", |b| {
        b.iter(|| {
            client.revenue(
                "user_bench_123",
                49.99,
                "USD",
                "order_789",
                Some(json!({"product": "premium"})),
            );
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_track_no_props,
    bench_track_small_props,
    bench_track_small_props_builder,
    bench_track_large_props,
    bench_track_large_props_builder,
    bench_track_with_super_props,
    bench_track_burst,
    bench_log,
    bench_identify,
    bench_revenue,
);
criterion_main!(benches);
