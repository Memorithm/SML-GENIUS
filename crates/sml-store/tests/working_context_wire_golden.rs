//! CONTEXT-2 G5: golden-byte fixtures for the v1 working-context snapshot
//! wire format (`SWC2`, wire version 1, schema 1).
//!
//! Format qualification only. The committed `.hex` fixtures under
//! `tests/fixtures/working_context_wire/` freeze the exact bytes produced for
//! a small set of fixed, deterministic snapshots. Any change to the layout,
//! to canonical ordering, or to the admission/fold semantics that feed the
//! snapshot makes these tests fail and requires an explicit, reviewed
//! re-bless (`SML_WIRE_GOLDEN_BLESS=1 cargo test -p sml-store --test
//! working_context_wire_golden`).
//!
//! Segments are built in this file (not via `synthetic_stream`) so the
//! fixtures do not silently depend on that helper. No model-quality,
//! context-span, latency or throughput claim is made (`NOT_MEASURED`).

use std::path::PathBuf;

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_store::working_context::{
    RetentionFate, WorkingContext, WorkingContextError, WorkingPolicy, WorkingSegment,
    WORKING_CONTEXT_SCHEMA_VERSION,
};
use sml_store::working_context_wire::{
    decode_and_restore, decode_snapshot, decode_snapshot_with_limits, encode_context,
    encode_snapshot, encode_snapshot_with_limits, SnapshotWireError, SnapshotWireLimits,
    WIRE_HEADER_BYTES, WIRE_ROW_BYTES, WIRE_SEEN_ID_BYTES, WORKING_CONTEXT_WIRE_MAGIC,
    WORKING_CONTEXT_WIRE_VERSION,
};

const BLESS_ENV: &str = "SML_WIRE_GOLDEN_BLESS";
const GOLDEN_STREAM: u64 = 0x5357_4332_0000_0007;

const OFF_WIRE_VERSION: usize = 4;
const OFF_SCHEMA: usize = 6;
const OFF_RETAINED_COUNT: usize = 81;
const OFF_EVICTION_COUNT: usize = 85;
const OFF_SEEN_COUNT: usize = 89;

struct GoldenCase {
    name: &'static str,
    policy: WorkingPolicy,
    admissions: u64,
    /// Expected (retained, eviction-log, seen) counts, checked independently
    /// of the fixture so a wrong bless cannot go unnoticed.
    counts: (usize, usize, usize),
}

fn cases() -> Vec<GoldenCase> {
    vec![
        GoldenCase {
            name: "fixed_cap3_empty",
            policy: WorkingPolicy::Fixed { capacity: 3 },
            admissions: 0,
            counts: (0, 0, 0),
        },
        GoldenCase {
            name: "fixed_cap2_lost",
            policy: WorkingPolicy::Fixed { capacity: 2 },
            admissions: 4,
            counts: (2, 2, 4),
        },
        GoldenCase {
            name: "expanding_hm4_partial",
            policy: WorkingPolicy::Expanding { hard_max: 4 },
            admissions: 3,
            counts: (3, 0, 3),
        },
        GoldenCase {
            name: "capacity_bounded_cap3_evicting",
            policy: WorkingPolicy::CapacityBounded { capacity: 3 },
            admissions: 5,
            counts: (3, 2, 5),
        },
    ]
}

fn stream() -> ContextStreamId {
    ContextStreamId::new(GOLDEN_STREAM).unwrap()
}

/// Deterministic, hand-specified segment `i` (1-based). Independent of
/// `synthetic_stream`.
fn golden_segment(i: u64) -> WorkingSegment {
    let identity = ContextSegmentId::new(stream(), 100 + i).unwrap();
    let mut hash = [0_u8; 32];
    for (j, b) in hash.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(31).wrapping_add(j as u8);
    }
    let fingerprint = 0x0101_0101_0101_0101_u64.wrapping_mul(i) ^ 0xA5A5_0000_0000_5A5A;
    WorkingSegment::new(
        identity,
        10 * i,
        ContextContentHash::from_sha256(hash),
        fingerprint,
    )
    .unwrap()
}

fn build(case: &GoldenCase) -> WorkingContext {
    let mut ctx = WorkingContext::new(stream(), case.policy).unwrap();
    for i in 1..=case.admissions {
        ctx.admit(golden_segment(i)).unwrap();
    }
    ctx
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/working_context_wire")
        .join(format!("{name}.hex"))
}

