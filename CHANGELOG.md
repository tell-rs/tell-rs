# Changelog

## v0.2.0

Breaking:
- client: rename log service param to component, separating app identity from module context

Fix:
- worker: config-level service now stamped on both events and logs (was ignored for logs)
- worker: per-log component mapped to wire source field instead of overwriting service

New:
- config: service builder method to set app-level service name
- encoding: service field support in event FlatBuffer (field 2)
