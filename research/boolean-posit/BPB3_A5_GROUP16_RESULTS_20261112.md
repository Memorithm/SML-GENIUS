# BPB-3-A5 — held-out local16 mixed-precision results, 2026-11-12

**Status: RESEARCH-ONLY.** Arithmetic correctness PASS; superset recovery relative to A4 PASS; prospective superiority over A1 and native Q3_K **FAIL** at the declared 108 B/256 budget. No production model activation.

## Frozen prospective source and audit chain

- [A5 protocol](BPB3_A5_GROUP16_PROTOCOL_20261112.md) frozen **before new TEST was read**, commit `6e08b83d5ed589259e45fa048449abeb08bb67bd`.
- Model: [HuggingFaceTB/SmolLM2-135M](https://huggingface.co/HuggingFaceTB/SmolLM2-135M), original pretrained Apache-2.0 source, immutable revision `93efa2f097d58c2a74874c7e644dbc9b0cee75a2` and model.safetensors SHA256 `80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`.
- New, preselected original trained matrices: layer10 attention output and q_proj, layer11 MLP up_proj and down_proj. These are different tensors from A3 (layers2/3) and A4 (layers6/7) but **the same model architecture**, not independent out-of-model evidence.
- 24 DEV and 24 disjoint TEST 256-element source rows per tensor, with deterministic SHA-seeded selection and exact BF16→FP32 widening, SHA manifest retained.
- [Green standalone A5 + held-out workflow](https://github.com/Memorithm/SML-GENIUS/actions/runs/43269763129); 11 Rust 1.89 tests passed / 0 failed (including A4 inherited oracles, 2-bit mode wire, signed POPCOUNT, group-scale exact i128 dot, exhaustive vs DP and A5≤A4 at identical storage). [Raw evidence and provenance artifact](https://github.com/Memorithm/SML-GENIUS/actions/runs/43269763129/artifacts/13967801305); [standalone tests](https://github.com/Memorithm/SML-GENIUS/actions/runs/43269763129/artifacts/13968830210).
- Exact `a5-all-results.csv` SHA256 `554c28212acb5ea4e863b65797556b302b286222e464c36996c9bd0d999ef22d`, selected source manifest SHA256 `93cf1696ec02bf3650695b00f99a4d6ee23bb8a2e1b4ff418da1323c307519c4`.
- All five other controls on the **same original source** and with **exactly the same sixteen bipolar masks per row**: fixed tagged A1 108B/256, A1 adaptive 108B/256, A4 local16 108B/256, pinned stock ggml Q3_K 110B/256, Q4_K 144B/256. Models and ggml reference hashes verified before testing.

## Why this is a genuinely new bit allocation strategy

A5 replaces A4's single 1-byte mode for a whole 64-lane block by four 2-bit fields (still a **single byte**) selecting 1/2/3 magnitude planes separately per 16-lane subgroup, alongside A4's four 1-byte dyadic exponents and common 8-byte sign plane. Wire cost: **13 + 2 × Σ(mode_g) bytes per 64**, from 21 to 37 bytes. Exactly 108 bytes per 256 original coefficients are charged in TEST, no free local scale or precision flags. An exhaustive 52-exponent local source quantizer and exact global two-byte-step DP select group precisions under 2,592 bytes per 24×256 tensor split.

A5 includes **every uniform-A4 configuration** at the same wire size, so non-increasing source SSE relative to A4 is a mathematical consequence of an expanded feasible set. This does not imply anything about stock Q3_K, nor guarantee lower error on finite sampled bipolar dots.

## TEST results on all four frozen matrices

| Matrix | A5 weight RMSE | A4 weight RMSE | A1 adaptive weight RMSE | Q3_K weight RMSE | A5 dot RMSE | A1 adaptive dot RMSE |
|---|---:|---:|---:|---:|---:|---:|
| attention_output_layer10 | 0.040656579065 | 0.042491767190 | **0.037264268715** | 0.026234826737 | 0.656170776425 | **0.579348569581** |
| attention_q_layer10 | 0.050917516475 | 0.054002376237 | **0.047888190884** | 0.031963393930 | 0.850028812549 | **0.778160912133** |
| mlp_up_layer11 | 0.049366774529 | 0.052935543615 | **0.045631916958** | 0.030512116364 | 0.807094865967 | **0.750035256987** |
| mlp_down_layer11 | 0.049550436477 | 0.053032737931 | **0.045588168929** | 0.030741665323 | 0.821047553787 | **0.740353280885** |

Every matrix contains 6,144 TEST weights and 384 paired bipolar dot samples. No matrix omitted after observing the outcome.

## Pooled TEST, correctly sample-weighted

The pooled RMSE is `sqrt(sum(n_i * RMSE_i^2) / sum(n_i))` across four matrices, **not** the mean of matrix RMSEs. Each arm has 24,576 original source weights and 1,536 exact same-mask dot samples.

| Codec | Bytes / 256 weights | Weight RMSE | Matched bipolar dot RMSE |
|---|---:|---:|---:|
| Tagged fixed BPB-3 | 108 | 0.045255256668 | 0.734699036989 |
| **A1 adaptive** | **108** | **0.044278825472** | **0.716214883518** |
| A4 local16 shared mode | 108 | 0.050834166631 | 0.830796475276 |
| **A5 local16 mixed modes** | **108** | 0.047796116703 | 0.787183313823 |
| Stock Q3_K | 110 | **0.029941459205** | **0.474734462767** |
| Stock Q4_K | 144 | 0.014188110970 | 0.226904241060 |

- **A5 vs A4**: −5.98% weight RMSE, −5.25% dot RMSE. The more flexible precision groups partially recover the metadata penalty.
- **A5 vs A1 adaptive**: **+7.94%** weight RMSE, **+9.91%** dot RMSE — still worse at identical 108B/256.
- **A5 vs official Q3_K**: **+59.63%** weight RMSE, **+65.82%** matched dot RMSE, despite Q3_K costing only 2 more bytes per 256 coefficients.
- A5 mode counts over all four TEST matrices: 455 one-plane / 1,010 two-plane / 71 three-plane **16-coefficient groups**. These counts must not be compared directly to A4's 64-coefficient block mode counts (different unit of analysis).

## Scientific decision

**Validated**: A5 on-wire mixed precision, correct signed Boolean arithmetic, exact knapsack with independently verified A4 inclusion property, repeatable original-source experiments on a fresh within-model TEST set.

**Not validated**: improving A1 or competing with Q3_K, model-level perplexity/task scores, generated-token fidelity, encoder throughput, real memory traffic, kernel speed, or standard Posit format compliance. Both local-exponent formats (A4 and A5) remain strictly experimental; A1 retains the lowest reported BPB adaptive RMSE at 108B/256 on this new TEST.

The remaining testable avenue is **more compact subgroup scale metadata** rather than spending 4 whole exponent bytes per 64 weights: e.g. shared base exponent plus fully costed two-byte signed 4-bit local offsets, with fail-closed range checks. This is a new hypothesis requiring fresh prospective TEST, not an observed improvement or reason to retune prior TEST results.

No physical Dell/Thor/Mac workloads were started.
