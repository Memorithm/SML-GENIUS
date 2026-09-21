# SBG-0.12 ARM64 qualification evidence

SBG-0.12 qualifies the safe packed semantic kernel on the current self-hosted
AArch64 reference runner. It does not establish x86 AVX2/AVX-512 behavior or a
cross-machine performance distribution.

## Qualified implementation

- scalar bit-by-bit semantic oracle;
- packed `u64` XOR/AND/POPCOUNT kernel;
- exact equivalence at 16,384 logical bits and at a non-word-aligned width;
- reproducible release-mode benchmark;
- fixed-width release code-generation audit;
- crate-wide `#![forbid(unsafe_code)]` retained.

## ARM64 evidence

Source: GitHub Actions run `35645198631`, PR #55, runner
`tarek-scirust-arm64-01`.

Toolchain:

- target: `aarch64-unknown-linux-gnu`;
- rustc: 1.89.0;
- LLVM: 20.1.7.

Measured 16,384-bit comparison, after 32 warm-up iterations and over 512
measured iterations:

- scalar oracle: 49,765 ns/iteration;
- packed-u64 kernel: 2,103 ns/iteration;
- observed scalar/packed ratio: 23.653816x for this run.

The timing is one run and is not treated as a stable speedup distribution.

The exact audited function body contained 286 assembly lines and the audit
reported:

- AArch64 `cnt` instructions: 10;
- `addv`/horizontal reduction family: 5;
- vector-register mentions: 43;
- XOR family: 5;
- AND family: 6;
- direct vector-load patterns counted by the audit: 0.

The assembly includes vector forms such as `cnt v*.16b` and vector reduction
operations. This is direct code-generation evidence that the safe packed-u64
implementation receives AArch64 vector operations from LLVM on this runner.

## Decision

No explicit `std::arch`/NEON intrinsic layer is introduced for the current
AArch64 path. The safe Rust implementation is retained. Architecture-specific
specialization may be revisited only if later measurements show a remaining
bottleneck or a new reference platform requires separate qualification.
