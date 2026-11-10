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
