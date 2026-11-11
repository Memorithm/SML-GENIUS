# Experimental BPB3-K Q5-178 tiled AVX2 (2026-11-12)

Status: **code implemented; exact-head CI and physical model qualification pending**. Research-only, not a production quantization format. Source: `bpb3k_q5_178_tiled_avx2.rs`. This experiment must not replace the working Q4-146 tiled or unmodified Q3_K path.

## Exact layout

A 178-byte block represents 256 integer coefficients and consists of:

| Byte interval | Length | Meaning |
| --- | ---: | --- |
| 0..128 | 128 | Low four bits in 8 groups of 32; each 16-byte group packs low nibbles for the first 16 coefficients and high nibbles for the second 16 |
| 128..160 | 32 | Fifth bit, coefficient `i` in bit `i % 8` of byte `128 + i / 8` |
| 160..176 | 16 | Signed i8 subgroup scales, one per 16 coefficients |
| 176..178 | 2 | Original little-endian raw FP16 global-scale bits |

Let `u_i` be the unsigned five-bit code, in `[0,31]`. The signed coefficient is `q_i = u_i - 16` and the represented weight is `q_i * scale[i / 16] * fp16(global)`. The experimental signed Q8_K dot uses `Q8[i] = (input[4+i] as i8)`; its own floating scale is stored verbatim at bytes `0..4`. Q8_K block length is 292 bytes. **This layout is not standard GGUF Q5_K** (the latter has a different contract and length).

The gross storage cost is `178 / 256 = 5.5625` bits per weight including metadata, versus `146 / 256 = 4.5625` for Q4-146 and `110 / 256 = 3.4375` for Q3_K. Q5-178 requires 32 additional bytes, **+21.9178%** versus Q4-146.

## Implemented

- `pack_signed`: checks every signed code `[-16,15]`, packs all five bits, preserves 16 signed scales and raw FP16 global bits. It does **not** implement a five-bit floating-point quantizer.
- `transcode_q3` and `transcode_q4`: losslessly re-encode Q3_K/Q4-146 coefficients in Q5-178 using the existing Q4 oracle. Re-encoding already-quantized weights **cannot improve the underlying model precision**.
- `scalar_lanes`: independent signed Q5 × signed Q8_K integer oracle and eight-lane reduction order.
- `dot_tiled::<1/2/4>`: native Rust AVX2. For each 32-element group, load a Q8 vector once and compute the bias-16 correction once; decode each row's nibbles and high bit using AVX2 `VPSHUFB`, then multiply/accumulate into independent vector registers. No full 256-weight dense materialization, no assumed alignment, no allocations inside the microtile, no fallback on unsupported CPU.
- Deterministic tests: all 32 code values; adversarial `-128/127` Q8 and scale values; FP16/f32 bit preservation; lossless original-Q3 oracle; SIMD/scalar lane parity for 1, 2 and 4 rows; unaligned buffers and malformed inputs.

## Unimplemented / blocked on later qualification

1. A **real** five-bit re-quantizer from sufficiently precise FP16/BF16 source tensors, with specified calibration, error minimization and held-out quality targets. Re-encoding Q3_K is not evidence of Q5 quality.
2. Hooking this independent Rust tiled microkernel into a source-pinned, isolated ggml whole-model runtime, including a distinct nonstandard GGUF representation and strict source checks. **The current PR does not execute SmolLM2 through Q5-178**.
3. Reproducible native Q5_K and Q4-146 tiled complete-model baselines on identical weights, quantization calibration, vocabulary logits, selected tokens, prompt processing, generation, RSS and end-to-end conversion/load cost.
4. Dell Xeon E5 v4 physical throughput, cache/NUMA/CPU counters and contamination controls through **Memorithm/RemoteOps only**.

Any speed/quality ranking requires matched same-host, same-model, same-condition evidence with retained failures and exact-head CI. Never claim production readiness or CPU/model speedup from pure integer-parity tests.

## Standalone qualification

```bash
rustc --version
rustc --edition 2021 -D warnings --test research/boolean-posit/bpb3k_q5_178_tiled_avx2.rs -o /tmp/bpb3k-q5-178-tests
/tmp/bpb3k-q5-178-tests --nocapture
```

The dedicated GitHub Actions workflow performs these commands with Rust 1.89 on a hosted x86-64 CPU. A physical Dell campaign must never run without RemoteOps workload admission.
