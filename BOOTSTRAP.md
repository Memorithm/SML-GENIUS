# SML-GENIUS bootstrap

This file is the durable re-entry point for continuing the SML-GENIUS research programme from the current repository state. Read it together with `README.md`, `docs/ROADMAP.md`, the latest merged pull requests, and the executable contracts in `sml-core`.

## Current architectural direction

SML is an independent, predominantly Boolean / low-bit autoregressive architecture. It is not defined by reference to Transformer internals. Its native mechanisms include direct routing, Mixture-of-Routers / sparse expert selection, bounded associative memory, recurrent depth, Elastic Weights, and Boolean-integral learning experiments.

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

Implementation uses the independent **SBG-0.x** slice namespace while preserving the same roadmap content.

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
  - no mandatory full reconstruction into RAM.

- **SBG-0.14 — Integrated SBG Prototype**
  - encoder -> SBG -> MOR -> Elastic Weights -> expert compute -> recurrent semantic rewrite;
  - end-to-end evidence and ablations;
  - promotion only after reproducible tests.

## Relationship to the earlier requested SML labels

The roadmap content originally described as SML-0.7 through SML-0.20 maps in order to SBG-0.1 through SBG-0.14. This rename exists solely to avoid colliding with already merged repository milestones.

## Current work

Completed:

- SBG-0.1 Semantic BitField Core merged on main as `c079acafea90549f6a97dfd8956a9f16fb58641f`.
- SBG-0.2 contextual Boolean semantic metric merged on main as `d38023771bb5427a802b3337e40f857f0be8af50`.
- SBG-0.3 semantic binding and bundling algebra merged on main as `776995c49e71cb6ea3209e14a7a310340f0668bb`.
- SBG-0.4 Context Meaning Mask merged on main as `cd16486261c2b83995b2573c333571011000521d`.
- SBG-0.5 Ambiguity Field merged on main as `54c07e50fd313c28a041a05742f7d573b4baefd1`.
- SBG-0.6 Semantic Composition merged on main as `966518009d1360fe3fa00116b955c20e4e1c25cf`.
- SBG-0.7 native MOR integration merged on main as `1610f6b267f8aa45d05b9969977a56c06146552c`.
- SBG-0.8 recurrent semantic rewriting merged on main as `ae7e65c3aeec4446d3e742cd44a892cc206ac798`.
- SBG-0.9 Elastic Weight region activation merged on main as `396b65df23c816edcd4407e8f5000ff694e516c9`.
- These slices passed the repository CI gates before merge.

Active development branch:

```text
research/sbg-0.10a-evidence-learning
```

Current implementation target:

```text
crates/sml-core/src/semantic/evidence_learning.rs
```

SBG-0.10 is intentionally split into evidence-gated sub-slices. SBG-0.10 must remain incomplete until structural BIND/SPLIT/MERGE learning semantics are executable.

Definition of done for SBG-0.10a:

1. V/M/A/R coordinates share one persistent native `BooleanIntegralAccumulator` evidence space;
2. grouped XOR interventions retain the existing shared-credit semantics and bounded evidence policy;
3. KEEP/FLIP decisions come from the existing Boolean-integral policy, while candidate flips map to explicit SBG actions such as SET/CLEAR, MASK/UNMASK, ambiguity updates and ROUTE on/off;
4. candidate mutation is evaluated on an immutable semantic-state clone;
5. the existing `BooleanCommitEvidence` train/holdout gate is authoritative before atomic state replacement;
6. stale or malformed candidates fail closed and holdout rejection leaves the original semantic state unchanged;
7. SBG-0.10 remains open for structural BIND/SPLIT/MERGE evidence semantics;
8. `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` pass in CI.

## Scientific boundary

SBG is currently an architectural hypothesis plus executable reference semantics. No claim is made yet that it matches or exceeds continuous language embeddings, that 16,384 bits is optimal, that Boolean-only training scales to 25B, or that SBG improves language-model quality. Those are experimental questions gated by the roadmap above.
