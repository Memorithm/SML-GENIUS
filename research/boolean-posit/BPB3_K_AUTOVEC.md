# BPB3-K / Q3_K — compiler auto-vectorization evidence

## Hypothesis, provenance and baseline

Investigate whether Clang, GCC and Rustc can compile a **simple signed
integer dot loop** into a faster Haswell-compatible SIMD implementation
than our existing bitplane/popcount kernels, without explicit AVX2
intrinsics or hand-written assembly.

Use the unchanged original ggml Q3_K AVX2 C kernel, pinned to
[llama.cpp f7591a6](https://github.com/ggml-org/llama.cpp/tree/f7591a6a01f9273de03331732bdc0b39721168be),
as the primary comparison. The exact source checks and independent
original C/C++ oracle live in run_bpb3k_stock_oracle.sh. This is not a
comparison to an unoptimized scalar GGML kernel.

**Important**: The three C candidates consume the *original* Q3_K
110-byte wire format, not BPB3-K, while the Rust candidate follows the
same source format. The 292-byte signed-Q8_K activations are identical.
They decode into a temporary contiguous 256-element signed-i16
scaled-weight array, then perform a regular signed i16×i8 -> i32 dot.
Both phases are executed **inside every timed dot**. This avoids
unfairly excluding per-row decoding from an autovector baseline,
but the temporary stack array may make it slower than original GGML.
It is a compiler study, not a proposed new permanent GGUF layout.

Neither -O3 nor Rust -C opt-level=3 guarantees actual SIMD instructions.
Compiler loop-vectorization reports, emitted assembly and physical
timings must be considered separately. C candidates share **identical
source bytes** and each has its own uniquely named function:

- mem_q3_clang_auto: Clang -std=c11 -O3 -march=haswell, default auto SIMD;
- mem_q3_clang_novec: same Clang O3/Haswell, with both loop and SLP
  vectorizers explicitly disabled;
- mem_q3_gcc_auto: GCC -std=c11 -O3 -march=haswell, default auto SIMD;
- rust_auto: Rustc 1.89 -C opt-level=3 -C target-cpu=haswell,
  separate equivalent Rust loop with no manual vector intrinsics;
- ggml_vec_dot_q3_K_q8_K: **unchanged pinned C** original optimized
  Q3_K AVX2 reference, compiled by the existing exact source extractor
  at its previously frozen O2/mavx2/fma/f16c settings.

The difference between stock's pinned flags and the auto-vec candidates
is intentional and documented: stock is the real already-optimized
baseline, not a 1-to-1 compiler-flag ablation. The within-Clang
auto-vs-no-vector **is** a source-and-flags-controlled vectorizer ablation.

## Acceptance conditions, declared before observing any timing

1. Validate the exact original Q3_K bit and scale extraction against
   an independent Rust scalar original-wire oracle for 96 deterministic
   source blocks × 16 signed-Q8 activation patterns × 4 candidate
   compilers, including -128, 127, and a complete 256-code sweep.
   Integer operations are bounded, no signed overflow permitted.
2. With weight FP16 and activation FP32 scales fixed to exactly 1.0
   only, check 1,024 source-Q3_K rows against original C AVX2
   single-block output **bit-for-bit** as FP32. The integer sums are
   bounded by 4,194,304 and therefore exactly representable in f32.
   Raw FP16/f32 metadata preservation alone is not FP32 model parity.
3. Compile all candidates with target-cpu=haswell. Collect Clang
   -Rpass=loop-vectorize, GCC -fopt-info-vec-optimized and objdump
   for their actual generated assembly. Absence of a vectorization
   report is evidence that a compiler did not report a vectorized loop,
   **not** evidence of identical machine code or scalar-only behavior.
   Rust auto-vectorization requires function-specific disassembly
   before attributing any gain. No VPOPCNTDQ/AVX512 required.
4. Compare all five candidates on **identical** source operands in one
   x86-64 hosted runner with 1/32/256/1,024 rows, 3 warmups,
   20 fully balanced cyclic/reversed order rounds per row count,
   4 repeats per invocation, all 80 raw five-way timing samples
   retained without selective deletion and medians in ns/dot.
   Negative control must remain a fair O3 Clang build except for
   -fno-vectorize and -fno-slp-vectorize.
5. No speed threshold makes CI green. Correctness, source provenance,
   build reproducibility, diagnostic publication and full sample count
   are the CI gates. Compare medians as descriptive only.

## Limitations and decision policy

This benchmark measures one Q3_K superblock at a time, with synthetic
deterministic weights and Q8 activation, on a GitHub hosted runner.
It does **not** measure SmolLM2 tokens/s, arbitrary-scale/multiblock
floating-point accumulation, DRAM pressure, CPU-port contention or
register spills on Dell Xeon E5 v4. A favorable microbenchmark alone
does not justify integrating a new kernel into the inference runtime.

Future hardware profiling on the Dell may only use Memorithm/RemoteOps
with admission, CPU-contamination thresholds, resource budgets and
unchanged model/source identities. Do not interrupt running services.

Transferable observations on compiler optimization must be replicated
under SciRust or FLAT-ATTENTION's own data/layout contracts before reuse.
No external GPU dependencies are introduced.


## First measured run — 2026-11-10

Exact benchmark [GitHub Actions 42853509550](https://github.com/Memorithm/SML-GENIUS/actions/runs/42853509550),
at head 1f276f21aa3154c021dc7e6dc50f8c9c33074644.
Compiled with Clang 18.1.3, GCC 13.3.0 and Rustc 1.89.0,
Haswell-compatible target flags.

**Compiler evidence**: Clang explicitly reported a vectorized loop
at source line 59 with vectorization width 8 and interleave factor 4.
GCC reported the same source line vectorized with **32-byte vectors**.
The negative control disabled both Clang loop and SLP vectorizers.
There is no confirmed Rustc per-function vectorization report yet:
Rustc was built with opt-level=3 and target-cpu=haswell, but its
generated hot-loop disassembly must still be examined before labeling
it vectorized.

The independent original-Q3 wire oracle passed **6144** signed-integer
candidate checks (96 blocks × 16 activation fixtures × 4 compilers).
The original upstream C AVX2 test passed its 896 cases, and the tiled
oracle its 112 cases. All **1024** 1-superblock, unity-FP16/f32-scale
model-equivalent FP32 candidate results matched stock bit for bit.
All **80** five-way timing samples were retained.

Descriptive median nanoseconds **per fully timed original-Q3_K dot**:

| Rows | Stock Q3_K AVX2 | Clang O3 vectorized | Clang O3 no-vector | GCC O3 auto | Rustc O3 |
|---:|---:|---:|---:|---:|---:|
| 1 | 40.00 | 1199.25 | 1352.00 | 1129.25 | 1134.25 |
| 32 | 18.31 | 858.72 | 766.16 | 664.59 | 841.27 |
| 256 | 17.63 | 669.97 | 771.45 | 614.93 | 597.41 |
| 1024 | **17.59** | **663.29** | **775.40** | **620.51** | **599.05** |

For 1024 rows, Clang's auto-vectorized candidate takes
(775.40 - 663.29) / 775.40 = **14.46% less time** than the same
source compiled with both vectorizers disabled. Yet it is
663.29 / 17.59 = **37.71x slower** than the genuinely optimized
stock Q3_K kernel. The Rust O3 source is approximately
599.05 / 17.59 = **34.06x slower** than stock, regardless of whether
Rustc vectorized its particular loop.

This **does not** show that auto-vectorization is intrinsically
37.71x inferior to SIMD intrinsics. The C/Rust candidates choose a
different expensive *decode-to-256-i16-array-then-dot* algorithm.
The comparison establishes:
(1) compiler vectorization can help the same C algorithm,
(2) vectorizing the final reduction is not enough to pay for
complete per-row dense decoding, and
(3) a performance-competitive candidate would need a fused
packed-weight vectorization pathway without materializing this array,
or a separately justified reusable predecoded format whose **extra
storage/bandwidth** is honestly measured.

At 32 rows, the Clang O3 vectorized variant was *slower* than its
no-vector control in this session (858.72 vs 766.16 ns/dot), illustrating
that a single hosted-runner median is not a universal speed theorem.
No outlying sample was removed and no favorable subset promoted.

**Decision**: do not promote any auto-vectorized dense-decode kernel.
A further study should first separate decoding cost and product-loop
cost with an explicit paired ablation, using the same stock baseline,
then investigate compiler-assisted vectorizable Q3_K unpacking
without an intermediate 512-byte block buffer. Any physical Dell
measurements need prior RemoteOps admission and independent model-wide
numeric verification.
