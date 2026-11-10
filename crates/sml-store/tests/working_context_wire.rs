//! CONTEXT-2 G4: versioned bounded snapshot wire format — canonical roundtrips
//! and adversarial negative corpus.
//!
//! Synthetic streams only. No real-text quality claim, no LONGCTX-B1
//! measurement, no throughput metric.

use sml_core::context_ledger::{ContextSegmentId, ContextStreamId};
use sml_store::working_context::{
    synthetic_stream, RetentionFate, WorkingContext, WorkingContextError, WorkingContextOracle,
    WorkingContextSnapshot, WorkingPolicy, WORKING_CONTEXT_SCHEMA_VERSION,
};
use sml_store::working_context_wire::{
    decode_and_restore, decode_snapshot, decode_snapshot_with_limits, encode_context,
    encode_snapshot, encode_snapshot_with_limits, restore_into, SnapshotWireError,
    SnapshotWireLimits, WireSection, MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_ROWS, WIRE_HEADER_BYTES,
    WIRE_ROW_BYTES, WIRE_SEEN_ID_BYTES, WORKING_CONTEXT_WIRE_MAGIC, WORKING_CONTEXT_WIRE_VERSION,
};

// Header field offsets (frozen by the v1 layout documented in the module).
const OFF_WIRE_VERSION: usize = 4;
const OFF_SCHEMA: usize = 6;
const OFF_STREAM: usize = 8;
const OFF_POLICY_TAG: usize = 16;
const OFF_POLICY_LIMIT: usize = 17;
const OFF_STEP: usize = 25;
const OFF_RETAINED_COUNT: usize = 81;
const OFF_EVICTION_COUNT: usize = 85;
const OFF_SEEN_COUNT: usize = 89;
// Row field offsets.
const ROW_SEGMENT_ID: usize = 0;
const ROW_RESOLVED_FLAG: usize = 64;
const ROW_RESOLVED_STEP: usize = 65;
const ROW_FATE: usize = 73;

fn stream() -> ContextStreamId {
    ContextStreamId::new(42).unwrap()
}

fn ctx_after(policy: WorkingPolicy, n: u64) -> WorkingContext {
    let mut ctx = WorkingContext::new(stream(), policy).unwrap();
    for seg in synthetic_stream(stream(), n).unwrap() {
        assert_eq!(ctx.admit(seg).unwrap(), RetentionFate::Retained);
    }
    ctx
}

fn all_policy_cases() -> Vec<(WorkingPolicy, u64)> {
    vec![
        (WorkingPolicy::Fixed { capacity: 3 }, 0),
        (WorkingPolicy::Fixed { capacity: 3 }, 2),
        (WorkingPolicy::Fixed { capacity: 3 }, 3),
        (WorkingPolicy::Fixed { capacity: 3 }, 7),
        (WorkingPolicy::Fixed { capacity: 1 }, 4),
        (WorkingPolicy::Expanding { hard_max: 8 }, 0),
        (WorkingPolicy::Expanding { hard_max: 8 }, 5),
        (WorkingPolicy::Expanding { hard_max: 8 }, 8),
        (WorkingPolicy::CapacityBounded { capacity: 3 }, 0),
        (WorkingPolicy::CapacityBounded { capacity: 3 }, 2),
        (WorkingPolicy::CapacityBounded { capacity: 3 }, 9),
        (WorkingPolicy::CapacityBounded { capacity: 1 }, 4),
    ]
}

