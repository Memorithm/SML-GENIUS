# SML-GENIUS bootstrap

This file is the durable re-entry point for continuing the SML-GENIUS research programme from the current repository state. Read it together with `README.md`, `docs/ROADMAP.md`, the latest merged pull requests, and the executable contracts in `sml-core`.

## Current architectural direction

SML is an independent, predominantly Boolean / low-bit autoregressive architecture. It is not defined by reference to Transformer internals. Its native mechanisms include direct routing, Mixture-of-Routers / sparse expert selection, bounded associative memory, recurrent depth, Elastic Weights, and Boolean-integral learning experiments.

Native decision capability is a first-class parallel objective. A substantial decision-expert family and dedicated training objectives must be evaluated before SML-2, not bolted onto a finished language model. Direct decisions and bounded recurrent refinement must work without a text decoder; language generation is optional. See `docs/SML-D-BOOTSTRAP.md`. Total decision capacity, active work per decision and direct-decision coverage are separate quantities; no arbitrary 25B percentage is frozen.

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

DKV-3a context integration is preregistered in `docs/SML-DKV-3A-CONTEXT-PREREGISTRATION.md` but execution is intentionally blocked on CONTEXT-0/1. Delta-KV reads are derived semantic evidence, never authoritative source provenance; exact/source-required tasks must independently verify immutable raw context segments.

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
