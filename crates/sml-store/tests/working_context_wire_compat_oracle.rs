//! CONTEXT-2 G6: cross-version / restart compatibility oracle for the v1
//! working-context snapshot wire format (`SWC2`, wire version 1, schema 1).
//!
//! The oracle in this file is a second, independent reading of the layout
//! documented in `working_context_wire.rs`. It shares no code with the
//! production encoder/decoder: it parses and re-serializes bytes with its own
//! offsets and its own canonical-form rules, and is compared against the
//! production decoder on the frozen G5 golden corpus and on systematic
//! mutations of it (every prefix, single-byte flips, count inflation, version
//! bumps, torn writes between two committed snapshots).
//!
//! Properties checked:
//! - frozen corpus provenance: each G5 `.hex` fixture is pinned by length and
//!   SHA-256 (the hash detects accidental drift only; it is not an
//!   authenticity or signature mechanism);
//! - accept/reject agreement between oracle and production decoder, and
//!   decode → encode identity for every accepted input;
//! - unknown wire versions and schemas are rejected; nothing is migrated;
//! - restart: decode → restore → encode reproduces the exact bytes, and a
//!   restored context does not replay any admission;
//! - a failed decode never mutates live state (`restore_into` is atomic);
//! - interrupted writes (prefixes, torn splices of an old and a new
//!   snapshot) are either rejected or fully admitted, never half-applied;
//! - resource caps (byte and row limits) are enforced before allocation.
//!
//! Limitations: v1 is the only wire version that exists, so "cross-version"
//! means rejection of every other version tag, not migration. A torn write
//! that happens to produce a wire-valid, semantically-valid snapshot is
//! admitted (there is no checksum in v1). No model-quality, context-span,
//! latency or throughput claim is made (`NOT_MEASURED`).

use std::path::PathBuf;

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_core::elastic_ngram_paged_sha::sha256;
use sml_store::working_context::{WorkingContext, WorkingPolicy, WorkingSegment};
use sml_store::working_context_wire::{
    decode_and_restore, decode_snapshot, decode_snapshot_with_limits, encode_context,
    encode_snapshot, restore_into, SnapshotWireError, SnapshotWireLimits,
};

// ---------------------------------------------------------------------------
// Frozen corpus (G5 fixtures) with provenance pins.
// ---------------------------------------------------------------------------

struct Pinned {
    name: &'static str,
    len: usize,
    sha256_hex: &'static str,
}

/// Pins recorded from the G5 fixtures merged on `main` in SML-GENIUS #859
/// (merge commit 21d1f6f3a31648b59b283078c531902be80599bc).
const PINNED: &[Pinned] = &[
    Pinned {
        name: "fixed_cap3_empty",
        len: 93,
        sha256_hex: "dac52fd87fd6bc7086bfea7a3057186baf8ebbd973bb783d80d5fc689945317c",
    },
    Pinned {
        name: "fixed_cap2_lost",
        len: 421,
        sha256_hex: "67cdd8ef664064f4d1ca754c60ec043597a70ad3ef91e57a2a36e236519b3fef",
    },
    Pinned {
        name: "expanding_hm4_partial",
        len: 339,
        sha256_hex: "2c76f1fbbb863d234deab1b2ee69a9df8ce422a6728bdac615232175b858dd3f",
    },
    Pinned {
        name: "capacity_bounded_cap3_evicting",
        len: 503,
        sha256_hex: "80b61c57c34e2f5eba1f6501de5f3defcbe8dbb969854476c347967789191f50",
    },
];

fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/working_context_wire")
        .join(format!("{name}.hex"));
    let text = std::fs::read_to_string(&path).expect("read fixture");
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        assert!(line.len() % 2 == 0, "odd hex line in {name}");
        for k in (0..line.len()).step_by(2) {
            out.push(u8::from_str_radix(&line[k..k + 2], 16).expect("hex"));
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn corpus() -> Vec<(&'static str, Vec<u8>)> {
    PINNED
        .iter()
        .map(|p| (p.name, fixture_bytes(p.name)))
        .collect()
}

// ---------------------------------------------------------------------------
// Independent oracle (spec re-implementation, no production code).
// ---------------------------------------------------------------------------

const O_HEADER: usize = 93;
const O_ROW: usize = 74;
const O_SEEN: usize = 8;
const O_MAX_ROWS: u64 = 65_536;
const O_MAX_BYTES: usize = O_HEADER + 65_536 * (O_ROW + O_SEEN);

#[derive(Debug, Clone, PartialEq, Eq)]
struct ORow {
    segment_id: u64,
    order_index: u64,
    hash: [u8; 32],
    fingerprint: u64,
    admitted_step: u64,
    resolved: Option<u64>,
    fate: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OSnap {
    stream: u64,
    tag: u8,
    limit: u64,
    step: u64,
    register: u64,
    stats: [u64; 5],
    retained: Vec<ORow>,
    evicted: Vec<ORow>,
    seen: Vec<u64>,
}

fn le16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn le64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

fn oracle_row(b: &[u8], o: usize) -> Option<ORow> {
    let segment_id = le64(b, o);
    if segment_id == 0 {
        return None;
    }
    let flag = b[o + 64];
    let raw = le64(b, o + 65);
    let resolved = match flag {
        0 if raw == 0 => None,
        1 => Some(raw),
        _ => return None,
    };
    let fate = b[o + 73];
    if fate > 2 {
        return None;
    }
    Some(ORow {
        segment_id,
        order_index: le64(b, o + 8),
        hash: b[o + 16..o + 48].try_into().unwrap(),
        fingerprint: le64(b, o + 48),
        admitted_step: le64(b, o + 56),
        resolved,
        fate,
    })
}

fn strictly_increasing(keys: impl Iterator<Item = u64>) -> bool {
    let mut prev = None;
    for k in keys {
        if matches!(prev, Some(p) if k <= p) {
            return false;
        }
        prev = Some(k);
    }
    true
}

/// Wire-level acceptance per the v1 spec. `None` = reject.
fn oracle_parse(b: &[u8], max_bytes: usize, max_rows: u64) -> Option<OSnap> {
    if b.len() > max_bytes || b.len() < O_HEADER {
        return None;
    }
    if &b[0..4] != b"SWC2" || le16(b, 4) != 1 || le16(b, 6) != 1 {
        return None;
    }
    let stream = le64(b, 8);
    let tag = b[16];
    let limit = le64(b, 17);
    if stream == 0 || !(1..=3).contains(&tag) || limit == 0 || usize::try_from(limit).is_err() {
        return None;
    }
    let (r, e, s) = (
        u64::from(le32(b, 81)),
        u64::from(le32(b, 85)),
        u64::from(le32(b, 89)),
    );
    if r > max_rows || e > max_rows || r + e > max_rows || s > max_rows || r > limit {
        return None;
    }
    let need = O_HEADER as u64 + (r + e) * O_ROW as u64 + s * O_SEEN as u64;
    if b.len() as u64 != need {
        return None;
    }
    let mut rows = Vec::new();
    for i in 0..(r + e) as usize {
        rows.push(oracle_row(b, O_HEADER + i * O_ROW)?);
    }
    let evicted = rows.split_off(r as usize);
    let retained = rows;
    let seen_off = O_HEADER + (r + e) as usize * O_ROW;
    let seen: Vec<u64> = (0..s as usize)
        .map(|i| le64(b, seen_off + i * O_SEEN))
        .collect();
    if seen.contains(&0)
        || !strictly_increasing(retained.iter().map(|x| x.admitted_step))
        || !strictly_increasing(evicted.iter().map(|x| x.segment_id))
        || !strictly_increasing(seen.iter().copied())
    {
        return None;
    }
    let mut stats = [0_u64; 5];
    for (k, v) in stats.iter_mut().enumerate() {
        *v = le64(b, 41 + 8 * k);
    }
    Some(OSnap {
        stream,
        tag,
        limit,
        step: le64(b, 25),
        register: le64(b, 33),
        stats,
        retained,
        evicted,
        seen,
    })
}

fn oracle_default(b: &[u8]) -> Option<OSnap> {
    oracle_parse(b, O_MAX_BYTES, O_MAX_ROWS)
}

fn oracle_encode(s: &OSnap) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"SWC2");
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&s.stream.to_le_bytes());
    out.push(s.tag);
    for v in [s.limit, s.step, s.register] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in s.stats {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for n in [s.retained.len(), s.evicted.len(), s.seen.len()] {
        out.extend_from_slice(&(n as u32).to_le_bytes());
    }
    for row in s.retained.iter().chain(s.evicted.iter()) {
        out.extend_from_slice(&row.segment_id.to_le_bytes());
        out.extend_from_slice(&row.order_index.to_le_bytes());
        out.extend_from_slice(&row.hash);
        out.extend_from_slice(&row.fingerprint.to_le_bytes());
        out.extend_from_slice(&row.admitted_step.to_le_bytes());
        out.push(u8::from(row.resolved.is_some()));
        out.extend_from_slice(&row.resolved.unwrap_or(0).to_le_bytes());
        out.push(row.fate);
    }
    for id in &s.seen {
        out.extend_from_slice(&id.to_le_bytes());
    }
    out
}