/// Independent reference writer for the v1 layout (does not call the crate
/// encoder). Freezes the byte format: any layout drift fails this test.
fn reference_bytes(snap: &WorkingContextSnapshot) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"SWC2");
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&snap.schema_version.to_le_bytes());
    out.extend_from_slice(&snap.stream_id.get().to_le_bytes());
    let (tag, limit) = match snap.policy {
        WorkingPolicy::Fixed { capacity } => (1_u8, capacity),
        WorkingPolicy::Expanding { hard_max } => (2_u8, hard_max),
        WorkingPolicy::CapacityBounded { capacity } => (3_u8, capacity),
    };
    out.push(tag);
    out.extend_from_slice(&(limit as u64).to_le_bytes());
    out.extend_from_slice(&snap.step.to_le_bytes());
    out.extend_from_slice(&snap.recurrent_register.to_le_bytes());
    for v in [
        snap.stats.admitted,
        snap.stats.retained,
        snap.stats.evicted,
        snap.stats.lost,
        snap.stats.recurrent_folds,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(snap.retained.len() as u32).to_le_bytes());
    out.extend_from_slice(&(snap.eviction_log.len() as u32).to_le_bytes());
    out.extend_from_slice(&(snap.seen_segment_ids.len() as u32).to_le_bytes());
    for row in snap.retained.iter().chain(snap.eviction_log.iter()) {
        out.extend_from_slice(&row.identity.segment_id.to_le_bytes());
        out.extend_from_slice(&row.order_index.to_le_bytes());
        out.extend_from_slice(row.content_hash.bytes());
        out.extend_from_slice(&row.fingerprint.to_le_bytes());
        out.extend_from_slice(&row.admitted_step.to_le_bytes());
        match row.resolved_step {
            None => {
                out.push(0);
                out.extend_from_slice(&0_u64.to_le_bytes());
            }
            Some(s) => {
                out.push(1);
                out.extend_from_slice(&s.to_le_bytes());
            }
        }
        out.push(match row.fate {
            RetentionFate::Retained => 0,
            RetentionFate::Evicted => 1,
            RetentionFate::Lost => 2,
        });
    }
    for id in &snap.seen_segment_ids {
        out.extend_from_slice(&id.to_le_bytes());
    }
    out
}

