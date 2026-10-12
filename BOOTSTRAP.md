# SML-GENIUS bootstrap

This file is the durable re-entry point for continuing the SML-GENIUS research programme from the current repository state. Read it together with `README.md`, `docs/ROADMAP.md`, the latest merged pull requests, and the executable contracts in `sml-core`.

## Current PVP6 validation slice — 2026-10-08

After the model-owned selector and executor, validate the public executor on the
unchanged frozen PVP6 corpus with `pvp6-model-executor` and the mandatory fixture
script. The destination contract now includes Q*R reuse output, checked density,
read-only result shape and lossless query-major projection. See
[the qualification contract](docs/SML-PVP6-MODEL-EXECUTOR-QUALIFICATION.md).
This is synthetic functional qualification only; architecture freeze, recurrent
integration, learned utility and physical speed confirmation remain open.

## Current architectural direction

SML is an independent, predominantly Boolean / low-bit autoregressive architecture. It is not defined by reference to Transformer internals. Its native mechanisms include direct routing, Mixture-of-Routers / sparse expert selection, bounded associative memory, recurrent depth, Elastic Weights, and Boolean-integral learning experiments.

Native decision capability is a first-class parallel objective. A substantial decision-expert family and dedicated training objectives must be evaluated before SML-2, not bolted onto a finished language model. Direct decisions and bounded recurrent refinement must work without a text decoder; language generation is optional. See `docs/SML-D-BOOTSTRAP.md`. Total decision capacity, active work per decision and direct-decision coverage are separate quantities; no arbitrary 25B percentage is frozen.

The **SML-CLM** research track adds contrastive state-question-decision learning and SML-specific scaling experiments to this native decision programme. It must compare the current task objective, an explicitly separated InfoNCE research reference and a native integer-margin alternative; it does not import CLM's encoder, scaling coefficients or runtime. The durable contract is [below](#sml-clm--contrastive-decisions-and-measured-scaling); implementation gates are in [the roadmap](docs/ROADMAP.md#sml-clm--contrastive-decisions-and-measured-scaling). This is a research plan, not an implemented learner or a model-quality result.

Long context is also a first-class architecture programme. SML must not equate context capacity with a single dense token buffer. The target is a bounded active semantic/recurrent working set plus immutable raw context segments, associative memory and mapped SBG/raw indexes with explicit provenance. See `docs/SML-CONTEXT-BOOTSTRAP.md`. Raw tokens ingested, effective retrievable span, resident RAM, mapped/disk bytes and retrieval traffic are separate measurements.

Recursive self-improvement belongs to the future **SML-HARNESS**, not to the 25B model. The Harness will be a separate `Memorithm/SML-HARNESS` repository created after the first real SML-6 25B champion exists. Forge, ProofLab, TDI, KVLab, ElasticXxx, CCOS-Enterprise, RemoteOps and other useful Memorithm products remain independently evolving repositories; SML-HARNESS will compose them through explicit versioned adapters/contracts. After the 25B bootstrap milestone, the current champion may autonomously use the Harness to diagnose weaknesses, generate candidates, launch descendant training, qualify results and drive later generations beyond 25B when capability/resource gates justify growth. Existing SML-RSI code is retained only as groundwork until that point. See `docs/SML-MODEL-HARNESS-BOUNDARY.md` and `docs/SML-RSI-BOOTSTRAP.md`.

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

Current DKV-2a Development qualification has advanced through N14. The retained candidate remains the q=3 fixed-width PAGE/PAGE_VALUE composition with the N11 two-bank PRIMARY/ORDERED resolver and N12 exact source/record/subject preflight. N13B preserved all ten frozen semantic/page-skew gates under balanced, N13A-empirical and independent heavy-skew populations; primary-page isolation remains the default and cross-primary sharing requires an explicit replicated numeric write. N14 then measured the unchanged candidate on the pinned aarch64 runner: all 24 cells preserve the frozen semantic/single-read invariants, Linux latency/RSS/process-I/O/page-fault/CPU-runtime instrumentation is available, measured block-device bytes and major faults are zero, and K1024 verified p50 latency is about 13.5-13.7 us on this run. No supported monotonic `energy_uj` domain is exposed, so the full physical-resource gate and any core API promotion remain blocked pending an unchanged N14 replay on an energy-capable host. See `docs/SML-DKV-2A-N13B-PAGE-SKEW-OBSERVATION.md` and `docs/SML-DKV-2A-N14-PHYSICAL-OBSERVATION.md`. The owner-supplied DKV-4k/4l/4m patch is retained as historical mechanism input only: its shared-M/R key-correlation finding is consistent with current N6 evidence, but its field-mask/context-weight APIs are not imported into the current stack; see `docs/SML-DKV-4K-4M-INPUT-REVIEW.md`. The subsequent owner-supplied DKV-4n density sweep is also retained as negative historical input: progressively thinning M/R improves same-chain geometry only while twin-context separation collapses, with no preregistered joint operating point at either frozen load. Its `with_context_divisor` API is not imported; the current N11 two-bank/write-rule design already moves the incompatible semantics outside one monolithic address. See `docs/SML-DKV-4N-INPUT-REVIEW.md`.

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

