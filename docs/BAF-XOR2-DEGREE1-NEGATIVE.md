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

## Update: bounded pair-flip escape (research/baf-pair-flip-escape)
`JointFocusPolicy::max_pair_flips` (default 256; `0` = single-flip-only). Each
round, after all single flips, same-page degree-1 pairs (atoms i<j, page IDs
ascending) are evaluated; a pair is accepted only if it strictly beats the best
single flip. Pair count per round is preflighted against `max_pair_flips`
(fail closed `CandidateBudget`) and pair evaluations are counted in the
`max_candidate_evaluations` worst case. Validation is still consulted only after
the TRAIN winner freezes; veto/rollback semantics unchanged.

Note: escaping only when *no* single improves was tried first and was
insufficient (seed 0 stuck at 16 TRAIN bits): greedy commits early to small
single-flip gains on the wrong page/atom. Letting pairs compete every round fixes it.

Result (8 seeds, `baf_hardened_compare`, mean error bits train/val/test/ood):
- xor2 anf_degree1: 0/0/0/0 (all seeds); was 22.25+ train with single-flip.
- conj2 anf_degree2: 0/0/0/0 (unchanged positive control).
- parity3 anf_degree1: 18.75/7.75/6.50/3.00 (range 0..36) — **remaining plateau**;
  parity of 3 needs a triple of atoms, pairs are not sufficient on every seed.
- Hamming router: xor2 18.75/9.50/7.75/5.00, conj2 17.38/7.75/5.38/5.00,
  parity3 24.75/11.25/8.00/10.50.

Single-flip-only still plateaus; pair escape recovers. Locked by
`xor2_degree1_single_flip_plateaus_pair_escape_recovers`. Research-only; no quality claims.