fn put_u16(bytes: &mut [u8], at: usize, v: u16) {
    bytes[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn put_u32(bytes: &mut [u8], at: usize, v: u32) {
    bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn put_u64(bytes: &mut [u8], at: usize, v: u64) {
    bytes[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
fn row_off(i: usize) -> usize {
    WIRE_HEADER_BYTES + i * WIRE_ROW_BYTES
}
fn seen_off(rows: usize, i: usize) -> usize {
    WIRE_HEADER_BYTES + rows * WIRE_ROW_BYTES + i * WIRE_SEEN_ID_BYTES
}

/// CapacityBounded{3} after 5 admissions: 3 retained + 2 evicted rows, 5 seen.
fn evicting_bytes() -> Vec<u8> {
    encode_context(&ctx_after(
        WorkingPolicy::CapacityBounded { capacity: 3 },
        5,
    ))
    .unwrap()
}

// ---------------------------------------------------------------------------
// Layout / constants
// ---------------------------------------------------------------------------

#[test]
fn documented_layout_constants_are_frozen() {
    assert_eq!(WORKING_CONTEXT_WIRE_MAGIC, *b"SWC2");
    assert_eq!(WORKING_CONTEXT_WIRE_VERSION, 1);
    assert_eq!(WORKING_CONTEXT_SCHEMA_VERSION, 1);
    assert_eq!(WIRE_HEADER_BYTES, 93);
    assert_eq!(WIRE_ROW_BYTES, 74);
    assert_eq!(WIRE_SEEN_ID_BYTES, 8);
    assert_eq!(MAX_SNAPSHOT_ROWS, 65_536);
    assert_eq!(
        MAX_SNAPSHOT_BYTES,
        WIRE_HEADER_BYTES + MAX_SNAPSHOT_ROWS * (WIRE_ROW_BYTES + WIRE_SEEN_ID_BYTES)
    );
    let d = SnapshotWireLimits::DEFAULT;
    assert_eq!(d.max_bytes(), MAX_SNAPSHOT_BYTES);
    assert_eq!(d.max_rows(), MAX_SNAPSHOT_ROWS);
}

#[test]
fn encoding_matches_independent_reference_writer_for_all_policies() {
    for (policy, n) in all_policy_cases() {
        let ctx = ctx_after(policy, n);
        let snap = ctx.snapshot();
        let bytes = encode_snapshot(&snap).unwrap();
        assert_eq!(bytes, reference_bytes(&snap), "{policy:?} n={n}");
        let rows = snap.retained.len() + snap.eviction_log.len();
        assert_eq!(
            bytes.len(),
            WIRE_HEADER_BYTES
                + rows * WIRE_ROW_BYTES
                + snap.seen_segment_ids.len() * WIRE_SEEN_ID_BYTES
        );
    }
}

// ---------------------------------------------------------------------------
// Deterministic canonical roundtrips (all three policies, with/without evictions)
// ---------------------------------------------------------------------------

#[test]
fn canonical_roundtrip_all_policies_with_and_without_evictions() {
    let mut saw_evicted = false;
    let mut saw_lost = false;
    let mut saw_none = false;
    for (policy, n) in all_policy_cases() {
        let ctx = ctx_after(policy, n);
        let snap = ctx.snapshot();
        saw_evicted |= snap
            .eviction_log
            .iter()
            .any(|r| r.fate == RetentionFate::Evicted);
        saw_lost |= snap
            .eviction_log
            .iter()
            .any(|r| r.fate == RetentionFate::Lost);
        saw_none |= snap.eviction_log.is_empty();

        let bytes = encode_snapshot(&snap).unwrap();
        // Deterministic: same input -> identical bytes.
        assert_eq!(bytes, encode_snapshot(&snap).unwrap());
        assert_eq!(bytes, encode_context(&ctx).unwrap());
        // decode -> snapshot equality.
        let decoded = decode_snapshot(&bytes).unwrap();
        assert_eq!(decoded, snap, "{policy:?} n={n}");
        // Canonical identity: encode(decode(bytes)) == bytes.
        assert_eq!(encode_snapshot(&decoded).unwrap(), bytes);
        // decode -> restore yields an equal live context.
        let restored = decode_and_restore(&bytes).unwrap();
        assert_eq!(restored, ctx, "{policy:?} n={n}");
        assert_eq!(encode_context(&restored).unwrap(), bytes);
    }
    assert!(saw_evicted && saw_lost && saw_none);
}

#[test]
fn independent_runs_produce_identical_bytes() {
    for (policy, n) in all_policy_cases() {
        let a = encode_context(&ctx_after(policy, n)).unwrap();
        let b = encode_context(&ctx_after(policy, n)).unwrap();
        assert_eq!(a, b);
    }
}

#[test]
fn restart_through_bytes_continues_identically_to_uninterrupted_run() {
    let total = 9_u64;
    for policy in [
        WorkingPolicy::Fixed { capacity: 3 },
        WorkingPolicy::Expanding { hard_max: 16 },
        WorkingPolicy::CapacityBounded { capacity: 2 },
    ] {
        let segments = synthetic_stream(stream(), total).unwrap();
        for cut in 0..=segments.len() {
            let mut live = WorkingContext::new(stream(), policy).unwrap();
            for seg in &segments[..cut] {
                live.admit(seg.clone()).unwrap();
            }
            let bytes = encode_context(&live).unwrap();
            drop(live); // "process restart": only the bytes survive
            let mut restored = decode_and_restore(&bytes).unwrap();
            for seg in &segments[cut..] {
                restored.admit(seg.clone()).unwrap();
            }
            let uninterrupted = ctx_after(policy, total);
            assert_eq!(restored, uninterrupted, "{policy:?} cut={cut}");
            let oracle = WorkingContextOracle::evaluate(policy, &segments).unwrap();
            assert_eq!(restored.recurrent_register(), oracle.recurrent_register);
            assert_eq!(restored.stats(), oracle.stats);
            assert_eq!(
                encode_context(&restored).unwrap(),
                encode_context(&uninterrupted).unwrap()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Limits applied before allocation
// ---------------------------------------------------------------------------

#[test]
fn limits_constructor_rejects_zero_and_above_hard_caps() {
    assert!(SnapshotWireLimits::new(0, 1).is_err());
    assert!(SnapshotWireLimits::new(WIRE_HEADER_BYTES, 0).is_err());
    assert!(SnapshotWireLimits::new(MAX_SNAPSHOT_BYTES + 1, 1).is_err());
    assert!(SnapshotWireLimits::new(MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_ROWS + 1).is_err());
    assert!(SnapshotWireLimits::new(MAX_SNAPSHOT_BYTES, MAX_SNAPSHOT_ROWS).is_ok());
}

#[test]
fn negative_input_above_hard_byte_cap_rejected_before_parse() {
    let huge = vec![0_u8; MAX_SNAPSHOT_BYTES + 1];
    assert_eq!(
        decode_snapshot(&huge),
        Err(SnapshotWireError::InputTooLarge {
            len: MAX_SNAPSHOT_BYTES + 1,
            max: MAX_SNAPSHOT_BYTES
        })
    );
}

#[test]
fn negative_input_above_caller_byte_limit_rejected() {
    let bytes = evicting_bytes();
    let tight = SnapshotWireLimits::new(bytes.len() - 1, MAX_SNAPSHOT_ROWS).unwrap();
    assert_eq!(
        decode_snapshot_with_limits(&bytes, tight),
        Err(SnapshotWireError::InputTooLarge {
            len: bytes.len(),
            max: bytes.len() - 1
        })
    );
    let exact = SnapshotWireLimits::new(bytes.len(), MAX_SNAPSHOT_ROWS).unwrap();
    assert!(decode_snapshot_with_limits(&bytes, exact).is_ok());
}

#[test]
fn negative_row_count_over_caller_limit_rejected() {
    let bytes = evicting_bytes(); // 5 provenance rows, 5 seen ids
    let tight = SnapshotWireLimits::new(MAX_SNAPSHOT_BYTES, 4).unwrap();
    assert!(matches!(
        decode_snapshot_with_limits(&bytes, tight),
        Err(SnapshotWireError::RowCountLimit { .. })
    ));
    let snap = decode_snapshot(&bytes).unwrap();
    assert!(matches!(
        encode_snapshot_with_limits(&snap, tight),
        Err(SnapshotWireError::RowCountLimit { .. })
    ));
    let byte_tight = SnapshotWireLimits::new(bytes.len() - 1, MAX_SNAPSHOT_ROWS).unwrap();
    assert!(matches!(
        encode_snapshot_with_limits(&snap, byte_tight),
        Err(SnapshotWireError::InputTooLarge { .. })
    ));
    let ok = SnapshotWireLimits::new(bytes.len(), 5).unwrap();
    assert_eq!(encode_snapshot_with_limits(&snap, ok).unwrap(), bytes);
}

#[test]
fn negative_oversized_length_prefixes_rejected_before_allocation() {
    // u32::MAX counts on a tiny buffer: must be rejected by the row-count
    // limit (never by an allocation attempt or a panic).
    for off in [OFF_RETAINED_COUNT, OFF_EVICTION_COUNT, OFF_SEEN_COUNT] {
        for v in [u32::MAX, (MAX_SNAPSHOT_ROWS as u32) + 1] {
            let mut bytes = evicting_bytes();
            put_u32(&mut bytes, off, v);
            let err = decode_snapshot(&bytes).unwrap_err();
            assert!(
                matches!(
                    err,
                    SnapshotWireError::RowCountLimit { .. }
                        | SnapshotWireError::RetainedExceedsPolicyLimit { .. }
                ),
                "off={off} v={v} err={err:?}"
            );
        }
    }
    // Sum of retained + eviction exceeds max_rows while each alone fits.
    let mut bytes = encode_context(&ctx_after(
        WorkingPolicy::Expanding { hard_max: 1 << 20 },
        0,
    ))
    .unwrap();
    put_u32(&mut bytes, OFF_RETAINED_COUNT, MAX_SNAPSHOT_ROWS as u32);
    put_u32(&mut bytes, OFF_EVICTION_COUNT, 1);
    assert!(matches!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::RowCountLimit { .. })
    ));
    // Counts within limit but far beyond the buffer -> Truncated (exact
    // length is computed from counts before any row vector is allocated).
    let mut bytes = encode_context(&ctx_after(
        WorkingPolicy::Expanding { hard_max: 1 << 20 },
        0,
    ))
    .unwrap();
    put_u32(&mut bytes, OFF_RETAINED_COUNT, MAX_SNAPSHOT_ROWS as u32);
    assert!(matches!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::Truncated { .. })
    ));
}

#[test]
fn negative_retained_count_above_policy_limit() {
    let mut bytes = evicting_bytes(); // capacity 3
    put_u32(&mut bytes, OFF_RETAINED_COUNT, 4);
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::RetainedExceedsPolicyLimit {
            retained: 4,
            limit: 3
        })
    );
}

// ---------------------------------------------------------------------------
// Truncation / trailing bytes
// ---------------------------------------------------------------------------

#[test]
fn negative_every_strict_prefix_is_truncated() {
    for (policy, n) in all_policy_cases() {
        let bytes = encode_context(&ctx_after(policy, n)).unwrap();
        for cut in 0..bytes.len() {
            assert!(
                matches!(
                    decode_snapshot(&bytes[..cut]),
                    Err(SnapshotWireError::Truncated { .. })
                ),
                "{policy:?} n={n} cut={cut}"
            );
        }
    }
}

#[test]
fn negative_truncation_mid_row_and_mid_seen() {
    let bytes = evicting_bytes();
    let mid_row = row_off(1) + WIRE_ROW_BYTES / 2;
    assert_eq!(
        decode_snapshot(&bytes[..mid_row]),
        Err(SnapshotWireError::Truncated {
            needed: bytes.len(),
            got: mid_row
        })
    );
    let mid_seen = seen_off(5, 2) + 3;
    assert_eq!(
        decode_snapshot(&bytes[..mid_seen]),
        Err(SnapshotWireError::Truncated {
            needed: bytes.len(),
            got: mid_seen
        })
    );
    assert_eq!(
        decode_snapshot(&[]),
        Err(SnapshotWireError::Truncated {
            needed: WIRE_HEADER_BYTES,
            got: 0
        })
    );
}

#[test]
fn negative_trailing_bytes() {
    for extra in [1_usize, 7, 8, WIRE_ROW_BYTES] {
        let mut bytes = evicting_bytes();
        let len = bytes.len();
        bytes.extend(std::iter::repeat_n(0_u8, extra));
        assert_eq!(
            decode_snapshot(&bytes),
            Err(SnapshotWireError::TrailingBytes { extra }),
            "len={len}"
        );
    }
}

// ---------------------------------------------------------------------------
// Versioning / header fields
// ---------------------------------------------------------------------------

#[test]
fn negative_bad_magic() {
    for i in 0..4 {
        let mut bytes = evicting_bytes();
        bytes[i] ^= 0x20;
        assert_eq!(decode_snapshot(&bytes), Err(SnapshotWireError::BadMagic));
    }
}

#[test]
fn negative_unknown_wire_version() {
    for v in [0_u16, 2, 0x0100, u16::MAX] {
        let mut bytes = evicting_bytes();
        put_u16(&mut bytes, OFF_WIRE_VERSION, v);
        assert_eq!(
            decode_snapshot(&bytes),
            Err(SnapshotWireError::UnknownWireVersion { got: v })
        );
    }
}

#[test]
fn negative_unsupported_schema_version() {
    for v in [0_u16, 2, u16::MAX] {
        let mut bytes = evicting_bytes();
        put_u16(&mut bytes, OFF_SCHEMA, v);
        assert_eq!(
            decode_snapshot(&bytes),
            Err(SnapshotWireError::UnsupportedSchema { got: v })
        );
    }
}

#[test]
fn negative_zero_stream_id() {
    let mut bytes = evicting_bytes();
    put_u64(&mut bytes, OFF_STREAM, 0);
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::InvalidStreamId)
    );
}

#[test]
fn negative_unknown_policy_tag_and_zero_limit() {
    for tag in [0_u8, 4, 0x80, 0xFF] {
        let mut bytes = evicting_bytes();
        bytes[OFF_POLICY_TAG] = tag;
        assert_eq!(
            decode_snapshot(&bytes),
            Err(SnapshotWireError::UnknownPolicyTag { got: tag })
        );
    }
    let mut bytes = evicting_bytes();
    put_u64(&mut bytes, OFF_POLICY_LIMIT, 0);
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::ZeroPolicyLimit)
    );
}

