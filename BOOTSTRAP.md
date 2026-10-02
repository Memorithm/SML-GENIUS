# SML-GENIUS bootstrap

This file is the durable re-entry point for continuing the SML-GENIUS research programme from the current repository state. Read it together with `README.md`, `docs/ROADMAP.md`, the latest merged pull requests, and the executable contracts in `sml-core`.

## Current architectural direction

SML is an independent, predominantly Boolean / low-bit autoregressive architecture. It is not defined by reference to Transformer internals. Its native mechanisms include direct routing, Mixture-of-Routers / sparse expert selection, bounded associative memory, recurrent depth, Elastic Weights, and Boolean-integral learning experiments.

Native decision capability is a first-class parallel objective. A substantial decision-expert family and dedicated training objectives must be evaluated before SML-2, not bolted onto a finished language model. Direct decisions and bounded recurrent refinement must work without a text decoder; language generation is optional. See `docs/SML-D-BOOTSTRAP.md`. Total decision capacity, active work per decision and direct-decision coverage are separate quantities; no arbitrary 25B percentage is frozen.

The **SML-CLM** research track adds contrastive state-question-decision learning and SML-specific scaling experiments to this native decision programme. It must compare the current task objective, an explicitly separated InfoNCE research reference and a native integer-margin alternative; it does not import CLM's encoder, scaling coefficients or runtime. The durable contract is [below](#sml-clm--contrastive-decisions-and-measured-scaling); implementation gates are in [the roadmap](docs/ROADMAP.md#sml-clm--contrastive-decisions-and-measured-scaling). This is a research plan, not an implemented learner or a model-quality result.

Long context is also a first-class architecture programme. SML must not equate context capacity with a single dense token buffer. The target is a bounded active semantic/recurrent working set plus immutable raw context segments, associative memory and mapped SBG/raw indexes with explicit provenance. See `docs/SML-CONTEXT-BOOTSTRAP.md`. Raw tokens ingested, effective retrievable span, resident RAM, mapped/disk bytes and retrieval traffic are separate measurements.

Recursive self-improvement is a separate human-gated programme. The current champion may propose training data, curricula, code, architecture/runtime changes, evaluation evidence and GitHub pull requests, but it may never silently promote itself. For RSI work, `main` is the approved champion and the repository owner's explicit GitHub merge decision is the promotion event. `Memorithm/RSI` is an internal source of inspiration for AUTOPILOT, DGM, cumulative archives, flywheels, candidate-state, evaluation-pipeline and loop-engineering ideas; SML-RSI must re-implement any adopted mechanism natively and must have no dependency on that repository. See `docs/SML-RSI-BOOTSTRAP.md`.

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

## Delta-KV rewritable associative-memory programme

SML now has a separate **Delta-KV** research track for bounded rewritable associative
memory. It complements, rather than replaces, the existing directly addressed
`BoundedAssociativeMemory`, mapped SBG/raw stores and recurrent workspace.

The durable protocol is [`docs/SML-DELTA-KV-BOOTSTRAP.md`](docs/SML-DELTA-KV-BOOTSTRAP.md).

The first slice (**DKV-0a**) is a deterministic dependency-free Q12 reference matrix
using a normalized delta correction. It reports exact semantic bits, operation
counts and saturation, and preserves overlapping-key interference as evidence.
No Transformer-semantic KV cache, attention dependency or performance claim is
introduced.

Current progression is DKV-0a fixed-point semantics -> DKV-0b frozen interference
panel -> DKV-1 matched memory controls -> DKV-2a SBG/MOR key projection -> DKV-2b
frozen-key recurrent memory integration -> DKV-2c dynamic re-keying qualification ->
DKV-3 context evaluation -> DKV-4 adaptive-compute handoff. Forge may propose
candidates and ElasticXxx may enforce resource budgets, but SML retains memory
semantics and independent requalification.

Current Delta-KV evidence: DKV-0a through DKV-2b are merged. DKV-2b merged through PR #83 as `6a26a65d0c953a7e83aedc9f6e785a7df8b88ea8`, providing a bounded Delta-KV read as one native SBG recurrent input with adaptive/fixed/no-Delta-KV controls. DKV-2c is the next active research slice; it must isolate dynamic re-keying from routing changes.

