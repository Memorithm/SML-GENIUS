# BPB3-K compact preencoded 114-byte signed-i8 scales (SCALE-2)

Status: research-only, stacked on MROW-1 and SCALE-1,
without inference/GGUF default or cross-repository deployment.

Signed six-bit Q3_K subgroup scales are in [-32,+31] and therefore
fit in one signed 8-bit byte each. SCALE-1 stored 16 signed i16
subgroup scales in 32 bytes, resulting in 130 B/block (20 B more
than stock's 110 B). This SCALE-2 candidate stores:
- 96 original BPB3-K signed bitplane bytes (identical);
- 16 consecutive signed i8 subgroup scales at 96..111;
- 2 raw global FP16 scale bytes at 112..113.

**Total = 114 bytes per 256 weights**, a 4-byte overhead,
only 4/110 = **3.6364%** above the original. Conversion preserves
all signed weight coefficients, subgroup scales and original FP16
metadata exactly. The vectorized dot still uses VPBROADCASTD,
VPSHUFB, VPMADDUBSW and VPMADDWD with reuse of signed Q8_K
activation and shared +4 bias across up to four weight rows.

No dense weight buffer, heap allocation per dot, or Q8_K
cached-sum correctness assumption is added. The 114-byte research
payload is NOT a valid original ggml Q3_K GGUF wire block and must
never be silently passed to stock ggml.

## Prospective benchmark / fidelity protocol

1. Independent original-Q3_K source weight decoding of all 16
   six-bit signed subgroup scales, same 256 signed coefficients,
   all Q8_K signed 8-bit codes and raw FP16/f32 metadata bits
   against both stock PSHUFB and signed-i16 SCALE-1.
   Reject all malformed source/runtime lengths and preserve
   read-only input bytes. Exercise all 32 misalignments.
2. Run pinned unchanged original C AVX2 896-case and tiled C++
   112-case native oracles (llama.cpp exact SHA
   f7591a6a01f9273de03331732bdc0b39721168be).
   For 1024 source Q3_K weight rows with FP16/f32 scales
   fixed to exactly 1.0, require bitwise FP32 equality of
   stock 110 B, packed110 AVX2, 130 B signed-i16 and
   114 B signed-i8 candidates.
3. Hosted x86-64 Haswell-targeted Rustc 1.89 same-run four-way
   benchmark at tile sizes 1/2/4, with output rows 32/256/1024.
   Three warmups, 24 balanced four-way order rounds and four
   calls per timed invocation. Preserve all 216 raw observations,
   not just favorable medians. Preencoding of both formats
   occurs only once outside the timed steady-state region.
4. Inspect compiled AVX2 instruction candidates and ensure
   absence of AVX-512 popcount. Global instruction grep does
   not qualify per-function register spilling or CPU ports.
5. Report all four relative and absolute times on the same
   hosted runner. Any apparent win must also account for the
   +3.6364% model weight footprint and initial conversion
   cost in a separately admitted physical RemoteOps run.
   No model inference tok/s, multiblock FP32 accumulation
   or arbitrary-batch ggml nrc=4 parity is established here.

This is a distinct prospectively frozen experiment, not
retroactive editing of SCALE-1's earlier 130-byte results.
It targets a lower memory overhead, not guaranteed speed.
A kernel-only green CI must not cause model dispatch promotion.
