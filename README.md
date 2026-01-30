# Tell Rust SDK

<p align="center">
  <img src="https://img.shields.io/badge/status-alpha-orange" alt="Alpha">
  <a href="https://crates.io/crates/tell"><img src="https://img.shields.io/crates/v/tell.svg" alt="crates.io"></a>
  <a href="https://docs.rs/tell"><img src="https://img.shields.io/docsrs/tell" alt="docs.rs"></a>
  <a href="https://doc.rust-lang.org/edition-guide/rust-2024/"><img src="https://img.shields.io/badge/Rust-2024_edition-blue.svg" alt="Rust 2024"></a>
  <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT"></a>
</p>

Analytics SDK for Rust. Events and structured logging over TCP + FlatBuffers.

- **80 ns per call.** Serializes, encodes to FlatBuffers, and enqueues for TCP delivery. Your thread moves on.
- **10M events/sec delivered.** Full pipeline — batched, encoded, sent over TCP.
- **Fire & forget.** Synchronous API, async background worker. Never blocks on I/O.
- **Thread-safe.** `Clone + Send + Sync`, share across threads cheaply via `Arc`.

## Installation

```bash
cargo add tell
cargo add tokio --features rt-multi-thread,macros
```

## Quick Start

```rust
use tell::{Tell, TellConfig, props};

#[tokio::main]
async fn main() {
    let client = Tell::new(
        TellConfig::production("a1b2c3d4e5f60718293a4b5c6d7e8f90").unwrap()
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
    client.revenue("user_123", 49.99, "USD", "order_456", None::<serde_json::Value>);

    // Structured logging
    client.log_error("DB connection failed", Some("api"), props! {
        "host" => "db.internal"
    });

    client.close().await.ok();
}
```

## Performance

**Delivery throughput** — 10M events enqueued, batched, encoded, and sent over TCP:

| Batch size | ~200B payload | No properties |
|------------|---------------|---------------|
| 10 | 7.8M/s | 14.5M/s |
| 100 | 8.3M/s | 14.3M/s |
| 500 | **9.8M/s** | **18.2M/s** |

Each `track()` call takes **80 ns** on the caller thread — that includes JSON serialization, FlatBuffer encoding, and channel send. The event is wire-ready for TCP delivery before your function returns. [FlashLog](https://github.com/JunbeomL22/flashlog) achieves ~16 ns by deferring all serialization to a background worker and writing to local disk only — a different design point.

```bash
cargo bench -p tell-bench --bench hot_path             # caller latency
cargo bench -p tell-bench --bench comparison            # vs flashlog
cargo run -p tell-bench --example throughput --release   # delivery throughput
```

## Configuration

```rust
use tell::TellConfig;

// Production — collect.tell.rs:50000, batch=100, flush=10s
let config = TellConfig::production("a1b2c3d4e5f60718293a4b5c6d7e8f90").unwrap();

// Development — localhost:50000, batch=10, flush=2s
let config = TellConfig::development("a1b2c3d4e5f60718293a4b5c6d7e8f90").unwrap();

// Custom — see examples/config.rs for all builder options
let config = TellConfig::builder("a1b2c3d4e5f60718293a4b5c6d7e8f90")
    .endpoint("collect.internal:50000")
    .on_error(|e| eprintln!("[Tell] {e}"))
    .build()
    .unwrap();
```

## API

`Tell` is `Clone + Send + Sync`. Cloning is cheap (internally `Arc`).

```rust
let client = Tell::new(config)?;

// Events — user_id is always the first parameter
client.track(user_id, event_name, properties);
client.identify(user_id, traits);
client.group(user_id, group_id, properties);
client.revenue(user_id, amount, currency, order_id, properties);
client.alias(previous_id, user_id);

// Super properties — merged into every track/group/revenue call
client.register(props!{"app_version" => "2.0"});
client.unregister("app_version");

// Logging
client.log(level, message, service, data);
client.log_info(message, service, data);
client.log_error(message, service, data);
// + log_emergency, log_alert, log_critical, log_warning,
//   log_notice, log_debug, log_trace

// Lifecycle
client.reset_session();
client.flush().await?;
client.close().await?;
```

Properties accept `props!`, `Props::new()`, `Option<impl Serialize>`, or `None::<serde_json::Value>`:

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
client.track("user_123", "Click", None::<serde_json::Value>);
```

## Requirements

- **Rust**: 2024 edition
- **Runtime**: Tokio 1.x

## License

MIT