DKV-2c observation: dynamic SBG re-keying is executable but is not promoted into DKV-3. The preregistered missing-successor control remained confidently non-ambiguous, and dynamic re-keying did not improve the final target state over the DKV-2b frozen-key arm on the frozen fixtures. DKV-3 must use DKV-2b until a separate miss/confidence mechanism qualifies dynamic re-keying. See `docs/SML-DKV-2C-OBSERVATION.md`.

DKV-3a context integration remains governed by `docs/SML-DKV-3A-CONTEXT-PREREGISTRATION.md`. The two supplied DKV-3a-0 patches are consolidated in the bounded source-evidence adapter `sml_store::delta_kv_context`; see `docs/SML-DKV-3A-0-HARDENING.md`. Software conformance is not context promotion. The projected-key SBG recurrence and MOR preregistration/integration remain a separate, deferred owner-supplied tranche. Raw-source authority stays outside the Q12 matrix.

## Versioning reconciliation

The repository already uses `SML-0.7` for Elastic Weights and has merged later `SML-0.8.x` MOR / Boolean-integral work. The earlier conversational SBG roadmap used overlapping SML-0.x labels.

Do **not** reuse those occupied SML labels.

Implementation uses the independent **SBG-0.x** slice namespace while preserving the same roadmap content. Native decision work uses **SML-D**; hierarchical context uses **SML-CONTEXT**; recursive self-improvement uses **SML-RSI**; connectomic structural priors use **CSP**.

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

SBG-0.13a byte format merged through PR #57 as `786c0d549c78aae550bff14157d95ed472af2e8d`. SBG-0.13b Linux mapped reader merged through PR #60 as `d2ba5c9999379e5a8bd75838f98a8f7823ec2f89`; its qualification and limits are documented in `docs/SBG-0.13b-MAPPED-RUNTIME.md`.

Native decision work has advanced through:

- D0/D1a merged PR #61;
- D1b observed-SBG bridge merged PR #62;
- D1c bounded learned SBG feature-pair selection merged PR #63;
- D1d MOR-gated sparse decision expert bank with matched dense work-count control merged PR #64;
- D1e MOR-selected Elastic Weight PLAN/VALIDATE admission merged PR #65 as `ef4238fd877554f6671a57140e354a12248d9afb`.

D1f-a verified disk-to-RAM materialization merged through PR #66 as `f30caf193ab575b7700a36d692d3f0081f61881c`. It remains deliberately fail-closed for VRAM and precision conversion until real qualified backends exist.

SML-CONTEXT bootstrap merged through PR #67 as `91d05157d1f9c203a15aa163b7138c1d32440b1b`. Durable contract: `docs/SML-CONTEXT-BOOTSTRAP.md`.

Active parallel programme bootstrap: **SML-RSI**, branch `research/sml-rsi-0-bootstrap`, durable contract in `docs/SML-RSI-BOOTSTRAP.md`. It is inspired by mature `Memorithm/RSI` loop-engineering ideas but is implemented independently for SBG/MOR/decision/context/runtime candidate promotion, with no build/runtime/orchestration dependency and with the human GitHub merge gate preserved.

### Decision-engineering lessons retained from Laya review

The 2026-09-23 source review of `NandhaKishorM/laya` is used as external research evidence, not as a runtime dependency or architecture template. Laya itself uses bidirectional encoder models and floating-point/PyTorch components that are not imported into SML. The transferable engineering ideas are:

- typed `choice / ordinal-score / Boolean` decisions without text decoding;
- many questions sharing one encoded state/pass;
- strictly proper scoring objectives for decision distributions;
- a separate ACT/refine signal using uncertainty/margin-style evidence;
- capability routing before inference instead of trusting confidence from an incapable expert;
- calibration stratified by question type/cardinality;
- coarse-to-fine shortlisting for large candidate spaces;
- hot/preloaded checkpoint residency and explicit cold/warm routing costs;
- option-order/permutation robustness as a decision-quality metric.

