# SML-GENIUS research roadmap

Progression is evidence-gated. Model size does not advance because a calendar milestone elapsed.

| Gate | Approximate scale | Primary question | Required evidence before progression |
|---|---:|---|---|
| SML-0 | reference primitives | Is the architecture contract executable and auditable? | deterministic oracles, exact accounting, recurrent-depth runner, direct routing |
| SML-1 | 1-5M | Can the mechanisms learn bounded algorithmic tasks? | learned routing/memory beats declared ablations on held-out synthetic tasks |
| SML-2 | 50M | Can SML learn useful natural-language next-token structure? | reproducible training, fixed tokenizer, quality and resource curves |
| SML-3 | 300M | Does SML exhibit useful scaling? | measured loss/quality scaling with total and active parameters separated |
| SML-4 | 1.5B | Can the target runtime envelope be met? | <=8 GiB RAM, <=2 GiB VRAM, >=40 t/s on frozen hardware, active fraction <=1% per cycle |
| SML-5 | 7B | Does capacity grow faster than active compute? | active fraction/traffic remain sublinear while quality improves |
| SML-6 | 25B | Can the full atlas be trained and served? | 25B real learned parameters, audited storage, runtime limits preserved |
| SML-7 | 25B | Can instruction tuning preserve SML properties? | post-training quality without collapse of sparsity/depth/memory behavior |
| SML-8 | 25B | Is the system reproducibly qualified? | independent reruns, ablations, robustness, complete artefact provenance |

## SML-0 implementation status

The following are reference-semantics milestones, not model-quality results:

- [x] exact learned-parameter and packed-bit accounting;
- [x] bit-packed Boolean circuit oracle;
- [x] direct page routing without history scans;
- [x] bounded recurrent-depth execution and deterministic stability halting;
- [x] explicit non-attention audit counters;
- [x] GF(2) vector and linear-map reference operators;
- [x] canonical ANF / Zhegalkin polynomial evaluation;
- [x] bounded associative memory with explicit miss/collision/replacement outcomes;
- [x] dense O(1) circuit-atlas page manifest with metadata/alignment accounting;
- [x] recurrent-cycle trace records binding pages, active bits and state transitions;
- [x] SML-1 deterministic task generators and complete-answer oracles;
- [x] first continuous-latent / hard-forward trainability primitive for two-input Boolean gates;
- [x] fixed-wiring multi-gate Boolean circuit trainability with exact hard inference;
- [x] learned direct-routing bits under explicit structural budgets with hard O(1) freeze;
- [x] transactional elastic-weight precision/residency/activation planner with exact RAM/VRAM and active-set accounting;
- [x] versioned ElasticXxx validation/projection adapter for `sml.elastic-weight-plan@1.0.0`, qualified without physical actuation claims;
- [ ] learned wiring trainability remains a separate experiment;
- [x] discrete topology search through Forge `sml-topology/v1` with disjoint development/final-holdout rows;
- [x] independent destination-side Forge candidate parsing and full-oracle requalification;
- [ ] frozen SML-0 exit report.

## SML-1 pilot status

The following are integration milestones, not satisfaction of the SML-1 quality gate:

