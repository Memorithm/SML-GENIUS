# SBG-REA-2 — native model trace export for paired edge-policy replay

Status: model-owned research-only collector, **stacked after SBG-REA-1**.
Not an SML default-routing, quality, performance or scale promotion.

## Delivered source

`crates/sml-core/src/semantic/rae_native_trace.rs` captures native
`SemanticMorBridge::route` decisions on a cycle-by-cycle sequence of
actual `SemanticState` instances using the SBG-REA-1 observer. Unlike a
synthetic page truth table, the required-page mask is composed directly
from `MorDecision::selected_experts`. The model-owned observer keeps
the original page identity rather than reindexing a ranked candidate.

Every retained row records:

- monotonic cycle ID;
- packed, exact SBG R-bitplane (original feature coordinates);
- packed rising and falling feature masks, preserving event coordinate identities
  for a possible TRAIN-only event-to-page learning experiment;
- packed native MOR-selected expert/page mask;
- exact selected `PageId` values;
- number of rising, falling and active routing bits;
- a context-invalidation flag, including changes in V/M/A, supplied
  runtime hints or previous-route inputs, and any additional explicit
  invalidation supplied by the caller.

The recorded selected expert mask is the **native MOR reference**:
it does not establish which softmax K/V keys would matter in FLAT
and does not prove the model's output is correct. The collector calls
original native MOR on every cycle (even when there is no rising front)
and cannot elide any computation. It retains evidence outside the
inference critical path and uses only the SML crate (no FLAT dependency).

## Fail-closed scope

- Store at most 10,000 frames and require 1..=65,536 registered experts.
- Refuse changed expert capacity or invalid cycle sequence.
- Detect out-of-range or duplicate selected expert IDs.
- Construct each row against a cloned observer and commit only after
  native-route evaluation and page validation succeed.
- Caller remains responsible for marking other relevant changes in
  model/expert semantics, signature/registry updates, memory generation,
  request identity or engine policy. Not all of these are observable from
  the SBG state itself.
- Physical RAM traffic, model latency, quality, and per-tick speedup
  are not inferred from raw front/page counts.

## Next cross-project experiment

Export a frozen model/trace sample with explicit provenance and frozen
feature-to-page mapping into a neutral data container. Replay those
same R-field snapshots, required native MOR page sets and invalidation
flags through FLAT-ATTENTION's research-only
`rae_policy_audit::audit_paired_page_policies`.

This is a **comparison of page-selection policies against native MOR**,
not yet a comparison of numerical attention against dense FLAT.

Require no missed native MOR pages before considering work-elision.
Require further independent downstream model-quality and physical
hardware gates, and always account for detection overhead and the
cost of feature-to-page metadata. Any production implementation
should retain non-edge-triggered recurrent and memory invalidation
paths and support a reliable native fallback.

## Local commands

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p sml-core semantic::rae_native_trace
cargo test --locked --workspace
```

Exact-head CI must be fully green before merger. A CI-green repository
does not establish scientific novelty, hardware improvement, or quality.

Related: `docs/SBG-REA-1-RISING-FRONT-MOR.md`;
FLAT research `docs/research/RAE_3_MATCHED_POLICY_REPLAY.md`;
SML `README.md`, `AGENTS.md`, `BOOTSTRAP.md` and `docs/ROADMAP.md`.
