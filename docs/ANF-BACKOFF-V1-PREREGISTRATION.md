# ANF-BACKOFF v1 — sparse full-byte conditional readout preregistration

Date: 2026-10-12. Status: **frozen Development-only functional experiment**.
This protocol commit must precede new module source and unit tests. It neither
revises A6-E1/E2/E3 STOP outcomes nor replaces the separate A6-E4 Candide
full-byte matched gate-study protocol already merged as PR #516.

## Hypothesis / relationship to existing work

`sml_core::anf_sparse_readout::SparseBinaryReadout` already supports
hierarchical sparse 2-class conditional estimates over feature signatures.
A6-E3's learned readout instead uses flat multinomial 256-class tables and
cannot fall back to a shorter selected Boolean context within that readout.
Test a direct byte-alphabet extension of the binary mechanism, preserving
the existing public binary API and the native Witten-Bell ngram4 baseline.

This module consumes caller-supplied *frozen* feature signatures. It does not
learn Boolean coefficients, candidate gates, feature order, recurrent state,
or infer unobserved labels. It is a distribution readout, not an architecture
promotion.

## Frozen algorithm

- TRAIN input: `(signature:u64, target_byte:u8)`, at most 500000 rows,
  fixed 0..16 signature bits in a caller-frozen significance order.
- For every prefix depth 0..width, store sparse counts only for observed
  signatures and their 256 next-byte classes. Each depth sums exactly to the
  global number of TRAIN rows.
- Total context cap 8192 across all depths; checked `u32` totals/counts.
  Refuse invalid widths, out-of-range signature bits, empty input and
  exceeded context budgets (never silently drop contexts).
- Witten-Bell interpolation: initialize `P_(-1)(byte)=1/256`. For each
  existing prefix address `h` of depth d, let `n(h)` be its total count
  and `T(h)` its number of distinct following bytes. Then
  `P_d(byte|h) = [count(h,byte) + T(h)*P_(d-1)(byte)]/[n(h)+T(h)]`.
  Missing prefixes retain `P_(d-1)`. Use the frozen order 0..width.
- Return all 256 probabilities. Require strictly positive, finite
  normalized values on observed and unobserved feature signatures.
  Prediction never mutates the fitted counts or context census.
- Report stored-prefix count, TRAIN example count and logical dense
  counter payload separately from total resident memory.

## Functional gates (synthetic only)

1. Exact deterministic counts and 256-class probability normalization.
2. Backoff from an unseen high-order signature to informative lower
   feature context; correct observed label gets probability >0.75 in
   the frozen balanced binary-label fixture embedded in a byte alphabet.
3. Identical model for permuted TRAIN row order, no mutation from
   prediction, stable result for a never-observed signature.
4. Reject out-of-domain feature bits, over-budget context stores,
   empty TRAIN and widths >16; preserve the old sparse binary API.
5. CI: pinned Rust 1.89, `fmt --check`, `cargo check`,
   Clippy `-D warnings`, unit tests, no model/production changes.

This functional evidence does not establish that sparse byte backoff beats
the current competent ngram4 baseline or a raw-feature conditional readout.
A future separately predeclared matched real-corpus study is mandatory.
Do not evaluate E4 frozen Candide TEST for model selection until E4's
predeclared candidate policies are frozen.

References:
- Chen & Goodman (1999), Computer Speech & Language, DOI 10.1006/csla.1999.0128.
- `crates/sml-core/src/anf_sparse_readout.rs`
- `crates/sml-core/src/char_baselines.rs`
- `docs/RESULTS-A5-STEP3.md`, `docs/RESULTS-A6-E3-FULL-BYTE-BOOLEAN-BANK-STOP.md`.
