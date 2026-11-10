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


## Results observed 10 November 2026 — research-only

[Exact-head dedicated run 42876557549](https://github.com/Memorithm/SML-GENIUS/actions/runs/42876557549)
at  c9bbcf2885445ca4af8c77297b30cfeb43e5651c:
green. 23 Rust unit tests, 896 original optimized upstream C AVX2
oracle cases, 112 original C++ tiled cases, 1024 source-Q3_K
single-superblock unit-scale bitwise FP32 rows and all 216
four-way timed raw records passed without selective sample removal.

Hosted-runner descriptive medians in ns per row (lower is faster):

| Output rows | Tile | Stock Q3_K 110 B | Packed BPB3-K 110 B | Preencoded i16 130 B | Preencoded i8 114 B |
|---:|---:|---:|---:|---:|---:|
| 32 | 1 | 27.16 | 49.22 | 38.66 | 36.77 |
| 32 | 2 | 26.99 | 63.14 | 37.16 | 35.52 |
| 32 | 4 | 17.99 | 31.06 | 27.15 | 25.51 |
| 256 | 1 | 17.65 | 27.77 | 31.57 | 25.93 |
| 256 | 2 | 27.32 | 59.81 | 36.89 | 34.71 |
| 256 | 4 | 27.65 | 56.10 | 47.63 | 43.04 |
| 1024 | 1 | 27.06 | 47.58 | 37.78 | 36.74 |
| 1024 | 2 | 17.78 | 39.48 | **24.31** | 30.61 |
| 1024 | 4 | **19.49** | 31.17 | 31.29 | **30.72** |

The 114-byte signed-i8 scale candidate uses only 4 extra bytes
versus stock rather than 20 for the 130-byte signed-i16 format.
At 1024 rows / tile4 it improves on the 130-byte version
by (31.29 - 30.72) / 31.29 = **1.82% lower time**, but
is 30.72 / 19.49 = **1.576x slower** than stock.
At tile2 on those same 1024 rows, it regresses versus i16
(30.61 versus 24.31 ns/row). The result therefore does
NOT establish a general speed advantage for signed-i8 scale
preencoding over signed-i16 scale preencoding.

All results reflect hosted synthetic one-superblock kernel time;
conversion remains outside timing, and 114-byte model footprint,
end-to-end GGUF load, cache/NUMA and multi-block FP32 fidelity have
not been qualified. Neither the 114-byte nor 130-byte candidate
has beaten original optimized stock Q3_K, and no runtime mode
promotion is warranted.
