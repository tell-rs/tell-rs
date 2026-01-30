#!/usr/bin/env bash
# Clone comparison libraries for benchmarking.
# Run from the repo root: ./scripts/setup-compare.sh

set -e

DIR="compare"
mkdir -p "$DIR"

clone() {
    local name=$1 url=$2
    if [ -d "$DIR/$name" ]; then
        echo "  skip  $name (exists)"
    else
        echo "  clone $name"
        git clone --depth 1 -q "$url" "$DIR/$name"
    fi
}

clone flashlog       https://github.com/JunbeomL22/flashlog.git
clone fastrace       https://github.com/fast/fastrace.git
clone tracing        https://github.com/tokio-rs/tracing.git
clone lightning-log  https://github.com/simplysabir/lightning-log
clone rust-loguru    https://github.com/j-raghavan/rust-loguru.git

echo "  done — run: cargo bench -p tell-bench --bench comparison"
