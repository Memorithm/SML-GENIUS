# BPB3-K Q5-178: deterministic source-precision FP16/BF16/FP32 quantizer

Status: **research implementation only**. This is a new signed five-bit quantizer from *source-precision weights*, not another conversion from the already-quantized Q3_K model. See `bpb3k_q5_source_quantizer.rs`, `bpb3k_q5_178_tiled_avx2.rs`, and the underlying 178-byte contract `BPB3_K_Q5_178_TILED.md`.

## Implemented algorithm

- Accept exactly 256 source weights, as `[f32; 256]`, `[u16; 256]` FP16 or `[u16; 256]` BF16. Decode FP16 including subnormals and signed zero; reject nonfinite input.
- Choose a global positive FP16 scale as a power of two, the *smallest representable power of two* satisfying `max(abs(weights)) <= 15 * 127 * global`, with exact zero-block handling; reject over-range values instead of silently clipping an entire block.
- For each 16-element subgroup independently, enumerate every signed i8 scale value `-128..=127`. For each candidate, divide each source coefficient by `global * signed_scale`, round to nearest integer and clamp to signed Q5 `[-16, 15]`. Evaluate full squared error against the **original FP32 values** in f64 and retain the minimizing scale and 16 codes. The zero scale maps all codes to zero.
- Encode as 128 low-nibble bytes, 32 fifth-bit bytes, 16 subgroup i8 scales and original FP16 global bits. Metadata semantics and SIMD calculation follow the separate Q5-178 tiled kernel.
- Provide a separately optimized same-scale Q4 integer oracle (signed `[-8,7]`, same subgroup scale search), **not** the deployed BPB3-K Q4-146 surrogate. Its coefficient set is contained in Q5; the exact discrete Q5 squared-error optimum cannot be worse at the same global scale. This does not guarantee better model perplexity, model accuracy or speed.

## Tensor-level packing

The Rust API now packs whole FP32, FP16 and BF16 tensors with lengths divisible by 256 into consecutive 178-byte blocks using `quantize_f32_tensor`, `quantize_fp16_tensor`, and `quantize_bf16_tensor`. Output allocation is checked, each block is independently quantized, and source nonfinite errors report the absolute tensor coefficient index. A failed block produces no publicly returned partial payload. The returned `QuantizedTensor` records the exact block count and source-level squared error. Its byte stream is **not standard GGUF**; no model runtime has been adapted to consume it.

## Evidence boundaries

The scalar integer dot and AVX2 tiled 1/2/4-row kernels are available from the parent PR and tested on deterministic vectors. This source quantizer adds actual five-bit coefficients beyond Q3_K's original `[-4,3]`, including FP16/BF16 raw source conversions and deterministic, per-block MSE metrics.

**Not implemented in this PR**: a complete GGUF v3 converter that discovers model tensor types, rewrites metadata and aligned offsets, and supplies a matching runtime loader, an end-to-end llama.cpp Q5 runtime path, a calibrated real-model evaluation, a native Q5_K model baseline, physical Dell throughput, or production deployment. Existing Q3_K and Q4-146 pathways are not altered.

## Qualification

- Rust 1.89 hosted x86-64: source-precision exactness, nonfinite/range rejection, fixed-global reference, repeated determinism, integer SIMD dot parity, signed Q5 versus same-global Q4 SSE on seeded source blocks.
- Future model gate: identical FP16/BF16 reference weights for all formats, preserve calibration and tokenization parity, compare held-out full-vocabulary logits, generated tokens, prefill and generation throughput, RSS and conversion cost.
- Future Dell experiments require RemoteOps admission, isolation and exact-head code pinning.

To rerun the small standalone test suite locally:

```bash
rustc --edition 2021 -D warnings --test research/boolean-posit/bpb3k_q5_source_quantizer.rs -o /tmp/bpb3k-q5-source-tests
/tmp/bpb3k-q5-source-tests --nocapture
```
