#!/usr/bin/env bash
# HLE-5 reproducible measurement: learned bounded slot memory (hindsight write
# gate), searched sparse long-range bit contexts and conditional routing,
# against order-k counts, HLE-4 match memory and equal-capacity controls.
# Env: HLE5_TRAIN (1000000), HLE5_SEEDS (3), HLE5_CAP (1024), HLE5_WINDOW
# (1024), HLE5_HEADS (4), HLE5_WIDTH (12), HLE5_ITERS (24), HLE5_CORPUS_DIR.
set -euo pipefail
echo "HLE5_ENV,sha=$(git rev-parse HEAD),arch=$(uname -m),cpus=$(nproc),rust=$(rustc --version)"
cargo test --locked --release -p sml-core --lib hle5_table -- --ignored --nocapture --test-threads=1
