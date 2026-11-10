# BPB3-K Q4 146/160 B vs paired 228 B — frozen experimental protocol

Status: research-only. No inference dispatch, GGUF type, or production
kernel change. Physical Dell operation must use RemoteOps only.

Original comparison: pinned unchanged optimized ggml Q3_K C AVX2
and tiled C++ source at upstream llama.cpp commit
f7591a6a01f9273de03331732bdc0b39721168be.

## Three distinct hypotheses

Q4-146: 256 signed three-bit Q3_K coefficients stored as 4-bit
unsigned-offset q+4 codes in **128 B nibble payload**. Preserve the
sixteen exact signed subgroup scales as sixteen i8 bytes and the
original raw FP16 global scale as two bytes: **146 B/256 weights**,
+32.73% relative to 110-byte native Q3_K. Two nibbles pack
low/high positions of each 32-code group, allowing an AVX2
broadcast, mask, shuffle and direct VPMADDUBSW/VPMADDWD.
No dense 256-code buffer inside the kernel. No model GGUF use.

Q4-160: exactly the same coefficient and signed-scale bytes, plus
**14 zero padding bytes** between scale bytes and the raw FP16,
giving a stride divisible by 32: **160 B/256 weights**,
+45.45% relative to native Q3_K. This does not guarantee actual
base pointer alignment. Testing it directly prevents speculative
claims that padding alone improves AVX2 performance.

PAIR228: concatenate TWO previously qualified, unchanged
114-byte BPB3-K blocks: **228 B/512 weights** (NOT 228 bytes
for only 256 weights). Both original 16 signed subgroup scales
and FP16 bits per original 256-code subblock remain. Fuse two
consecutive 256-code Q8_K products (292 B x2 activations)
in an eight-step SIMD loop with independent YMM accumulators.
Test the paired pipeline against real original Q3_K C n=512
and two separately called standard 114-byte BPB3-K kernels.
Do not mix per-512-block timing with per-256-block timing.

All formats are mathematically lossless. One-time conversion
is **outside steady-state timing**. Full-model memory bandwidth,
model-load conversion and cache/NUMA behavior remain unqualified.

## Frozen qualification protocol

1. An independently coded original Q3_K wire decoder checks
   signed integer results for 96 source seeds, extremes signed
   Q8 values, the complete 256 signed byte-code domain,
   raw FP16/f32 metadata patterns including nonfinite bits,
   malformed sizes, unaligned sources and immutability.
2. Preserve the original SHA-pinned upstream ggml C native
   AVX2 896-case oracle and tiled C++ 112-case oracle.
   With exact unit FP16/f32 scales, require bitwise stock
   FP32 parity over 1024 single-block Q3_K source rows
   against original C, previous 114, new Q4-146 and Q4-160;
   and over 512 **two-block** source pairs for PAIR228.
   No arbitrary-scale multiblock FP32-order claim.
3. On exactly one hosted x86-64 Haswell-targeted Rustc 1.89
   runner, original stock C vs prior 114B vs Q4-146 and
   Q4-160 for 32,256,1024 rows; 3 warmups, 24 balanced
   four-arm order rounds, 4 repeats per timing window.
   Retain **72 original four-arm raw records**.
4. Separately compare original stock C n=512, two separately
   called 114B dots, and paired228 two-block fused SIMD
   for 16,128,512 pairs; 3 warmups, 24 balanced three-arm
   order rounds and 4 repeats per measurement. Retain
   **72 original three-arm raw records**.
5. Inspect actual Haswell SIMD mnemonics and exclude AVX512
   popcount; no false instruction-port/spill inference.
   No speed threshold controls green CI. Avoid cherry picking.

A favorable arithmetic microbenchmark does not establish
model-level token speed, energy use or full-model FP32 fidelity.
No production promotion until independent model-level and
RemoteOps-admitted native Dell tests are fully qualified.

## Decision rules

Do not prefer the padded 160-byte form unless it actually
wins versus 146-byte on identical runs, including bandwidth.
Do not claim that 228 bytes increases density: it has twice
the weights of the 114-byte form and its memory traffic is
twice that of a single 114-byte block. Compare equal arithmetic
work and equal activation values on the native original Q3_K.


## First observed result — 2026-11-10, same hosted runner

