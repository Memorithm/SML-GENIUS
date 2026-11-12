# BPB-3-A3 — SmolLM2-135M source reconstruction and matched bipolar dots

Date: 2026-11-12. Research only; **NOT** model end-to-end inference or standard posit compliance.

## Preregistration and exact evidence

- [Protocol frozen before A3 model data](BPB3_ADAPTIVE_A3_SMOLLM2_PREREG_20261112.md), immutable preregistration commit `04eeb24adcd640683029288301354b9b133999bc`.
- Model: [HuggingFaceTB/SmolLM2-135M](https://huggingface.co/HuggingFaceTB/SmolLM2-135M), Apache-2.0, Llama-family, original BF16 trained parameters; structurally independent from the prior Pythia-14M checkpoint. HF repo revision `93efa2f097d58c2a74874c7e644dbc9b0cee75a2`; model.safetensors verified SHA256 `80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`.
- Four frozen trained tensors: layer 2 attention o_proj and q_proj, layer 3 MLP up_proj and down_proj. All four are BF16 and were widened exactly to f32 in a non-pickle, mmap-based reader. Per matrix: **24 disjoint DEV + 24 disjoint TEST nonoverlapping source blocks of 256**, with selected indices and source block SHA retained.
- Original source is the **same** across all codec arms, and each of the **16 bipolar input masks per 256-weight row** is shared across every codec; the Boolean integer dot is compared exactly against a second dense signed-integer oracle before using any dot RMSE. Official unmodified ggml Q3_K/Q4_K encoder at `f7591a6a01f9273de03331732bdc0b39721168be`, with separate Rust/C reconstruction comparison.
- Successful research workflow [run 43258369828](https://github.com/Memorithm/SML-GENIUS/actions/runs/43258369828); exact collected [artifact 13962741314](https://github.com/Memorithm/SML-GENIUS/actions/runs/43258369828/artifacts/13962741314), 20 Rust tests passed and zero failed. Raw A3 CSV SHA256 `7985cd922c5b2e9e25050857cc1ca237c68f4835178a570cd74cca97b59c8db5`. The first attempt correctly failed closed on a BF16 mmap lifetime bug, then the fixed code reran successfully; the initial failure is not a source-weight quality result.
- Tests are **component** checks; no full trained model quantized or served. GitHub-hosted CPU only; Dell/Thor/Mac not used.

## Fixed source-byte budget

For every tensor and each split: 96 packed 64-weight groups = 6,144 coefficients. Tagged fixed BPB-3 mode 2: **2,592 bytes** (108 B per 256, 3.375 b/w); BPB-3-A1 adaptive mode 1/2/3: **also exactly 2,592 bytes** (same 108 B per 256, all tags and scales counted). Native official Q3_K: 2,640 bytes (110 B/256, 3.4375 b/w). Native official Q4_K: 3,456 bytes (144 B/256, 4.5 b/w). Historical untagged BPB-3/v0 at 104 B/256 is **not** used as an equal-budget control.

## TEST — all four frozen strata

| Matrix | Fixed weight RMSE | Adaptive weight RMSE | Official Q3_K weight RMSE | Official Q4_K weight RMSE | Fixed dot RMSE | Adaptive dot RMSE | Q3_K dot RMSE | Q4_K dot RMSE | Adaptive mode 1/2/3 counts |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| attention_output_layer2 | 0.032825364096 | 0.030198018831 | **0.020995879756** | 0.009781449014 | 0.594458434652 | 0.524483889074 | **0.294306562749** | 0.150535697780 | 10/76/10 |
| attention_q_layer2 | 0.058036505066 | 0.051515236727 | **0.038067266318** | 0.017759881521 | 0.865167518113 | 0.799068422360 | **0.598936183122** | 0.288638825572 | 22/52/22 |
| mlp_up_layer3 | 0.040515964418 | 0.039807547088 | **0.027578252383** | 0.013035546461 | 0.621483975985 | 0.619006869390 | **0.452775119010** | 0.200981457957 | 2/92/2 |
| mlp_down_layer3 | 0.040406292619 | 0.040406292619 | **0.027507067474** | 0.013181461668 | 0.646970999167 | 0.646970999167 | **0.456140628838** | 0.218729226184 | 0/96/0 |

All four TEST matrices were retained even when the adaptive optimizer selected only mode 2. The identical mlp_down reconstruction is a **valid no-improvement result**, not a pipeline failure. This study does not justify per-matrix retuning after TEST.

## Pooled RMSE with actual matched sample counts

Pooling means `sqrt(sum(n_i * RMSE_i^2) / sum(n_i))` and **not** the unweighted arithmetic mean of per-matrix RMSE.

| Codec | Pooled DEV weight RMSE | Pooled TEST weight RMSE | Pooled TEST signed dot RMSE |
|---|---:|---:|---:|
| BPB-3 fixed tagged | 0.047884672344 | 0.043931629563 | 0.690418255678 |
| BPB-3-A1 adaptive | 0.045047500313 | **0.041179634665** | **0.654853928679** |
| Official Q3_K | **0.030160389025** | **0.029185366612** | **0.463252222337** |
| Official Q4_K | 0.014444350769 | 0.013736507377 | 0.220345971578 |

Four TEST matrices × 6,144 coefficients = **24,576 weights**; four × (24 rows × 16 masks) = **1,536 truly matched signed dots per codec**. The adaptive scheme lowers TEST pooled weight RMSE by **6.26%** and matched-dot RMSE by **5.15%** versus tagged fixed BPB-3 at exactly 108 B/256. However its pooled weight RMSE is **41.10% higher**, and paired dot RMSE **41.36% higher**, than native Q3_K, which uses 110 B/256.

For DEV, the `mlp_up_layer3` adaptive **dot** RMSE (0.635374970230) is slightly **worse** than fixed (0.628277946228), despite its lower weight SSE. This shows why minimizing weight MSE does not guarantee lower error for every finite dot sample. No TEST metric or tensor has been removed based on outcome.

## Decision and next hypothesis

**A3 result: component-level independent-trained-model replication PASS; competitive advantage against Q3_K FAIL.** Exact Boolean codec arithmetic and global budget optimizer are qualified, and two separate trained architectures show fixed-budget reconstruction gains vs tagged fixed BPB-3. Native Q3_K's smaller error remains an explicit negative result on both. No full-model perplexity, generated-token quality, native inference throughput, RSS, DRAM traffic, energy, cross-device isolation or Posit Standard (2022) equivalence has been validated.

Next preregistered design hypothesis (not an observed result): introduce *local* 16-coefficient dyadic subscales (fully cost their metadata) while reusing Boolean bitplanes. Compare at a truly equal or explicitly costed model storage budget, independent second holdout, and model-level perplexity; do not tune to these revealed TEST strata. The shared-scale granularity is a plausible driver of the remaining Q3_K gap, **not a proven causal attribution**.
