# Changelog

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
