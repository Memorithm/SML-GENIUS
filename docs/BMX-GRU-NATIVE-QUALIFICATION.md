# Frozen Dell GRU: native qualification

Status: implementation qualification only, **not evidence of superiority**.
No reserved corpus was scored for this qualification.

## Provenance

- Training: RemoteOps run `38486923303`, successful Dell job.
- Original checkpoint: `gru512_run.pt`, 4,144,891 bytes, SHA-256
  `b570f6a81778843e53e494a46396cba07ad80b2f183b2102691d3bf7cee56efe`.
- Original weights are retained on RemoteOps `data/bmx-gru-dell-weights`,
  commit `2ab2912880a212eb147759484f760da9bb07ce44`.
- Native export: 4,142,128 bytes, SHA-256
  `6ad7d53991d0b99dfce47522e3e33dd1a0ddbed83a2f7d67e8a6e7b9e865357c`.
- Architecture: 256-byte vocabulary, embedding 64, one GRU layer of 512,
  linear output 256. All 1,035,520 weights remain frozen.

`scripts/eng1d/export-gru.py` admits only the original checkpoint hash and
whitelisted tensor-pickle descriptors; it imports neither torch nor CUDA.
The execution model accepts only the bounded flat native tensor format.

## Execution contract

Pure Rust CPU inference converts stored f32 weights to f64. Dot products have
a fixed four-lane reduction. The reset gate is applied after the recurrent
affine map, matching PyTorch GRU semantics. Hidden state starts at zero and
consumes newline (byte 10) as BOS. Softmax masses have a 1e-12 floor and are
normalized; causal bit conditionals are rounded to p12 and clamped to 1..4095.
Each completed observed byte updates hidden state. No future byte, training,
external process, GPU or Python runtime participates in coding.

This is **not bit-exact PyTorch/cuDNN**. Floating exp/tanh means cross-machine
codec portability is not established. Qualify encode/decode using the same
binary and architecture; changing weights changes container model identity.

## Measurement interface

`sml-bmx-gru-eval WEIGHTS PRIME INPUT NEW_REPORT` measures three fixed arms:

1. Unmodified default BMX-v5.
2. The same BMX-v5 plus GRU, `SideInputs::ProbabilityOnly`.
3. The same BMX-v5 plus a uniform p12=2048 placebo side predictor.

The placebo distinguishes useful network signal from changes caused by adding
an extra mixer input. BMX learns online in all arms; the GRU never learns.
Each encoder and decoder starts independently and receives identical PRIME.
The entire INPUT must roundtrip, not merely a sampled prefix. Reports retain
ideal and effective coded bits, actual payload/container bytes, separate setup
and coding times, byte counts, input/prime/weight/binary SHA-256 and identity.
An incomplete report lacks its final COMPLETE marker; outputs cannot overwrite
an existing report. No hyperparameter selection is performed by this binary.

## Checks completed on 2026-10-14

- Four native unit tests: malformed weights, uniform probabilities, recurrent
  reset placement/replay, nonzero causal state isolation and weight identity.
- Independent NumPy/BLAS recurrence, 285 synthetic states, all 256 outputs:
  maximum absolute probability error `2.8310687127941492e-15`, tolerance 1e-11.
- Actual Dell weights, full default BMX-v5 tables: all three arms roundtrip a
  222-byte synthetic bilingual input after 72 bytes of synthetic prime.

The independent oracle is reproducible with:

```sh
OPENBLAS_NUM_THREADS=1 python3 scripts/eng1d/check-gru-oracle.py \
  target/release/sml-bmx-gru-eval /path/to/native.bin
```

## Scientific limitations and next gate

Training did not retain a pinned full corpus manifest/hash; original directory
and exclusion rules cannot prove complete pretraining/test disjointness.
Training validation loss is not a BMX comparison. Synthetic coding results
must not be reported as a superiority percentage. Before scoring independent
DEV/TEST, separately publish a fixed source manifest, byte splits, selection
rules, effect threshold, statistical decision rule and failure policy. Retain
negative results; never tune the network or mixer on TEST. No production
promotion or merge is authorized by implementation qualification alone.
