#!/usr/bin/env bash
# HLE-3 reproducible measurement: count-table output head with controls on
# the repo-docs proxy corpus (not S5h), and learned-key routing through
# HybridLogicEngine with/without the exact routing index. Run on the target
# host (e.g. Dell/Thor via a RemoteOps workflow pinning an exact SHA).
set -euo pipefail
echo "HLE3_ENV,sha=$(git rev-parse HEAD),arch=$(uname -m),cpus=$(nproc),rust=$(rustc --version)"
cargo test --locked --release -p sml-core --lib hle3_head_table -- --ignored --nocapture --test-threads=1
cargo test --locked --release -p sml-core --lib hle3_route_bench -- --ignored --nocapture --test-threads=1
