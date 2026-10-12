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

## WideSparseANF — fan-in beyond eight without exponential dense storage (2026-10-12)

The `LocalAnfGate` fan-in=8 constant is the **retained low-width dense
fast-path limit**, not the final SML-GENIUS architectural ceiling. Read the
[frozen wide-gate protocol](docs/ANF-WIDE-SPARSE-1024-PREREGISTRATION.md),
the [post-baseline optimization addendum](docs/ANF-WIDE-SPARSE-1024-PERF-AMENDMENT.md)
and [native Rust evidence](docs/RESULTS-ANF-WIDE-SPARSE-1024.md).

The new public `anf_wide_sparse` module provides `WideAnfGate` and
`WideAnfNetwork` with **dynamic fan-in and explicit resource budgets**,
adaptive dense `k<=8` versus sparse active-monomial dispatch, multiword
u64 masks, shared-prefix AND products, caller-reused 64-lane scratch and a
native recurrent-network adapter. It interoperates with historical
`LocalAnfGate`, `LocalAnfNetwork`, and `LargeAnfGate` while leaving
existing SBG/MOR/Delta-KV routes unchanged. Exactness evidence:
**22/22** synthetic gate/width truth panels for k8,16,32,64,128,256,1024;
**2/2** independent N128 native recurrent trajectories; **1/1** legacy
31-bit sparse bridge; and one actual **N1024 recurrent step containing a
1024-input gate**.

The measured k1024 `linear32` and `mixed16` sparse functions use just
32 and 16 active monomials, not `2^1024`. In the qualified native
benchmark they have logical gate payloads of 13,056 and 11,120 bytes,
scratch budgets of 33 and 48 u64 words, respectively. High-degree
`full_and` still requires 1024 shared-prefix AND nodes and costlier
evaluation (~2.37 microseconds in the optimized CPU run). These timings
are **micro-kernel evidence only**; different ephemeral runners and
a checksum instrumentation adjustment prevent attributing all before/
after speed differences to the O(1) source-bound optimization.

**Keep independent qualification gates:** discovering/learning monomials
at k>8, routing wide gates through native SBG/MOR/Delta-KV, physical
AVX/NEON/GPU execution, a K=1,048,576 function bank and SML-25B language
model quality. A gate width of 1024 is **not** the number of ANF functions
in a bank, nor a count of active gates per token. Native Rust only;
no proprietary NVIDIA software stack apart from permitted drivers.

## WideSparseANF selected pages in SBG/MOR/Delta-KV — 2026-10-12

The [frozen WideSparseANF page-integration protocol](docs/ANF-WIDE-SBG-MOR-DELTAKV-PREREGISTRATION.md)
and [verified native results](docs/RESULTS-ANF-WIDE-SBG-MOR-DELTAKV.md)
extend PR #498 beyond standalone wide-gate and recurrent execution.

The opt-in `WideAnfPageAtlas`, `WideRoutedExperts`, and `WideAnfDeltaKvLoop`
now use the existing MOR `MorDecision` and `RoutedSemantic`, the existing
SBG `SbgCoordinate` projections, and the actual `DeltaKvDynamicRekeyKernel`.
A native selected page reads **1024 actual semantic-state input coordinates**
into a 1024-input sparse AND gate and commits its output only if budgets,
selected-page destinations and coupled memory stages succeed. Tests confirm
a single immutable SBG snapshot for all selected page reads, 2 complete
manual-vs-coupled MOR/page/Delta-KV cycle comparisons, and rollback of
routes/traces/memory on a failing projection or gate budget.

Important: each scalar SBG coordinate is **broadcast** to all 64 u64 lanes
before the native WideAnfGate evaluator runs. This does **not** benchmark
64 different SBG states simultaneously. It is an explicit alternative
selected-page backend; old PVP6 and LargeAnfPageAtlas routes still work
unchanged and are not automatically promoted.

Next: train sparse wide monomials with TRAIN/VAL-only selection, build a
truly budgeted multi-page bank with active/MOR page gating and physical
memory accounting, add 64-state batched SBG evaluation, and qualify
native Thor acceleration without non-driver NVIDIA software. No million-
function bank, SML-25B language results, or general learned topology
has been qualified by these functional integration tests.

## Wide ANF sparse monomial discovery via active interventions — 2026-10-12