SML-native implementations should use SBG/MOR/Boolean/low-bit/fixed-point mechanisms where evidence supports them. No Laya benchmark number is transferred as an SML performance claim.

CONTEXT-0 is merged through PR #89. CONTEXT-1a raw persistence is the next active context slice: immutable segment artifacts with exact SHA-256 source identity, while semantic indexes remain separate derived data. DKV-3a remains blocked until CONTEXT-1b bounded streaming ingest is also merged. CONTEXT-1b is the stacked bounded-ingest slice; it retains only segment-local working payload and explicit cumulative counters, not the complete stream in RAM.

### Context-window architectural direction

SML context is treated as a **hierarchical context horizon**, not merely a maximum prompt length. Two representations are mandatory:

1. immutable raw segments for exact copying, identifiers, numbers, code and source fidelity;
2. SBG semantic records/indexes linked back to raw segment provenance for sparse retrieval/routing.

The active working set must stay bounded while older context may live in associative memory and mapped stores. The context programme must separately account raw bytes/tokens ingested, recent resident raw span, active SBG bits, associative entries, mapped records, raw-segment storage, page faults/traffic, selected context pages, recurrent work and latency.

The initial evaluation ladder is experimental, not a promise:

```text
4k -> 16k -> 64k -> 256k -> 1M tokenizer-token dependency spans
```

Advance only when the preceding span passes quality, memory, traffic and latency gates. Writing a million tokens to disk is not evidence of a million-token effective context.

Implementation surfaces now include:

```text
crates/sml-decision/
crates/sml-runtime/
crates/sml-store/
crates/sml-core/src/semantic/
docs/SML-D-BOOTSTRAP.md
docs/SML-CONTEXT-BOOTSTRAP.md
docs/SML-RSI-BOOTSTRAP.md
```

The workspace retains `forbid(unsafe_code)` and the core retains no external dependencies. OS mapping uses pinned mmap-guard, which encapsulates unsafe code in its dependencies. This is not a safety proof against arbitrary external mutation: mapped inodes must never be modified/truncated while in use. Locks are cooperative; CRC32C is not authentication. Reader counters are logical payload/mapping counters, not RSS, page-cache or actual I/O measurements.

Next implementation gates:

- **SML-CLM-0/1**: freeze the contrastive decision protocol and implement independently checked, bounded native scoring before training comparisons. This feeds SML-D/SBG without replacing their gates; see the SML-CLM contract below.
- **RSI-0a/0b**: versioned candidate-PR evidence manifest and promotion-eligibility verifier. The verifier may say "eligible for human review" but cannot merge or mark itself promoted.
- **RSI-1**: model-proposed exact-verifier curriculum compared against a fixed human-authored curriculum at matched compute, using independently implemented DGM/AUTOPILOT/flywheel-style principles inspired by prior RSI work.
- **DKV-2c**: preregister and qualify dynamic re-projection of the Delta-KV query key from evolving SBG state against the merged DKV-2b frozen-key, matched fixed-depth and no-Delta-KV controls. MOR selection remains frozen in this slice so re-keying is the only changed mechanism.
- **CONTEXT-0/1**: exact raw/token/span accounting, immutable raw segment ledger, provenance-linked SBG index and bounded streaming ingest.
- **SML-D2**: proper-scoring/calibration work on reserved data, including Brier/RPS-style fixed-point candidates, risk/coverage and false-acceptance metrics.
- **SML-D3**: large-candidate SBG/MOR shortlisting, hot expert residency/prefetch and complete end-to-end throughput accounting.
- **SML-D4**: DIRECT / REFINE / ABSTAIN gate using decision margin, SBG ambiguity, MOR disagreement, memory evidence and recurrent stability.
- **CONTEXT-2..7**: recurrent working context, hierarchical retrieval, retention/compaction, long-range refinement, long-range learning and evidence-gated scale ladder. See `docs/SML-CONTEXT-BOOTSTRAP.md`.
- **SBG-0.13c**: cold/warm latency, RSS/page faults and traffic on growing stores; streamed publication and stronger immutable/index-integrity contracts.
- **SBG-0.14**: connect mapped lookup to native SBG/MOR/Elastic/expert/memory/recurrent execution, native decisions and optional generation with common evidence.
- **SML-1 follow-up**: learned marker/control, wider matched ablations, fixed-versus-adaptive depth. Freeze SML-0/SML-1 protocols before SML-2.
- **Elastic actuation**: after RAM qualification, add only genuine physical backends for further residency tiers/precision conversions; other Memorithm repositories remain research/qualification partners, not mandatory inference dependencies.

