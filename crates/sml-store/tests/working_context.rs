//! CONTEXT-2: exact-oracle fixtures + negative tests for bounded working context.
//!
//! Synthetic streams only. No real-text quality claim. No LONGCTX-B1 measurement.

use std::collections::BTreeMap;

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_store::working_context::{
    synthetic_stream, RetentionFate, WorkingContext, WorkingContextError, WorkingContextOracle,
    WorkingContextSnapshot, WorkingPolicy, WorkingSegment, WORKING_CONTEXT_SCHEMA_VERSION,
};

fn stream() -> ContextStreamId {
    ContextStreamId::new(42).unwrap()
}

fn admit_all(
    ctx: &mut WorkingContext,
    segments: &[WorkingSegment],
) -> Result<(), WorkingContextError> {
    for seg in segments {
        let fate = ctx.admit(seg.clone())?;
        assert_eq!(fate, RetentionFate::Retained);
    }
    Ok(())
}

fn assert_matches_oracle(ctx: &WorkingContext, oracle: &WorkingContextOracle) {
    assert_eq!(ctx.recurrent_register(), oracle.recurrent_register);
    assert_eq!(ctx.stats(), oracle.stats);
    let retained: Vec<u64> = ctx
        .retained_identities()
        .iter()
        .map(|id| id.segment_id)
        .collect();
    assert_eq!(retained, oracle.retained_ids);
    let fates: BTreeMap<u64, RetentionFate> = ctx
        .accounting()
        .into_iter()
        .map(|row| (row.identity.segment_id, row.fate))
        .collect();
    assert_eq!(fates, oracle.fates);
}

#[test]
fn three_policies_diverge_on_identical_stream() {
    let segments = synthetic_stream(stream(), 6).unwrap();
    let policies = [
        WorkingPolicy::Fixed { capacity: 3 },
        WorkingPolicy::Expanding { hard_max: 16 },
        WorkingPolicy::CapacityBounded { capacity: 3 },
    ];
    let mut registers = Vec::new();
    let mut retained_sets = Vec::new();
    for policy in policies {
        let mut ctx = WorkingContext::new(stream(), policy).unwrap();
        admit_all(&mut ctx, &segments).unwrap();
        let oracle = WorkingContextOracle::evaluate(policy, &segments).unwrap();
        assert_matches_oracle(&ctx, &oracle);
        registers.push(ctx.recurrent_register());
        retained_sets.push(ctx.retained_identities());
    }
    // Fixed and CapacityBounded share capacity=3 so retained IDs match, but
    // fates differ (Lost vs Evicted) — checked below.
    assert_eq!(retained_sets[0], retained_sets[2]);
    assert_ne!(retained_sets[0], retained_sets[1]); // expanding keeps all 6
    assert_eq!(retained_sets[1].len(), 6);
    assert_eq!(retained_sets[0].len(), 3);

    // Fate divergence: Fixed marks overwritten as Lost; CapacityBounded Evicted.
    let mut fixed = WorkingContext::new(stream(), WorkingPolicy::Fixed { capacity: 3 }).unwrap();
    admit_all(&mut fixed, &segments).unwrap();
    let mut bounded =
        WorkingContext::new(stream(), WorkingPolicy::CapacityBounded { capacity: 3 }).unwrap();
    admit_all(&mut bounded, &segments).unwrap();
    let fixed_fates: BTreeMap<_, _> = fixed
        .accounting()
        .into_iter()
        .map(|r| (r.identity.segment_id, r.fate))
        .collect();
    let bounded_fates: BTreeMap<_, _> = bounded
        .accounting()
        .into_iter()
        .map(|r| (r.identity.segment_id, r.fate))
        .collect();
    assert_eq!(fixed_fates.get(&1), Some(&RetentionFate::Lost));
    assert_eq!(bounded_fates.get(&1), Some(&RetentionFate::Evicted));
    assert_eq!(fixed_fates.get(&4), Some(&RetentionFate::Retained));
    assert_eq!(bounded_fates.get(&4), Some(&RetentionFate::Retained));
    // Same retained fingerprints => identical XOR register.
    assert_eq!(registers[0], registers[2]);
    assert_ne!(registers[0], registers[1]);
}

