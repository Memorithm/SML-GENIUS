# SML Pascal Vector Projection (PVP) hardware-alignment bootstrap

Status: architecture/research bootstrap. No model-runtime promotion is authorized.

## Goal

Preserve the exact Pascal/subset-zeta and ANF semantics retained by PB-6..PB-9,
while compiling their execution into hardware-regular contiguous bitplanes rather
than pointer-based lattice/graph traversal.

The primary candidate is an address-major, gate-bitplane layout:

- CPU logical storage: `[K, ceil(G/64)] u64`;
- portable GPU storage unit: `[K, ceil(G/128)] vec4<u32>`;
- inner dimension: contiguous banks/gates;
- transform: deterministic staged XOR butterfly;
- no adjacency lists, graph nodes or pointer chasing in the hot path.

Here `K` is the Boolean-domain width and `G` the number of ANF gates/banks.

## Model sovereignty

SML-GENIUS is the model. The final model must be sufficient to execute its
declared model semantics without FLAT-ATTENTION, SciRust or NNIS being mandatory
runtime dependencies.

External repositories are research, qualification and reusable-primitive
partners:

- SciRust: reusable bitplane/transpose/pack primitives, CPU SIMD capability
  dispatch and shared reference/oracle work;
- FLAT-ATTENTION: WGPU/WGSL kernel realization, tiling, subgroup candidates,
  workgroup staging and device autotuning;
- NNIS: optional portable CPU/WGPU runtime carrier and end-to-end hardware
  qualification;
- ElasticXxx: later selection only among already-qualified execution plans.

A winning primitive may be internalized into SML only after a versioned contract,
independent oracle equivalence, provenance/license review and destination
requalification.

## Hardware sovereignty

For the new SML/PVP path, the target is ecosystem-first and vendor-neutral.

Allowed vendor-specific layer:

- the installed GPU driver needed by the selected open GPU backend.

Not allowed as required SML/PVP dependencies at this stage:

- CUDA;
- NVRTC;
- cuDNN;
- TensorRT / TensorRT-LLM;
- NVML;
- CUTLASS;
- CUBIN-specific runtime contracts;
- project-authored C/C++ vendor-SDK bridges.

Existing historical/reference vendor-specific code in other repositories does not
authorize its use on the SML/PVP path.

## Execution programme

1. PVP-0: freeze address-major bitplane layout, pack/unpack/transpose and scalar
   exact oracle.
2. PVP-1: qualify SciRust scalar-u64, NEON, AVX2, AVX-512 and SVE candidates
   against the same oracle.
3. PVP-2: qualify FLAT portable WGSL `u32/vec4<u32>` butterfly.
4. PVP-3: test fused stages, workgroup tiling, subgroup and double-buffer
   candidates without changing semantics.
5. PVP-4: execute the same versioned layout through NNIS portable CPU/WGPU
   sessions.
6. PVP-5: compare backends under exact input/output identity and real-device
   measurements; no microbenchmark-only promotion.
7. PVP-6: run a prospectively declared massive-ANF-bank model-aligned study.
8. PVP-7: internalize only the minimum winning execution contract required by
   the self-contained SML model.
9. PVP-8: after the model interface is frozen, define a separate SML-HARNESS
   repository/contract.

## Harness boundary

The future SML-HARNESS is not the model. It may orchestrate training,
evaluation, serving experiments, datasets, benchmarks and external tools, but it
must not provide hidden model semantics required for normal SML execution.

The model must remain usable without the harness.

## Evidence boundary

PB-9 proves exact large-width CPU packed execution and one bounded task-utility
surface. It does not prove SIMD/GPU acceleration, hardware-independent speedup,
attention replacement or model quality. Each such claim requires its own
prospectively frozen experiment and exact-head hardware evidence.
