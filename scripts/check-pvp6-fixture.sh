#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
cargo test --locked -p sml-core --example pascal_boolean_pb8_gate_bank
for pass in first second; do
  cargo run --locked --release -p sml-core --example pascal_boolean_pb8_gate_bank -- pvp6-fixture > "$out/fixture-$pass.csv"
  cargo run --locked --release -p sml-core --example pascal_boolean_pb8_gate_bank -- pvp6-exact-task > "$out/task-$pass.csv"
done
cmp "$out/fixture-first.csv" "$out/fixture-second.csv"
cmp "$out/task-first.csv" "$out/task-second.csv"
expected_fixture=feb1a144e23d9a29439b7cad6704f56637a7f7ead1a9b9abb39ad9c0ac5325fd
printf '%s  %s\n' "$expected_fixture" "$out/fixture-first.csv" | sha256sum --check
cat > "$out/task-expected.csv" <<'EXPECTED'
PVP6_EXACT_TASK,schema=1,cells=144,arm_comparisons=720,words=3427200,mismatches=0
PROMOTION,BLOCKED,pvp6_fixture_and_exact_task_only
EXPECTED
cmp "$out/task-first.csv" "$out/task-expected.csv"
test "$(wc -l < "$out/fixture-first.csv")" -eq 32770
test "$(wc -c < "$out/fixture-first.csv")" -eq 1404649
cat "$out/task-first.csv"
printf 'PVP6_FIXTURE_SHA256,%s\n' "$expected_fixture"

for pass in first second; do
  cargo run --locked --release -p sml-core --example pascal_boolean_pb8_gate_bank -- pvp6-task-plan > "$out/plan-$pass.csv"
done
cmp "$out/plan-first.csv" "$out/plan-second.csv"
test "$(wc -l < "$out/plan-first.csv")" -eq 22

# Qualify the public model executor on the unchanged frozen corpus. Compare both
# layouts, complete reuse output and padding; this is not a new timing endpoint.
for pass in first second; do
  cargo run --locked --release -p sml-core --example pascal_boolean_pb8_gate_bank -- pvp6-model-executor > "$out/model-$pass.csv"
done
cmp "$out/model-first.csv" "$out/model-second.csv"
cat > "$out/model-expected.csv" <<'EXPECTED'
PVP6_MODEL_EXECUTOR,schema=1,cells=144,query_words=685440,packed_words=171456,coefficient_words=2048,mismatches=0
PVP6_MODEL_SELECTION,pascal=141,sparse=2,submask=1,measured_coordinates=20,diagnostic_only=144
PROMOTION,BLOCKED,model_executor_synthetic_correctness_only
EXPECTED
cmp "$out/model-first.csv" "$out/model-expected.csv"
cat "$out/model-first.csv"

# A4 public SBG/ANF recurrence: complete golden trajectories, native runner,
# literal control, owned checkpoint replay and bounded trace-codec roundtrip.
# No historical fixture or timing endpoint is redefined by this extra gate.
expected_a4=5d7ef8fb5ec5e574e1a32fe0c94b67a6aafc41c7a3a94d00a5c73c80e63b8346
golden_a4=docs/research/evidence/a4-sbg-20261008/trajectory.csv
printf '%s  %s\n' "$expected_a4" "$golden_a4" | sha256sum --check
for pass in first second; do
  cargo run --locked --release -p sml-core --example pvp6_sbg_qualification > "$out/a4-$pass.csv"
done
cmp "$out/a4-first.csv" "$out/a4-second.csv"
cmp "$out/a4-first.csv" "$golden_a4"
test "$(wc -l < "$out/a4-first.csv")" -eq 915
grep '^A4_SBG_SUMMARY,' "$out/a4-first.csv"
printf 'A4_SBG_TRAJECTORY_SHA256,%s\n' "$expected_a4"
rustc --edition=2021 -D warnings scripts/verify-a4-sbg.rs -o "$out/verify-a4-sbg"
rustc --edition=2021 -D warnings --test scripts/verify-a4-sbg.rs -o "$out/verify-a4-sbg-tests"
"$out/verify-a4-sbg-tests"
"$out/verify-a4-sbg" "$out/a4-first.csv"

# Full selected-expert/current-route Delta-KV loop. Old frozen fixtures stay unchanged.
closed_golden=docs/research/evidence/a4-closed-loop-20261009/trajectory.csv
closed_hash=7ac9f94f1f98680e6d3789f5840433917d5bbeba0cd6dd8a8e0b61c9f5bcf0dc
printf '%s  %s\n' "$closed_hash" "$closed_golden" | sha256sum --check
for pass in first second; do
  cargo run --locked --release -p sml-core --example pvp6_closed_loop_qualification > "$out/closed-$pass.csv"
done
cmp "$out/closed-first.csv" "$out/closed-second.csv"
cmp "$out/closed-first.csv" "$closed_golden"
test "$(wc -l < "$out/closed-first.csv")" -eq 675
test "$(wc -c < "$out/closed-first.csv")" -eq 95452
rustc --edition=2021 -D warnings scripts/verify-a4-closed-loop.rs -o "$out/verify-closed"
rustc --edition=2021 -D warnings --test scripts/verify-a4-closed-loop.rs -o "$out/verify-closed-tests"
"$out/verify-closed-tests"
"$out/verify-closed" "$out/closed-first.csv"
grep '^A4_CLOSED_SUMMARY,' "$out/closed-first.csv"
printf 'A4_CLOSED_TRAJECTORY_SHA256,%s\n' "$closed_hash"
