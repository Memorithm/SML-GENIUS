//! CONTEXT-2 G3+G4 integration: wire decode -> `WorkingContext::restore`
//! parity and G3 fail-closed invariants reached through the G4 wire path.
//!
//! RemoteOps#1208 (ChatGPT comment 6708570670): before merging either #849
//! (G3) or #850 (G4), the combined semantics must be exercised on one head —
//! stats / zero-ID invariants and decode→restore parity.
//!
//! Synthetic, seeded, bounded streams only. Metrics NOT_MEASURED; LONGCTX-B1
//! BLOCKED; no long-context, quality or throughput claim.

use sml_core::context_ledger::ContextStreamId;
use sml_store::working_context::{
    synthetic_stream, RetentionFate, WorkingContext, WorkingContextError, WorkingContextSnapshot,
    WorkingPolicy, WorkingSegment,
};
use sml_store::working_context_wire::{
    decode_and_restore, decode_snapshot, encode_context, encode_snapshot, restore_into,
    SnapshotWireError, WireSection, WIRE_HEADER_BYTES, WIRE_ROW_BYTES, WIRE_SEEN_ID_BYTES,
};

// Frozen v1 header offsets (see `working_context_wire` module docs).
const OFF_STEP: usize = 25;
const OFF_REGISTER: usize = 33;
const OFF_STATS_ADMITTED: usize = 41;
const OFF_STATS_RETAINED: usize = 49;
const OFF_STATS_EVICTED: usize = 57;
const OFF_STATS_LOST: usize = 65;
const OFF_STATS_FOLDS: usize = 73;
const OFF_EVICTION_COUNT: usize = 85;
const ROW_FINGERPRINT: usize = 48;
const ROW_RESOLVED_FLAG: usize = 64;
const ROW_RESOLVED_STEP: usize = 65;

/// Seeds per policy family (bounded so the suite stays fast).
const SEEDS: u64 = 24;
const MAX_LEN: u64 = 12;

fn stream() -> ContextStreamId {
    ContextStreamId::new(1_208_850).unwrap()
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Fixed,
    Expanding,
    CapacityBounded,
}

const KINDS: [Kind; 3] = [Kind::Fixed, Kind::Expanding, Kind::CapacityBounded];

struct Fixture {
    seed: u64,
    policy: WorkingPolicy,
    segments: Vec<WorkingSegment>,
    ctx: WorkingContext,
}

fn fixture(kind: Kind, seed: u64) -> Fixture {
    let r = mix(seed ^ 0x0850_0849_6708_5706);
    let cap = 1 + (r % 4) as usize;
    let n = (r >> 8) % (MAX_LEN + 1);
    let policy = match kind {
        Kind::Fixed => WorkingPolicy::Fixed { capacity: cap },
        Kind::CapacityBounded => WorkingPolicy::CapacityBounded { capacity: cap },
        Kind::Expanding => WorkingPolicy::Expanding {
            hard_max: (n as usize).max(1) + ((r >> 24) % 3) as usize,
        },
    };
    let segments = synthetic_stream(stream(), n).unwrap();
    let mut ctx = WorkingContext::new(stream(), policy).unwrap();
    for s in &segments {
        assert_eq!(ctx.admit(s.clone()).unwrap(), RetentionFate::Retained);
    }
    Fixture {
        seed,
        policy,
        segments,
        ctx,
    }
}

fn all_fixtures() -> Vec<Fixture> {
    KINDS
        .iter()
        .flat_map(|&k| (0..SEEDS).map(move |s| fixture(k, s)))
        .collect()
}

fn fixtures_where(pred: impl Fn(&Fixture) -> bool) -> Vec<Fixture> {
    let out: Vec<Fixture> = all_fixtures().into_iter().filter(|f| pred(f)).collect();
    assert!(!out.is_empty(), "seeded corpus must exercise this family");
    out
}

fn next_segment(fx: &Fixture) -> WorkingSegment {
    synthetic_stream(stream(), fx.segments.len() as u64 + 1)
        .unwrap()
        .pop()
        .unwrap()
}

