# Q3_K-equivalent unsigned-offset three-bitplane format — AVX2

Status: research-only, no inference integration.
Original ggml Q3_K reference: llama.cpp f7591a6a01f9273de03331732bdc0b39721168be.
This is an **additional, distinct format candidate** in the broadcast32
branch, not a retroactive change to the earlier frozen broadcast32 run.

## Format and conversion identity

Baseline BPB3-K stores, for each 64-coefficient group, three LE u64
bitplanes: negative-bit, sign-magnitude-bit0 and sign-magnitude-bit1.
A Q3_K signed coefficient q is in [-4,+3], and in that representation:

    q = (bit0 XOR negative) + 2*(bit1 XOR negative) - 4*negative

The proposed alternative stores *the three bits of q+4 directly*:
the top plane is NOT negative, and the two lower planes are
(bit0 XOR negative), (bit1 XOR negative), respectively. This is a
**lossless word-wide XOR/NOT transformation**, with four groups of
three u64s = 96 bytes plus the original 12-byte signed six-bit scales
and original 2-byte FP16 global scale: **110 bytes per 256 coefficients**
exactly as original Q3_K/BPB3-K, no larger dense coefficient buffer.

Now the entire inner SIMD decode is:

    unsigned_q = plane0 + 2*plane1 + 4*plane2
    signed_q = unsigned_q - 4

Each plane uses one four-byte YMM VPBROADCASTD and VPSHUFB expansion
per 32 weights. There are no per-lane sign XORs. Signed-Q8_K activation
remains 292 bytes; using VPMADDUBSW, the -4 correction is subtracted
inside SIMD, before original signed six-bit scales are applied with
VPMADDWD. **Q8_K cached sums are not trusted or needed**; the 16KiB
lookup cache is likewise absent. Original GGUF and model dispatch
are not changed. Conversion is outside the timed dot window exactly
as in prior BPB3-K studies; end-to-end conversion amortization is
not established by these measurements.

## Frozen evidence before observing results

1. Original Q3_K independent scalar signed-integer oracle and prior
   signed-magnitude BPB3-K PSHUFB oracle on 96 original weight
   blocks x 16 Q8 activation patterns, including all 256 signed
   Q8 codes and -128/+127 extrema. Copy raw FP16/f32 metadata bits
   untouched, including nonfinite payloads.
2. Exact malformed weight/activation lengths and unaligned 0..31
   byte offsets. Input sentinels remain unchanged. Converter rejects
   malformed 110-byte original BPB3-K buffers.
3. Fetch the original unmodified ggml AVX2 C source and tiled C++
   source at pinned upstream commit. Preserve their 896 and 112
   independent cases. Verify 1,024 original Q3_K source blocks,
   single-block FP32 bit-for-bit agreement with FP16 and f32 scales
   fixed at exactly 1.0. Arbitrary-scale/multiblock floating-point
   parity is **not** established by this experiment.
4. Compare original Q3_K C AVX2, the **already successful**
   signed-magnitude broadcast32 Rust variant, and unsigned-offset
   broadcast32 Rust variant **on the same hosted runner** with
   exactly 1/32/256/1,024 weight rows, 3 warmups, 24 fully balanced
   3-way order rounds and 4 repeats per timing call. Retain all
   96 raw triplets and all per-size descriptive medians. No positive
   speed threshold controls CI; no cherry-picked samples.
5. Emit Rustc 1.89 AVX2/Haswell assembly and report actual presence
   of VPSHUFB, VPBROADCASTD, VPMADDUBSW, VPMADDWD and absence
   of VPOPCNT. Do not infer register spills/port contention from
   mnemonics alone.

No Dell RemoteOps job or real-model inference is launched by the
hosted benchmark. Only proceed to any physical qualification after
strict full-model numeric parity, separate model-level code path,
and RemoteOps admission/contamination controls. This research
format is not a claim of stock Q3_K superiority until the actual
native optimized AVX2 benchmark demonstrates it.

If successful, the three-plane unsigned-offset coding and AVX2
broadcast/shuffle primitive could be useful in SciRust but only
after destination-specific ABI, license, provenance and numeric
requalification. FLAT-ATTENTION has different hardware scheduling
needs and is not an automatic beneficiary.
