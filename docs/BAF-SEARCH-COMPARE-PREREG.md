# BAF-SEARCH-COMPARE — pre-registration

Method comparison of TRAIN-side search strategies for the BAF joint-focus
fit on the existing `baf_hardened_compare` harness. This is **not** a
learnability claim at any width; it compares search procedures under one
protocol.

## Strategies

| `--search` | Function | Description |
|---|---|---|
| `kflip` (default) | `fit_joint_focus` | Greedy single flips over bias + basis, then same-page k-flip escapes (k = 2, 3, 4, per-k caps). Unchanged; default CSV is byte-stable apart from wall-clock µs. |
| `beam` | `fit_joint_focus_beam` | Deterministic beam (`--beam-width`, default 4) over single flips; dedups evaluated states; keeps best-ever state. |
| `gf2` | `fit_joint_focus_gf2` | Probes each editable page forced on/off to read desired activations, then solves the GF(2) system over bias + degree ≤ `max_degree` feature bank by Gaussian elimination. Inconsistent system → baseline (not admitted); solution with > `max_terms` terms → `CandidateBudget`. Oracle for degree-1/parity. Assumes editable pages do not interact. |

## Fairness

- Same `--max-evals` counter (`max_candidate_evaluations`) bounds all three; each preflights its own worst case and fails closed (`CandidateBudget`, status 1) instead of truncating.
- Same seeds, widths, families, train/validation/test/OOD splits.
- Validation is sealed: consulted once after search; admission requires train < baseline and validation ≤ baseline validation. TEST/OOD are not used for selection.
- Negatives and budget failures are kept in the output.

## Hypotheses (fixed before the run)

- H1: `gf2` is exact (train = val = 0) on parity families (xor2, parity3, parity4) at every degree, since the target lies in the degree-1 span.
- H2: `beam` matches `kflip` on structured families within budget, or fails honestly (non-zero train error / not admitted / budget status), never silently.
- H3: `random` stays null: no strategy reaches zero train error with generalising validation.

## Metrics

Per family × degree × strategy: mean train error, mean validation error, admitted count (of 8 seeds), mean evaluations, mean wall µs, status counts.

## Seed policy

Seeds 0..8 (`--seeds 8`), same for every strategy; no seed selection or reruns.

## Run

```
B=target/release/examples/baf_hardened_compare
for s in kflip beam gf2; do
  $B --width 7 --seeds 8 --max-degree 3 --parity4 --max-quad-flips 70 \
     --max-evals 5000 --search $s --json /tmp/c_$s.json > /tmp/c_$s.csv
done
```

Results are reported in the PR body (local, width 7 only).
