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
