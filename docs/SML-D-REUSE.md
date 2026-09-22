# SML-D reuse assessment — 2026-09-22

The decision crate directly reuses native SML `SignalRef` and `TruthTable2`.
It does not create a competing truth-table convention or depend on another
Memorithm repository at inference time.

Inspected destination research contract:
`Memorithm/Forge`, commit `e657e3629dcdc7d69e972851db1044e52c7b575f`,
`docs/SML_TOPOLOGY_DOMAIN_V1.md` and the code-search entry for
`forge-domains/src/sml_topology.rs`.

That contract uses two-input four-bit gate tables and acyclic signal references,
but has one output and a truth-table oracle of at most eight inputs. Unreachable
gates are excluded from its measured active costs. D0 currently evaluates every
declared gate and supports multiple typed heads over up to 64 supplied features.
Those work metrics must not be equated without an explicit adapter and test.

Next reusable implementation: convert a requalified Forge single-output topology
into a native decision head, then test all input rows with scalar and bit-sliced
execution. Keep Forge development/final-holdout partitions intact, distinguish
hand-authored and learned tables, and add reachable-gate accounting before any
matched-cost claim. Multi-head search needs a separately versioned contract; do
not silently extend `sml-topology/v1` or share protected test labels with search.

No Forge source was modified in this slice. Reusing a compatible representation
is immediate; changing its search/holdout contract is not justified by D0 tests.
No latency/quality benefit to Forge or SciRust is claimed without destination
qualification.
