# BPB3-K AVX2 Q8_K cached subgroup-sum correction

Status: **research-only**, SML-GENIUS model runtime untouched.
Reference: original ggml Q3_K x signed Q8_K AVX2 C from
[llama.cpp commit f7591a6](https://github.com/ggml-org/llama.cpp/tree/f7591a6a01f9273de03331732bdc0b39721168be),
source-integrity checked by the existing stock oracle extractor.

## Optimization hypothesis

The existing BPB3-K PSHUFB kernel maps each signed 3-bit coefficient
q from [-4,+3] to unsigned q+4 in [0,7] to use VPMADDUBSW on all
256 Q8 signed activations. It then subtracts the contribution from
+4 using **a second VPMADDUBSW and a VPSUBW for each of eight
32-element pairs**. That correction is redundant when the input
activation is a well-formed ggml Q8_K superblock. Q8_K already stores
the signed 16-activation subgroup sums as sixteen little-endian i16
values at offsets 260..291, computed during activation quantization.

For a subgroup with six-bit signed scale s and 16 Q8 signed codes
a_i, the exact correction is:

    sum_i [ (q_i + 4) * a_i * s ] - 4*s*sum_i a_i
       = sum_i q_i*a_i*s

The signed Q8_K sum is cached as bsums[subgroup]. Build sixteen
signed BPB3-K subgroup scales (32 bytes total stack scratch) while
decoding into AVX2 lanes. Apply eight VPMADDUBSW (one per pair)
and eight VPMADDWD for positive biased q coefficients; then load
both i16[16] vectors (scale and Q8_K bsums) and apply **one final
VPMADDWD**, a fourfold i32 shift and a vector subtract to correct
all sixteen groups at once.

No extra activation transposition/LUT or weight-buffer expansion:
weights remain exactly 110 bytes and activations exactly 292 bytes.
The source memory is never modified. The correction uses signed
six-bit scales, signed i16 Q8 sums, and exact i32 arithmetic. The
maximum weighted integer magnitude is <=4,194,304. No i16 saturating
VPMADDUBSW is possible since 2*7*128 < 32,768.

**Contract limitation**: cached Q8_K bsums must be consistent with the
256 signed activation codes. The previous PSHUFB kernel recomputes
the offset directly and can tolerate deliberately corrupted bsums;
this new one does not. Production ggml Q8_K blocks contain correct
bsums. An optional, separate Rust input-admission helper
q8_k_cached_sums_valid verifies all 16 group sums against payload.
Its O(256) validation cost is NOT inside the timed dot; if an API
requires adversarial/untrusted Q8 input, admission validation must
be performed and accounted for before promoting this kernel.
No general-purpose dot on arbitrary 292-byte blobs is asserted.

## Frozen experiment before observing data

1. Validate each signed integer against an *independent decoder of
   original Q3_K wire format*, 96 weight cases x 16 activation
   patterns, including q8=-128/+127, all 256 signed codes,
   varying raw f16/f32 scale bits including NaN patterns and exact
   boundary values. Verify previous PSHUFB parity.
2. Validate the Q8_K cache against the signed input at the admission
   gate. Introduce corrupt cached sums and corrupted activation to
   confirm that the admission gate rejects them, and reject malformed
   lengths before unsafe vector loads. Test all 32 pointer offsets
   without input mutation or overread.
3. On a hosted x86-64 runner, fetch the SHA-pinned unmodified ggml
   original AVX2 C and C++ tiled code, keep its independent 896 + 112
   oracle checks. With a complete set of signed Q8 activations and
   exactly 1.0 FP16 and FP32 scales, verify 1,024 one-superblock rows
   *bitwise* against the reference and previous PSHUFB candidate.
   Arbitrary-scale, multiblock FP32 and logits/token fidelity remain
   outside this test.
4. Compare original C AVX2, previous Rust PSHUFB AVX2 and the new
   Q8_K bsums AVX2 candidate **in one process**, exact same 1/32/256/
   1,024 row blocks, 3 warmups, 24 fully balanced ABC-order rounds
   per size with four repeats per timed window. Preserve all 96 raw
   timing triplets and descriptive medians; no selection of favorable
   runs. The one-time Q3_K->BPB3-K transcode is outside timing,
   exactly as in the earlier experimental comparisons.
5. Compile Rust 1.89 with target-cpu=haswell and audit the generated
   machine code for actual VPMADDUBSW/VPMADDWD/VPSHUFB and absence
   of any AVX512 popcount. Static assembler presence alone is not
   a proof of instruction counts or no register spilling; physical
   profiling is necessary for those claims.

No speed threshold determines a green CI. No Dell resources, active
background workloads, inference modes or production binaries are
modified. RemoteOps controls any later admitted Dell measurement.
A favorable hosted microbenchmark does NOT constitute a model-level
speed claim or numeric qualification.

## Decisions and transfer policy

If the new kernel is faster than previous PSHUFB but remains below
stock, retain it only as a documented research result. If actual
improvement is verified in a controlled model-level workload,
first demonstrate bitwise output fidelity and no metadata corruption
on the frozen model/strides/packing modes; only then consider CPU
dispatch under RemoteOps physical admission.

Potential reuse in SciRust requires its own SIMD/cross-platform ABI
test and verification of the Q8_K cached-sum consistency contract.
FLAT-ATTENTION has separate quantization and scheduling constraints;
there is no automatic application of this optimization.
