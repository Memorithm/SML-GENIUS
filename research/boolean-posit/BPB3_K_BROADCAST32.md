# BPB3-K 32-bit-plane broadcast AVX2: zero 128-bit insertion

Research-only BPB3-K candidate, original 110-byte BPB3-K wire format and
original 292-byte signed Q8_K activation format; no model default changes.

## Proposed optimization

The prior Rust PSHUFB kernel loads two separate 16-bit plane fragments
into two separate SSE 128-bit lanes and inserts one half before a
256-bit shuffle. That happens three times for every 32 weights:
sign, low magnitude and high magnitude.

These fragments are **contiguous** in the existing BPB3-K format.
A single read of four little-endian bytes contains all bits for the
next 32 weights of each respective plane. Broadcast those four bytes
as a u32 throughout all 8 dwords of a YMM register. Then VPSHUFB
selects the first, second, third and fourth source bytes into the
corresponding 8 output byte lanes using a constant byte-select mask
with positions 0/1/2/3. Bit tests derive each of 32 packed {0,1}
coefficients.

This avoids 64-bit bitplane loads with 16-bit dynamic shifts, two
SSE inserts and 128-bit lane assembly overhead. It does **not** reduce
the number of PSHUFB instructions: there remain three independent
bitplane shuffles per 32-weight group. The exact saved instruction
count depends on compiler code generation and must be determined by
function-specific assembly, not guessed from Rust source.

The signed Q3_K integer q in [-4,+3] is represented as:

    q = (bit0 XOR sign) + 2*(bit1 XOR sign) - 4*sign

Unsigned q+4 can be handled by AVX2 VPMADDUBSW against signed Q8,
then the original -4 offset is subtracted before VPMADDWD applies the
six-bit signed subgroup scales, exactly as in the previously qualified
PSHUFB implementation. **Q8_K bsums are not trusted**, unlike the
separate cached-sum candidate in PR #939. The 110/292-byte ABI,
raw FP16/f32 metadata and 256-weight block are unchanged.

## Frozen validation protocol

1. Original independent Q3_K-wire scalar integer oracle vs both
   old and new PSHUFB AVX2: 96 blocks x 16 signed-Q8 patterns,
   -128/+127 and all 256 signed Q8 codes, raw f16/f32 NaN
   metadata payloads.
2. Validate bounds and read-only 110/292-byte source, every one of
   32 unaligned offsets for both pointers. No untrusted cached-sum
   invariants are added.
3. Fetch unchanged upstream original AVX2 C and C++ tiled function
   from SHA f7591a6a01f9273de03331732bdc0b39721168be. Preserve
   its 896 AVX2 / 112 tiled oracle checks. With scales exactly 1.0,
   check 1,024 source-Q3_K blocks bitwise in FP32.
4. Paired on one hosted x86-64 runner with exactly the same
   source/activation rows: optimized original Q3_K stock AVX2 C,
   previous Rust PSHUFB, new Rust broadcast32. 1/32/256/1024 rows,
   three warmups, four repeats, 24 fully balanced six-order
   3-way rounds per size; **96 complete raw triplets** and median
   ns/dot, no cherry-picked samples.
5. Haswell-targeted Rustc 1.89 optimized assembly must contain
   VPSHUFB, VPMADDUBSW and VPMADDWD and exclude AVX-512 VPOPCNTDQ.
   Check emitted VPBROADCASTD and generated memory loads; its absence
   is a compiler-codegen diagnostic, not a numeric failure if the
   actual arithmetic is correct.

Do not treat a green exact-head CI as Dell or production proof.
As with prior microbenchmarks, original Q3_K -> BPB3-K re-layout
occurs outside the time windows and only a unit-scale single-block
subset proves FP32 exactness. A winner would need full-model
bit-exact logits, stride and tiled parity and physical RemoteOps
admission. This experiment does not interrupt the Dell machine,
add GPU dependencies, or change inference dispatch.

If the 32-bit broadcast proves superior for the same arithmetic,
the reusable SIMD technique may be requalified within SciRust.
Any reuse must preserve the independent destination ABI and exact
quantization contract.


## Observed first broadcast32 run — 2026-11-10

[Exact-head workflow 42858071622](https://github.com/Memorithm/SML-GENIUS/actions/runs/42858071622),
at SHA daeb66e943403eaf16ed9644a3adcd7d10c228b9, passed.

- 24 Rust tests, original upstream AVX2 C 896 source-independent
  oracle cases, original tiled C++ 112 cases, and 1024 exact
  unit-scale FP32 original-Q3 output comparisons passed.
- Generated Haswell machine code did contain VPBROADCASTD, VPSHUFB,
  VPMADDUBSW and VPMADDWD and no AVX512 VPOPCNT.
- All 96 original-source / prior PSHUFB / broadcast32 timed
  triplets passed checksum parity and were retained.

Descriptive median nanoseconds/dot within this single runner:

| Rows | Original Q3_K stock | Earlier PSHUFB | New broadcast32 |
|---:|---:|---:|---:|
| 1 | 47.50 | 90.00 | 67.75 |
| 32 | 35.23 | 72.16 | 52.98 |
| 256 | 35.14 | 80.18 | 55.03 |
| 1024 | **28.04** | **54.08** | **42.44** |

On 1024 rows the optimized broadcast32 decode reduced the
prior candidate's time by **(54.08 - 42.44) / 54.08 = 21.52%**
(throughput factor 54.08/42.44 = 1.2743).
It nonetheless remained **42.44 / 28.04 = 1.5135x slower**
than the original native optimized Q3_K AVX2 C function.

This is a genuine *within-candidate* improvement in the frozen
hosted synthetic measurement, not a winning model/inference kernel.
Follow-up unsigned-offset coding is preregistered and measured
separately in BPB3_K_OFFSET32.md; do not compare absolute timings
from different hosted runs as if they were controlled head-to-head.