fn to_hex_fixture(name: &str, bytes: &[u8]) -> String {
    let mut s = format!(
        "# CONTEXT-2 G5 golden: {name}\n# SWC2 wire v1, {} bytes, lowercase hex, 32 bytes/line\n",
        bytes.len()
    );
    for chunk in bytes.chunks(32) {
        for b in chunk {
            s.push_str(&format!("{b:02x}"));
        }
        s.push('\n');
    }
    s
}

fn parse_hex_fixture(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        assert!(line.len() % 2 == 0, "odd hex line length");
        for k in (0..line.len()).step_by(2) {
            out.push(u8::from_str_radix(&line[k..k + 2], 16).expect("hex digit"));
        }
    }
    out
}

fn golden(case: &GoldenCase) -> Vec<u8> {
    let path = fixture_path(case.name);
    if std::env::var_os(BLESS_ENV).is_some() {
        let bytes = encode_context(&build(case)).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Atomic replace: tests run in parallel and read the same fixtures.
        let tmp = path.with_extension(format!("hex.tmp.{:?}", std::thread::current().id()));
        std::fs::write(&tmp, to_hex_fixture(case.name, &bytes)).unwrap();
        std::fs::rename(&tmp, &path).unwrap();
    }
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing golden fixture {}: {e}", path.display()));
    parse_hex_fixture(&text)
}

fn evicting() -> (GoldenCase, Vec<u8>) {
    let case = cases().pop().unwrap();
    assert_eq!(case.name, "capacity_bounded_cap3_evicting");
    let bytes = golden(&case);
    (case, bytes)
}

// ---------------------------------------------------------------------------
// Positive: golden equality and roundtrip
// ---------------------------------------------------------------------------

#[test]
fn golden_fixtures_have_expected_frame_and_lengths() {
    for case in cases() {
        let bytes = golden(&case);
        let (r, e, s) = case.counts;
        assert_eq!(
            bytes.len(),
            WIRE_HEADER_BYTES + (r + e) * WIRE_ROW_BYTES + s * WIRE_SEEN_ID_BYTES,
            "{}",
            case.name
        );
        assert_eq!(bytes[..4], WORKING_CONTEXT_WIRE_MAGIC, "{}", case.name);
        assert_eq!(
            u16::from_le_bytes([bytes[4], bytes[5]]),
            WORKING_CONTEXT_WIRE_VERSION
        );
        assert_eq!(
            u16::from_le_bytes([bytes[6], bytes[7]]),
            WORKING_CONTEXT_SCHEMA_VERSION
        );
        assert_eq!(
            u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            GOLDEN_STREAM
        );
        let count = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        assert_eq!(count(OFF_RETAINED_COUNT) as usize, r, "{}", case.name);
        assert_eq!(count(OFF_EVICTION_COUNT) as usize, e, "{}", case.name);
        assert_eq!(count(OFF_SEEN_COUNT) as usize, s, "{}", case.name);
    }
}

#[test]
fn encode_of_fixed_snapshots_equals_golden_bytes() {
    for case in cases() {
        let expected = golden(&case);
        let ctx = build(&case);
        assert_eq!(encode_context(&ctx).unwrap(), expected, "{}", case.name);
        assert_eq!(
            encode_snapshot(&ctx.snapshot()).unwrap(),
            expected,
            "{}",
            case.name
        );
    }
}

#[test]
fn decode_of_golden_roundtrips_to_identical_snapshot_and_bytes() {
    for case in cases() {
        let bytes = golden(&case);
        let snap = decode_snapshot(&bytes).unwrap();
        assert_eq!(snap, build(&case).snapshot(), "{}", case.name);
        assert_eq!(encode_snapshot(&snap).unwrap(), bytes, "{}", case.name);
    }
}

#[test]
fn golden_fate_tags_match_policy_semantics() {
    let all = cases();
    let lost = decode_snapshot(&golden(&all[1])).unwrap();
    assert!(lost
        .eviction_log
        .iter()
        .all(|r| r.fate == RetentionFate::Lost && r.resolved_step.is_some()));
    let evicting = decode_snapshot(&golden(&all[3])).unwrap();
    assert!(evicting
        .eviction_log
        .iter()
        .all(|r| r.fate == RetentionFate::Evicted && r.resolved_step.is_some()));
    for snap in [&lost, &evicting] {
        assert!(snap
            .retained
            .iter()
            .all(|r| r.fate == RetentionFate::Retained && r.resolved_step.is_none()));
    }
}

