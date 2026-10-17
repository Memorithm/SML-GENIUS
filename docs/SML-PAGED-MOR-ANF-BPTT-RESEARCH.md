# Disk-paged MOR/ANF truncated BPTT: scientific integration candidate

Status: **research implementation — requires exact-head CI and native Dell evidence before merge**.

## What changed

`MorAnfRecurrent::train_document_bptt(text, window_bytes)` uses the existing actual disk-backed `SparseGradientAtlas` and existing two learned MOR gate experts. It does not allocate a dense 50M model into RAM. For each window of 1..=512 predicted bytes it first evaluates both routed expert pages at all eight bit positions, retains the scalar recurrent trace, and computes backward gradients through the entire *frozen-parameter* window before applying any weights.

The chain rule covers the bitwise mixture cross-entropy, expert logits, MOR gate coefficients, and recurrent parameters. Page-gradient updates use `apply_expert_gradient` so the actual first-touch bitmap and on-disk weights are updated. The existing forward-mode `train_document` remains unchanged as the RTRL comparison/control.

**Limit:** the candidate page addresses and nonlinear GF(2) ANF monomial topology are fixed. Only the mixture gate and recurrent coefficients are trained, along with the physical f32 expert weights. The recurrent state resets at document and truncation boundaries. This is truncated BPTT, not full-document BPTT and not evidence that a 25B model is trained.

The code returns summed loss in bits and the number of predicted bytes. Two physical expert gradients are recorded per supervised output bit, and distinct actually touched weights are recorded by the atlas. Small deterministic unit tests check loss improvement on in-sample sequences, numerical recurrence-gradient agreement with finite differences (on physically learned expert pages), counter identities and invalid-window rejection.

## Validation and experiment gates

1. Require format, Clippy and all `sml-core` unit tests green on the exact head SHA.
2. On the Dell, compare frozen independent SEC validation documents between current online RTRL `train_document` and bounded BPTT `train_document_bptt`, under identical update exposure, train documents, source SHA and page budgets. Do **not** reuse the same disk snapshots for the two arms.
3. Record wall time, max resident page budget, physical distinct parameter touches, page loads, in-sample and independent holdout log loss, and SHA-verified checkpoint/reopen (both head and atlas).
4. Keep 2024 TEST unopened and the disputed issuer-document rights gate `NOT_ADMITTED`. A stronger result on a small SEC holdout is not conversational or financial predictive capability.

## Open limitations

* Window BPTT is per 512 predicted bytes maximum; it does not backpropagate across truncation boundaries.
* The disk atlas performs f32 updates after a frozen f64 backward pass; atomic all-or-nothing optimizer transactions have not been implemented.
* The system does not yet learn ANF monomial selection, candidate-page addresses, or activation decisions end-to-end. No guarantee of useful learning across all 50,000,896 allocated weights.
* No claims of FR/EN chat readiness, generalization, training on 100 GB, production use, or 25B model quality.

## Native SHA-bound pilot entrypoint

The existing `sml-mor-anf-rtrl-fit` tool retains `--pilot` as the unchanged RTRL control. The new `--pilot-bptt` mode uses 48,829 physical atlas pages = 50,000,896 f32 training master weights (200,003,584 bytes), with only 32 cached pages. It SHA-verifies the complete existing 10GiB SEC UTF-8 corpus and each consumed document plus provenance ledger, then actually trains on separately selected *small* document prefixes (at most 1,024 bytes each, TRAIN target 12 KiB) and evaluates on different document prefixes (DEV target 8 KiB). The metric `train_bytes` is **consumed** prefix bytes, not 10GiB, and all counters measure actual gradient application. The pilot does **not** train all allocated weights nor claim that full 10GiB received BPTT gradient updates. Retain artifact contents limited to metrics and SHA-bound manifests; keep physical master weights private on the Dell.
