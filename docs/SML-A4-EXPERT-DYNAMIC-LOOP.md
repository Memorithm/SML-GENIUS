# A4 selected expert execution and dynamic state/route memory

Date: 2026-10-09. Ten-slice functional integration after PR247.
Status: implementation and tests, not a timing or learned-utility experiment.

## Frozen scope before integrated qualification

The intended cycle is input SBG state -> existing inner ANF rewrite -> native
MOR selection -> selected ANF expert page bodies -> Delta-KV key reprojected
from the post-expert state and current MOR decision -> memory read/state update.
The output is the input of the next native SBG cycle. Halting remains owned by
`run_semantic_recurrent`; R=1 query reuse is not recurrent depth.

Ten review slices:
1. Read-only typed ANF page body and literal-oracle checks.
2. Sparse page lookup and simultaneous, conflict-checked selected-page writes.
3. Transactional execution after native routing, with explicit work bounds.
4. Dynamic Delta-KV using an explicitly supplied current MOR decision, leaving
   the existing DKV-2c frozen-route entry point unchanged.
5. Coupled routed-expert/memory loop and complete handoff evidence.
6. Memory-work/trace admission and rollback accounting.
7. Version-checked replacement of expert coefficients.
8. Owned in-memory continuation for the full qualified pipeline.
9. Direct-control integrated trajectory and failure-injection matrix.
10. Retained deterministic full-output report and mandatory CI replay.

ANF expert pages are a new declared page encoding bound to existing `PageId`.
They are not a reinterpretation of `PackedBooleanPage` or a trained page export.
All pages use explicit SBG wiring and the model-owned scalar executor. Inactive
pages must not execute; selected pages read the same pre-expert state, with
ambiguous overlapping destinations rejected rather than resolved by order.

## Evidence and operational limits

Every test has exact equality/error acceptance, not a post-hoc quality margin.
No physical speed, learned utility, attention replacement or complete A4/25B
architecture claim is authorized. Keys are compressed and may collide: a changed
state or route does not necessarily produce a different key. Test evidence must
retain this distinction and raw provenance separately from key contents.

Budgets are logical counters, not measured RSS/VRAM/time. Returned-error rollback
covers owned mutable state, not external effects or aliased handles. In-memory
checkpointing is not persistent storage. Current ANF monomial masks remain u16;
no million-address admission is inferred. Historical fixtures and negative PGF2
mixing results remain unchanged. CI must be green on each exact PR head before
merge. RemoteOps remains the physical control plane; Rust-native model execution
has no mandatory partner runtime or vendor SDK dependency.

## Implemented interfaces and evidence boundaries

`SbgAnfExpert` executes a declared PageId-bound ANF body. `SbgExpertAtlas` uses a
sorted sparse page vector rather than allocating through the maximum ID. Every
selected page reads the same incoming SBG snapshot; disjoint writes are combined,
and overlapping destinations are rejected before any selected body executes.
This is not weighted expert blending or the PackedBooleanPage learned encoding.

`RoutedExpertSemantic` wraps the existing native MOR path and executes its chosen
pages. Full page outputs, revisions, queries and logical work are retained. The
resulting state goes into `CoupledSbgLoop`, which invokes the opt-in
`DeltaKvDynamicRekeyKernel::rewrite_with_mor` on that post-expert state and the
current route. The resulting memory state is the input of the next cycle.
The existing DKV-2c frozen-route entry point is unchanged when used alone.

Coefficient replacement preserves page ID/wiring, validates a whole candidate
bank and checks the expected local revision. Identical banks are no-ops. Stale,
invalid or overflowing updates cannot mutate the page or earlier cycle evidence.
Structural acceptance of new coefficients is not training/holdout approval.

The expert and memory budgets are checked before their respective work executes.
Memory read-MAC and projected-coordinate observations must equal planned counts.
The memory retained-unit charge is 8*ceil(S/64)+key_dim+value_dim per cycle:
one unit per retained state word or key/read scalar, not a byte count. Route and
provenance metadata, other stages' logs, temporary clones and allocation overhead
are outside this counter. These are not physical RAM/VRAM/time measurements.