#[test]
fn exact_oracle_fixture_capacity_bounded_window() {
    let segments = synthetic_stream(stream(), 5).unwrap();
    let policy = WorkingPolicy::CapacityBounded { capacity: 2 };
    let mut ctx = WorkingContext::new(stream(), policy).unwrap();
    admit_all(&mut ctx, &segments).unwrap();
    let oracle = WorkingContextOracle::evaluate(policy, &segments).unwrap();
    assert_matches_oracle(&ctx, &oracle);
    assert_eq!(oracle.retained_ids, vec![4, 5]);
    assert_eq!(oracle.fates.get(&1), Some(&RetentionFate::Evicted));
    assert_eq!(oracle.fates.get(&2), Some(&RetentionFate::Evicted));
    assert_eq!(oracle.fates.get(&3), Some(&RetentionFate::Evicted));
    assert_eq!(oracle.stats.admitted, 5);
    assert_eq!(oracle.stats.retained, 2);
    assert_eq!(oracle.stats.evicted, 3);
    assert_eq!(oracle.stats.lost, 0);
    // Provenance rows exist for every segment.
    assert_eq!(ctx.accounting().len(), 5);
    for row in ctx.accounting() {
        assert_eq!(row.identity.stream_id, stream());
        assert_ne!(row.fingerprint, 0);
        match row.fate {
            RetentionFate::Retained => assert!(row.resolved_step.is_none()),
            RetentionFate::Evicted | RetentionFate::Lost => {
                assert!(row.resolved_step.is_some());
            }
        }
    }
}

#[test]
fn exact_oracle_fixture_fixed_marks_lost() {
    let segments = synthetic_stream(stream(), 4).unwrap();
    let policy = WorkingPolicy::Fixed { capacity: 2 };
    let mut ctx = WorkingContext::new(stream(), policy).unwrap();
    admit_all(&mut ctx, &segments).unwrap();
    let oracle = WorkingContextOracle::evaluate(policy, &segments).unwrap();
    assert_matches_oracle(&ctx, &oracle);
    assert_eq!(oracle.retained_ids, vec![3, 4]);
    assert_eq!(oracle.fates.get(&1), Some(&RetentionFate::Lost));
    assert_eq!(oracle.fates.get(&2), Some(&RetentionFate::Lost));
    assert_eq!(oracle.stats.lost, 2);
    assert_eq!(oracle.stats.evicted, 0);
}

#[test]
fn exact_oracle_fixture_expanding_retains_all() {
    let segments = synthetic_stream(stream(), 7).unwrap();
    let policy = WorkingPolicy::Expanding { hard_max: 7 };
    let mut ctx = WorkingContext::new(stream(), policy).unwrap();
    admit_all(&mut ctx, &segments).unwrap();
    let oracle = WorkingContextOracle::evaluate(policy, &segments).unwrap();
    assert_matches_oracle(&ctx, &oracle);
    assert_eq!(oracle.retained_ids, (1..=7).collect::<Vec<_>>());
    assert!(oracle.fates.values().all(|f| *f == RetentionFate::Retained));
    assert_eq!(oracle.stats.evicted, 0);
    assert_eq!(oracle.stats.lost, 0);
}

#[test]
fn negative_invalid_capacity() {
    assert!(matches!(
        WorkingContext::new(stream(), WorkingPolicy::Fixed { capacity: 0 }),
        Err(WorkingContextError::InvalidCapacity)
    ));
    assert!(matches!(
        WorkingPolicy::CapacityBounded { capacity: 0 }.validate(),
        Err(WorkingContextError::InvalidCapacity)
    ));
}