The V888-only connectomic programme is a parallel SML research track under the **CSP** namespace so it does not collide with SML or SBG milestones. Its durable execution plan is [`docs/V888_CONNECTOME_BOOTSTRAP.md`](docs/V888_CONNECTOME_BOOTSTRAP.md); cross-project coordination with ITD V888-BOOL is frozen in [`docs/V888_BOOL_COORDINATION.md`](docs/V888_BOOL_COORDINATION.md).

Canonical coordination is ITD strategy revision `2026-09-22.1` at commit `7f8f08e764dd5f36688533e9f7e7fb2f290f6e17`. SML retains its existing CSP numbering and quality gates. Source counts must remain typed by selected audited scope: metadata nodes, directed pairs, contacts and raw records are distinct quantities. The old Codex display counts are not SML evidence.

The first functional campaign is delayed XOR with exact sequential-logic, no-reservoir and topology-matched controls; fixed recurrence and a bounded readout precede recurrent learning or topology search. Simplified LIF, bounded integer-event and genuinely Boolean recurrent dynamics remain separate rails. The Rust admission surface is `sml_core::v888_handoff`.

The future consumer stage is `V888-BOOL-4.0`. It remains blocked on executable graph qualification in `Memorithm/scirust#1520` (a focused downstream gate of the V888-GROWTH/source umbrella #1500), exact Rust/source identities, graph-only/router-only/hybrid ablations, bounded resources, no hidden history, no raw-V888 inference dependency and independent SML quality/resource validation.

CSP treats BANC v888 as an external source of structural priors, not as an inference-time dependency and not as a biological model to reproduce. The required progression is topology fingerprint -> matched structural controls -> low-bit sparse recurrence -> event-driven active compute -> bounded-memory/SBG/MOR integration -> trainable/Forge-selected topology -> independent TDI evaluation. No CSP result may enter SML-2 merely because it uses V888; it must beat the strongest declared matched controls under CSP-0.11. Coordination completion is not model promotion.

## DeepSeek-V4.1 principles track — adaptation without Transformer reintroduction

Research input reviewed 2026-09-24: DeepSeek-AI, *DeepSeek-V4.1-Flash: Pushing the Limits of KV Cache Compression*.

SML must not import CED, CSA2, SWA, Q/K/V attention, or Transformer KV caches into its native core merely because the source report is successful. The reusable principles to test are instead:

- asymmetric **ingest versus recurrent-decode compute**, analogous in systems intent to cheaper prefill but implemented through SML-native state construction;
- **hierarchical candidate narrowing**, using SBG/MOR/direct routing before expensive expert or memory work;
  MOR-H1 provides bounded coarse-to-fine pools; MOR-H2 adds adaptive 1 -> 2 -> 4 widening from observable coarse/MOR margins, structural disagreement and SBG ambiguity with cumulative work budgets. These are engineering mechanisms pending matched quality/resource qualification. MOR-H3 adds exposure-conditioned inter/intra-cluster balancing from the same cluster partition. MOR-H4a learns only cluster prototypes from training-only SBG routing states while freezing expert membership and using development holdout solely as a promotion gate. A separate bounded majority-bit arm provides a centroid-like control with seed fallback and hard mutation budgets. Its matched qualification harness compares fixed, learned-medoid, training-majority and deterministic random-medoid arms before H4b may alter membership.
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

## Independent A2 projection ablation

The continuation to candidate A is an isolated Development experiment in
`docs/SML-DKV-3A-R1-A2-PREREGISTRATION.md` and
`crates/sml-store/examples/dkv_projection_contrast_a2.rs`.
It compares unchanged DKV-2a, a two-call complementary projection and
exact direct memory, crossing source payloads independently of seeds.
Retained evidence: `research/dkv-r1a2/`. The original A evidence remains
unchanged. Latent decisions are not source-verified or authorized actions;
context promotion remains BLOCKED. No counterpart code was consulted.


## Counterpart B comparison and subject binding

The owner supplied the independent DKV-3a-1 probe after A/A2/A3 development.
`docs/SML-DKV-B-COMPARISON-OBSERVATION.md` records reproduction on Rust1.89,
method differences, the prospective native-MOR comparison and retained evidence.
An exact compressed key identifies candidates, not an exact subject. Use
`observed_history::bind_projected_subject` with the complete verified history
when binding projected candidates to an independently observed SubjectKey.
It excludes other subjects, rejects missing versions and forked snapshots, and
never replaces or authenticates a latent answer. Existing A/A2/A3 evidence and
the numeric DKV rule are unchanged. Context promotion remains BLOCKED.

Counterpart B follow-up: after `bind_projected_subject`, prefer
`BoundProjectedSubject::screen(candidate)` to retain the same subject/snapshot
without a second identity argument. See `docs/SML-DKV-B-BOUND-SCREENING.md`.
Screening still only preserves or suppresses; it never repairs a latent answer.

## Counterpart C: record versions and subjects

The latest aliasing patch is integrated as an adapted diagnostic and a bounded
`VerifiedContext::bind_record` view; see `docs/SML-DKV-C-OBSERVATION.md`.
A record-version proof must never replace complete subject history. Use
`BoundProjectedSubject::screen` for subject screening; no compressed key or
FNV state surrogate is an authority token. A/A2/B evidence remains unchanged.
Context/model promotion remains BLOCKED.


## DKV-4 adaptive-compute development result

The owner-requested ten-slice DKV-4 engineering programme is summarized in
`docs/SML-DKV-4-FINAL-REPORT.md`. DKV-4 controls recurrent compute only:
`Refine`, `StopCandidate` or `Abstain`; it has no answer-acceptance action.

Use provenance before numeric memory signals. `Missing` and `Conflict`
abstain before any Delta-KV matrix read; `Unverified` may consume only its
bounded research/refinement budget and never reaches `StopCandidate`.
A projected key, low ambiguity or stable numeric output is not presence proof.

DKV-4a is retained as a negative reference: its absolute Q12 correction
threshold caused roughly 1.95x the recurrent cycles of the simpler SBG
stability control without improving correctness on the paired K64/K256
Development panel. Do not restore that residual-magnitude rule as a default.

DKV-4c is the preferred Development controller reference. Correction magnitude
is telemetry only; refinement is driven by actual V/M/A/R change, memory
ambiguity and explicit MOR escalation/disagreement under the same hard depth
bound. It preserves DKV-4a written correctness while removing about 49% of its
measured recurrent cycles. On the current fixed one-page MOR panel, however,
DKV-4c exactly matches the simpler stability-only arm in aggregate correctness
and cycle count. This is not evidence of adaptive-compute superiority.

Retained evidence is in `research/dkv-4b/` and `research/dkv-4c/`. Any next
claim requires a separately frozen non-trivial MOR panel, learned/representative
states and physical resource measurements. Context/model promotion remains
BLOCKED.

## Pascal-GF(2) / Sierpiński reversible-mixer research track

Status: research hypothesis added 2026-10-05. No model-quality, latency, novelty or
production claim is implied.

SML must not reinterpret this track as Transformer attention. The candidate is a
native Boolean state transform for Semantic Boolean Geometry (SBG), defined over
GF(2) by the lower-triangular Pascal matrix

```text
P[n,k] = binom(n,k) mod 2
y[n]   = XOR{x[k] | k is a binary submask of n}
```

For power-of-two widths, Lucas' theorem gives the executable membership rule
`k & n == k`. Over GF(2), the subset-zeta transform is self-inverse, so the
reference contract requires `P(P(x)) == x` exactly. This algebraic property is
the reason to test the candidate; it is not evidence that it improves semantic
quality.

The first implementation target is Rust-only and uses packed `u64` words. A
64-bit scalar/bit-parallel reference has six butterfly stages; wider
power-of-two states add one stage per width doubling. Wider SBG fields may apply
the transform independently to fixed power-of-two blocks until a separately
qualified cross-block construction exists.

### PGF2 invariants

- no Q/K/V, QK^T, softmax, attention-score matrix or Transformer-semantic KV
  cache is introduced;
- no `f32`/`f64` dependency in the PGF2 core;
- exact self-inversion and padding validity are mandatory before any semantic
  experiment;
- the transform is a candidate mixer, not a learned semantic metric;
- identity, fixed reversible permutation and the current qualified XOR/permutation
  mixing path are mandatory matched controls;
- all state widths, operation counts, temporary bytes and block boundaries are
  reported exactly;
- a positive synthetic separation result does not authorize SBG/MOR/Delta-KV
  promotion;
- negative and null results are retained.

### PGF2 progression

1. **PGF2-0 — mathematical/scalar oracle — complete**
   - deterministic Boolean reference for 8/16/32/64-bit fixtures;
   - exhaustive small-width self-inversion checks;
   - equivalence against the binomial-mod-2 matrix definition.
2. **PGF2-1 — packed Rust butterfly — complete**
   - allocation-free in-place `u64` transform after construction;
   - exact parity with PGF2-0;
   - operation and temporary-storage accounting.
3. **PGF2-2 — width ladder — complete**
   - 64/128/256/512/1024/2048-bit packed states qualified in PR #197;
   - K=2048 passes the direct oracle and exact self-inversion on all 16 frozen
     fixtures at 11,264 logical XOR updates, 272 packed-word updates, 256 state
     bytes and zero scratch words.
4. **PGF2-3 — SBG representation ablation — complete, negative**
   - preregistered in PR #198 and executed in PR #200;
   - retained evidence/observation merged through PR #202;
   - at K=2048, global PGF2 contracts STRUCTURED_LOCAL nearest-wrong class
     median 32 -> 12 and route median 64 -> 6;
   - at h=64, structured class retrieval falls 100% -> 42.38% and route
     accuracy 100% -> 57.03%;
   - H1/H5/H6/H7 hold, H2/H3/H4 are refuted; selection is `PGF2-4=NONE`.
5. **PGF2-4 — MOR / Delta-KV handoff — blocked for global K=2048 PGF2**
   - PGF2-3 did not produce the preregistered positive signal;
   - no transformed global PGF2 state may be promoted into MOR or Delta-KV from
     this track.
6. **PGF2-5 — model-level qualification — blocked**
   - component qualification did not pass the PGF2-3 gate.

Stop rule: if PGF2 does not beat the best matched reversible Boolean control on a
declared SBG objective after full transform cost is included, retain it as a
negative result and do not place it on the default SML path.

Current representation-mixing evidence covers both the **global K=2048
transform** and the preregistered **block-local 64/128/256/512** rescue. Global
PGF2-3 is retained in `docs/SML-PGF2-3-SBG-ABLATION-OBSERVATION.md` and
`research/pgf2-3/`; block-local PGF2-3B is retained in
`docs/SML-PGF2-3B-BLOCK-LOCAL-OBSERVATION.md` and `research/pgf2-3b/`.
PGF2-3B improves locality/cost and the failed global h=64 result but does not
recover the preregistered structured geometry/noise gates at any tested block
width. The current Pascal-as-general-SBG-representation-mixer branch is therefore
closed and PGF2-4 remains blocked. Future Pascal work requires a different
prospectively declared objective, not another post-hoc block size, coordinate
ordering or orientation.

### Pascal-Boolean functional suite

A separate prospectively declared functional track now tests Pascal/GF(2)
outside the closed representation-mixing objective.

- PB-1 P-ZETA: retained exact subset-parity aggregation at K=2048;
- PB-2 P-ROUTER: retained exact implicit submask routing across all 2,048
  eleven-bit addresses;
- PB-3 P-ANF: retained exact truth-table <-> ANF conversion, with random-control
  evidence showing that low-degree truncation is not a generic approximation;
- PB-4 P-FEISTEL: retained reversible nonlinear composition at 2/4/6 rounds,
  with zero inverse/collision failures and explicit nonlinearity witnesses;
- PB-5 composed functional evaluation: retained after 131,072 independent
  state/address comparisons with zero end-to-end mismatches; all C1..C6 gates
  hold and the frozen decision is `RETAIN_COMPOSED_FUNCTION`.
- PB-6 matched task utility: retained on the frozen K=2048 subset-parity task.
  U1..U6 hold; P-ZETA median whole-pipeline time is 3.368060 ms versus
  13.344107 ms for the faster matched control, a 3.961956x median speedup with
  20/20 paired wins and exact checksum equality. The advantage is task-bounded,
  not a model-wide speedup claim.

Protocol and retained evidence are in
`docs/SML-PASCAL-BOOLEAN-SUITE-PREREGISTRATION.md`,
`docs/SML-PASCAL-BOOLEAN-SUITE-OBSERVATION.md`,
`docs/SML-PASCAL-BOOLEAN-PB5-PREREGISTRATION.md`,
`docs/SML-PASCAL-BOOLEAN-PB5-OBSERVATION.md`,
`docs/SML-PASCAL-BOOLEAN-PB6-TASK-UTILITY-PREREGISTRATION.md`,
`docs/SML-PASCAL-BOOLEAN-PB6-TASK-UTILITY-OBSERVATION.md`,
`research/pascal-boolean-suite/`,
`research/pascal-boolean-pb5/` and
`research/pascal-boolean-pb6/`.

PB-7 utility-surface generalization is now executed and retained. Across
K=64..4096, packed Pascal matches both the direct oracle and an independently
written logical fast-zeta reference; G1..G7 all hold and the frozen decision is
`RETAIN_GENERALIZED_UTILITY_SURFACE`. On the full-domain affine panel the
median speedup over the faster matched control rises from 3.359x at K=64 to
29.021x at K=4096, with 12/12 paired wins in every full-domain cell. The
K=2048 LOW_POPCOUNT panel also retains the losing boundary: at Q=32 direct
on-demand is 2.877x faster, and at Q=128 Pascal misses the frozen 10% advantage
threshold. See
`docs/SML-PASCAL-BOOLEAN-PB7-UTILITY-SURFACE-OBSERVATION.md` and
`research/pascal-boolean-pb7/`.

The retained surface distinguishes structural work from physical latency:
packed u64 execution can cross over earlier than scalar logical-operation
counts predict. This is measured implementation evidence, not a universal cost
model.

PB-7 satisfies the preregistered gate for defining a separate PB-8
model-aligned task protocol.

PB-8 is now preregistered in
`docs/SML-PASCAL-BOOLEAN-PB8-MODEL-ALIGNED-GATE-PREREGISTRATION.md`. It tests
16-gate banks of 11-feature Boolean/ANF predicates at four coefficient densities
against sparse-ANF scan, direct submask query, generic truth materialization and
a separately labelled hot-resident truth-table reference. The full deterministic
matrix has 144 density/population/Q/reuse cells; the physical panel freezes 17
cells on the qualified ARM64 runner. The experiment is a candidate
admission/decision-gate workload only and does not modify current MOR or
decision semantics.

PB-9 large-width scaling is executed and retained in
`docs/SML-PASCAL-BOOLEAN-PB9-LARGE-WIDTH-SCALING-OBSERVATION.md` and
`research/pascal-boolean-pb9/`. L1..L6 all hold. Exactness and accounting
survive the mandatory K=8,192..1,048,576 ladder; K=1,048,576 records a
183.634 us in-place median for a 128 KiB packed state. On the frozen affine
query panel, direct evaluation still wins at Q=128, while Pascal wins at Q=512
and reaches a 19.4818x median speedup with 12/12 paired wins at Q=2,048.
The resource-bounded extension reaches every preregistered width through
K=268,435,456 (32 MiB packed state) without STOP_RESOURCE; the deepest kernel
median is 81.195066 ms and exact extension accounting/self-inversion remain
intact. The frozen decision is
`RETAIN_LARGE_WIDTH_SCALING_AND_1M_UTILITY`.

PB-6/PB-7/PB-8/PB-9 do not reopen PGF2-4 and do not by themselves promote
Pascal operators into SBG, MOR, Delta-KV, context or model architecture.



### Pascal Vector Projection (PVP) hardware alignment

PB-9 establishes exact packed Pascal execution through K=268,435,456, but the next systems question is hardware alignment rather than larger scalar widths. The PVP programme freezes an address-major gate-bitplane representation and compiles Pascal/subset-zeta into regular staged XOR butterflies over contiguous words/register vectors. See `docs/SML-PASCAL-PVP-HARDWARE-ALIGNMENT-BOOTSTRAP.md`.

The candidate CPU layout is `[K, ceil(G/64)] u64`; the portable GPU unit is `vec4<u32>`, representing 128 gates at one address. The hot path must not materialize the subset lattice as graph nodes or adjacency lists. SciRust qualifies reusable CPU/SIMD and layout primitives; FLAT qualifies portable WGPU kernels and autotuning; NNIS provides optional portable CPU/WGPU runtime qualification. None becomes a mandatory dependency of the final model.

Hardware policy for this programme is vendor-neutral and ecosystem-first: the GPU driver is the only vendor-specific software layer admitted in the target path. The final SML model must be sufficient to itself; after its model interface is frozen, a distinct `SML-HARNESS` will own orchestration/training/evaluation/serving utilities without supplying hidden model semantics.

## A4 recurrent reference (execution-only)

`crates/sml-core/src/pvp6_recurrent.rs` introduces a bounded ANF-bank trajectory
reference with explicit bank replacement and complete output comparison. It is a
step toward A4, not the full recurrent model: the final state is the previous
query-major output, not SBG/MOR/Delta-KV state. No learned utility or performance
promotion is authorized. RemoteOps remains the physical control plane.

## A4 state-fed SBG integration batch

The sequence-only PR237 reference is no longer the active integration surface.
`pvp6_sbg`, `pvp6_sbg_kernel`, `semantic::transactional`, `pvp6_sbg_routing`
and `pvp6_sbg_trace` provide declared state feedback, checked coefficient updates,
owned-stage rollback, logical budgets, native MOR routing and bounded trace exchange.
The native DKV-2b/ANF/MOR test has 48 direct-control trajectories. The complete
public-API panel has 192 cases, 720 cycles, owned ANF checkpoint continuation and
full four-field trajectory replay. Read `docs/SML-A4-SBG-INTEGRATION.md` for exact
contracts and remaining gates. These are synthetic functional results, not trained
utility, physical speed or final architecture qualification. Checkpoint continuation
is in-memory and ANF-only; expert-page execution and dynamic DKV keys remain open.

## Active next A4 slice — selected experts and dynamic state/route memory

Read `docs/SML-A4-EXPERT-DYNAMIC-LOOP.md` before extending the PR238–247 path.
The next ten slices execute declared expert page bodies and feed their output
plus the current native MOR decision into Delta-KV projection. Legacy frozen-key
and frozen-route controls stay unchanged. Model-quality and performance gates
remain separate; execution tests alone do not close A4.

## A4 selected experts / dynamic memory qualification

The active declared loop now executes native MOR-selected ANF page bodies and
reprojects Delta-KV from their post-expert SBG output plus the current route.
`semantic::session` resumes the complete owned pipeline in memory. Read
`docs/SML-A4-EXPERT-DYNAMIC-LOOP.md` for API semantics, logical budget boundaries
and the 108-case/456-cycle exact panel with independent std-only arithmetic replay.
This does not qualify learned page quality, physical speed, persistent checkpoint
storage, broader geometries or a final model architecture. Old fixtures remain.

## A6-C12d identifiability and screening evidence — 2026-10-12

Before extending three-gate ANF source selection or connecting it to
SML-GENIUS learned banks, read the preregistered
[correlated/alias protocol](docs/A6-C12D-CORRELATED-IDENTIFIABILITY-PREREGISTRATION.md)
and [measured results](docs/RESULTS-A6-C12D-CORRELATED-IDENTIFIABILITY.md).

The native hybrid W2+Sketch-MITM recovered 12/12 exact hidden positions,
input channels and tables for strongly correlated **but distinct** sources
within an N128 fixed-ring/three-hidden-gate setup and a 508-candidate pool.
On the *different* input domain `x1=x2`, 12/12 native alternative-generator
comparisons yielded **identical** outputs despite distinct hidden input
channels. The learner matched all 12/12 native TEST outputs but no longer
matched the generator's arbitrarily chosen input-channel name (0/12).

Scientific contract for future work: **structural parameter equality**,
**observable functional equivalence**, and **identifiable minimal
representations** are different gates. When input aliases exist, require a
formal identifiability diagnostic before interpreting a structural miss.
Keep native TEST free-running with no teacher forcing, frozen source rankings,
registered budgets and honest controls. The bounded ~35× improvement over
exhaustive W3 on eight paired correlated/alias streams is a CPU measurement,
not a worst-case complexity bound, a universal LPN solution, a language-model
quality result or million-function-bank scaling.

Next: distinguish non-identifiability from finite-sample failure under
biased/time-shifted channels and corrupt recurrent state; only then generalize
to k≥4/5 with an appropriately capable learner. SML remains self-contained;
RemoteOps is the reference physical control plane, and no proprietary
NVIDIA software is imported.

## A6-C12e temporal and stochastic-state evidence — 2026-10-12

Read the [frozen experimental protocol](docs/A6-C12E-TEMPORAL-STATE-PREREGISTRATION.md)
and [native Rust results](docs/RESULTS-A6-C12E-TEMPORAL-STATE.md)
before continuing the A6/ANF source-selection and bank programmes.

The bounded N128/three-gate hybrid learner can distinguish imperfect
one-step temporal aliases (12/12 exact original positions, sources,
truths), but when `x3(t)=x4(t-1)`, independent native generators using
`j68/x3` and `j67/x4` have the same observable behavior (12/12
zero-disagreement TEST witnesses). The hybrid recovers the observable
function 12/12 without necessarily returning the arbitrary original
position/channel labels (0/12 strict identity). Evaluation contracts
must carry a distinct observational-equivalence class identifier or
witness and must not penalize an unidentifiable hidden representation.

A third native panel flips bits of the **persisted recurrent state**
`state[127]` with probability 1/256 per lane per step. The learner
still identified the original deterministic parameters on all 12/12
cases and matched that deterministic model's errors against the noisy
teacher labels. This is evidence of structure discovery under the
specific weak hidden-state perturbation, not latent-noise prediction,
a minimax error floor or successful general stochastic inference.

All measurements were made in a fixed 128-bit recurrent topology with
508 candidate input-channel/lag features and two synthetic three-gate
families. Never cite the measured ≈35× W3-vs-hybrid fit speed difference
as a worst-case asymptotic improvement, million-function bank result,
Jetson/Thor native speed or SML-25B language benchmark. Subsequent
capacity expansion to k≥4/5 gates must explicitly represent those
gates before assessing their recoverability. Retain Rust-only native
tooling, no proprietary GPU software beyond driver support, and
RemoteOps as independent physical control plane.

## A6-C12f state-noise and bitwise-equivalence evidence — 2026-10-12

Before promoting the current three-gate ANF source selector or using it
to qualify a massive bank, consult the [pre-registered experiment](docs/A6-C12F-STATE-NOISE-OBSERVABILITY-PREREGISTRATION.md)
and its [native TEST/CI evidence](docs/RESULTS-A6-C12F-STATE-NOISE-OBSERVABILITY.md).

**New required evaluation contract**: distinguish exact hidden
parameters, equal *numbers* of errors, **equal output bits** on
independently generated held-out streams, and provable observational
equivalence on a declared restricted input domain. Native evaluation
executes two independent LocalAnfNetworks from zero state on the
same TEST inputs, compares their outputs bit-by-bit, and is independent
of noisy labels. Equal error counts are not sufficient.

The 6-mode N128 fixed-ring, k=3 benchmark injects Bernoulli(1/256)
or Bernoulli(1/64) flips into actual persistent state slots 0,68,127
**after** transition and **before** readout. It recovered 72/72 original
gate triples and produced zero bit disagreements with the underlying
deterministic native teacher on all 72 held-out stream comparisons.
These sources were selected among **508 candidate channel/position
features**, not 508 state bits or 508 independent gates. This result
does NOT imply prediction of hidden random transition flips.
A separate exact temporal-lag witness formally relates the
j68/x3 and j67/x4 XOR injections only under their lagged-input
constraint and zero initial state.

Keep the measured ~35.138× advantage over brute-force W3 (24 matched
cases) bounded to this synthetic CPU fit. The learner is still
fixed-topology, three gates; no graph-learning guarantee, no millions
of ANF bank entries demonstrated, no k≥4/5 learned circuits, no
SML-25B language evaluation, no Thor GPU acceleration. Require a
new source-aware and capacity-matched preregistration for future
experiments with hidden state correlations, masking and expanded k.
The ANF/SBG/MOR/Delta-KV integration gate remains independent.
