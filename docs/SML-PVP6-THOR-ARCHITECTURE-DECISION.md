# PVP6 — Thor architecture decision

Status: integrated model-owned selector and exact executor, diagnostic-only.
This document does not authorize an attention-replacement, model-quality or
production-speed claim.

## Evidence identity

The policy is derived from the native Thor campaign completed by the exact
main head used for the corrected run:

- workflow run: [37753037098](https://github.com/Memorithm/SML-GENIUS/actions/runs/37753037098)
- artifact: [pvp6-thor-native-evidence](https://github.com/Memorithm/SML-GENIUS/actions/runs/37753037098/artifacts/11541367312)
- artifact digest:
  sha256:adf421f5cd9b518e55e38a3c0c398a6812fcf6ef883003a71628ee01933034eb
- protocol: sml-pvp6-anf-fixture/v1
- host architecture: aarch64
- campaign source revision: 15576861eaae652d012b32abef4bd5dd3e651d5a
- fixture SHA-256:
  feb1a144e23d9a29439b7cad6704f56637a7f7ead1a9b9abb39ad9c0ac5325fd

The corrected run also performed the SciRust structural-admission check in the
SciRust workspace with a private Cargo cache and failed closed on command
errors. The native campaign completed ten processes, with 2,000 measured
iterations and 500 warmups per process, exact outputs, and zero mismatches.

## Observed decision

Across the 20 declared cells, Pascal ANF materialization was the fastest
non-resident arm in 17 cells. The three measured exceptions were all the
low-query cell with density 4, query count 32, and reuse 1:

| Population profile | Selected arm |
| --- | --- |
| affine-uniform | sparse ANF scan |
| high-popcount | sparse ANF scan |
| low-popcount | submask coefficient query |

The resident truth-table arm is a lower-bound/control measurement. Generic truth
materialization was not selected.

## Runtime policy

sml_core::pascal_vector_projection::Pvp6Workload::select implements exactly
that conservative rule:

1. For density 4, query count 32, reuse 1, select sparse for affine-uniform
   and high-popcount inputs, and submask for low-popcount inputs.
2. For every other measured cell, select Pascal ANF materialization.
3. For an unmeasured workload, keep Pascal as the conservative default and mark
   the selection as unmeasured.
4. Every selection remains marked diagnostic_only.

The policy does not require CUDA, NVRTC, cuDNN, TensorRT, NVML, CUTLASS or a
vendor SDK. It is a model-owned Rust contract; SciRust, FLAT-ATTENTION, NNIS
and RemoteOps remain qualification or control-plane partners.

## Model-owned executor

The merged follow-up executor is exposed by
sml_core::pvp6_executor::Pvp6Bank. It accepts canonical ANF monomial masks,
constructs the packed coefficient bitplane, applies the selected arm, and
returns gate-major packed query outputs. Pascal materialization uses the
existing scalar exact Pascal-GF(2) primitive; sparse and submask paths execute
their direct Boolean definitions. All three paths are checked against an
independent oracle in Rust unit tests.

This is semantic internalization only. It does not introduce a mandatory
SciRust, FLAT-ATTENTION or NNIS runtime dependency and it does not turn the
Thor diagnostic into a production performance claim.

## Limits

This evidence is a synthetic shared-CPU diagnostic. It validates exact
functional output for the fixture and supplies a Thor-local architecture
decision; it does not prove portable hardware speed, model quality, register
allocation, or production promotion. A separate model-aligned experiment and
self-contained harness are required before changing the promotion boundary.
