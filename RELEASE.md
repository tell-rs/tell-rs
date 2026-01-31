# Release

```bash
# 1. Verify (must pass, no warnings)
cargo test -p tell -p tell-encoding
cargo clippy -p tell -p tell-encoding -- -D warnings

# 2. E2E smoke test (start your Tell server first)
cargo run -p tell --example e2e
# Override endpoint: TELL_ENDPOINT=collect.tell.app:50000 cargo run -p tell --example e2e

# 3. Bump version in workspace Cargo.toml
# edit: [workspace.package] version = "X.Y.Z"

# 4. Publish (tell-encoding first, only if it changed)
cargo publish -p tell-encoding
cargo publish -p tell

# 5. Commit, tag, push
git add -A && git commit -m "vX.Y.Z"
git tag vX.Y.Z
git push && git push --tags
```
