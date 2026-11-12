# BPB-3-A4 local16 — 2026-11-12 negative held-out result

Status: **RESEARCH-ONLY, functional correctness PASS, equal-budget reconstruction hypothesis FAIL**. No model-integrated inference, no standard Posit implementation, no production activation.

## Frozen before reading new TEST

Original protocol: [A4 preregistration](BPB3_A4_LOCAL16_PROTOCOL_20261112.md) committed `de558c0abc5591e7a1e0afc53cbb227222584013`, **before** source-weights download or evaluation. The exact source is a SHA256-verified Apache-2.0 pretrained SmolLM2-135M checkpoint, HF revision `93efa2f097d58c2a74874c7e644dbc9b0cee75a2`, safetensors SHA256 `80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`. Four fresh layer-6/7 trained matrices have 24 nonoverlapping DEV and 24 TEST 256-weight rows each, not the previously exposed A3 layers2/3. These are different **tensors of the same model**, not an independent second model.

## Exact component and provenance evidence

- A4 Rust 1.89 correctness suite: **5 passed, 0 failed**, including 21/29/37-byte mode wire serialization, invalid/malformed wire, exact signed Boolean POPCOUNT 16-lane group sums vs independent dense integer oracle, exact i128 dyadic summation, budget/knapsack matching a brute-force oracle, and source quantizer.
- [A4 qualification run](https://github.com/Memorithm/SML-GENIUS/actions/runs/43267714117), jobs `a4-correctness` and `a4-heldout`: both success.
- [Raw result CSV, full split manifest and SHA evidence](https://github.com/Memorithm/SML-GENIUS/actions/runs/43267714117/artifacts/13967520701); [standalone test log](https://github.com/Memorithm/SML-GENIUS/actions/runs/43267714117/artifacts/13966846928).
- Source-matched controls run on identical original weights with the same 16 bipolar masks per 256-weight input: tagged A1 fixed 108 B/256, A1 adaptive 108 B/256, A4 local16 108 B/256, official stock GGML Q3_K 110 B/256 and Q4_K 144 B/256 at upstream `f7591a6a01f9273de03331732bdc0b39721168be`. All four TEST matrices are retained.
- Checksum: `a4-all-results.csv` SHA256 `f124459543c6ce10e32717668decbac6b103263be4f8d487fa0f4e0061eb9678`; selected-source manifest SHA256 `74519391605cabd6eca8100db69a34f7c7e3dfe9d210fa89dd0a62b6c55b43f8`.

A4 wire: **1 mode byte + 4 signed local exponents (one per 16) + 8-byte sign plane + mode × 8-byte magnitude plane**. That is 21/29/37 B per 64-weight group, two bytes more than A1's one-exponent 19/27/35 B. An exact global 96-group DP respects 24 × 108 = 2,592 B per tensor/split. At this budget, A4 is *not* guaranteed to improve A1 because the extra scale bytes consume bitplane capacity.

## All four TEST source matrices

| Matched trained matrix | A1 adaptive weight RMSE | A4 local16 weight RMSE | Official Q3_K weight RMSE | A4 vs A1, weight RMSE |
| --- | ---: | ---: | ---: | ---: |
| attention_output_layer6 | 0.035554364726 | 0.040130172419 | **0.024701323860** | +12.87% |
| attention_q_layer6 | 0.044196147581 | 0.049012769556 | **0.031365680823** | +10.90% |
| mlp_up_layer7 | 0.043168966126 | 0.051382327566 | **0.029321369110** | +19.03% |
| mlp_down_layer7 | 0.041991900190 | 0.050712643867 | **0.029172309055** | +20.77% |

Each TEST matrix has 6,144 original coefficients and 384 shared-mask bipolar dots per format. Pooled metrics use **sqrt(sum(coeff_count × RMSE²) / sum(coeff_count))** for weights (and analogously for dots), not arithmetic averaging.

| Codec | Wire bytes per 256 | Pooled TEST weight RMSE | Pooled TEST dot RMSE |
| --- | ---: | ---: | ---: |
| A1 tagged fixed | 108 | 0.043225556416 | 0.697679926503 |
| **A1 adaptive** | **108** | **0.041365117037** | **0.662260980482** |
| A4 local16 | 108 | 0.048022385801 | 0.753259757322 |
| Stock Q3_K | 110 | **0.028743380264** | **0.466706807633** |
| Stock Q4_K | 144 | 0.013638439047 | 0.216830339888 |

Pooled across 24,576 TEST source coefficients and 1,536 matched dots: **A4 is 16.09% worse in source RMSE, 13.74% worse in dot RMSE compared with A1 adaptive**. It is 67.07% worse in source RMSE than the official Q3_K. All TEST matrices are negative versus A1.

Observed A4 TEST mode counts across all 384 groups are **105 mode-1 / 270 mode-2 / 9 mode-3**, compared with A1 adaptive **27 / 330 / 27**. This demonstrates the precision redistribution caused by A4's extra bytes; it is consistent with the hypothesis that metadata displacement dominates, but does not establish causality.

## Decision

- **Accept** A4 as an independently serializable, verified research counterexample with fully accounted extra local scales.
- **Reject** A4 local16 as a *replacement* for A1 or Q3_K at this budget; no default model path changed.
- Next prospective design: independent **2-bit precision choice per 16-lane subgroup**, instead of sharing one mode across all 64 lanes. This can reuse the same 1-byte header and 4 local exponents (A5 local16 mixed bitplanes); it can represent all A4 configurations at identical bytes and may improve the source SSE, but no empirical superiority claim is made. Freeze fresh test data and compare all outcomes; do not rerun this exposed A4 TEST for prospective claims.

No physical Dell/Thor/Mac runners were used. Full-model perplexity, model-generated tokens, throughput, DRAM, RSS and energy remain untested.
