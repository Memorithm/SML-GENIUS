# BPB-3-A6 — packed 2/4-bit exponent deltas: prospective TEST results

Date: 2026-11-12. Status: **research-only**. Exact bitplane/DP component validation PASS; original trained-weight reconstruction improves over A1; native GGML Q3_K remains superior. No production activation, model perplexity claim or standard posit compliance.

## Frozen scientific protocol, reproducibility, and motivation

- [Frozen A6 protocol](BPB3_A6_DELTA_SCALES_PROTOCOL_20261112.md) committed at `f408f0319982b5265d9978015e7887731d4e870b` **before reading untouched TEST source**. Two fixed-width delta codebooks and a per-block mixed selector were specified in advance. No model hyperparameters altered after TEST.
- Source: original BF16-trained weights in [HuggingFaceTB/SmolLM2-135M](https://huggingface.co/HuggingFaceTB/SmolLM2-135M), pretrained Apache-2.0 model, revision `93efa2f097d58c2a74874c7e644dbc9b0cee75a2`, exact `model.safetensors` SHA256 `80521b40281d6ce74e35c9282c22539e75aa0ac8578892b2a59955ef78d55da1`. Four new matrices: layer18 attention o_proj, layer18 attention q_proj, layer19 MLP up_proj, layer19 MLP down_proj. Unlike A3, A4 and A5, no layer2/3, 6/7 or 10/11 TEST matrices reused. New layers are **within the same architecture**, not an independently trained model.
- Each matrix: 24 nonoverlapping DEV + 24 independent TEST rows of 256 original values, frozen deterministic source selection. Full source-index/hash manifest retained, no original weights uploaded.
- Native reference: unchanged official GGML Q3_K and Q4_K, pinned at `f7591a6a01f9273de03331732bdc0b39721168be` with Rust/C decoder cross-check. **Same original FP32-widened source and exactly the same 16 signed bipolar input masks per source row** for A6 delta2/delta4/mixed, A5, A4, A1 fixed/adaptive and native K formats.
- [Initial successful A6 correctness and TEST run 43273317573](https://github.com/Memorithm/SML-GENIUS/actions/runs/43273317573); [CSV, hashes and source manifest](https://github.com/Memorithm/SML-GENIUS/actions/runs/43273317573/artifacts/13968403907). CSV SHA256 `4b9b59aadcc8837571be3ceedef62130a6f1de965967456be7587925c06e828e`; source manifest SHA256 `1ddfb01d3925fcbc617f444f6b0b2cdb541e1069f5a752397a28e1df2b89e8b5`.
- After that measurement, two **non-quantizer-changing** independent synthetic regression tests were added to assert wire compatibility and the A1 representational inclusion property. Exact-head hosted Rust 1.89 **20 tests passed, 0 failed** on the updated code. Full GitHub CI remains a separate merge gate.

## Format and compression algorithm

Every 64-coefficient block stores 4 × 2-bit independent magnitude precision fields, a 1-byte **base exponent + variant** symbol, 4 local exponent differences packed into 1 byte (2-bit codebook) or 2 bytes (4-bit codebook), a 64-bit sign plane and 2 bytes for each local magnitude bitplane. Explicit wire cost:

- `A6_delta2`: **11 + 2 × Σ(local_mode)** bytes, 19–35 per 64 coefficients. Offset code 0/1/2/3 means exponent delta −1/0/+1/+2. Every exponent in [−24,27]; reserved symbols rejected.
- `A6_delta4`: **12 + 2 × Σ(local_mode)** bytes, 20–36 per 64 coefficients. Signed nibble range −8..+7.
- `A6_mixed`: choose the two modes per 64-coefficient block, never exceeding the original source-split budget.
- **All three** use **108 bytes per 256 weights** = 3.375 bits/weight including scales and mode metadata. A1 tagged fixed/adaptive, A4, A5 also use 108 B/256; official Q3_K uses 110 B/256, Q4_K 144 B/256.

An exact local common-base/delta enumeration and nested global dynamic program select the best source-SSE configuration under the hard packed-byte ceiling. The unit test establishes a useful mathematical inclusion: **every A1 configuration is expressible by A6 delta2 at exactly the same bytes** (group modes equal and every delta zero), so an exact budgeted A6 delta2 source-SSE optimum cannot exceed A1's source SSE on the same admitted source. This is a *format/optimizer property*, not proof of improved language model quality or latency.

## Complete TEST matrices, 6,144 source weights each

| Original trained matrix | A6 delta2 weight RMSE | A1 adaptive weight RMSE | Official Q3_K weight RMSE | A6 delta2 dot RMSE | A1 adaptive dot RMSE |
|---|---:|---:|---:|---:|---:|
| attention_output_layer18 | **0.032263173290** | 0.035957881161 | 0.026688581594 | **0.503228380184** | 0.533184061692 |
| attention_q_layer18 | **0.035634861353** | 0.039967650502 | 0.030907667218 | **0.549213999617** | 0.630634404781 |
| mlp_up_layer19 | **0.041461087918** | 0.045621907533 | 0.030925043952 | **0.644762753803** | 0.733781165595 |
| mlp_down_layer19 | **0.038305770750** | 0.042651476980 | 0.029448102519 | **0.595179463385** | 0.663651986950 |

Every matrix includes 384 identical-mask bipolar dot samples, for 24,576 matched TEST weights and 1,536 dot samples per codec. All four strata and negative native comparisons are retained.

## Pooled TEST, weighted by actual sample count

Pooled RMSE computed from `sqrt(sum_i(n_i × RMSE_i²)/sum_i(n_i))`, **not** the unweighted mean of per-matrix RMSE.

| Codec | B/256 | Pooled weight RMSE | Pooled bipolar dot RMSE |
|---|---:|---:|---:|
| Tagged fixed A1 | 108 | 0.044689424820 | 0.720517383400 |
| **A1 adaptive** | **108** | **0.041203425949** | **0.644370575620** |
| A4 local16 full exponents | 108 | 0.046831618703 | 0.730700016747 |
| A5 mixed16 full exponents | 108 | 0.044388497875 | 0.679951236156 |
| **A6 delta2** | **108** | **0.037071247546** | **0.575506824965** |
| A6 delta4 | 108 | 0.040517098496 | 0.628098943436 |
| **A6 mixed** | **108** | **0.037071247546** | **0.575506824965** |
| Official Q3_K | 110 | **0.029542822164** | **0.472816826402** |
| Official Q4_K | 144 | 0.013862866167 | 0.218180751959 |

**A6 delta2 versus A1 adaptive** on the fresh TEST set: **−10.03% source weight RMSE** and **−10.69% matched bipolar dot RMSE** at identical storage. Versus A5: **−16.48% weight RMSE**. However A6 still has **+25.48% source RMSE** and **+21.72% dot RMSE** relative to official Q3_K at 2 bytes/256 lower storage.

The per-block mixed optimizer selected the two-bit delta variant for **all 384 TEST blocks across four matrices**, never the 4-bit variant: the wider delta range did not compensate for its extra byte per 64 on this holdout. This **does not** show the four-bit variant is universally useless.

## Decision and next research priorities

**Accept A6 delta2 as the best measured BPB-3 experimental source codec to date at the declared 108 B/256 in these held-out matrices**, and retain A1 as a proven fallback. **Reject a claim of competitive superiority to Q3_K**, or generality beyond the measured model/tensors. The A6 improvement is at component level; no full-model perplexity, token-level quality, kernel throughput, DRAM traffic, physical energy, GGUF production integration or Posit Standard-compliant floating number was measured.

The next distinct research decision should compare **superblock-shared scale metadata** (e.g. 256-coefficient base and quantized group-scale codebook) and real source **scale-delta histograms** before considering Rice, Huffman or rANS. Entropy methods may lose on tiny independent 4-scale blocks and may complicate random-access decoding; do not claim savings until code length, header cost and measured decode speed are included. Official GGML Q3_K 6-bit group scales under a 256-weight shared scale offer a concrete baseline, not a new invention.
