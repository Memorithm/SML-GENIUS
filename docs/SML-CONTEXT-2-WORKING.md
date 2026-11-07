# SML-CONTEXT-2 — bounded recurrent working-context substrate

**Status:** implemented (fixture/oracle reference). **Owner:** Grok (G2).  
**Code:** `crates/sml-store/src/working_context.rs`.  
**Tests:** `crates/sml-store/tests/working_context.rs`.

## Scope

CONTEXT-2 recurrently updates a **bounded working state** from an ordered
segment stream and compares three matched policies on identical synthetic
inputs:

| Policy | Behavior | Fate of displaced segments |
| --- | --- | --- |
| `Fixed { capacity }` | Constant-capacity ring | **Lost** (irreversible from working state) |
| `Expanding { hard_max }` | Retains every admission until the ceiling | Never displaces; ceiling overflow **fails closed** |
| `CapacityBounded { capacity }` | FIFO capacity | **Evicted** (explicit provenance in eviction log) |

This is **not** an effective context window, not CCOS recall, and not a 1M
attention claim. No real-text quality metric is produced here
(`NOT_MEASURED`).

## Accounting contract

- Every admitted segment has a `SegmentAccounting` row with raw
  `ContextSegmentId` provenance, content hash, fingerprint, fate, and steps.
- Cumulative invariant after every successful admit:
  `admitted == retained + evicted + lost`.
- The recurrent register is the XOR of fingerprints of currently retained
  residents (deterministic, reversible on eviction/loss).

## Fail-closed

- Capacity / hard_max of zero.
- Duplicate segment ID (collision).
- Stream ID mismatch.
- Expanding ceiling overflow (no silent drop; ID not recorded as seen).
- Restart from snapshot with unsupported schema, truncated retained coverage,
  unknown provenance, register mismatch, or stats mismatch.
- Admission of a segment with `fingerprint == 0` even when constructed via
  public fields (bypassing `WorkingSegment::new`); checked before any
  mutation (seen set, step, residents unchanged) — `ZeroFingerprint`.
- Restore of a snapshot whose retained rows are not in strictly increasing
  `admitted_step` (FIFO oldest-first) — `InconsistentFifoOrder`. The supplied
  order is rejected, never reconstructed, so it cannot drive the next eviction.
- Restore of a snapshot with inconsistent chronology —
  `InconsistentStepChronology`: `step != stats.admitted`; admission steps of
  retained ∪ eviction_log not exactly `1..=step`; retained rows not the most
  recent admissions; `resolved_step` outside `admitted_step+1..=step`; under
  FIFO policies `resolved_step != admitted_step + capacity` or fate not the
  policy fate (Fixed → Lost, CapacityBounded → Evicted); any resolved row under
  Expanding.

Hardening origin: Codex review on #845 (r4501101631 FIFO order, r4501101640
step chronology, r4501101645 zero-fingerprint admit), handoff RemoteOps #1208
comment 6705965765.

## Out of scope (this slice)

- CONTEXT-4 hot/warm/cold retention tiers.
- CONTEXT-5 adaptive historical refinement.
- LONGCTX-B1 campaign / real-text quality.
- Learned retention policies.

## Acceptance mapping

1. Deterministic Fixed vs Expanding vs CapacityBounded comparison — tests
   `three_policies_diverge_on_identical_stream` + oracle fixtures.
2. Explicit retained/evicted/lost + provenance — `SegmentAccounting` /
   `exact_oracle_fixture_*`.
3. Fail-closed negatives — `negative_*` tests.
4. Exact-oracle fixtures — `WorkingContextOracle`.
5. PR + SHA + local hooks + exact-head CI — delivery evidence in agent reports.
