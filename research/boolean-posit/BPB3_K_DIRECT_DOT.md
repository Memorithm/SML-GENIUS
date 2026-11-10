# BPB3-K DIRECT — bitplane popcount dot products (bipolar and Q8_K)

**Status:** research-only CPU scalar/word-parallel candidates (DIRECT-1 bipolar,
DIRECT-Q8 general signed activations). Neither is a qualified physical CPU
speedup, GPU kernel, native model integration, or FLAT-ATTENTION path.

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


## DIRECT-Q8 — signed 8-bit activation generalization (2026-11-10)

The newly implemented **dot_bpb3k_q8** supports the **full signed Q8_K
activation domain -128..127**, rather than only bipolar inputs. Its inputs
are one exact 110-byte BPB3-K block and one exact 292-byte Q8_K block
(4-byte activation f32 scale, 256 signed i8 codes, 32 bytes cached bsums).
Both scale fields are preserved **as raw bits**, and the output is the exact
integer product before floating-point scaling. The cached Q8_K bsums are
not needed and are not read by the Boolean path.

For each weight subgroup of 16, let S be the BPB3-K negative plane,
B0 and B1 its magnitude planes, and A_k the 16-bit mask of bit k
from the signed activation bytes (two's-complement). The signed Q3_K
coefficient remains

    q_i = (1 - 2*S_i) * (B0_i + 2*B1_i) - S_i

The exact contribution from activation bitplane A_k is:

    C_k = popcnt(B0 & A_k) - 2*popcnt(S & B0 & A_k)
        + 2*popcnt(B1 & A_k) - 4*popcnt(S & B1 & A_k)
        - popcnt(S & A_k)

All signed activations are thus represented with:

    subgroup_dot = sum(k=0..6, C_k * 2^k) - C_7 * 128

Finally, the subgroup dot is multiplied by its original signed six-bit
Q3_K scale, and 16 subgroup results are added into one i32 result.
The safe upper bound is 256 * 4 * 32 * 128 = 4,194,304. This replaces
256 coefficient-by-activation scalar multiplications with bitplane
transposition, masks, shifts and population counts. **Sixteen scale
multiplications per superblock remain**; do not describe it as a
zero-multiplication algorithm. The prototype performs no heap allocation
and does not form a dense 256-element weight buffer.

Its scalar test oracle indexes coefficients from the **original
untranscoded Q3_K bytes** and multiplies every one of the 256 signed
activation codes independently; it does not call the Boolean dot's weight
decoder. Further checks cover input length rejection, signed extremes,
negative Q3_K offsets, FP16 and FP32 raw metadata (including infinities
and NaN payloads), and agreement with the existing bipolar-only kernel
when all activation codes are ±1.

### Admission boundary

This is a new, **research-only**, integer-domain building block. Q3_K,
BPB3-K and their previous runtime modes are unchanged. In particular, a
mathematically exact integer dot does not certify matching the optimized
stock ggml floating-point output: the f32 multiplication and accumulation
order differs between stock AVX2, tiled and generic implementations.
Separate independent stock-vector/tiled oracles, full-model logit/token
parity and trusted RemoteOps physical measurements are mandatory before
using this code as a runtime path or claiming a speed or DRAM advantage.
The additional Boolean operations may outweigh saved scalar products;
**no improvement is presumed**. Existing stock-ordered
fused_v2_stock remains the qualified experimental model candidate,
and the BPB3-K storage budget is unchanged.


## DIRECT-Q8-PREP — prepared activation bitplanes (2026-11-10)

A distinct, research-only candidate now exposes
**prepare_q8_bitplanes(q8)** and **dot_bpb3k_q8_prepared(weights, prepared)**.
It transposes the exact 256 signed Q8_K codes **once per activation
block**, producing 16 subgroups × eight 16-bit masks (256 bytes), plus
four bytes of unchanged f32 activation-scale bits. The original 292-byte
Q8_K source remains unchanged. Prepacked data is **additional temporary
state**, not a compression or reduction of source DRAM bytes.

The signed Q3_K weight coefficient q in [-4,+3] also has an equivalent
three-bit two's-complement identity:

    q = (bit0 XOR sign) + 2*(bit1 XOR sign) - 4*sign

The prepared path computes a signed Q8 integer subgroup dot with **three
popcounts per activation bitplane**, rather than five. Its 16 signed
Q3_K subscales are still multiplied in integer arithmetic. It preserves
the precise full signed Q8_K domain and the raw FP16 and f32 scale bits.
No dense weight tensor is materialized; no heap allocation occurs in
the prepare/dot APIs. Both original scalar and prepared APIs remain
independently addressable. Neither changes the model runtime.

Independent correctness tests compare prepared results, the prior
unprepared Q8 Boolean result, and an oracle reading the **original**
(untranscoded) Q3_K wire bits for 96 deterministic weight fixtures × 16
activation patterns, plus all 256 signed byte codes over 256 distinct
weight rows, extreme [-4,+3] Q3 and [-128,+127] Q8 values,
malformed input lengths, and FP16/FP32 metadata preservation.

### Diagnostic performance protocol (frozen before observing results)

The standalone **bpb3k_prepared_bench.rs**, executed by the dedicated
hosted CI workflow, measures **two implementations of the same integer
operation**: legacy unprepared Q8 (transpose for every weight row) versus
the new prepared path (transpose once per activation batch).
It generates deterministic source Q3_K weight blocks and losslessly
transcodes them. It runs matched workloads with 1, 32, 256 and 1024
weight rows, four repetitions per invocation, three warmup passes, and
16 paired AB/BA alternate-order rounds per row count. Preparation
overhead is **included in each candidate timed invocation**. It records
all paired raw nanosecond durations, checks exact checksums, and reports
only descriptive median ns/dot and the legacy/prepared ratio.

A figure above 1.0 indicates a faster prepared candidate **only for this
standalone synthetic hosted-runner workload**. There is **no hardcoded
speed threshold**, no retrospective exclusion of slow timings, and no
substitution of this benchmark for the full SmolLM2 optimized Q3_K stock
comparison. SIMD/AVX2 effects and source reads are not isolated here.
CI validates compilation, correctness, and bounded execution; a
numerical or runtime performance success does not promote the kernel
to production or claim a physical Dell speedup.

A subsequent model experiment must verify exact source/weight identity,
matching stock AVX2/tiled floating-point reductions, all logits and
tokens, real allocation and activation reuse inside ggml, and a
prospectively admitted **RemoteOps** paired physical test. The previous
physical v2 campaigns remain category E due to third-party CPU
contamination; their thresholds and original records are unchanged.


## Independent optimized Q3_K stock comparator (preregistered before first run)

The follow-up **bpb3k_prepared_stock_bench.rs** compares the prepared
signed-Q8 Boolean dot with **the unchanged optimized AVX2 C
ggml_vec_dot_q3_K_q8_K function** extracted from upstream llama.cpp
commit f7591a6a01f9273de03331732bdc0b39721168be. The pinned
original C source is checked against git HEAD before it is compiled
by the existing independent stock-oracle script. That script must
first pass its own unchanged-source C/Rust oracle fixtures.

For exact arithmetic comparability, each dot uses **one 256-weight
Q3_K block and one 256-code Q8_K block**, with both global scales
exactly 1.0. One-block original stock AVX2 FMA accumulation then
produces a precisely representable integer f32; the benchmark
asserts that this integer and the Boolean candidate agree across
all 1024 original Q3_K → BPB3-K losslessly transcoded rows **before
timing**. No floating-point tolerance is relaxed.

After the correctness preflight it measures 1, 32, 256, 1024 rows;
4 repetitions per invocation; 3 warmups; 16 paired AB/BA rounds.
The prepared candidate's **one transposition per batch is inside
the measured interval**. Every raw duration and exact checksum
is retained. The median stock/candidate nanoseconds-per-dot
ratio is descriptive only. A ratio greater than 1 means prepared
won this exact **standalone synthetic** comparison, not that the
SmolLM2 model outran its optimized Q3_K baseline.

The experiment excludes real full-model operator scheduling,
the >=8-token tiled Q3_K branch, activation quantizer overhead,
cache/NUMA ownership, host-contamination admission, tokens/s and
DRAM traffic. Its C code is a diagnostic oracle, **not a dependency
of the SML inference runtime**. A physical Dell measurement must
still be performed solely through RemoteOps after merged-source
qualification and with a complete frozen protocol. Source,
toolchain, ISA support, output identity, warmup/order, raw
measurements and any failed preconditions must remain auditable.
