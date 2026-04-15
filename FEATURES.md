# Features

## Events

- Track — record user actions with arbitrary properties.
- Identify — associate a user with traits (flat payload, no nesting).
- Group — associate a user with a group, with optional properties.
- Revenue — track order completions with amount, currency, and order ID.
- Alias — link two user identities.
- Standard event names — 40+ predefined constants covering user lifecycle, billing, subscriptions, trials, shopping, engagement, and communication.

## Sessions

- Opt-in auto session — enable_session() on the builder generates one process-wide UUID v4 and stamps it on track, revenue, and log calls.
- Per-call override — track_with_session, revenue_with_session, and log_with_session stamp a caller-supplied 16-byte id for upstream-owned sessions.
- Identity exempt — identify, alias, and group never carry a session id, since they describe the actor, not activity.
- Manual rotation — reset_session rotates the process-wide id when enabled, and reports a validation error via on_error when disabled.

## Logging

- Structured logs — RFC 5424 severity levels (emergency through trace) with component tagging and arbitrary data.
- Per-entry service override — forward logs from multiple services through a single collector.
- Backpressure signal — try_log returns false when the channel is full instead of silently dropping.
- Convenience methods — log_info, log_error, log_debug, etc. for every severity level.

## Metrics

- Gauge — point-in-time numeric value with labeled dimensions.
- Counter — cumulative or delta counts with configurable temporality.
- Histogram — distribution with explicit bucket boundaries.
- Zero-allocation labels — static string labels avoid heap allocation entirely.
- Dynamic label variants — gauge_dyn and counter_dyn for runtime-generated label values.
- Source tagging — hostname or instance identifier stamped on every metric.

## Properties

- Props builder — chainable key-value builder that writes JSON directly into a byte buffer, skipping intermediate DOM allocation.
- props! macro — concise syntax for inline property construction.
- Flexible input — accepts Props, json!(), Option<impl Serialize>, or any Serialize type.

## Transport

- TCP with FlatBuffers — binary-encoded batches sent over persistent TCP connections.
- Auto-reconnect — transparent reconnection on connection failure.
- Batching — configurable batch size and flush interval with automatic size-triggered flushes.
- Retry with backoff — exponential backoff on send failure (100ms, 200ms, 400ms, ...).
- Bulk drain — high-throughput path amortises channel overhead across thousands of messages.

## Disk Buffer

- Write-ahead log — failed TCP sends persist to disk and retry on subsequent flushes.
- Crash recovery — unconsumed frames survive restarts and are drained on startup.
- Graceful shutdown — queued data saved to WAL when TCP flush times out.
- Size-bounded — configurable max bytes with FIFO eviction of oldest frames.
- Auto-compaction — reclaims disk space when consumed data exceeds half the file.
- Symlink protection — refuses to open WAL paths that are symlinks.

## Configuration

- Builder pattern — TellConfigBuilder with fluent API for all settings.
- Presets — development (localhost, fast flush) and production (default endpoint, tuned defaults).
- Service name — app-level service stamped on every event and log.
- Error callback — on_error hook for non-fatal errors (validation, transport).
- Tunable timeouts — separate network, close, and flush interval settings.

## Architecture

- Sync API, async worker — calls never block the caller; a background Tokio task handles I/O.
- Clone + Send + Sync — Arc-wrapped interior; cloning is cheap, all clones share one connection.
- Lock-free hot path — super properties use parking_lot RwLock; metrics and logs use bounded channel with no locks.
- Sub-microsecond timestamps — quanta rdtsc clock anchored to system time (~2ns per timestamp vs ~20ns for SystemTime).
- Channel backpressure — 10,000-slot pre-allocated ring buffer; callers get immediate feedback when full.