After the merged wide-kernel PR #498 and MOR/SBG/Delta-KV page
integration PR #499, the [frozen A6-WL1 protocol](docs/ANF-WIDE-ACTIVE-SPARSE-LEARNING-PREREGISTRATION.md)
and [native test results](docs/RESULTS-ANF-WIDE-ACTIVE-SPARSE-LEARNING.md)
establish the **first fit, rather than merely execution**, of
previously unknown sparse ANF monomials up to **1024 inputs**.

`anf_wide_active_learn::fit_selector_anchored_sparse` learns via
TRAIN-only **adaptive membership queries**: 4 PUBLIC selector
positions `x0..x3`, exactly one positive-literal AND monomial gated
by each selector, remaining support unknown. Under this restricted
grammar, one-hot interventions plus group bisection identify each
unknown support. The native learning core does **not** read generator
monomials or VAL/TEST, and reports physical query costs; the noisy
panel uses 15 independent repeated observations with p=1/16 label
flips. The resulting WideAnfGate can be placed directly into
`WideAnfPageAtlas` and executed under a native selected MOR PageId.

**Bounded evidence:** 42/42 exact source supports and ANF functions
(7 k widths×3 seeds×2 TRAIN panels), 42/42 VAL accepts, zero bit
disagreements on 42×4096 heldout TEST predictions, 6 k8 exhaustive
truth tables, 7/7 detected cross-selector misspecification controls
and 16/16 selected-page outputs for a learned k1024 gate. Panels
share only **21 distinct teachers**, and the fit's one-hot oracle
access is an active data-generation assumption, NOT passive learning
from natural language sequences.

**Do not promote** this to an arbitrary 1024-input ANF learner,
to the selection of unknown routing selectors or learned page IDs,
to a bank of 1,048,576 trained ANFs, or to SML-25B quality/speed.
Next scientific gates: learn unknown selectors, multiple and
interacting monomials under explicit observability and resource
constraints, then replay the fit inside true recurrent SBG/MOR/
Delta-KV model loops with passive input streams and held-out tasks.
Keep Rust-only and vendor software restrictions unchanged.

## A6-WL2 selector-free sparse ANF support recovery — 2026-10-12

The [pre-registered design](docs/A6-WL2-UNANCHORED-ANF-PREREGISTRATION.md)
and [native scientific evidence](docs/RESULTS-A6-WL2-UNANCHORED-ANF.md)
advance beyond A6-WL1's four PUBLIC selector positions. The new
`anf_wide_unanchored_learn::fit_unanchored_disjoint_sparse` learns
the **full unordered variable support** of four positive GF(2)
ANF monomials **without a designated selector variable**.

**Important identifiability correction:** choosing which factor
of an AND is called a selector is unobservable from its truth table,
because Boolean conjunction is commutative. Instead evaluate exact
supports and autonomous held-out output bits.

The learner's **strong announced prior** is four nonempty, positive,
pairwise-DISJOINT monomials of degree 2..5 and an active membership
oracle; an adaptive residual-positive witness is minimized by
group-deletion to a degree-bounded monomial. It does NOT learn
overlapping/negated terms, unknown number of monomials, or passive
language sequence dependencies. TRAIN noise p=1/16 is controlled
using **21 strict-majority observations** per logical query.

The bounded k=8,16,32,64,128,256,1024 native experiment recovered
**42/42 exact supports** across 21 distinct teacher structures
(3 seeds×2 TRAIN panels), passed all 42 separate 544-case VAL panels
and 42 independent-stage 4096-case TEST validations (0/172032
prediction-bit disagreements), 6/6 exhaustive k8 truth tables,
and rejected 7/7 targeted fifth high-degree AND controls. A truly
**learned k1024 WideAnfGate** was inserted into the MOR-decision-
selected WideAnfPageAtlas and passed 16/16 SBG output checks.
On k1024 391.3 logical queries and 8218 physical noisy
observations were measured on average. These are local synthetic
membership callbacks, not production access to an interactive
teacher of arbitrary tasks.

Keep separate scientific gates for overlapping supports, multiple
monomials interacting under XOR, passive/online learning, unknown
sparse cardinality, native k4/k5 hidden recurrent gate discovery,
MOR policy learning and bank-level SML-25B validation. The current
API is purely Rust, retains the old engines and requires no
proprietary NVIDIA software other than allowed GPU drivers.

