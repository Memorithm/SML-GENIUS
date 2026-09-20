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
- [ ] trainability experiments;
- [ ] frozen SML-0 exit report.

## Immediate implementation sequence

1. Implement the first trainability path with continuous latent parameters and hard Boolean forward semantics.
2. Define a leak-safe Forge domain for discrete topology / ANF search.
3. Benchmark fixed recurrent depths against adaptive recurrent depth on the deterministic task panel.
4. Define the first page-residency planner contract for ElasticXxx.
5. Freeze the SML-0 exit report and SML-1 development protocol before larger training runs.
6. Promote reusable primitives to SciRust only after semantics stabilize.

## Stop conditions

Scaling stops if any of the following persists after a bounded research iteration:

- quality fails to improve with scale under a controlled training budget;
- active compute grows approximately linearly with total parameter count;
- recurrent depth adds cost without measurable task benefit;
- routing degenerates into an all-history scan or all-pairs scoring mechanism;
- metadata/index storage overwhelms the low-bit weight advantage;
- the target memory hierarchy cannot hide storage traffic sufficiently for the SML-4 throughput gate.
