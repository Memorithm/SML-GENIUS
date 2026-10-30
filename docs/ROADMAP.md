# SML-GENIUS research roadmap

## Fourier–Walsh × ANF — research sequence (2026-10-19)

Detailed [evidence-gated spectral discovery roadmap](SML-FOURIER-WALSH-ANF-ROADMAP-20261019.md): reuse existing A6-C12a–h and A6-I5 results, including C12c's 508-source hybrid and C12g/h noisy-parity failures, plus A6-E5 spectral Beam, arbiter and materialization kernels; F0 audit, F1 bounded exact oracles, F2 TRAIN-only adaptive choice between already measured W2+Sketch/MITM and W3, F3 noisy/nonlinear interactions, F4 next-byte, F5 frozen A6-E5 criteria, F6 native resources, F7 model interface. No new TEST access, no reinterpretation of old successes as general-language model results, no change to 8 MiB / 2x and log-loss gates. F1/F2 require separate CI-green PRs before integration.

Progression is evidence-gated. Model size does not advance because a calendar milestone elapsed.


## Programme-level success criterion

**Competitiveness at parity is success.** The fully integrated, actually
trained SML architecture need not beat conventional reference architectures.
It must demonstrate **both**:

- **Learning at least as well:** preregistered non-inferiority or superiority
  on held-out learning and capability metrics, comparing matched training
  data, data splits, tokenizer/task protocol, training budgets and seeds.
- **Quality/cost tradeoff at least as good:** preregistered non-inferiority or
  superiority of an end-to-end quality/resource frontier or task-specific
  utility. Equality in both quality and cost counts as success. For each
  comparator, report matched-quality and/or matched-resource operating
  points and separately account for training compute/time, inference latency
  and throughput, RAM/VRAM residency, mapped storage, page/parameter traffic
  and energy only if measured with a valid meter.

Register the competitive baselines, quality/cost definitions, practical
non-inferiority margins, uncertainty intervals, stopping rules and untouched
evaluation split **before** running the decisive tests. Use paired estimates
and uncertainty to distinguish evidence of non-inferiority from merely
failing to detect a difference. Require complete model integration (native
SBG/MOR, learned expert/circuit selection, recurrent memory/state and
output/decision path) and end-to-end measurement; a component microbenchmark,
synthetic success or standalone compressor does not close this gate.

This is a **programme-level cross-architecture success criterion**, not a
waiver of the distinct within-SML scaling, safety and reproducibility gates
below. No current experiment is claimed to have satisfied it.


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


## Model-first boundary

`SML-GENIUS` is the Model programme. Durable boundary:
[SML-MODEL-HARNESS-BOUNDARY.md](SML-MODEL-HARNESS-BOUNDARY.md).

The first real SML-6 25B artifact is the bootstrap champion. The future
`Memorithm/SML-HARNESS` will be a **separate repository** created after that
milestone. It will compose Forge, ProofLab, TDI, KVLab, ElasticXxx,
CCOS-Enterprise, RemoteOps and other independently evolving Memorithm products
through versioned contracts/adapters rather than absorbing them into the model.

Recursive self-improvement, cross-repository mutation, experiment orchestration,
promotion/rollback and autonomous scale-up belong to SML-HARNESS. After the
first 25B champion, the model may use the Harness to design, train, qualify and
propose larger descendant generations. No post-25B parameter count is frozen:
growth is accepted only when capability improves while active compute/resource
scaling remains within the SML gates.

Until the first real 25B model exists, the critical path is the model. External
tooling advances only where required to train, measure and fail-closed qualify
that model.

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

1. Close the remaining **model-core** SML-0/SML-1 gates: learned control/micro-operation semantics, learned-routing/memory advantage, bounded recurrent behavior and the frozen SML-0 exit report.
2. Complete the inference-time SBG -> MOR -> memory/expert -> recurrent rewrite -> decision/generation path, including the active DKV domain/memory qualification required by the model.
3. Complete context mechanisms that are part of model runtime semantics: bounded active context, mapped semantic/raw lookup and provenance-linked retrieval.
4. Measure growing-store cold/warm latency, RSS, page faults, traffic and active working-set behavior so the scale gates remain physical and auditable.
5. Advance the learned model through SML-2 50M -> SML-3 300M -> SML-4 1.5B -> SML-5 7B with frozen scale gates and total-versus-active parameter accounting.
6. Train and serve the first real **SML-6 25B** champion with audited learned-parameter count, storage layout and runtime envelope.
7. Freeze the Model/Harness interface and create the separate **Memorithm/SML-HARNESS** repository.
8. Integrate the independently evolving ecosystem capabilities needed by SML-HARNESS and enable autonomous descendant training/evolution. Later generations may exceed 25B only under explicit capability/resource scaling gates.

## Stop conditions

Scaling stops if any of the following persists after a bounded research iteration:

- quality fails to improve with scale under a controlled training budget;
- active compute grows approximately linearly with total parameter count;
- recurrent depth adds cost without measurable task benefit;
- routing degenerates into an all-history scan or all-pairs scoring mechanism;
- metadata/index storage overwhelms the low-bit weight advantage;
- the target memory hierarchy cannot hide storage traffic sufficiently for the SML-4 throughput gate.

## Pascal-Boolean functional track

The closed PGF2 representation-mixing result remains unchanged. A separate
K=2048 functional suite has retained P-ZETA, P-ROUTER, P-ANF and P-FEISTEL
under frozen exactness/reversibility contracts. PB-5 also retains their composed
pipeline: all C1..C6 gates hold and all 131,072 end-to-end comparisons match the
independent oracle. See `SML-PASCAL-BOOLEAN-SUITE-OBSERVATION.md` and
`SML-PASCAL-BOOLEAN-PB5-OBSERVATION.md`.

PB-6 completes the first matched-control task-utility gate on the frozen K=2048
subset-parity workload. U1..U6 hold: P-ZETA produces exact answers, has equal
256-byte answer-vector storage to the generic-precompute control, wins 20/20
paired physical repetitions, and records a 3.368060 ms median versus
13.344107 ms for the faster matched control (3.961956x). See
`SML-PASCAL-BOOLEAN-PB6-TASK-UTILITY-OBSERVATION.md`.

PB-7 utility generalization is executed and retained. G1..G7 hold across
the frozen K=64..4096 deterministic/physical programme and the decision is
`RETAIN_GENERALIZED_UTILITY_SURFACE`. Full-domain affine speedup over the
faster matched control is 3.359x at K=64, 13.643x at K=512, 22.594x at
K=2048 and 29.021x at K=4096, with exact checksums and 12/12 paired wins in
every full-domain cell. The retained K=2048 LOW_POPCOUNT boundary shows direct
on-demand winning at Q=32 and Pascal missing the 10% threshold at Q=128. See
`SML-PASCAL-BOOLEAN-PB7-UTILITY-SURFACE-OBSERVATION.md`.

PB-8 is preregistered as the first model-aligned Pascal task study. It
treats 11-feature ANF predicates as candidate admission/decision gates and
compares packed Pascal materialization against sparse ANF scan, direct submask
query, generic truth materialization and a hot-resident truth-table reference.
The frozen deterministic matrix spans coefficient density, record population,
batch size and reuse; the physical panel tests materialization, dense-gate
utility and amortization. See
`SML-PASCAL-BOOLEAN-PB8-MODEL-ALIGNED-GATE-PREREGISTRATION.md`.

No current MOR/decision implementation is changed by PB-8. A positive result
can justify only a later separately preregistered integration arm.

PB-9 large-width scaling is executed and retained. All L1..L6 gates hold.
Exact mandatory evidence reaches K=1,048,576; its packed 128 KiB state transforms
in 183.634 us median on the pinned ARM64 runner. At K=1,048,576 the frozen
Q=2,048 affine task records 903.755 us for Pascal versus 17.606782 ms direct
(19.4818x, 12/12 paired wins), while Q=128 remains a retained direct-evaluation
win. The resource-bounded extension reaches the full preregistered deep ladder
through K=268,435,456 without STOP_RESOURCE; the 32 MiB state has an 81.195066 ms
kernel median. Decision:
`RETAIN_LARGE_WIDTH_SCALING_AND_1M_UTILITY`. See
`SML-PASCAL-BOOLEAN-PB9-LARGE-WIDTH-SCALING-OBSERVATION.md`.

## Semantic Boolean Geometry track

SBG is a parallel evidence-gated semantic-representation track. It does not reuse occupied SML-0.x milestone numbers; implementation slices use the `SBG-0.x` namespace. The durable detailed sequence is recorded in `BOOTSTRAP.md`.

Current status:

- [x] SBG-0.1 packed const-generic Semantic BitField reference implementation;
- [x] SBG-0.1 native V/M/A/R `SemanticState` payload accounting;
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