// ---------------------------------------------------------------------------
// Row-level canonical encoding
// ---------------------------------------------------------------------------

#[test]
fn negative_noncanonical_resolved_flag_and_padding() {
    let mut bytes = evicting_bytes();
    bytes[row_off(0) + ROW_RESOLVED_FLAG] = 2;
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::NoncanonicalResolvedStep { row: 0 })
    );
    // flag 0 (None) but nonzero padding step.
    let mut bytes = evicting_bytes();
    put_u64(&mut bytes, row_off(0) + ROW_RESOLVED_STEP, 9);
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::NoncanonicalResolvedStep { row: 0 })
    );
}

#[test]
fn negative_unknown_fate_tag() {
    for tag in [3_u8, 0x7F, 0xFF] {
        let mut bytes = evicting_bytes();
        bytes[row_off(4) + ROW_FATE] = tag;
        assert_eq!(
            decode_snapshot(&bytes),
            Err(SnapshotWireError::UnknownFateTag { row: 4, got: tag })
        );
    }
}

#[test]
fn negative_zero_segment_ids() {
    let mut bytes = evicting_bytes();
    put_u64(&mut bytes, row_off(2) + ROW_SEGMENT_ID, 0);
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::InvalidSegmentId { row: 2 })
    );
    let mut bytes = evicting_bytes();
    put_u64(&mut bytes, seen_off(5, 0), 0);
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::InvalidSegmentId { row: 5 })
    );
}

