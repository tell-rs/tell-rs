# tell-tracing

A `tracing_subscriber::Layer` that forwards `tracing` events to the [Tell](https://tell.rs) SDK as structured logs.

```rust
use tracing_subscriber::prelude::*;

let client = tell::Tell::new(tell::TellConfig::production("your-32-hex-api-key")?)?;
tracing_subscriber::registry()
    .with(tell_tracing::TellLayer::new(client.clone()))
    .init();

tracing::info!(user = "u1", "signed in");
```

Levels map ERROR, WARN, INFO, DEBUG, TRACE to the matching Tell log levels. Fields become properties. By default the event target becomes the log component, which replaces the log's `source` (the host or instance in your config); call `.with_component_from_target(false)` to keep `source` as the host, and record a module you need as an event field. Spans are ignored.

The direct `Tell` API is faster and remains the recommended path for hot loops; use the layer to adopt Tell in existing `tracing`-based code without rewriting call sites.
