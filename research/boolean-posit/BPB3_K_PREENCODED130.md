# BPB3-K 130-byte lossless scale-preencoded AVX2 microtile (SCALE-1)

Status: Research-only, experimentally DIFFERENT wire layout from 110-byte
BPB3-K and Q3_K; no ggml GGUF type ID or inference dispatch change.

The SML MROW-1 microtile in PR #941 already loads the signed Q8
activation vector ONCE for up to four weight rows and computes the
shared signed +4 bias ONCE. However, it must still extract each
signed 6-bit Q3_K subgroup scale from 12 compressed scale bytes
inside every dot; each row uses 16 subgroup scales per superblock.

This experiment transcodes original 110-byte BPB3-K once at model
loading to **130 bytes**:
- 96 original BPB3-K signed bitplane bytes, unchanged;
- 32 bytes: 16 signed little-endian i16 scales, exact original
  six-bit values [-32,+31] and same subgroup order;
- 2 bytes: global FP16 raw bits, unchanged, at offsets 128..129.

The format therefore requires 20 extra bytes per 256 weight values,
+18.1818% per block. It does NOT create a dense 256-weight i16
buffer. This overhead is a material change to the weight-read budget;
a faster in-cache dot may still lose in actual model inference due
to memory bandwidth, cache pressure or model-load conversion.

The new candidate applies the same AVX2 grouped PSHUFB, VPMADDUBSW,
VPMADDWD and shared-Q8 microtile arithmetic as MROW-1. The only
intentional algorithmic change is to read each signed subgroup scale
directly as i16, without repeatedly decoding its compact six-bit
representation. All signed Q8_K values [-128,+127] and exact
per-superblock i32 integer results must be preserved.

## Frozen evaluation contract

1. For each of 96 original Q3_K source cases and 16 signed Q8_K
   patterns, check all four independently converted 130-byte
   weight rows against the lossless original-Q3 scalar dot and
   existing 110-byte PSHUFB AVX2 oracle. Verify 16 subgroup scales,
   all f16 raw metadata bits incl. NaN, and all 256 possible Q8 codes.
2. Test converter rejects non-110-byte source blocks, CPU kernel
   rejects malformed 130-byte rows and 292-byte activation blocks,
   test unaligned addresses and no source mutation.
3. Keep original unchanged upstream ggml C AVX2 (896) and C++
   tiled (112) checks, fetched from pinned SHA
   f7591a6a01f9273de03331732bdc0b39721168be.
   With f16/f32 scales exactly 1.0 compare 1,024 one-block original
   C stock, prior packed110 microtile and expanded130 microtile
   **bitwise** as FP32; multiblock FP32 order is not qualified here.
4. Same hosted process, identical inputs, 1/2/4 microtiles,
   32/256/1024 output rows, three untimed warmups,
   18 fully balanced triplet order rounds per size, four repeats
   per timed call, retaining 162 raw timing triplets and all
   descriptive medians in ns/output row. Both 110->130 and original
   Q3->110 BPB3 transformations occur **outside** the timed region,
   as already done in MROW-1. This excludes real conversion cost;
   it is a *steady-state kernel-only* comparison.
5. Compile Rust 1.89 with Haswell target, audit actual optimized
   monomorphized instruction diagnostics without assuming any
   particular static instruction count; no AVX512 VPOPCNT.
6. A green research CI never implies complete model speed, accuracy,
   resource qualification or advantage over optimized stock Q3_K.
   A favorable kernel throughput result must still survive the
   increased memory footprint (+18.18%) and startup transcode
   cost in a separate full-model paired physical RemoteOps run.

No binary, GGUF model type, default kernel, runner service or GPU
dependency is changed by this experimental study. SciRust and
FLAT-ATTENTION reuse is conditional on independent destination
quantization and layout contracts.

SCALE-1 is a scientifically distinct experiment. It must not be
retrospectively conflated with the unmodified 110-byte MROW-1
predeclared benchmark.


## Observed hosted run — 2026-11-10

[Workflow run 42875135924](https://github.com/Memorithm/SML-GENIUS/actions/runs/42875135924)
on source SHA 935a9dd7e77ef26b5f5812dd45c3b34147dbed59
completed successfully. Twenty-three Rust unit tests, the original
C AVX2 896 and tiled C++ 112 checks, and all 1,024 original-wire
unit-scale FP32 model-independent source comparisons passed.
The actual benchmark emitted 162 raw 3-way timings with exact
checksums, not just summary medians. Its Haswell assembly contained
VPSHUFB/VPMADDUBSW/VPMADDWD/VPBROADCASTD, no VPOPCNT.
The counts were for the complete monomorphized benchmark assembly,
not a function-scoped instruction/uop or register-spill profile.

Descriptive medians (ns / output row), same hosted runner per row:

| Rows | Tile N | Original Q3_K stock C | 110 B packed AVX2 | 130 B scale-preencoded AVX2 |
|---:|---:|---:|---:|---:|
| 32 | 1 | 24.11 | 35.62 | 31.78 |
| 32 | 2 | 24.27 | 42.89 | 27.47 |
| 32 | 4 | 36.71 | 67.23 | 56.90 |
| 256 | 1 | 37.03 | 64.57 | 50.44 |
| 256 | 2 | 37.46 | 85.20 | 53.88 |
| 256 | 4 | 37.75 | 74.74 | 63.38 |
| 1024 | 1 | 33.72 | 53.75 | 46.09 |
| 1024 | 2 | 32.21 | 58.42 | **39.50** |
| 1024 | 4 | 25.05 | 41.63 | **35.76** |

For 1,024 output rows and tile N=2, SCALE-1 uses
(58.42 - 39.50)/58.42 = **32.39% less kernel time** than
the equal-tile 110-byte packed baseline. However, native original
Q3_K remains faster: 39.50/32.21 = **1.226x**.
For N=4, the preencoded layout improves the equal-tile packed
candidate by (41.63 - 35.76)/41.63 = **14.10% less time**,
but native stock is 35.76/25.05 = **1.428x faster**.
All comparisons above are *within the same hosted run*;
absolute ns/dot measurements from previous sessions cannot be
pooled because host conditions drift.

The 130-byte representation also permanently adds **20 B of
weight reads per 256 coefficients** unless a separate cache
placement strategy avoids their transfer; this may outweigh
the in-cache kernel gains. Transcode cost and full-model
memory pressure have not been measured. This is evidence
that predecoding scales can substantially accelerate our
experimental kernel, **not** evidence of stock superiority or
a model throughput benefit. No runtime promotion is authorized.
