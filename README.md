# Tell Rust SDK

<p align="center">
  <img src="https://img.shields.io/badge/status-alpha-orange" alt="Alpha">
  <a href="https://crates.io/crates/tell"><img src="https://img.shields.io/crates/v/tell.svg" alt="crates.io"></a>
  <a href="https://docs.rs/tell"><img src="https://img.shields.io/docsrs/tell" alt="docs.rs"></a>
  <a href="https://doc.rust-lang.org/edition-guide/rust-2024/"><img src="https://img.shields.io/badge/Rust-2024_edition-blue.svg" alt="Rust 2024"></a>
  <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT"></a>
</p>

Rust SDK for Tell — product analytics, structured logging, and metrics, [1,000× faster](#performance) than PostHog and Mixpanel.

- **80 ns per call.** Serializes, encodes, and enqueues. Your thread moves on.
- **10M events/sec delivered.** Batched, encoded, sent over the wire.
- **Fire & forget.** Synchronous API, async background worker. Zero I/O blocking.
- **Thread-safe.** `Clone + Send + Sync`. Share across threads via `Arc`.
- **Runtime-optional.** Uses your Tokio runtime if there is one, otherwise spawns its own worker thread.

## Installation

```bash
cargo add tell
cargo add tokio --features rt-multi-thread,macros   # optional: without it, Tell runs its own worker thread
cargo add tell-tracing                               # optional: tracing-subscriber Layer
```

## Quick Start

```rust
use tell::{Tell, TellConfig, props};

#[tokio::main]
async fn main() {
    let client = Tell::new(
        TellConfig::production("feed1e11feed1e11feed1e11feed1e11").unwrap()
    ).unwrap();

    // Track events
    client.track("user_123", "Page Viewed", props! {
        "url" => "/home",
        "referrer" => "google"
    });

    // Identify users
    client.identify("user_123", props! {
        "name" => "Jane",
        "plan" => "pro"
    });

    // Revenue
    client.revenue("user_123", 49.99, "USD", "order_456", ());

    // Structured logging
    client.log_error("DB connection failed", None, props! {
        "component" => "api",
        "host" => "db.internal"
    });

    client.close().await.ok();
}
```

## Performance

**Delivery throughput** — batched, encoded, and sent over TCP (Apple M4 Pro):

| Batch size | With payload | No payload |
|------------|--------------|------------|
| 10 | 7.8M/s | 14.5M/s |
| 100 | 8.3M/s | 14.3M/s |
| 500 | **9.8M/s** | **18.2M/s** |

Each event is 200 bytes on the wire — device ID, session ID, timestamp, event name, and user properties, FlatBuffer-encoded with API key and batch headers.

**Caller latency** — serialize, encode, and enqueue. Wire-ready before your function returns:

| Operation | With properties | No properties |
|-----------|-----------------|---------------|
| `track` | 84 ns | 52 ns |
| `log` | 76 ns | 50 ns |

PostHog and Mixpanel send an HTTP request on every track call — ~85 µs on localhost, milliseconds in production. Tell enqueues a wire-ready event in 84 ns — **1,000× less overhead**. Your thread never touches the network.

For comparison, [FlashLog](https://github.com/JunbeomL22/flashlog) achieves ~16 ns by copying raw bytes into a ring buffer — serialization and I/O happen later. Tell pays upfront for a wire-ready event.

```bash
cargo bench -p tell-bench --bench hot_path             # caller latency
cargo bench -p tell-bench --bench comparison            # vs flashlog
cargo run -p tell-bench --example throughput --release   # delivery throughput
```

## Configuration

```rust
use tell::TellConfig;

// Production — collect.tell.rs:50000, batch=100, flush=10s
let config = TellConfig::production("feed1e11feed1e11feed1e11feed1e11").unwrap();

// Development — localhost:50000, batch=10, flush=2s
let config = TellConfig::development("feed1e11feed1e11feed1e11feed1e11").unwrap();

// Custom — see crates/tell/examples/config.rs for all builder options
let config = TellConfig::builder("feed1e11feed1e11feed1e11feed1e11")
    .service("my-backend")                // stamped on every event and log
    .endpoint("collect.internal:50000")
    .queue_capacity(50_000)               // default 10,000 messages in flight
    .buffer_path("/var/lib/my-backend/tell") // WAL for failed sends (off by default)
    .on_error(|e| eprintln!("[Tell] {e}"))
    .build()
    .unwrap();
```

Defaults: batch 100, flush every 10 s, 5 s connect and write timeout, 5 s close timeout, 3 retries. With `buffer_path` set, a failed send goes straight to the write-ahead log and is retried from disk on the next flush, so retries never stall ingestion.

## API

`Tell` is `Clone + Send + Sync`. Cloning is cheap (internally `Arc`).

```rust
let client = Tell::new(config)?;

// Events — user_id is always the first parameter
client.track(user_id, event_name, properties);
client.try_track(user_id, event_name, properties);   // false when the queue is full
client.track_static(user_id, Events::PAGE_VIEWED, properties); // no name allocation
client.identify(user_id, traits);
client.group(user_id, group_id, properties);
client.revenue(user_id, amount, currency, order_id, properties);
client.alias(previous_id, user_id);

// Super properties — merged into every track/group/revenue call
client.register(props!{"app_version" => "2.0"});
client.unregister("app_version");

// Logging
// component: legacy override of the log source (host/instance); pass None
// and put a module or target in data instead
client.log(level, message, component, data);
client.log_info(message, component, data);
client.log_error(message, component, data);
// + log_emergency, log_alert, log_critical, log_warning,
//   log_notice, log_debug, log_trace

// Lifecycle
client.reset_session();
client.dropped();                 // messages dropped because the queue was full
client.flush().await?;
client.close().await?;
client.close_blocking()?;         // outside a Tokio runtime
```

When the queue fills, `on_error` receives one `TellError::QueueFull` per episode and `dropped()` keeps counting. Raise `queue_capacity` or shorten `flush_interval` if it grows.

## Metrics

Two ways to send a metric. Use the direct call for a value you already hold; use a registered instrument for anything counted per request.

```rust
// Direct — one message on the queue per call. Right for a reading you took once.
client.gauge("system.cpu.user", 45.2, &[("core", "0")]);
client.counter("bytes_sent", 1024.0, &[("iface", "eth0")]);   // delta
client.histogram("request_ms", HistogramParams { .. }, &[]);
```

A `counter()` call per upload puts one message per upload on the queue shared with events and logs. Register the counter instead: the call site bumps an atomic, and the background worker ships one point per series every `metrics_interval` (default 15 s). A thousand increments in a tick cost one message, and sampled points are appended to the worker's own batch, so the queue can never drop them.

```rust
let m = client.metrics();

// Counter split by a closed label set. Delta by default: sum the points for a total.
let uploads = m.counter("uploads_total").by("source", &["web", "api"]).register();
uploads.add("web", 1);

// Cumulative counter: the running total since start. Read the last point.
let bytes = m.counter("bytes_total").cumulative().register();
bytes.inc(4096);

// Histogram: count, sum, min, max and cumulative buckets over fixed bounds.
let latency = m.histogram("analysis_ms", &[100.0, 500.0, 2000.0]).by("format", &["elf", "pe"]).register();
latency.observe("elf", 340.0);

// Gauge: a reader the worker calls on every tick. Keep it quick.
let pool = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(8));
let p = pool.clone();
m.gauge("workers_live", move || p.load(std::sync::atomic::Ordering::Relaxed) as f64);
m.gauge_by("jobs_queued", "status", |out| {
    out.push(("pending", 3.0));
    out.push(("running", 1.0));
});
```

Handles are `Arc`s, so clone them into your call sites. Label values are fixed at registration; a value outside the set is dropped, so cardinality cannot grow at runtime and the hot path never allocates.

A counter ships either deltas or running totals, never both. A `sum` over a series that mixes the two counts every cumulative point again.

Once anything is registered, every sample also carries `tell.sdk.dropped`, the same number `dropped()` returns, so message loss shows up in Tell while it is happening instead of at shutdown.

Set the cadence with `.metrics_interval(Duration::from_secs(15))` on the config builder.

Properties accept `props!`, `Props::new()`, `Option<impl Serialize>`, or `()`:

```rust
use tell::{props, Props};

// props! macro — fastest path
client.track("user_123", "Click", props! {
    "url" => "/home",
    "count" => 42,
    "active" => true
});

// Props builder — for dynamic values
let p = Props::new()
    .add("url", &request.path)
    .add("status", response.status);
client.track("user_123", "Request", p);

// serde_json — works with any Serialize type
client.track("user_123", "Click", Some(json!({"url": "/home"})));

// No properties
client.track("user_123", "Click", ());
```

Keys are always emitted as valid JSON. Literal keys in `props!` are checked at compile time and copied without a scan; dynamic keys are scanned once and escaped only if they contain a quote, backslash, or control byte.

## tracing integration

Already on `tracing`? The `tell-tracing` crate ships a `Layer` that forwards events into Tell logs:

```rust
use tracing_subscriber::prelude::*;

tracing_subscriber::registry()
    .with(tell_tracing::TellLayer::new(client.clone()))
    .init();

tracing::info!(user = "u1", "signed in");
```

The direct `Tell` API is faster and stays the recommended path for hot loops; the layer is for adopting Tell in existing code without rewriting call sites.

## Requirements

- **Rust**: 2024 edition
- **Runtime**: Tokio 1.x if you have one. Without a runtime, `Tell::new` starts a dedicated worker thread; use `flush_blocking` and `close_blocking`.

## License

MIT