/// Bytes that decode at the wire layer must restore exactly as the decoded
/// snapshot restores directly (G4 adds no semantic acceptance of its own),
/// and a rejection must leave the live context untouched.
fn assert_wire_restore_rejects(
    fx: &Fixture,
    bad: &[u8],
    family: &str,
    expected: impl Fn(&WorkingContextError) -> bool,
) {
    let decoded = decode_snapshot(bad)
        .unwrap_or_else(|e| panic!("[{family}] seed {}: wire decode failed: {e:?}", fx.seed));
    let direct = WorkingContext::restore(&decoded).unwrap_err();
    assert!(
        expected(&direct),
        "[{family}] seed {} policy {:?}: unexpected direct error {direct:?}",
        fx.seed,
        fx.policy
    );
    assert_eq!(
        decode_and_restore(bad).unwrap_err(),
        SnapshotWireError::Restore(direct.clone()),
        "[{family}] wire path must surface the same G3 restore error"
    );
    assert_live_untouched(fx, bad, family);
}

fn assert_live_untouched(fx: &Fixture, bad: &[u8], family: &str) {
    let mut live = fx.ctx.clone();
    let before_bytes = encode_context(&live).unwrap();
    let err = restore_into(&mut live, bad).unwrap_err();
    assert_eq!(
        restore_into(&mut live, bad).unwrap_err(),
        err,
        "[{family}] rejection must be deterministic"
    );
    assert_eq!(live, fx.ctx, "[{family}] rejected bytes mutated live state");
    assert_eq!(encode_context(&live).unwrap(), before_bytes, "[{family}]");
    // Live context keeps working after the rejection.
    match live.admit(next_segment(fx)) {
        Ok(_) => {
            decode_and_restore(&encode_context(&live).unwrap()).unwrap();
        }
        Err(WorkingContextError::ExpandingCeiling { .. }) => assert_eq!(live, fx.ctx),
        Err(e) => panic!("[{family}] live admit after reject failed: {e:?}"),
    }
}

// ---------------------------------------------------------------------------
// Positive parity: decode -> restore == direct restore == live, all policies
// ---------------------------------------------------------------------------

#[test]
fn integrated_wire_decode_restore_parity_all_policies() {
    let mut per_kind = [0_usize; 3];
    for (ki, &kind) in KINDS.iter().enumerate() {
        for seed in 0..SEEDS {
            let fx = fixture(kind, seed);
            let snap = fx.ctx.snapshot();
            let bytes = encode_context(&fx.ctx).unwrap();
            assert_eq!(encode_snapshot(&snap).unwrap(), bytes);
            let decoded = decode_snapshot(&bytes).unwrap();
            assert_eq!(decoded, snap, "seed {seed} {kind:?}: decode != snapshot");
            let via_wire = decode_and_restore(&bytes).unwrap();
            let direct = WorkingContext::restore(&snap).unwrap();
            assert_eq!(via_wire, direct, "seed {seed} {kind:?}");
            assert_eq!(via_wire, fx.ctx, "seed {seed} {kind:?}");
            assert_eq!(via_wire.stats(), fx.ctx.stats());
            assert_eq!(via_wire.recurrent_register(), fx.ctx.recurrent_register());
            assert_eq!(via_wire.accounting(), fx.ctx.accounting());
            per_kind[ki] += 1;
        }
    }
    assert!(per_kind.iter().all(|&c| c == SEEDS as usize));
}

#[test]
fn integrated_cut_restart_through_wire_matches_fresh_run_all_policies() {
    for fx in all_fixtures() {
        for cut in 0..=fx.segments.len() {
            let mut prefix = WorkingContext::new(stream(), fx.policy).unwrap();
            for s in &fx.segments[..cut] {
                prefix.admit(s.clone()).unwrap();
            }
            let mut resumed = decode_and_restore(&encode_context(&prefix).unwrap()).unwrap();
            assert_eq!(
                resumed,
                WorkingContext::restore(&prefix.snapshot()).unwrap()
            );
            for s in &fx.segments[cut..] {
                let a = resumed.admit(s.clone());
                let b = prefix.admit(s.clone());
                assert_eq!(a, b, "seed {} cut {cut}: admit diverged", fx.seed);
                // Restart again through the wire after every step.
                resumed = decode_and_restore(&encode_context(&resumed).unwrap()).unwrap();
            }
            assert_eq!(resumed, fx.ctx, "seed {} cut {cut}", fx.seed);
        }
    }
}