// ---------------------------------------------------------------------------
// Noncanonical ordering
// ---------------------------------------------------------------------------

fn swap_ranges(bytes: &mut [u8], a: usize, b: usize, len: usize) {
    let tmp: Vec<u8> = bytes[a..a + len].to_vec();
    bytes.copy_within(b..b + len, a);
    bytes[b..b + len].copy_from_slice(&tmp);
}

#[test]
fn negative_noncanonical_retained_order() {
    // Rows 0..3 are retained (segments 3,4,5), rows 3..5 eviction (1,2).
    let mut bytes = evicting_bytes();
    swap_ranges(&mut bytes, row_off(0), row_off(1), WIRE_ROW_BYTES);
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::NoncanonicalOrder {
            section: WireSection::Retained,
            index: 1
        })
    );
}

#[test]
fn negative_noncanonical_eviction_order() {
    let mut bytes = evicting_bytes();
    swap_ranges(&mut bytes, row_off(3), row_off(4), WIRE_ROW_BYTES);
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::NoncanonicalOrder {
            section: WireSection::EvictionLog,
            index: 1
        })
    );
}

#[test]
fn negative_noncanonical_and_duplicate_seen_ids() {
    let mut bytes = evicting_bytes();
    swap_ranges(
        &mut bytes,
        seen_off(5, 1),
        seen_off(5, 2),
        WIRE_SEEN_ID_BYTES,
    );
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::NoncanonicalOrder {
            section: WireSection::SeenIds,
            index: 2
        })
    );
    let mut bytes = evicting_bytes();
    let first = bytes[seen_off(5, 0)..seen_off(5, 1)].to_vec();
    bytes[seen_off(5, 1)..seen_off(5, 2)].copy_from_slice(&first);
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::NoncanonicalOrder {
            section: WireSection::SeenIds,
            index: 1
        })
    );
}