## A6-WL3 — overlapping wide ANF supports and hidden model order (2026-10-12)

Read the [scientific protocol frozen before code](docs/A6-WL3-OVERLAP-ORDER-PREREGISTRATION.md)
and [full native results](docs/RESULTS-A6-WL3-OVERLAP-ORDER.md)
before claiming generalized wide ANF learning. Following A6-WL1
(known selector) and A6-WL2 (unknown selector but disjoint terms),
the Rust `anf_wide_overlap_learn::fit_overlap_sparse` now handles
**overlapping and nested monomial supports, with UNKNOWN exact term
count in 2..4**, using a bounded active GF(2) membership oracle.

The core methodological change is an **exact Möbius coefficient
certificate** on all `2^d` submasks for a candidate degree d≤5.
A locally minimal positive XOR witness can be a coalition of
three overlapping active monomials and is therefore not itself
sufficient proof of a coefficient. Only a real residual ANF
coefficient of 1 may be accepted; stop occurs after 512
uniformly sampled residual negatives, even when four terms were
already recovered. On a nonzero degree≤5 GF(2) polynomial with
a clean independent Boolean oracle, the minimum-weight distance
bound gives per-stop false-negative chance
`(31/32)^512≈8.72e-8`. Under noise, misspecification or
correlated inputs this is **NOT** a certainty certificate.

The native synthetic panels comprised 7 widths 8..1024, three
patterns STAR4/CHAIN3/NESTED2, 3 seeds and clean versus
Bernoulli1/16 TRAIN noise with 21-vote majority. Exactly
**126/126 unordered hidden supports AND term counts recovered**,
126/126 validation acceptances (544 labels each),
**0/516096 bitwise errors** across heldout TEST rows, 18/18
full k8 truth tables, seven targeted fifth-degree6 AND teachers
explicitly rejected with `TooManyTerms`, and 16/16 correct
outputs from a **fitted k1024 gate installed in a selected SBG
page**. The two noise panels share the same 63 underlying
teachers and paired TEST streams; do not treat 126 rows as
126 independent populations.

The **explicit boundaries** remain up to four positive monomials
of degree ≤5, controllable membership/intervention queries,
bounded 160k oracle observations, and explicit selected page.
This is NOT identification of arbitrary ANF polynomials,
passive language sequence training, native GPU/Thor speed,
dynamic learned MOR policy, full SBG/Delta-KV end-to-end fitting,
a million learned ANF functions or demonstrated SML-25B quality.
The next evidence gate should study passive observations and
identifiability of rare interaction supports under memory and
recurrence, not silently generalize the active-oracle results.

## A6-I1 epistemic intuition and observational nonidentifiability — 2026-10-12

Read [the preregistered protocol](docs/A6-I1-EPISTEMIC-INTUITION-PREREGISTRATION.md)
and [full experimental results](docs/RESULTS-A6-I1-EPISTEMIC-INTUITION.md).
The `anf_epistemic_intuition` Rust module is a bounded, uncertainty-aware
**two-hypothesis WideAnfGate belief**, with `ContextualPrior` learned
from earlier **fully labeled tasks**, predicted output probabilities,
explicit hypothesis agreement/disagreement, consensus-only abstention
and a separate, charged active-intervention classifier.

The test **constructively proves an identifiability limit**:
`f1(x)=f0(x) XOR x_a x_b`, with the passive context always satisfying
`x_a x_b=0`. No amount of observations from that support can
distinguish their task-local identities, even under independent
readout label noise. A conditional prior from the histories of
**other tasks**, provided with a context tag, can nonetheless improve
prediction on context-shifted unseen inputs **if the prior is stable**.
Over 7 widths k8..1024, 3 seeds, 2 passive censoring modes, 2
label-noise panels and 3 deployment regimes, 252 blinded cells
confirmed exact passive equivalence, 16 OOD forced-disagreement
queries per blind task and honest negative controls.

