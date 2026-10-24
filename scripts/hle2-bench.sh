#!/usr/bin/env bash
# HLE-2 reproducible measurement: exact multi-index Hamming routing index vs
# full scan, and the byte-level BPC proxy table (repo docs, not S5h). Run on
# the target host (e.g. Dell/Thor via a RemoteOps workflow pinning an exact
# SHA); numbers from other hosts are not Dell/Thor evidence.
set -euo pipefail
echo "HLE2_ENV,sha=$(git rev-parse HEAD),arch=$(uname -m),cpus=$(nproc),rust=$(rustc --version)"
cargo test --locked --release -p sml-core --lib hle2_index_bench -- --ignored --nocapture --test-threads=1
cargo test --locked --release -p sml-core --lib hle2_byte_lm_table -- --ignored --nocapture --test-threads=1
