# Changelog

## v0.6.0

New:
- events: try_track returns a backpressure signal like try_log; track_static skips the event-name allocation for literals
- client: dropped() counter and a one-shot TellError::QueueFull report per full episode replace silent drops
- client: works without a Tokio runtime — the worker runs on a dedicated thread; flush_blocking and close_blocking for sync programs
- config: queue_capacity builder option (default 10,000)
- properties: () accepted as "no properties" instead of None::<serde_json::Value>
- properties: keys are escaped — literal keys in props! are checked at compile time, dynamic keys scanned at runtime
- tracing: new tell-tracing crate with a tracing-subscriber Layer that forwards tracing events into Tell logs

Fix:
- clock: timestamps re-anchor to SystemTime every second, so suspend, sleep, and NTP steps no longer skew events forever
- worker: a close queued behind a flush now shuts the worker down instead of being acknowledged early; flush acks only after data is sent
- client: flush and close no longer block a runtime thread; a full queue on a current-thread runtime cannot deadlock
- worker: shutdown honours close_timeout instead of a hardcoded 5s
- transport: network_timeout now bounds frame writes as well as connects (default lowered from 30s to 5s)
- worker: with a disk buffer configured, a failed send goes straight to the WAL and retries from there, so retries never stall ingestion
- worker: frames are capped at batch_size entries; queues keep their capacity between flushes
- buffer: corrupt WAL headers are rejected before allocating; appends and cursor writes are fsynced; cursor committed once per drain pass
- buffer: no /dev/null placeholder, so the WAL works on Windows
- client: super properties are pre-serialized once and spliced in, removing a JSON re-parse per call
- encoding: protocol version comes from DEFAULT_VERSION instead of a bare literal
- docs: default endpoint docstring said collect.tell.app; it is collect.tell.rs

Breaking:
- TellError is #[non_exhaustive]; matches need a wildcard arm
- network_timeout default is 5s (was 30s)
- max_retries is ignored when buffer_path is set

## v0.5.0

  - session: opt-in tracking via enable_session
  - session: track_with_session, revenue_with_session, log_with_session when session id is needed
  - session: reset_session errors when tracking is disabled

## v0.4.1

New:
- logging: per-entry service override for forwarding logs from multiple services through one collector

Fix:
- logging: source field falls back to config hostname when component is None
- build: CI workflow, clippy, deny, and formatting configs

## v0.4.0

New:
- metrics: gauge, counter, histogram types with zero-alloc label path
- metrics: _dyn variants for runtime-generated label values
- config: source builder method for hostname/instance tagging on metrics
- config: buffer_path and buffer_max_bytes for opt-in disk WAL
- buffer: disk WAL persists unsent batches across restarts and shutdown timeouts
- logging: try_log returns backpressure signal instead of silently dropping

Fix:
- worker: bulk message drain reduces overhead under high throughput
- worker: inline retry with backoff replaces fire-and-forget spawned retries
- worker: graceful shutdown saves queued data to WAL when TCP flush times out
- client: flush and close handle full channel via send_timeout instead of failing immediately
- client: parking_lot RwLock replaces std RwLock, removing lock poisoning panics

## v0.3.0

- client: identify flattens traits into top-level payload instead of nesting under traits key
- encoding: service field added to EventParams in benchmarks
- docs: sanitize placeholder API keys across README, examples, and tests

## v0.2.0

Breaking:
- client: rename log service param to component, separating app identity from module context

Fix:
- worker: config-level service now stamped on both events and logs (was ignored for logs)
- worker: per-log component mapped to wire source field instead of overwriting service

New:
- config: service builder method to set app-level service name
- encoding: service field support in event FlatBuffer (field 2)
