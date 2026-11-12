# BPB-3 — scale-metadata compression algorithm decisions (2026-11-12)

Status: **research design**, not an observed model inference improvement. All comparisons assume 64 weights divided into four contiguous 16-weight groups and local dyadic scales k ∈ [−24,27] (52 admissible exponent values). **Sizes below cover exponent metadata only**, excluding the separately accounted precision-selector byte, sign plane and magnitude bitplanes unless stated.

## Verified baseline and prior outcome

A1 64-lane shared scale uses a canonical 2-byte scale field; full tagged A1 mode-m wire: 11 + 8m bytes. A4/A5 replace the shared scale with four independent signed one-byte exponents: 4 scale bytes, 13 + 2 Σmode bytes per 64. Fully matched 108B/256 TEST across pretrained SmolLM2 layers6/7 and10/11 found A4/A5 **less accurate** than A1, confirming that extra scale flexibility need not compensate for the lost coefficient bitplanes at fixed budget.

A6 scales have **one biased 1-byte base/variant symbol and packed four deltas**, either 2-bit fields (1 additional byte; **2 exponent bytes per64** total) or signed4bit fields (2 additional bytes; **3 exponent bytes per64** total). Its wire is 11 + 2 Σmode (delta2) or 12 + 2 Σmode (delta4) bytes. A6 is implemented and unit-tested, and a preregistered held-out run on new layer18/19 original weights found delta2 **10.03% lower source RMSE** than A1 at identical 108B/256; the 4-bit arm was worse than delta2. [A6 results](BPB3_A6_DELTA_SCALES_RESULTS_20261112.md). The mixed arm selected delta2 in every tested block; this is a sample result, not an entropy theorem.

## Ranked candidates and exact storage arithmetic

| Candidate | Exponent metadata for 64 weights | Conditions and trade-off | Status |
|---|---:|---|---|
| A6 base + four 2-bit signed offsets | **2 bytes** | Each k_i−base ∈ [−1,+2]; 64-block random access and two simple byte loads | **Implemented, positive vs A1 on frozen TEST** |
| A6 base + four signed 4-bit offsets | **3 bytes** | Each k_i−base ∈ [−8,+7]; wider local range but one byte per64 displaced from coefficient planes vs delta2 | Implemented; **loses to delta2** on frozen TEST |
| **Four absolute 6-bit exponent indices** | **3 bytes** | Four independent codewords k_i+24 in [0,51], reserved 52..63 must reject; exact 24-bit packing, no common-base restriction | **Next prototype:** full independent-scale fidelity at same scale-byte count as A6 delta4 |
| A5 four independent signed 8-bit exponents | 4 bytes | No offset constraint, one redundant precision bit per exponent vs 52-value range | Validated but worse than A1 |
| One shared exponent across all 64, A1 | 2 bytes | Only one exponent / all 64, but more complete bitplane capacity | Validated control |
| One shared **256-weight** base + sixteen 2-bit deltas | **5 bytes per256** (1+16×2/8), vs A6 four blocks × 2 = **8 B/256** | Savings 3 bytes per256; sixteen exponents must fit near one base, may increase source SSE | Prospective, not evaluated |
| One shared 256-weight base + sixteen 4-bit deltas | **9 bytes per256** (1+16×4/8), vs A6 four blocks × 3 = **12 B/256** | Saves 3B/256 if a single base fits; random access requires superblock boundary | Prospective, not evaluated |
| Empirical Huffman, Golomb-Rice, RLE or rANS on exponent symbols | **Unknown** | Net bytes must include coding table/escape/run/index and random-access costs; model/column-level codebook may be required | No admissible savings claim without fresh DEV histogram |

**Why absolute 6-bit coding is a stronger control than A6 delta4:** the four independent 6-bit codewords can reproduce every four-exponent combination admitted by the signed4-bit deltas at the **same 3 exponent bytes per64**, and all 52 possible exponents are directly representable. At the same coefficient bitplanes and exact storage budget, an *exact* source-SSE optimizer over absolute6 candidates therefore cannot produce higher optimal SSE than delta4. This is mathematical **representational inclusion**, not a claim against 2-bit deltas or Q3_K.

Why this is **not** the same as GGML Q3_K: Q3_K's official representation uses 6-bit *quantized subgroup scale factors* multiplied by a separate shared FP16 superblock scale; BPB uses signed dyadic exponents (power-of-two scales) and Boolean bitplanes. A 6-bit exponent code here does not create a 6-bit arbitrary linear scale. Official specification and block sizes: [llama.cpp tensor encoding schemes](https://github.com/ggml-org/llama.cpp/wiki/Tensor-Encoding-Schemes), [ggml-common.h](https://github.com/ggml-org/llama.cpp/blob/master/ggml/src/ggml-common.h).

## Research gates before costly implementation

1. **Prototype absolute6 first**, with standalone Rust canonical packing, reserved-code rejection, exact Boolean signed-dot oracle and DP comparison. A7 must compare both full-cost variants with identical original sources, not against A6 using different TEST.
2. For 256-weight shared bases, freeze full format, new four matrices and source split **before inspecting TEST**. Measure whether a common base can cover 16 group exponents with two/four-bit offsets, and whether 3 bytes saved/256 are actually reinvested in useful coefficient planes.
3. Compute DEV-only original exponent-difference histogram and empirical bit length **including headers**, fixed per-model codebooks and bitstream seek indexes before testing Golomb-Rice/Huffman/rANS. RLE only if verified run structure offsets justify it. No external pretrained arithmetic/rANS service on the decoding path without measured CPU cost.
4. Admission requires *source-matched* RMSE and paired signed-dot RMSE against A1, A6 and official Q3_K at fully charged 108B vs110B/256, followed by independently pinned second-model heldout perplexity, token behavior and CPU/DRAM benchmarks via Memorithm/RemoteOps on admitted isolated hosts. Do not promote a research format to production based on source RMSE alone.

No NVIDIA dependency, no speculative hardware load, no change to productive model selection.
