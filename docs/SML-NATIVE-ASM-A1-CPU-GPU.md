# Native Bitkernels A1 — CPU ISA + GPU PTX

27 October 2026. Research-only, opt-in, SML-GENIUS. No changes to core
logic, learned weights, inference, MOR routing or SBG semantics.

## Scope and selection policy

Introduce assembly *where a specific instruction replaces or accelerates
measured Boolean operations*; do **not** mechanically rewrite Rust into
assembly. Rust LLVM already emits native POPCNT or vector instructions
for common expressions, so assembly has no assumed superiority.

| Backend | ISA / implementation | Ops | Admission |
|---|---|---|---|
| CPU all | ordinary optimized Rust | bit count, GF2 dot parity, Hamming, ternary partial | mandatory oracle |
| x86-64 | runtime-gated POPCNT via core::arch::asm | same | test + actual faster throughput |
| AArch64 | runtime-gated NEON CNT/ADDV/UMOV via core::arch::asm | same | cross-check + Thor/Apple silicon physical benchmark |
| NVIDIA GPU | native PTX LOP3, POPC, XOR, AND | XOR3, Hamming, signed ternary bitplane partial | actual GPU JIT + bit-exact runtime run |
| AMD GPU | GCN/RDNA ISA backend not yet implemented | — | future backend after actual hardware |
| Apple GPU | Metal compute shader/driver pathway, NOT arbitrary user PTX | — | future backend after actual device tests |

AArch64 is a CPU ISA and NEON does not execute on Apple GPU. GPU
assembly is vendor- and architecture-specific. PTX is NVIDIA's
virtual assembly language, not SASS hardware instructions.
Hardware kernel binaries, cache policy, occupancy, register pressure,
data transfer and power need their own measurements.

Existing Rust reference crates remain `#![forbid(unsafe_code)]`;
the new `sml-native-kernels` crate **deliberately and exclusively**
isolates auditable inline assembly and optional driver FFI. There is
no implicit unsafe execution in `sml-core` or `sml-runtime`.

## Exact kernels and safety

- `CpuKernels::detect()`: CPUID-gated POPCNT on x86-64,
  feature-gated NEON byte population counts on AArch64, otherwise
  portable Rust. The CPU path is selected once per kernel object.
- `gf2_dot_parity`: `popcount(a & b) & 1`.
- `hamming`: `popcount(a ^ b)`.
- `ternary_partial`: `popcount(x & plus) - popcount(x & minus)`.
  For admissible ternary weights, plus/minus planes do not overlap.
  The kernel deliberately supports arbitrary masks as an exact
  primitive; signed range per word is [-64,64].
- PTX GPU kernels operate on independent `u32` lanes, using a
  bounds guard per thread. `xor3` maps exactly to
  `lop3.b32` truth table 0x96; the other two use `popc.b32`.
  All three accept matching-length A/B/C slices and write one u32
  per lane; signed partial results have i32 two's-complement bits.
- Optional Linux CUDA Driver API adapter resolves `libcuda.so.1` and
  symbols dynamically. **CUDA Toolkit, cuBLAS, cuDNN, CUDA Runtime
  and other NVIDIA software stacks are not dependencies.** Driver API
  creates context, JIT-loads PTX, allocates and copies memory, runs,
  synchronizes, reads back, then releases allocations, module, context.
  Failure returns an error; it never claims a fake GPU execution.

## Validation and physical evidence

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-native-kernels --all-features --all-targets
cargo clippy --locked -p sml-native-kernels --all-features --all-targets -- -D warnings
cargo test --locked -p sml-native-kernels --all-features
cargo run --locked --release -p sml-native-kernels --example native_bitkernels
```

Cross-compile syntax on a CI host with Rust AArch64 std installed:

```bash
rustup target add aarch64-unknown-linux-gnu
cargo check --locked -p sml-native-kernels --all-features --target aarch64-unknown-linux-gnu
```

Exact GPU device execution **only** with a real NVIDIA GPU + driver:

```bash
SML_NATIVE_GPU_TEST=1 cargo test --locked -p sml-native-kernels --features cuda-driver
SML_NATIVE_GPU_TEST=1 cargo run --locked --release -p sml-native-kernels --features cuda-driver --example native_bitkernels
```

GitHub-hosted CI without an NVIDIA device compiles the driver and
checks PTX source invariants and CPU semantics; it does **not**
prove the PTX device code JITs or runs. These must be qualified on
Jetson Thor or equivalent before enabling a runtime dispatch.
A test launch uses 1,031 words, including a partial final thread
block. The GPU benchmark includes context, JIT and host-device
transfer overhead; it must not be mistaken for steady-state kernel
latency or tokens/s.

Report `arch`, operating system, host CPU, instruction feature,
input sizes, timing repetitions, compiler/Rust version, exact
output checksums, kernel-only GPU events if available, device type,
driver version and transfer timings. Compare with optimized Rust,
not an intentionally slow scalar baseline. If no measurable win,
keep the Rust fallback and leave assembler experimental.

## Go/no-go gates

1. Exact equivalence to independently computed portable oracle for
   boundary values and at least 65,536 input cases, on each target.
2. No unchecked CPU instruction execution, no unsupported GPU JIT,
   no silent emulation of device results.
3. Measured practical throughput/latency improvement on actual target
   hardware and no unacceptable memory or energy regression.
4. Report compilation/dispatch costs, PTX JIT, PCIe/UMA copy costs.
5. No changes to model meaning, training, SBG storage conventions,
   MOR admission, research holdouts or approval/governance.
6. Other projects SciRust and FLAT-ATTENTION remain **independent**.

The CPU/GPU bit kernels are intentionally NOT connected to the
SML-GENIUS model runtime by this first research slice.
