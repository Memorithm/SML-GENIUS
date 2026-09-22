# SML-GENIUS bootstrap

This file is the durable re-entry point for continuing the SML-GENIUS research programme from the current repository state. Read it together with `README.md`, `docs/ROADMAP.md`, the latest merged pull requests, and the executable contracts in `sml-core`.

## Current architectural direction

SML is an independent, predominantly Boolean / low-bit autoregressive architecture. It is not defined by reference to Transformer internals. Its native mechanisms include direct routing, Mixture-of-Routers / sparse expert selection, bounded associative memory, recurrent depth, Elastic Weights, and Boolean-integral learning experiments.

Native decision capability is a first-class parallel objective. A substantial decision-expert family and dedicated training objectives must be evaluated before SML-2, not bolted onto a finished language model. Direct decisions and bounded recurrent refinement must work without a text decoder; language generation is optional. See `docs/SML-D-BOOTSTRAP.md`. Total decision capacity, active work per decision and direct-decision coverage are separate quantities; no arbitrary 25B percentage is frozen.

The new semantic representation programme is **Semantic Boolean Geometry (SBG)**.

SBG treats meaning as a context-sensitive Boolean state rather than as a fixed real-valued embedding.

The reference semantic state is:

```text
SemanticState = (V, M, A, R)

V = Semantic Value Field
M = Context Meaning Mask
A = Ambiguity Field
R = Routing Field
```

The initial experimental width is 16,384 bits per field. Four fields therefore represent 65,536 logical bits = 8,192 packed payload bytes before container metadata. The width is a benchmark hypothesis, not a frozen architectural constant.

Core invariants:

- packed machine-word representation, initially `u64`;
- no `f32`/`f64` dependency in the SBG core;
- fail-closed dimensional/padding validation;
- allocation-free in-place Boolean primitives after construction;
- XOR/XNOR/AND/OR/NOT/POPCOUNT as primary operations;
- context-dependent masked similarity rather than one immutable semantic metric;
- ambiguity preserved explicitly instead of forcing premature single-sense collapse;
- compositional binding/bundling/permutation;
- routing field feeds native MOR selection;
- recurrent depth rewrites semantic state until a declared stability criterion;
- meaning/routing masks may drive Elastic Weight page activation;
- learning work must test discrete Boolean evidence/credit mechanisms rather than assume gradient descent is required.

## Versioning reconciliation

The repository already uses `SML-0.7` for Elastic Weights and has merged later `SML-0.8.x` MOR / Boolean-integral work. The earlier conversational SBG roadmap used overlapping SML-0.x labels.

Do **not** reuse those occupied SML labels.

Implementation uses the independent **SBG-0.x** slice namespace while preserving the same roadmap content. Native decision work uses **SML-D**; connectomic structural priors use **CSP**.

## Semantic Boolean Geometry implementation roadmap

- **SBG-0.1 — Semantic BitField Core**
  - const-generic logical dimension;
  - packed `u64` storage;
  - exact padding validation;
  - V/M/A/R `SemanticState`;
  - XOR/XNOR/AND/OR/NOT/POPCOUNT primitives;
  - allocation-free in-place operations;
  - deterministic unit tests.

- **SBG-0.2 — Boolean Semantic Metric**
  - Hamming distance;
  - XNOR similarity;
  - masked Hamming / masked XNOR;
  - integer-only normalization/reporting where needed;
  - prove that context masks change semantic neighbourhoods.

- **SBG-0.3 — Binding / Bundling / Permutation**
  - deterministic role binding and inverse/unbinding;
  - majority bundling;
  - reversible/qualified permutation;
  - collision and retrieval measurements.

- **SBG-0.4 — Context Meaning Mask**
  - derive/update M from current semantic context;
  - test context-dependent metric deformation;
  - explicit finance/river-style polysemy fixtures without language-specific claims beyond the fixture.

- **SBG-0.5 — Ambiguity Field**
  - represent unresolved/conflicting coordinates;
  - track ambiguity increase/decrease through context;
  - do not require monotonic decrease.