// ---------------------------------------------------------------------------
// Encoder refuses to emit noncanonical / unbounded bytes
// ---------------------------------------------------------------------------

#[test]
fn encoder_rejects_noncanonical_or_invalid_snapshots() {
    let base = ctx_after(WorkingPolicy::CapacityBounded { capacity: 3 }, 5).snapshot();

    let mut s = base.clone();
    s.schema_version = 2;
    assert_eq!(
        encode_snapshot(&s),
        Err(SnapshotWireError::UnsupportedSchema { got: 2 })
    );

    let mut s = base.clone();
    s.retained.swap(0, 1);
    assert!(matches!(
        encode_snapshot(&s),
        Err(SnapshotWireError::NoncanonicalOrder {
            section: WireSection::Retained,
            ..
        })
    ));

    let mut s = base.clone();
    s.eviction_log.swap(0, 1);
    assert!(matches!(
        encode_snapshot(&s),
        Err(SnapshotWireError::NoncanonicalOrder {
            section: WireSection::EvictionLog,
            ..
        })
    ));

    let mut s = base.clone();
    s.seen_segment_ids.swap(0, 1);
    assert!(matches!(
        encode_snapshot(&s),
        Err(SnapshotWireError::NoncanonicalOrder {
            section: WireSection::SeenIds,
            ..
        })
    ));

    let mut s = base.clone();
    s.retained[0].identity = ContextSegmentId {
        stream_id: ContextStreamId::new(7).unwrap(),
        segment_id: s.retained[0].identity.segment_id,
    };
    assert_eq!(
        encode_snapshot(&s),
        Err(SnapshotWireError::RowStreamMismatch { row: 0 })
    );

    let mut s = base.clone();
    s.retained[1].identity.segment_id = 0;
    assert_eq!(
        encode_snapshot(&s),
        Err(SnapshotWireError::InvalidSegmentId { row: 1 })
    );

    let mut s = base.clone();
    s.retained[0].resolved_step = None;
    s.eviction_log[0].resolved_step = None;
    // Wire-level allows None; encoding is still canonical. Semantic restore
    // rejects it (eviction row missing resolved_step).
    let bytes = encode_snapshot(&s).unwrap();
    assert!(matches!(
        decode_and_restore(&bytes),
        Err(SnapshotWireError::Restore(
            WorkingContextError::TruncatedProvenance { .. }
        ))
    ));

    let mut s = base;
    s.policy = WorkingPolicy::Fixed { capacity: 0 };
    assert_eq!(encode_snapshot(&s), Err(SnapshotWireError::ZeroPolicyLimit));
}

