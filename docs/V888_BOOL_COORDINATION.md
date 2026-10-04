# CSP × V888-BOOL coordination contract

Status: coordination contract for SML-GENIUS issue #59. This document does not qualify a V888-derived model or authorize a protected/final evaluation.

## Canonical strategy identity

The SML CSP track is coordinated with the ITD V888-BOOL programme at:

- strategy revision: `2026-09-22.1`;
- ITD strategy commit: `7f8f08e764dd5f36688533e9f7e7fb2f290f6e17`;
- ITD tracking issue: `Memorithm/itd-simulator#58`;
- SML tracking issue: `Memorithm/SML-GENIUS#59`.

The canonical ITD inputs are:

- `.agent/ITD_RESEARCH_BOOTSTRAP.md`;
- `.agent/V888_BOOL_PROGRAMME.json`;
- `.agent/V888_BOOL_RESEARCH_PROGRAMME.md`.

SML retains its own milestone numbering and scientific authority. This coordination does not renumber SML, SBG or CSP milestones and does not change existing SML quality gates.

## Source-count discipline

The canonical ITD strategy distinguishes multiple audited scopes. At the pinned strategy revision it records:

| audited scope | count |
| --- | ---: |
| metadata nodes in the audited induced graph | 188,508 |
| directed pairs in the audited induced graph | 13,620,865 |
| contacts in the audited induced graph | 42,309,621 |
| records in the retained raw table | 198,816,365 |
| retained raw table measured bytes | 19,733,123,829 |

These values are not interchangeable. In particular:

- metadata-node reconciliation does not establish pairwise equality;
- directed pairs are not contacts;
- contacts are not a conductance measurement;
- raw-record counts are not graph-edge counts;
- an upstream display total is not a substitute for a selected audited artifact scope.

A future SML consumer run must bind the exact selected source scope, artifact SHA-256 and Rust producer/source identity. The table above is a coordination reference only; it does not itself admit a handoff.

The previously copied Codex display counts in `V888_CONNECTOME_BOOTSTRAP.md` are retired from SML evidence accounting.

## SciRust executable-graph prerequisite

`Memorithm/scirust#1500` is the V888-GROWTH umbrella/source programme. Its issue body does not itself define the executable graph qualification that SML #59 requires.

The explicit executable-graph prerequisite is therefore `Memorithm/scirust#1520`, created as a focused downstream gate of #1500. It must retain exact source identities, distinguish directed-pair/contact semantics, validate endpoint and subset rules, retain discrepancies, expose deterministic Rust serialization/checksums and provide the active-node/edge/event accounting needed by SML controls.

Until that gate is qualified, a V888-BOOL-4.0 SML handoff remains blocked.

## First functional campaign: delayed XOR

The first functional target is:

`y[t] = a[t-d] XOR b[t]`

Before execution, freeze the delay grid, seeds, port placement, distractors, resets, encoding and readout budget. The encoder may not compute the label or supply delayed input history to the readout.

Required controls are:

1. exact sequential logic / shift-register control;
2. no-reservoir readout control;
3. topology-matched graph control.

Fixed recurrence and a bounded readout precede recurrent learning or topology search. Cross-trial state leakage and hidden input-history buffers are failures, not implementation details.

Required dynamics rails remain separate:

1. simplified LIF numeric reference;
2. binary spikes with bounded integer-event accumulators/weights;
3. genuinely Boolean recurrent state with bounded nonlinear functions.

Binary spikes alone do not establish Boolean-only internal arithmetic.

The dependency-free Rust validator for these requirements is `sml_core::v888_handoff::DelayedXorProtocol`.

## CSP mapping

The V888-BOOL programme maps into the existing CSP namespace without renumbering:

- CSP-0.3: reference sparse recurrence semantics;
- CSP-0.5: active-front/event work accounting;
- CSP-0.6: existing bounded-memory coupling;
- CSP-0.7: existing SBG/MOR routing integration and graph-only/router-only/hybrid comparisons;
- CSP-0.8: adaptive wiring only after fixed-topology qualification;
- CSP-0.9: lesion and perturbation panels;
- CSP-0.10: delayed Boolean, memory, routing and other algorithmic development tasks;
- CSP-0.11: unchanged SML-2 admission gate.

CSP-0.11 remains controlling: neither source-data audit success nor synthetic task correctness is sufficient for SML model promotion.

## V888-BOOL-4.0 consumer handoff

The future SML consumer stage is exactly `V888-BOOL-4.0`.

The fail-closed Rust contract is `sml_core::v888_handoff::V888BoolHandoff`. Admission requires all of the following:

- exact strategy revision and strategy commit;
- exact Rust interface ID and Rust source commit;
- exact selected source-scope ID and artifact SHA-256;
- typed, non-duplicated source counts with at least metadata-node and directed-pair scopes;
- exact graph-artifact SHA-256;
- graph-only, existing-router-only and hybrid ablations;
- explicit bounded memory and bounded readout;
- no hidden input-history bypass;
- no raw V888 inference dependency;
- reuse of existing SML GF(2)/ANF, bounded-memory and routing contracts;
- completed SciRust executable-graph qualification;
- independent SML quality and resource validation;
- no protected-final access, model-promotion request or default runtime-policy change from this handoff.

The handoff contract deliberately does not encode a topology advantage. A null or negative topology result must remain admissible evidence.

## Current status

This coordination slice establishes contracts only.

- No V888-derived candidate is promoted.
- No large-model training is launched.
- No protected final evaluation is opened.
- No SML runtime default is changed.
- ITD issue #58 remains the research umbrella.
- SciRust #1520 remains the executable-graph prerequisite.
- The actual delayed-XOR campaign and V888-BOOL-4.0 qualification remain future evidence work.

Closing SML-GENIUS #59 after this contract is merged means the durable cross-project coordination is complete. It does not close ITD #58, SciRust #1500/#1520, the CSP research programme or the CSP-0.11 admission gate.
