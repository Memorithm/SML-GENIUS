//! CONTEXT-2 G3: adversarial snapshot parser/restore corpus.
//!
//! ChatGPT -> Grok handoffs on Memorithm/RemoteOps#1208 (comments 6706636166
//! and 6707274787). Seeded, bounded fixtures; at least one dedicated negative
//! test per fault family; every rejected restore is checked to leave no
//! partial live state; fresh-vs-restored next-eviction parity.
//!
//! Synthetic streams only. No quality or long-context measurement
//! (metrics NOT_MEASURED; LONGCTX-B1 BLOCKED). CCOS recall is not a 1M
//! attention window and nothing here claims one.

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_store::working_context::{
    synthetic_stream, RetentionFate, WorkingContext, WorkingContextError, WorkingContextSnapshot,
    WorkingPolicy, WorkingSegment, WorkingSegmentProvenance,
};

// ---------------------------------------------------------------------------
// Seeded bounded fixtures
// ---------------------------------------------------------------------------

/// Number of seeds per family. Bounded so the corpus stays fast and finite.
const SEEDS: u64 = 48;
/// Maximum synthetic stream length per fixture.
const MAX_LEN: u64 = 12;

fn stream() -> ContextStreamId {
    ContextStreamId::new(1208).unwrap()
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// One seeded fixture: policy, full stream, live context after admitting it.
#[derive(Debug, Clone)]
struct Fixture {
    seed: u64,
    policy: WorkingPolicy,
    segments: Vec<WorkingSegment>,
    ctx: WorkingContext,
}

impl Fixture {
    fn snapshot(&self) -> WorkingContextSnapshot {
        self.ctx.snapshot()
    }
}

/// Deterministic fixture for `seed`. Expanding fixtures size `hard_max` so the
/// stream never hits the ceiling.
fn fixture(seed: u64) -> Fixture {
    let r = mix(seed ^ 0x1208_6706_6361_6600);
    let cap = 1 + (r % 4) as usize;
    let n = (r >> 8) % (MAX_LEN + 1);
    let policy = match (r >> 16) % 3 {
        0 => WorkingPolicy::Fixed { capacity: cap },
        1 => WorkingPolicy::CapacityBounded { capacity: cap },
        _ => WorkingPolicy::Expanding {
            hard_max: (n as usize).max(1) + ((r >> 24) % 3) as usize,
        },
    };
    build(seed, policy, n)
}

fn build(seed: u64, policy: WorkingPolicy, n: u64) -> Fixture {
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

/// Fixtures filtered by a predicate on the valid snapshot.
fn fixtures_where(pred: impl Fn(&Fixture) -> bool) -> Vec<Fixture> {
    let out: Vec<Fixture> = (0..SEEDS).map(fixture).filter(|f| pred(f)).collect();
    assert!(!out.is_empty(), "seeded corpus must exercise this family");
    out
}

fn fifo_capacity(policy: WorkingPolicy) -> Option<usize> {
    match policy {
        WorkingPolicy::Fixed { capacity } | WorkingPolicy::CapacityBounded { capacity } => {
            Some(capacity)
        }
        WorkingPolicy::Expanding { .. } => None,
    }
}

/// Recompute register and counters so a mutation is caught by the targeted
/// structural check rather than incidentally by register/stats.
fn reseal(snap: &mut WorkingContextSnapshot) {
    snap.recurrent_register = snap.retained.iter().fold(0, |acc, r| acc ^ r.fingerprint);
    snap.stats.retained = snap.retained.len() as u64;
    snap.stats.evicted = snap
        .eviction_log
        .iter()
        .filter(|r| r.fate == RetentionFate::Evicted)
        .count() as u64;
    snap.stats.lost = snap
        .eviction_log
        .iter()
        .filter(|r| r.fate == RetentionFate::Lost)
        .count() as u64;
    snap.stats.admitted = snap.stats.retained + snap.stats.evicted + snap.stats.lost;
    snap.stats.recurrent_folds = snap.stats.admitted;
}

/// Assert `bad` is rejected with an error accepted by `expected`, and that the
/// rejection leaves no partial live state:
///
/// - `restore` is pure: the live fixture context is byte-for-byte unchanged;
/// - the rejection is deterministic (same error on retry);
/// - the pristine snapshot still restores to a context equal to the live one;
/// - the live context still admits a fresh segment and stays restorable.
fn assert_rejected(
    fx: &Fixture,
    bad: &WorkingContextSnapshot,
    family: &str,
    expected: impl Fn(&WorkingContextError) -> bool,
) {
    let live_before = fx.ctx.clone();
    let err = match WorkingContext::restore(bad) {
        Ok(_) => panic!(
            "[{family}] seed {} policy {:?}: adversarial snapshot restored",
            fx.seed, fx.policy
        ),
        Err(e) => e,
    };
    assert!(
        expected(&err),
        "[{family}] seed {} policy {:?}: unexpected error {err:?}",
        fx.seed,
        fx.policy
    );
    assert_eq!(
        WorkingContext::restore(bad).unwrap_err(),
        err,
        "[{family}] rejection must be deterministic"
    );
    assert_eq!(fx.ctx, live_before, "[{family}] live state mutated");

    let pristine = fx.snapshot();
    let restored = WorkingContext::restore(&pristine).unwrap();
    assert_eq!(
        restored, fx.ctx,
        "[{family}] valid restore broken after reject"
    );

    let mut live = fx.ctx.clone();
    let next = fresh_segment(fx.segments.len() as u64 + 1);
    match live.admit(next) {
        Ok(_) => {
            WorkingContext::restore(&live.snapshot()).unwrap();
        }
        Err(WorkingContextError::ExpandingCeiling { .. }) => {
            assert_eq!(live, fx.ctx, "ceiling refusal must not mutate");
        }
        Err(e) => panic!("[{family}] live admit after reject failed: {e:?}"),
    }
}

fn fresh_segment(id: u64) -> WorkingSegment {
    synthetic_stream(stream(), id).unwrap().pop().unwrap()
}

fn is_truncated(e: &WorkingContextError) -> bool {
    matches!(e, WorkingContextError::TruncatedProvenance { .. })
}

fn is_chronology(e: &WorkingContextError) -> bool {
    matches!(e, WorkingContextError::InconsistentStepChronology { .. })
}

// ---------------------------------------------------------------------------
// Family 1: truncated snapshots
// ---------------------------------------------------------------------------

#[test]
fn g3_truncated_drop_last_retained_row_rejected() {
    for fx in fixtures_where(|f| !f.snapshot().retained.is_empty()) {
        let mut bad = fx.snapshot();
        bad.retained.pop();
        assert_rejected(&fx, &bad, "truncated/retained", is_truncated);
        // Resealed register/stats: still caught structurally.
        reseal(&mut bad);
        assert_rejected(&fx, &bad, "truncated/retained/resealed", is_truncated);
    }
}

#[test]
fn g3_truncated_drop_eviction_log_row_rejected() {
    for fx in fixtures_where(|f| !f.snapshot().eviction_log.is_empty()) {
        let mut bad = fx.snapshot();
        bad.eviction_log.pop();
        reseal(&mut bad);
        assert_rejected(&fx, &bad, "truncated/eviction_log", is_truncated);
    }
}

#[test]
fn g3_truncated_seen_ids_rejected() {
    for fx in fixtures_where(|f| !f.snapshot().seen_segment_ids.is_empty()) {
        let mut bad = fx.snapshot();
        let dropped = bad.seen_segment_ids.pop().unwrap();
        assert_rejected(&fx, &bad, "truncated/seen", |e| {
            *e == WorkingContextError::UnknownProvenance {
                segment_id: dropped,
            }
        });
    }
}

#[test]
fn g3_truncated_prefix_cut_of_every_section_rejected() {
    // A snapshot cut mid-serialization: every section keeps only a prefix.
    for fx in fixtures_where(|f| f.snapshot().seen_segment_ids.len() >= 2) {
        let base = fx.snapshot();
        for keep in 0..base.seen_segment_ids.len() {
            let mut bad = base.clone();
            bad.retained.truncate(keep.min(bad.retained.len()));
            bad.eviction_log.truncate(keep.min(bad.eviction_log.len()));
            bad.seen_segment_ids.truncate(keep);
            assert_rejected(&fx, &bad, "truncated/prefix", |_| true);
        }
    }
}

// ---------------------------------------------------------------------------
// Family 2: oversized snapshots (retained exceeds policy)
// ---------------------------------------------------------------------------

fn append_forged_retained(snap: &mut WorkingContextSnapshot) {
    let id = snap.seen_segment_ids.iter().copied().max().unwrap_or(0) + 1;
    let seg = fresh_segment(id);
    snap.step += 1;
    snap.retained.push(WorkingSegmentProvenance {
        identity: seg.identity,
        order_index: seg.order_index,
        content_hash: seg.content_hash,
        fingerprint: seg.fingerprint,
        admitted_step: snap.step,
        resolved_step: None,
        fate: RetentionFate::Retained,
    });
    snap.seen_segment_ids.push(id);
    reseal(snap);
}

#[test]
fn g3_oversized_retained_beyond_policy_limit_rejected() {
    for fx in fixtures_where(|f| f.snapshot().retained.len() == f.policy.limit()) {
        let mut bad = fx.snapshot();
        append_forged_retained(&mut bad);
        assert_rejected(&fx, &bad, "oversized/append", |e| {
            *e == WorkingContextError::TruncatedProvenance {
                reason: "retained exceeds policy limit",
            }
        });
    }
}

#[test]
fn g3_oversized_policy_shrunk_below_retained_rejected() {
    for fx in fixtures_where(|f| f.snapshot().retained.len() >= 2) {
        let mut bad = fx.snapshot();
        let shrunk = bad.retained.len() - 1;
        bad.policy = match bad.policy {
            WorkingPolicy::Fixed { .. } => WorkingPolicy::Fixed { capacity: shrunk },
            WorkingPolicy::CapacityBounded { .. } => {
                WorkingPolicy::CapacityBounded { capacity: shrunk }
            }
            WorkingPolicy::Expanding { .. } => WorkingPolicy::Expanding { hard_max: shrunk },
        };
        assert_rejected(&fx, &bad, "oversized/shrunk", |e| {
            *e == WorkingContextError::TruncatedProvenance {
                reason: "retained exceeds policy limit",
            }
        });
    }
}

#[test]
fn g3_oversized_zero_capacity_policy_rejected() {
    let fx = fixture(0);
    let mut bad = fx.snapshot();
    bad.policy = WorkingPolicy::CapacityBounded { capacity: 0 };
    assert_rejected(&fx, &bad, "oversized/zero", |e| {
        *e == WorkingContextError::InvalidCapacity
    });
}

// ---------------------------------------------------------------------------
// Family 3: duplicate admission steps / duplicate segment coverage
// ---------------------------------------------------------------------------

#[test]
fn g3_duplicate_admission_step_rejected() {
    for fx in fixtures_where(|f| f.snapshot().step >= 2) {
        let base = fx.snapshot();
        let rows = base.retained.len() + base.eviction_log.len();
        for victim in 0..rows {
            let mut bad = base.clone();
            // Give row `victim` the admitted_step of a different row.
            let all: Vec<u64> = bad
                .retained
                .iter()
                .chain(bad.eviction_log.iter())
                .map(|r| r.admitted_step)
                .collect();
            let donor = all[(victim + 1) % rows];
            if victim < bad.retained.len() {
                bad.retained[victim].admitted_step = donor;
            } else {
                bad.eviction_log[victim - bad.retained.len()].admitted_step = donor;
            }
            assert_rejected(&fx, &bad, "duplicate/step", |e| {
                is_chronology(e) || *e == WorkingContextError::InconsistentFifoOrder
            });
        }
    }
}

#[test]
fn g3_duplicate_eviction_row_rejected() {
    for fx in fixtures_where(|f| !f.snapshot().eviction_log.is_empty()) {
        let mut bad = fx.snapshot();
        let dup = bad.eviction_log[0].clone();
        bad.eviction_log.push(dup.clone());
        assert_rejected(&fx, &bad, "duplicate/eviction", |e| {
            *e == WorkingContextError::SegmentIdCollision(dup.identity)
        });
    }
}

#[test]
fn g3_duplicate_seen_id_rejected() {
    for fx in fixtures_where(|f| !f.snapshot().seen_segment_ids.is_empty()) {
        let mut bad = fx.snapshot();
        let id = bad.seen_segment_ids[0];
        bad.seen_segment_ids.push(id);
        assert_rejected(
            &fx,
            &bad,
            "duplicate/seen",
            |e| matches!(e, WorkingContextError::SegmentIdCollision(s) if s.segment_id == id),
        );
    }
}

#[test]
fn g3_duplicate_segment_in_retained_and_eviction_rejected() {
    for fx in fixtures_where(|f| {
        let s = f.snapshot();
        !s.retained.is_empty() && !s.eviction_log.is_empty()
    }) {
        let mut bad = fx.snapshot();
        let dropped = bad.eviction_log[0].identity.segment_id;
        bad.eviction_log[0].identity = bad.retained[0].identity;
        bad.seen_segment_ids.retain(|id| *id != dropped);
        assert_rejected(&fx, &bad, "duplicate/coverage", |e| {
            matches!(e, WorkingContextError::SegmentIdCollision(_))
        });
    }
}

// ---------------------------------------------------------------------------
// Family 4: missing steps / gaps in step chronology
// ---------------------------------------------------------------------------

#[test]
fn g3_missing_step_row_removed_consistently_rejected() {
    // Remove one provenance row and every trace of its id, then reseal stats
    // and step so only the step-chronology gap remains.
    for fx in fixtures_where(|f| f.snapshot().step >= 2) {
        let base = fx.snapshot();
        let rows = base.retained.len() + base.eviction_log.len();
        for victim in 0..rows {
            let mut bad = base.clone();
            let removed = if victim < bad.retained.len() {
                bad.retained.remove(victim)
            } else {
                bad.eviction_log.remove(victim - bad.retained.len())
            };
            // Keep the newest admission so a gap (not a shorter stream) remains.
            if removed.admitted_step == base.step {
                continue;
            }
            bad.seen_segment_ids
                .retain(|id| *id != removed.identity.segment_id);
            reseal(&mut bad);
            bad.step = bad.stats.admitted;
            assert_rejected(&fx, &bad, "missing/gap", is_chronology);
        }
    }
}

#[test]
fn g3_missing_step_shifted_hole_rejected() {
    // Same row count, but one admission step pushed past `step` leaving a hole.
    for fx in fixtures_where(|f| f.snapshot().step >= 1) {
        let mut bad = fx.snapshot();
        let last = bad.retained.len() - 1;
        bad.retained[last].admitted_step = bad.step + 1;
        assert_rejected(&fx, &bad, "missing/hole", is_chronology);
    }
}

#[test]
fn g3_missing_step_counter_mismatch_rejected() {
    for fx in fixtures_where(|f| f.snapshot().step >= 1) {
        for delta in [-1_i64, 1] {
            let mut bad = fx.snapshot();
            bad.step = bad.step.wrapping_add_signed(delta);
            assert_rejected(&fx, &bad, "missing/step-counter", is_chronology);
        }
    }
}

// ---------------------------------------------------------------------------
// Family 5: out-of-order steps
// ---------------------------------------------------------------------------

#[test]
fn g3_out_of_order_retained_rotation_rejected() {
    for fx in fixtures_where(|f| f.snapshot().retained.len() >= 2) {
        let base = fx.snapshot();
        for k in 1..base.retained.len() {
            let mut bad = base.clone();
            bad.retained.rotate_left(k);
            assert_rejected(&fx, &bad, "out-of-order/rotate", |e| {
                *e == WorkingContextError::InconsistentFifoOrder
            });
        }
    }
}

#[test]
fn g3_out_of_order_evicted_newer_than_retained_rejected() {
    // Swap admission steps between an evicted row and a retained row: an
    // evicted segment claims to be newer than a resident.
    for fx in fixtures_where(|f| {
        let s = f.snapshot();
        !s.retained.is_empty() && !s.eviction_log.is_empty()
    }) {
        let mut bad = fx.snapshot();
        let r = bad.retained[0].admitted_step;
        let e = bad.eviction_log[0].admitted_step;
        bad.retained[0].admitted_step = e;
        bad.eviction_log[0].admitted_step = r;
        assert_rejected(&fx, &bad, "out-of-order/swap", |e| {
            is_chronology(e) || *e == WorkingContextError::InconsistentFifoOrder
        });
    }
}

#[test]
fn g3_out_of_order_resolved_before_admitted_rejected() {
    for fx in fixtures_where(|f| !f.snapshot().eviction_log.is_empty()) {
        let mut bad = fx.snapshot();
        let row = &mut bad.eviction_log[0];
        row.resolved_step = Some(row.admitted_step.saturating_sub(1));
        assert_rejected(&fx, &bad, "out-of-order/resolved", is_chronology);
    }
}

#[test]
fn g3_eviction_log_row_order_is_not_semantic() {
    // Positive control: eviction_log is a set keyed by segment id; permuting
    // it restores to the identical canonical state.
    for fx in fixtures_where(|f| f.snapshot().eviction_log.len() >= 2) {
        let mut perm = fx.snapshot();
        perm.eviction_log.reverse();
        perm.seen_segment_ids.reverse();
        let restored = WorkingContext::restore(&perm).unwrap();
        assert_eq!(restored, fx.ctx);
        assert_eq!(restored.snapshot(), fx.snapshot());
    }
}

// ---------------------------------------------------------------------------
// Family 6: overflow (step / counter / capacity arithmetic)
// ---------------------------------------------------------------------------

#[test]
fn g3_overflow_resolution_arithmetic_rejected() {
    // admitted_step + capacity overflows u64 under a usize::MAX FIFO capacity.
    let base = build(9001, WorkingPolicy::Fixed { capacity: 2 }, 3);
    let mut bad = base.snapshot();
    bad.policy = WorkingPolicy::Fixed {
        capacity: usize::MAX,
    };
    // Make the evicted row resolve at a legal step so only overflow remains.
    bad.eviction_log[0].admitted_step = u64::MAX;
    assert_rejected(&base, &bad, "overflow/huge-admitted", is_chronology);

    let mut bad2 = base.snapshot();
    bad2.policy = WorkingPolicy::Fixed {
        capacity: usize::MAX,
    };
    assert_rejected(&base, &bad2, "overflow/capacity", |e| {
        *e == WorkingContextError::StepOverflow
    });
}

#[test]
fn g3_overflow_step_at_u64_max_rejected() {
    for fx in fixtures_where(|_| true) {
        let mut bad = fx.snapshot();
        bad.step = u64::MAX;
        bad.stats.admitted = u64::MAX;
        assert_rejected(&fx, &bad, "overflow/step-max", is_chronology);
    }
}

#[test]
fn g3_overflow_stats_counters_rejected_as_stats_mismatch() {
    // Counter overflow must be reported as snapshot stats corruption, not as
    // an internal invariant failure.
    for fx in fixtures_where(|f| f.snapshot().step >= 1) {
        for field in 0..3 {
            let mut bad = fx.snapshot();
            match field {
                0 => bad.stats.retained = u64::MAX,
                1 => bad.stats.evicted = u64::MAX,
                _ => bad.stats.lost = u64::MAX,
            }
            assert_rejected(&fx, &bad, "overflow/stats", |e| {
                *e == WorkingContextError::StatsMismatch
            });
        }
    }
}

#[test]
fn g3_recurrent_folds_tamper_rejected() {
    for fx in fixtures_where(|_| true) {
        for folds in [fx.snapshot().stats.admitted + 1, u64::MAX] {
            let mut bad = fx.snapshot();
            bad.stats.recurrent_folds = folds;
            assert_rejected(&fx, &bad, "overflow/folds", |e| {
                *e == WorkingContextError::StatsMismatch
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Family 7: policy / fate mismatch
// ---------------------------------------------------------------------------

#[test]
fn g3_policy_fate_retained_row_with_resolved_fate_rejected() {
    for fx in fixtures_where(|f| !f.snapshot().retained.is_empty()) {
        for fate in [RetentionFate::Evicted, RetentionFate::Lost] {
            let mut bad = fx.snapshot();
            bad.retained[0].fate = fate;
            assert_rejected(&fx, &bad, "fate/retained", |e| {
                *e == WorkingContextError::TruncatedProvenance {
                    reason: "retained row has non-retained fate",
                }
            });
        }
        let mut bad = fx.snapshot();
        bad.retained[0].resolved_step = Some(bad.step);
        assert_rejected(&fx, &bad, "fate/retained-resolved", |e| {
            *e == WorkingContextError::TruncatedProvenance {
                reason: "retained row has resolved_step",
            }
        });
    }
}

#[test]
fn g3_policy_fate_eviction_row_with_retained_fate_rejected() {
    for fx in fixtures_where(|f| !f.snapshot().eviction_log.is_empty()) {
        let mut bad = fx.snapshot();
        bad.eviction_log[0].fate = RetentionFate::Retained;
        assert_rejected(&fx, &bad, "fate/eviction-retained", |e| {
            *e == WorkingContextError::TruncatedProvenance {
                reason: "eviction_log row not Evicted/Lost",
            }
        });
        let mut bad = fx.snapshot();
        bad.eviction_log[0].resolved_step = None;
        assert_rejected(&fx, &bad, "fate/eviction-unresolved", |e| {
            *e == WorkingContextError::TruncatedProvenance {
                reason: "eviction_log missing resolved_step",
            }
        });
    }
}

#[test]
fn g3_policy_fate_wrong_fate_for_policy_rejected() {
    // Fixed must produce Lost, CapacityBounded must produce Evicted.
    for fx in fixtures_where(|f| !f.snapshot().eviction_log.is_empty()) {
        let mut bad = fx.snapshot();
        for row in &mut bad.eviction_log {
            row.fate = match row.fate {
                RetentionFate::Lost => RetentionFate::Evicted,
                _ => RetentionFate::Lost,
            };
        }
        reseal(&mut bad);
        assert_rejected(&fx, &bad, "fate/policy-swap-rows", |e| {
            *e == WorkingContextError::InconsistentStepChronology {
                reason: "eviction fate does not match policy",
            }
        });
        // Swap the policy kind instead of the rows.
        let mut bad = fx.snapshot();
        bad.policy = match bad.policy {
            WorkingPolicy::Fixed { capacity } => WorkingPolicy::CapacityBounded { capacity },
            WorkingPolicy::CapacityBounded { capacity } => WorkingPolicy::Fixed { capacity },
            e @ WorkingPolicy::Expanding { .. } => e,
        };
        assert_rejected(&fx, &bad, "fate/policy-swap-kind", is_chronology);
    }
}

#[test]
fn g3_policy_fate_capacity_changed_with_history_rejected() {
    for fx in fixtures_where(|f| !f.snapshot().eviction_log.is_empty()) {
        let cap = fifo_capacity(fx.policy).unwrap();
        let mut bad = fx.snapshot();
        bad.policy = match bad.policy {
            WorkingPolicy::Fixed { .. } => WorkingPolicy::Fixed { capacity: cap + 1 },
            WorkingPolicy::CapacityBounded { .. } => {
                WorkingPolicy::CapacityBounded { capacity: cap + 1 }
            }
            e @ WorkingPolicy::Expanding { .. } => e,
        };
        assert_rejected(&fx, &bad, "fate/capacity-change", |e| {
            *e == WorkingContextError::InconsistentStepChronology {
                reason: "resolved_step != admitted_step + capacity",
            }
        });
        // Re-labelling a FIFO history as Expanding is also refused.
        let mut bad = fx.snapshot();
        bad.policy = WorkingPolicy::Expanding { hard_max: 64 };
        assert_rejected(&fx, &bad, "fate/expanding-relabel", |e| {
            *e == WorkingContextError::InconsistentStepChronology {
                reason: "expanding policy never resolves a row",
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Family 8: fingerprint = 0 (and zero identity) in snapshot rows
// ---------------------------------------------------------------------------

#[test]
fn g3_zero_fingerprint_in_retained_row_rejected() {
    for fx in fixtures_where(|f| !f.snapshot().retained.is_empty()) {
        let mut bad = fx.snapshot();
        bad.retained[0].fingerprint = 0;
        reseal(&mut bad);
        assert_rejected(&fx, &bad, "zero-fp/retained", |e| {
            *e == WorkingContextError::ZeroFingerprint
        });
    }
}

#[test]
fn g3_zero_fingerprint_in_eviction_row_rejected() {
    for fx in fixtures_where(|f| !f.snapshot().eviction_log.is_empty()) {
        let mut bad = fx.snapshot();
        bad.eviction_log[0].fingerprint = 0;
        assert_rejected(&fx, &bad, "zero-fp/eviction", |e| {
            *e == WorkingContextError::ZeroFingerprint
        });
    }
}

#[test]
fn g3_zero_segment_id_in_snapshot_rows_rejected() {
    // ContextSegmentId fields are public, so a forged row can carry
    // segment_id 0 which ContextSegmentId::new refuses.
    for fx in fixtures_where(|f| !f.snapshot().retained.is_empty()) {
        let mut bad = fx.snapshot();
        let old = bad.retained[0].identity.segment_id;
        bad.retained[0].identity = ContextSegmentId {
            stream_id: stream(),
            segment_id: 0,
        };
        for id in &mut bad.seen_segment_ids {
            if *id == old {
                *id = 0;
            }
        }
        assert_rejected(&fx, &bad, "zero-id/retained", |e| {
            *e == WorkingContextError::InvalidSegmentIdentity
        });
    }
    for fx in fixtures_where(|f| !f.snapshot().eviction_log.is_empty()) {
        let mut bad = fx.snapshot();
        let old = bad.eviction_log[0].identity.segment_id;
        bad.eviction_log[0].identity.segment_id = 0;
        for id in &mut bad.seen_segment_ids {
            if *id == old {
                *id = 0;
            }
        }
        assert_rejected(&fx, &bad, "zero-id/eviction", |e| {
            *e == WorkingContextError::InvalidSegmentIdentity
        });
    }
}

#[test]
fn g3_foreign_stream_rows_rejected() {
    let other = ContextStreamId::new(1344).unwrap();
    for fx in fixtures_where(|f| !f.snapshot().retained.is_empty()) {
        let mut bad = fx.snapshot();
        bad.retained[0].identity.stream_id = other;
        assert_rejected(&fx, &bad, "stream/retained", |e| {
            matches!(e, WorkingContextError::StreamMismatch { .. })
        });
    }
    for fx in fixtures_where(|f| !f.snapshot().eviction_log.is_empty()) {
        let mut bad = fx.snapshot();
        bad.eviction_log[0].identity.stream_id = other;
        assert_rejected(&fx, &bad, "stream/eviction", |e| {
            matches!(e, WorkingContextError::StreamMismatch { .. })
        });
    }
}

#[test]
fn g3_register_tamper_rejected() {
    for fx in fixtures_where(|_| true) {
        let mut bad = fx.snapshot();
        bad.recurrent_register ^= mix(fx.seed) | 1;
        assert_rejected(&fx, &bad, "register", |e| {
            matches!(e, WorkingContextError::RegisterMismatch { .. })
        });
    }
}

// ---------------------------------------------------------------------------
// Family 9: restart / replay determinism + next-eviction parity
// ---------------------------------------------------------------------------

#[test]
fn g3_replay_restore_at_every_cut_matches_fresh_run() {
    for seed in 0..SEEDS {
        let fx = fixture(seed);
        for cut in 0..=fx.segments.len() {
            let mut prefix = WorkingContext::new(stream(), fx.policy).unwrap();
            for s in &fx.segments[..cut] {
                prefix.admit(s.clone()).unwrap();
            }
            let snap = prefix.snapshot();
            let mut restored = WorkingContext::restore(&snap).unwrap();
            // Idempotent: restore -> snapshot is the identity on valid input.
            assert_eq!(restored.snapshot(), snap, "seed {seed} cut {cut}");
            for s in &fx.segments[cut..] {
                restored.admit(s.clone()).unwrap();
            }
            assert_eq!(restored, fx.ctx, "seed {seed} cut {cut}: replay diverged");
            assert_eq!(restored.snapshot(), fx.snapshot());
        }
    }
}

#[test]
fn g3_replay_chained_restarts_are_deterministic() {
    // Restart after every single admission; must equal the uninterrupted run.
    for seed in 0..SEEDS {
        let fx = fixture(seed);
        let mut ctx = WorkingContext::new(stream(), fx.policy).unwrap();
        for s in &fx.segments {
            ctx = WorkingContext::restore(&ctx.snapshot()).unwrap();
            ctx.admit(s.clone()).unwrap();
        }
        let ctx = WorkingContext::restore(&ctx.snapshot()).unwrap();
        assert_eq!(ctx, fx.ctx, "seed {seed}");
    }
}

#[test]
fn g3_fresh_vs_restored_next_eviction_parity_all_policies() {
    for seed in 0..SEEDS {
        let fx = fixture(seed);
        let next = fresh_segment(fx.segments.len() as u64 + 1);
        let mut fresh = fx.ctx.clone();
        let mut restored = WorkingContext::restore(&fx.snapshot()).unwrap();
        let a = fresh.admit(next.clone());
        let b = restored.admit(next);
        assert_eq!(a, b, "seed {seed}: next admission outcome diverged");
        assert_eq!(
            fresh, restored,
            "seed {seed}: post-admission state diverged"
        );
        // Victim identity / fate / resolution step parity.
        let victim = |c: &WorkingContext| {
            c.accounting()
                .into_iter()
                .filter(|r| r.resolved_step == Some(c.step()))
                .collect::<Vec<_>>()
        };
        assert_eq!(victim(&fresh), victim(&restored), "seed {seed}");
        if let Some(cap) = fifo_capacity(fx.policy) {
            if fx.segments.len() >= cap {
                let v = victim(&fresh);
                assert_eq!(v.len(), 1, "seed {seed}: exactly one victim");
                let expected_victim = fx.segments.len() + 1 - cap;
                assert_eq!(v[0].identity.segment_id, expected_victim as u64);
            }
        }
    }
}

#[test]
fn g3_expanding_ceiling_parity_after_restore() {
    for hard_max in 1..=4_usize {
        let fx = build(
            7000 + hard_max as u64,
            WorkingPolicy::Expanding { hard_max },
            hard_max as u64,
        );
        let next = fresh_segment(hard_max as u64 + 1);
        let mut fresh = fx.ctx.clone();
        let mut restored = WorkingContext::restore(&fx.snapshot()).unwrap();
        let a = fresh.admit(next.clone()).unwrap_err();
        let b = restored.admit(next).unwrap_err();
        assert_eq!(a, b);
        assert_eq!(fresh, fx.ctx, "ceiling refusal mutated fresh");
        assert_eq!(restored, fx.ctx, "ceiling refusal mutated restored");
    }
}

#[test]
fn g3_corpus_is_seeded_and_covers_all_policies() {
    // Guard: the seeded corpus is deterministic and exercises every policy
    // with and without evictions, so family tests cannot silently go vacuous.
    let a: Vec<WorkingContextSnapshot> = (0..SEEDS).map(|s| fixture(s).snapshot()).collect();
    let b: Vec<WorkingContextSnapshot> = (0..SEEDS).map(|s| fixture(s).snapshot()).collect();
    assert_eq!(a, b);
    let kinds = |k: &str| a.iter().filter(|s| s.policy.kind_name() == k).count();
    assert!(kinds("fixed") > 0 && kinds("capacity_bounded") > 0 && kinds("expanding") > 0);
    assert!(a
        .iter()
        .any(|s| s.policy.kind_name() == "fixed" && !s.eviction_log.is_empty()));
    assert!(a
        .iter()
        .any(|s| s.policy.kind_name() == "capacity_bounded" && !s.eviction_log.is_empty()));
    assert!(a.iter().any(|s| s.step == 0));
    assert!(a.iter().all(|s| s.step <= MAX_LEN));
    let _ = ContextContentHash::from_sha256([0; 32]);
}
