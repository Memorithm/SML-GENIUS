#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
if [[ "$(uname -m)" != aarch64 ]]; then
  echo 'A4_COW_MEASURE_REJECTED,requires=aarch64' >&2
  exit 2
fi
if [[ ! -r /proc/self/status ]]; then
  echo 'A4_COW_MEASURE_REJECTED,requires=linux_procfs' >&2
  exit 2
fi
cargo +1.89.0 build --locked --release -p sml-core --example pvp6_closed_loop_qualification
binary="${CARGO_TARGET_DIR:-target}/release/examples/pvp6_closed_loop_qualification"
[[ -x "$binary" ]]
source_sha="$(git rev-parse HEAD)"
for pass in 1 2 3 4 5; do
  out="$(mktemp)"; trap 'rm -f "$out"' EXIT
  "$binary" > "$out" &
  pid=$!
  peak=0
  while kill -0 "$pid" 2>/dev/null; do
    if [[ -r "/proc/$pid/status" ]]; then
      rss="$(awk '/^VmRSS:/ {print $2}' "/proc/$pid/status" 2>/dev/null || true)"
      if [[ "$rss" =~ ^[0-9]+$ && "$rss" -gt "$peak" ]]; then peak="$rss"; fi
    fi
    sleep 0.01
  done
  wait "$pid"
  sha="$(sha256sum "$out" | cut -d' ' -f1)"
  [[ "$sha" == 7ac9f94f1f98680e6d3789f5840433917d5bbeba0cd6dd8a8e0b61c9f5bcf0dc ]]
  printf 'A4_COW_SAMPLE,source=%s,pass=%s,observed_rss_kib=%s,output_sha256=%s\n' "$source_sha" "$pass" "$peak" "$sha"
  rm -f "$out"; trap - EXIT
done
printf 'A4_COW_LIMIT,observed_rss_is_sampled_not_peak,no_baseline,no_speedup_claim\n'
