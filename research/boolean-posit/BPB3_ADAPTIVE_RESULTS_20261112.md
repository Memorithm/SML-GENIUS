# BPB-3-A1 adaptive precision: frozen Pythia trained-weight reconstruction result

**Observed 2026-11-12, research-only.** Functional validation succeeds. Source-weight reconstruction improves versus tagged fixed BPB-3 at an identical storage budget, but **official Q3_K remains more accurate in all four unseen TEST matrices**. No SML model quality, inference speed, cache/DRAM traffic, industry readiness, standard-posit compliance or claimed originality follows.

## Provenance and frozen design

- Experiment and budget were preregistered in Git commit `b6893295bdd014727050b5d1cf52f4c72d8b105c`, before the trial. Contract: `BPB3_ADAPTIVE_PROTOCOL_20261112.md`.
- Source code PR [#961](https://github.com/Memorithm/SML-GENIUS/pull/961), candidate head at successful hosted run: `c26cec837832f25f6108f367c02f03dc745cd0e1` (GitHub pull-request merge-ref checkout may differ; verify exact-head workflow before merge).
- [Hosted GitHub Actions run 43252808723](https://github.com/Memorithm/SML-GENIUS/actions/runs/43252808723): `adaptive-correctness` and `fresh-trained-weights` succeeded; **13 Rust tests, 0 failed**. [Artifact 13960906264](https://github.com/Memorithm/SML-GENIUS/actions/runs/43252808723/artifacts/13960906264) contains exact report CSVs, SHA256s, pinned-source admission and tensor/source-block manifest. No weights are uploaded.
- EleutherAI *Pythia-14M*, Apache-2.0 trained checkpoint at revision `94f7c35d5e9f2e9bac8ca839329f505b4d007d5d`, `model.safetensors` expected/checked SHA256 `116a02532db461f91386a5b20f942ff2c8d4de7341e21b55caafc3d7b25f49a1`.
- Four never-used-in-prior-E1 matrices: attention output and QKV from layer 2, MLP expansion and projection from layer 3. Each matrix has 24 DEV and 24 disjoint TEST blocks of 256 coefficients, sampled deterministically before the run, with indices saved in artifact. Each split comprises 6,144 weights per matrix; the four-matrix TEST pool contains **24,576** weights. These are disjoint blocks and different layers, but still **one model**.
- Official reference K-quant functions linked from pinned unmodified ggml/llama.cpp `f7591a6a01f9273de03331732bdc0b39721168be`, with the source-precision Q3_K/Q4_K C encoder and independent Rust/C dequantizer checks from `reconstruction_e1.rs`.
- Exact source/block budget for adaptive and tagged fixed BPB-3: 96 blocks of 64 weights per matrix/split, each experiment uses **2,592 bytes** (108 bytes per original 256 coefficients, 3.375 bits per weight including the adaptive mode byte). The standard native Q3_K reference is **110 bytes/256** (3.4375 bits/weight); native Q4_K **144 bytes/256** (4.5 bits/weight). Original *untagged* BPB-3/v0 is 104 bytes/256, not the same budget.

## TEST reconstruction: all four predefined strata

| Source matrix | Tagged fixed BPB-3 RMSE | Adaptive BPB-3-A1 RMSE | Official Q3_K RMSE | Official Q4_K RMSE | Adaptive vs fixed RMSE change | Adaptive vs Q3_K RMSE change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| attention_output_layer2 | 0.004495346 | 0.004255130 | **0.002693042** | 0.001297310 | −5.34% | +58.00% |
| attention_qkv_layer2 | 0.010496492 | 0.007039584 | **0.006919798** | 0.003129999 | −32.93% | +1.73% |
| mlp_expansion_layer3 | 0.006562467 | 0.006388893 | **0.004382391** | 0.002049187 | −2.64% | +45.79% |
| mlp_projection_layer3 | 0.004850245 | 0.004777157 | **0.003219023** | 0.001527296 | −1.51% | +48.40% |

The pooled TEST RMSE is computed as `sqrt(sum_i(n_i*RMSE_i^2) / sum_i(n_i))`, **not** the arithmetic mean of four RMSEs. Equal sample sizes `n_i=6144` yield:

| Codec | Pooled TEST RMSE |
| --- | ---: |
| BPB-3 fixed tagged | 0.007017395 |
| BPB-3-A1 adaptive | 0.005729334 |
| ggml Q3_K | **0.004601726** |
| ggml Q4_K | 0.002122010 |

Adaptive reduces pooled RMSE by **18.36%** versus tagged fixed BPB-3 at 108B/256, while its RMSE remains **24.50% above** Q3_K's at 110B/256. These are descriptive, deterministic *sample* differences only; no confidence interval is claimed.

Mode choices on TEST out of 96 blocks per matrix are (1 magnitude plane / 2 / 3, or total 2/3/4 bits per weight):

- attention output: **12 / 72 / 12**
- attention QKV: **34 / 28 / 34**
- MLP expansion: **9 / 78 / 9**
- MLP projection: **2 / 92 / 2**

The allocation always spends exactly the declared budget in this experiment and exchanges lower-precision blocks against higher-precision blocks. The fixed 3-bit tagged codec is feasible within the DP budget, so non-increasing source SSE versus it is a **guaranteed optimization property**. The informative findings are the *magnitude*, distribution, and cross-format comparisons, not merely the sign of the fixed-control comparison.

## What is and is not validated

1. **A1 Boolean arithmetic and representation: PASS.** Rust 1.89 format, strict compiler, tests of signed bitplanes across three domains, canonical rejection, exact reference POPCOUNT integer dots, brute-force global allocation oracle, budget accounting, and checked i128 `2^-24` inter-block scale alignment.
2. **A2 real trained-weight reconstruction: PASS with an explicitly negative Q3_K comparison.** The same source matrices and corresponding blocks underlie the RMSE reference and candidate. All four TEST matrices are retained. Signed bipolar dot RMSE is comparable between the *two BPB-3 modes*, which use the same per-block mask seeds; the reused original GGML E1 harness uses a different mask protocol, so **dot RMSE across BPB-3 and Q3_K must not be ranked as paired** without re-running both with identical masks.
3. **A3 external generalization: NOT QUALIFIED.** No independent second trained architecture, integrated SML perplexity/task/accuracy, model end-to-end runtime, physical DRAM traffic/RSS or production read path. This trial does not establish quantizer superiority over Q3_K, although a narrower further examination of attention QKV is justified as a prospective hypothesis.

## Negative findings and next measured experiments

- On TEST all Q3_K RMSE values were lower, despite its 2-byte/256 higher storage cost. Q4_K was lower still at a substantially higher 144-byte budget.
- The eight DEV/TEST matrices all show adaptation versus fixed reconstruction improvement, but that alone is mathematically expected from the DP's superset of fixed choices.
- Next A3: freeze one new independent trained model, use source-matched native quantization and a *shared* dot mask corpus, then measure held-out task perplexity and original-model token changes; build a physical CPU activation and memory-traffic comparison via RemoteOps only on admitted isolated hosts. No automatic production promotion.

### References

- [Preregistered contract](BPB3_ADAPTIVE_PROTOCOL_20261112.md)
- [Raw evidence artifact](https://github.com/Memorithm/SML-GENIUS/actions/runs/43252808723/artifacts/13960906264)
- [Prior negative Pythia BPB-3 versus Q3_K evidence](https://github.com/Memorithm/SML-GENIUS/issues/853#issuecomment-6710236853)