`semantic::session::SemanticSession` generalizes in-memory continuation to an
owned native semantic pipeline, including routing history, expert coefficients,
memory keys/counters, state and the original halting policy. It reuses the native
runner with absolute-depth translation. Chunk boundaries do not override the
original minimum depth. Failed chunks roll back all their owned state; finished
sessions add no work. The optional equality check compares the complete expected
kernel, not a cryptographic digest. This is not serialization or disk persistence.
The generic session also works without PVP; it adds no partner-runtime dependency.

## Full-loop declared test panel

`tests/support/a4_closed_loop.rs` defines S=8, ANF domain=64 and key_dim=8.
Four initial seeds, three explicitly seeded memory targets, depth limits 1/2/4/8
and two expert-update schedules yield 96 fixed-depth cases. Twelve additional
cases use native adaptive halting. The active ANF rewrites R from V1; native MOR
chooses between two page bodies; the body updates V0; memory then updates V1/A1.
The scheduled case replaces page 0 coefficients at depth 3. Four pre-inference
Delta-KV seeding writes per case are declared setup, not online model learning.

Observed complete report: 108 cases, 456 cycles, 158 key changes and 28 route
changes, with zero exact mismatches. Every case has a full-pipeline continuation
comparison. All 12 adaptive cases reach their depth cap: this panel establishes
no advantage from adaptive stopping. Changes to state/route need not change a
compressed key; counts describe this corpus only, not collision-free identity.

The public-API control compares every incoming/post-expert/final state, selected
page/revision, inner/expert query and output, full native route/previous context,
key/read evidence, raw provenance, memory/projection counters, deltas and halting.
Literal ANF evaluation is independently coded; this first control shares the
qualified native MOR/projector/memory primitives. It separately verifies actual
owned-pipeline checkpoint continuation, including scheduled coefficient updates.

The independent `scripts/verify-a4-closed-loop.rs` links only std. It reconstructs
all 432 setup writes, frozen matrices, key hashes/quantization, signed Q12 read
rounding, ANF page outputs and full states without calling sml-core. The routing
control is intentionally fixture-specific: top-k=1 on all-zero/all-one R gives
one exact candidate; the native ranker only ranks proposed pages, so hysteresis
cannot introduce a missing candidate. This is not an independent general MOR
implementation. Independent arithmetic replay is not itself a checkpoint test;
that behavior is executed by the public-API suite. Four verifier tests reject
changed state/key/read/page/revision/matrix, metadata and missing/reordered/extra
records. Complete bytes are compared, not merely a success count.

Golden trajectory: `research/evidence/a4-closed-loop-20261009/trajectory.csv`.
675 lines; 95,452 bytes; SHA-256:
`7ac9f94f1f98680e6d3789f5840433917d5bbeba0cd6dd8a8e0b61c9f5bcf0dc`.
The existing mandatory fixture script runs the report twice, compares both full
reports to the frozen golden bytes/hash and runs the independent verifier/tests.
No existing fixture, historical failed result, CI workflow or security boundary
is relaxed. Native/local and CI/run identities must be read from their receipts.

## Remaining model gates

Expert bodies and dynamic state/route memory now execute in the declared loop;
full-pipeline owned continuation is tested. Broader SBG/domain/route geometries,
multiple selected experts under realistic wiring, quality-gated learned page
exports, persistent authenticated full-pipeline checkpoints, physically admitted
end-to-end costs, learning and matched ablations remain open. The present corpus
is not a 25B model, a speed result, a learning result or a final architecture freeze.

## Cost caveat of the current transactional reference

Owned cloning currently copies the complete enclosing kernel, including inactive
expert banks and retained histories. Inactive page bodies are not evaluated, but
this does NOT establish sparse physical traffic or low-cost rollback. Avoiding
whole-atlas/history copies requires a separate immutable-sharing or transactional
journal design with renewed isolation tests and physical measurement. The logical
selected-work counters intentionally do not conceal this unmeasured clone cost.

## A4 next performance-isolation slice — shared immutable atlas

The expert atlas now uses Rust `Arc<Vec<SbgAnfExpert>>` for immutable bank sharing
across cloned transactional candidates. `replace_bank` uses copy-on-write so
coefficient changes in one candidate do not mutate the original or sibling
snapshots. An exact pointer-identity and coefficient-change test covers sharing,
separation, old-result preservation and rejected replacement. This is an
implementation-level reduction of bank-clone work, not an observed latency or
RSS/VRAM improvement. Other owned history/counter clones remain and must be
measured or redesigned before a physical sparse-traffic claim.