// ---------------------------------------------------------------------------
// Wire-valid but semantically invalid -> restore rejects (fail closed)
// ---------------------------------------------------------------------------

#[test]
fn wire_valid_semantic_corruptions_rejected_by_restore() {
    // Register flip.
    let mut snap = decode_snapshot(&evicting_bytes()).unwrap();
    snap.recurrent_register ^= 1;
    let bytes = encode_snapshot(&snap).unwrap();
    assert!(decode_snapshot(&bytes).is_ok());
    assert!(matches!(
        decode_and_restore(&bytes),
        Err(SnapshotWireError::Restore(
            WorkingContextError::RegisterMismatch { .. }
        ))
    ));
    // Step / stats drift.
    let mut bytes = evicting_bytes();
    put_u64(&mut bytes, OFF_STEP, 99);
    assert!(decode_snapshot(&bytes).is_ok());
    assert!(matches!(
        decode_and_restore(&bytes),
        Err(SnapshotWireError::Restore(_))
    ));
    // Policy limit shrink below retained count is a wire-level reject.
    let mut bytes = evicting_bytes();
    put_u64(&mut bytes, OFF_POLICY_LIMIT, 2);
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::RetainedExceedsPolicyLimit {
            retained: 3,
            limit: 2
        })
    );
    // Policy kind swap CapacityBounded -> Fixed: wire-valid, restore rejects
    // (eviction fate does not match policy).
    let mut bytes = evicting_bytes();
    bytes[OFF_POLICY_TAG] = 1;
    assert!(decode_snapshot(&bytes).is_ok());
    assert!(matches!(
        decode_and_restore(&bytes),
        Err(SnapshotWireError::Restore(
            WorkingContextError::InconsistentStepChronology { .. }
        ))
    ));
    // Zero fingerprint row.
    let mut bytes = evicting_bytes();
    put_u64(&mut bytes, row_off(0) + 48, 0);
    assert!(matches!(
        decode_and_restore(&bytes),
        Err(SnapshotWireError::Restore(
            WorkingContextError::ZeroFingerprint
        ))
    ));
}

// ---------------------------------------------------------------------------
// Reject-without-mutation across restart
// ---------------------------------------------------------------------------

