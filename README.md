# SML-GENIUS

SML-GENIUS is Memorithm's research repository for **State-Memory Lattice (SML)**: an experimental, predominantly Boolean / low-bit autoregressive architecture intended to study whether large learned capacity can be separated from per-token active compute.

The long-range target is a 25-billion-parameter model with a storage-backed circuit atlas, bounded recurrent state, bounded associative memory and adaptive **recurrent depth**. The target inference envelope is **8 GiB RAM**, **2 GiB VRAM** and **at least 40 decoded tokens/s** on a future frozen reference machine.

These are engineering targets. No trained 25B SML model, throughput result, quality result or architectural-novelty claim exists yet.


## Model and Harness boundary

`SML-GENIUS` is the model programme. The **Model** is the learned artifact plus
the inference-time mechanisms required to execute it: SBG, MOR, sparse
expert/circuit activation, associative memory, recurrent state transitions,
Elastic Weight runtime semantics and native output/decision heads.

The future **SML-HARNESS** is a separate repository. It will orchestrate
self-improvement, training, architecture/code mutation, evaluation,
promotion/rollback, deployment and the independently evolving Memorithm
components required by those workflows.

The first real 25B SML model is the bootstrap champion, not a permanent maximum.
After that milestone, the champion may autonomously use SML-HARNESS to create,
train and qualify larger descendants. Later generations are promoted only when
measured capability improves without violating active-compute, memory, traffic,
robustness and reproducibility gates.

See [docs/SML-MODEL-HARNESS-BOUNDARY.md](docs/SML-MODEL-HARNESS-BOUNDARY.md).

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
- **sml-core::delta_kv**: research-only bounded signed-Q12 rewritable associative matrix with normalized delta correction, exact bit accounting and explicit interference/saturation evidence.
- **sml-core::routing_train**: budgeted latent-bit training that freezes into the existing O(1) direct page router.
- **sml-core::elastic_weights**: transactional precision/residency/activation planning for elastic weight pages with exact RAM/VRAM and active-set accounting.
- **sml-core::semantic**: Semantic Boolean Geometry (SBG) packed V/M/A/R state and integer-only Boolean semantic primitives.
- **sml-core::semantic::mor_hierarchical**: bounded coarse-to-fine SBG/MOR candidate narrowing with exact candidate-work accounting; experimental, not a quality/latency claim.
- **sml-core::semantic::mor_adaptive_hierarchy**: adaptive 1 -> 2 -> 4 MOR-H2 widening from observable routing/ambiguity signals with cumulative work budgets and an exhaustive research control.
- **sml-core::mor_specialization**: MOR-H3 exposure-conditioned inter/intra-cluster balancing with neutral first exposure and bounded specialization-preserving adjustments.
- **sml-core::semantic::mor_hierarchy_train**: MOR-H4a bounded training-only search for cluster prototypes with frozen expert memberships and a separate development gate.
- **sml-core::semantic::mor_hierarchy_majority**: matched H4a majority-bit control with weak-bit seed retention, hard mutation budgets and the same development gate shape.
- **sml-core::semantic::mor_h4a_qualification**: matched fixed/learned/majority/random prototype controls over the same frozen train/development split.
- **sml-core::task_loss**: task-answer training for hard Boolean conjunction and overwrite-state micro-operations plus held-out panel evaluation.
- **sml-core::atlas**: dense O(1) circuit-page manifests with exact metadata/alignment accounting.
- **sml-core::audit**: fail-closed non-attention execution-counter gate.
- **sml-core::candidate**: integrated SML-1 pilot combining learned hard event routing, injectable learned Boolean micro-operations, bounded memory and per-event evidence.
- **sml-core::circuit**: fixed-wiring acyclic circuits with jointly trainable hard Boolean gate identities.
- **sml-core::forge_topology**: strict `sml-topology/v1` parser and independent full-oracle destination requalification.
- **sml-core::trace**: immutable recurrent-cycle evidence tied directly to execution.
- **sml-core::tasks**: deterministic SML-1 development tasks with complete symbolic oracles.
- **sml-core::train**: continuous-latent training of hard two-input Boolean truth tables.
- **sml-cli**: executable contract, SML-0.x probes, and SML-1.0 through SML-1.2 candidate qualification smokes.
- **docs/SML-0-SPEC.md**: executable SML-0 architecture boundary.
- **docs/SML-0.1-ALGEBRA-MEMORY-ATLAS.md**: reference algebra/memory/page semantics.
- **docs/SML-0.2-TRACE-TASKS.md**: recurrent trace and deterministic SML-1 task contract.
- **docs/SML-0.3-TRAINABLE-GATES.md**: first executable Boolean trainability contract.
- **docs/SML-0.4-TRAINABLE-CIRCUITS.md**: multi-gate acyclic trainability and representation accounting.
- **docs/SML-0.5-FORGE-REQUALIFICATION.md**: Forge search boundary and independent destination promotion gate.
- **docs/SML-0.6-LATENT-DIRECT-ROUTING.md**: budgeted learned route bits and hard O(1) deployment contract.
- **docs/SML-0.7-ELASTIC-WEIGHTS.md**: deterministic elastic precision, residency, activation and plan/commit boundary.
- **docs/SML-1.0-TASK-CANDIDATE.md**: first integrated learned-routing task pilot and evidence boundaries.
- **docs/SML-1.1-TASK-LOSS-GATE.md**: first micro-operation learned from task answers with explicit semantic controls.
- **docs/SML-1.2-STATE-UPDATE.md**: overwrite-state transition learned from task answers with complete state/input coverage.
- **docs/RESULTS-A5-TOY.md**: first A5 learning prototype (local fan-in ANF recurrent model, gradient-free search) vs baselines on toy sequence tasks; diagnostic only.
- **docs/ROADMAP.md**: evidence-gated progression from reference primitives to 25B.
- **docs/SML-DELTA-KV-BOOTSTRAP.md**: evidence-gated bounded rewritable-memory programme and matched controls.
- **BOOTSTRAP.md**: durable project re-entry state, including the SBG roadmap and current implementation slice.
- **docs/PRIOR_ART.md**: initial prior-art anchors; explicitly incomplete.

## Build

Enable the versioned git hooks once per clone (pre-commit: rustfmt check; pre-push: fmt, clippy `-D warnings`, tests); see [CONTRIBUTING.md](CONTRIBUTING.md):

    bash scripts/install-hooks.sh

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
    cargo run -p sml-cli -- sml06-smoke
    cargo run -p sml-cli -- sml07-smoke
    cargo run -p sml-cli -- sml10-smoke
    cargo run -p sml-cli -- sml11-smoke
    cargo run -p sml-cli -- sml12-smoke

## Scientific discipline

A generated connection is not counted as millions of learned parameters merely because it expands to many edges. Total learned parameters, packed physical bits, active parameters per recurrent cycle, total parameter touches per emitted token, memory traffic, recurrent depth and latency are reported separately.

SML advances through gates. Scaling from 50M to 300M to 1.5B to 7B to 25B is permitted only when the previous stage provides reproducible quality and systems evidence.

## License

PolyForm Noncommercial 1.0.0, aligned with SciRust. Commercial licensing requires a separate agreement. See LICENSE.md.
