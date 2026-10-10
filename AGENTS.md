# SML-GENIUS Agent Bootstrap Contract

Before autonomous coding, architecture search, training and evaluation, cross-repository execution, task-scoped capability work, workspace materialization, resource-budget changes, sandboxing, checkpoint/resume, PR creation or merge decisions, read:

```bash
git fetch origin agent/ecosystem-roadmap && \
git show origin/agent/ecosystem-roadmap:.agent/SML_GENIUS_ECOSYSTEM_ROADMAP.yaml
```

If the roadmap cannot be read, fail closed for major architectural, security, experimental-promotion or merge decisions. Read-only diagnosis remains allowed.

## AX-inspired task execution

Google AX is an architectural reference only; it is not a runtime dependency. Memorithm uses Rust-native ownership boundaries: SciRust Hub owns task identity/lifecycle/workspace provenance and backend admission; RemoteOps owns concrete host/runtime enforcement; ElasticXxx owns adaptive resource policy; SML-GENIUS retains its own scientific and model authority.

A supervised process is not a hostile-code sandbox. Task success does not upgrade model quality, scientific evidence, novelty or authorization. Required CI must be green on the exact PR head before merge.


## SML hardware sovereignty and PVP

Before any Pascal/ANF hardware-projection, massive Boolean-bank, SIMD/GPU execution or cross-repository kernel work, read `docs/SML-PASCAL-PVP-HARDWARE-ALIGNMENT-BOOTSTRAP.md` and the current off-main SML ecosystem roadmap.

For the new SML/PVP path, only the installed GPU driver may be vendor-specific. CUDA, NVRTC, cuDNN, TensorRT/TensorRT-LLM, NVML, CUTLASS and vendor-SDK bridges are not required dependencies. SciRust, FLAT-ATTENTION and NNIS are qualification/reusable-primitive partners; the final SML model remains self-sufficient and the future SML-HARNESS remains a separate surface.

## Local gates before every push

Run `bash scripts/install-hooks.sh` once per clone. The versioned `.githooks/pre-push` hook runs `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings` and `cargo test --locked --workspace`; never push with `--no-verify`. See CONTRIBUTING.md.