- [x] first integrated candidate with learned hard event routing and bounded direct memory;
- [x] deterministic disjoint case-ID holdout ranges;
- [x] complete per-event route, answer and execution-counter evidence;
- [x] fail-closed memory collision behavior without silent eviction;
- [x] learn the first micro-operation (Boolean conjunction) from task-answer loss with complete input coverage;
- [x] compare that learned gate against a deliberately wrong XOR control and the hand-coded AND oracle;
- [x] learn overwrite-state update semantics from task-answer loss with complete previous-state/incoming-value coverage;
- [x] compare the learned state update against a deliberately wrong XOR control and the fixed RIGHT-projection overwrite oracle;
- [x] SML-1.3 fixed-routing, disabled-keyed-memory and constrained-capacity reference ablations (PR #52);
- [x] SML-1.4 unary recall/copy projections learned from task answers, compared with wrong NOT controls and fixed IDENTITY (PR #56);
- [ ] learn marker activation, payload/control flow and other remaining fixed micro-operation semantics;
- [ ] extend matched ablations beyond the current bounded reference panel;
- [ ] demonstrate held-out improvement attributable to learned routing/memory rather than fixed task semantics.

Current reference ablations do not establish a learned-routing advantage: learned and fixed routing recover the same task answers. Disabled keyed memory is not an ablation of every recurrent/payload state. SML-1.4 learns bit projections, not the entire copy-control algorithm. None of these fixtures is a 1-5M-parameter trained model.

## Immediate implementation sequence

1. Bootstrap SML-RSI RSI-0a/0b as an independent SML-GENIUS implementation inspired by lessons from Memorithm/RSI: candidate PR evidence manifest, exact-head evidence binding, promotion eligibility, lineage/rollback and the invariant that only the repository owner's merge can promote.
2. Start SML-CONTEXT CONTEXT-0/1: exact raw/token/span accounting, immutable raw-segment ledger, provenance-linked SBG index and bounded streaming ingest. Durable protocol: `SML-CONTEXT-BOOTSTRAP.md`.
3. Advance SML-D2/D3/D4: proper-scoring calibration on reserved data, SBG/MOR coarse-to-fine candidate shortlisting, hot expert residency/prefetch, and DIRECT/REFINE/ABSTAIN gating.
4. Measure SBG-0.13c growing-store cold/warm lookup latency, RSS, page faults and index/record traffic. Logical mapped-byte counters alone do not satisfy a RAM gate. SBG-0.13b was merged through PR #60.
5. Build SBG-0.14 native integration: mapped semantic/raw lookup -> SBG -> MOR -> Elastic Weight page selection/materialization -> expert/memory execution -> recurrent rewriting -> typed decision or optional generation, with common evidence and ablations.
6. Learn marker/control micro-operations and compare fixed versus adaptive recurrent depth on the same task panels; keep protected holdouts separate from development feedback.
7. Freeze the SML-0 exit report and SML-1 development protocol before larger training runs.
8. Evaluate learned wiring separately against fixed/Forge-selected topology; assess reusable primitives for SciRust without coupling the final SML runtime to it.

## Stop conditions

Scaling stops if any of the following persists after a bounded research iteration:

- quality fails to improve with scale under a controlled training budget;
- active compute grows approximately linearly with total parameter count;
- recurrent depth adds cost without measurable task benefit;
- routing degenerates into an all-history scan or all-pairs scoring mechanism;
- metadata/index storage overwhelms the low-bit weight advantage;
- the target memory hierarchy cannot hide storage traffic sufficiently for the SML-4 throughput gate.

## Semantic Boolean Geometry track

SBG is a parallel evidence-gated semantic-representation track. It does not reuse occupied SML-0.x milestone numbers; implementation slices use the `SBG-0.x` namespace. The durable detailed sequence is recorded in `BOOTSTRAP.md`.

Current status:

- [x] SBG-0.1 packed const-generic Semantic BitField reference implementation;
- [x] SBG-0.1 native V/M/A/R SemanticState payload accounting;
- [x] SBG-0.2 masked Boolean semantic metric;
- [x] SBG-0.3 binding/bundling/permutation;
- [x] SBG-0.4 context meaning mask;
- [x] SBG-0.5 ambiguity dynamics;
- [x] SBG-0.6 semantic composition;
- [x] SBG-0.7 native MOR integration;
- [x] SBG-0.8 recurrent semantic rewriting;
- [x] SBG-0.9 Elastic Weight region activation;
- [x] SBG-0.10 Boolean evidence learning integration;
  - [x] SBG-0.10a V/M/A/R Boolean-integral coordinate credit and train/holdout commit gate;
  - [x] SBG-0.10b structural BIND/SPLIT/MERGE evidence decisions;
- [x] SBG-0.11 linguistic micro-benchmark;
- [x] SBG-0.12 SIMD qualification on the current ARM64 reference runner;
  - [x] SBG-0.12a scalar-oracle versus safe packed-u64 POPCOUNT kernel qualification;
  - [x] SBG-0.12b reproducible wall-clock measurement on the ARM64 self-hosted runner;
  - [x] SBG-0.12c exact release code-generation audit under the safe-Rust boundary;
- [ ] SBG-0.13 mmap semantic store and resource qualification;
  - [x] SBG-0.13a versioned checksummed mmap-friendly binary format (PR #57);
  - [x] SBG-0.13b Linux read-only mapped reader, bounded per-record decoding, corruption/lock/reopen/replacement tests (merged PR #60);
  - [ ] SBG-0.13c growing-store resource/latency evidence, immutable-artifact hardening and authenticated index/manifest evaluation;
- [ ] SBG-0.14 integrated prototype, including native decision outputs.

SBG-0.13b is Linux-specific and relies on an immutable mapped inode with cooperative locking. It rejects heap fallback and does not impose an OS page-cache/RSS ceiling. Other platforms, streaming writes, cryptographic authenticity and production deployment hardening remain unqualified. See `docs/SBG-0.13b-MAPPED-RUNTIME.md`.

The SBG track is an experimental hypothesis. It does not yet establish language quality, optimal semantic width, or 25B-scale trainability. The SBG-0.12 ratio is relative to the independent bit-by-bit oracle, not a model throughput measurement or a comparison against every packed-word alternative.

## SML-D — first-class native decision programme

The full protocol and D0..D5 implementation sequence are in [`SML-D-BOOTSTRAP.md`](SML-D-BOOTSTRAP.md). A substantial decision capacity is part of the target architecture, with separate total/active parameter accounting and dedicated training objectives. No fixed fraction of the 25B atlas is justified yet. Native direct and recurrent decisions must not require the text decoder.

- [x] D0 reference implementation: typed Boolean/choice/ordinal heads, explicit unknowns and abstention, shared circuit evaluation, 64-lane bit-sliced groups and admission budgets (merged PR #61).
- [x] D1a finite two-input expert-fitting fixture: training-only selection, validation rejection, separate test fixtures. This is not the general D1 gate.
- [x] D1b reference implementation: observed V/M/A/R coordinate projection, shared inference/finite-training path and separate projection budgets (merged PR #62). See `SML-D1b-SBG-BRIDGE.md`.
- [x] D1c bounded learned SBG feature-pair selection against a fixed-coordinate control (merged PR #63). See `SML-D1c-FEATURE-SELECTION.md`.
- [x] D1d MOR-gated sparse decision expert bank with matched dense work-count control (merged PR #64). See `SML-D1d-MOR-SPARSE-BANK.md`.
- [x] D1e MOR-selected Elastic Weight PLAN/VALIDATE admission before sparse decision execution (merged PR #65). See `SML-D1e-ELASTIC-ADMISSION.md`.
- [x] D1f-a verified disk-to-RAM MATERIALIZE/VERIFY/COMMIT-or-ROLLBACK for unchanged-precision pages (merged PR #66). See `SML-D1f-RAM-MATERIALIZATION.md`.
- [ ] D1 substantial trained experts, learned SBG-derived features and conditional questions/candidates; multi-task training and independent quality evidence.
- [ ] D2 proper-scoring/calibration on reserved data, cost-sensitive thresholds, risk/coverage and distribution-shift tests.
- [ ] D3 SBG/MOR coarse-to-fine candidate shortlisting, streamed/columnar bounded batches, hot/warm/cold expert residency/prefetch and complete packing/I/O/output cost measurements.
- [ ] D4 DIRECT / REFINE / ABSTAIN gate with bounded memory/recurrent escalation and matched fixed/adaptive baselines.
- [ ] D5 independently reproducible comparisons at matched quality/coverage against exact rules, small classifiers, constrained generative models and declared decision baselines.

D0 outputs are uncalibrated proposals, never action authorization. Numeric question IDs and caller-supplied features do not implement natural-language question understanding. Logical truth-table bits are not physical in-memory storage. A reduction in word-kernel call count is not a measured speedup; report decisions/sec and latency distributions only from actual end-to-end benchmarks. D0/D1a/D1b do not close the SML-1 or SML-2 model gates. D1b observation masks remain explicit caller metadata; M is relevance rather than availability, and clear A is not calibrated confidence. Store v1 does not supply those observation masks.

The existing Forge `sml-topology/v1` contract was inspected for reuse; its single-output, at-most-eight-input search boundary differs from these typed multi-head batches. See `SML-D-REUSE.md` before integrating or modifying either repository.

## SML-CONTEXT — hierarchical context horizon

The durable protocol is in [`SML-CONTEXT-BOOTSTRAP.md`](SML-CONTEXT-BOOTSTRAP.md). SML context is not defined as one permanently resident token buffer. It uses a bounded active working state plus immutable raw segments, associative memory and mapped semantic/raw indexes with provenance.

- [ ] CONTEXT-0 exact byte/token/span accounting, stable segment IDs and raw/SBG provenance contract;
- [ ] CONTEXT-1 bounded streaming ingest into immutable raw segment store plus SBG index;
- [ ] CONTEXT-2 bounded recurrent semantic working context with matched fixed/expanding controls;
- [ ] CONTEXT-3 hierarchical SBG/MOR top-k context retrieval with exhaustive-scan controls;
- [ ] CONTEXT-4 hot/warm/cold retention and optional lossy compaction with raw provenance preserved;
- [ ] CONTEXT-5 recurrent historical refinement and adaptive halting;
- [ ] CONTEXT-6 long-range learning curriculum with protected holdouts;
- [ ] CONTEXT-7 evidence-gated span ladder: 4k -> 16k -> 64k -> 256k -> 1M tokenizer-token dependency spans.

Do not report an N-token effective context because N tokens fit on disk or were ingested. Admission requires a frozen task whose dependency spans that distance plus quality, RAM, traffic and latency evidence.

## SML-RSI — human-gated recursive self-improvement

The durable protocol is in [`SML-RSI-BOOTSTRAP.md`](SML-RSI-BOOTSTRAP.md). It is implemented independently in SML-GENIUS, using `Memorithm/RSI` only as a source of design inspiration.

Governance invariant: `main` is the approved champion. The model may create/revise branches and PRs and gather CI/verifier evidence; it cannot merge an RSI PR. Human GitHub merge/reject is the terminal promotion decision.

- [x] RSI-0a versioned PR/candidate evidence manifest bound to parent main SHA and exact candidate head (merged PR #69);
- [x] RSI-0b promotion-eligibility verifier: exact parent/head binding, evidence completeness, protected-eval separation and hard-regression gate; never merge authorization (merged PR #70);
- [x] RSI-1 model-proposed exact-verifier curriculum using native task oracles, reserved case IDs and deterministic train/development split (merged PR #71);
- [ ] RSI-1b matched proposed-vs-fixed curriculum training using the same candidate trainer and separate holdout; report ties/regressions without promotion claims (active slice);
- [x] RSI-2 bounded frozen-champion -> verified curriculum -> frozen candidate -> exact evidence -> review-eligibility state machine (merged PR #72);
- [ ] RSI-3 proposer/solver/verifier co-evolution using learning progress rather than unsolved difficulty alone;
- [ ] RSI-4 recursive improvement of SML-D, MOR, memory, SML-CONTEXT and recurrent policies;
- [ ] RSI-5 bounded topology/runtime-policy self-optimization;
- [ ] RSI-6 evaluator candidates against frozen anchor evaluators and exact oracles;
- [ ] RSI-7 candidate changes to the improvement procedure itself, still emitted as reviewable PRs;
- [ ] RSI-8 multi-generation qualification with human merge gate retained.

Inspiration targets from `Memorithm/RSI`, to be re-implemented independently: AUTOPILOT frozen specs/task DAGs/PR emission, DGM isolated empirical gate, CandidateState deterministic complete-tree identities, CumulativeArchive lineages, EvaluationPipeline typed evidence, Loop Engineering stop/checkpoint/breaker/HITL controls, and flywheel trajectories from real outcomes. Do not add any Cargo/git/submodule/MCP/service/CI/runtime/orchestration dependency on the RSI repository; any adopted mechanism must be implemented and qualified natively in SML-GENIUS.

## CSP — BANC v888 Connectomic Structural Prior track

The V888-only structural-prior programme is defined in [`V888_CONNECTOME_BOOTSTRAP.md`](V888_CONNECTOME_BOOTSTRAP.md) and uses the independent `CSP-0.x` namespace. CSP is evidence-gated alongside SML/SBG: V888-derived topology receives no promotion privilege and must outperform the strongest declared edge-count/degree/reciprocity/modularity controls before entering larger SML experiments. Raw V888 data is never an inference-time dependency, and new execution work is Rust-first on CPU/WGPU portable paths.