#[test]
fn integrated_next_eviction_parity_after_wire_restore() {
    for fx in all_fixtures() {
        let mut fresh = fx.ctx.clone();
        let mut restored = decode_and_restore(&encode_context(&fx.ctx).unwrap()).unwrap();
        for extra in 1..=4_u64 {
            let seg = synthetic_stream(stream(), fx.segments.len() as u64 + extra)
                .unwrap()
                .pop()
                .unwrap();
            assert_eq!(
                fresh.admit(seg.clone()),
                restored.admit(seg),
                "seed {} policy {:?}",
                fx.seed,
                fx.policy
            );
            assert_eq!(fresh.retained_identities(), restored.retained_identities());
            assert_eq!(fresh.accounting(), restored.accounting());
            assert_eq!(fresh, restored);
        }
    }
}

// ---------------------------------------------------------------------------
// G3 invariants reached through wire bytes (decode OK, restore rejects)
// ---------------------------------------------------------------------------

#[test]
fn integrated_g3_stats_counter_overflow_via_wire_is_stats_mismatch() {
    for fx in fixtures_where(|f| f.ctx.step() >= 1) {
        let good = encode_context(&fx.ctx).unwrap();
        for off in [OFF_STATS_RETAINED, OFF_STATS_EVICTED, OFF_STATS_LOST] {
            let mut bad = good.clone();
            put_u64(&mut bad, off, u64::MAX);
            assert_wire_restore_rejects(&fx, &bad, "g3/stats-overflow", |e| {
                *e == WorkingContextError::StatsMismatch
            });
        }
        let mut bad = good;
        put_u64(&mut bad, OFF_STATS_ADMITTED, u64::MAX);
        assert_wire_restore_rejects(&fx, &bad, "g3/stats-admitted", |e| {
            matches!(
                e,
                WorkingContextError::StatsMismatch
                    | WorkingContextError::InconsistentStepChronology { .. }
            )
        });
    }
}

#[test]
fn integrated_g3_recurrent_folds_tamper_via_wire_rejected() {
    for fx in all_fixtures() {
        let good = encode_context(&fx.ctx).unwrap();
        for folds in [fx.ctx.stats().admitted + 1, u64::MAX] {
            let mut bad = good.clone();
            put_u64(&mut bad, OFF_STATS_FOLDS, folds);
            assert_wire_restore_rejects(&fx, &bad, "g3/folds", |e| {
                *e == WorkingContextError::StatsMismatch
            });
        }
    }
}

#[test]
fn integrated_g3_zero_fingerprint_via_wire_rejected() {
    for fx in fixtures_where(|f| f.ctx.retained_count() > 0) {
        let mut bad = encode_context(&fx.ctx).unwrap();
        put_u64(&mut bad, row_off(0) + ROW_FINGERPRINT, 0);
        assert_wire_restore_rejects(&fx, &bad, "g3/zero-fp-retained", |e| {
            *e == WorkingContextError::ZeroFingerprint
        });
    }
    for fx in fixtures_where(|f| !f.ctx.snapshot().eviction_log.is_empty()) {
        let r = fx.ctx.retained_count();
        let mut bad = encode_context(&fx.ctx).unwrap();
        put_u64(&mut bad, row_off(r) + ROW_FINGERPRINT, 0);
        assert_wire_restore_rejects(&fx, &bad, "g3/zero-fp-eviction", |e| {
            *e == WorkingContextError::ZeroFingerprint
        });
    }
}

