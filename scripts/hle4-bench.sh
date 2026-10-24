#!/usr/bin/env bash
# HLE-4 reproducible measurement: long-context (16..1024 B) Boolean features
# (cache, match memory, hashed-bigram sketch) and routed circuits mixed with
# order-k backoff counts, with controls, on the repo-docs proxy corpus.
# Env: HLE4_TRAIN (bytes, default 400000), HLE4_SEEDS (default 3).
set -euo pipefail
echo "HLE4_ENV,sha=$(git rev-parse HEAD),arch=$(uname -m),cpus=$(nproc),rust=$(rustc --version)"
cargo test --locked --release -p sml-core --lib hle4_long_context_table -- --ignored --nocapture --test-threads=1