#[test]
fn negative_segment_id_collision() {
    let segments = synthetic_stream(stream(), 2).unwrap();
    let mut ctx = WorkingContext::new(stream(), WorkingPolicy::Expanding { hard_max: 8 }).unwrap();
    ctx.admit(segments[0].clone()).unwrap();
    let err = ctx.admit(segments[0].clone()).unwrap_err();
    assert!(matches!(err, WorkingContextError::SegmentIdCollision(_)));
}

#[test]
fn negative_stream_mismatch() {
    let other = ContextStreamId::new(99).unwrap();
    let mut ctx = WorkingContext::new(stream(), WorkingPolicy::Fixed { capacity: 2 }).unwrap();
    let bad = WorkingSegment::new(
        ContextSegmentId::new(other, 1).unwrap(),
        0,
        ContextContentHash::from_sha256([1; 32]),
        7,
    )
    .unwrap();
    assert!(matches!(
        ctx.admit(bad),
        Err(WorkingContextError::StreamMismatch { .. })
    ));
}

#[test]
fn negative_expanding_ceiling() {
    let segments = synthetic_stream(stream(), 3).unwrap();
    let mut ctx = WorkingContext::new(stream(), WorkingPolicy::Expanding { hard_max: 2 }).unwrap();
    ctx.admit(segments[0].clone()).unwrap();
    ctx.admit(segments[1].clone()).unwrap();
    let err = ctx.admit(segments[2].clone()).unwrap_err();
    assert!(matches!(
        err,
        WorkingContextError::ExpandingCeiling {
            hard_max: 2,
            retained: 2
        }
    ));
    // Fail-closed: third ID not recorded as seen; retry would still collide
    // only after a successful admit — here seen stays at 2.
    assert_eq!(ctx.retained_count(), 2);
    assert_eq!(ctx.stats().admitted, 2);
}

#[test]
fn negative_zero_fingerprint() {
    let err = WorkingSegment::new(
        ContextSegmentId::new(stream(), 1).unwrap(),
        0,
        ContextContentHash::from_sha256([2; 32]),
        0,
    )
    .unwrap_err();
    assert!(matches!(err, WorkingContextError::ZeroFingerprint));
}

#[test]
fn snapshot_roundtrip_preserves_state() {
    let segments = synthetic_stream(stream(), 5).unwrap();
    let policy = WorkingPolicy::CapacityBounded { capacity: 3 };
    let mut ctx = WorkingContext::new(stream(), policy).unwrap();
    admit_all(&mut ctx, &segments).unwrap();
    let snap = ctx.snapshot();
    assert_eq!(snap.schema_version, WORKING_CONTEXT_SCHEMA_VERSION);
    let restored = WorkingContext::restore(&snap).unwrap();
    assert_eq!(restored.recurrent_register(), ctx.recurrent_register());
    assert_eq!(restored.stats(), ctx.stats());
    assert_eq!(restored.retained_identities(), ctx.retained_identities());
    assert_eq!(restored.accounting(), ctx.accounting());
    // Further admission after restore remains consistent with oracle on full stream+1.
    let extra = synthetic_stream(stream(), 6).unwrap().pop().unwrap();
    // segment 6
    let mut restored = WorkingContext::restore(&snap).unwrap();
    restored.admit(extra.clone()).unwrap();
    let mut all = segments;
    all.push(extra);
    let oracle = WorkingContextOracle::evaluate(policy, &all).unwrap();
    assert_matches_oracle(&restored, &oracle);
}

