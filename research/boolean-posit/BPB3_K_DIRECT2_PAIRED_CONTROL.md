# BPB3-K DIRECT-2 — matched original-Q3_K wire control

**Status:** experimental correctness comparison. No hardware performance, general
SIMD, GPU, model or FLAT-ATTENTION result is claimed.

## What was implemented

- research/boolean-posit/q3k_direct.rs: a dependency-free, byte-addressed
  original Q3_K dot-product kernel for bipolar activations, using **all 110
  original Q3_K bytes directly**; no dense 256-value decompression.
- research/boolean-posit/bpb3k_direct.rs (DIRECT-1, PR #862): a separate
  direct dot product from the transcoded 110-byte BPB3-K bitplane layout.
- research/boolean-posit/bpb3k_q3k_paired.rs: an independent paired parity
  panel checking both direct methods against original GGUF Q3_K reconstruction.

The same 256 bipolar activations are encoded in four u64 masks for both
candidates, with bit 1 meaning -1. Both methods return a bounded exact
integer dot and preserve the original FP16 global scale bits without forcing
an FP32 reduction order. Both process the sixteen original signed 6-bit
subgroup scales independently. The storage budget is 110 B per 256 weights
in both representations; BPB3-K transcode is one-time preprocessing before
this comparison, not a zero-cost runtime operation.

## Independent qualification

1. 64 deterministic Q3_K wire fixtures times 64 bipolar input masks:
   **4,096 paired cases**. The codec bytes include all original Q3_K hmask,
   low codes, subgroup scales and global scale.
2. On finite, exactly representable power-of-two scale cases, compare
   the integer result times the scale against a scalar 256-term dot on
   gguf_k.rs-decoded **original** Q3_K bytes.
3. Sweep every signed 6-bit subgroup scale code (-32..31) under eight
   distinct bipolar masks, checking both direct paths and the decoder
   independently (512 additional matched cases).
4. Explicit -4 and +3 integer extremal fixtures, empty/oversize inputs,
   zero/negative-zero, subnormal and non-finite FP16 global-scale metadata.

Only a successfully completed, exact-head CI run permits the label
"correctness qualified". The controlled tests do not validate real model
activation distributions or the quantization quality of original weights.

## Scientific interpretation

The byte-addressed Q3_K implementation is a **correctness control**, not a
claim to beat or faithfully reproduce the speed of optimized ggml Q3_K
vectorized CPU kernels. An apparent BPB3-K speedup against this byte-loop
reference would **not** establish a competitive advantage against ggml
intrinsics, AVX2, NEON, tiled GPU or an optimized Q3_K SIMD baseline.

Conversely, the Q3_K format can also be processed with on-the-fly bitplane
extraction. A serious performance comparison must include that competent
baseline before concluding any architectural win.

## Prospectively frozen next hardware experiment (no results yet)

**Hypothesis:** with weights in persistent native representation and the same
bipolar inputs, direct BPB3-K POPCOUNT may lower time per complete block
relative to a byte-addressed original-Q3_K control. This is not assumed to
hold against optimized SIMD controls.

- Dataset: synthetic Q3_K bytes pinned by SHA and deterministic generator,
  processed as both exact 110-byte Q3_K wire and its 110-byte BPB3-K transcode.
  Fixed inputs, identical numerical results, same block order and same
  residency/NUMA placement. Report input metadata including 32 bytes of
  bipolar masks per block separately from the 110-byte weight payload.
- Core primary comparison: **separate steady-state kernel timing** with both
  forms pre-resident. Conversion cost is reported separately for a realistic
  loading scenario. Physical DRAM read-CAS, cache misses and CPU utilization
  must be captured with tool and topology metadata when a claim concerns
  physical traffic; measured read-CAS is not logical payload size.
- Controls: at least one independently optimized Q3_K competitor (upstream
  pinned ggml SIMD or a qualified direct equivalent), randomized/alternating
  order, same-arm sham pairs, explicit drift rejection before interpreting
  comparisons, NUMA pin/residency evidence, and exact oracle success for each
  scanned block.
- Measurement scope: process-level elapsed time versus isolated kernel time,
  CPU frequency, system load, memory topology, cache state and energy meter
  must be identified. Confidence intervals and all rejected sessions must be
  recorded, not silently discarded. No universal speedup inferred from one
  device or synthetic benchmark.
- Physical workload execution is authorized only through Memorithm/RemoteOps
  after host admission. Do not impact active Dell/Thor/Mac workloads.
- No physical experiment is performed as part of this code-only PR.

## Reproduction (GitHub-hosted CI)

The required main CI fmt job compiles and runs:

    rustfmt --edition 2021 --check research/boolean-posit/q3k_direct.rs research/boolean-posit/bpb3k_q3k_paired.rs
    rustc --edition=2021 -D warnings --test research/boolean-posit/bpb3k_q3k_paired.rs -o /tmp/bpb3k-q3k-paired-tests
    /tmp/bpb3k-q3k-paired-tests
    rustc --edition=2021 -D warnings -O --crate-type lib research/boolean-posit/q3k_direct.rs -o /tmp/libq3k_direct.rlib

Source upstream reference: ggml-org/llama.cpp at
f7591a6a01f9273de03331732bdc0b39721168be;
prior transcode specification: BPB3_K_EQUIVALENCE.md.
