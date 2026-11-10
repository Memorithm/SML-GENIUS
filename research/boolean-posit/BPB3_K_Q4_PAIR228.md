# BPB3-K Q4 146/160 B vs paired 228 B — frozen experimental protocol

Status: research-only. No inference dispatch, GGUF type, or production
kernel change. Physical Dell operation must use RemoteOps only.

Original comparison: pinned unchanged optimized ggml Q3_K C AVX2
and tiled C++ source at upstream llama.cpp commit
f7591a6a01f9273de03331732bdc0b39721168be.

## Three distinct hypotheses

Q4-146: 256 signed three-bit Q3_K coefficients stored as 4-bit
unsigned-offset q+4 codes in **128 B nibble payload**. Preserve the
sixteen exact signed subgroup scales as sixteen i8 bytes and the
original raw FP16 global scale as two bytes: **146 B/256 weights**,
+32.73% relative to 110-byte native Q3_K. Two nibbles pack
low/high positions of each 32-code group, allowing an AVX2
broadcast, mask, shuffle and direct VPMADDUBSW/VPMADDWD.
No dense 256-code buffer inside the kernel. No model GGUF use.

Q4-160: exactly the same coefficient and signed-scale bytes, plus
**14 zero padding bytes** between scale bytes and the raw FP16,
giving a stride divisible by 32: **160 B/256 weights**,
+45.45% relative to native Q3_K. This does not guarantee actual
base pointer alignment. Testing it directly prevents speculative
claims that padding alone improves AVX2 performance.

PAIR228: concatenate TWO previously qualified, unchanged
114-byte BPB3-K blocks: **228 B/512 weights** (NOT 228 bytes
for only 256 weights). Both original 16 signed subgroup scales
and FP16 bits per original 256-code subblock remain. Fuse two
consecutive 256-code Q8_K products (292 B x2 activations)
in an eight-step SIMD loop with independent YMM accumulators.
Test the paired pipeline against real original Q3_K C n=512
and two separately called standard 114-byte BPB3-K kernels.
Do not mix per-512-block timing with per-256-block timing.

All formats are mathematically lossless. One-time conversion
is **outside steady-state timing**. Full-model memory bandwidth,
model-load conversion and cache/NUMA behavior remain unqualified.

## Frozen qualification protocol

1. An independently coded original Q3_K wire decoder checks
   signed integer results for 96 source seeds, extremes signed
   Q8 values, the complete 256 signed byte-code domain,
   raw FP16/f32 metadata patterns including nonfinite bits,
   malformed sizes, unaligned sources and immutability.
2. Preserve the original SHA-pinned upstream ggml C native
   AVX2 896-case oracle and tiled C++ 112-case oracle.
   With exact unit FP16/f32 scales, require bitwise stock
   FP32 parity over 1024 single-block Q3_K source rows
   against original C, previous 114, new Q4-146 and Q4-160;
   and over 512 **two-block** source pairs for PAIR228.
   No arbitrary-scale multiblock FP32-order claim.
3. On exactly one hosted x86-64 Haswell-targeted Rustc 1.89
   runner, original stock C vs prior 114B vs Q4-146 and
   Q4-160 for 32,256,1024 rows; 3 warmups, 24 balanced
   four-arm order rounds, 4 repeats per timing window.
   Retain **72 original four-arm raw records**.
4. Separately compare original stock C n=512, two separately
   called 114B dots, and paired228 two-block fused SIMD
   for 16,128,512 pairs; 3 warmups, 24 balanced three-arm
   order rounds and 4 repeats per measurement. Retain
   **72 original three-arm raw records**.
5. Inspect actual Haswell SIMD mnemonics and exclude AVX512
   popcount; no false instruction-port/spill inference.
   No speed threshold controls green CI. Avoid cherry picking.

A favorable arithmetic microbenchmark does not establish
model-level token speed, energy use or full-model FP32 fidelity.
No production promotion until independent model-level and
RemoteOps-admitted native Dell tests are fully qualified.

## Decision rules

Do not prefer the padded 160-byte form unless it actually
wins versus 146-byte on identical runs, including bandwidth.
Do not claim that 228 bytes increases density: it has twice
the weights of the 114-byte form and its memory traffic is
twice that of a single 114-byte block. Compare equal arithmetic
work and equal activation values on the native original Q3_K.