#[test]
fn negative_restart_truncated_retained() {
    let segments = synthetic_stream(stream(), 3).unwrap();
    let mut ctx =
        WorkingContext::new(stream(), WorkingPolicy::CapacityBounded { capacity: 3 }).unwrap();
    admit_all(&mut ctx, &segments).unwrap();
    let mut snap = ctx.snapshot();
    // Drop one retained provenance row but leave seen_segment_ids intact.
    let dropped = snap.retained.pop().unwrap();
    let err = WorkingContext::restore(&snap).unwrap_err();
    assert!(matches!(
        err,
        WorkingContextError::TruncatedProvenance { .. }
    ));
    // Unknown provenance: put dropped id only in seen, not in any log — already covered.
    // Explicit unknown: add a fake eviction with an id not in seen.
    let mut snap2 = ctx.snapshot();
    snap2
        .eviction_log
        .push(sml_store::working_context::WorkingSegmentProvenance {
            identity: ContextSegmentId::new(stream(), 99).unwrap(),
            order_index: 99,
            content_hash: ContextContentHash::from_sha256([9; 32]),
            fingerprint: 99,
            admitted_step: 1,
            resolved_step: Some(2),
            fate: RetentionFate::Evicted,
        });
    let err2 = WorkingContext::restore(&snap2).unwrap_err();
    assert!(matches!(
        err2,
        WorkingContextError::UnknownProvenance { segment_id: 99 }
            | WorkingContextError::TruncatedProvenance { .. }
            | WorkingContextError::SegmentIdCollision(_)
    ));
    // Ensure we actually exercised the dropped-id path (use dropped to avoid unused).
    assert_eq!(dropped.fate, RetentionFate::Retained);
}

#[test]
fn negative_restart_register_mismatch() {
    let segments = synthetic_stream(stream(), 2).unwrap();
    let mut ctx = WorkingContext::new(stream(), WorkingPolicy::Fixed { capacity: 2 }).unwrap();
    admit_all(&mut ctx, &segments).unwrap();
    let mut snap = ctx.snapshot();
    snap.recurrent_register ^= 0xdead_beef;
    assert!(matches!(
        WorkingContext::restore(&snap),
        Err(WorkingContextError::RegisterMismatch { .. })
    ));
}

#[test]
fn negative_restart_unsupported_schema() {
    let segments = synthetic_stream(stream(), 1).unwrap();
    let mut ctx = WorkingContext::new(stream(), WorkingPolicy::Expanding { hard_max: 4 }).unwrap();
    admit_all(&mut ctx, &segments).unwrap();
    let mut snap = ctx.snapshot();
    snap.schema_version = 0;
    assert!(matches!(
        WorkingContext::restore(&snap),
        Err(WorkingContextError::UnsupportedSchema { got: 0 })
    ));
}

#[test]
fn negative_restart_stats_mismatch() {
    let segments = synthetic_stream(stream(), 2).unwrap();
    let mut ctx = WorkingContext::new(stream(), WorkingPolicy::Expanding { hard_max: 4 }).unwrap();
    admit_all(&mut ctx, &segments).unwrap();
    let mut snap = ctx.snapshot();
    snap.stats.retained = 99;
    assert!(matches!(
        WorkingContext::restore(&snap),
        Err(WorkingContextError::StatsMismatch) | Err(WorkingContextError::InternalInvariant)
    ));
}

#[test]
fn snapshot_type_is_publicly_constructible_for_negative_fixtures() {
    // Ensures WorkingContextSnapshot fields stay reachable for adversarial tests.
    let snap = WorkingContextSnapshot {
        schema_version: WORKING_CONTEXT_SCHEMA_VERSION,
        stream_id: stream(),
        policy: WorkingPolicy::Fixed { capacity: 1 },
        step: 0,
        recurrent_register: 0,
        retained: vec![],
        eviction_log: vec![],
        stats: Default::default(),
        seen_segment_ids: vec![],
    };
    let restored = WorkingContext::restore(&snap).unwrap();
    assert_eq!(restored.retained_count(), 0);
}

// ---------------------------------------------------------------------------
// Codex hardenings (PR #845 review r4501101631 / r4501101640 / r4501101645).
// ---------------------------------------------------------------------------

