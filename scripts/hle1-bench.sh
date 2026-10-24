#!/usr/bin/env bash
# HLE-1 reproducible measurement: controls table (wiring/routing frozen) and
# Hamming top-k routing vs DirectRouteTable timing. Must be run on the target
# host (e.g. Dell/Thor via a RemoteOps workflow pinning an exact SHA); numbers
# from other hosts are not Dell/Thor evidence.
set -euo pipefail
echo "HLE1_ENV,sha=$(git rev-parse HEAD),arch=$(uname -m),cpus=$(nproc),rust=$(rustc --version)"
cargo test --locked --release -p sml-core --lib hle1_controls_table -- --ignored --nocapture --test-threads=1
cargo test --locked --release -p sml-core --lib hle1_route_bench -- --ignored --nocapture --test-threads=1