Native context-conditioned forced predictions on the OOD
hypothesis-disagreement subset were **87.6379% accurate under
matched historical priors** versus ~50% for pooled/uniform controls.
They failed **catastrophically under inverted priors: 11.8151%**;
without contextual signal accuracy was **49.6503%**, close to chance.
Calibrated confidence under MATCHED had ~1.45 percentage-point gap,
but ~74.37 points under REVERSED. Consensus-only output covered
~50.13% of OOD queries, with **zero clean errors**, abstaining
on all hypothesis disagreements. A SEPARATE targeted intervention
with 1 clean/21 noisy observations disambiguated all 5376 blind
tasks per regime in the tested corpus; intervention is additional
information, NOT passive intuition.

**No learned human-like cognitive intuition is demonstrated.**
Both candidate functions are provided by the benchmark and
include the true teacher by construction; historical tasks carry
their true candidate identity, and the context tag is given.
Prior-shift detection is NOT solved by local observations. Future
tests require >2 hypotheses including true-model-missing cases,
meta-history with feedback instead of oracle identity labels,
adaptive change detection/intervention, and passive recurrent
SBG/MOR/Delta-KV observation streams before any SML-25B claim.
Retain native Rust, restricted GPU vendor software and strong
rollback/CI contracts.

## A6-I2 passive ANF committee, OOD shifts and missing-teacher risk — 2026-10-12

The [frozen four-way passive protocol](docs/A6-I2-FOUR-HYPOTHESIS-PASSIVE-PREREGISTRATION.md),
the [honestly recorded comparator amendment](docs/A6-I2-FAIR-BASELINE-AMENDMENT.md)
and the [source-backed A6-I2 results](docs/RESULTS-A6-I2-FOUR-HYPOTHESIS-PASSIVE.md)
qualify **uncertainty management, not general ANF discovery**.

A new Rust `anf_passive_committee::PassiveCommittee` ranks 2..8
GIVEN `WideAnfGate` hypotheses by a passive noisy-label
log-posterior. Its four-candidate k8..1024 synthetic test uses a
**persistent Markov input process**, 512 TRAIN steps, optional
Bernoulli1/16 label flips and 2048 new OOD inputs from a changed
process. Across **504 frozen committees**, EXACT passive signature
classes identify 1, 2 or 4 distinguishable candidates according
to the three visibility regimes. All four teacher labels and
posteriors are indistinguishable in BLIND; partial visibility
separates two groups, FULL reveals four. Teacher posteriors
are ≥0.99 in 168/168 FULL fits.

With the true teacher guaranteed IN the supplied bank, confidence
threshold 0.90 accepts **56.19% of BLIND OOD, 74.94% of PARTIAL
and 100% of FULL**, with no accepted clean errors across this
bounded corpus. This is **not** proof of calibrated confidence
under arbitrary OOD; 21 OOD input streams are shared across
the teacher panels. The fixed simplicity prior [4,2,2,1]/9
underperforms a fairly updated uniform prior in BLIND Brier
(0.120071 vs 0.109515) and PARTIAL (0.069606 vs 0.062645)
on BALANCED teachers. Both converge in FULL. Original static
uniform and posthoc uniform-updated reference results remain
separately documented; do not confuse them.

**Safety-critical counterexample**: when the true function
`h0 XOR x[r6]x[r7]` is absent from all four hypotheses,
21/21 episodes with visible training evidence are flagged by
the model-adequacy guard. If x[r6]=0 throughout passive TRAIN,
the extra monomial is mathematically invisible: 21/21 bank
checks accept, yet **5981/24168 high-confidence OOD predictions
are wrong** (≈24.75% error). A unanimous candidate bank can be
confidently wrong because the bank itself is incomplete.
Do not trust 100% apparent consensus as universal certainty.

**Outstanding gates**: learn candidate monomials rather than just
rank a supplied bank; detect missing support and covariate shift
via controlled low-budget interventions; update a genuine SBG/MOR/
Delta-KV recurrent model, measure calibration/selective risk
under balanced and historical meta priors, and validate true
1,048,576-function bank behavior. The tested Markov process is
an input recurrence, NOT SBG/Delta-KV model recurrence. No
SML-25B language quality, native GPU performance or human-like
intuition claim follows. Rust-only; no prohibited NVIDIA software.

## A6-I3 conservative OOD support envelope with mandatory unseen-pair failure — 2026-10-12

