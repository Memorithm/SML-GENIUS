# A6-E4 — ANF-JOINT: joint feature-subset selection (preregistration)

Status: **Development-only preregistered protocol; no quality or physical result claimed**.
Frozen on 2026-10-12 **before writing the A6-E4 Rust implementation or running its TEST cases**.
Existing A6-E1/E2/E3 negative scientific verdicts remain unchanged.

## Scientific hypothesis

The A6-E3 greedy 128-candidate selector admitted 96 additional composed Boolean gates but chose none. A one-feature-at-a-time selector can miss XOR synergy with zero marginal information. A bounded pair-first TRAIN-only search may discover jointly informative candidates that a strictly improving greedy baseline misses. The gain must be attributed to the actual selected features, not to a larger candidate dictionary alone.

This is **not** the existing A6-C10 pair-of-hidden-recurrent-gates system-identification problem, and does not replace the already-merged A6-C1 small (<=16 sources) constructive residual gate search. A6-E4 is a separate byte-class conditional-feature selection primitive, reusing the native ANF gate outputs supplied by the caller.

## Frozen reference algorithm

- Input: only TRAIN rows `(feature_bits:u128, target_byte:u8)`. Feature bits must come from caller-defined fixed ANF or RAW candidates, never TEST labels. This module does not learn the gate equations.
- Hard budgets: 2..128 feature candidates; target alphabet 2..256; 1..8 selected features; <=500000 TRAIN rows; probe <=4096 TRAIN rows; <=32 full-data pair finalists. All invalid dimensions, non-finite/negative smoothing parameters, out-of-domain bits and targets fail closed.
- Predictive fit: empirical conditional multinomial counts per signature with symmetric Dirichlet smoothing. TRAIN leave-one-out objective, exact histogram aggregation; each training point is excluded from its own class count. `alpha=0.5` default per class for the first reference (fixed before science).
- Baseline: greedy selection from empty signature, retaining only strictly improving single-feature additions by TRAIN leave-one-out bits/symbol; ties resolve by lowest feature ID.
- Candidate: score all pairs on a deterministic evenly spaced TRAIN-only probe; rank by probe LOO with lexicographic ties; rescore the at-most-32 top pairs on **all TRAIN**; retain a pair only if it beats the empty model. Continue from its two-feature seed with the same single-feature greedy rule until the same feature-count cap. Choose the better completed candidate versus greedy baseline on full TRAIN, never on validation/TEST.
- Frozen models keep train count tables; held-out prediction is a normalized, positive conditional distribution. No refitting on VAL or TEST.
- No arbitrary removal of negative/neutral evidence; candidate-proposal counts, selected feature IDs and actual train LOO values are reported.

## Synthetic qualification panels (independent fixture streams; NOT natural-language evidence)

1. Pure balanced XOR: `y=x0 XOR x1`, nuisance x2. Each of 8 input combinations is repeated 64 times in TRAIN. **Expected:** greedy strictly-improving singleton baseline selects no feature; joint selects exactly `[0,1]`; conditional probability of the correct class >0.95 at each input pattern.
2. Uniform conditional null: for every feature context, equal counts of both labels. **Expected:** neither greedy nor joint admits unsupported features over the empty model.
3. Two-class and 256-class normalization, unseen feature combinations, determinism, absence of prediction-time mutation, out-of-range and budget rejections.
4. Exactness/reference accounting: full TRAIN and probe objectives use the same defined histogram LOO, stable tie breaks, candidate scopes and counts. No claim of statistical significance from repeated synthetic patterns.

Synthetic implementation tests are functionality gates, not evidence that composed ANF features improve French/English next-byte performance. The existing A6-E3 TEST has already been exposed and **cannot** certify A6-E4 improvement. A future, separately preregistered model-aligned comparison requires fresh independent text, matched RAW/ANF populations, normalized 256-class loss, VAL-only policy selection and frozen TEST before any model promotion.

## Integration boundary

Expose an opt-in standalone `sml_core::anf_joint_feature_selection` module without modifying SBG/MOR, Delta-KV, the default inference router, A6-E3's frozen scorer, or any production path. Safe Rust 1.89, no additional crate dependency, no background service or NVIDIA proprietary SDK. Require fmt, strict Clippy and targeted library tests on the exact final PR HEAD before merge. A negative real-data result is not a CI failure.

Sources/antecedents: `docs/RESULTS-A5-STEP3.md`, `docs/RESULTS-A6-E3-FULL-BYTE-BOOLEAN-BANK-STOP.md` (A6-E3 branch until merged), `crates/sml-core/src/anf_constructive.rs`, `crates/sml-core/src/anf_sparse_readout.rs`.
