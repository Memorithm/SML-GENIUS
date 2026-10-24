#!/usr/bin/env bash
# HLE-P1 local smoke: every preregistered arm (seed 1) on the docs/*.md proxy.
# Not S5h. Numbers are proxy measurements only.
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:-target/hle-p1-proxy}"
cargo run --release -q -p sml-core --bin sml-hle-p1-s5h -- proxy docs "$out"
