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
