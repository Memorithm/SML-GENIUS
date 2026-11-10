# BPB3-K AVX2 same-activation multirow tile (MROW-1)

Status: research-only / no inference runtime mode change / no model or Dell physical speed claim.

## Source and distinction from earlier tiled kernels

Source: research/boolean-posit/bpb3k_batch4_avx2.rs.
Reference: unmodified optimized ggml_vec_dot_q3_K_q8_K in
[llama.cpp f7591a6](https://github.com/ggml-org/llama.cpp/tree/f7591a6a01f9273de03331732bdc0b39721168be).

Existing bpb3k_cpu_kernels.rs has a 4-row generic grouped loop,
but the production-compatible fused_v2_stock path calls stock_vector_row
or stock_tiled_row for individual lines. Neither is the **new
direct fused AVX2 same-activation microtile**.

The MROW-1 candidate computes 1/2/4 rows of original 110-byte BPB3-K
weights against the same signed 292-byte Q8_K activation superblock.
Within each of eight 32-element pairs, the kernel loads the Q8 vector
ONCE and computes the sign-offset 4 × Q8 pair vector ONCE. Both
YMM values are reused across weight rows before advancing the
activation pointer; each weight still has its own 6-bit scale and
integer YMM accumulator. Three bitplanes per row are expanded from
contiguous 32-bit fragments with VPBROADCASTD and VPSHUFB.
No dynamic allocation per dot; no Q8_K cached-bsum contract;
no padding or larger weight format; no AVX512 or GPU SDK.

**Limit:** N=1/2/4 are supported. Eight rows would be 2 × four-row
tiles, not an eight-row single-register fusion. N=3 and N=8 return a
typed error. The runtime ggml nrc=4 ABI is a 4x4 weight×activation
tile and stock additionally selects a different C++ tiled path for
large batch sizes. MROW-1 is NOT a substitute for those outputs;
no FP32 multiblock or model-level fidelity claim is made.

## Independent evidence and controls, frozen before timing

1. 96 × 16 generated original Q3_K weight cases, 4 rows per case,
   256 signed Q8 codes/exact extremal patterns. Compare each integer
   output with an independent original Q3_K wire-field oracle and
   existing separately called BPB3-K PSHUFB Rust reference. Raw f16/f32
   scale metadata bits, including nonfinite patterns, must survive.
   Check invalid sizes and all 32 unaligned pointer offsets, with
   input sentinels unchanged.
2. Fetch pinned original optimized Q3_K C and C++ tiled oracle, run
   source-integrity checks 896 AVX2 + 112 tiled cases, then verify
   1,024 original-to-BPB3-K single-superblock results against the
   native C output bit-for-bit with both FP16/f32 scales exactly 1.
   Weights conversion to BPB3-K is performed once OUTSIDE timings.
3. Compare three methods in exactly one hosted process: (a) original
   stock Q3_K C once PER output row with the same Q8 input, (b)
   separately called Rust PSHUFB once PER output row, (c) MROW-1
   shared-Q8 tile N=1,2,4. For 32/256/1024 rows, 3 warmups,
   18 fully balanced 3-way order rounds, 4 repeats per invocation.
   Retain all 162 raw 3-way samples and descriptive medians in ns/row.
   There is no favorable speed threshold in CI.
4. Compile Haswell AVX2 Rust 1.89 and audit output assembly for
   VPSHUFB, VPMADDUBSW, VPMADDWD and VPBROADCASTD.
   Do not infer that the compiler actually hoists loads for all rows
   merely from the source-level loop; inspect the generated machine
   code and profile the Dell before an empirical reuse assertion.
5. No 4x4 ggml tiled comparison, multi-superblock FP32 fidelity,
   end-to-end SmolLM2 speed, NUMA or counter performance claim until
   separately qualified. The real-model stock default is untouched.

## Follow-up experiments

A separate preencoded-scale experiment should compare a 32-byte
unpacked scale layout (20 bytes overhead per 256 weights compared with
the 12 packed scale bytes) against the same batch2/batch4 kernel.
Both a one-time amortized model-load conversion and additional
cache/DRAM traffic must be included in physical tests, with frozen
pinning and no unqualified generalization.

RemoteOps must own Dell host admission, existing workload controls,
frequency/topology/NUMA and perf capability checks. SML retains the
scientific baseline and model results.