#[test]
fn integrated_g3_register_and_step_tamper_via_wire_rejected() {
    for fx in all_fixtures() {
        let good = encode_context(&fx.ctx).unwrap();
        let mut bad = good.clone();
        put_u64(
            &mut bad,
            OFF_REGISTER,
            fx.ctx.recurrent_register() ^ (mix(fx.seed) | 1),
        );
        assert_wire_restore_rejects(&fx, &bad, "g3/register", |e| {
            matches!(e, WorkingContextError::RegisterMismatch { .. })
        });

        let mut bad = good;
        put_u64(&mut bad, OFF_STEP, u64::MAX);
        put_u64(&mut bad, OFF_STATS_ADMITTED, u64::MAX);
        assert_wire_restore_rejects(&fx, &bad, "g3/step-max", |e| {
            matches!(e, WorkingContextError::InconsistentStepChronology { .. })
        });
    }
}

#[test]
fn integrated_g3_zero_segment_id_rejected_on_both_layers() {
    // G3 rejects a forged zero segment id in `restore`; G4 rejects it earlier
    // at decode. Neither layer may admit it, and the encoder refuses to emit it.
    for fx in fixtures_where(|f| f.ctx.retained_count() > 0) {
        let snap = fx.ctx.snapshot();
        let rows = snap.retained.len() + snap.eviction_log.len();
        let old = snap.retained[0].identity.segment_id;

        let mut forged = snap.clone();
        forged.retained[0].identity.segment_id = 0;
        for id in &mut forged.seen_segment_ids {
            if *id == old {
                *id = 0;
            }
        }
        forged.seen_segment_ids.sort_unstable();
        assert_eq!(
            WorkingContext::restore(&forged).unwrap_err(),
            WorkingContextError::InvalidSegmentIdentity
        );
        assert!(matches!(
            encode_snapshot(&forged),
            Err(SnapshotWireError::InvalidSegmentId { .. })
        ));

        let mut bad = encode_context(&fx.ctx).unwrap();
        put_u64(&mut bad, row_off(0), 0);
        assert_eq!(
            decode_snapshot(&bad).unwrap_err(),
            SnapshotWireError::InvalidSegmentId { row: 0 }
        );
        assert_live_untouched(&fx, &bad, "g3+g4/zero-id-row");

        let mut bad = encode_context(&fx.ctx).unwrap();
        put_u64(&mut bad, seen_off(rows, 0), 0);
        assert_eq!(
            decode_snapshot(&bad).unwrap_err(),
            SnapshotWireError::InvalidSegmentId { row: rows }
        );
        assert_live_untouched(&fx, &bad, "g3+g4/zero-id-seen");
    }
}

// ---------------------------------------------------------------------------
// Negatives: malformed / canonical-violating wire never mutates store state
// ---------------------------------------------------------------------------

fn swap_rows(bytes: &mut [u8], a: usize, b: usize) {
    let (ra, rb) = (row_off(a), row_off(b));
    for k in 0..WIRE_ROW_BYTES {
        bytes.swap(ra + k, rb + k);
    }
}

