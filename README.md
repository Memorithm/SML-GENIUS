# SML-GENIUS

SML-GENIUS is Memorithm's research repository for **State-Memory Lattice (SML)**: an experimental, predominantly Boolean / low-bit autoregressive architecture intended to study whether large learned capacity can be separated from per-token active compute.

The long-range target is a 25-billion-parameter model with a storage-backed circuit atlas, bounded recurrent state, bounded associative memory and adaptive **recurrent depth**. The target inference envelope is **8 GiB RAM**, **2 GiB VRAM** and **at least 40 decoded tokens/s** on a future frozen reference machine.

These are engineering targets. No trained 25B SML model, throughput result, quality result or architectural-novelty claim exists yet.

## Non-attention candidate contract

The SML candidate core is designed without Q/K/V attention, QK^T, softmax token scoring, dense token-by-token score matrices, all-history Hamming/POPCOUNT ranking, or a Transformer-semantic KV cache. External attention models remain valid baselines.

## Core research hypothesis

Instead of traversing all learned parameters for every token, SML aims to keep most capacity cold and activate a small directly routed subset during each internal reasoning cycle:

    persistent token state + associative memory
                  |
                  v
          Boolean/algebraic routing
                  |
                  v
          selected atlas pages
                  |
                  v
           recurrent workspace
                  |
            repeat 1..R cycles
                  |
                  v
                COMMIT

Recurrent depth is separate from token recurrence: a single emitted token may use multiple internal reasoning cycles while reusing the same physical parameter atlas.

## Initial SML-25B representation hypothesis

| Physical class | Learned parameters | Packed bits/parameter |
|---|---:|---:|
| Boolean | 21.25B (85%) | 1 |
| Ternary | 2.50B (10%) | 2 |
| Residual low-bit | 1.25B (5%) | 4 |
| **Total** | **25B** | **31.25B bits total** |

The raw packed weights would therefore require exactly **3,906,250,000 bytes** before metadata and alignment. The split is an experimental starting hypothesis, not a frozen final architecture.

## Workspace

- **sml-core**: dependency-free deterministic reference semantics, exact counters, Boolean circuit oracle, direct routing and adaptive recurrent-depth runner.
- **sml-core::gf2**: bit-packed GF(2) vectors and deterministic linear transforms.
- **sml-core::anf**: canonical algebraic-normal-form / Zhegalkin relation evaluation.
- **sml-core::memory**: bounded direct associative memory with explicit miss, collision and replacement outcomes.
- **sml-core::atlas**: dense O(1) circuit-page manifests with exact metadata/alignment accounting.
- **sml-core::audit**: fail-closed non-attention execution-counter gate.
- **sml-core::circuit**: fixed-wiring acyclic circuits with jointly trainable hard Boolean gate identities.
- **sml-core::forge_topology**: strict `sml-topology/v1` parser and independent full-oracle destination requalification.
- **sml-core::trace**: immutable recurrent-cycle evidence tied directly to execution.
- **sml-core::tasks**: deterministic SML-1 development tasks with complete symbolic oracles.
- **sml-core::train**: continuous-latent training of hard two-input Boolean truth tables.
- **sml-cli**: executable contract plus SML-0.1 through SML-0.5 smoke probes.
- **docs/SML-0-SPEC.md**: executable SML-0 architecture boundary.
- **docs/SML-0.1-ALGEBRA-MEMORY-ATLAS.md**: reference algebra/memory/page semantics.
- **docs/SML-0.2-TRACE-TASKS.md**: recurrent trace and deterministic SML-1 task contract.
- **docs/SML-0.3-TRAINABLE-GATES.md**: first executable Boolean trainability contract.
- **docs/SML-0.4-TRAINABLE-CIRCUITS.md**: multi-gate acyclic trainability and representation accounting.
- **docs/SML-0.5-FORGE-REQUALIFICATION.md**: Forge search boundary and independent destination promotion gate.
- **docs/ROADMAP.md**: evidence-gated progression from reference primitives to 25B.
- **docs/PRIOR_ART.md**: initial prior-art anchors; explicitly incomplete.

## Build

    cargo fmt --all -- --check
    cargo check --workspace --all-targets
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo run -p sml-cli -- contract
    cargo run -p sml-cli -- sml01-smoke
    cargo run -p sml-cli -- sml02-smoke
    cargo run -p sml-cli -- sml03-smoke
    cargo run -p sml-cli -- sml04-smoke
    cargo run -p sml-cli -- sml05-smoke

## Scientific discipline

A generated connection is not counted as millions of learned parameters merely because it expands to many edges. Total learned parameters, packed physical bits, active parameters per recurrent cycle, total parameter touches per emitted token, memory traffic, recurrent depth and latency are reported separately.

SML advances through gates. Scaling from 50M to 300M to 1.5B to 7B to 25B is permitted only when the previous stage provides reproducible quality and systems evidence.

## License

PolyForm Noncommercial 1.0.0, aligned with SciRust. Commercial licensing requires a separate agreement. See LICENSE.md.
