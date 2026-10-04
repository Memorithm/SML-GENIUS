# BANC V888 structural-prior bootstrap for SML-GENIUS

Status: active research bootstrap. This track does not alter the non-attention candidate contract and does not assert a model-quality advantage.

## Source rule

SML-GENIUS will use only FlyWire BANC v888 for connectome-derived research. FAFB v783 and every other connectome are excluded from this programme unless a later explicit protocol revision authorizes them.

The external dataset is not vendored into this repository. SML source accounting follows the canonical ITD V888-BOOL strategy revision `2026-09-22.1` at commit `7f8f08e764dd5f36688533e9f7e7fb2f290f6e17`, not earlier Codex display totals.

At that pinned strategy revision, the audited reference scopes are explicitly distinct:

- 188,508 metadata nodes in the audited induced graph;
- 13,620,865 directed pairs in the audited induced graph;
- 42,309,621 contacts in the audited induced graph;
- 198,816,365 records in the retained raw table;
- 19,733,123,829 measured bytes for that raw table.

These numbers are coordination references, not a substitute for the exact artifact selected by an experiment. A handoff must bind its selected source-scope ID and SHA-256. Node-total reconciliation does not prove pairwise equality; directed pairs, contacts and raw records must never be collapsed into one generic edge count.

Sources and coordination:
- canonical ITD programme: `Memorithm/itd-simulator#58`;
- executable SciRust graph gate: `Memorithm/scirust#1520`;
- V888-GROWTH/source parent: `Memorithm/scirust#1500`;
- SML coordination contract: [`V888_BOOL_COORDINATION.md`](V888_BOOL_COORDINATION.md).

All ingestion is delegated to qualified, versioned SciRust contracts. Until the executable graph gate is satisfied, only tiny synthetic fixtures may be used for SML graph execution.

## Research hypothesis

The SML architecture already seeks large cold learned capacity, bounded recurrent state, bounded associative memory, Boolean/low-bit routing and low active compute per token.

The V888 hypothesis is narrower:

Can structural statistics extracted from a biological sparse recurrent graph improve SML routing, state evolution, robustness or sample efficiency compared with topology controls at the same edge/parameter/active-compute budget?

The target is not to reproduce a fly brain and not to make V888 a runtime dependency. The desired product is a learned or synthesized SML topology that can survive without the external dataset.

## V888-BOOL coordination

The first functional target is delayed XOR, `y[t] = a[t-d] XOR b[t]`. Required controls are exact sequential logic/shift register, no-reservoir readout and topology-matched graphs. Fixed recurrence and a bounded readout precede recurrent learning or topology search. Simplified LIF, bounded integer-event and genuinely Boolean recurrent dynamics are separate rails; binary spikes alone are not Boolean-only internal arithmetic.

The future consumer handoff is exactly `V888-BOOL-4.0`. It requires exact Rust interface/source identities, graph-only / existing-router-only / hybrid ablations, bounded memory and readout, no hidden input-history bypass, no raw V888 inference dependency and independent SML quality/resource validation. Existing GF(2)/ANF, bounded-memory and routing contracts must be reused rather than reimplemented as another Boolean stack.

These rules are executable in `sml_core::v888_handoff`. The full cross-project contract and current blockers are in [`V888_BOOL_COORDINATION.md`](V888_BOOL_COORDINATION.md).

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

- SciRust: graph, sparse, event and spiking/reference primitives; executable graph qualification is tracked by `Memorithm/scirust#1520`, downstream of the V888-GROWTH/source umbrella #1500.
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
