# Testing

## Unit + integration tests
```bash
cargo test --all
```

## Coverage
```bash
cargo llvm-cov --all --ignore-filename-regex '_test\.rs$|tell-bench/|compare/'
```

## E2E (requires running server + valid apikey)
```bash
cargo run -p tell --example e2e
```