#[test]
fn negative_restore_rejects_reordered_retained_fifo() {
    // r4501101631: capacity-2 snapshot [1,2] reversed must not restore, else
    // admitting 3 would retain [1,3] instead of the FIFO-correct [2,3].
    let segments = synthetic_stream(stream(), 3).unwrap();
    let policy = WorkingPolicy::CapacityBounded { capacity: 2 };
    let mut ctx = WorkingContext::new(stream(), policy).unwrap();
    admit_all(&mut ctx, &segments[..2]).unwrap();
    let mut snap = ctx.snapshot();
    assert_eq!(
        snap.retained
            .iter()
            .map(|r| r.identity.segment_id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    snap.retained.reverse();
    assert_eq!(
        WorkingContext::restore(&snap),
        Err(WorkingContextError::InconsistentFifoOrder)
    );
    // Same defect under Fixed policy.
    let mut fixed = WorkingContext::new(stream(), WorkingPolicy::Fixed { capacity: 2 }).unwrap();
    admit_all(&mut fixed, &segments[..2]).unwrap();
    let mut snap_fixed = fixed.snapshot();
    snap_fixed.retained.swap(0, 1);
    assert_eq!(
        WorkingContext::restore(&snap_fixed),
        Err(WorkingContextError::InconsistentFifoOrder)
    );
}

#[test]
fn negative_restore_rejects_inconsistent_step_chronology() {
    // r4501101640: step=0 after two admissions would make the next admission
    // reuse admitted_step 1.
    let segments = synthetic_stream(stream(), 2).unwrap();
    let mut ctx =
        WorkingContext::new(stream(), WorkingPolicy::CapacityBounded { capacity: 4 }).unwrap();
    admit_all(&mut ctx, &segments).unwrap();
    let base = ctx.snapshot();
    assert_eq!(base.step, 2);

    let mut snap = base.clone();
    snap.step = 0;
    assert!(matches!(
        WorkingContext::restore(&snap),
        Err(WorkingContextError::InconsistentStepChronology { .. })
    ));

    let mut snap_ahead = base.clone();
    snap_ahead.step = 5;
    assert!(matches!(
        WorkingContext::restore(&snap_ahead),
        Err(WorkingContextError::InconsistentStepChronology { .. })
    ));

    // Duplicate admitted_step with otherwise valid step.
    let mut snap_dup = base.clone();
    snap_dup.retained[1].admitted_step = 1;
    assert!(matches!(
        WorkingContext::restore(&snap_dup),
        Err(WorkingContextError::InconsistentFifoOrder
            | WorkingContextError::InconsistentStepChronology { .. })
    ));

    // admitted_step beyond step.
    let mut snap_future = base;
    snap_future.retained[1].admitted_step = 3;
    assert!(matches!(
        WorkingContext::restore(&snap_future),
        Err(WorkingContextError::InconsistentStepChronology { .. })
    ));
}

#[test]
fn negative_restore_rejects_inconsistent_resolution_step() {
    let segments = synthetic_stream(stream(), 4).unwrap();
    let mut ctx =
        WorkingContext::new(stream(), WorkingPolicy::CapacityBounded { capacity: 2 }).unwrap();
    admit_all(&mut ctx, &segments).unwrap();
    let base = ctx.snapshot();
    assert_eq!(base.eviction_log.len(), 2);
    let mut snap = base.clone();
    snap.eviction_log[0].resolved_step = Some(4);
    assert!(matches!(
        WorkingContext::restore(&snap),
        Err(WorkingContextError::InconsistentStepChronology { .. })
    ));
    let mut snap_fate = base;
    snap_fate.eviction_log[0].fate = RetentionFate::Lost;
    snap_fate.stats.evicted -= 1;
    snap_fate.stats.lost += 1;
    assert!(matches!(
        WorkingContext::restore(&snap_fate),
        Err(WorkingContextError::InconsistentStepChronology { .. })
    ));
}

#[test]
fn negative_admit_rejects_zero_fingerprint_without_mutation() {
    // r4501101645: public fields allow bypassing WorkingSegment::new.
    for policy in [
        WorkingPolicy::Fixed { capacity: 2 },
        WorkingPolicy::Expanding { hard_max: 4 },
        WorkingPolicy::CapacityBounded { capacity: 2 },
    ] {
        let segments = synthetic_stream(stream(), 2).unwrap();
        let mut ctx = WorkingContext::new(stream(), policy).unwrap();
        ctx.admit(segments[0].clone()).unwrap();
        let before = ctx.clone();
        let bad = WorkingSegment {
            identity: ContextSegmentId::new(stream(), 77).unwrap(),
            order_index: 1,
            content_hash: ContextContentHash::from_sha256([3; 32]),
            fingerprint: 0,
        };
        assert_eq!(ctx.admit(bad), Err(WorkingContextError::ZeroFingerprint));
        assert_eq!(ctx, before, "zero-fingerprint admit must not mutate state");
        assert_eq!(ctx.step(), 1);
        assert_eq!(ctx.stats().admitted, 1);
        // Segment id 77 was not marked seen: a valid segment with id 77 admits.
        let good = WorkingSegment::new(
            ContextSegmentId::new(stream(), 77).unwrap(),
            1,
            ContextContentHash::from_sha256([3; 32]),
            0x77,
        )
        .unwrap();
        ctx.admit(good).unwrap();
        // Exported snapshot still restores.
        WorkingContext::restore(&ctx.snapshot()).unwrap();
    }
}

#[test]
fn valid_snapshot_roundtrip_all_policies_after_hardening() {
    for (policy, n) in [
        (WorkingPolicy::Fixed { capacity: 3 }, 0_u64),
        (WorkingPolicy::Fixed { capacity: 3 }, 7),
        (WorkingPolicy::Expanding { hard_max: 8 }, 5),
        (WorkingPolicy::CapacityBounded { capacity: 1 }, 4),
        (WorkingPolicy::CapacityBounded { capacity: 3 }, 9),
    ] {
        let segments = synthetic_stream(stream(), n).unwrap();
        let mut ctx = WorkingContext::new(stream(), policy).unwrap();
        admit_all(&mut ctx, &segments).unwrap();
        let snap = ctx.snapshot();
        let restored = WorkingContext::restore(&snap).unwrap();
        assert_eq!(restored, ctx);
        assert_eq!(restored.snapshot(), snap);
    }
}

#[test]
fn restored_capacity_bounded_next_eviction_matches_fresh_oracle() {
    // Deterministic next-eviction oracle: restore mid-stream, keep admitting,
    // and compare against a fresh run on the full stream after every step.
    let policy = WorkingPolicy::CapacityBounded { capacity: 2 };
    let segments = synthetic_stream(stream(), 6).unwrap();
    for cut in 0..segments.len() {
        let mut ctx = WorkingContext::new(stream(), policy).unwrap();
        admit_all(&mut ctx, &segments[..cut]).unwrap();
        let mut restored = WorkingContext::restore(&ctx.snapshot()).unwrap();
        for k in cut..segments.len() {
            restored.admit(segments[k].clone()).unwrap();
            let oracle = WorkingContextOracle::evaluate(policy, &segments[..=k]).unwrap();
            assert_matches_oracle(&restored, &oracle);
            assert_eq!(restored.step(), (k + 1) as u64);
        }
    }
    // Explicit Codex scenario: [1,2] restored then admit 3 retains [2,3].
    let mut ctx = WorkingContext::new(stream(), policy).unwrap();
    admit_all(&mut ctx, &segments[..2]).unwrap();
    let mut restored = WorkingContext::restore(&ctx.snapshot()).unwrap();
    restored.admit(segments[2].clone()).unwrap();
    let ids: Vec<u64> = restored
        .retained_identities()
        .iter()
        .map(|id| id.segment_id)
        .collect();
    assert_eq!(ids, vec![2, 3]);
    let row1 = restored
        .accounting()
        .into_iter()
        .find(|r| r.identity.segment_id == 1)
        .unwrap();
    assert_eq!(row1.fate, RetentionFate::Evicted);
    assert_eq!(row1.admitted_step, 1);
    assert_eq!(row1.resolved_step, Some(3));
}
