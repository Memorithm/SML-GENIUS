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