The [SML-CLM track](#sml-clm--contrastive-decisions-and-measured-scaling) supplies a proposed learning comparison for D1, separate ranking/calibration evidence for D2, bounded shortlisting experiments for D3 and verified refinement outcomes for D4. It does not close any D gate or replace D5's independent evaluation.

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

The durable protocol is in [`SML-CONTEXT-BOOTSTRAP.md`](SML-CONTEXT-BOOTSTRAP.md). SML context is not defined as one permanently resident token buffer. It uses a bounded active working state plus immutable raw segments, associative memory and mapped SBG/raw indexes with provenance.

- [x] CONTEXT-0 exact byte/token/span accounting, stable segment IDs and raw/SBG provenance contract (merged PR #89);
- [ ] CONTEXT-1 bounded streaming ingest into immutable raw segment store plus SBG index;
  - [x] CONTEXT-1a immutable SHA-256 raw segment persistence (merged PR #90);
  - [ ] CONTEXT-1b bounded stream-to-segment ingest with constant-in-total-length working payload bound (active stacked slice);
  - [ ] CONTEXT-1c synchronized streaming SBG store + raw/SBG provenance manifest emission with bounded per-segment budgets (active slice);
  - [ ] CONTEXT-1a immutable SHA-256-verified raw segment artifact store (current PR);
  - [ ] CONTEXT-1b bounded streaming ingest + separate semantic-index emission;
- [ ] CONTEXT-2 bounded recurrent semantic working context with matched fixed/expanding controls;
- [ ] CONTEXT-3 hierarchical SBG/MOR top-k context retrieval with exhaustive-scan controls;
- [ ] CONTEXT-4 hot/warm/cold retention and optional lossy compaction with raw provenance preserved;
- [ ] CONTEXT-5 recurrent historical refinement and adaptive halting;
- [ ] CONTEXT-6 long-range learning curriculum with protected holdouts;
- [ ] CONTEXT-7 evidence-gated span ladder: 4k -> 16k -> 64k -> 256k -> 1M tokenizer-token dependency spans.

Do not report an N-token effective context because N tokens fit on disk or were ingested. Admission requires a frozen task whose dependency spans that distance plus quality, RAM, traffic and latency evidence.

## SML-DELTA-KV — bounded rewritable associative memory

The durable protocol is in [`SML-DELTA-KV-BOOTSTRAP.md`](SML-DELTA-KV-BOOTSTRAP.md).
Delta-KV is a separate memory track: it does not replace the existing direct
`BoundedAssociativeMemory`, immutable raw context, mapped SBG store or recurrent
workspace.

- [x] DKV-0a deterministic signed-Q12 normalized-delta reference semantics, exact
  semantic-bit accounting, overwrite/orthogonal/overlap controls and explicit
  saturation counters (merged PR #78);
- [x] DKV-0b preregistered 14-case interference/correction panel across overlap bands,
  conflicting targets, partial learning rates, repeated overwrite, saturation and dimension scaling (merged PR #79);
- [x] DKV-1 matched declared-storage comparison against RejectCollision/ReplaceExisting
  direct associative memory plus bounded append-only record control; same frozen script, no composite winner metric (merged PR #80);
- [x] DKV-2a deterministic bounded SBG V/M/A/R + existing MOR-selection projection into Q12 key space; raw provenance remains separate (merged PR #81);
- [x] DKV-2b frozen-query Delta-KV reads in bounded SBG recurrence with adaptive/fixed/no-DKV controls and direct memory retained independently (merged PR #83);
- [x] DKV-2c dynamic SBG re-keying mechanism evaluated under frozen MOR selection (merged implementation PR #86; not promoted after missing-successor control remained confidently non-ambiguous);
- [ ] DKV-3a context/provenance integration preregistered; execution blocked on CONTEXT-0/1, uses qualified DKV-2b frozen-key recurrence only and requires raw verification for source-exact tasks with exact raw provenance and long-range task
  evidence;
- [x] DKV-4 Development adaptive-compute programme: fail-closed provenance-first
  controller, actual DKV-2b/SBG/MOR signals, fixed-depth/stability controls, paired
  K64/K256 evidence, and semantic-impact follow-up completed. DKV-4a absolute-Q12
  refinement is rejected; DKV-4c removes ~49% of DKV-4a cycles without correctness
  loss but matches stability-only on the current fixed-MOR panel, so scientific
  promotion remains blocked. See `SML-DKV-4-FINAL-REPORT.md`.

The external Cadence work is only a hypothesis source for rewritable memory and
repair-oriented computation. No external implementation or benchmark result is
imported as SML evidence. Forge may propose candidates and ElasticXxx may bound
resources, but SML owns the memory semantics and promotion gate.

## SML-RSI — human-gated recursive self-improvement

The durable protocol is in [`SML-RSI-BOOTSTRAP.md`](SML-RSI-BOOTSTRAP.md). It is implemented independently in SML-GENIUS, using `Memorithm/RSI` only as a source of design inspiration.

Governance invariant: `main` is the approved champion. The model may create/revise branches and PRs and gather CI/verifier evidence; it cannot merge an RSI PR. Human GitHub merge/reject is the terminal promotion decision.

- [x] RSI-0a versioned PR/candidate evidence manifest bound to parent main SHA and exact candidate head (merged PR #69);
- [x] RSI-0b promotion-eligibility verifier: exact parent/head binding, evidence completeness, protected-eval separation and hard-regression gate; never merge authorization (merged PR #70);
- [x] RSI-1 model-proposed exact-verifier curriculum using native task oracles, reserved case IDs and deterministic train/development split (merged PR #71);
- [x] RSI-1b matched proposed-vs-fixed conjunction curriculum training using the same candidate trainer and separate holdout (merged PR #73; reference result tie);
- [x] RSI-1c train conjunction/overwrite/recall/copy jointly from one verified RSI curriculum, with keyed/distractor families evaluated but not claimed learned (merged PR #74);
- [x] RSI-1d bounded fixed-vs-proposed curriculum portfolio selected on a shared disjoint development panel (merged PR #75);
- [x] RSI-1e trusted-evaluator post-selection holdout, exact-oracle validated and disjoint from training/development, compares frozen selection with best fixed control without reselection (merged PR #76);
- [ ] RSI-2a aggregate-only measured flywheel records derived from frozen development and post-selection holdout evidence without leaking holdout task content (active slice);
- [x] RSI-2 bounded frozen-champion -> verified curriculum -> frozen candidate -> exact evidence -> review-eligibility state machine (merged PR #72);
- [ ] RSI-3 proposer/solver/verifier co-evolution using learning progress rather than unsolved difficulty alone;
- [ ] RSI-4 recursive improvement of SML-D, MOR, memory, SML-CONTEXT and recurrent policies;
- [ ] RSI-5 bounded topology/runtime-policy self-optimization;
- [ ] RSI-6 evaluator candidates against frozen anchor evaluators and exact oracles;
- [ ] RSI-7 candidate changes to the improvement procedure itself, still emitted as reviewable PRs;
- [ ] RSI-8 multi-generation qualification with human merge gate retained.

Inspiration targets from `Memorithm/RSI`, to be re-implemented independently: AUTOPILOT frozen specs/task DAGs/PR emission, DGM isolated empirical gate, CandidateState deterministic complete-tree identities, CumulativeArchive lineages, EvaluationPipeline typed evidence, Loop Engineering stop/checkpoint/breaker/HITL controls, and flywheel trajectories from real outcomes. Do not add any Cargo/git/submodule/MCP/service/CI/runtime/orchestration dependency on the RSI repository; any adopted mechanism must be implemented and qualified natively in SML-GENIUS.

## CSP — BANC v888 Connectomic Structural Prior track

The V888-only structural-prior programme is defined in [`V888_CONNECTOME_BOOTSTRAP.md`](V888_CONNECTOME_BOOTSTRAP.md), coordinated with ITD in [`V888_BOOL_COORDINATION.md`](V888_BOOL_COORDINATION.md), and uses the independent `CSP-0.x` namespace. CSP is evidence-gated alongside SML/SBG: V888-derived topology receives no promotion privilege and must outperform the strongest declared edge-count/degree/reciprocity/modularity controls before entering larger SML experiments. Raw V888 data is never an inference-time dependency, and new execution work is Rust-first on CPU/WGPU portable paths.

- [x] Freeze ITD strategy revision `2026-09-22.1` / commit `7f8f08e764dd5f36688533e9f7e7fb2f290f6e17` without renumbering CSP.
- [x] Retire unscoped Codex display totals from SML evidence accounting; require typed audited source scopes and exact artifact identities.
- [x] Encode delayed-XOR admission requirements and the V888-BOOL-4.0 fail-closed consumer contract in `sml_core::v888_handoff`.
- [x] Require graph-only, existing-router-only and hybrid ablations plus reuse of existing GF(2)/ANF, bounded-memory and routing contracts.
- [x] Make hidden input history, raw-V888 inference dependency, protected-final access, direct model promotion and default runtime-policy changes fail closed in the coordination contract.
- [ ] Complete SciRust executable graph qualification in `Memorithm/scirust#1520`; #1500 remains its V888-GROWTH/source parent.
- [ ] Run the separately frozen delayed-XOR campaign on a selected audited source scope with exact sequential-logic, no-reservoir and topology-matched controls.
- [ ] Qualify any V888-BOOL-4.0 consumer candidate with independent SML quality/resource evidence before CSP-0.11 can consider promotion.

The checked items close the durable coordination work of SML-GENIUS #59 only. They do not close the ITD/SciRust research umbrellas and do not satisfy CSP-0.11.

## SML-DS41 — efficient long-horizon principles (2026-09-24)

This programme adapts systems principles from the reviewed DeepSeek-V4.1-Flash report without changing SML's non-attention candidate contract.

### SML-DS41-0 — ingest/decode asymmetry
Define separate budgets and counters for context ingestion, semantic-state construction, and recurrent decode. Demonstrate on frozen tasks that reduced ingest work does not silently move equivalent work into later cycles.

### SML-DS41-1 — hierarchical SBG/MOR candidate pool
Use an early broad routing stage to construct a bounded candidate pool, then restrict later refinement to that pool. Compare against exhaustive routing, current direct routing, and matched random pools. Report recall/task quality, active pages, metadata, traffic, and latency separately.

- [x] MOR-H1 reference coarse-to-fine candidate narrowing with exact cluster/expert work accounting and hard candidate/edge budgets.
- [x] MOR-H2 adaptive 1 -> 2 -> 4 widening from coarse margin, MOR margin/disagreement and SBG ambiguity, with cumulative fail-closed work budgets and a separate exhaustive-primary recall control.
- [x] MOR-H3 specialization-preserving balance: exposure-conditioned EMAs, neutral first exposure, shared H1/H2 cluster partition, and separate bounded inter/intra-cluster adjustments.
- [x] MOR-H4a bounded learned cluster prototypes from training-only SBG routing states, with frozen memberships and a separate development holdout gate.
- [x] MOR-H4a majority-bit control with identical target-expert semantics, weak-bit seed retention, hard mutation budgets and a separate development gate.
- [x] MOR-H4a matched qualification harness for fixed, learned-medoid, training-majority and deterministic random-medoid prototype arms with explicit gain gates.
- [ ] MOR-H4b learned expert-to-cluster membership only after a frozen representative-task H4a campaign uses the matched harness with repeated seeds and retains quality/resource evidence.
- [ ] Preregister and run matched exhaustive/fixed-1/fixed-2/fixed-4/adaptive/random-pool plus legacy/no-balance/H3 balance comparisons on disjoint development tasks.
- [ ] Qualify shortlist recall, final task quality, physical latency/traffic and expert-starvation behavior before runtime promotion.

### SML-DS41-2 — conditional memory comparison
Add an explicit experiment comparing deterministic conditional lookup against existing bounded associative memory and Delta-KV controls. Any hash/gating mechanism remains SML-native and must preserve collision/interference evidence.

### SML-DS41-3 — storage versus recomputation
Define which recurrent or context-derived states may be dropped and reconstructed from immutable provenance. Pair full-state and replay continuations, then expose only qualified actions to ElasticXxx.

### SML-DS41-4 — reasoning-effort scalar
Introduce a deployment-visible effort scalar that maps to bounded recurrent depth/refinement/verification budgets. Train or calibrate only on Development data, preserve fixed-depth controls, and report quality/latency/active-work curves. No hidden chain-of-thought artifact is required; the control concerns compute allocation.

### SML-DS41-5 — long-horizon comparison
Run protected long-range tasks against SML-CONTEXT and declared attention-model baselines at matched task definitions. Separate quality, resident memory, mapped storage, traffic, active compute, and wall-clock time.

CED/CSA2/SWA/Engram remain external comparison concepts. They are not native SML components unless a future separately preregistered architectural change explicitly overturns the current non-attention contract.

## SML-CLM — contrastive decisions and measured scaling

Added 2026-10-02. **All implementation/evaluation gates below are open.** The design and source boundaries are in [BOOTSTRAP.md](../BOOTSTRAP.md#sml-clm--contrastive-decisions-and-measured-scaling). This independent namespace does not renumber SML, SBG, SML-D, DKV or CONTEXT stages.

Source: Jacky Kwok et al. (2026), [Contrastive Language Models](https://github.com/Contrastive-LM/CLM/blob/bb42c6c5bf914fd449bed2f6ca65be80602cb1f7/README.md#scaling-laws-for-verification), reviewed 2026-10-02. CLM supplies an external contrastive-learning and scaling hypothesis, not SML benchmark evidence. Its head/data ratio and fitted exponents must not be transferred to the 25B Boolean atlas. Numerical upstream reproduction requires resolving its exact counting conventions, compute boundaries and fit uncertainty first.

### SML-CLM-0 — preregistration and evidence contract

- [ ] Freeze versioned state/question/candidate/outcome schemas, explicit observation masks, all-valid-positive/equivalence sets, unknown/abstention semantics and source/verifier identities. Question/candidate semantics must not be reduced to arbitrary IDs.
- [ ] Declare tasks, task/template/source-disjoint train/development/calibration/evaluation partitions, candidate counts, negative-generation/mining policy, easy-to-hard versus from-start curricula, objective weights, stopping rules, seeds and RAM/VRAM/time/work budgets. Protected final populations remain untouched unless separately authorized.
- [ ] Pin the unmodified task-learning baseline and specify independent label oracles, false-negative checks, duplicate/permutation tests, acceptance thresholds and retained failure artifacts. Freeze a paired comparison plan before learning or inspecting evaluation outcomes.

Definition of done: a reviewable protocol and executable split/label/budget validation, not merely this roadmap entry. No new language-quality, scale or model-promotion claim.

### SML-CLM-1 — bounded native score and objective semantics

- [ ] Implement Rust-native masked XOR/XNOR/POPCOUNT compatibility with explicit observed/relevant/unambiguous support, fixed normalization rules, empty-support rejection and mask-collapse controls. Qualify against an independent scalar oracle, including padding, overflow and budget failures.
- [ ] Specify multi-positive state-question-to-decision learning first; test repeated valid actions and no-valid-candidate cases. Bidirectional training is an optional separately controlled arm, not an assumed one-to-one mapping.
- [ ] Define a bounded research-only InfoNCE reference and a distinct native integer-margin/discrete-credit objective. Test stable numerical behavior and degeneracies of the reference, and exact deterministic semantics of the integer arm. Declare how latent/discrete updates are learned; a Hamming identity is not a trainability result.

Definition of done: deterministic conformance tests, independent reference agreement and explicit batch/negative-pool/working-memory bounds. Research log/exp or batch matrices must not enter the Boolean core or become all-history inference scoring; native candidate work stays bounded and directly routed.

### SML-CLM-2 — matched decision-learning comparison

- [ ] Compare current task-answer training, the declared InfoNCE arm and the integer-margin arm with the same accessible information, declared capacity and matched training budgets. Keep task/language objectives explicit and compare auxiliary-loss weights only on permitted development data.
- [ ] Use verified hard negatives for negation, role reversal, stale information, incorrect associations and missing evidence. Compare curriculum schedules; audit multi-positive masking and reject ambiguous/unverified negative labels. Optional teacher proposals must be independently checked and teacher-free controls retained.
- [ ] Report held-out decision accuracy, ranking quality, abstentions, per-expert exposure/update counts and task regressions with repeated seeds and the preregistered uncertainty procedure. Retain ties, negative results and failed hypotheses.

Definition of done: attributable learning evidence against the unchanged task baseline at declared costs. Adoption requires the predeclared benefit and regression gates, not just a lower contrastive loss. This does not by itself satisfy SML-1, broad D1 or SML-2.

### SML-CLM-3 — calibration and selective decisions

- [ ] Feed qualified decision scores into SML-D2; fit proper-scoring/calibration and thresholds on reserved non-test data, then freeze them. Report Brier/proper-score evidence where applicable, risk/coverage and false acceptance with uncertainty.
- [ ] Test option-count changes, reordered/duplicate candidates, absent correct candidates, unknown inputs and distribution shift. A relative candidate softmax or absence of SBG ambiguity is not a calibrated correctness probability.

Definition of done: preregistered error/coverage requirements on untouched evaluation data; typed abstention remains available and decision proposals still require separate action authorization. Ranking success cannot waive D2 or D5.

### SML-CLM-4 — SBG/MOR, memory and recurrent integration

- [ ] **4a / D3:** evaluate contrastively learned bounded shortlists against current direct routing, fixed/random controls and a separately budgeted exhaustive research baseline. Separate shortlist recall from final decision correctness; include expert exposure imbalance, selected pages and cold/warm costs.
- [ ] **4b / memory:** evaluate relevant-versus-stale/wrong/missing-evidence retrieval on new development fixtures inspired by the DKV-2c observation. Keep qualified DKV-2b and immutable raw verification; do not promote dynamic re-keying or bypass CONTEXT prerequisites. Expose observable miss evidence rather than asking scores to infer unavailable information.
- [ ] **4c / D4:** learn DIRECT / REFINE / ABSTAIN from observable state features and verified outcome changes under a fixed budget. Compare fixed and adaptive recurrent depth; measure useful corrections, harmful changes, coverage, cumulative active work and latency. Evaluator truth must not enter the controller's inputs.

Definition of done: separately ablated integrations, each preserving SBG observation semantics, non-attention audits, Elastic admission and resource gates. Do not bundle all three changes into one unidentifiable improvement claim or add a CLM runtime dependency.

### SML-CLM-5 — SML-specific scaling surfaces

- [ ] On qualified small tasks, vary unique data, decision-expert capacity, learned representation capacity/SBG width and recurrent depth, first with other factors controlled and then in joint bottleneck experiments. Distinguish tokens from pairs, repeats from unique data, field width from learned parameters, and total atlas capacity from active capacity and touches.
- [ ] Report quality at matched budgets and cost at matched quality. Include per-expert exposure/starvation, packed bits/metadata, RAM/RSS/VRAM, logical/word operations, training/search/state-construction costs, routing, page materialization, verification, wall-clock time, cold/warm p50/p95/p99 and traffic. Teacher/encoder work and one-off precomputation are separately included, never hidden by caching.
- [ ] Compare InfoNCE only under a frozen candidate/positive/negative/temperature/evaluation protocol. Fit empirical curves only over measured ranges, report repeated-seed uncertainty and residuals, check bottlenecks and loss-floor alternatives, and retain unsuccessful fits. Rank-loss, task-quality and cost curves remain separate evidence.

Definition of done: reproducible measured surfaces and a justified next scale experiment, not a universal power law. No 310-tokens-per-parameter prescription, invented exponent or extrapolated 25B quality/throughput claim; existing SML-0 through SML-8 gates remain controlling.

### SML-CLM-6 — optional external comparison and ecosystem transfer

- [ ] After verifying the actual CLM installation on Thor, pin checkpoint/encoder/pooling and evaluate it only as an external comparator or bounded training-data proposer. Match tasks/information and distinguish native typed fixtures from any text serialization. Include full cold/warm encoder, head, cache and verification costs; do not attach the published head directly to SBG.
- [ ] Inspect destination contracts before sharing qualified Rust score/label tests with SciRust, candidate/negative-search ideas with Forge, matched panels with TDI, resource telemetry with ElasticXxx or provenance-aware memory controls with CCOS-Enterprise. Require independent destination validation; do not add mandatory inference dependencies.

Definition of done: traceable optional comparisons or independently requalified reuse. External CLM availability is not a prerequisite for native SML-CLM-0 through SML-CLM-5, and no external baseline result is a native SML result.

### Admission and execution rules

The immediate next slice is SML-CLM-0, then SML-CLM-1; later runs require the preceding semantic and evidence contracts plus the relevant SML-D/CONTEXT/DKV gates. Source verification, labels and bounded training semantics precede large runs. New SML implementation is Rust-native, preserves `forbid(unsafe_code)` and the dependency-free core, and leaves language decoding optional for decisions.

Use Memorithm/RemoteOps for remote execution with exact task/code/data identities and explicit resource envelopes. Store immutable configurations and complete evidence, not only best scores. Do not alter protected evaluation populations, execute external actions or promote an RSI candidate through this track; the existing human RSI merge gate remains unchanged. This documentation change launches no training, installs no model and completes none of the gates above.

## DKV-3a-0 patch hardening (owner-requested correction)

Contract: [SML-DKV-3A-0-HARDENING.md](SML-DKV-3A-0-HARDENING.md).
Historical negative observations are preserved separately; the two cumulative
patches must not be applied on top of one another.

- [x] Implement bounded exact-key source-evidence adapter in sml-store, retaining the unchanged numeric core.
- [x] Add private query/context-bound SHA-256 verification receipts, typed failures, ordinary-error rollback and source/write/element admission.
- [x] Add independent raw-fixture resolution, exact extracted fields, complete conflict/supersession history, artifact corruption and geometry conformance tests.
- [x] Separate software conformance from machine-readable promotion eligibility; missing projected-recurrence evidence remains blocking.
- [ ] Independently preregister projected-key DKV-2a plus MOR/SBG recurrent integration (reserved for the next owner-supplied tranche).
- [ ] Qualify matched task/information/resource controls, persisted sidecar recovery and end-to-end cost on that separate programme.

Implementation checkboxes do not assert scientific qualification or completed
CI; exact-head CI evidence belongs to the correction PR. Existing DKV-3a,
SML-CONTEXT, SML-D, SML-CLM and RSI promotion boundaries remain unchanged.

## DKV-3a-R1-A — independent projected recurrence candidate

Protocol: [SML-DKV-3A-R1-A-PREREGISTRATION.md](SML-DKV-3A-R1-A-PREREGISTRATION.md).
The owner authorized this independent implementation after PR #99; the earlier
deferred-tranche note is superseded for this named Development candidate only.

- [x] Freeze an independently identifiable protocol before execution.
- [x] Exercise actual MOR -> DKV-2a -> frozen-key DKV-2b -> SBG recurrence.
- [x] Include direct/no-memory, fixed-depth and one-read cache controls.
- [x] Retain 864 paired Development rows, per-cycle traces and exact projected geometry.
- [x] Keep latent correctness separate from independently verified source resolution.
- [ ] Compare interfaces, information access and controls with the owner's implementation.
- [ ] Independently qualify full context integration, physical resources and crash recovery.

These are engineering/development milestones, not trained-model or language results.
Scientific promotion remains BLOCKED and no protected final evaluation is armed.

## A2 complementary projection Development follow-up

- [x] Freeze a separate protocol before implementation/execution: `SML-DKV-3A-R1-A2-PREREGISTRATION.md`.
- [x] Add isolated standard/contrast/direct arms with actual MOR, frozen keys, cached DKV-2b recurrence and independent payload/seed crossing.
- [x] Retain complete deterministic Development output and exact identities in `research/dkv-r1a2/`.
- [ ] Independently confront the owner's counterpart and qualify any mechanism beyond the synthetic Development panel.
- [ ] Qualify source-safe context, physical resource costs and representative tasks before context promotion.

These checkboxes describe an implemented/executed diagnostic, not model
quality or architectural promotion. Candidate A is not overwritten;
higher accuracy at greater projection work is not an equal-cost win.


## Counterpart B integration (ten engineering slices)

- [x] Preserve input identity and freeze comparison scope before new executions.
- [x] Reproduce supplied fixed-route probe on pinned Rust1.89.
- [x] Add exact signed Gram diagnostics and partial/complete resolution separation.
- [x] Exercise actual MOR with controlled Hamming fixtures.
- [x] Bind projected candidates to exact SubjectKey and the same complete verified snapshot.
- [x] Regress real aliases, absent subjects, omitted versions, forks and conflicts.
- [x] Retain/replay the separate 1728-row differential diagnostic without tuning it.
- [ ] Qualify learned/representative tasks, source-safe deployment, durable sidecars and physical costs.

See `SML-DKV-B-COMPARISON-OBSERVATION.md`. Checked engineering boxes do not
qualify a trained model or promote context. No old experiment is overwritten.

Counterpart B composition: verified binding now offers consuming subject-bound
screening and regression tests for projected aliases/conflicts/unknowns.
The ten-PR input-integration audit is `research/dkv-b/BATCH10-AUDIT.json`.
This is software conformance only; no new scientific promotion.

## Counterpart C integration

- [x] Freeze input and adapted diagnostic before execution.
- [x] Port exact-distance states and distinguish full-state identity from FNV.
- [x] Add bounded, borrowed record-source views without answer acceptance.
- [x] Test stream/record/source limits and complete subject-version separation.
- [x] Retain and replay all 3072 adapted rows under Rust1.89.
- [ ] Independently qualify learned/representative tasks and physical costs.

See `SML-DKV-C-OBSERVATION.md`; engineering completion does not promote context.




## DKV-2a-N1 ambiguity-neutral projection candidate

Protocol: [SML-DKV-2A-N1-AMBIGUITY-NEUTRAL-PREREGISTRATION.md](SML-DKV-2A-N1-AMBIGUITY-NEUTRAL-PREREGISTRATION.md).

- [x] Triage the supplied DKV-4b/4c patch without reusing its now-conflicting DKV-4 labels.
- [x] Isolate the current bipolar zero-ambiguity common-vector mechanism in the DKV-2a projector.
- [x] Freeze an active-only ambiguity candidate while keeping the qualified projector unchanged by default.
- [x] Implement explicit bipolar/active-only ambiguity modes with exact compatibility and work-counter tests.
- [x] Execute the Rust1.89 ZERO/SPARSE/DENSE_CONTROL load and key-geometry panel; retain byte-identical evidence in `research/dkv-2a-n1/`.
- [x] Re-run the frozen key-level ZERO near-miss panel; ActiveOnly does not increase exact-key aliases in any cell.
- [ ] Re-run exact semantic-record/subject-bound alias controls with ActiveOnly before any projector promotion.
- [ ] Measure representative SBG ambiguity density before deciding the dense-ambiguity tradeoff.
- [ ] Compare physical cost and false-acceptance behavior before combining with projection agreement or DKV-4.

The supplied Rust1.97 recall/cosine numbers are retained as external input observations. Current Rust1.89 observation: [SML-DKV-2A-N1-CURRENT-PANEL-OBSERVATION.md](SML-DKV-2A-N1-CURRENT-PANEL-OBSERVATION.md). ActiveOnly remains non-default because DENSE_CONTROL regresses.


## DKV-2a-N2 page/distribution requalification

Protocol: [SML-DKV-2A-N2-PAGE-DENSITY-REPLAY.md](SML-DKV-2A-N2-PAGE-DENSITY-REPLAY.md).

- [x] Triage the supplied DKV-4e patch against the merged ActiveOnly implementation.
- [x] Reject the duplicate SparseA constructor/core change; current ActiveOnly already supplies the mechanism.
- [x] Freeze a Rust1.89 replay for page diversity, page conflicts and exact A-density crossover.
- [x] Execute the current library through both Bipolar and ActiveOnly with byte-identical retained Rust1.89 output in `research/dkv-2a-n2/`.
- [x] Decide current MOR PageId weight 8 is not an adequate discriminator: wrong-page recall nearly equals own-page recall and two-page conflicts remain unresolved.
- [ ] Preregister and qualify a stronger bounded MOR/PageId key discriminator against N2 E1/E2 plus existing alias/provenance controls.
- [ ] Use representative SBG A-density measurements, not synthetic density alone, for projector promotion.

The source patch already contains Rust1.97 outcomes, so N2 hypotheses are imported replay targets, not blind preregistration. Current Rust1.89 observation: [SML-DKV-2A-N2-PAGE-DENSITY-OBSERVATION.md](SML-DKV-2A-N2-PAGE-DENSITY-OBSERVATION.md).


## DKV-2a-N3 ambiguity producer and page conditioning

Input review: [SML-DKV-2A-N3-INPUT-REVIEW.md](SML-DKV-2A-N3-INPUT-REVIEW.md).

- [x] Triage the supplied DKV-4f/4g/4h diff without reusing its conflicting DKV-4 labels.
- [x] Keep ActiveOnly as the existing A mechanism; reject the duplicate SparseA constructor.
- [x] Record additive PageSpread as a negative source result, not a library candidate.
- [x] Freeze N3A current-Rust1.89 replay using the repository's actual derive_ambiguity producer.
- [x] Freeze N3B current-Rust1.89 PageSeeded replay using only the existing projector API plus an outer page-derived seed.
- [x] Execute and retain byte-identical N3A Rust1.89 evidence; all five imported replay hypotheses hold.
- [x] Execute and retain byte-identical N3B Rust1.89 evidence; PageSeeded gives 0% wrong-page and 100% conflict resolution on the frozen one-page panel.
- [ ] PageSeeded reproduced; freeze multi-expert semantics, unbalanced-page controls and record/subject alias requalification before any core API.
- [ ] Measure representative recurrent SBG ambiguity, not just independent-bit hypothesis noise, before A-mode promotion.

Protocols: [N3A](SML-DKV-2A-N3A-HYPOTHESIS-AMBIGUITY-REPLAY.md) and [N3B](SML-DKV-2A-N3B-PAGE-SEEDED-REPLAY.md). Observations: [N3A](SML-DKV-2A-N3A-HYPOTHESIS-AMBIGUITY-OBSERVATION.md), [N3B](SML-DKV-2A-N3B-PAGE-SEEDED-OBSERVATION.md). No context/model promotion.


## DKV-2a-N4 recurrent-chain requalification

Input review: [SML-DKV-2A-N4-INPUT-REVIEW.md](SML-DKV-2A-N4-INPUT-REVIEW.md).
Protocol: [SML-DKV-2A-N4-CHAIN-REPLAY.md](SML-DKV-2A-N4-CHAIN-REPLAY.md).

- [x] Triage the supplied DKV-4i/4j diff against the merged N3 gates.
- [x] Reject premature combined/PageSeeded core API until multi-expert and DKV-3a gates are satisfied.
- [x] Retain value-field weighting as negative source evidence; do not import the option.
- [x] Freeze current-Rust1.89 replay of the combined ActiveOnly + outer PageSeeded arm on the historical recurrent-chain panel.
- [x] Execute and retain byte-identical N4 Rust1.89 chain evidence in `research/dkv-2a-n4/`.
- [x] Preserve the remaining per-hop capability failure: low-load combined ORACLE is L0=16/16, L1=16/16, L2=13/16, L3=8/16 at every K; no cell is capable.
- [x] Freeze N5 per-hop teacher-forced read / free-running rewrite / reprojection fidelity protocol before proposing another projector mechanism.
- [x] Execute and retain byte-identical N5 Rust1.89 evidence; every low-load failed L2/L3 chain first diverges on a teacher-forced associative read.
- [x] Freeze N6 terminal-write crosstalk decomposition: SELF vs same-chain vs count-matched cross-chain vs FULL, plus key geometry.
- [x] Execute and retain byte-identical N6 Rust1.89 evidence: same-chain correlation ~0.67 dominates count-matched cross-chain interference and reproduces 10/11 K1024 FULL L2/L3 failures.
- [x] Freeze N7 harness-only PAGE_VALUE domain conditioning from exact V; include N6 crosstalk, N4 chain capability and 1/2/4-bit brittleness controls.
- [x] Execute and retain byte-identical N7 evidence: exact-V conditioning restores K1024 capability to 100% but one-bit perturbations yield 0/96 retrieval, so direct promotion is rejected.
- [x] Freeze N8 fixed-width PAGE/PAGE_VALUE hybrid-domain tradeoff at q={0,1/4,1/2,3/4,1}, with capability and Hamming-robustness gates.
- [x] Execute and retain byte-identical N8 evidence; q=3/4 is the smallest interior fraction passing both low- and high-load capability/robustness gates.
- [x] Freeze N9 canonical multi-expert domain semantics: PRIMARY_ANCHORED candidate versus ORDERED_ROUTE isolation control.
- [x] Execute and retain byte-identical N9 evidence; PRIMARY_ANCHORED passes continuity/capability/robustness but H9 exact-collision gate fails, so canonical selection is NONE.
- [x] Freeze N10 fixed-width PRIMARY_ANCHORED/ORDERED_ROUTE interpolation at r={0,1/4,1/2,3/4,1}, keeping q=3 fixed.
- [x] Execute and retain byte-identical N10 evidence; no interior route fraction clears both secondary-conflict isolation and secondary continuity, so selection is NONE.
- [x] Freeze N11 two-bank PRIMARY shared retrieval + ORDERED route-specific override resolver with an explicit record/route binding directory; retrieval strength is never presence authority.
- [x] Execute and retain byte-identical N11 evidence; all ten gates hold and the two-bank resolver is retained as Development memory-composition semantics.
- [x] Freeze N12 DKV-3a verified-write-locator requalification for the retained two-bank resolver, using current RecordRequest/VerifiedContext/VerifiedHistory/subject-binding APIs and existing Missing/Conflict/Unverified semantics.
- [x] Execute and retain byte-identical N12 Rust1.89 evidence; all ten identity/provenance gates hold after explicit preregistered wrong-candidate H6 requalification.
- [x] Exact record/subject/provenance qualification is complete for the retained N11 two-bank Development semantics.
- [x] Freeze N13A prototype-native census over learned SML-1 route traces, retained q=3 recurrent SBG states and native dynamic-MOR controls; explicitly not claimed representative of a future 25B model.
- [x] Execute and retain byte-identical N13A census evidence; learned SML-1 pages are strongly skewed (page8=43.06%, page2=36.81%, page9=0) while the retained q=3 recurrent proxy remains ambiguity-sparse.
- [x] Preregister N13B after retained N13A values: BALANCED=[15,15,15,15,14,14,14,14,14,14], empirical=[2,4,53,2,4,6,2,9,62,0], independent heavy-skew=[116,4,3,3,3,3,3,3,3,3].
- [x] Execute and retain byte-identical N13B evidence; all ten frozen gates hold across balanced, N13A-empirical and independent heavy-skew populations; default primary-page isolation and explicit replicated cross-primary sharing are retained as Development semantics. See [SML-DKV-2A-N13B-PAGE-SKEW-OBSERVATION.md](SML-DKV-2A-N13B-PAGE-SKEW-OBSERVATION.md).
- [x] Freeze N14 physical-resource protocol for the retained q=3/two-bank/N12-verified composition, with K256 width control, verified PRIMARY and mixed two-bank workloads, direct-matrix diagnostic, unprivileged Linux latency/RSS/I/O/page-fault/CPU-runtime instrumentation and fail-closed energy availability. See [SML-DKV-2A-N14-PHYSICAL-PREREGISTRATION.md](SML-DKV-2A-N14-PHYSICAL-PREREGISTRATION.md).
- [x] Implement and execute N14 physical qualification on the retained candidate; run #708 retains 24 cells with H1-H5 holding everywhere, H6/H7 measured as preregistered, zero measured block-device bytes, zero major faults and exact one-read semantics. See [SML-DKV-2A-N14-PHYSICAL-OBSERVATION.md](SML-DKV-2A-N14-PHYSICAL-OBSERVATION.md).
- [ ] Complete the N14 energy gate on a pinned host exposing a readable monotonic energy counter. The current aarch64 runner reports no supported `energy_uj` domain in all 24 cells, so H8 is unavailable/refuted and PageSeeded/two-bank core API promotion remains blocked.
- Historical owner input DKV-4n is retained as a negative key-density result: no `with_context_divisor` setting jointly preserves chain recall and same-V/different-context twin separation, so no density API enters the current core. See [SML-DKV-4N-INPUT-REVIEW.md](SML-DKV-4N-INPUT-REVIEW.md).

Observation: [SML-DKV-2A-N4-CHAIN-OBSERVATION.md](SML-DKV-2A-N4-CHAIN-OBSERVATION.md). The combined arm materially improves F6, including 23.05% -> 71.48% at K1024/G16, but no historical capability cell passes. No controller or context/model promotion follows.

N5 localization: [SML-DKV-2A-N5-HOP-FIDELITY-OBSERVATION.md](SML-DKV-2A-N5-HOP-FIDELITY-OBSERVATION.md). The remaining low-load L2/L3 failure is already present in exact-state teacher-forced retrieval; free-running drift/rewrite/reprojection are not the first failing stage.

N6 crosstalk: [SML-DKV-2A-N6-CROSSTALK-OBSERVATION.md](SML-DKV-2A-N6-CROSSTALK-OBSERVATION.md). SELF is perfect; count-matched cross-chain controls are perfect; same-chain shared-M/R keys remain ~0.67 correlated and reproduce most L2/L3 failures.

N7 value-domain result: [SML-DKV-2A-N7-VALUE-DOMAIN-OBSERVATION.md](SML-DKV-2A-N7-VALUE-DOMAIN-OBSERVATION.md). PAGE_VALUE removes same-chain correlation and reaches 100% K1024 capability, but exact-V conditioning is rejected for direct promotion because a one-bit V perturbation produces near-orthogonal keys and 0/96 next-value recovery.

N8 hybrid result: [SML-DKV-2A-N8-HYBRID-OBSERVATION.md](SML-DKV-2A-N8-HYBRID-OBSERVATION.md). At fixed K, q=3/4 PAGE_VALUE is selected at both G=4 and G=16: K1024 F6=100% / 91.80% while Hamming-1 exact+clear recovery remains 25/96=26.04%.

N9 multi-expert result: [SML-DKV-2A-N9-MULTIEXPERT-OBSERVATION.md](SML-DKV-2A-N9-MULTIEXPERT-OBSERVATION.md). PRIMARY_ANCHORED preserves secondary continuity and K1024 q=3 capability, but exact R2/R3 conflict-key collisions refute H9; ORDERED_ROUTE isolates conflicts but gives zero secondary continuity.

N10 route-hybrid result: [SML-DKV-2A-N10-ROUTE-HYBRID-OBSERVATION.md](SML-DKV-2A-N10-ROUTE-HYBRID-OBSERVATION.md). Geometry interpolates smoothly, but r=2 retains ~90% continuity with only 55.47% conflict recall while r=3 reaches 98.83% conflict recall with <8% continuity; no single-key interior candidate is selected.

N11 two-bank result: [SML-DKV-2A-N11-TWO-BANK-OBSERVATION.md](SML-DKV-2A-N11-TWO-BANK-OBSERVATION.md). PRIMARY preserves secondary-route continuity while ORDERED carries explicit route-specific overrides; at K1024 all ten frozen gates hold, with 100% P1/P2/P3 continuity/override recall, 100% G4 F6, 92.97% G16 F6, 29.17% Hamming-1 recovery, zero non-bound false overrides and exactly one resolved bank read per directory probe.

N12 verified-identity result: [SML-DKV-2A-N12-VERIFIED-IDENTITY-OBSERVATION.md](SML-DKV-2A-N12-VERIFIED-IDENTITY-OBSERVATION.md). All ten source/identity gates hold: clean continuity and route overrides remain 100% at K1024, Missing/Conflict/Unverified are zero-read fail-closed outcomes, the supersession control preserves an explicitly wrong candidate rather than repairing it, and 2,306 VerifiedPresent resolutions correspond to exactly 2,306 selected-bank matrix reads.

N13A prototype census: [SML-DKV-2A-N13A-RUNTIME-CENSUS-OBSERVATION.md](SML-DKV-2A-N13A-RUNTIME-CENSUS-OBSERVATION.md). The learned SML-1 route population is already 31x imbalanced among nonzero pages; the exact reduced vector is `[2,4,53,2,4,6,2,9,62,0]`. N13B protocol: [SML-DKV-2A-N13B-PAGE-SKEW-PREREGISTRATION.md](SML-DKV-2A-N13B-PAGE-SKEW-PREREGISTRATION.md).

## DKV projection-agreement follow-up

Input review: [SML-DKV-PROJECTION-AGREEMENT-INPUT-REVIEW.md](SML-DKV-PROJECTION-AGREEMENT-INPUT-REVIEW.md).

- [x] Triage the supplied DKV-4 patch against current provenance, aliasing and adaptive-control contracts.
- [x] Reject the legacy FNV/unbounded context adapter and older controller where current main is stronger.
- [x] Retain separately seeded projection agreement as a research-only abstention/compute hypothesis, not a source/correctness signal.
- [ ] Preregister a current-Rust1.89 replay with 1/3/5/9 members and exact/nearest-neighbour matched controls.
- [ ] Extend the replay into storage-matched write counts in the hundreds and retain key-width false-acceptance behavior.
- [ ] Qualify selective-risk/coverage plus physical latency/RSS/traffic before exposing any runtime signal.
- [ ] Only after separate qualification, compare projection agreement with DKV-4/applicability in a factorial ablation.

This follow-up does not alter the current DKV-3a acceptance policy, DKV-4h protocol or context/model promotion boundary.

## DKV-4 development completion

- [x] Implement typed `Refine / StopCandidate / Abstain` compute control with no
  `Accept` action.
- [x] Bind executed DKV-2b cycle evidence, exact SBG V/M/A/R deltas and MOR
  disagreement/escalation into a bounded controller.
- [x] Enforce `Missing/Conflict` zero-read abstention and prevent `Unverified`
  evidence from stopping a candidate.
- [x] Freeze and execute paired K=64/K=256 Development controls with immutable
  evidence and exact code/protocol identities.
- [x] Preserve DKV-4a as negative evidence after ~1.95x stability-only cycles
  without a correctness gain.
- [x] Execute DKV-4c semantic-impact follow-up: written correctness preserved;
  cycles/read-MACs reduced by 48.78% at K64 and 49.08% at K256 versus DKV-4a.
- [ ] Demonstrate benefit beyond SBG stability-only on a separately frozen,
  non-trivial MOR-disagreement/escalation panel.
- [ ] Qualify learned/representative states and physical latency/RSS/traffic/energy.

The checked items close the engineering/development milestone only. They do not
promote Delta-KV context integration or establish model-quality superiority.

## PGF2 — Pascal-GF(2) reversible Boolean mixer

Status: global and block-local K=2048 representation-mixing qualification is
complete with retained negative SBG results. The current Pascal-as-general-SBG-
representation-mixer branch is closed. The durable architectural contract is in
`BOOTSTRAP.md#pascal-gf2--sierpiński-reversible-mixer-research-track`.

### Research question

Can the self-inverse Pascal/subset-zeta transform over GF(2) provide a useful
hierarchical reversible mixing primitive for SBG at lower or comparable cost
than existing reversible Boolean controls, without introducing attention or
floating-point semantics?

### Milestones

- [x] **PGF2-0**: scalar/binomial oracle, exhaustive small-width equivalence and
  `P(P(x)) == x`.
- [x] **PGF2-1**: packed Rust `u64` butterfly with exact oracle parity and
  operation accounting.
- [x] **PGF2-2**: fixed width ladder 64/128/256/512/1024/2048 bits; PR #197
  qualifies K=2048 at 11,264 logical XOR updates, 272 packed-word updates,
  256 state bytes and zero scratch words.
- [x] **PGF2-3**: frozen K=2048 SBG ablation versus identity, fixed reversible
  permutation and current qualified Boolean mixer; preregistered in
  [SML-PGF2-3-SBG-ABLATION-PREREGISTRATION.md](SML-PGF2-3-SBG-ABLATION-PREREGISTRATION.md)
  and retained in
  [SML-PGF2-3-SBG-ABLATION-OBSERVATION.md](SML-PGF2-3-SBG-ABLATION-OBSERVATION.md).
- [ ] **PGF2-4**: **BLOCKED for global and block-local K=2048 PGF2**. PGF2-3
  and PGF2-3B both select `NONE`; no MOR/Delta-KV handoff is authorized.
- [ ] **PGF2-5**: blocked because the component gate did not pass.

PGF2-3 retained result: exactness, random-control sanity, the preregistered
local-noise bound and structural-cost bound hold, but the required SBG benefit
does not. On STRUCTURED_LOCAL, global PGF2 contracts class nearest-wrong median
from 32 to 12 and route median from 64 to 6; at h=64, class retrieval falls from
100% to 42.38% and route accuracy from 100% to 57.03%. H2/H3/H4 are refuted and
`PGF2-4=NONE`. The negative result is retained in `research/pgf2-3/`.

Any future block-local, reordered or differently oriented Pascal construction is
a new candidate requiring an independent preregistration; it does not reopen or
overwrite the global PGF2-3 result.

- [x] **PGF2-3B**: block-local 64/128/256/512 rescue executed and retained.
  Locality reduces amplification and structural work and B64 improves the failed
  global candidate at h=64, but no width satisfies the required structured
  geometry + low/moderate-noise joint gate. Selection is `NONE`,
  `representation_mixing=CLOSE`, and PGF2-4 remains blocked. Protocol:
  [SML-PGF2-3B-BLOCK-LOCAL-PREREGISTRATION.md](SML-PGF2-3B-BLOCK-LOCAL-PREREGISTRATION.md);
  observation:
  [SML-PGF2-3B-BLOCK-LOCAL-OBSERVATION.md](SML-PGF2-3B-BLOCK-LOCAL-OBSERVATION.md).

### Mandatory measurements

- exact transform/inverse correctness;
- logical Boolean operations and temporary bytes;
- wall-clock latency only from declared measured backends;
- collision and retrieval statistics;
- intra/inter-class Hamming separation;
- bit-flip/noise stability;
- MOR candidate/routing changes when tested;
- Delta-KV interference/read quality when tested;
- downstream task quality for model-level work.

No stage may infer semantic superiority from invertibility, Sierpiński structure
or asymptotic operation count alone.



## Pascal Vector Projection (PVP) hardware-alignment track

This track follows retained PB-9 evidence and does not change the closed PGF2 representation-mixing result.

- [ ] **PVP-0** — versioned address-major bitplane layout; exact pack/unpack/transpose and scalar oracle.
- [ ] **PVP-1** — SciRust CPU candidate matrix: scalar u64, NEON, AVX2, AVX-512, SVE where available; correctness before timing.
- [ ] **PVP-2** — FLAT portable WGSL scalar-u32 and `vec4<u32>` butterfly parity.
- [ ] **PVP-3** — multi-stage workgroup fusion, subgroup and double-buffer candidates with bounded resource accounting.
- [ ] **PVP-4** — NNIS portable CPU/WGPU session carrier over the same immutable layout contract.
- [ ] **PVP-5** — exact cross-backend parity, real-device manifests and deterministic autotuning keyed by problem/device identity.
- [ ] **PVP-6** — prospectively declared massive-ANF-bank model task; compare against competent sparse/direct/resident controls.
- [ ] **PVP-7** — internalize only a qualified minimal execution contract into self-sufficient SML; external repositories remain optional.
- [ ] **PVP-8** — freeze Model/Harness interface and create the separate `Memorithm/SML-HARNESS` repository.

The PVP target path is CPU/SIMD/WGPU/open-GPU. No CUDA, NVRTC, cuDNN, TensorRT/TensorRT-LLM, NVML, CUTLASS or vendor SDK is a required dependency; only the installed GPU driver may be vendor-specific. A hardware microbenchmark cannot close PVP-6, and a model-quality result cannot be inferred from kernel throughput.


## Architecture freeze programme — 2026-10-07

The user-approved seven-gate validation sequence is recorded in
[SML-ARCHITECTURE-DECISION-GATES.md](SML-ARCHITECTURE-DECISION-GATES.md).
The first implementation is the
[PVP6 ANF fixture and complete-output contract](SML-PVP6-ANF-FIXTURE-CONTRACT.md).
This reference qualification does not close PVP-6 or select a model architecture.
RemoteOps remains the physical execution control plane; trained-model export,
native task comparisons, recurrent integration, learning, ablations and scaling
remain separate evidence gates.

## PVP6 model-owned executor qualification — 2026-10-08

Destination check: [SML-PVP6-MODEL-EXECUTOR-QUALIFICATION.md](SML-PVP6-MODEL-EXECUTOR-QUALIFICATION.md).
The public model API is compared against the unchanged 144-cell synthetic corpus,
with complete Q*R outputs, both layouts and padding. The seven executor tests
also cover width/word boundaries, false density and changed coefficients.
This narrows an implementation gap; it does not mark PVP-6/PVP-7, a recurrent
model, learning or the architecture-freeze gate complete.

Next slice: freeze the SML recurrent bank/state-update contract and direct-evaluation
control, then compare full bounded-sequence trajectories with coefficient changes.
RemoteOps controls physical runs; partner kernels remain optional. Do not infer
new executor latency from the historical selector-coordinate measurements.

## A4 recurrent destination reference — 2026-10-08

A bounded, model-owned `pvp6_recurrent` execution-only reference is introduced.
It evaluates a sequence of canonical ANF banks with explicit replacements, bounded
step count, complete query-major output per step, and deterministic final output
state. It compares to an independent direct-monomial oracle and rejects malformed
updates without mutating the initial bank. This does **not** yet integrate the
actual SBG/MOR/Delta-KV loop, learned banks, memory, or measured latency. A4
remains open pending complete routing/state/memory/output integration, varied
sequence lengths and independently qualified end-to-end cost.

## A4 SBG/ANF ten-slice functional integration

Contract and limits: [SML-A4-SBG-INTEGRATION.md](SML-A4-SBG-INTEGRATION.md).
The native SemanticRewriter path now feeds current V/M/A/R state into declared
ANF wiring, with revisioned bank replacement, transactional stage composition,
logical budgets, native MOR decisions and DKV-2b integration controls.
The golden public-API panel and independent std-only replay are mandatory in
the existing fixture CI step. They validate 192 cases and 720 complete cycles;
the separate DKV/ANF/MOR integration matrix contains 48 trajectories.

These close the declared synthetic execution checks, not the complete A4 gate.
Next model work must qualify dynamic key/state coupling and actual selected
expert-page execution, larger declared or learned integration banks, persistent
full-pipeline checkpointing and controlled end-to-end cost. A5 learning, A6
matched architectural ablations and A7 scaling remain open. Final SML remains
self-contained and partner runtimes optional; RemoteOps retains physical control.

## A4 next ten slices — 2026-10-09

The [selected expert / dynamic memory contract](SML-A4-EXPERT-DYNAMIC-LOOP.md)
freezes the next functional integration sequence: typed ANF pages, sparse selection,
transactional execution, current-route dynamic keys, coupled state loop, logical
budgets, coefficient revisions, full owned continuation and direct-control replay.
These are declared model semantics, not a learned bank or physical performance
promotion. Persisted checkpoints and full-scale training remain separate gates.

## A4 closed-loop functional evidence — 2026-10-09

The selected expert / dynamic-memory ten-slice implementation adds actual page
execution, post-expert/current-route key projection, revisioned updates and owned
whole-pipeline continuation. The declared small-state panel has 108 cases and
456 cycles with exact independent report replay. All adaptive panel cases hit
their cap, so no stopping-efficiency or performance improvement is claimed.

Next qualifications: broader declared SBG/domain/selection geometries; quality-
gated learned page export; persistent authenticated whole-pipeline checkpoints;
then separately admitted physical end-to-end cost and matched learning/ablation
gates. Do not infer those results from the synthetic closed-loop equality checks.

## A6-C12c parity source screening — bounded 508-source evidence (2026-10-12)

[Pre-registration](A6-C12C-SKETCH-MITM-508-PREREGISTRATION.md) and
[measurements](RESULTS-A6-C12C-SKETCH-MITM-508.md) qualify the independent
synthetic three-gate N128 ring **as a diagnostic of source selection**, not
an end-to-end SML language-model training milestone. On 24 paired
task/panel/seed streams, the 508-feature hybrid of unrestricted Walsh pairs
and 14-bit sketch-based meet-in-the-middle recovered all 24 generator
positions, sources and Boolean tables. The exhaustive order-3 reference
also recovered 8/8 of its declared n508 subset; hybrid total fit was
35.06 times faster **on those eight matched cases**, including factorization.

Do not infer subcubic worst-case complexity: measured sketch bucket collisions
and W2 pair scans matter. The current research learner still assumes a fixed
128-state-bit delay ring and **at most three hidden gates**. No general
508-state-bit free-graph learning, SML-25B perplexity, GPU performance or
million-function-bank selection has been demonstrated.

Next evidence gates, each requiring prior registration and negative controls:
masked-variable identifiability proofs; correlated distractors and
non-i.i.d. label noise; adaptive selection among W3 exhaustive and W2/MITM
under matched cost/accuracy; eventual variable k=4/5 *with enough learner
capacity* and learned bank integration. The experiments must retain the
native Rust-only/hardware-independent boundary, and avoid software NVIDIA
dependencies beyond allowed GPU drivers.

## A6-C12d correlated-source identifiability — bounded diagnostic (2026-10-12)

[Pre-registered protocol](A6-C12D-CORRELATED-IDENTIFIABILITY-PREREGISTRATION.md)
and [measured results](RESULTS-A6-C12D-CORRELATED-IDENTIFIABILITY.md)
extend A6-C12c to 508 candidate features with deliberately correlated
raw channels, while preserving the **fixed N128 ring and three hidden gates**
limitation. The independent native 12-case near-alias panel showed
**12/12 exact source, position and ANF truth-table recoveries** with hybrid
W2+Sketch-MITM, plus native held-out prediction parity with the generator.

A deliberately **non-identifiable** panel set raw `x1=x2`. Two different
native networks reading x1 versus x2 at the same hidden position gave
identical predictions on **12/12 TEST streams** and are identical for the
entire constrained input domain by substitution. The hybrid model still
matched the generator functionally in **12/12** cases, although it could
not recover the generator's arbitrarily chosen source label (**0/12**).
Research evaluations must therefore distinguish *identical hidden
parameters*, *observable functional equivalence*, and *minimal identifiable
representations*; never count this structural ambiguity as a learning
failure.

On four matched streams per correlation mode, the native W3 exhaustive
reference was approximately 34.99× (near alias) and 35.21× (exact alias)
slower in total fit time than the hybrid. These are **matched, synthetic
measurements**, not proofs of subcubic complexity, arbitrary hidden graphs,
nor performance results for million-function ANF banks or SML-25B.

Next evidence gates: time-shifted and biased/correlated distractors,
partial observability and identifiability equivalence classes with formal
negative controls, state-transition noise and non-i.i.d. labels, then
k-variable capacity-controlled studies with four/five learnable ANF gates.
No cross-repository integration or inference-kernel promotion until
the identifiability and cost bounds are independently qualified.

## A6-C12e temporal observability and recurrent transition noise — 2026-10-12

The [pre-registered protocol](A6-C12E-TEMPORAL-STATE-PREREGISTRATION.md)
and [measured native report](RESULTS-A6-C12E-TEMPORAL-STATE.md) extend the
bounded source-identification programme to time-shifted aliases and actual
hidden recurrent-state perturbations, with **508 candidate source features
and only 128 native state bits / three hidden gates**.

With imperfect delayed aliases `x1(t)≈x2(t-1)` and `x3(t)≈x4(t-1)`,
the native W2+Sketch-MITM union recovered all **12/12** original sources,
positions and ANF truth tables and matched autonomous TEST errors. With
**exact** delayed equality, moving the intermediate gate from `j68/x3`
to `j67/x4` gives a distinct, observationally equivalent native circuit:
**12/12** zero-disagreement independent alternative-oracle witnesses.
The learner recovered **12/12** oracle-matched TEST functions but **0/12**
identical canonical internal structures; this is not a model failure
when the input domain cannot identify the source/position.

A stochastic teacher injecting Bernoulli(1/256) flips into the persisted
`state[127]` after each native step still yielded **12/12 recovery** of
the true three deterministic gate tables and source positions; selected
models matched the *deterministic* teacher's own error against the
stochastically perturbed labels. They **did not** predict unknown hidden
noise draws. The resulting TEST mismatch on clean labels depended strongly
on the circuit (mean 6.168% XOR³ vs 0.766% AND→XOR→XOR).
On four matched W3 references per mode, total fit was approximately
35.00×, 35.70× and 35.22× faster for the hybrid, respectively.
No theoretical worst-case cost bound follows.

**Next evidence gates:** characterize true observational equivalence classes
rather than comparing hidden labels blindly; stress more gates/slots/rates
of state noise, time-varying conditional perturbations and input masking
*after* a formal observability check; only then expand unknown gate count
k=4/5 with an appropriately capable learner. Do not promote this
three-gate N128 synthetic result to free graphs, memory banks of millions
of ANF functions, SML-25B language quality, GPU/VRAM/throughput claims,
or a general parity-with-noise solution. Native Rust remains the required
implementation boundary; no NVIDIA software dependencies beyond drivers.

## A6-C12f six-state-noise and strict native-output equivalence — 2026-10-12

The [frozen protocol](A6-C12F-STATE-NOISE-OBSERVABILITY-PREREGISTRATION.md)
and [measured report](RESULTS-A6-C12F-STATE-NOISE-OBSERVABILITY.md)
extend the native N128 fixed-ring **three-gate** source discovery benchmarks.
The new observable-equivalence gate compares two independently executed
networks' **output bits** on the same held-out inputs, without using TEST labels
or relying on equality of aggregate loss counts. These are fundamentally
different checks: matching held-out error counts is not a proof of equal
output functions.

Across 2 native tasks, clean/label-noisy panels, 3 seeds, 3 perturbed
state slots (0/68/127) and 2 hidden-transition flip probabilities
(1/256 or 1/64), the hybrid W2+Sketch-MITM learner recovered all **72/72**
original sources/positions/truth tables, and its independently executed
selected models disagreed with the deterministic reference on **zero bits
in 72/72 held-out streams**. A separate causal certificate establishes
that moving the middle XOR injection from j68/x3 to j67/x4 is
observationally equivalent **under x3(t)=x4(t-1) with zero initial history**
(6/6 native exact-lag witnesses), but distinguishable when the temporal
constraint is imperfect (6/6 controls). The certificate describes a
specific pair, NOT a complete equivalence-class enumeration.

The teacher's actual **persisted** hidden-state transitions are
stochastically perturbed; zero output disagreements between the learned
and deterministic reference do **not** mean that either predicts the
random disturbance. On clean-label stochastic targets, observed XOR³
error increased from ~6% at 1/256 to ~19% at 1/64, versus ~0.76%
to ~2.97% for AND→XOR→XOR. The 24 matched n508 W3 exhaustive
references averaged 5,573.193 ms total fit vs 158.610 ms for the hybrid,
a measured **35.138×** ratio on these native synthetic cases, **not
a worst-case complexity guarantee**. 508 refers only to candidate
sources, not to the number of state bits/gates.

**Next gates**: new classes under simultaneous hidden-slot flips and
temporally correlated noise; input masking/identifiability analyses;
observation-class representations and matched validation beyond
error-count equality; only then a separately preregistered learner
whose grammar can actually fit k=4 or 5 hidden gates. Keep quality,
bank-scale cost, native GPU/Thor performance and SML-25B token
generation as *independent unqualified goals*. Rust native only,
with no vendor software stack other than permitted GPU drivers.

## WideSparseANF k8–k1024 native gate and recurrent bridge evidence — 2026-10-12

A standalone [preregistered k8..1024 protocol](ANF-WIDE-SPARSE-1024-PREREGISTRATION.md),
its append-only [performance amendment](ANF-WIDE-SPARSE-1024-PERF-AMENDMENT.md)
and the [native measured report](RESULTS-ANF-WIDE-SPARSE-1024.md)
define the executable transition **away from dense `2^k` coefficient
storage**. The Rust `anf_wide_sparse` API accepts dynamic input indices,
canonical active monomials and explicit budgets, uses
`ceil(k/64)` mask words per monomial, caches repeated prefixes in
caller-owned `u64` lane scratch, and dispatches dense only for
`k<=8` with >=25% active coefficients. The old `LocalAnfGate` API
remains unchanged.

**Qualifying evidence:** 22/22 truth-panel rows across
k=8,16,32,64,128,256,1024 (8 input patterns ×64 lanes per row);
2/2 exact 256-step N128 native recurrent-model conversions;
1/1 LargeAnfGate 31-bit adapter; and a compiled/executed N1024
recurrent step with a k1024 gate. The 64-lane sparse kernels require
no dense `2^1024` coefficient table, no resident service, no `unsafe`
and no NVIDIA SDK. The validated source-bound check is O(1) per
call after construction; all runtime budgets are checked before
allocating packed masks and shared-prefix scratch.

**Performance guardrails:** report both before and after results
with their different runner identities; do not hide the negative
high-degree case. Post-optimization k1024 linear32 ≈42.739 ns/call,
mixed16 ≈55.535 ns/call, full degree1024 AND ≈2365.915 ns/call.
The AND case was ≈1341.112 ns/call in the pre-optimization benchmark,
so no blanket wide-fan-in acceleration is established. Likewise the
k8 dense128 reference is retained for comparison, not deprecated.
Memory metrics are logical *payload* and *scratch* only, not RSS.

**Next engineering gates before SML architecture promotion:**
1. An explicit MOR/SBG page and Delta-KV dataflow that can execute a
   selected `WideAnfGate` end-to-end and replay the same state exactly,
   with native bank-page activation and memory read accounting.
2. An independent **learning** workflow for wide sparse monomial
   discovery, fitness on truly held-out outputs, sparsity/degree
   constraints and bounded train/validation search; evaluating a
   1024-input polynomial does NOT mean fitting one.
3. SIMD/ARM-NEON and GPU-native kernel benchmarking on actual hardware
   with feature parity and zero external proprietary NVIDIA runtime,
   including scratch, bandwidth, timing and energy/power where measured.
4. K=1,048,576 **banked functions**, with bounded physical storage,
   dynamic choice of active pages, 64-lane batched execution and
   transparent resident versus mapped bytes; bank addresses,
   functions, physical gates and active monomials must remain separate.
5. Capacity-matched k4/k5 *hidden gate discovery*, avoiding false
   inference from the older fixed three-gate structure benchmarks,
   before moving toward larger free topologies.

No SML-25B perplexity, generated-token throughput, 100M active
gates, semantic retrieval improvement, general LPN solution or
worst-case subcubic complexity follows from these synthetic exactness
and CPU microbenchmark observations.

## WideSparseANF MOR-selected SBG pages and Delta-KV integration — 2026-10-12

After [PR #498's native wide ANF core](RESULTS-ANF-WIDE-SPARSE-1024.md),
the [predeclared qualification protocol](ANF-WIDE-SBG-MOR-DELTAKV-PREREGISTRATION.md)
and [measured tests](RESULTS-ANF-WIDE-SBG-MOR-DELTAKV.md) define the first
**real opt-in dataflow** through WideSparseANF → selected SBG expert pages
→ MOR route selection → current-route Delta-KV projection and update.
The existing `RoutedSemantic` and `DeltaKvDynamicRekeyKernel` perform
the routing/memory work; these mechanisms are not mocked.

Natively tested: one 1024-input sparse AND gate selected from an atlas
by a MOR decision, using 1024 SBG `value` input coordinates to write
`meaning_mask`; all-ones and 0/512/1023 one-zero controls; inactive
page exclusion; simultaneous selected-page snapshot semantics; duplicate
IDs, output conflicts and independent gate/trace/scratch budgets;
two full coupled-vs-manual MOR→WideSparseANF→Delta-KV cycles; atomic
rollback under bad provenance or subsequent budget failure. Fixed
routing, not a learned route policy; scalar SBG state broadcast to
u64 lanes, not 64 independent SBG evaluations.

**Follow-up gates:** (1) explicit page-atlas activation in the chosen
model runtime; (2) k>8 trainable sparse monomial discovery with
held-out evidence and resource caps; (3) actual multi-SBG vectorization
and NEON/SIMD/Thor measurements; (4) page cache and K=1,048,576
function-bank storage/selection validation; (5) separate language
model-scale and quality benchmarks. Keep the historical PVP6 and
LargeAnf page executors unchanged for regression comparison.
No NVIDIA toolkit dependency beyond permitted GPU drivers.

## A6-WL1 active discovery of wide sparse ANF monomials — 2026-10-12

The [scientific preregistration](ANF-WIDE-ACTIVE-SPARSE-LEARNING-PREREGISTRATION.md)
and [complete benchmark results](RESULTS-ANF-WIDE-ACTIVE-SPARSE-LEARNING.md)
qualify a new `anf_wide_active_learn` component with explicit
membership-query and resource-budget boundaries.

**Real measured milestone:** 42 train/validation-frozen supported
models on 21 distinct hidden supports, 7 widths
`k=8,16,32,64,128,256,1024`, 4 PUBLIC one-hot selectors, exactly
1 unknown positive-literal AND monomial per selector, 3 seeds,
TRAIN clean vs noisy (1/16 response flips, 15 majority votes).
Adaptive group interventions found **42/42 exact hidden supports**,
all 42 were accepted on independently generated 544-case
validation sets, and all 42 showed **zero bit disagreement on 4096
held-out random inputs per fit**. Six k8 exhaustive 256-input
truth tables also agree exactly; deliberate extra interaction
`x0 ∧ x1` is invisible to the one-hot TRAIN probes but
**7/7 rejected** by validation including simultaneous selectors.

At k1024, average training cost was **240 logical queries** for
the clean panel, **3600 physical observations** with noisy
repetition, yielding a native four-term sparse WideAnfGate of
**9024 logical payload bytes and 19 words scratch**. Test of the
**actual fitted gate** inserted into a MOR-decision-selected
`WideAnfPageAtlas<1024>` passed 16/16 native SBG output checks.
The page selection in that experiment was EXPLICIT, not learned;
the independent PR #499 native MOR/Delta-KV integration remains
the runtime component. Measured times cover a local synthetic
membership oracle, not a physical production data-collection cost.

**Next work, in separate preregistered PRs:**
- Unknown selector discovery without handing over `x0..x3`;
  quantify identifiability when selectors can interact, and
  negative supports/negations or multiple monomials per selector.
- Passive/online learning and conditional noisy recurrence
  without controllable membership queries; expose cases of
  information-theoretic impossibility rather than claim generality.
- Frozen TRAIN/VAL/TEST fitting plus truly selected SBG/MOR/
  Delta-KV recurrent execution, with causal route selection,
  active gate counts and rollback evidence.
- Sparse learning/selection across a bounded model-owned bank
  and then the target `K=1,048,576` address/function scale;
  distinguish functions stored, functions activated and
  coefficients actually searched or touched.
- Native SIMD/ARM/Jetson Thor physical benchmarks under the
  no-proprietary-NVIDIA-software-beyond-drivers constraint,
  and independent model language-quality evaluation.
A6-WL1 does **not** qualify free Boolean circuits, unknown
five-gate recurrent topologies, SML-25B perplexity or tok/s.

## A6-WL2 selector-free sparse ANF discovery and limitations — 2026-10-12

The [pre-registration](A6-WL2-UNANCHORED-ANF-PREREGISTRATION.md)
and [native results](RESULTS-A6-WL2-UNANCHORED-ANF.md) establish
a first model-owned active learner for **four unordered unknown ANF
monomial supports with NO distinguished selectors**.

Seven bit widths `k∈{8,16,32,64,128,256,1024}` ×3 fixed
teachers per width ×2 TRAIN noise panels produced **42/42 exact
supports**, 42/42 accepted 544-case independent VAL, no bit
differences on 42×4096 TEST predictions, six complete k8
truth tables, 7/7 out-of-prior targeted fifth high-degree AND
detections and 16/16 real learned k1024 SBG selected-page outputs.
Under noisy TRAIN (Bernoulli1/16, 21 repeated votes), k1024
averaged **8218 physical observations per fit** versus 391.3
logical group queries. The support constraints are **four
disjoint positive monomials of degree 2..5**, and the fitter
actively selects query assignments. No PASS generalizes beyond
this restricted identifiable function class.

The method uses successive minimal-positive witnesses of a
residual Boolean oracle that cancels previously found monomials.
Disjoint supports make it possible to distinguish a single
active conjunction from odd overlaps by a ≤5 degree gate.
Without such structure the approach may be wrong; training on
only passive streaming tokens, overlapping monomials or changing
input distributions remains **unproven**. The 7 out-of-grammar
controls are targeted: a fifth **all-input AND** is nearly
invisible to uniform random validation at k1024, but a
predeclared all-one validation probe exposes it, rejecting all
seven fixtures. This is not a complete misspecification
detector.

**Next falsification priorities:**
1. ANF monomials with controlled overlap between support sets
   and explicit observability rather than disjointness.
2. Unknown number of active monomials and bounded model-order
   selection, including negative cases with exactly matching
   random validation loss but different functions.
3. Replace active membership queries with passive or
   intervention-limited recurrent training, separating
   information deficiency from computational search failure.
4. Explicitly use learned monomials inside the full
   selected MOR/SBG/Delta-KV loop for multiple cycles with
   rollback under memory budget and provenance errors, rather
   than only a manually selected one-page replay.
5. Model-owned page banks with incremental storage,
   1,048,576 actual function addresses and bounded active
   coefficients, SIMD/Thor measurements and independent
   SML-25B language-quality benchmarks.
These are **not** established by A6-WL2.

## A6-WL3 — overlapping ANF, unknown term count and Möbius certificates (2026-10-12)

[Preregistered A6-WL3 protocol](A6-WL3-OVERLAP-ORDER-PREREGISTRATION.md)
and [fully auditable result report](RESULTS-A6-WL3-OVERLAP-ORDER.md)
extend the TRAIN-only active Boolean monomial learning programme.
Unlike A6-WL2, the learner does not assume pairwise-disjoint
supports or the EXACT number of hidden monomials, only a cap of
4, positive literals, degrees 2..5 and the right to intervene
with active membership queries.

**Science result on bounded corpus:** 7 k-widths × STAR4
shared-hub, CHAIN3 overlap and NESTED2 monomials ×3 seeds×
clean/noisy (independent Bernoulli1/16 errors, 21-vote majority)
= **126 fitted policies from 63 distinct true teachers**.
Model order and unordered supports are **126/126 exact**,
544 VAL cases per fit have zero mismatches, 126×4096
independently staged TEST output bits give **zero mismatches**,
18 k8 exhaustive truth-table tests pass, seven fifth
degree6 monomial controls are **explicitly rejected at fit
via TooManyTerms** (no misleading validation score), and
16 learned k1024 selected SBG page outputs match their oracle.

**Critical algebraic guard:** locally minimal positive assignments
may combine 3 overlapping XOR monomials. To avoid false term
claims, `anf_wide_overlap_learn` queries all ≤32 subassignments
of each candidate and computes its GF(2) Möbius coefficients.
Unknown order is terminated only after 512 independent random
residual-zero membership checks, including after the fourth
monomial. The false-stop probability ≤`(31/32)^512` requires
a clean uniform independent oracle AND true residual degree≤5;
it is **not** a distribution-free certificate, especially under
remaining noise, passive data or unsupported high-degree terms.
Every oracle vote/query is budgeted and reported; worst observed
physical count was **22,470 per fit**, under the fixed cap
160,000; k1024 STAR4 noisy averaged 20,552 physical reads.
The local Rust fit timings cannot stand in for collection cost.

**Next falsification gates, separately preregistered:**
1. Passive recurrent traces with held-out downstream contexts:
   measure identifiability as a function of context coverage
   and counterexamples that defeat observed-sample fitting.
2. Controlled high-degree and negative-literal interactions;
   quantify where 512 randomized probes cannot detect a rare
   residual and derive a revised conditional stopping contract.
3. Unbounded/unknown number of monomials, overlapping free
   circuits and capacity-matched unknown 4/5 hidden recurrent
   gates; include exhaustive small-k truth controls.
4. Truly learned sparse pages in multi-cycle selected
   MOR/SBG/Delta-KV, including safety budgets, page mutation,
   causal route selection and rollback without teacher forcing.
5. Million-address ANF bank, active-page resource accounting,
   native ARM/NEON/Thor benchmarking and distinct SML-25B
   language/generalization metrics.

Retain Rust-only native execution, no NVIDIA proprietary software
beyond permissible GPU drivers, and no premature scaling claims
from this synthetic degree-bounded active membership setting.

## A6-I1 epistemic ANF intuition — confidence, shift and intervention controls (2026-10-12)

[Prerecorded A6-I1 hypotheses](A6-I1-EPISTEMIC-INTUITION-PREREGISTRATION.md)
and [native quantitative report](RESULTS-A6-I1-EPISTEMIC-INTUITION.md)
turn the informal 'intuition' proposal into an operational,
falsifiable signal: a prior over two ANF functions can improve OOD
behavior even when their current task's passive observations are
**provably indistinguishable**.

The test uses two native `WideAnfGate` hypotheses
`f0=b`, `f1=b XOR x_a x_b`, whose candidates share a randomized
four-term base ANF. On the passive support `x_a x_b=0`
the functions coincide identically; on 16 forced OOD
coactivation queries they differ everywhere. The learner receives
two candidate gates and a context tag, and learns its
`P(f1|tag)` from 64 labeled historical tasks per tag using
Beta(1,1) smoothing, not from truth labels of the current blind
task. Uncertainty is retained as predictive probability plus
explicit consensus-only abstention. A targeted active probe is
measured as a SEPARATE information-acquisition arm.

**Measured 252-cell panel:** 7 k widths, 3 seeds, 2 passive
censorship regimes, clean vs noisy labels and 3 deployment
prior-shift conditions. All passive pair comparisons agree,
86,016 deliberately forced OOD disagreements per regime,
with **128,700 natural+forced OOD disagreements** in the
258,048 OOD query panel per regime. On these ambiguous OOD
cases, the trained contextual prior reaches **87.64% correct
and 0.10970 Brier** under matching history, compared with
~50% and Brier 0.25 for uninformed alternatives.
After **prior reversal**, accuracy collapses to **11.82%**
and Brier worsens to 0.659; with no real prior signal, accuracy
is **49.65%** and Brier 0.385. The mean prior confidence
~86.19% therefore becomes dangerously miscalibrated under
shift. Selective abstention covers ~50.13% OOD and has zero
clean output errors on accepted queries, since the true
teacher is in the two-hypothesis ensemble by construction.
A separate 1/21-query intervention recovers all hidden
teacher IDs in this synthetic panel. These numbers do
NOT demonstrate human intelligence, a general ANF learner,
or robustness when the candidate family omits the teacher.

**Follow-up preregistered evidence gates:**
1. >2 bank hypotheses, sparse/elastic model scores and true
   function sometimes absent from the candidate set; distinguish
   epistemic consensus from dangerous model misspecification.
2. Learn useful priors/analogy context from **past performance
   feedback** or passive recurrent observation, without being
   given historical hidden function identities.
3. Online shift detection and adaptive active query budgets;
   measure Brier, regret, abstention coverage, false certainty,
   cost of intervention and hard distributional adversaries.
4. Integrate this belief with actual multi-cycle SBG/MOR/Delta-KV
   state update and memory provenance without leaking TEST
   truth into router selection or learner.
5. Independently validate vocabulary, natural-language
   generalization and GPU/Thor native inference under
   Rust-only/software-vendor constraints before any
   SML-25B or million-ANF active-bank claim.

The next demonstration must deliberately include observable
failures and uncertainty, not only easy synthetic successes;
identifiability is an information property, not something
computational effort alone can eliminate.

## A6-I2 passive epistemic committee and missing-hypothesis certificate — 2026-10-12

[Pre-registered A6-I2 experiment](A6-I2-FOUR-HYPOTHESIS-PASSIVE-PREREGISTRATION.md),
[append-only fair baseline amendment](A6-I2-FAIR-BASELINE-AMENDMENT.md)
and [complete native evidence report](RESULTS-A6-I2-FOUR-HYPOTHESIS-PASSIVE.md)
expand the epistemic A6-I1 pair into **four native sparse ANF
hypotheses** with passive Markov-correlated observations, varying
visibility, OOD context shifts and deliberate true-teacher
absence.

**Measured scientific milestone:** 7 widths k8..1024 ×3 seeds
×4 balanced teachers×3 passive Markov visibility regimes×2
label panels = **504 frozen committees** on shared inputs,
with **21 independent-width/seed OOD input streams** and
1,032,192 repeated native TEST prediction comparisons. Their
full passive prediction signatures certify 1, 2 and 4
observational equivalence classes respectively. 168/168
FULL teacher posteriors ≥0.99. Selective confidence ≥0.90
covers **56.19% BLIND, 74.94% PARTIAL, 100% FULL** with zero
error on selected clean test inputs under **true teacher in
the supplied bank**. Forced MAP nonetheless makes 75,360
BLIND errors and 43,108 PARTIAL errors. The supplied
simplicity prior [4,2,2,1]/9 is worse in Brier than an
**UPDATED uniform Bayesian comparator** when balanced
teachers hide at least one dimension; original static
comparator results are explicitly retained as a different
baseline, not falsely equated with updating.

**Essential negative, not to suppress:** 21 out-of-bank
teachers with visible x[r6]x[r7] signals trigger the
predeclared 1/8 training mismatch guard; 21 more with the
same missing term but TRAIN x[r6]=0 are **undetectable from
that support** and pass the guard. OOD release exposes
**5,981 high-confidence errors out of 24,168 accepted
predictions** in the censored condition. A 100%-unanimous
bank cannot certify correctness of unmodeled functions.
No distribution-free certainty, consciousness or universally
calibrated confidence is claimed.

**Next independently falsifiable PRs:**
1. Candidate-bank **expansion from bounded active interventions**
   with explicit cost in new oracle labels, source provenance
   and direct comparison against passive-only abstention.
2. **Unknown hypothesis** state / model inadequacy alarm that
   distinguishes distribution shift in the input support,
   epistemic uncertainty across known candidates and latent
   misspecification; document cases impossible to detect
   from passive labels alone.
3. Real recurrent **SBG/MOR/Delta-KV** temporal execution
   with learned wide sparse gates and dynamically ranked
   page candidates; tests for missing pages, memory
   rollback and observational equivalence.
4. Controlled high-degree, >4-term and incomplete candidate
   registries with long-range history and heavy-tail
   rare-context coverage; calibrated posterior and
   selective-risk intervals on independent teacher populations.
5. Dynamic K=1,048,576 ANF function-bank storage, physical
   page activation, SIMD/ARM/Thor native measurements and
   independent SML-25B language benchmarks.

Keep hypothesis **selection** distinct from discovery of
ANF monomial structure, passively observed Markov input streams
distinct from model-owned recurrent state, and native CPU
measurements distinct from GPU/token quality.

## A6-I3 — empirical support envelopes, pairwise blind spots and honest abstention (2026-10-12)

[Pre-registered support protocol](A6-I3-EPISTEMIC-SUPPORT-PREREGISTRATION.md)
and [complete negative/positive report](RESULTS-A6-I3-EPISTEMIC-SUPPORT.md)
add `anf_marginal_support::AnfMarginalSupportGuard` to the native
four-candidate `PassiveCommittee` from merged A6-I2. The API
tracks only whether every input dimension's **current 0/1 value
has been observed** in the 512 passive TRAIN steps. No hidden
candidate identity or label enters the guard. Combined selective
forecast requires 0.90 confidence, bank TRAIN mismatch <=1/8,
and zero marginal novelty. All invalid widths, empty TRAIN,
out-of-sync committee histories and exhausted budgets fail closed.

The preregistered panel covers 7 k widths×3 seeds×2 noise panels
×5 regimes = **210 frozen policies, 430,080 later TEST inputs**.
Outcomes for each 42-fit regime (86,016 TEST outputs):
- Missing teacher with r6 censored in TRAIN: 43,008 wrong
  confident bare-committee forecasts reduced to **zero wrong
  guarded accepted forecasts**, at **50% coverage**.
- True teacher actually IN bank, same censoring: **43,008
  unnecessary abstentions**; the base model had zero error.
- Missing teacher with r6/r7 observed separately but never
  jointly 1: **43,008 high-confidence guarded errors remain**
  despite 100% coverage, because a univariate support alarm
  cannot see the unseen interaction.
- Full coverage with true teacher included: zero guarded errors
  at 100% coverage.
- Fully visible missing teacher: the native TRAIN mismatch
  alarm rejects all 42 inadequate banks, despite no novel
  marginal input values.

**Critical interpretation:** empirical novelty alarms lower risk
on explicit distribution shifts while also losing useful coverage,
and DO NOT bound the risk of hidden high-order interactions.
The 43,008 pair-censored failures are required negative evidence,
not a test failure to hide or threshold-tune away. No model
proves an unseen function is impossible absent interventions
or a sound hypothesis-class completeness assumption.

**Next separately preregistered research gates:**
1. Explicit learned/budgeted **pairwise coactivation** support
   coverage (O(k²) naïve) with sparsified sketches and memory
   footprint on k1024/large K, while including an analogous
   **three-way invisible interaction** negative control.
2. Adaptive information acquisition: active queries selected
   only when uncertainty or support novelty warrants, reporting
   query cost versus extrapolation risk/abstention utility.
3. Passive recurrent SBG/MOR/Delta-KV trajectories and
   hidden teacher functions outside candidate banks, including
   graceful abstention and detection of distribution drift.
4. Incremental candidate-bank hypothesis generation and
   true model-order search beyond the small fixed four-way
   supplied ANF library, with exact source-aware signatures.
5. Function-bank scaling toward K=1,048,576, memory/active
   coefficient budgets, and independently measured linguistic
   token prediction and native Thor performance.

Keep strict Rust, no NVIDIA vendor software beyond allowed
drivers, no private runner execution and no broad claims from
small synthetic ANF support evidence.

## A6-I4 — pairwise ANF coverage and mandatory third-order failure (2026-10-12)

[Preregistered experiment](A6-I4-PAIR-SUPPORT-TRIPLE-PREREGISTRATION.md)
and [complete native report](RESULTS-A6-I4-PAIR-SUPPORT-TRIPLE.md)
test whether tracking the joint input-value support of each
PAIR can reduce the confident OOD mistakes demonstrated by
A6-I3's marginal-only alarm. The new safe Rust `anf_pair_support`
uses 512 bit-sliced TRAIN observations per input coordinate,
computes pairwise 4-value masks once and retains only sparsely
MISSING joint patterns (explicit max width1024,
max 600000 pairs and 8192 missing pairs budget).
`PassiveCommittee` remains a supplied four-function candidate
bank, not a learned complete model.

On the predeclared **252 fits / 516096 OOD predictions**
covering six regimes, the pair guard prevents exactly **43008**
confident errors on a previously unseen pair11; the univariate
guard accepts these 43008 wrong outputs. When the teacher is
IN the bank, pair-only conservatism incorrectly abstains from
**43008 otherwise correct predictions**, proving a real
coverage/selective-risk trade-off. In a constructed triple
teacher absent from bank, each of the three PAIRS has full
TRAIN coverage but triple111 never co-activates. The new guard
fails **mandatorily** with **43008 confident errors out of
86016 accepted inputs** and ZERO unseen pairs. Thus observing
all r-way configurations does not identify r+1-way
interactions. Full-coverage true-teacher controls accept
86016/86016 with zero errors; observable missing-teacher
cases are rejected by the fixed 1/8 TRAIN bank-mismatch alarm.

**Next falsification sequence:**
1. Third-order presence and conditional masks with strictly
   bounded resident/traced bytes; k1024 naive k³ coverage
   is unacceptable without sparse conditioning or adaptive
   selection. Keep a fourth-order invisible interaction
   as a compulsory counterexample, no hidden oracle leak.
2. Distinguish support novelty, candidate-bank inadequacy,
   conflicting hypotheses and task-transfer prior shift;
   evaluate Brier, calibration, coverage and costs on
   balanced teachers and OOD regimes.
3. Permit TRAIN-only discovery of previously absent low-degree
   ANF monomials *after* the guard signals novelty, then
   add them as native WideSparseANF candidate pages with
   rollback/capacity evidence, not from TEST labels.
4. Test passive SBG/MOR/Delta-KV autonomous multi-cycle
   trajectories with state corruption, limited interventions
   and unseen temporal interaction regimes.
5. Verify million-function bank physical accounting and
   native SIMD/Thor performance separately from language
   SML-25B quality/perplexity/tokens-per-second.

A6-I4 remains a synthetic **known-bank OOD guard**, NOT
an ANF learner or a human-intuition proof. Use native
Rust only, without proprietary NVIDIA software beyond
permitted drivers.

## A6-I5 — higher-order epistemic coverage and falsification (2026-10-12)

The [A6-I5 preregistration](A6-I5-TRIPLE-COVERAGE-PREREGISTRATION.md) and [native results](RESULTS-A6-I5-TRIPLE-COVERAGE.md) qualify an explicit, budgeted third-order input-support guard alongside the real supplied-candidate ANF committee. On **90 native policies** (512 TRAIN each) and **46,080 held-out decisions**, 7,680 triple-trigger errors from A6-I4 are blocked with zero errors on 7,680 retained predictions, but 7,680 **correct** predictions are lost in the included-teacher control. The mandatory fourth-order hidden teacher remains confidently wrong on 7,680/15,360 accepted predictions with **no novelty alarm** even though every triple assignment was seen. All conclusions are restricted to controlled eight-role synthetic teachers embedded in widths 8..128.

Next scientific gates, without promoting A6-I5 to model-quality evidence:
1. **Adaptive and sparse higher-order observation design:** select interaction subsets using TRAIN-only information, control multiple-testing/selection leakage, and measure OOD missed interactions versus RAM and compute. Never call a partial subset full-width certification.
2. **Active intervention and bank repair:** issue separately budgeted distinguishing queries when legitimate observation access exists, learn new ANF terms into a candidate bank without hidden support labels, track attempts and corrections against no-intervention controls.
3. **Native coupled-model evidence:** carry uncertainty and abstention signals through actual MOR-selected SBG/ANF/Delta-KV cycles and test recurrence drift, correlated observations and resource rollback, without replacing existing runtime policies by default.
4. **Independent replication and scaling:** rerun on new teacher populations/streams, full resource accounting on target device, predeclared baselines and mandatory higher-order counterexamples before considering any SML-1/SML-2 model-level claims.

The k128 exact support scan covers 341,376 triples. With the frozen budget 350,000, k256 and k1024 full-width scans are refused before allocation. No model 25B parameter training, one-million-function bank, or Thor GPU speed evidence follows.

## A6-I6 limited fourth-order interaction memory and mandatory blind-spot falsification — 2026-10-12

The [preregistered A6-I6 protocol](A6-I6-BOUNDED-FOURTH-SUPPORT-PREREGISTRATION.md)
and [measured positive/negative report](RESULTS-A6-I6-BOUNDED-FOURTH-SUPPORT.md)
extend epistemic uncertainty controls from marginal, pair and
global triple coverage to **budgeted externally registered
fourth-order context monitoring** in the existing native
`PassiveCommittee` runtime. Input-only support masks store
16 observed bit patterns per watched quadruple (`u16`)
and support width k1024 without an O(k^4) enumeration.

Evidence: 168 frozen policies, 86,016 OOD predictions, four
matched scenarios. With a relevant public watched interaction,
10,752 missing-bank errors were prevented at 50% coverage;
with a bank-complete true teacher, 10,752 entirely correct
predictions were unnecessarily suppressed. **Two mandatory
negatives remain severe:** an unmonitored fourth-order or
fifth-order missing teacher term yielded **10,752 confident
errors among 21,504 accepted predictions** per regime.
Input novelty of a partial watchlist can never certify
completion of a candidate function bank; unknown support
outside the watchlist may be indistinguishable on TRAIN.

The exact logical guarded-input retention for one tuple is
40 bytes on the hosted x86_64 ABI, excluding host allocations
and the *much larger* supplied candidate committee. This
is NOT a native 1,048,576-function bank, an attention replacement
performance benchmark, nor native Thor GPU evidence.

**Research next steps, each separately preregistered:**
1. A6-I7 active question selection with bounded membership
   access/delayed feedback, including a control where permitted
   queries **cannot** reveal the hidden fourth-order interaction.
2. Causal source localization of unknown ANF terms **without**
   handing over watch coordinates, and explicit memory/latency
   tradeoffs for k1024 under a limited number of probes.
3. Calibration and shift detection when historical contextual
   priors are wrong, and missing true teacher cannot be caught
   from passive evidence.
4. Convert accepted/abstained predictions into multi-cycle
   SBG/MOR/Delta-KV actions with observed rollback, provenance
   and resource counters, independent of language-quality claims.
5. Truly learned variable-width page banks, adversarial
   large-degree interactions, million-address function storage
   and physical ARM/NEON/Thor tests under Rust/native policies.

## A6-E1 — model-level relevance before another order-of-support escalation (2026-10-12)

**Verified negative STOP**, not a production model promotion. See [the preregistered real-byte protocol](A6-E1-REAL-BYTE-SUPPORT-RISK-PREREGISTRATION.md) and [the measured positive/negative native report](RESULTS-A6-E1-REAL-BYTE-SUPPORT-RISK.md). A6-E1 uses a real SHA256-pinned public-domain French text corpus, four **TRAIN-learned** native ANF experts with different candidate-specific input supports, full-16-bit TRAIN-nearest Hamming support, conditional degree-1/2 GF(2) design-rank diagnostics, 32 VAL-chosen selective-risk policies, and explicit ngram4 fallback on every abstention. A separate mandatory counterexample proves that full pairwise covering-array patterns do **not** imply identification even for a degree-one parity teacher.

Historical A5 **full-next-byte** score: Witten–Bell context4 **2.019 bits/byte**, best ANF **2.474 bits/byte**. This A6-E1 **binary-next-space** test is a distinct task and must not be numerically compared to A5: on 43,136 binary TEST events, ngram4 **0.21751656 bits/event** decisively beat the committee **2.52933653** and the selected ANF+fallback policy **2.52734408** (99.895679% coverage). Increasing gate count or raising guard degree cannot be presented as progress while the underlying hard Boolean committee is grossly miscalibrated on real data.

**Immediate model-oriented follow-up, ordered by evidence:**
1. Redesign a **calibrated, normalized learned ANF probability model** (TRAIN count estimates, sparse backoff and a selectable nonconstant feature learning signal). Preserve existing ngrams and ngram4, plus a majority baseline, on the *same* frozen real-data splits; compare all-token loss, not accuracy only where predicting.
2. Evaluate risk-versus-coverage as a diagnostic, with candidate-support-scoped interactions, full-input novelty and a known-grammar degree hypothesis **explicitly conditional** on realizability and GF(2) full-rank evidence. No inferred teacher-support injection, TEST threshold tuning or implicit guarantee from covering arrays.
3. **End-to-end gate:** first beat a predeclared competent baseline with held-out actual log-loss and matched resource accounting; expand to a full **256-byte** predictive task and an independent corpus before any SML-2 consideration. A previous auxiliary binary subtask success alone would not suffice.
4. Maintain two outcomes: scientifically valid **STOP** when the model loses (as it does here) and promotion only if all preregistered criteria hold. Do not change experiment thresholds based on TEST, hide zero-monomial learned gates or characterize failure as confidence protection success.
5. Once model-level quality is demonstrated, compare resource-bounded flat/Pascal SIMD/WGPU kernels from SciRust, FLAT-ATTENTION and NNIS on the actual model workload, preserving SML's hardware-sovereignty and self-sufficient model/harness separation.

The existing A6-I3/I4/I5 and A6-I6 (#508) guards are useful bounded negative controls, not a logical progression toward 25B without a generalization-positive model.

## A6-C12g / A6-C12h — transitions spectrales mesurées, code fusionné et domaine de validité (2026-10-12)

**STATUT QUALIFIÉ — synthétique seulement.** Les PRs #510 et #511 sont fusionnées sur `main`. Les CI GitHub-hosted ont finalement été **vertes**, contrairement aux premiers échecs `steps=[]`. Voir [A6-C12g run 38292779849](https://github.com/Memorithm/SML-GENIUS/actions/runs/38292779849), [A6-C12h run 38292873509](https://github.com/Memorithm/SML-GENIUS/actions/runs/38292873509) et le [rapport chiffré réconcilié](RESULTS-A6-C12GH-NOISE-PHASE-QUALIFIED.md). Les six ZIP complets (SHA256 consignés) valident 1232+1872 FROZEN et autant de TEST au code exact, trois réplicas par protocole ; l'audit recompute les résultats à partir de chaque TEST et compare les invariants. L'ensemble statistique est de 8 ou 16 graines par cellule, **pas** 24 ou 48.

Le seuil empirique Walsh-3 (7/8 ou 14/16 succès exacts sur TEST autonome) est de **256/2048/16384 bits** sur XOR pour η=20/30/40 %, tandis que l'hybride pairwise échoue à atteindre ce seuil même à 32768 bits dans les trois cellules. Sur AND→XOR→XOR η=30 %, l'hybride atteint **4096 bits** contre **8192** pour Walsh-3 ; sur AND η=40 %, **aucune méthode** n'atteint le critère jusqu'à 32768 bits. Les paliers, covariances entre budgets, Wilson exploratoires et erreurs de modèle restent visibles dans le CSV [334 lignes](../research/results/A6-C12GH-PHASE-SUMMARY.csv) ; une courbe théorique heuristique n'est **pas une borne**.

**Suite technique priorisée :**
1. Ne pas refaire ni retoucher C12g/h. Préenregistrer une étude distincte **sélecteur TRAIN/VAL** W3/HYBRID, à budget de calcul apparié et avec négatifs obligatoires (pur XOR sous bruit, AND hors budget, nouveaux enseignants non représentables, bruit d'état). Ne pas lire TEST pour sélectionner le chemin.
2. Ajouter réplication avec **nouvelles graines et variantes de tâches** (non seulement des répétitions sur d'autres runners), démontrer les intervalles et vrais coûts mémoire/temps par échantillon et par interaction ; comparer la vraie structure aux faux seulement rétrospectivement.
3. Revenir au critère bloquant de qualité naturelle **A6-E1 STOP** : apprendre un lecteur ANF probabiliste calibré, comparer sur le même corpus à ngram4 avec décision stricte sur la log-loss et évaluation 256 symboles. Les prototypes synthétiques ne sont pas une preuve de qualité SML-2/25B.
4. Après seulement un gain réel de qualité, instrumenter les variantes Pascal / registre SIMD / SBG-MOR-Delta-KV sur workloads natives et matériel ciblé, sans technologie propriétaire NVIDIA hors drivers et sans extrapoler à un accélérateur non mesuré.

## A6-E2 — calibration ANF real-data mesurée, valeur modèle toujours négative (2026-10-12)

**Résultat qualifié = STOP.** Voir [préenregistrement](A6-E2-KT-CALIBRATED-ANF-PREREGISTRATION.md), [rapport chiffré et traces](RESULTS-A6-E2-KT-CALIBRATED-ANF-STOP.md), [workflow hosted 38297230454](https://github.com/Memorithm/SML-GENIUS/actions/runs/38297230454). 346240 TRAIN, 43136 VAL, 43136 TEST (événements binaires *next-space*); 4 supports appris k2..k5 ; 16 lectures probabilistes KT et 96 mélanges sélectionnés par VAL avant accès TEST. Deux réplicas complets identiques hors temps, SHA256 corpus figé.

Sur TEST, meilleur KT-ANF **0.45528156 bits/événement** contre ngram4 **0.21751656** ; le mélange VAL retient **λ=0**, donc ngram4 seul et zéro gain. Ancien comité dur E1 **2.52933653** ; la calibration soulage la surconfiance mais **ne rend pas ce modèle compétitif**. Les 3 premiers supports k2/3/4 produisent zéro monôme majoritaire actif et le k5 16, contrôle négatif conservé.

**Étapes suivantes, protocoles distincts avant TEST :** (1) tester le gain de longueur de contexte et l'ordre des interactions **sans** injecter les variables cachées, à temps/mémoire et nombre de paramètres équivalents ; (2) ajouter un second corpus indépendant, ablation de répétition des séquences et fuite éventuelle, puis la tâche multi-octets à 256 symboles avec pertes par octet ; (3) comparer les fonctions Pascal/ANF entraînées à modèles statistiques compétents et non seulement à une majorité ; (4) réserver l'accélération NNIS/SciRust/FLAT-ATTENTION à un vrai modèle apportant un gain mesuré. Ne pas retraiter A6-E2 selon ses propres TEST, ni présenter un échec de modèle comme un succès SML-25B.

## A6-E3 — test full-byte 256 symboles : gain du mélange RAW mais échec de la valeur ajoutée ANF (2026-10-12)

[Préenregistrement](A6-E3-FULL-BYTE-BOOLEAN-BANK-PREREGISTRATION.md) avant code, [rapport scientifique E3](RESULTS-A6-E3-FULL-BYTE-BOOLEAN-BANK-STOP.md) et [CI native 38300703335](https://github.com/Memorithm/SML-GENIUS/actions/runs/38300703335) avec deux réplicas complets. Sur 43008 octets TEST, ngram4 **2.01532016 bits/octet**, banque booléenne pure **3.33509577**, mélange ngram4+RAW **1.99022046**, mélange ngram4+ANF **1.99022046**. Le gain **0.02509970** du mélange sur le ngramme n'est **pas dû aux nouvelles portes ANF composées** : le sélecteur glouton a choisi les **mêmes huit bits bruts** dans les deux bras, et les critères de qualité préenregistrés donnent `STOP`. Une amélioration brute de log-loss sur un corpus n'implique ni validité sur un autre texte ni intelligence générale.

**Recherche suivante avec préenregistrement distinct, sans tuning sur TEST E3 :**
1. **Effet de sélection conjointe (non-greedy) :** à budget k4/k6/k8 et coûts appariés, tester des blocs de portes XOR/AND/triples choisis collectivement, car un bénéfice purement interactionnel peut être invisible aux critères marginaux; conserver RAW32, ANF128 et ngram4 comme contrôles.
2. **Incertitude/statistique :** faire des comparaisons de perte appariées par voie/document et de vrais corpus indépendants ; le corpus Verne seul et les mêmes réplicas ne permettent pas de conclure à une supériorité universelle.
3. **Qualité avant accélération :** ne déployer ni banque 1 048 576 portes ni path GPU/Thor sur seule évidence synthétique. Utiliser SciRust/FLAT-ATTENTION/NNIS lorsqu'une représentation apporte un gain prédictif distinct mesuré, puis faire des benchmarks natifs avec comptabilité mémoire/énergie.
4. **Gouvernance :** figer les nouveaux critères avant code, conserver les échecs sur TEST et les faits négatifs d'A6-E1/E2/E3 ; ne pas assimiler une fonction publique vérifiée GF(2) à une nouvelle fonction réellement apprise.

## A6-E5 — séparer architecture algébrique vérifiée de l'expérience linguistique finale

[Corpus Flaubert #14155 figé avant code](A6-E5-FLAUBERT-CORPUS-FROZEN.md), [protocole et seuils préenregistrés](A6-E5-NUMERIC-FROZEN-BEFORE-CODE.md), [banc natif algébrique/MDL (synthétique)](A6-E5-SPECTRAL-BEAM-KERNEL-EVIDENCE.md). Le nouveau noyau opt-in calcule le **résidu négatif du gradient logit**, propose les parités à partir d'un spectre de Walsh restreint, synthétise les conjonctions via Möbius GF(2) et conserve des hypothèses non conclusives dans le Beam pour ne pas rater des interactions. Les contrôles demandent `TRAIN_fit` et `TRAIN_inner_VAL` distincts : ne pas interpréter l'API comme preuve automatique de non-fuite ou de qualité hors distribution.

**À construire avant TEST sur Flaubert :** (1) fit et comparaison des banques complètes à k8, RAW greedy/RAW Beam/ANF greedy/ANF-joint #517/Beam spectral ; (2) adapter le multinomial BACKOFF de #518 au Dirichlet hiérarchique sans le dupliquer ; (3) entraîner l'arbitre contextuel sur **prédictions OOF** avec log-loss (pas ECE), mesurer ECE et Brier après fit ; (4) afficher la qualité à budget RAM/CPU équivalent ; (5) figer tous les choix et lancer **une seule** campagne confirmatoire sur TEST Flaubert, avec publication obligatoire des STOP. La représentation passe seulement si des **portes composées effectivement sélectionnées** apportent >0.01 bit/octet vs RAW équivalent ; modèle >0.02 vs NG4 avec comparateurs fixés. Les accélérations ATLAS/Pascal doivent être profilées après preuve de qualité, pas assumées.
