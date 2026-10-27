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
Run [40510929412](https://github.com/Memorithm/SML-GENIUS/actions/runs/40510929412), source `f5918f9`
(main, merge of #751), trusted-main push, `SML_PORTABLE_GPU_REQUIRED=1` + `SML_PORTABLE_GPU_REQUIRE_HARDWARE=1`.
Runner groups admitted by RemoteOps PR #883 (Thor-General bootstrap run 40510611861, Dell-General run 40510745878).

| Host | Job | Driver evidence | wgpu adapter (name / backend / kind) | Oracle 65,537 lanes x 3 ops | Verdict |
|---|---|---|---|---|---|
| Dell T430 (`debian`, x86_64, 6.12.88+deb13) | [122212323680](https://github.com/Memorithm/SML-GENIUS/actions/runs/40510929412/job/122212323680) | `nvidia-smi`: GPU 0 NVIDIA GeForce RTX 4060; Vulkan 1.4.329, DISCRETE_GPU, NVIDIA 595.45.04 | `NVIDIA GeForce RTX 4060` / Vulkan / DiscreteGpu | bit-exact; 7/7 crate tests | **PASS** |
| Jetson Thor (`tarek`, aarch64, 6.8.12-tegra) | [122212324623](https://github.com/Memorithm/SML-GENIUS/actions/runs/40510929412/job/122212324623) | `nvidia-smi`: GPU 0 NVIDIA Thor; Vulkan 1.4.315, INTEGRATED_GPU, NVIDIA 580.00 | `NVIDIA Tegra NVIDIA Thor` / Vulkan / IntegratedGpu | bit-exact; 7/7 crate tests | **PASS** |

Both hosts also expose llvmpipe; the hardware gate confirms wgpu selected the NVIDIA device, not the software adapter.
Default build had no wgpu on both hosts. No NVIDIA SDK installed by the workflow (vendor driver Vulkan ICD only).
H1 supported on 2 NVIDIA devices (Vulkan). **Not claimed:** performance; Metal/DX12; AMD/Intel/Apple GPUs (UNTESTED).
Negatives kept: Deviation 1 (run 40499157718 never scheduled, cancelled); PR #751 cloud checks did not start (GitHub
billing message, not code failures) and were merged per owner instruction to rely on self-hosted RemoteOps runners.
Software baseline: lavapipe CI job 121954525450 (PR #748), correctness only.

### Deviation 1 (recorded before any device result)
Run [40499157718](https://github.com/Memorithm/SML-GENIUS/actions/runs/40499157718) (merge `1ba478e`)
stayed queued ~1h and was cancelled by us, with all four RemoteOps runners idle. Jobs: Thor 122172173346
(`self-hosted,Linux,ARM64,remoteops,thor`), Dell 122172173738 (`self-hosted,Linux,X64,remoteops,dell-t430`).
Cause: the `remoteops`-labelled runners are RemoteOps-repo control runners (per RemoteOps
`docs/runner-fleet-architecture.md` they must never run general repository code). The documented build plane is
the org runner groups `Thor-General` (`memorithm-thor-heavy-01`, labels `memorithm-ci,thor-general,ci-heavy,trusted-main`,
host `tarek`) and `Dell-General` (`memorithm-dell-heavy-*`, labels `memorithm-ci,dell-general,ci-heavy,trusted-main`,
host `debian`), both `restricted_to_workflows`. Change: target those labels with a hostname guard, and admit this
workflow in the groups via RemoteOps PR #883. Method/criteria otherwise unchanged.
