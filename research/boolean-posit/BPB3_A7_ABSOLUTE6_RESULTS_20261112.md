# BPB-3-A7 — absolute 6-bit exponent compression, 2026-11-12

**Status:** Research-only. **Wire/Boolean arithmetic/DP PASS, prospective competitive hypothesis versus A6-delta2 FAIL**. No change to production inference or numeric format defaults.

## Preregistration and evidence

- Experimental protocol [BPB3_A7_ABSOLUTE6_PROTOCOL_20261112.md](BPB3_A7_ABSOLUTE6_PROTOCOL_20261112.md) was frozen in commit `4545639ba3c3c725a430f5fd81d15e529b9c558a` before new TEST weights were read.
- Model: pretrained Apache-2.0 SmolLM2-135M, immutable HF revision `93efa2f097d58c2a74874c7e644dbc9b0cee75a2`, SHA256 of original safetensors `80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`. Fresh layer24 attention o_proj and q_proj plus layer25 MLP up_proj/down_proj, **different tensor strata from A3 2/3, A4 6/7, A5 10/11, A6 18/19**; still the *same trained model*, not new architecture replication.
- Four matrix strata × 24 disjoint TEST 256-weight rows = **24,576 original held-out weights**; each codec uses the same original FP32-widened weights. Identical 16 bipolar masks per row = **1,536 matched 256-weight signed-dot probes per codec**. Separate 24 DEV rows per matrix. The manifests record source-index selections and exact input SHA.
- [GitHub Actions A7 qualification run 43280770457](https://github.com/Memorithm/SML-GENIUS/actions/runs/43280770457): `a7-correctness` and `a7-heldout` both **success**. Rust 1.89 strict format plus **32 tests passed and zero failed**, including inherited independent A4/A6 signed-dot oracles, reserved 6-bit exponent codes, negative zero, exact multiple-choice budget and representation of A6-delta4 choices at equal bytes.
- [Complete experimental CSV and selection manifest, artifact 13972003687](https://github.com/Memorithm/SML-GENIUS/actions/runs/43280770457/artifacts/13972003687). CSV `a7-all-results.csv` SHA256 `4abcd44f0bea487886af7682c21c7747091840360f4b32712bcc94f33109d0a1`. Manifest `a7-source/selected/manifest.json` SHA256 `8a47f3791df9845be598b588812044cb6bf002ed8efa55d027926968556a52fb`.
- Control Q3_K and Q4_K are from pinned unmodified ggml commit `f7591a6a01f9273de03331732bdc0b39721168be`, with encoded/decoded results validated against independent C and Rust reconstruction.

## Storage budget and algorithm

A7 puts four absolute signed dyadic exponents in a **24-bit packed field**, each exponent represented as biased code `exponent + 24` in 0..51; 52..63 is reserved and rejected. One 64-coefficient group block has one 1-byte 4×2-bit local precision header, three 6-bit exponent bytes, 8-byte sign plane and 2-byte/16-lane magnitude plane. **Total 12 + 2×sum(local plane counts)**, min 20B, max 36B per 64 weights. A7 is compared with fully charged **108B/256 original weights**, matching all A1/A4/A5/A6 controls; stock Q3_K uses 110B/256 and Q4_K 144B/256. No uncounted scale, positional or entropy decoder metadata.

The independent per-subgroup 52-scale source optimizer uses FP64 SSE over exactly widened original FP32 weights and an exact global 2-byte-step DP across the selected original groups. A7 includes every 4-bit-base-delta A6 configuration at the same actual packed byte size, so its optimal source SSE cannot be higher than A6-delta4's on the **same source**. It cannot be assumed to beat A6-delta2, whose scale metadata costs one byte fewer per 64 coefficients.

## All frozen TEST matrix results

| Source tensor | A7 absolute6 weight RMSE | A6 delta2 weight RMSE | A6 delta4 weight RMSE | Official Q3_K weight RMSE | A7 signed dot RMSE | A6 delta2 signed dot RMSE |
|---|---:|---:|---:|---:|---:|---:|
| attention_output_layer24 | 0.045699871089 | **0.041649250643** | 0.045699871089 | **0.031316385077** | 0.697976000899 | **0.642896589929** |
| attention_q_layer24 | 0.034661212683 | **0.031772043912** | 0.034661212683 | **0.025859563191** | 0.547684514386 | **0.513020184746** |
| mlp_up_layer25 | 0.046560315617 | **0.042263982919** | 0.046560315617 | **0.031385389752** | 0.802219138209 | **0.720843612105** |
| mlp_down_layer25 | 0.043837393618 | **0.039833705256** | 0.043837393618 | **0.030292215208** | 0.655912480427 | **0.608225023924** |

Each row represents 6,144 original TEST weights and 384 paired signed-dot samples per codec. All four original TEST strata, including regressions, are retained. No post-hoc exclusions.

## Pooled TEST metrics, each method on identical original sample counts

Pooled RMSE uses `sqrt(sum(n_i * RMSE_i²) / sum(n_i))`, **not** the arithmetic mean of per-matrix RMSE values.

| Codec | Bytes per256 | Source weight RMSE | Paired bipolar dot RMSE |
|---|---:|---:|---:|
| A1 fixed tagged | 108 | 0.044713938419 | 0.721107441089 |
| A1 adaptive | 108 | 0.043225096223 | 0.706244063356 |
| A4 local16 | 108 | 0.049881310460 | 0.785494485516 |
| A5 local16 mixed | 108 | 0.047289080622 | 0.764516581303 |
| **A6 delta2** | **108** | **0.039105917647** | **0.625711380778** |
| A6 delta4 | 108 | 0.042951885732 | 0.682075060535 |
| A6 mixed | 108 | 0.039105917647 | 0.625711380778 |
| **A7 absolute6** | **108** | **0.042951885732** | **0.682075060535** |
| Official Q3_K | 110 | **0.029799723047** | **0.471116522306** |
| Official Q4_K | 144 | 0.014130221641 | 0.222827398322 |

**A7 absolute6 vs A6 delta2**: +9.83% original source RMSE and +9.01% bipolar dot RMSE (worse). **A7 vs official Q3_K**: +44.14% source RMSE at 2 fewer bytes/256. A7 and A6 delta4 achieve *identical* source and dot RMSE on all four TEST matrices, with exactly the same local mode allocation counts in this study: A7 mode1/2/3 groups across 4 matrices = **323 / 1,082 / 131**, from the complete raw CSV. This does not establish general equivalence between these formats.

The chosen 2-bit delta A6 remains best of the measured BPB-3 family here, and the precision-group allocator's additional degrees of freedom are not sufficient to offset one extra byte of scale metadata per 64 weights. That is an observed trade-off; a specific causal ablation still requires a separate prospective test.

## Decision

**Accept A7 as a validated alternative encoding and a negative, documented, source-matched scientific control, not as the new default.** Keep A6 delta2 as the research leader at 108B/256. For next steps, prioritize the preregistered engineering hypothesis of **one shared 256-weight superblock base plus sixteen signed 2-bit deviations**, with every byte and navigation cost accounted for. That format may save 3 scale bytes per256 versus A6's four independent bases, but may lose precision if the 16 local scales cannot share one base. No claim of actual savings or quality until independent prospective measurements.

No full-model perplexity, task-level accuracy, generated token agreement, native runtime throughput, CPU/DRAM traffic, energy or standard Posit compliance was measured. No physical Dell/Thor/Mac jobs were run.