After A6-I1 [intuition under observation ambiguity] and
A6-I2 [four-way passive committee with missing-teacher failures],
the [frozen A6-I3 protocol](docs/A6-I3-EPISTEMIC-SUPPORT-PREREGISTRATION.md)
and [measured report](docs/RESULTS-A6-I3-EPISTEMIC-SUPPORT.md) qualify an
explicit **O(k) empirical marginal support guard** around the native
`PassiveCommittee`, without altering the learned posterior or candidate bank.

The new `AnfMarginalSupportGuard` stores one u8 per input dimension,
tracking separately whether 0/1 appeared in TRAIN. The separate
`predict_guarded` action accepts an existing confident posterior
prediction (threshold 0.90) **only** if the original native committee
has adequate TRAIN fit (minimum candidate error <=1/8) and the
current input contains no marginally unseen values. This is an
ABSTENTION policy, not a new probability estimate or a proof of
candidate-bank completeness.

The 210-fit ×2048 TEST native stress showed:
- CENSORED_ABSENT: **43,008 confident committee errors removed** by
  rejecting the novel half; 43,008 accepted predictions remain exact.
- CENSORED_INCLUDED: **43,008 correct predictions also rejected**;
  this exposes the cost of the conservative alarm.
- PAIR_CENSORED_ABSENT: both bits 0 and1 seen separately, but never
  jointly 1; the guard accepts 86,016 OOD inputs and makes **43,008
  confident wrong predictions**, a mathematically exact blind spot.
- FULL_INCLUDED: 86,016/86,016 accepted with no error.
- FULL_ABSENT: existing committee's 1/8 TRAIN mismatch alarm rejects
  42/42 inadequate candidate banks and abstains from all 86,016 TESTs.

**Never claim a universal guarantee from input support alone**.
No univariate alarm can detect a novel pair when each marginal has
already taken both values. High-confidence agreement of a committee
is conditional on the true hypothesis being in the bank. Real
SML-GENIUS deployment requires explicit OOD monitoring, groupwise
interaction witnesses, false-abstention accounting, and tests
where high-order hidden functions remain observationally
indistinguishable. This is synthetic Rust reference evidence, not
human intuition, SML-25B language quality, passive learning of
unknown monomials or native Thor GPU speed.

## A6-I4 — input-pair support guard and higher-order failure (2026-10-12)

After merged A6-I1/I2/I3, the [frozen pairwise-OOD protocol](docs/A6-I4-PAIR-SUPPORT-TRIPLE-PREREGISTRATION.md)
and [native scientific results](docs/RESULTS-A6-I4-PAIR-SUPPORT-TRIPLE.md)
add the PUBLIC Rust `anf_pair_support::AnfPairSupportGuard` alongside
the existing `AnfMarginalSupportGuard` and `PassiveCommittee`.
Only passive TRAIN inputs enter the guard: it stores 512 bit-sliced
observations per variable, seals each input PAIR's four possible
Boolean configurations once via u64 intersections and retains
only the **missing pairs**. No teacher monomial, future output,
dynamic MOR route or corrected candidate list is supplied.

On **252 preregistered fits** and **516096 OOD predictions**
(7 widths k8..1024 ×3 seeds×2 TRAIN label panels×6 regimes),
the new guard eliminates **43008 confident errors** that the
old marginal filter misses when the two individual values have
been seen but their pair11 was never co-activated. **This costs
43008 unnecessarily rejected correct predictions** if the
true teacher is already in the four-candidate bank.

**Critical required counterexample:** when all three input PAIRS
have each observed all four Boolean assignments but the TRIPLE
111 was never observed in passive TRAIN, a missing degree-three
coefficient remains invisible. The guard accepts **86016/86016**
predictions and **43008 are confidently WRONG**, exactly as
preregistered. No claim that order-two support monitoring
certifies distribution-free correctness or bank completeness.
For k1024 the logical training-column payload is **65536 bytes**;
one missing pair adds 24 bytes on the measured native ABI;
1023 missing pairs under single-dimension censoring consume
90088 bytes total, excluding Vec overhead and transient memory.

**Next independently qualified gates:** adaptively budgeted
third-order and higher support representation with mandatory
order+1 failures; explicit posterior calibration under prior
and distribution shift; novelty-triggered candidate expansion
from TRAIN only; and real passive SBG/MOR/Delta-KV multi-cycle
validation. Do not confuse absence of a visible interaction
with proof of correctness, or synthetic performance with
SML-25B model quality.