## Scientific boundary

SBG is currently an architectural hypothesis plus executable reference semantics. No claim is made yet that it matches or exceeds continuous language embeddings, that 16,384 bits is optimal, that Boolean-only training scales to 25B, or that SBG improves language-model quality. Those are experimental questions gated by the roadmap above. SML-D adds bounded native decision machinery but has not yet established Jev/Laya-level general decision quality or a whole-system speed advantage. SML-CONTEXT is likewise a target architecture: mapped storage, semantic indexing or isolated needle retrieval do not by themselves establish an effective long-context window. SML-RSI currently defines a governed recursive-improvement process, not evidence of open-ended self-improvement; successful candidate PR generation is not promotion until the repository owner explicitly merges it.

## BANC v888 Connectomic Structural Prior track

The V888-only connectomic programme is now a parallel SML research track under the **CSP** namespace so it does not collide with SML or SBG milestones. Its durable execution plan is [`docs/V888_CONNECTOME_BOOTSTRAP.md`](docs/V888_CONNECTOME_BOOTSTRAP.md).

CSP treats BANC v888 as an external source of structural priors, not as an inference-time dependency and not as a biological model to reproduce. The required progression is topology fingerprint -> matched structural controls -> low-bit sparse recurrence -> event-driven active compute -> bounded-memory/SBG/MOR integration -> trainable/Forge-selected topology -> independent TDI evaluation. No CSP result may enter SML-2 merely because it uses V888; it must beat the strongest declared matched controls under the evidence gate in the CSP roadmap.

## DeepSeek-V4.1 principles track — adaptation without Transformer reintroduction

Research input reviewed 2026-09-24: DeepSeek-AI, *DeepSeek-V4.1-Flash: Pushing the Limits of KV Cache Compression*.

SML must not import CED, CSA2, SWA, Q/K/V attention, or Transformer KV caches into its native core merely because the source report is successful. The reusable principles to test are instead:

- asymmetric **ingest versus recurrent-decode compute**, analogous in systems intent to cheaper prefill but implemented through SML-native state construction;
- **hierarchical candidate narrowing**, using SBG/MOR/direct routing before expensive expert or memory work;
- **conditional memory lookup**, compared against SML's existing direct associative memory and Delta-KV rather than relabeled as Engram;
- **storage versus recomputation**, exposed through ElasticXxx only after SML defines exact state/replay semantics;
- a scalar **reasoning-effort budget** controlling recurrent depth, refinement, verification, and optional tool work, with fixed-depth matched controls.

The corresponding roadmap namespace is `SML-DS41`. External benchmark gains and DeepSeek architecture claims are not SML evidence.

## SML-CLM — contrastive decisions and measured scaling

Research direction recorded 2026-10-02. Status: **design only**; SML-CLM-0 through SML-CLM-6 remain open. No contrastive SML training, CLM installation on Thor, scaling fit, calibration result or model promotion is established by this documentation change.

### External evidence and transfer boundary