#[test]
fn decode_and_restore_of_golden_continues_like_uninterrupted_run() {
    for case in cases() {
        if matches!(case.policy, WorkingPolicy::Expanding { hard_max } if case.admissions as usize >= hard_max)
        {
            continue;
        }
        let bytes = golden(&case);
        let mut restored = decode_and_restore(&bytes).unwrap();
        let mut live = build(&case);
        assert_eq!(restored, live, "{}", case.name);
        let next = golden_segment(case.admissions + 1);
        assert_eq!(
            restored.admit(next.clone()).unwrap(),
            live.admit(next).unwrap()
        );
        assert_eq!(restored, live, "{}", case.name);
        assert_eq!(
            encode_context(&restored).unwrap(),
            encode_context(&live).unwrap()
        );
    }
}

// ---------------------------------------------------------------------------
// Negative: typed rejections derived from golden bytes
// ---------------------------------------------------------------------------

#[test]
fn golden_wire_version_mismatch_rejected() {
    for case in cases() {
        let mut bytes = golden(&case);
        for v in [0_u16, 2, u16::MAX] {
            bytes[OFF_WIRE_VERSION..OFF_WIRE_VERSION + 2].copy_from_slice(&v.to_le_bytes());
            assert_eq!(
                decode_snapshot(&bytes),
                Err(SnapshotWireError::UnknownWireVersion { got: v }),
                "{}",
                case.name
            );
            assert!(matches!(
                decode_and_restore(&bytes),
                Err(SnapshotWireError::UnknownWireVersion { .. })
            ));
        }
    }
}

#[test]
fn golden_schema_mismatch_and_bad_magic_rejected() {
    let (_, golden_bytes) = evicting();
    let mut bytes = golden_bytes.clone();
    bytes[OFF_SCHEMA..OFF_SCHEMA + 2].copy_from_slice(&2_u16.to_le_bytes());
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::UnsupportedSchema { got: 2 })
    );
    let mut bytes = golden_bytes;
    bytes[0] = b'X';
    assert_eq!(decode_snapshot(&bytes), Err(SnapshotWireError::BadMagic));
}

#[test]
fn every_strict_prefix_of_every_golden_is_truncated() {
    for case in cases() {
        let bytes = golden(&case);
        for cut in 0..bytes.len() {
            match decode_snapshot(&bytes[..cut]) {
                Err(SnapshotWireError::Truncated { needed, got }) => {
                    assert_eq!(got, cut, "{} cut {cut}", case.name);
                    assert!(needed > cut, "{} cut {cut}", case.name);
                    if cut >= WIRE_HEADER_BYTES {
                        assert_eq!(needed, bytes.len(), "{} cut {cut}", case.name);
                    }
                }
                other => panic!("{} cut {cut}: expected Truncated, got {other:?}", case.name),
            }
        }
    }
}

#[test]
fn golden_with_trailing_bytes_rejected() {
    for case in cases() {
        let bytes = golden(&case);
        for extra in [1_usize, 7, WIRE_SEEN_ID_BYTES, WIRE_ROW_BYTES] {
            let mut padded = bytes.clone();
            padded.extend(std::iter::repeat_n(0_u8, extra));
            assert_eq!(
                decode_snapshot(&padded),
                Err(SnapshotWireError::TrailingBytes { extra }),
                "{}",
                case.name
            );
        }
    }
}

#[test]
fn golden_over_caller_byte_limit_rejected_on_decode_and_encode() {
    for case in cases() {
        let bytes = golden(&case);
        let exact = SnapshotWireLimits::new(bytes.len(), 16).unwrap();
        assert_eq!(
            decode_snapshot_with_limits(&bytes, exact).unwrap(),
            build(&case).snapshot()
        );
        let tight = SnapshotWireLimits::new(bytes.len() - 1, 16).unwrap();
        let err = SnapshotWireError::InputTooLarge {
            len: bytes.len(),
            max: bytes.len() - 1,
        };
        assert_eq!(
            decode_snapshot_with_limits(&bytes, tight),
            Err(err.clone()),
            "{}",
            case.name
        );
        assert_eq!(
            encode_snapshot_with_limits(&build(&case).snapshot(), tight),
            Err(err),
            "{}",
            case.name
        );
    }
}

#[test]
fn golden_over_caller_row_limit_rejected() {
    let (_, bytes) = evicting();
    // R + E = 5 rows, S = 5 seen ids.
    let limits = SnapshotWireLimits::new(bytes.len(), 4).unwrap();
    assert!(matches!(
        decode_snapshot_with_limits(&bytes, limits),
        Err(SnapshotWireError::RowCountLimit { max: 4, .. })
    ));
    let limits = SnapshotWireLimits::new(bytes.len(), 5).unwrap();
    assert!(decode_snapshot_with_limits(&bytes, limits).is_ok());
}