/// Production wire decode followed by production encode.
fn prod_roundtrip(b: &[u8]) -> Result<Vec<u8>, SnapshotWireError> {
    encode_snapshot(&decode_snapshot(b)?)
}

/// Core agreement: oracle accepts iff production wire decode accepts, and
/// both reproduce the input bytes exactly when they accept.
fn assert_agree(b: &[u8], what: &str) {
    let o = oracle_default(b);
    let p = decode_snapshot(b);
    assert_eq!(
        o.is_some(),
        p.is_ok(),
        "{what}: oracle={} prod={:?}",
        o.is_some(),
        p.as_ref().err()
    );
    if let Some(o) = o {
        assert_eq!(oracle_encode(&o), b, "{what}: oracle not canonical");
        assert_eq!(prod_roundtrip(b).unwrap(), b, "{what}: prod not canonical");
    }
}

// ---------------------------------------------------------------------------
// Live contexts for restart / interruption tests (independent builder).
// ---------------------------------------------------------------------------

const LIVE_STREAM: u64 = 0x4736_0000_0000_0042;

fn live_segment(i: u64) -> WorkingSegment {
    let stream = ContextStreamId::new(LIVE_STREAM).unwrap();
    let identity = ContextSegmentId::new(stream, 1_000 + i).unwrap();
    let mut h = [0_u8; 32];
    for (j, x) in h.iter_mut().enumerate() {
        *x = (i as u8).wrapping_mul(17) ^ (j as u8);
    }
    WorkingSegment::new(
        identity,
        7 * i,
        ContextContentHash::from_sha256(h),
        i.wrapping_mul(0x9E37_79B9_7F4A_7C15),
    )
    .unwrap()
}

fn live(policy: WorkingPolicy, admissions: u64) -> WorkingContext {
    let stream = ContextStreamId::new(LIVE_STREAM).unwrap();
    let mut ctx = WorkingContext::new(stream, policy).unwrap();
    for i in 1..=admissions {
        ctx.admit(live_segment(i)).unwrap();
    }
    ctx
}