fn negative_corpus() -> Vec<(&'static str, Vec<u8>)> {
    let good = evicting_bytes();
    let mut corpus: Vec<(&'static str, Vec<u8>)> = Vec::new();
    corpus.push(("empty", Vec::new()));
    corpus.push(("header_only_prefix", good[..WIRE_HEADER_BYTES - 1].to_vec()));
    corpus.push(("mid_row_truncation", good[..row_off(2) + 10].to_vec()));
    let mut t = good.clone();
    t.push(0);
    corpus.push(("trailing_byte", t));
    let mut t = good.clone();
    put_u16(&mut t, OFF_WIRE_VERSION, 2);
    corpus.push(("unknown_wire_version", t));
    let mut t = good.clone();
    put_u16(&mut t, OFF_SCHEMA, 9);
    corpus.push(("unsupported_schema", t));
    let mut t = good.clone();
    put_u32(&mut t, OFF_RETAINED_COUNT, u32::MAX);
    corpus.push(("oversized_retained_prefix", t));
    let mut t = good.clone();
    put_u32(&mut t, OFF_EVICTION_COUNT, u32::MAX);
    corpus.push(("oversized_eviction_prefix", t));
    let mut t = good.clone();
    put_u32(&mut t, OFF_SEEN_COUNT, u32::MAX);
    corpus.push(("oversized_seen_prefix", t));
    let mut t = good.clone();
    swap_ranges(&mut t, row_off(0), row_off(1), WIRE_ROW_BYTES);
    corpus.push(("noncanonical_retained_order", t));
    let mut t = good.clone();
    bytes_flip(&mut t, OFF_POLICY_TAG, 0xFF);
    corpus.push(("unknown_policy_tag", t));
    let mut t = good.clone();
    put_u64(&mut t, OFF_STEP, 99);
    corpus.push(("semantic_step_drift", t));
    let mut t = good;
    bytes_flip(&mut t, OFF_POLICY_TAG, 1 ^ 3);
    corpus.push(("semantic_policy_kind_swap", t));
    corpus
}

fn bytes_flip(bytes: &mut [u8], at: usize, mask: u8) {
    bytes[at] ^= mask;
}

#[test]
fn rejected_bytes_never_mutate_live_context() {
    for policy in [
        WorkingPolicy::Fixed { capacity: 2 },
        WorkingPolicy::Expanding { hard_max: 8 },
        WorkingPolicy::CapacityBounded { capacity: 2 },
    ] {
        let mut live = ctx_after(policy, 4);
        let before = live.clone();
        let before_bytes = encode_context(&live).unwrap();
        for (name, bad) in negative_corpus() {
            assert!(
                restore_into(&mut live, &bad).is_err(),
                "{name} unexpectedly accepted"
            );
            assert_eq!(live, before, "{name} mutated live context");
            assert_eq!(encode_context(&live).unwrap(), before_bytes, "{name}");
        }
        // The live context keeps working after rejected restores.
        let next = synthetic_stream(stream(), 5).unwrap().pop().unwrap();
        live.admit(next).unwrap();
        assert_eq!(live, ctx_after(policy, 5));
    }
}

#[test]
fn restore_into_replaces_state_only_on_full_success() {
    let mut live = ctx_after(WorkingPolicy::Fixed { capacity: 2 }, 4);
    let target = ctx_after(WorkingPolicy::CapacityBounded { capacity: 3 }, 5);
    restore_into(&mut live, &encode_context(&target).unwrap()).unwrap();
    assert_eq!(live, target);
}

// ---------------------------------------------------------------------------
// Exhaustive deterministic mutation sweep: accepted => canonical identity
// ---------------------------------------------------------------------------

#[test]
fn every_single_byte_mutation_is_rejected_or_canonical() {
    for (policy, n) in [
        (WorkingPolicy::Fixed { capacity: 2 }, 4_u64),
        (WorkingPolicy::Expanding { hard_max: 4 }, 3),
        (WorkingPolicy::CapacityBounded { capacity: 2 }, 4),
    ] {
        let good = encode_context(&ctx_after(policy, n)).unwrap();
        let mut accepted_wire = 0_usize;
        let mut accepted_restore = 0_usize;
        for pos in 0..good.len() {
            for mask in [0x01_u8, 0x02, 0x80, 0xFF] {
                let mut m = good.clone();
                m[pos] ^= mask;
                if let Ok(snap) = decode_snapshot(&m) {
                    accepted_wire += 1;
                    assert_eq!(
                        encode_snapshot(&snap).unwrap(),
                        m,
                        "noncanonical accept pos={pos} mask={mask:#x}"
                    );
                }
                if let Ok(ctx) = decode_and_restore(&m) {
                    accepted_restore += 1;
                    assert_eq!(encode_context(&ctx).unwrap(), m);
                }
            }
        }
        // Wire layer accepts semantic payload flips (hashes, order_index);
        // restore must be at least as strict as wire decode.
        assert!(accepted_restore <= accepted_wire);
    }
}
