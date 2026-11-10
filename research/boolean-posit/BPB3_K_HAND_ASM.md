# BPB3-K handwritten x86-64 AVX2 — register-controlled kernel

Status: research-only, non-promotable. This branch is stacked on the
[PSHUFB-intrinsics PR #936](https://github.com/Memorithm/SML-GENIUS/pull/936).
Do not merge into inference without a new, separately frozen whole-model
numeric and RemoteOps physical qualification.

## Hypothesis and implementation

The source research/boolean-posit/bpb3k_avx2_hand.S implements exactly
one 256-weight signed-Q8_K dot in GNU assembler, Intel syntax, x86-64
System V ABI. The Rust safe boundary is bpb3k_hand_asm.rs. The 110-byte
BPB3-K and 292-byte Q8_K payloads are unchanged. No dynamic activation
preparation, heap allocation, changes to dispatch defaults, or GPU SDK.

Eight 32-code pairs are statically unrolled. Three sets of AVX2
instructions decode the BPB3-K sign, low and high bitplanes in YMM
registers. VPSHUFB expands two 16-bit subgroups directly into 32 SIMD
lanes. Integer value q in [-4,+3] obeys:

    q = (low XOR sign) + 2*(high XOR sign) - 4*sign

VPMADDUBSW receives q+4 as unsigned [0,7] and signed Q8 values as its
second operand. A second VPMADDUBSW subtracts the four-offset
contribution. VPMADDWD applies exact signed six-bit subgroup scales.
The design avoids saturating i16 and accumulates eight i32 SIMD lanes
until a single final reduction to eax. There are **no stack accesses in
the hand-written function**. Only caller-saved GPR and YMM registers
are written. The final VZEROUPPER avoids AVX/SSE transition penalties.

The exact 32-byte constants are each aligned to 32 bytes using .p2align
5. The input weight and activation pointers may be **unaligned**: scalar
loads and VMOVDQU are used. No claim that alignment is irrelevant for
performance is made. The safe boundary validates slice lengths and AVX2
feature availability before calling the unsafe extern ABI routine.

This does not prove that upstream callers or Rust-generated wrappers
cannot spill registers, nor does it prove port throughput. Function-level
disassembly and native counter evidence must be examined separately.

## Fixed evaluation before observing results

1. Compile the GNU .S in a GitHub hosted x86-64 runner. Inspect its full
   object disassembly for VPSHUFB, VPMADDUBSW and VPMADDWD, absence of
   VPOPCNT and absence of stack push/pop or [rsp]/[rbp] memory operands.
2. Run the independent original Q3_K wire-format scalar integer oracle
   plus existing PSHUFB Rust intrinsic comparison over 96 source
   blocks × 16 signed-Q8 patterns. Include extrema and all Q8 signed
   codes, raw FP16/f32 metadata, exact length rejection and **all
   32 pointer misalignments** with writable sentinels.
3. Fetch the pinned original ggml C AVX2 function, unmodified, from
   [llama.cpp f7591a6](https://github.com/ggml-org/llama.cpp/tree/f7591a6a01f9273de03331732bdc0b39721168be)
   and use the existing source-integrity checking extractor/oracle.
   Keep the 896 original AVX2 and 112 original tiled checks.
4. For the one-block, one-thread microbench, freeze the FP16/f32 scales
   at 1.0 for exact FP32/int conversion. The independent source Q3_K
   data are transcoded once OUTSIDE timing. All 256 signed Q8_K codes
   appear once. Verify 1,024 rows bit-exact against both the original
   AVX2 C and the Rust intrinsic implementation before measuring.
5. Compare all **three** implementations in one process, same data,
   same compiler target-cpu=haswell, across 1/32/256/1,024 rows.
   Three untimed warmups, 18 rounds per row count, four repeats per
   invocation, all six permutations of execution order repeated
   three times. Record the **72 raw timing triplets**, with no
   trimming, plus descriptive median ns/dot and stock/asm, Rust/asm
   ratios. The original C is built with its pinned known AVX2 flags.
6. No positive speed threshold can turn failure into CI success.
   CI green means exact math, expected instructions and complete
   measurements; it does not mean the assembly is faster.

## Decision boundaries

The assembly is not portable to Windows/macOS ABIs or ARM64. On the
Dell E5 v4, AVX2 is available but AVX-512 VPOPCNTDQ is not.
The static no-stack condition excludes **direct** spills from the .S
function, not spill traffic in enclosing inference call sites.

A genuine physical claim requires RemoteOps, control of background CPU
contention, same exact model GGUF, full-logit/token fidelity and paired
latency/tokens/s under a frozen prospective protocol. Models and hosted
synthetic microbenchmarks cannot be substituted for those checks.
No kernel or full-model default is changed.

The source is written independently against the publicly documented
Intel and x86-64 SysV ABI instruction contracts, with external llama.cpp
used only as an independent oracle; license/provenance must still be
reviewed for any planned transfer into SciRust.


## Observed CI results — 2026-11-10 (no retroactive change to protocol)

[Exact-source CI run 42851204567](https://github.com/Memorithm/SML-GENIUS/actions/runs/42851204567),
source head 810b6b33cd9e169da48f5b75cb5f648e75fa551b:
the standalone hand-assembler oracle completed successfully.
24 Rust unit tests succeeded (including the existing PSHUFB tests),
the independent original AVX2 oracle ran 896 cases and the original
tiled oracle 112 cases. A 1,024-row Q3_K-vs-Rust-vs-asm
unit-scale, single-superblock bitwise-FP32 oracle succeeded.
All 72 synthetic timed triplets were recorded. The exact function
object disassembly contains VPSHUFB, VPMADDUBSW and VPMADDWD, no
VPOPCNT and no stack memory references or push/pop instructions.

Medians in nanoseconds per product, identical runner / corpus:

| Rows | Original stock AVX2 | Rust AVX2 intrinsic | Handwritten AVX2 |
|---:|---:|---:|---:|
| 1 | 45.00 | 87.75 | 62.50 |
| 32 | 37.18 | 90.88 | 54.95 |
| 256 | 35.56 | 81.29 | 51.14 |
| 1024 | **23.93** | **52.86** | **43.29** |

At 1,024 rows handwritten assembly is faster than the Rust intrinsics
on this single hosted experiment by **52.86 / 43.29 = 1.221x**,
or approximately 18.1% lower time per product. However the
original native stock is **43.29 / 23.93 = 1.809x faster** than
the assembly candidate. Timing noise and host variations prohibit
extrapolation beyond these paired observations.

The assembly hypothesis is supported for *improving the experimental
Rust candidate*, but is **rejected as currently meeting the target of
beating the optimized stock Q3_K kernel**. Do not initiate a
CPU-intensive, interruptive Dell campaign for this candidate merely
to seek a favorable result. Next justified work is code-size,
critical-dependency and port-profile analysis, with pre-registered
changes and fresh paired controls. Lack of hand-function stack loads
does not prove that surrounding Rust/ggml code never spills.

The CI checks described here are a diagnostic test of the exact
assembly, not the full-model CI required for merging the stacked PR.