Primary reference: Jacky Kwok et al. (2026), *Contrastive Language Models: A System One Model for Fast and Generalizable Decision-Making*. The reviewed [CLM README](https://github.com/Contrastive-LM/CLM/blob/bb42c6c5bf914fd449bed2f6ca65be80602cb1f7/README.md#scaling-laws-for-verification) reports bidirectional InfoNCE, hard-negative refinement and empirical verification-loss scaling with compute, data, projection-head size and encoder size. It reports `N* proportional to D^1.02` and approximately 310 tokens per projection-head parameter on its Nemotron DQA experiments. These are source-reported results, not independently reproduced SML evidence.

The published [CLM-v0.1-8B model](https://huggingface.co/Contrastive-LM/CLM-v0.1-8B) and [head implementation](https://github.com/Contrastive-LM/CLM/blob/bb42c6c5bf914fd449bed2f6ca65be80602cb1f7/src/clm/heads.py) are external comparators. Their heads require the encoder/pooling they were trained with; they are not drop-in SBG heads.

Transfer the experimental questions and contrastive principle, **not** the numerical exponents, optimum, token ratio, architecture or claimed speedups. In particular, do not multiply the 25B atlas size by 310 to prescribe a training corpus. SBG bit width, learned parameter count, packed storage and active compute are different quantities. Exact upstream parameter-count conventions, compute exclusions, fit ranges and uncertainties must be established before attempting a numerical reproduction; missing details remain unknown.

### Learning contract

- Learn compatibility between an **observed state + question + candidate definitions** and a verified decision/outcome. Preserve the current task-answer learner as a baseline; an auxiliary contrastive loss must not silently replace language or task objectives.
- Compare three declared arms: current task objective; an InfoNCE research reference; and a native integer-margin/discrete-credit objective. Use Rust for new SML code. Any real-valued log/exp reference stays in a separately bounded research harness, outside the dependency-free Boolean core and production inference path. Exact InfoNCE and an integer surrogate are not interchangeable claims.
- For Boolean vectors, `(2x-1)^T(2y-1) = d - 2*d_H(x,y)` is an exact score identity, not an equivalence of learned model quality. Native masked XOR/XNOR/POPCOUNT scoring must respect observation, relevance and ambiguity separately. Freeze normalization and eligible-coordinate rules, report their support, reject empty support, and prevent mask collapse from creating a spurious high score. Learning requires a separately specified discrete-credit or surrogate-gradient mechanism; POPCOUNT alone supplies no training algorithm.
- Define all valid positives before assigning negatives. Repeated valid actions such as ABSTAIN must not become in-batch false negatives. Prefer state-question-to-decision learning first; allow a bidirectional arm only with explicit multi-positive/equivalence semantics. Duplicate candidates, permutations, ties and no-valid-candidate cases require declared behavior rather than forced argmax.
- Build verified hard negatives for role reversal, negation, stale versus current facts, wrong memory associations and missing evidence. Compare easy-to-hard and from-start schedules at matched budgets rather than assuming one curriculum transfers. Mine training negatives only from the permitted training partition; preserve task/template/source separation and protected holdouts.
- Use exact task oracles or independently verified outcomes for labels. An optional CLM teacher may propose training examples or rankings but cannot label the test set, authorize an action or act as sole ground truth. Record teacher/checkpoint identity, source rights, verification outcomes and rejected proposals.

### Native integration and safeguards

SBG should retain the distinctions needed for decisions; MOR should shortlist bounded experts/candidates; SML-D should decide without text decoding; recurrent refinement should spend extra work only when its measured task benefit warrants it. These are hypotheses to test independently, not guarantees from contrastive training.

The training-only batch comparison is not permission to introduce Q/K/V attention, softmax token scoring, all-history Hamming ranking or an unbounded all-pairs inference scan. Bound and account training batches/negative pools separately; retain direct routing and bounded candidate work in native inference. No CLM, Qwen or Python/PyTorch runtime dependency is added to SML.

A relative ranking score is neither calibrated correctness nor authenticated evidence. SML-D2 must separately qualify calibration/proper scoring, risk/coverage and false acceptance, including changed option counts and distribution shift. Unknown inputs remain unknown; absence of SBG ambiguity does not imply confidence. SML-D4 DIRECT / REFINE / ABSTAIN learning must use observable features and verified refinement outcomes, not hidden evaluator answers.

The DKV-2c missing-successor observation motivates new development controls, not reuse of protected evaluation data or automatic promotion of dynamic re-keying. A scorer cannot detect missing evidence that its inputs do not expose. Qualified DKV-2b, CONTEXT prerequisites and immutable raw-source verification remain in force; contrastive similarity never substitutes for provenance.

### SML-specific scaling and resource accounting

Measure a joint response surface before fitting a law. Separate unique training pairs/tasks, text tokens where applicable, repeat exposures, negative counts and per-expert examples/updates. Report total learned capacity, decision-expert capacity, learned representation capacity, SBG width, packed bits, active parameters per cycle, cumulative touches per decision, recurrent depth and memory capacity. A larger semantic field is not automatically a larger learned encoder.

At fixed declared budgets, compare representation versus expert versus data bottlenecks and expert under-exposure. Sweep data, representation width/capacity, decision capacity and recurrent depth first on small controlled tasks; freeze other dimensions and then examine interactions. Keep both fixed-budget quality curves and matched-quality cost curves. Do not infer useful scaling from isolated kernel timings, larger cold storage or better retrieval loss alone.

Costs include state construction, negative mining, teacher/encoder work, training/search, routing, page materialization, verification and inference. Distinguish one-off embedding precomputation from amortized reuse. Record logical/word operations, wall-clock time, RAM/RSS/VRAM, cold/warm p50/p95/p99 latency, page faults, disk traffic and energy only when measured; FLOPs alone are not a common cost unit for dense and Boolean systems.

Freeze candidate count, positive semantics, negative distribution, temperature protocol, task splits and evaluation identities when comparing InfoNCE losses. Report decision quality, ranking, abstention coverage and false acceptance separately. Retain null/negative findings; fit only supported measured ranges with repeated seeds, uncertainty, residual/bottleneck checks and an explicit loss-floor alternative. Do not extrapolate a fitted curve to 25B without new evidence at each existing SML scale gate.

### Execution and reuse

Next work is **SML-CLM-0** preregistration, followed by **SML-CLM-1** bounded score/label semantics; later gates are defined in [docs/ROADMAP.md](docs/ROADMAP.md#sml-clm--contrastive-decisions-and-measured-scaling). This bootstrap authorizes neither protected final evaluation nor large training runs. SML-RSI human promotion rules remain unchanged.

Remote execution uses `Memorithm/RemoteOps` as the control plane with pinned task/code/data identities, declared budgets and retained evidence. A CLM-on-Thor baseline is optional and must first have a verified installation and compatible encoder/checkpoint; its full cost is separate from native SML inference.

After destination-contract review and qualification, reusable Rust score/label-conformance primitives may benefit SciRust, hard-negative generation/search may benefit Forge, matched panels may benefit TDI, resource telemetry may benefit ElasticXxx, and provenance-aware memory controls may benefit CCOS-Enterprise. Reuse is conditional on tests and independent destination requalification, not a new mandatory SML dependency or a claim that these integrations already exist.

## DKV-3a-0 correction handoff

The owner-supplied cumulative patch versions are identified and their negative
observations retained in `docs/SML-DKV-3A-0-LEGACY-OBSERVATIONS.md`.
The corrected source-evidence adapter is `sml_store::delta_kv_context`; the
numerical Delta-KV core is unchanged. Exact-key lookup, pinned ledger versions,
nonempty private verification receipts, source/element budgets and typed raw
fixture resolution replace the unchecked proof gate. See
`docs/SML-DKV-3A-0-HARDENING.md` for limits and API migration.

Next: receive and review the separate preregistration for Delta-KV as an SBG
recurrent input using DKV-2a projected keys and actual MOR admission. Do not
start that experiment or promote context from the conformance tests here.

## Independent DKV projected-recurrence candidate A

The owner subsequently requested an independent implementation of the previously
deferred tranche. Candidate A is defined by
`docs/SML-DKV-3A-R1-A-PREREGISTRATION.md` and the Rust example
`crates/sml-store/examples/dkv_projected_recurrence_a.rs`.
Its retained development evidence is `research/dkv-r1a/`; the manifest binds
protocol/code/output identities. It uses actual MOR routing, DKV-2a projection,
DKV-2b fixed-key recurrence, direct/no-memory and fixed/cached-read controls.
This is a bounded Development pilot, not full DKV-3a/context promotion.
The owner's separate implementation remains an independent comparison target.