## A6-I5 — empirical strength-three support does not certify generalization (2026-10-12)

Following merged #506 (A6-I4), the opt-in `anf_triple_support::AnfTripleSupportGuard` examines every Boolean pattern of every three-coordinate input subset, under explicit combinatorial and retained-record budgets. Read the [protocol frozen before implementation](docs/A6-I5-TRIPLE-COVERAGE-PREREGISTRATION.md) and the [qualified positive and mandatory negative results](docs/RESULTS-A6-I5-TRIPLE-COVERAGE.md). The first native run verified 90 TRAIN-frozen committees and 46,080 OOD TEST decisions: in the hidden-triple scenario, the pair guard accepted 15,360 / 7,680 erroneous decisions while the triple guard accepted 7,680 / 0 erroneous decisions; with the true candidate present, the triple guard unnecessarily abstained from 7,680 correct decisions. Crucially, on a hidden degree-four interaction whose every triple pattern was seen during TRAIN, the triple guard accepted 15,360 decisions with 7,680 confident errors and zero triple-novelty alarms.

This is an input-coverage gate, not coefficient reconstruction, epistemic correctness, or a general confidence certificate. Exact global strength-three construction refuses k256 or k1024 under the frozen 350,000-triple budget (k128 uses 341,376). Any next candidate must use explicitly budgeted feature/interaction selection and compare the cost of abstention against the cost of additional informative observations, with higher-order negative controls and untouched TEST. The bank remains supplied, not autonomously enriched; no language/SML-25B inference quality or GPU speed has been demonstrated.

## A6-I6 fourth-order watched interaction guard — 2026-10-12

Read the [frozen preregistration](docs/A6-I6-BOUNDED-FOURTH-SUPPORT-PREREGISTRATION.md)
and [fully auditable results](docs/RESULTS-A6-I6-BOUNDED-FOURTH-SUPPORT.md)
before making confidence or OOD guarantees for the native ANF committee.

After I3's per-input marginal support, I4's exact pairs and I5's
exhaustive triplets, the new opt-in `anf_budgeted_four_support`
provides fourth-order **watchlist support**, tracking precisely
the sixteen input assignments for **only externally declared**
tuples, without enumerating `C(k,4)`. At k1024 an exhaustive
fourth-order guarantee would entail 45,545,029,376 tuples.
A watchlist may be useful when a relevant interaction subsystem
was known BEFORE seeing the blind teacher: it is a structural
prior, NOT newly inferred hidden teacher coefficients.

The A6-I6 native experiment used 7 widths k8..k1024, three
seeds, clean/noisy TRAIN labels, 512 passive TRAIN contexts and
512 independent heldout TEST contexts over four regimes:
**168 frozen native PassiveCommittee + watch guard policies**,
86,016 OOD TEST decisions. Watch for a missing fourth-order
teacher term caught **10,752 erroneous predictions**, leaving
10,752 accepted and zero errors; with teacher genuinely in-bank
it needlessly abstained **10,752 correct predictions**.
**Mandatory adversarial failures:** when the hidden teacher
instead used an **UNWATCHED fourth- or fifth-order ANF term**,
the guard retained 21,504 high-confidence predictions and
made **10,752 errors** in EACH scenario, with no novel public
watchlist patterns. Every regime's hidden teacher is exactly
indistinguishable from h0 on the censored TRAIN inputs.

Memory metric: one watched tuple stores 40B of logical payload
on tested x86_64, NOT total committee model RAM or coverage
of all fourth-order combinations. The watchlist is capped at
16 externally declared tuples and the observed sample count
at 512. No benchmark here establishes arbitrary absent-model
detection, recovered ANF coefficients, production SML runtime,
online active feedback, GPU throughput or SML-25B quality.
Next scientific gate: intervention/feedback under delayed
teacher access with an explicit question-selection budget,
plus mandatory inaccessible-interaction counterexamples.

## A6-E1 — stop using synthetic support guards as a substitute for real model utility (2026-10-12)

