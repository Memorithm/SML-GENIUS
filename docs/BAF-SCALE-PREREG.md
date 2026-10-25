# BAF-SCALE: pre-registration (harness only)

Status: **pre-registered, harness landed, no Dell/Thor results yet.**
Line: Boolean Associative Focus (BAF-0..BAF-3b, XOR2 pair escape, PARITY3
triple escape). Research-only; no speed, quality or energy claims. Fit
wall-time is recorded for scaling bookkeeping only.

## Harness (this PR)

`crates/sml-core/examples/baf_hardened_compare.rs` keeps BAF-3b defaults
bit-identical (width 7, seeds 0..8, degrees 1..2, five families, default
`JointFocusPolicy`). New opt-in flags:

| Flag | Default | Meaning |
|---|---|---|
| `--width N` | 7 | input bits, event = N-4, state = 4; 7..=16 |
| `--seeds N` | 8 | seeds `0..N` (64 for RemoteOps) |
| `--max-degree D` | 2 | `3` adds the `anf_degree3` arm |
| `--parity4` | off | adds the `parity4` family |
| `--max-pair-flips N` | 256 | per-round pair escape ceiling |
| `--max-triple-flips N` | 128 | per-round triple escape ceiling |
| `--max-quad-flips N` | 0 | per-round quadruple escape ceiling (parity4 fits only) |
| `--max-evals N` | 5000 | `max_candidate_evaluations` per fit |
| `--max-basis N` | 136 | `max_basis_terms` per fit |
| `--max-samples N` | 8192 | train+validation sample capacity per fit (width 16 needs ≥ 40960) |
| `--json PATH` | none | JSON summary (means, admits, per-fit stats) |

`fit_joint_focus` now accepts ANF degree 3 and a `max_quad_flips` budget
(default **0 = disabled**). C(n,4) per page grows quickly (n=7: 35, n=10:
210, n=12: 495, n=16: 1820), so the quadruple escape stays opt-in. Every
k-flip count (pairs, triples, quads) and the total worst case are checked
before search. Anything over budget **fails closed** with `CandidateBudget`.
The harness then reports the inactive atlas, `admitted=false`, `status=1`.
At width >= 10 the default budgets intentionally fail closed. A scale run
has to pass explicit budgets.

Width-generalized splits: OOD = event bit 2 AND state bit 2 (1/4 of inputs).
The rest is seed-shuffled into TRAIN 7/12, VALIDATION 1/4 and TEST the rest,
which matches BAF-3b exactly at width 7. Gate inputs are always event bits 0..3
and state bits 0..2. Extra event bits at width > 7 are distractors.

## Hypotheses

- H1 (and3@deg3): `anf_degree3` recovers and3 exactly (0 error bits on
  train/val/test/ood) on >= 90% of seeds at every width. Degrees 1..2 stay
  partial (preserved negative).
- H2 (parity4 + quad escape): `anf_degree1` with the quadruple escape is exact
  on >= 90% of seeds. With `max_quad_flips = 0` some seed stays partial
  (preserved negative).
- H3 (structured families scale): conj2@deg2, xor2@deg1, parity3@deg1 stay
  exact at widths 10/12/16 under explicit budgets.
- H4 (nulls): `random` tables stay null. No arm is materially better than
  `inactive`/`hamming_key_router` on sealed TEST/OOD. A "win" on random is a
  harness bug.
- H5 (scaling): candidate evaluations per fit follow the analytic bound
  (basis+1)*pages*rounds + sum_k C(n,k)*pages*rounds. Wall-time is recorded,
  not claimed.

## Metrics

Per (family, seed, arm, split): Boolean next-state error bits and admission.
Per fit: candidate evaluations, wall-time micros, status (0 ok, 1 budget
fail-closed, 2 error). Primary: exact-recovery rate over seeds on TEST and
OOD. Secondary: mean[min..max] error bits.

## Seed policy and protocol

- CI: seeds 0..8. RemoteOps: seeds 0..64. Seeds are contiguous from 0 and
  never cherry-picked. All seeds are reported, including failures.
- Budgets are fixed per width before the run. They are the same for every
  arm and family (the quad budget applies to parity4 only, as declared).
- Search uses only TRAIN, and VALIDATION is used once for the admission veto.
  **TEST and OOD stay sealed** until train+val are complete for the whole
  sweep. They are read only for the final report.
- Negatives (and3 at deg <= 2, parity4 without quads, random) are preserved
  in the results doc.

## Local smoke (box CPU, 8 seeds, not a claim)

- Width 7, `--max-degree 3 --parity4 --max-quad-flips 128`: and3@deg3 exact on
  8/8. parity4@deg1 exact on 8/8 with quads. Without quads, parity4@deg1
  averages 10.88 train error bits (partial).
- Width 10, `--max-degree 3 --parity4 --max-triple-flips 256
  --max-quad-flips 512 --max-evals 20000 --max-basis 200`: conj2@deg2,
  xor2@deg1, parity3@deg1, and3@deg3 and parity4@deg1 are exact on 8/8.
  random stays null. Whole run was about 3.5 s. Width 10 with default budgets
  fails closed everywhere (expected).
- Width 12 with the w12 budgets below: same pattern. All structured targets
  are exact on 8/8 and random stays null. 0 budget failures, about 22.6 s.

## Planned RemoteOps commands (follow-up, not dispatched here)

```
cargo run --locked -p sml-core --release --example baf_hardened_compare -- \
  --width 10 --seeds 64 --max-degree 3 --parity4 \
  --max-triple-flips 256 --max-quad-flips 512 --max-evals 20000 \
  --max-basis 200 --json baf-scale-w10.json > baf-scale-w10.csv
cargo run --locked -p sml-core --release --example baf_hardened_compare -- \
  --width 12 --seeds 64 --max-degree 3 --parity4 \
  --max-triple-flips 512 --max-quad-flips 1024 --max-evals 40000 \
  --max-basis 400 --json baf-scale-w12.json > baf-scale-w12.csv
cargo run --locked -p sml-core --release --example baf_hardened_compare -- \
  --width 16 --seeds 64 --max-degree 3 --parity4 \
  --max-triple-flips 1200 --max-quad-flips 4000 --max-evals 100000 \
  --max-basis 800 --max-samples 65536 --json baf-scale-w16.json > baf-scale-w16.csv
```

## Amendment 2026-10-25: width-16 sample capacity

On Thor, the first width-16 smoke failed every fit with status 2 (error), not status 1 (budget). The cause was `Split(TooManySamples)`.

At width 16 there are 65536 inputs. OOD takes a quarter, leaving 49152; train (7/12) plus validation (1/4) of those is 40960 samples. That is above the default `FocusFitPolicy::max_samples` of 8192. Widths up to 12 stay under the default (w12 uses 2560 samples).

The fix is the new `--max-samples` flag. It only raises input capacity and leaves the search budgets, splits and metrics as pre-registered. Width 16 runs with `--max-samples 65536`.

Fits that error now print their error kind to stderr; stdout is unchanged. This amendment was made before any width-16 TRAIN, VAL, TEST or OOD result existed.

A local one-seed width-16 check (degree 3, parity4, the budgets above) finished with all 18 fits at status 0. The largest was `random` at degree 3 with 21372 evaluations, and the whole run took about 2 minutes on one core.
