# Native A2 — resident CUDA PTX and device event qualification (Thor)

27 October 2026. Research-only, no SML inference integration.

## A1 measured result and confounders

A1's three PTX bitplane kernels passed bit-exact Rust oracles for 1,031
words on real Thor GPU (workflow 40429253150). The original 204–418 ms
per-operation results include new CUDA context, PTX module JIT,
CUDA allocations, copies and device launches. They cannot be
interpreted as raw GPU kernel latency, speedups or tokens/s.

## A2 controls

- One same-thread GPU context and one PTX JIT per process;
  one set of four allocated u32 word buffers reused across all
  shapes, ops and successive input updates.
- Resolve three function handles (XOR3 LOP3; Hamming POPC; ternary
  partial signed POPC) once. No permanent service or idle GPU resident
  daemon: resources are freed deterministically when the object drops.
- Explicitly upload the complete active A/B/C slices. Reject invalid
  size/capacity before device writes. Reuse the context across
  different payloads and non-multiple-of-256 sizes
  (1, 31, 255, 256, 257, 1,031, 19).
- After a warmup device launch plus synchronization, record CUDA
  events on the default stream around exactly 512 repeated launches.
  The event interval excludes host/device copies, CUDA context and PTX JIT,
  but still includes device scheduling effects and cannot isolate
  single-instruction latency.
- Measure separately (CPU monotonic clock) cold context+JIT+
  allocations, H2D writes, host launch submission and D2H readback.
- Compare **every** returned word bit-exactly to the portable Rust
  oracle; no tolerance, no numerical approximation.
- Evaluate at 1,031 and 65,536 words; report raw timings and
  checksums without selecting a winner before measurements.
- Real GPU tests opt-in via `SML_NATIVE_GPU_TEST=1` and Linux
  `cuda-driver` feature. The GitHub-hosted PR CI only compiles
  and tests the host-side contract; it does not claim device tests.

## Deployment boundary

The only new low-level code lives in `sml-native-kernels`. The
`sml-core` and runtime crates remain unchanged, safe and unaffected.
No service runs when idle and no silent GPU fallback is allowed.
The existing A1 one-shot path remains intact as an independently
reproducible control. Do not merge without full workspace CI green
and no governance regression; run on Thor only after trusted main
integration, preserving the repository's persistent-runner policy.

## Reproduction

```bash
cargo fmt --all -- --check
cargo test --locked -p sml-native-kernels --all-features
cargo clippy --locked -p sml-native-kernels --all-features --all-targets -- -D warnings
SML_NATIVE_GPU_TEST=1 cargo test --locked -p sml-native-kernels --features cuda-driver
SML_NATIVE_GPU_TEST=1 cargo run --locked --release -p sml-native-kernels --features cuda-driver --example native_resident_gpu
```

Use Thor AArch64 and its actual NVIDIA GPU only for the last two
commands; do not substitute CPU oracle tests or cross compilation.

## Next admission gate

Promote a resident GPU path only after matched optimized CPU and
hardware GPU latency tests at realistic model tensor shapes including
memory transfer, resource limits, thermal/power envelope, and
routed SBG/MOR inference quality. A2 makes no such promotion.
