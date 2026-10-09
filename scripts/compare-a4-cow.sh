#!/usr/bin/env bash
# Compare already-built native release executables; no build noise inside samples.
set -euo pipefail
if [[ $# -ne 2 ]]; then echo 'usage: compare-a4-cow.sh BASELINE_BINARY CURRENT_BINARY' >&2; exit 2; fi
[[ $(uname -m) == aarch64 ]] || { echo 'requires aarch64' >&2; exit 2; }
for binary in "$1" "$2"; do [[ -x "$binary" ]] || { echo "not executable: $binary" >&2; exit 2; }; done
expected=7ac9f94f1f98680e6d3789f5840433917d5bbeba0cd6dd8a8e0b61c9f5bcf0dc
run_one() {
  local label="$1" binary="$2" pair="$3" output pid start finish elapsed hwm rss value sha
  output=$(mktemp)
  start=$(date +%s%N)
  "$binary" > "$output" & pid=$!
  hwm=0
  while kill -0 "$pid" 2>/dev/null; do
    if [[ -r "/proc/$pid/status" ]]; then
      value=$(awk '/^VmHWM:/ {print $2}' "/proc/$pid/status" 2>/dev/null || true)
      if [[ $value =~ ^[0-9]+$ && $value -gt $hwm ]]; then hwm=$value; fi
    fi
    sleep 0.001
  done
  wait "$pid"
  finish=$(date +%s%N)
  sha=$(sha256sum "$output" | cut -d' ' -f1)
  rm -f "$output"
  [[ $sha == "$expected" ]] || { echo "golden output mismatch for $label pair=$pair" >&2; exit 1; }
  elapsed=$((finish-start))
  printf 'A4_MATCHED_SAMPLE,label=%s,pair=%d,elapsed_ns=%d,sampled_hwm_kib=%d,output_sha256=%s\n' "$label" "$pair" "$elapsed" "$hwm" "$sha"
}
printf 'A4_MATCHED_CONTEXT,arch=%s,kernel=%s,baseline_sha256=%s,current_sha256=%s\n' "$(uname -m)" "$(uname -r)" "$(sha256sum "$1"|cut -d' ' -f1)" "$(sha256sum "$2"|cut -d' ' -f1)"
for pair in $(seq 1 10); do
  if (( pair % 2 )); then run_one baseline "$1" "$pair"; run_one current "$2" "$pair";
  else run_one current "$2" "$pair"; run_one baseline "$1" "$pair"; fi
done
printf 'A4_MATCHED_LIMIT,wall_clock_includes_launch_and_sampling,VmHWM_is_polled,not_GPU_or_allocator_profiling,no_automatic_speedup_claim\n'