- **SBG-0.6 — Semantic Composition**
  - native role/value composition;
  - distinguish subject/action/object permutations;
  - preserve semantic relatedness while retaining relational differences.

- **SBG-0.7 — SBG to MOR Routing**
  - route from R against expert/router signatures;
  - integer XNOR/POPCOUNT scoring;
  - evaluate Top-1/Top-2/Top-4 sparse selection;
  - integrate with the existing native MOR runtime rather than create an external dependency.

- **SBG-0.8 — SBG Recurrent Depth**
  - semantic transition `S(k+1)=F(S(k), expert, memory)`;
  - exact V/M/A/R delta accounting;
  - stability halting based on Boolean change;
  - compare fixed versus adaptive depth.

- **SBG-0.9 — SBG to Elastic Weights**
  - map semantic/routing signatures to weight-page regions;
  - activate only selected page groups;
  - preserve transactional PLAN/VALIDATE/COMMIT semantics;
  - measure working-set RAM/VRAM and storage traffic.

- **SBG-0.10 — Boolean Evidence Learning**
  - discrete evidence counters;
  - KEEP/SET/CLEAR/FLIP/MASK/UNMASK/BIND/SPLIT/MERGE/ROUTE candidate decisions;
  - held-out commit gating;
  - integrate with existing Boolean-integral credit machinery where semantics align.

- **SBG-0.11 — Linguistic Micro-Benchmark**
  - synonym/antonym controls;
  - role reversal;
  - negation;
  - simple reference/correspondence;
  - controlled polysemy;
  - composition;
  - associative retrieval.

- **SBG-0.12 — SIMD Semantic Engine**
  - scalar reference first;
  - benchmark word-level POPCOUNT;
  - AVX2/AVX-512 where hardware and safe Rust support permit;
  - ARM NEON path where justified;
  - never claim gains without measured evidence.

- **SBG-0.13 — mmap Semantic Store**
  - versioned semantic dictionary/signature store;
  - checksummed format;
  - mmap-friendly alignment/indexing;
  - no mandatory full reconstruction into RAM;
  - Linux read-only mapped reference implementation;
  - independent growing-store resource/latency and integrity qualification before scale claims.

- **SBG-0.14 — Integrated SBG Prototype**
  - encoder -> SBG -> MOR -> Elastic Weights -> expert compute -> recurrent semantic rewrite;
  - feed native decision heads without mandatory text decoding;
  - end-to-end evidence and ablations;
  - promotion only after reproducible tests.

## Relationship to the earlier requested SML labels

The roadmap content originally described as SML-0.7 through SML-0.20 maps in order to SBG-0.1 through SBG-0.14. This rename exists solely to avoid colliding with already merged repository milestones.

## Current work and durable checkpoints

Reference-semantic SBG-0.1 through SBG-0.11 slices are merged. SBG-0.12 ARM64 kernel qualification merged through PRs #53-55; frozen evidence is in `docs/SBG-0.12-ARM64-QUALIFICATION.md`. Its timing ratio compares against a bit-by-bit oracle, not a complete model or all alternative packed kernels.

SML-1.3 controlled routing/keyed-memory ablations merged through PR #52. Learned routing does not beat fixed routing on that reference panel. SML-1.4 unary recall/copy projections merged through PR #56; marker activation and payload/control-flow semantics remain fixed and require separate learning experiments.

SBG-0.13a byte format merged through PR #57 as `786c0d549c78aae550bff14157d95ed472af2e8d`.

SBG-0.13b Linux mapped reader merged through PR #60 as `d2ba5c9999379e5a8bd75838f98a8f7823ec2f89`. Its qualification and limits are documented in `docs/SBG-0.13b-MAPPED-RUNTIME.md`.

Latest implementation slice: **SML-D0 / narrow D1a**, PR #61, branch `research/sml-d0-native-decision`. Read `docs/SML-D-BOOTSTRAP.md` and check live PR/CI state before continuing. Once merged, branch from current `main` rather than reviving this slice branch.