The [immutable preregistration](docs/A6-E1-REAL-BYTE-SUPPORT-RISK-PREREGISTRATION.md) and [first native corpus report](docs/RESULTS-A6-E1-REAL-BYTE-SUPPORT-RISK.md) implement a strict natural-data SML model-utility decision. The experiment TRAIN-learns four actual wide native ANF gates using 16 Boolean bits from the previous two UTF-8 bytes, derives input-pattern support from the union of those **learned** gate input positions, computes exact full-16-bit TRAIN-nearest Hamming distance, and explicitly compares degree-1/2 GF(2) design ranks without falsely certifying unknown teacher grammar. A backward-compatible `PassiveCommittee::new_with_source_width` lets heterogeneous 2/3/4/5-input candidate gates share a single 16-bit input. A deliberately mandatory negative unit test confirms that complete order-two pair coverage does not identify even a degree-one parity teacher without rank/realizability assumptions.

The real public-domain French next-space event corpus (433,036 bytes; strictly checksum-verified), matched 80/10/10 contiguous split, and VAL-only selection of all 32 preregistered risk-coverage policies completed successfully at [run 38275221470](https://github.com/Memorithm/SML-GENIUS/actions/runs/38275221470). On 43,136 held-out binary TEST events, Witten–Bell ngram4 achieved **0.21751656 bits/event**, whereas the committee without abstention was **2.52933653** and the VAL-selected ANF + mandatory ngram4-fallback policy remained **2.52734408** at 43,091 / 43,136 accepted predictions (**99.895679% coverage**). Three of four learned Boolean gates collapsed to constant zero, and the learned support-scope gates detected essentially no out-of-support tuples. **Preregistered verdict: STOP, not a model-quality success.** These are binary bits/event, not the earlier A5 full-byte bits/byte figures and not evidence for SML-2 or SML-25B.

**Execution change:** Further increases in guard degree alone are not an advancement. To reopen the model-level gate, propose a separately preregistered TRAIN-only *calibrated* ANF probabilistic readout with sparse/backoff counts, compare against the exact same ngram4 baseline and ultimately the 256-symbol next-byte task; select models exclusively on VAL, freeze before TEST, retain uncertainty, RAM/time and resource accounting, and do not change this negative result. Existing SciRust/FLAT-ATTENTION/NNIS projections are reusable only after model-quality evidence justifies them. A real task with a STOP criterion now takes precedence over multiplying synthetic assurances.

## A6-C12g / A6-C12h — résultats de transition qualifiés sur enseignants synthétiques (2026-10-12)

Les PRs [#510](https://github.com/Memorithm/SML-GENIUS/pull/510) et [#511](https://github.com/Memorithm/SML-GENIUS/pull/511) sont désormais **MERGED**. Après la panne initiale de provisionnement, les [trois jobs complets C12g](https://github.com/Memorithm/SML-GENIUS/actions/runs/38292779849) et les [trois jobs complets C12h](https://github.com/Memorithm/SML-GENIUS/actions/runs/38292873509) ont conclu **SUCCESS** avec `fmt`, `check`, Clippy et tests Rust. Le [rapport scientifique sourcé](docs/RESULTS-A6-C12GH-NOISE-PHASE-QUALIFIED.md) donne les six artefacts, SHA256, les 1232+1872 FROZEN et TEST exacts, les 334 courbes, les intervalles Wilson et les résultats négatifs. Les 3 réplicas réexécutent les **mêmes graines**, pas 3 nouvelles populations.

**Résultat distinctif :** Walsh-3 récupère la parité XOR à 20 % de bruit d'étiquettes avec 16/16 succès dès **256 bits proxy**, à 30 % avec 16/16 à **2048 bits**, et à 40 % avec 14/16 à **16384 bits** ; l'hybride W2+MITM n'atteint jamais 14/16 ou 7/8 sur ces trois scénarios dans la grille. Pour AND→XOR→XOR à 40 %, les deux méthodes restent sous le seuil préenregistré à 32768 bits (8/16 Walsh-3 contre 11/16 hybride). Ce résultat **ne démontre pas** de récupération de fonctions arbitraires ni une borne théorique. Le seuil heuristique `2ln(2M)/a²` n'est pas une preuve et les 8/16 graines ont une forte incertitude.

**Barrière modèle réelle non franchie :** A6-E1 conserve son verdict **STOP** face au ngram4 sur corpus naturel. Ne pas promouvoir SML-2/25B, une intuition générale, une banque 1M ou un GPU speedup en s'appuyant sur C12g/h.
