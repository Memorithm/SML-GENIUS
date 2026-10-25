# BAF xor2 at ANF degree 1 — negative result (search, not expressivity)

Bench: `crates/sml-core/examples/baf_hardened_compare.rs` (BAF-3b, PR #708), 8 seeds.

## Setup
- Target: `next = state | gate0<<2 | gate1<<3`, `gate_k = event[e_k] XOR state[s_k]`.
- Writers: pages 10/20 with `write_value == write_mask` (set-only, OR-like).
- Fitter: `fit_joint_focus`, degree 1, basis = all 7 atoms (true XOR atoms present),
  greedy single-flip, strict TRAIN improvement, then validation admission.

## Findings
- **Expressible (not (a))**: activation `e_k ^ s_k` (two degree-1 terms, bias 0) gives
  0 error bits on TRAIN/VALIDATION/TEST/OOD for all 8 seeds. Set-only writes are
  harmless here: when the target bit is already 1 a write of 1 is a no-op, and the
  write fires exactly when `gate_k` is 1.
- **No split/label bug (not (c))**: same labels give 0 errors under the oracle.
- **Greedy plateau (b)**: a single XOR atom alone is a parity half, so single flips
  tie or worsen TRAIN (e.g. seed 0 baseline 32, singles 27–37; seed 3 baseline 32,
  singles 31–35). Descent stops in local minima: TRAIN 33.9 → 22.25 mean, 0/8 exact.
- **Admission rollback (d), secondary**: 4/8 seeds are rolled back to the inactive
  atlas because validation worsens; this explains the reported ~24.8 TRAIN mean.

## Locked by
`xor2_degree1_representable_but_greedy_misses` (example test). If a future search
change (e.g. bounded pair-flip escape) solves xor2, that test must be updated
together with this note.
