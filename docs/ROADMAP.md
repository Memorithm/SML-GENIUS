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
- [ ] learn the remaining recall/copy control and other micro-operation semantics from task loss;
- [ ] compare learned routing against fixed-routing, no-memory and memory-capacity ablations;
- [ ] demonstrate held-out improvement attributable to learned routing/memory rather than fixed task semantics.

## Immediate implementation sequence

1. Extend task-loss training from conjunction and overwrite-state update into recall/copy control and remaining micro-operation semantics while preserving the same holdout boundary.
2. Run fixed-routing, no-memory and memory-capacity ablations before claiming a learned mechanism advantage.
3. Benchmark fixed recurrent depths against adaptive recurrent depth on the same panel.
4. Build physical elastic-weight page materialization plus VERIFY/ROLLBACK behind the qualified ElasticXxx SML plan adapter; the validation/projection boundary is complete, physical actuation is not.
5. Freeze the SML-0 exit report and SML-1 development protocol before larger training runs.
6. Evaluate learned wiring separately against fixed/Forge-selected topology, then promote stable reusable primitives to SciRust.

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
- [ ] SBG-0.13 mmap semantic store;
  - [ ] SBG-0.13a versioned checksummed mmap-friendly binary format;
  - [ ] SBG-0.13b read-only mapped runtime with bounded per-record decoding;
- [ ] SBG-0.14 integrated prototype.

The SBG track is an experimental hypothesis. It does not yet establish language quality, optimal semantic width, or 25B-scale trainability.

## CSP — BANC v888 Connectomic Structural Prior track

The V888-only structural-prior programme is defined in [`V888_CONNECTOME_BOOTSTRAP.md`](V888_CONNECTOME_BOOTSTRAP.md) and uses the independent `CSP-0.x` namespace. CSP is evidence-gated alongside SML/SBG: V888-derived topology receives no promotion privilege and must outperform the strongest declared edge-count/degree/reciprocity/modularity controls before entering larger SML experiments. Raw V888 data is never an inference-time dependency, and new execution work is Rust-first on CPU/WGPU portable paths.
