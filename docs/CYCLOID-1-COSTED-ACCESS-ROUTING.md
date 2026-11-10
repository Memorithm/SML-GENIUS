# CYCLOID-1 — bounded costed access paths for native MOR-selected pages

Status: executable research-only reference. No end-to-end runtime adapter, model-quality improvement, learned curvature, physical latency result, or default routing change.

## Why this is not Transformer attention

The SML candidate remains strictly non-attention. Cycloid-1 is an optional
control-plane planner for access to an expert page **already selected by MOR**.
It never performs softmax, Q/K/V scoring, history ranking, or expert reselection.

A path in the planner is an access-translation path, not a sequence of neural
circuits that execute intermediate experts. Only the final page is semantic.
Entry and relay nodes are control metadata.

The design tests a limited form of the brachistochrone analogy: a path with more
transitions can have a smaller *supplied estimated latency*. Neither Riemannian
curvature nor an information-propagation speed field has been established for SML.
Unlike a geodesic solver, the implementation enumerates simple directed paths
over an explicit bounded graph.

## Implemented code

- [Rust module](../crates/sml-core/src/cycloid_route.rs)
- [Current native MOR](../crates/sml-core/src/mor.rs)
- [Existing regret/cost training objective](../crates/sml-core/src/mor_route_objective.rs)

The existing MOR task-loss / compute / I/O objective already handles cost-aware
expert training. Cycloid-1 does **not** replace or duplicate that objective:
it addresses a different question, whether an alternative access path to the
*same selected page* avoids a costlier direct memory transition.

The caller builds a frozen, explicitly profiled graph:

- node zero: one entry point;
- any number of relay nodes;
- one unique terminal per page, with no outgoing edges;
- directed edges with strictly positive estimated nanoseconds and estimated
  read bytes, supplied by the caller;
- an explicit, mandatory direct edge from entry to the terminal page.

The search uses exhaustive bounded depth-first traversal of **simple** paths,
prunes paths that have already reached the direct latency lower bound,
and enforces a read budget. Every considered edge is counted. Any exceeding of
the exploration limit fails with SearchBudgetExceeded rather than returning a
possibly non-optimal partial path. The chosen indirect path must be strictly
faster in the declared cost model *after* adding the declared online dispatch
overhead. Otherwise the direct edge wins. Equal-cost indirect paths are ordered
by lower bytes and then lexicographic node sequence.

The graph is bounded to 64 nodes, 256 edges and 8 hops; the caller must also
set a maximum number of examined edges (up to 20,000). Cost additions are
checked to reject overflow. The current planner runs offline and is not
intended for per-token execution.

The endpoint is gated against MorDecision.selected_experts. This is an
*eligibility* check, not proof of source integrity or of real hardware access.
No generated plan is executed or substituted into a live model by this slice.

## Correctness and negative controls

Unit tests cover:

1. An indirect 2-hop path that beats a 1-hop direct path on total supplied time.
2. Strictly retaining the direct baseline on a tie or dispatch-overhead loss.
3. Rejecting a faster alternate when its estimated read bytes exceed the budget.
4. Refusing any terminal that MOR has not already selected.
5. Invalid terminal/graph topology, duplicate page identity and missing baseline.
6. Exhaustion of search budget returning an error, not a partial answer.
7. Deterministic tie breaks in a cyclic relay graph.

Run the exact repository gates from a checkout of the PR head:

    cargo fmt --all -- --check
    cargo clippy --locked --workspace --all-targets -- -D warnings
    cargo test --locked --workspace

GitHub Actions must be fully green on that exact head before merge.

## Required follow-on before any deployment

**CYCLOID-2 — integrate measured path costs.** Freeze the exact page/cache
layout, host identity, parameter-page residency, and definitions of transition
time and memory traffic. Provide a trusted, versioned cost profile measured
through Memorithm/RemoteOps, using identical payloads and no privileged arbitrary
code from an unreviewed PR. Report profile capture and planning overheads
separately from the online dispatch estimate.

**CYCLOID-3 — executable runtime adapter.** Compare actual direct and indirect
accesses to the *same bytes of the same MOR-selected page*. Verify content hashes,
destination identity, error behavior and boundary conditions before timing.
A planner-only result cannot demonstrate any memory-I/O reduction. The runtime
must avoid an extra graph search on every token; a compact frozen dispatch table
must be separately qualified.

**CYCLOID-4 — held-out effectiveness.** Apply the exact same task cases and
task-quality controls to direct and costed access. Measure training/evaluation
leakage boundaries, end-to-end p50/p95 latency, resident RAM, bytes actually read,
page faults, recurrent cycles and selected pages on named CPU/portable GPU
hardware. Retain the simpler direct route if no qualified benefit appears.

FLAT-ATTENTION's Boolean candidate admission and KV page selection are separate
research consumers of the **idea**, not a shared SML runtime dependency.
