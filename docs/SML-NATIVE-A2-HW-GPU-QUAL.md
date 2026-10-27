# SML-NATIVE-A2-HW — portable-gpu hardware GPU qualification (preregistration)

Status: PREREGISTERED (written and committed before any qualification workflow or
result-producing code). Scope: feature `portable-gpu` of `sml-native-kernels`
(PR #748, merge `4b45254`, head `d457c9b`). Results are appended below only after execution.

## Question / hypothesis
H1: on real (non-software) GPUs reachable through Memorithm/RemoteOps self-hosted runners,
the WGSL kernels (`wgsl/boolean_words.wgsl`, via wgpu 25) produce output **bit-identical** to
`ptx_word_oracle` for Xor3, Hamming and TernaryPartial on the same 65,537 frozen lanes
used by `portable_gpu_is_bit_exact_with_oracle`.

## Targets (discovered, not assumed)
Org runners (GitHub API `actions/runners`, 2026-10-27):
- `thor-remoteops-arm64-01` labels `self-hosted,Linux,ARM64,remoteops,thor,jetson-thor` — NVIDIA Jetson Thor (integrated GPU; already used by `native-ptx-thor-device.yml`).
- `dell-t430-remoteops-x64-01` labels `self-hosted,Linux,X64,remoteops,dell-t430` — RemoteOps README records NVIDIA GeForce RTX 4060 (8188 MiB).
- Not targeted here: `mac-mini` (Apple GPU / Metal) and `centre-mercure` (Windows business host). Mac-mini is a candidate for a later Metal run.

Both targets are NVIDIA hardware; the path under test is **Vulkan through the vendor driver's ICD**, not CUDA.
No NVIDIA SDK/toolkit is installed or required by the workflow; if no Vulkan ICD exists the result is recorded as BLOCKED.

## Method (frozen)
- Trusted `main` only, `workflow_dispatch` (+ push on the workflow file), never PR code on persistent hosts (same policy as `native-ptx-thor-device.yml`).
- Pinned Rust 1.89.0, isolated `CARGO_HOME`/`CARGO_TARGET_DIR` in `RUNNER_TEMP`.
- Command: `cargo test --locked -p sml-native-kernels --features portable-gpu -- --nocapture`
  with `SML_PORTABLE_GPU_REQUIRED=1` and `SML_PORTABLE_GPU_REQUIRE_HARDWARE=1`.
- New gate (test-only): with `SML_PORTABLE_GPU_REQUIRE_HARDWARE=1`, the test fails if the adapter kind is
  `SOFTWARE_RASTERIZER` (prevents lavapipe/llvmpipe on the host from being counted as hardware).
- Recorded environment: hostname, arch, kernel, `vulkaninfo --summary` (if present), `nvidia-smi -L` (if present), adapter name/backend/kind printed by the test.
- Deterministic inputs → a single execution per device is the unit; no seeds apply. Each host is one independent job.

## Success / failure criteria
- PASS(device): adapter kind ≠ SOFTWARE_RASTERIZER AND all 3 ops × 65,537 lanes equal oracle AND rest of crate tests pass.
- FAIL(device): any lane mismatch or test failure on a hardware adapter → NO_GO for that device, kept.
- BLOCKED(device): no hardware adapter (no Vulkan ICD, only software adapter, or runner unavailable) → recorded with logs, not counted as pass or fail of H1.
- Default build must stay free of wgpu; A1 / `cuda-driver` paths untouched.

## Not claimed
- No performance claim of any kind (timings, speedups) — would need a separate prereg.
- No claim for Metal/DX12/AMD/Intel unless actually executed.
- Lavapipe CI (job 121954525450 on PR #748) remains correctness-on-software only.

## Results
(pending execution)

### Deviation 1 (recorded before any device result)
Run [40499157718](https://github.com/Memorithm/SML-GENIUS/actions/runs/40499157718) (merge `1ba478e`)
stayed queued ~1h and was cancelled by us, with all four RemoteOps runners idle. Jobs: Thor 122172173346
(`self-hosted,Linux,ARM64,remoteops,thor`), Dell 122172173738 (`self-hosted,Linux,X64,remoteops,dell-t430`).
Cause: the `remoteops`-labelled runners are scoped to Memorithm/RemoteOps and are not available to SML-GENIUS. The
only runner SML-GENIUS uses is `tarek-scirust-arm64-01` (labels `self-hosted,Linux,ARM64`, host `tarek` = Thor;
e.g. run 40467876591). Change: Thor job uses those labels + hostname guard; Dell is dropped.
**Dell RTX 4060 = BLOCKED** (no SML-GENIUS runner). To enable: register a Dell runner for SML-GENIUS
or grant the RemoteOps runner group access to SML-GENIUS. Method/criteria otherwise unchanged.
