# BPB3-K DIRECT-1 — direct Boolean bipolar dot-product candidate

**Status:** research-only CPU scalar/word-parallel candidate. This is not a qualified
physical CPU speedup, GPU kernel, native model integration, or FLAT-ATTENTION path.

## Input and output contract

- Source: original ggml Q3_K block of **110 bytes / 256 weights**, transcoded
  without loss using bpb3k.rs into 96 bytes of bitplanes and 14 bytes of
  unchanged Q3_K subgroup/global scales.
- Input: four u64 words, each bit signaling a bipolar activation (-1 if set,
  +1 otherwise). This is an intentionally narrow workload; ordinary
  neural-network activations are not restricted to bipolar values.
- Output: DirectDot { weighted_integer: i32, global_scale_f16_bits: u16 }.
  Its exact mathematical value equals weighted_integer multiplied by the
  original FP16 global scale. We intentionally return the integer and scale
  **separately**, without inventing an FP32 accumulation/rounding policy.
- Non-finite global FP16 scales remain represented as raw bits; the caller
  must validate the numerical domain before using this result in a model.
- Any input whose byte length differs from exactly 110 is rejected.
- No heap allocation or 256-value dense weight buffer occurs in
  dot_bpb3k_bipolar. Popcount is applied directly to each 64-bit bitplane.

## Core formula

For each of 16 subgroups of 16 weights, let s_i be the sign bit, m_i the
two-bit magnitude (0..3), and x_i the bipolar activation. BPB3-K encodes:

    q_i = (1-2*s_i) * m_i - s_i
    m_i = bit0_i + 2*bit1_i

Let X_i = 1 when x_i=-1. Then E = !(sign XOR X), and for each subgroup
mask and amplitude plane M:

    C(M) = 2*popcount(E & M) - popcount(M)

The BPB3-K negative offset is necessary, unlike the BPB-3/v0 formula:

    correction = 2*popcount(sign & X) - popcount(sign)

The exact signed integer subgroup dot is

    D_j = C(bit0) + 2*C(bit1) + correction

using only subgroup-masked bitplanes. The final exact integer is

    D = sum_{j=0}^{15}(D_j * signed_q3_k_subgroup_scale_j)

The represented output is the pair (D, original_FP16_global_scale). This is
not a claim that term-by-term f32 accumulation yields identical final f32
bits: different permitted reduction orders may round differently.

## Independent oracle

The test process decodes **the original Q3_K wire bytes** with gguf_k.rs
(a distinct decoder already qualified against upstream gguf-py) and then
computes a dense 256-term scalar bipolar dot in f64. It compares this result
against the new popcount kernel for 36 deterministic byte fixtures and
32 activation masks each, using exactly representable binary-power-of-two
global scales. The test also checks sign-offset boundary (-4), sign-mask
balance, malformed lengths and raw FP16-bit preservation including
non-finite metadata. The existing BPB3-K transcode and decode tests are
included in the same standalone Rust test executable.

These tests establish reference arithmetic parity only for the declared
corpus and domain. They do not establish physical performance, novelty,
accuracy before quantization, or model generation quality.

## Reproduce without model installation

From the repository root on a machine with Rust toolchain 1.89.0:

    rustfmt --edition 2021 --check research/boolean-posit/bpb3k_direct.rs
    rustc --edition=2021 -D warnings --test research/boolean-posit/bpb3k_direct.rs -o /tmp/bpb3k-direct-tests
    /tmp/bpb3k-direct-tests
    rustc --edition=2021 -D warnings -O --crate-type lib research/boolean-posit/bpb3k_direct.rs -o /tmp/libbpb3k_direct.rlib

CI runs these steps via .github/workflows/bpb3k-direct-dot.yml.

## Next admission and comparators

A separate preregistered stage must implement and validate an **equivalent
direct Q3_K baseline** (no hidden dense intermediate) on the same fixed
bytes and activation masks. The following must be measured before making a
performance claim: kernel time, I/O bytes including all 110 bytes and input
masks, memory residency, physical DRAM reads under full NUMA accounting,
cache effects, execution ordering and confidence intervals. RemoteOps must
admit all self-hosted physical runs before allocation or execution.

Separately, compare with a competent SIMD/optimized implementation. The
current BPB3-K and Q3_K **storage footprints are identical**, so no payload
compression advantage is claimed. Follow-on non-bipolar activation and
real-model tests require a new contract and independent numerical oracle.
