# BDE-1 affine Boolean learning and measurement protocol

Status: experimental research candidate; no default-route promotion or
claim of superior model quality.

## Hypothesis and learning rule

`BooleanDeltaAffineLearner` consumes the Boolean output features of the
existing BDE-0 incremental circuit. It learns an affine GF(2) label rule,
`label = bias XOR (selected feature bits)`, using deterministic online
Gaussian elimination over at most 12 features plus one intercept column.
Training accepts consistent redundant equations, rejects contradictory labels
without modifying the model, and enforces an example budget. The model
abstains (`None`) whenever the observed equations do not uniquely determine
a queried label. It does **not** learn arbitrary nonlinear targets, noisy
labels, or long-context dynamics.

## Frozen independent-signature test

- Four independent input bits, forwarded as four readout features.
- Target: `NOT (feature_0 XOR feature_2)` (nonzero affine intercept).
- TRAIN signatures (decimal bitmasks): `0, 1, 2, 4, 8`.
- HELDOUT signatures: `3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15`.
- No HELDOUT readout signature appears in TRAIN. The five TRAIN rows span
  the five-dimensional augmented feature space under the affine assumption.
- Baseline: BDE-0 `BooleanDeltaLearner`, trained on exactly the same five
  examples, with its deterministic false-on-unseen-signature behavior.
- Metrics: heldout coverage/abstentions, errors for each learner, GF(2) rank,
  training nanoseconds, 20,000 inference calls per candidate, logical gate
  work, and fixed pivot-array capacity bytes.
- Negative controls: untrained/underdetermined abstention, redundant
  equations, contradictory labels, invalid input lengths and training-budget
  rejection with full-model state preservation.
- Deterministic order, no randomness, no external data or dependencies.

A test passing proves correctness **only for this exact synthetic affine
family**. It does not establish generalization to arbitrary unseen data or
superiority on realistic tasks. Inference timings are observational on CI
hardware; there is no asserted speedup or process-RSS claim.

## Qualification

On a qualified checkout with the repository's versioned hooks installed:

```bash
bash scripts/install-hooks.sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo test --locked -p sml-core --lib boolean_delta_affine::tests::measured_affine_generalization_against_truth_table -- --exact --nocapture
```

The focused test emits `BDE1_AFFINE_MEASURE_V1`. CI must record its output
alongside the exact PR head SHA. Record compiler and runner provenance before
interpreting elapsed time. The experiment is separate from CONTEXT-3 and
LONGCTX-B1 and authorizes no LONGCTX-B1 campaign or GPU dependency.
