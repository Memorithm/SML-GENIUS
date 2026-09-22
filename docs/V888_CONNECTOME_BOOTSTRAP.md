# BANC V888 structural-prior bootstrap for SML-GENIUS

Status: active research bootstrap. This track does not alter the non-attention candidate contract and does not assert a model-quality advantage.

## Source rule

SML-GENIUS will use only FlyWire BANC v888 for connectome-derived research. FAFB v783 and every other connectome are excluded from this programme unless a later explicit protocol revision authorizes them.

Codex currently identifies BANC v888 as Female Adult Fly Brain and Nerve Cord, snapshot 2026-05-20, with 158,262 neurons and 3,037,361 aggregated connections. The external dataset is not vendored into this repository.

Sources:
- https://codex.flywire.ai/?dataset=banc
- https://codex.flywire.ai/faq

All ingestion is delegated to a versioned SciRust contract once available. Until then, only tiny synthetic fixtures may be committed.

## Research hypothesis

The SML architecture already seeks large cold learned capacity, bounded recurrent state, bounded associative memory, Boolean/low-bit routing and low active compute per token.

The V888 hypothesis is narrower:

Can structural statistics extracted from a biological sparse recurrent graph improve SML routing, state evolution, robustness or sample efficiency compared with topology controls at the same edge/parameter/active-compute budget?

The target is not to reproduce a fly brain and not to make V888 a runtime dependency. The desired product is a learned or synthesized SML topology that can survive without the external dataset.

## Track namespace

Use CSP for Connectomic Structural Prior to avoid collision with existing SML and SBG milestones.

### CSP-0.0 — protocol and legal/provenance gate

Freeze:
- V888-only source rule;
- exact external artifact identity and checksum;
- data fields actually available;
- no inferred neurotransmitter/sign labels;
- no dataset redistribution in the repository;
- matched-control policy;
- resource-accounting schema.

Exit only when source provenance is machine-readable.

### CSP-0.1 — topology fingerprint

Consume SciRust descriptors and produce a versioned topology fingerprint:
- node and connection counts;
- in/out degree histograms;
- reciprocity;
- SCC structure;
- modularity/community summaries;
- motif panel;
- hub statistics;
- declared annotation distributions.

The fingerprint is descriptive evidence, not a model.

### CSP-0.2 — synthetic topology compiler

Generate SML-sized graphs from a frozen descriptor subset.

Required arms:
- random sparse;
- edge-count matched;
- in/out-degree matched;
- degree + reciprocity matched;
- degree + reciprocity + modularity matched;
- V888-derived structural prior.

Every arm must use the same node count and explicit edge budget for a given comparison.

### CSP-0.3 — low-bit recurrent graph core

Bind sparse directed topology to SML recurrent state transitions:
- hard active-set routing;
- bounded fan-in/fan-out;
- Boolean/ternary/low-bit edge policies;
- exact active-edge accounting;
- bounded recurrent depth;
- no all-history token scan;
- no hidden Q/K/V attention.

Start with deterministic hand-authored transitions before training.

### CSP-0.4 — hub and module routing

Test whether explicit module/hub organization helps:
- local recurrent modules;
- sparse inter-module routes;
- hub admission/broadcast;
- controlled removal of hubs;
- degree-matched hub controls.

Do not encode biological names into the model semantics unless the experiment explicitly tests them.

### CSP-0.5 — event-driven active computation

Replace full-graph recurrent sweeps with active-front propagation where valid.

Measure separately:
- active nodes;
- active edges;
- event count;
- state touches;
- atlas page touches;
- RAM traffic proxy;
- recurrent cycles per emitted token.

A lower event count is not a quality result.

### CSP-0.6 — bounded associative memory coupling

Couple the graph state to existing SML bounded memory:
- graph state may write/read only through declared memory contracts;
- collisions and replacement remain explicit;
- memory budget is matched across controls;
- test whether graph modules specialize retrieval or working-memory roles.

### CSP-0.7 — SBG/MOR integration

Use Semantic Boolean Geometry and Mixture-of-Routers without creating a second semantic stack:
- SBG routing field may seed active graph modules;
- graph state may update V/M/A/R through explicit operations;
- MOR and graph routing costs are accounted separately;
- compare MOR-only, graph-only and hybrid routing.

### CSP-0.8 — trainable topology and edge state

Introduce learning only after reference semantics are stable.

Separate:
- fixed V888-derived topology;
- fixed matched controls;
- trainable edge weights on fixed topology;
- trainable sparse rewiring under fixed edge budget;
- Forge-selected discrete topology.

A training procedure may not silently densify the graph.

### CSP-0.9 — robustness and lesion panel

Required perturbations:
- random node deletion;
- random edge deletion;
- targeted high-degree node deletion;
- targeted inter-module edge deletion;
- input corruption;
- state-bit corruption.

Compare degradation and recovery against topology-matched controls. Report whether robustness comes from sparsity, degree structure or a more specific prior.

### CSP-0.10 — language and algorithmic development tasks

Before natural-language scaling, run:
- associative recall;
- copy/overwrite;
- variable-delay retrieval;
- compositional Boolean tasks;
- long-horizon state retention;
- routing under distractors;
- controlled sequence prediction.

Use current SML holdout discipline and do not touch protected final surfaces.

### CSP-0.11 — SML-2 admission gate

CSP may enter SML-2 natural-language experiments only if:
- at least one V888-derived arm beats all declared structural controls on a preregistered task or resource/quality Pareto criterion;
- the result replicates across seeds;
- no advantage disappears under degree/reciprocity/module matching;
- active compute and memory traffic remain within SML accounting rules;
- topology can be serialized without carrying raw V888 data into the model artifact.

If the evidence is null, retain the negative result and stop promotion.

## Long-range opportunity for SML-GENIUS

A successful track would yield a model family with:
- sparse graph recurrence instead of token all-pairs attention;
- direct Boolean/low-bit module admission;
- bounded associative memory;
- adaptive recurrent depth;
- event-driven active compute;
- cold parameter atlas pages activated by graph/SBG routing.

This is compatible with the existing SML objective of separating total learned capacity from per-token active compute.

## Dependencies

- SciRust: graph, sparse, event and spiking/reference primitives.
- TDI: falsifiable matched-budget evaluation.
- Forge: discrete topology search after the domain contract is frozen.
- ElasticXxx: runtime resource policy only after static baselines exist.
- NNIS: portable runtime implementation.
- FLAT-ATTENTION: external attention and hybrid baselines, not a hidden dependency of the SML candidate.

## Stop conditions

Stop or redesign the track if:
- V888-derived topology loses its advantage against degree/reciprocity/module-matched controls;
- trainable rewiring converges to structures unrelated to the prior;
- event scheduling costs exceed sparse propagation savings;
- topology metadata negates low-bit storage gains;
- model quality requires dense all-pairs scoring;
- results cannot be reproduced without direct access to raw V888 data at inference time.
