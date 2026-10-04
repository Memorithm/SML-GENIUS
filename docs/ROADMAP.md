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
3. Advance SML-D2/D3/D4: proper-scoring calibration on reserved data, SBG/MOR coarse-to-fine candidate shortlisting, hot expert residency/prefetch, and DIRECT/REFINE/ABSTAIN gating. Include the SML-CLM-0/1 preregistration and bounded score/label foundation below as the contrastive research input; calibration and action authorization remain separate gates.
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