#[test]
fn integrated_noncanonical_wire_rejects_without_mutation() {
    let mut families = 0_usize;
    for fx in all_fixtures() {
        let snap: WorkingContextSnapshot = fx.ctx.snapshot();
        let good = encode_context(&fx.ctx).unwrap();
        let r = snap.retained.len();
        let e = snap.eviction_log.len();
        let s = snap.seen_segment_ids.len();

        // Trailing byte.
        let mut bad = good.clone();
        bad.push(0);
        assert_eq!(
            decode_snapshot(&bad).unwrap_err(),
            SnapshotWireError::TrailingBytes { extra: 1 }
        );
        assert_live_untouched(&fx, &bad, "trailing");

        // Truncated by one byte.
        let bad = good[..good.len() - 1].to_vec();
        assert!(matches!(
            decode_snapshot(&bad),
            Err(SnapshotWireError::Truncated { .. })
        ));
        assert_live_untouched(&fx, &bad, "truncated");

        // Inflated eviction-log length prefix (length/limit gate).
        let mut bad = good.clone();
        bad[OFF_EVICTION_COUNT..OFF_EVICTION_COUNT + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_snapshot(&bad).is_err());
        assert_live_untouched(&fx, &bad, "inflated-prefix");

        if r >= 2 {
            // Retained FIFO order swapped: G4 canonical check fires first.
            let mut bad = good.clone();
            swap_rows(&mut bad, 0, 1);
            assert_eq!(
                decode_snapshot(&bad).unwrap_err(),
                SnapshotWireError::NoncanonicalOrder {
                    section: WireSection::Retained,
                    index: 1
                }
            );
            assert_live_untouched(&fx, &bad, "retained-order");
            families += 1;
        }
        if e >= 2 {
            // Eviction-log order is not semantic for G3 restore, but the wire
            // is canonical: a permuted log must still be refused at decode.
            let mut bad = good.clone();
            swap_rows(&mut bad, r, r + 1);
            assert_eq!(
                decode_snapshot(&bad).unwrap_err(),
                SnapshotWireError::NoncanonicalOrder {
                    section: WireSection::EvictionLog,
                    index: 1
                }
            );
            let mut permuted = snap.clone();
            permuted.eviction_log.swap(0, 1);
            assert_eq!(WorkingContext::restore(&permuted).unwrap(), fx.ctx);
            assert_live_untouched(&fx, &bad, "eviction-order");
            families += 1;
        }
        if s >= 2 {
            // Duplicate seen id.
            let rows = r + e;
            let first = snap.seen_segment_ids[0];
            let mut bad = good.clone();
            put_u64(&mut bad, seen_off(rows, 1), first);
            assert_eq!(
                decode_snapshot(&bad).unwrap_err(),
                SnapshotWireError::NoncanonicalOrder {
                    section: WireSection::SeenIds,
                    index: 1
                }
            );
            assert_live_untouched(&fx, &bad, "dup-seen");
        }
        if r >= 1 {
            // Non-canonical resolved padding on a Retained row.
            let mut bad = good.clone();
            put_u64(&mut bad, row_off(0) + ROW_RESOLVED_STEP, 7);
            assert_eq!(bad[row_off(0) + ROW_RESOLVED_FLAG], 0);
            assert_eq!(
                decode_snapshot(&bad).unwrap_err(),
                SnapshotWireError::NoncanonicalResolvedStep { row: 0 }
            );
            assert_live_untouched(&fx, &bad, "resolved-padding");
        }
    }
    assert!(families > 0, "corpus must exercise ordering families");
}

// ---------------------------------------------------------------------------
// Layering property: wire adds no semantic acceptance
// ---------------------------------------------------------------------------

#[test]
fn integrated_single_byte_mutations_wire_restore_equals_direct_restore() {
    let cases = [
        WorkingPolicy::Fixed { capacity: 2 },
        WorkingPolicy::Expanding { hard_max: 5 },
        WorkingPolicy::CapacityBounded { capacity: 2 },
    ];
    for policy in cases {
        let mut ctx = WorkingContext::new(stream(), policy).unwrap();
        for s in synthetic_stream(stream(), 4).unwrap() {
            ctx.admit(s).unwrap();
        }
        let good = encode_context(&ctx).unwrap();
        let mut checked = 0_usize;
        for pos in 0..good.len() {
            for mask in [0x01_u8, 0x80, 0xFF] {
                let mut m = good.clone();
                m[pos] ^= mask;
                let wire = decode_and_restore(&m);
                match decode_snapshot(&m) {
                    Ok(snap) => {
                        checked += 1;
                        let direct = WorkingContext::restore(&snap);
                        assert_eq!(
                            wire,
                            direct.map_err(SnapshotWireError::Restore),
                            "pos={pos} mask={mask:#x}"
                        );
                    }
                    Err(e) => assert_eq!(wire.unwrap_err(), e, "pos={pos} mask={mask:#x}"),
                }
            }
        }
        assert!(checked > 0);
    }
}
