# Features

## Events

- Track — record user actions with arbitrary properties.
- Backpressure signal — try_track returns false when the queue is full; track_static avoids the event-name allocation for literals.
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
- Registered instruments — client.metrics() hands out Counter, Histogram and Gauge handles; call sites bump atomics and the worker samples them every metrics_interval, one point per series per tick.
- Sampled points bypass the queue — appended to the worker's own batch, so a full queue never loses a sample.
- Delta or cumulative per counter — pick at registration; a series never mixes the two.
- Closed label sets — label values fixed at registration; unknown values are dropped, so cardinality cannot grow at runtime.
- tell.sdk.dropped gauge — the dropped-message count ships with every sample once an instrument is registered.

## Properties

- Props builder — chainable key-value builder that writes JSON directly into a byte buffer, skipping intermediate DOM allocation.
- props! macro — concise syntax for inline property construction.
- Flexible input — accepts Props, json!(), Option<impl Serialize>, any Serialize type, or () for no properties.
- Safe keys — literal keys in props! are validated at compile time; dynamic keys are scanned once and escaped only when needed.
- Pre-serialized super properties — register() builds a byte fragment once; every track/group/revenue splices it without re-parsing.

## Transport

- TCP with FlatBuffers — binary-encoded batches sent over persistent TCP connections.
- Auto-reconnect — transparent reconnection on connection failure.
- Batching — configurable batch size and flush interval with automatic size-triggered flushes.
- Retry with backoff — exponential backoff on send failure (100ms, 200ms, 400ms, ...) when no disk buffer is configured.
- Write timeout — network_timeout bounds every frame write as well as the connect, so a stalled peer cannot hang the worker.
- Bounded frames — no frame carries more than batch_size entries; queues keep their capacity between flushes.
- Bulk drain — high-throughput path amortises channel overhead across thousands of messages, bounded so timers never starve.

## Disk Buffer

- Write-ahead log — failed TCP sends persist to disk (fsynced) and retry on subsequent flushes; with a WAL configured, a failed send goes straight to disk instead of stalling on inline retries.
- Corruption guard — frame headers are validated before allocation; a corrupt tail is skipped and reported.
- At-least-once — cursor committed once per drain pass and on drop; a crash mid-pass re-sends a few frames rather than losing any.
- Crash recovery — unconsumed frames survive restarts and are drained on startup.
- Graceful shutdown — queued data saved to WAL when TCP flush times out.
- Size-bounded — configurable max bytes with FIFO eviction of oldest frames.
- Auto-compaction — reclaims disk space when consumed data exceeds half the file.
- Symlink protection — refuses to open WAL paths that are symlinks.
- Portable — no /dev/null placeholder; works on Windows.

## Configuration

- Builder pattern — TellConfigBuilder with fluent API for all settings.
- Presets — development (localhost, fast flush) and production (default endpoint, tuned defaults).
- Service name — app-level service stamped on every event and log.
- Error callback — on_error hook for non-fatal errors (validation, transport, queue full).
- Tunable timeouts — separate network, close, and flush interval settings; close honours close_timeout end to end.
- Queue capacity — queue_capacity sets the in-flight message limit (default 10,000).

## Architecture

- Sync API, async worker — calls never block the caller; a background Tokio task handles I/O.
- Runtime-optional — inside a Tokio runtime the worker is spawned on it; outside one, a dedicated tell-worker thread runs its own current-thread runtime. flush_blocking and close_blocking serve sync programs.
- Non-blocking control — flush and close use an async sender, so a full queue on a current-thread runtime cannot deadlock.
- Clone + Send + Sync — Arc-wrapped interior; cloning is cheap, all clones share one connection.
- Lock-free hot path — super properties use parking_lot RwLock; metrics and logs use bounded channel with no locks.
- Sub-microsecond timestamps — quanta clock anchored to system time (~2ns per timestamp vs ~20ns for SystemTime), re-anchored every second so suspend and NTP steps never skew events.
- Channel backpressure — pre-allocated ring buffer; dropped() counts overflow and on_error receives one QueueFull per full episode.
- Ordered shutdown — a close queued behind a flush stops the worker; flush acks only after data is sent.

## Integrations

- tell-tracing — tracing-subscriber Layer that maps tracing levels to Tell log levels, uses the target as component, and collects fields into Props. Additive; the direct API remains the fast path.