[Exact-head GitHub Actions 42884129767](https://github.com/Memorithm/SML-GENIUS/actions/runs/42884129767)
on SHA 064d93bc8a9ae5d0841c98895f52dabed86de37e
**SUCCESS**: 34 Rust tests green, original native Q3_K AVX2
896-case oracle and tiled C++ 112-case oracle passed,
1024 original-source single-block bitwise FP32 rows passed
for stock C/114/146/160 with exact unit global scales;
512 original-source **two-block** bitwise FP32 native C
rows passed for 228. Retained all 72 four-way Q4 and
72 three-way pair228 raw timing records with exact checksums.
No AVX512 popcount in the Haswell assembly.

### Single 256-weight superblock (ns / output row; lower is faster)

| Rows | Original optimized Q3_K C, 110B | Previous BPB3-K, 114B | Q4-146 | Q4-160 |
|---:|---:|---:|---:|---:|
| 32 | 36.79 | 46.49 | **22.08** | 22.46 |
| 256 | 37.81 | 49.60 | **23.98** | 24.03 |
| 1024 | 32.83 | 40.81 | **21.44** | 22.20 |

At 1024 rows, Q4-146 reduced kernel time by
(32.83-21.44)/32.83 = **34.69%** versus original native
Q3_K C; throughput ratio 32.83/21.44 = **1.531x**,
*on this single synthetic hosted experiment only*.
Q4-160 was slightly **slower** than Q4-146
(22.20/21.44 - 1 = **3.54%** more time).
Padding does not deliver an observed win at this size.
The 146-byte format pays +32.73% weight footprint and
conversion costs excluded from this benchmark. In an
actual bandwidth-bound model this extra footprint may
reverse the arithmetic-kernel speed result.

### Two 256-weight superblocks (ns / TWO-block pair)

| Pairs | Original Q3_K C, n=512 | Two independent 114B kernels | Paired228 512-weight AVX2 |
|---:|---:|---:|---:|
| 16 | **58.55** | 103.47 | 74.52 |
| 128 | **57.77** | 100.99 | 73.57 |
| 512 | **53.83** | 83.12 | 69.19 |

At 512 pairs, paired228 is faster than TWO independent
114B kernel calls (83.12/69.19 = **1.201x throughput**);
however it is **69.19/53.83 = 1.285x slower** than
original optimized C Q3_K for the same 512 coefficients.
The 228-byte pair does not demonstrate stock superiority.

**Conclusion:** Q4-146 is the first candidate in this
research chain to beat the original optimized native
Q3_K C **in this exact hosted one-superblock benchmark**.
This is a valuable but *strictly narrow* result.
No arbitrary multiblock FP32 summation parity, actual
model logits, true model tokens/s, physical Dell
bandwidth/NUMA profile or load-time conversion cost has
been qualified. Do NOT change the GGUF model default,
promote the Q4-146 kernel, or claim a production speed
factor from these microbenchmarks. The next scientifically
necessary test is a model-scale, streaming memory and
exact-FP32 qualification under RemoteOps, using identical
original model weights and activation traces.


## Follow-up: independent large contiguous-weight streaming — 2026-11-10

The first cache-oriented result was **replicated** by
[GitHub Actions run 42884848086](https://github.com/Memorithm/SML-GENIUS/actions/runs/42884848086),
exact head e59272ff8701ce91498c0c68ad4f48d5183f5952.
The entire workflow succeeded: 34 Rust tests, original
optimized C AVX2 and C++ tiled oracles, 1024 original
single-block plus 512 genuine two-block unit-scale C
FP32 bit-exact rows, all original 72 + 72 paired microbench
raw records and an **additional 12** four-arm streaming
raw records. The streamed checksums cover all 262,144
distinct deterministic source rows. The kernel outputs
were also independently compared at intervals and the
last row against original C Q3_K stock. No upstream
model files were changed.

The additional streaming experiment uses **four separate,
contiguous arrays** rather than interleaving different
candidate formats per row. Total weight-array footprints:
original 110B: 28,835,840 bytes;
previous 114B: 29,884,416 bytes;
Q4-146: 38,273,024 bytes;
Q4-160: 41,943,040 bytes.
All candidates use the *same signed Q8_K activation*
repeatedly, with a unit FP16/f32 scale and one original
256-coefficient superblock per output row.
No physical memory-bandwidth counters or Dell
cache-miss measurements were obtained; the experiment
is called "streaming" because of the contiguous read
pattern, **NOT because DRAM bandwidth saturation
has been verified**.

### Repeat, single-block synthetic ns per row, same-run

| Rows | Original Q3_K C | Previous BPB3-K 114 | Q4-146 | Q4-160 |
|---:|---:|---:|---:|---:|
| 32 | 27.46 | 35.36 | **17.37** | 17.60 |
| 256 | 18.33 | 26.15 | 12.89 | **12.84** |
| 1024 | 18.60 | 26.18 | **13.16** | 13.89 |
| **262,144 streaming** | **17.27** | 24.89 | 11.81 | **11.72** |

At 1,024 synthetic rows, Q4-146 uses
(18.60 - 13.16)/18.60 = **29.25% less time**
versus true optimized original C stock, a **1.413x**
throughput factor. For the streamed 262,144
separate contiguous weights, Q4-146 achieves
(17.27 - 11.81)/17.27 = **31.62% less time**,
**1.462x** stock throughput despite the larger
weight-array footprint. This is evidence that the
first hosted speed improvement was not solely an
artifact of keeping 1024 rows in cache, although
cache/DRAM event attribution remains unknown.
Q4-160 is 0.09 ns/row ahead at the large streamed
size but 0.73 ns/row behind for 1024 rows and uses
3,670,016 more bytes at this streamed row count.
These small cross-size differences do not warrant
declaring padding generally beneficial.

### Repeat, genuine two-block 512-coefficient control (ns/pair)

| Pairs | Q3_K native C 512 | Two original 114B calls | Pair228 fused |
|---:|---:|---:|---:|
| 16 | **47.11** | 81.69 | 65.25 |
| 128 | **46.49** | 79.44 | 64.49 |
| 512 | **40.18** | 67.47 | 59.90 |

At 512 pairs, fused228 is 11.22% lower time than
the two separate114 reference calls, but still
59.90/40.18 = **1.491x slower** than unchanged
native optimized Q3_K C for the same 512 weights.
It is NOT the best candidate for promotion.

**Research decision:** Q4-146 remains our currently
strongest demonstrated arithmetic candidate versus
genuine original stock Q3_K in these hosted,
unit-scale, single-superblock workloads. It requires
32.73% more weight storage, model-load conversion,
and independent full-model arbitrary-scale/multiblock
FP32 and physical Dell throughput checks before
any runtime use. Never claim tokens-per-second or
model-quality improvement from this synthetic
benchmark, despite the attractive numerical result.
