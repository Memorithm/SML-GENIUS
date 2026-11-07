# SML-CONTEXT-2 — bounded recurrent working-context substrate

**Status:** implemented (fixture/oracle reference). **Owner:** Grok (G2).  
**Code:** `crates/sml-store/src/working_context.rs`.  
**Tests:** `crates/sml-store/tests/working_context.rs`.  
**Wire format (G4):** `crates/sml-store/src/working_context_wire.rs`,
tests `crates/sml-store/tests/working_context_wire.rs` — see
[Snapshot wire format](#snapshot-wire-format-g4).

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

## Snapshot wire format (G4)

Deterministic serialization / admission boundary for `WorkingContextSnapshot`.
The wire layer owns **syntax, limits and canonical form**; semantic validity
stays with `WorkingContext::restore`, which `decode_and_restore` /
`restore_into` call only after a complete wire decode succeeds.

**Versions.** Magic `SWC2`; wire version `1`
(`WORKING_CONTEXT_WIRE_VERSION`); snapshot schema `1`
(`WORKING_CONTEXT_SCHEMA_VERSION`). Any other wire version or schema is
rejected (`UnknownWireVersion` / `UnsupportedSchema`); there is no
best-effort or forward-compatible parse.

**Layout (little-endian).** 93-byte header (magic, wire version, schema,
stream id, policy tag 1/2/3 + non-zero limit, step, register, five stats
counters, `u32` counts R/E/S) → `R + E` fixed 74-byte provenance rows
(retained first, then eviction log) → `S` 8-byte seen ids. Field-level table
in the module docs.

**Limits (before allocation).**

| Limit | Value | Check |
| --- | --- | --- |
| `MAX_SNAPSHOT_ROWS` | 65 536 | `R + E` and `S` each, checked arithmetic |
| `MAX_SNAPSHOT_BYTES` | 93 + 65 536 × (74 + 8) = 5 374 045 | input length, before reading |
| policy limit | retained `R ≤ capacity/hard_max` | from header, before rows |
| exact length | `93 + 74(R+E) + 8S == len` | before any `Vec` is sized |

Callers may tighten limits with `SnapshotWireLimits::new` (zero or
above-hard-cap fails closed); the encoder applies the same limits.

**Canonical form.** Retained rows strictly increasing `admitted_step`;
eviction rows strictly increasing `segment_id`; seen ids strictly increasing
and non-zero; resolved flag ∈ {0,1} with zero padding when absent; row stream
id = header stream id. These are exactly the orders `snapshot()` emits. The
encoder refuses (never repairs) noncanonical snapshots, so for every accepted
input `encode(decode(b)) == b`.

**Fail-closed wire rejects.** `InputTooLarge`, `Truncated` (header, mid-row,
mid-seen), `TrailingBytes`, `BadMagic`, `UnknownWireVersion`,
`UnsupportedSchema`, `InvalidStreamId`, `UnknownPolicyTag`, `ZeroPolicyLimit`,
`RowCountLimit` (oversized length prefixes incl. `u32::MAX`),
`RetainedExceedsPolicyLimit`, `InvalidSegmentId`, `NoncanonicalResolvedStep`,
`UnknownFateTag`, `NoncanonicalOrder`, encoder-only `RowStreamMismatch`;
semantic failures surface as `Restore(WorkingContextError)`.

**Reject-without-mutation.** `restore_into(&mut ctx, bytes)` assigns only
after decode + restore succeed; a rejected input leaves the live context
bit-identical (tested over the negative corpus for all three policies).

**Non-claims.** No checksum/signature: semantically valid tampering of hashes
or order indices is not detected. Not an effective context window, not CCOS
recall, no 1M-attention or quality claim (`NOT_MEASURED`). No throughput
metric.

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
6. G4 wire format: documented schema/version limits (above + module docs);
   bounded negative corpus incl. oversized length prefixes and truncation
   (`negative_*` in `working_context_wire.rs`); deterministic canonical
   roundtrips for Fixed / Expanding / CapacityBounded with and without
   evictions (`canonical_roundtrip_all_policies_with_and_without_evictions`,
   `encoding_matches_independent_reference_writer_for_all_policies`,
   `restart_through_bytes_continues_identically_to_uninterrupted_run`);
   reject-without-mutation (`rejected_bytes_never_mutate_live_context`);
   exhaustive single-byte mutation sweep
   (`every_single_byte_mutation_is_rejected_or_canonical`). Crate is
   `#![forbid(unsafe_code)]`.