Implementation surfaces:

```text
crates/sml-decision/src/lib.rs
crates/sml-decision/src/training.rs
crates/sml-decision/tests/decision.rs
crates/sml-decision/tests/boundaries.rs
docs/SML-D-BOOTSTRAP.md
```

Definition of done for SML-D0:

1. validated acyclic Boolean program with typed binary/choice/ordinal outputs;
2. unknown-feature masks and explicit abstention rather than false defaults;
3. many heads share one circuit evaluation and one feature packing pass;
4. bit-sliced groups of at most 64 records match independent scalar semantics;
5. budgets checked before internal execution/allocation and counters distinguish logical work from word-kernel calls;
6. no text decoder, calibrated-probability claim or external action authorization;
7. exhaustive tables/partial-input cases, boundary groups, highest feature bit, constraints, malformed inputs and reconvergent unknown soundness tests;
8. complete locked workspace CI and decision evidence on the exact latest head before merge.

D1a adds only a finite two-input supervised-table fitting fixture with train-only selection and validation rejection. It is not a general decision model, not learned wiring and not a protected-final-holdout scaling result. Logical table bits are not physical Vec storage or a live persisted parameter commit.

The workspace retains `forbid(unsafe_code)` and the core retains no external dependencies. OS mapping uses pinned mmap-guard, which encapsulates unsafe code in its dependencies. This is not a safety proof against arbitrary external mutation: mapped inodes must never be modified/truncated while in use. Locks are cooperative; CRC32C is not authentication. Reader counters are logical payload/mapping counters, not RSS, page-cache or actual I/O measurements.

Next implementation gates:

- **SML-D1/D2**: substantial trained decision experts conditioned by SBG/state/questions, calibration and risk/coverage; no hand-authored feature schema may be presented as general language comprehension.
- **SML-D3/D4/D5**: streaming/sparse batches, bounded recurrent refinement and end-to-end matched-quality qualification; grouped call counts are not throughput gains.
- **SBG-0.13c**: cold/warm latency, RSS/page faults and traffic on growing stores; streamed publication and stronger immutable/index-integrity contracts. Do not mark the broad store/resource gate complete from small fixtures alone.
- **SBG-0.14**: connect mapped lookup to native SBG/MOR/Elastic/expert/memory/recurrent execution and native decision outputs with end-to-end evidence.
- **SML-1 follow-up**: learned marker/control, wider matched ablations, fixed-versus-adaptive depth. Freeze SML-0/SML-1 protocols before SML-2.
- **Elastic actuation**: native SML physical page materialization and VERIFY/ROLLBACK; other Memorithm repositories are research/qualification partners, not mandatory inference dependencies.

## Scientific boundary

SBG is currently an architectural hypothesis plus executable reference semantics. No claim is made yet that it matches or exceeds continuous language embeddings, that 16,384 bits is optimal, that Boolean-only training scales to 25B, or that SBG improves language-model quality. Those are experimental questions gated by the roadmap above. SML-D0 adds a bounded structured-feature decision runtime, not evidence of Jev-level capabilities or a measured whole-system speedup.

## BANC v888 Connectomic Structural Prior track

The V888-only connectomic programme is now a parallel SML research track under the **CSP** namespace so it does not collide with SML or SBG milestones. Its durable execution plan is [`docs/V888_CONNECTOME_BOOTSTRAP.md`](docs/V888_CONNECTOME_BOOTSTRAP.md).

CSP treats BANC v888 as an external source of structural priors, not as an inference-time dependency and not as a biological model to reproduce. The required progression is topology fingerprint -> matched structural controls -> low-bit sparse recurrence -> event-driven active compute -> bounded-memory/SBG/MOR integration -> trainable/Forge-selected topology -> independent TDI evaluation. No CSP result may enter SML-2 merely because it uses V888; it must beat the strongest declared matched controls under the evidence gate in the CSP roadmap.
