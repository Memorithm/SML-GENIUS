# SML-D1d: MOR-gated sparse decision expert bank

Implementation: `crates/sml-decision/src/mor_bank.rs`.
Tests: `crates/sml-decision/tests/mor_bank.rs`.

## Purpose

D1d connects the existing native SBG -> MOR router to the native decision engine.
It does not create a second router.

Each input is routed first. This slice requires exactly one selected expert page
per record. Only after every route in the batch is validated are records grouped
by selected page and passed to the corresponding `SemanticDecisionProgram`.

The result preserves original input order and each selected expert's declared
head order.

## Fail-closed boundaries

- `SemanticMorTopK::One` is required by the bank.
- The MOR result must contain exactly one selected expert for every record.
- Every registry expert page must have exactly one decision program.
- All expert programs must share schema ID and projection version.
- The routing observation mask must cover the complete R field.
- Every route is established before the first decision expert executes.
- Every selected expert batch is admission-planned before the first expert batch
  executes.
- Global limits cover records, number of expert batches, total result elements
  and the conservative sum of per-batch working payload bounds.

The full-R observation requirement is intentionally conservative. D1d does not
pretend that unobserved routing bits are false. A later masked-routing experiment
may relax this only with an explicit similarity definition and matched controls.

## Batching

Routing is still performed once per independent record in this slice. After
routing, records selecting the same page are grouped and retain the D0 bit-sliced
decision path.

This means an expert selected for 128 records can execute two 64-lane word-gate
groups rather than 128 one-record gate calls. This is work accounting, not a
wall-clock speedup claim.

Unselected decision programs are not executed.

## Route history and refinement

D1d uses a fresh `PreviousRoute::default()` for every independent direct
decision. It therefore qualifies direct sparse decisions only. Recurrent route
history, hysteresis across related observations and D4 refinement are separate
experiments.

Runtime hints are accepted through the existing MOR interface and retain native
MOR validation.

## Scientific boundary

D1d demonstrates sparse expert scheduling over controlled SBG states. It does
not establish:

- learned MOR signatures;
- calibrated confidence;
- general-language question understanding;
- Elastic Weight materialization;
- mapped-store-to-MOR end-to-end latency;
- a throughput advantage over dense or external decision systems.

The next integration should compare sparse MOR-gated decision execution against a
matched dense-expert control and then connect selected pages to native Elastic
Weight materialization.
