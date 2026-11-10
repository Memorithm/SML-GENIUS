# BPB3-K paired signed-i8 scale shuffle at 114 bytes (PAIR-SCALE-1)

Research-only, stacked on the 114-byte SCALE-2 kernel (PR #943),
which itself depends on #942 and #941. This experiment does not
change GGUF model defaults, runtime kernels or original quantization.

## Hypothesis

The initial 114-byte SCALE-2 AVX2 kernel extracts two adjacent
signed-i8 subgroup scales for each 32-weight pair. It constructs
the low and high 128-bit halves of a YMM vector using two
scalar-to-128-bit broadcasts plus one lane insertion.

At the same **114-byte per 256-weight** format, both adjacent signed
scale bytes can be read together as one little-endian u16,
broadcast across a YMM register, selected into low versus high
128-bit halves by VPSHUFB, then sign-extended from signed i8 to
signed i16 using one left word shift plus arithmetic right
word shift. The result is 8 copies of scale0 in low lane, 8 copies
of scale1 in high lane, with **exact** [-32,+31] signed values.
The exact signed int dot then uses the unchanged VPMADDWD
grouping, same three-bit coefficient decoder and same 292-byte
Q8_K activation load shared across one/two/four weight rows.

This could reduce scalar loads/lane assembly at the cost of
one extra shuffle and two 16-bit shifts. We do NOT assert an
instruction-count win, register-pressure reduction, throughput
advantage or vectorization simply from source structure. The
frozen test measures whether this exchange is profitable.

## Frozen independent validation

1. 96 random-pattern original Q3_K-weight blocks and every signed
   Q8 code, raw FP16/f32 metadata patterns, the existing original
   C and independent original-Q3 wire oracle comparisons, plus
   all -32/+31 subgroup scale extremes and all 32 byte
   misalignments. Verify no mutation and exact output for
   one and four rows. Reject invalid payload and activation sizes.
2. Fetch *unmodified* upstream llama.cpp SHA
   f7591a6a01f9273de03331732bdc0b39721168be and run
   original C AVX2 896-case and C++ tiled 112-case oracles.
   With unit FP16/Q8 FP32 scales, require exact 1,024 FP32
   one-superblock dot output rows against genuine C stock.
3. Four arms timed on one hosted Haswell x86-64 runner:
   (a) optimized original ggml Q3_K C AVX2, 110 bytes;
   (b) previous 110-byte packed BPB3-K AVX2 tile;
   (c) new **114-byte paired-scale-shuffle** AVX2 kernel;
   (d) prior **114-byte standard two-scale** AVX2 kernel.
   The two 114-byte arms have identical source weights and Q8
   inputs, same 114B bytes, same conversion performed once
   outside timing. All arms measured at tile=1/2/4 and
   output rows=32/256/1024, with 3 warmups, 24 balanced
   four-way order rounds, 4 repetitions per call. Preserve
   all **216** complete four-way raw records, all checksums
   and descriptive ns/row medians. No win threshold gates CI.
4. Rust 1.89 Haswell-targeted assembly is inspected for
   signed AVX2 instructions and absence of AVX-512 VPOPCNT.
   Full-program grep cannot prove register spilling or port
   latency, which require separate physical profiling.
5. This is *single Q3_K superblock* and *integer-dots* with
   unit-scale FP32 stock parity only. It does NOT qualify
   full-model logits, stock tiled 4×4 FP32 ABI, multiblock
   FP32 reduction order, memory bandwidth, physical Dell
   turbo/NUMA isolation or inference token throughput.

If pair-shuffle is slower, retain its raw negative evidence
rather than hiding it or changing the baseline. Even if faster
than baseline114, genuine stock Q3_K must independently be
beaten at model-level before any promotion. No RemoteOps Dell
job is part of this hosted-only research proposal.


## Observed same-run outcome, 10 November 2026

[Exact-head dedicated workflow 42878397655](https://github.com/Memorithm/SML-GENIUS/actions/runs/42878397655)
on SHA d264b4b8148e031f43e4d4493a73eae39760038b completed
successfully: 24 Rust tests passed (including manually populated
six-bit signed -32/+31 alternating scales, all 32 misalignments
and preserved input sentinels), 896 unchanged upstream C AVX2
and 112 C++ tiled oracle cases, and 1,024 original-wire
Q3_K bitwise FP32 one-block unit-scale comparisons. All 216
balanced four-way raw records passed integer checksum parity.

Descriptive medians, nanoseconds **per output row** on the same
hosted runner. Lower is faster.

| Rows | Tile | Original Q3_K C stock | Packed110 BPB3-K | Paired-shuffle114 | Standard114 |
|---:|---:|---:|---:|---:|---:|
| 32 | 1 | 45.96 | 73.89 | 54.43 | 53.27 |
| 32 | 2 | 27.62 | 50.09 | 32.55 | 31.01 |
| 32 | 4 | 27.59 | 46.84 | 36.81 | 36.08 |
| 256 | 1 | 26.49 | 40.60 | 32.58 | 31.86 |
| 256 | 2 | 27.50 | 69.89 | 37.08 | 46.41 |
| 256 | 4 | 23.53 | 49.50 | 43.90 | 42.92 |
| 1024 | 1 | 23.54 | 39.45 | 31.09 | 30.14 |
| 1024 | 2 | 23.57 | 53.24 | 32.41 | 29.51 |
| 1024 | 4 | **23.55** | 45.25 | **35.78** | 38.57 |

At 1,024 rows / four-way tile, pair-shuffle gives
(38.57-35.78)/38.57 = **7.23% less per-row time**
than the same 114-byte baseline, but original stock C Q3_K
still runs 35.78/23.55 = **1.519x faster**.
At 1,024 rows / two-way tile, pair-shuffle **regresses**
32.41/29.51-1 = **9.83%** versus standard114.
There is also a negative comparison at one-way tile.
Therefore this is a *mixed* result, not broad superiority.

Source-level removal of one scale-byte scalar load is not
proof that the compiler generated a faster per-function
instruction sequence. Further performance work needs actual
physical counters, spilling analysis and qualified full-model
FP32 fidelity. Retain as experiment; do not activate runtime.