fn policies() -> Vec<WorkingPolicy> {
    vec![
        WorkingPolicy::Fixed { capacity: 2 },
        WorkingPolicy::Expanding { hard_max: 4 },
        WorkingPolicy::CapacityBounded { capacity: 3 },
    ]
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[test]
fn frozen_corpus_matches_provenance_pins() {
    for p in PINNED {
        let b = fixture_bytes(p.name);
        assert_eq!(b.len(), p.len, "{}: length drift", p.name);
        assert_eq!(hex(&sha256(&b)), p.sha256_hex, "{}: sha256 drift", p.name);
    }
}

#[test]
fn oracle_and_production_accept_corpus_identically() {
    for (name, b) in corpus() {
        let o = oracle_default(&b).unwrap_or_else(|| panic!("{name}: oracle rejects"));
        let p = decode_snapshot(&b).unwrap();
        assert_eq!(o.stream, p.stream_id.get(), "{name}");
        assert_eq!(o.step, p.step, "{name}");
        assert_eq!(o.register, p.recurrent_register, "{name}");
        assert_eq!(o.retained.len(), p.retained.len(), "{name}");
        assert_eq!(o.evicted.len(), p.eviction_log.len(), "{name}");
        assert_eq!(o.seen, p.seen_segment_ids, "{name}");
        for (orow, prow) in o.retained.iter().zip(&p.retained) {
            assert_eq!(orow.segment_id, prow.identity.segment_id, "{name}");
            assert_eq!(orow.admitted_step, prow.admitted_step, "{name}");
            assert_eq!(orow.resolved, prow.resolved_step, "{name}");
        }
        assert_agree(&b, name);
    }
}

#[test]
fn restart_reproduces_exact_bytes_without_replay() {
    for (name, b) in corpus() {
        let ctx = decode_and_restore(&b).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(encode_context(&ctx).unwrap(), b, "{name}: restart drift");
        // Restart twice: still identical, no step advance, no replayed admit.
        let again = decode_and_restore(&encode_context(&ctx).unwrap()).unwrap();
        assert_eq!(encode_context(&again).unwrap(), b, "{name}: second restart");
        let snap = again.snapshot();
        let o = oracle_default(&b).unwrap();
        assert_eq!(snap.step, o.step, "{name}: step replayed");
        assert_eq!(
            snap.stats.admitted, o.stats[0],
            "{name}: admission replayed"
        );
    }
}

#[test]
fn unknown_wire_versions_and_schemas_are_rejected_not_migrated() {
    for (name, b) in corpus() {
        for v in [0_u16, 2, 3, 0x0100, 0x7FFF, u16::MAX] {
            let mut m = b.clone();
            m[4..6].copy_from_slice(&v.to_le_bytes());
            assert!(oracle_default(&m).is_none(), "{name} v{v}");
            assert_eq!(
                decode_snapshot(&m),
                Err(SnapshotWireError::UnknownWireVersion { got: v }),
                "{name} v{v}"
            );
            let mut m = b.clone();
            m[6..8].copy_from_slice(&v.to_le_bytes());
            assert!(oracle_default(&m).is_none(), "{name} schema {v}");
            assert_eq!(
                decode_snapshot(&m),
                Err(SnapshotWireError::UnsupportedSchema { got: v }),
                "{name} schema {v}"
            );
        }
        let mut m = b.clone();
        m[0..4].copy_from_slice(b"SWC1");
        assert_eq!(decode_snapshot(&m), Err(SnapshotWireError::BadMagic));
        assert!(oracle_default(&m).is_none());
    }
}

#[test]
fn every_prefix_is_rejected_by_both() {
    for (name, b) in corpus() {
        for k in 0..b.len() {
            let p = &b[..k];
            assert!(oracle_default(p).is_none(), "{name} prefix {k}");
            assert!(
                matches!(decode_snapshot(p), Err(SnapshotWireError::Truncated { .. })),
                "{name} prefix {k}: {:?}",
                decode_snapshot(p)
            );
        }
    }
}

#[test]
fn trailing_bytes_are_rejected_by_both() {
    for (name, b) in corpus() {
        for extra in [1_usize, 7, 8, 74] {
            let mut m = b.clone();
            m.extend(std::iter::repeat_n(0_u8, extra));
            assert!(oracle_default(&m).is_none(), "{name} +{extra}");
            assert!(decode_snapshot(&m).is_err(), "{name} +{extra}");
        }
    }
}

#[test]
fn single_byte_mutations_agree_with_oracle() {
    for (name, b) in corpus() {
        for pos in 0..b.len() {
            for mask in [0x01_u8, 0x80, 0xFF] {
                let mut m = b.clone();
                m[pos] ^= mask;
                assert_agree(&m, &format!("{name} pos {pos} ^{mask:#04x}"));
            }
        }
    }
}

#[test]
fn inflated_length_prefixes_fail_before_allocation() {
    for (name, b) in corpus() {
        for off in [81_usize, 85, 89] {
            for v in [u32::MAX, 65_537, 65_536] {
                let mut m = b.clone();
                m[off..off + 4].copy_from_slice(&v.to_le_bytes());
                assert!(oracle_default(&m).is_none(), "{name} @{off}={v}");
                assert!(
                    matches!(
                        decode_snapshot(&m),
                        Err(SnapshotWireError::RowCountLimit { .. }
                            | SnapshotWireError::RetainedExceedsPolicyLimit { .. }
                            | SnapshotWireError::Truncated { .. })
                    ),
                    "{name} @{off}={v}: {:?}",
                    decode_snapshot(&m)
                );
            }
        }
    }
}

#[test]
fn tightened_limits_are_enforced_by_both() {
    for (name, b) in corpus() {
        let tight = SnapshotWireLimits::new(b.len() - 1, 65_536).unwrap();
        assert!(matches!(
            decode_snapshot_with_limits(&b, tight),
            Err(SnapshotWireError::InputTooLarge { .. })
        ));
        assert!(
            oracle_parse(&b, b.len() - 1, O_MAX_ROWS).is_none(),
            "{name}"
        );
        let o = oracle_default(&b).unwrap();
        let rows = (o.retained.len() + o.evicted.len()).max(o.seen.len());
        if rows > 1 {
            let tight = SnapshotWireLimits::new(b.len(), rows - 1).unwrap();
            assert!(matches!(
                decode_snapshot_with_limits(&b, tight),
                Err(SnapshotWireError::RowCountLimit { .. })
            ));
            assert!(
                oracle_parse(&b, b.len(), rows as u64 - 1).is_none(),
                "{name}"
            );
        }
    }
}

#[test]
fn failed_decode_never_mutates_live_state() {
    for (name, b) in corpus() {
        for policy in policies() {
            let mut target = live(policy, 3);
            let before = encode_context(&target).unwrap();
            for k in [0, 1, 50, b.len() - 1] {
                assert!(restore_into(&mut target, &b[..k]).is_err(), "{name} {k}");
                assert_eq!(encode_context(&target).unwrap(), before, "{name} {k}");
            }
            let mut bad = b.clone();
            bad[4] = 2;
            assert!(restore_into(&mut target, &bad).is_err());
            assert_eq!(encode_context(&target).unwrap(), before, "{name} v2");
        }
    }
}

#[test]
fn interrupted_writes_are_all_or_nothing() {
    for policy in policies() {
        // Two committed checkpoints of the same stream: before and after
        // local completion of two more admissions.
        let old = encode_context(&live(policy, 2)).unwrap();
        let new = encode_context(&live(policy, 4)).unwrap();
        assert_ne!(old, new);
        let mut images: Vec<Vec<u8>> = Vec::new();
        for k in 0..=new.len() {
            images.push(new[..k].to_vec()); // crash mid-write of new file
            let mut torn = old[..k.min(old.len())].to_vec(); // in-place overwrite
            torn.extend_from_slice(&new[k.min(new.len())..]);
            images.push(torn);
        }
        for (i, img) in images.iter().enumerate() {
            assert_agree(img, &format!("{policy:?} image {i}"));
            let mut target = live(policy, 1);
            let before = encode_context(&target).unwrap();
            match restore_into(&mut target, img) {
                Ok(()) => {
                    // Admitted only as a whole, canonical snapshot; the live
                    // state is exactly what the image encodes.
                    assert!(oracle_default(img).is_some(), "{policy:?} {i}");
                    assert_eq!(&encode_context(&target).unwrap(), img);
                }
                Err(_) => assert_eq!(encode_context(&target).unwrap(), before),
            }
        }
        // Completed checkpoints restore to their own state; restoring the old
        // one does not replay the two later admissions.
        let o = decode_and_restore(&old).unwrap();
        let n = decode_and_restore(&new).unwrap();
        assert_eq!(o.snapshot().stats.admitted, 2, "{policy:?}");
        assert_eq!(n.snapshot().stats.admitted, 4, "{policy:?}");
        assert_eq!(encode_context(&o).unwrap(), old);
        assert_eq!(encode_context(&n).unwrap(), new);
    }
}

#[test]
fn canonical_order_mutations_are_rejected_by_both() {
    for (name, b) in corpus() {
        let o = oracle_default(&b).unwrap();
        if o.retained.len() >= 2 {
            let mut s = o.clone();
            s.retained.swap(0, 1);
            let m = oracle_encode(&s);
            assert!(oracle_default(&m).is_none(), "{name}");
            assert!(decode_snapshot(&m).is_err(), "{name}");
        }
        if o.seen.len() >= 2 {
            let mut s = o.clone();
            s.seen.swap(0, 1);
            let m = oracle_encode(&s);
            assert!(oracle_default(&m).is_none(), "{name}");
            assert!(decode_snapshot(&m).is_err(), "{name}");
            let mut s = o.clone();
            s.seen[1] = s.seen[0]; // duplicate
            let m = oracle_encode(&s);
            assert!(oracle_default(&m).is_none(), "{name}");
            assert!(decode_snapshot(&m).is_err(), "{name}");
        }
        if o.evicted.len() >= 2 {
            let mut s = o.clone();
            s.evicted.swap(0, 1);
            let m = oracle_encode(&s);
            assert!(oracle_default(&m).is_none(), "{name}");
            assert!(decode_snapshot(&m).is_err(), "{name}");
        }
    }
}
