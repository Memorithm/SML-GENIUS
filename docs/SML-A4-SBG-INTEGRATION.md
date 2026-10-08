# A4 — explicit SBG/ANF destination integration

Date: 2026-10-08. Status: staged functional integration, not model promotion.

PR237's `pvp6_recurrent::execute` remains a historical sequence-of-evaluations
reference: its output state is not fed into subsequent input queries. It must not
be described as a closed recurrent state/routing/memory loop.

The new path uses the existing `SemanticState` and `SemanticRewriter` contracts,
without adding a second halting engine or importing an external runtime.

## Wiring contract

`SbgAnfPlan<S,D>` projects explicit coordinates of V/M/A/R into log2(D)-bit ANF
addresses. It stores one source lane per query and query-major destinations
(lane * gate_count + gate). Every result has a unique destination; old-state
reads precede all writes. Unwritten coordinates and padding remain unchanged.
Invalid domains, ranges, duplicate inputs within a lane, duplicate destinations,
empty or inconsistent shapes and foreign batch shapes are rejected.

The present monomial carrier is u16; this contract accepts D=64..65536 powers
of two only. It does not claim qualification at D=1,048,576. Repeated queries
from identical projected addresses are valid. The plan is declared wiring,
not a learned feature extractor or a claim of semantic benefit.

## Ten-slice sequence

1. Checked four-field input/output projection.
2. State-fed ANF kernel implementing the native SBG rewriter and halting rules.
3. Atomic coefficient replacement with revision-bound cycle provenance.
4. Transactional composition of native semantic rewriters.
5. Explicit operation/trace budgets and no-mutation rejection.
6. Integration with the existing SBG-to-MOR engine.
7. Native Delta-KV/ANF/routing trajectory comparison against a direct control.
8. Bounded versioned four-field trace exchange and negative parser controls.
9. Model-owned checkpoint/continuation with identity and depth preservation.
10. Deterministic full qualification panel, replay evidence and integration report.

Each slice is independently tested and reviewed through a PR. Existing CI gates,
fixture identities and negative results are retained. New measurements, learning,
matched architectural ablations and capacity scaling remain separate gates.

## Ownership and reuse

SML owns the declared state-update semantics. A generic composition or trace
contract can serve other native SML mechanisms and external qualification
consumers, but does not force SciRust, FLAT-ATTENTION or NNIS runtime dependencies.
RemoteOps remains the reference physical control plane. No GPU, vendor SDK,
resident model service or separate SML-HARNESS is required for these CPU tests.
