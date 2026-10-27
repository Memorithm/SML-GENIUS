# SML-GROK-NATIVE-A2 — Portable GPU complement to Native A1 (preregistration)

Status: PREREGISTERED (committed before any A2 code).
Owner authorization: « Dès que tu en as la possibilité, tu introduis l'assembleur
pour la gestion cpu et GPU ». Constraints: no mandatory NVIDIA software; only
the installed GPU driver may be vendor-specific; the model stays autonomous.

## A1 audit (#745, as merged on main `b9aefbd`)
- Default build: CPU only. Portable Rust, plus runtime-gated x86-64 POPCNT and
  AArch64 NEON CNT inline asm. No GPU code is compiled.
- `cuda-driver` feature (off by default, Linux only): it `dlopen`s
  `libcuda.so.1` at **runtime** and JIT-loads verbatim PTX. No CUDA Toolkit,
  runtime, cuBLAS or cuDNN is needed at build time, so NVIDIA is not mandatory.
- Gap: the **only** GPU path is NVIDIA (PTX). AMD, Intel, Apple and mobile GPUs
  have none.

## Question
Can the same three A1 word kernels (XOR3, Hamming, ternary partial on
independent `u32` lanes) run on a vendor-neutral GPU path? That path is
WGSL, compiled by `naga` to SPIR-V/Metal/DXIL inside `wgpu`, and executed via
Vulkan/Metal/DX12. Must it be bit-exact with the A1 CPU oracle
(`ptx_word_oracle`) and with the A1 CPU asm path, with no NVIDIA dependency?

## Design (frozen)
- New feature `portable-gpu` on `sml-native-kernels`, **default off**, using an
  optional `wgpu` 25 dependency (MSRV ≤ 1.89) plus `pollster`. It pulls in no
  vendor SDK. The only runtime requirement is a system GPU driver
  (Vulkan ICD / Metal / DX12).
- `portable_gpu::execute(op, a, b, c) -> Result<Vec<u32>, PortableGpuError>`
  uses the same layout and semantics as `cuda_driver::execute`. If no adapter
  is found it returns `Unavailable` and never fakes a GPU result.
- WGSL uses `countOneBits`, `^`, `&` and a bounds guard per invocation, with
  workgroup size 64.
- The adapter name and `device_type` are reported. A software adapter
  (`DeviceType::Cpu`, e.g. Mesa lavapipe/llvmpipe) is labelled
  `SOFTWARE_RASTERIZER`.

## Tests and gates
1. Oracle equality, three ways: for N = 65,537 lanes from SplitMix64 seed
   0xA2, plus edge words (0, `u32::MAX`, alternating, single bits), every lane
   must satisfy `portable_gpu == ptx_word_oracle`. The CPU asm
   `CpuKernels::detect()` must also equal `CpuKernels::portable()` on the u64
   packing of the same words.
2. The test is skipped (and loudly reported) only when no adapter exists. A
   dedicated CI job installs Mesa lavapipe and **must** run it, not skip it.
3. The default build of the crate and workspace compiles with no `wgpu` in the
   dependency graph of default features.
4. A1 contracts and the `--all-features` workflow stay green.

## Claims policy
- Lavapipe qualifies **correctness only**. Timings on a software adapter are
  printed and labelled, but no performance claim is made. A performance claim
  needs a physical AMD, Intel, Apple or NVIDIA GPU run, preregistered separately.
- If gate 1 fails on any adapter, the result is NO_GO for that adapter and is
  kept.

## Results
(Filled only from executed runs.)