#[test]
fn golden_with_oversized_count_prefixes_rejected_before_allocation() {
    let (_, golden_bytes) = evicting();
    for off in [OFF_RETAINED_COUNT, OFF_EVICTION_COUNT, OFF_SEEN_COUNT] {
        let mut bytes = golden_bytes.clone();
        bytes[off..off + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let err = decode_snapshot(&bytes).unwrap_err();
        assert!(
            matches!(
                err,
                SnapshotWireError::RowCountLimit { .. }
                    | SnapshotWireError::RetainedExceedsPolicyLimit { .. }
            ),
            "offset {off}: {err:?}"
        );
    }
}

#[test]
fn golden_retained_count_above_policy_capacity_rejected() {
    let (case, golden_bytes) = evicting();
    let cap = case.policy.limit();
    let mut bytes = golden_bytes;
    bytes[OFF_RETAINED_COUNT..OFF_RETAINED_COUNT + 4]
        .copy_from_slice(&((cap as u32) + 1).to_le_bytes());
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::RetainedExceedsPolicyLimit {
            retained: cap as u64 + 1,
            limit: cap,
        })
    );
}

#[test]
fn golden_seen_count_off_by_one_is_length_mismatch() {
    // A declared count that disagrees with the payload length is rejected by
    // the exact-length check (short count -> trailing, long count -> truncated).
    let (_, golden_bytes) = evicting();
    let mut bytes = golden_bytes.clone();
    bytes[OFF_SEEN_COUNT..OFF_SEEN_COUNT + 4].copy_from_slice(&4_u32.to_le_bytes());
    assert_eq!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::TrailingBytes {
            extra: WIRE_SEEN_ID_BYTES
        })
    );
    let mut bytes = golden_bytes;
    bytes[OFF_SEEN_COUNT..OFF_SEEN_COUNT + 4].copy_from_slice(&6_u32.to_le_bytes());
    assert!(matches!(
        decode_snapshot(&bytes),
        Err(SnapshotWireError::Truncated { .. })
    ));
}

#[test]
fn golden_semantic_tamper_passes_wire_but_fails_restore() {
    // Wire layer owns syntax only; a register flip is wire-valid but must be
    // refused by WorkingContext::restore.
    let (_, golden_bytes) = evicting();
    let mut bytes = golden_bytes;
    bytes[33] ^= 0x01; // recurrent register, low byte
    assert!(decode_snapshot(&bytes).is_ok());
    assert!(matches!(
        decode_and_restore(&bytes),
        Err(SnapshotWireError::Restore(
            WorkingContextError::RegisterMismatch { .. }
        ))
    ));
}

#[test]
fn empty_golden_matches_hand_written_header() {
    // Independent of the crate encoder: header-only v1 frame for Fixed{3}.
    let mut expected = Vec::with_capacity(WIRE_HEADER_BYTES);
    expected.extend_from_slice(b"SWC2");
    expected.extend_from_slice(&1_u16.to_le_bytes());
    expected.extend_from_slice(&1_u16.to_le_bytes());
    expected.extend_from_slice(&GOLDEN_STREAM.to_le_bytes());
    expected.push(1); // Fixed
    expected.extend_from_slice(&3_u64.to_le_bytes());
    expected.extend_from_slice(&[0_u8; 8 + 8 + 40 + 12]); // step, register, stats, counts
    assert_eq!(golden(&cases()[0]), expected);
}

#[test]
fn golden_rows_carry_hand_specified_provenance() {
    let (_, bytes) = evicting();
    let snap = decode_snapshot(&bytes).unwrap();
    let ids: Vec<u64> = snap
        .retained
        .iter()
        .map(|r| r.identity.segment_id)
        .collect();
    assert_eq!(ids, vec![103, 104, 105]);
    let evicted: Vec<u64> = snap
        .eviction_log
        .iter()
        .map(|r| r.identity.segment_id)
        .collect();
    assert_eq!(evicted, vec![101, 102]);
    assert_eq!(snap.seen_segment_ids, vec![101, 102, 103, 104, 105]);
    for row in snap.retained.iter().chain(snap.eviction_log.iter()) {
        let i = row.identity.segment_id - 100;
        assert_eq!(row.order_index, 10 * i);
        assert_eq!(row.fingerprint, golden_segment(i).fingerprint);
        assert_eq!(row.content_hash, golden_segment(i).content_hash);
    }
}
