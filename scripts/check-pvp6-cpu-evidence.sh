#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
evidence=docs/research/evidence/pvp6-cpu-20261007
printf '%s  %s\n' fc0e1f7c41c95d7df36762425f27840a57e927c6588cd2e718b7e13ac02003b7 "$evidence/capture.tar.gz" | sha256sum --check
tar -xzf "$evidence/capture.tar.gz" -C "$out"
(cd "$out" && sha256sum --check files.sha256 > "$out/hash-verification.log")
rustfmt --check scripts/verify-pvp6-timing.rs
rustc --edition=2021 -D warnings --test scripts/verify-pvp6-timing.rs -o "$out/tests"
"$out/tests"
rustc --edition=2021 -D warnings scripts/verify-pvp6-timing.rs -o "$out/verifier"
"$out/verifier" "$out/capture/success" > "$out/audit.csv"
cmp "$out/audit.csv" "$evidence/audit.csv"
head -1 "$out/audit.csv"
