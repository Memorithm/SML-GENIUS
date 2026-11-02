# BDE-0 supervised learning and measurement protocol (experimental)

Status: implementation candidate only; not an SML default-route or scientific-quality promotion.

## Candidate and learning rule

`BooleanDeltaLearner` wraps the native `BooleanDeltaEngine`. The engine
computes up to 12 Boolean output features; the readout maintains two `u64`
vote counters (negative/positive) per feature signature. Training on a
labelled example increments the corresponding class counter. Prediction is
`positive > negative`, with ties returning false. There are no floating-point
weights, Transformer layers, GPU or external runtime dependencies.

Training examples and feature-table capacity are bounded at construction.
A rejected input shape or exhausted training budget leaves the model unchanged.
This is a supervised Boolean truth-table baseline, **not** a demonstrated
generalization mechanism for unseen feature signatures.

## Frozen synthetic protocol

- Four input bits: `a, b, c, nuisance`.
- Circuit: `xor(a, b)`; readout features: `[xor(a, b), c]`.
- Target label: `xor(a, b) AND c`.
- TRAIN: all eight assignments of `(a,b,c)`, with nuisance=false.
- HELDOUT: the corresponding eight **distinct full input assignments** with
  nuisance=true. Their readout signatures overlap TRAIN by construction.
- Control: untrained readout, predicting false on all ties.
- Metrics: mistakes on heldout before and after training; online pre-update
  mistakes; exact gate evaluations and table bytes; training and inference
  wall-clock nanoseconds against a full-circuit reference.
- Reproducibility: fixed example order and no random seeds or data downloads.

The test's fixed quality expectations are 2/8 mistakes before and 0/8 after
training, and one online pre-update mistake. These are **test oracles**, not
measured results until a qualifying run actually passes.

## Execution and evidence boundaries

Run from a qualified Rust checkout, with versioned pre-push hooks installed:

```bash
bash scripts/install-hooks.sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo test --locked -p sml-core boolean_delta_engine::learning_tests -- --nocapture
```

The final command emits `BDE0_LEARNING_MEASURE_V1` with counts and elapsed
nanoseconds. Record the exact source SHA, compiler, host/runner, execution
command, output and outcome. No timing assertions or speedup claims are made:
CI machines have variable load, no warmup or CPU-frequency pinning is applied,
and the two implementations have different memory layouts. Learned table
bytes are not peak RSS. Logical gate counts are not latency measurements.

The comparison only demonstrates a bounded synthetic learning rule. It does
not establish model-level quality, long-context behavior, adaptive learning
superiority, architectural novelty or performance superiority. No LONGCTX-B1
campaign is authorized by this protocol. Promotion requires independent
train/validation/test datasets and resource-qualified, preregistered repeats.
