#[path = "../examples/support/counterpart_b.rs"]
mod fixture;
use fixture::{distractor_state, project, target_state, RouteMode, Work};
use sml_core::context_ledger::{
    ContextSegmentId, ContextSegmentRef, ContextStreamId, RawContextLedger,
};
use sml_store::{
    delta_kv_context::{ContextLimits, DeltaKvContextMemory, VerificationLimits},
    observed_history::*,
    raw_context::sha256_raw_bytes,
};

struct World {
    ledger: RawContextLedger,
    raw: Vec<Vec<u8>>,
    memory: DeltaKvContextMemory,
}
fn subject(n: u64) -> SubjectKey {
    SubjectKey::new(n, 0).unwrap()
}
fn event(id: u64, s: u64, bits: [bool; 2], prior: Option<u64>) -> ObservationEvent {
    ObservationEvent::new(id, subject(s), bits, prior).unwrap()
}
fn hlimits() -> HistoryLimits {
    HistoryLimits {
        max_events: 16,
        max_keys: 8,
        max_element_bytes: 16384,
    }
}
fn vlimits() -> HistoryVerificationLimits {
    HistoryVerificationLimits {
        max_segments: 16,
        max_raw_bytes: 4096,
        max_record_bytes: MAX_EVENT_BYTES,
    }
}
fn limits() -> SubjectBindingLimits {
    SubjectBindingLimits {
        max_candidate_sources: 16,
        max_record_bytes: MAX_EVENT_BYTES,
    }
}
fn make(events: &[ObservationEvent], keys: &[Vec<i16>]) -> World {
    assert_eq!(events.len(), keys.len());
    let stream = ContextStreamId::new(1).unwrap();
    let mut ledger = RawContextLedger::new(stream);
    let mut raw = Vec::new();
    for (i, e) in events.iter().enumerate() {
        let bytes = encode_event(*e);
        let id = ContextSegmentId::new(stream, i as u64 + 1).unwrap();
        let reference = ContextSegmentRef::new(
            id,
            ledger.stats().raw_bytes,
            bytes.len() as u64,
            0,
            0,
            i as u64,
            sha256_raw_bytes(&bytes).unwrap(),
            vec![e.id()],
        )
        .unwrap();
        ledger.append(reference).unwrap();
        raw.push(bytes);
    }
    let mut memory = DeltaKvContextMemory::new(
        stream,
        16,
        2,
        ContextLimits {
            max_writes: 16,
            max_sources_per_write: 4,
            max_source_links: 32,
            max_ledger_segments: 16,
            max_ledger_semantic_links: 32,
            max_element_bytes: 1 << 20,
            max_staging_bytes: 1 << 20,
        },
    )
    .unwrap();
    for (i, e) in events.iter().enumerate() {
        memory
            .write(
                &ledger,
                e.id(),
                &keys[i],
                &e.bits().map(|b| if b { 4096 } else { -4096 }),
                4096,
                &[ledger.segments()[i].identity],
            )
            .unwrap();
    }
    World {
        ledger,
        raw,
        memory,
    }
}
fn history(w: &World) -> VerifiedHistory<'_> {
    verify_history(&w.ledger, hlimits(), vlimits(), |r| {
        Ok(w.raw[(r.identity.segment_id - 1) as usize].clone())
    })
    .unwrap()
}
fn receipt<'a>(w: &'a World, key: &[i16]) -> sml_store::delta_kv_context::VerifiedContext<'a> {
    w.memory
        .query(key)
        .unwrap()
        .verify(
            &w.ledger,
            VerificationLimits {
                max_sources: 16,
                max_raw_bytes: 4096,
            },
            |r| Ok(w.raw[(r.identity.segment_id - 1) as usize].clone()),
        )
        .unwrap()
}
fn alias() -> Vec<i16> {
    for seed in 0..8 {
        let a = target_state(seed);
        let key = project::<16>(
            &a,
            seed,
            RouteMode::NativeFixed,
            false,
            &mut Work::default(),
        )
        .unwrap()
        .unwrap()
        .key_q12;
        for d in 0..5 {
            let b = distractor_state(seed, d, 1);
            assert_ne!(a, b);
            let other = project::<16>(
                &b,
                seed,
                RouteMode::NativeFixed,
                false,
                &mut Work::default(),
            )
            .unwrap()
            .unwrap()
            .key_q12;
            if key == other {
                return key;
            }
        }
    }
    panic!("supplied known alias fixture changed; investigate without retuning")
}
#[test]
fn real_alias_receipt_is_not_evidence_of_unwritten_subject() {
    let key = alias();
    let w = make(&[event(1, 2, [false; 2], None)], std::slice::from_ref(&key));
    let h = history(&w);
    let r = receipt(&w, &key);
    assert_eq!(r.sources().len(), 1); // bytes really verify, but belong to subject2.
    assert!(matches!(
        bind_projected_subject(&r, &h, subject(1), limits()),
        Err(SubjectBindingError::MissingSubject)
    ));
    let screened = screen_candidate(&h, subject(1), Some([true; 2]));
    assert_eq!(screened.value(), None);
    assert_eq!(screened.original(), Some([true; 2]));
}
#[test]
fn unrelated_alias_sources_are_excluded_without_replacing_latent_answer() {
    let key = alias();
    let w = make(
        &[event(1, 1, [true; 2], None), event(2, 2, [false; 2], None)],
        &[key.clone(), key.clone()],
    );
    let h = history(&w);
    let r = receipt(&w, &key);
    let b = bind_projected_subject(&r, &h, subject(1), limits()).unwrap();
    assert_eq!(b.subject(), subject(1));
    assert_eq!(b.projected_key(), key);
    assert_eq!(b.matched_sources(), 1);
    assert_eq!(b.excluded_sources(), 1);
    assert_eq!(b.observed_state(), ObservedState::Agreement);
    assert_eq!(
        screen_candidate(&h, subject(1), Some([false; 2])).value(),
        Some([false; 2])
    );
}
#[test]
fn conflicting_versions_remain_conflicting_despite_projected_aliases() {
    let key = alias();
    let w = make(
        &[
            event(1, 1, [false; 2], None),
            event(2, 1, [true; 2], None),
            event(3, 2, [true; 2], None),
        ],
        &[key.clone(), key.clone(), key.clone()],
    );
    let h = history(&w);
    let r = receipt(&w, &key);
    let b = bind_projected_subject(&r, &h, subject(1), limits()).unwrap();
    assert_eq!(b.matched_sources(), 2);
    assert_eq!(b.observed_state(), ObservedState::Conflict);
    assert_eq!(
        screen_candidate(&h, subject(1), Some([true; 2])).value(),
        None
    );
}
#[test]
fn omitted_matching_version_rejects_even_if_remaining_value_agrees() {
    let key = alias();
    let mut different = key.clone();
    different[0] = if different[0] == 0 { 4096 } else { 0 };
    let w = make(
        &[
            event(1, 1, [true; 2], None),
            event(2, 1, [true; 2], Some(1)),
        ],
        &[key.clone(), different],
    );
    let h = history(&w);
    let r = receipt(&w, &key);
    assert!(matches!(
        bind_projected_subject(&r, &h, subject(1), limits()),
        Err(SubjectBindingError::IncompleteSubject)
    ));
}
#[test]
fn same_count_fork_and_invalid_budgets_cannot_bind() {
    let key = alias();
    let a = make(&[event(1, 1, [true; 2], None)], std::slice::from_ref(&key));
    let b = make(&[event(1, 1, [false; 2], None)], std::slice::from_ref(&key));
    let r = receipt(&a, &key);
    let h = history(&b);
    assert!(matches!(
        bind_projected_subject(&r, &h, subject(1), limits()),
        Err(SubjectBindingError::SnapshotMismatch)
    ));
    let h = history(&a);
    assert!(matches!(
        bind_projected_subject(
            &r,
            &h,
            subject(1),
            SubjectBindingLimits {
                max_candidate_sources: 0,
                ..limits()
            }
        ),
        Err(SubjectBindingError::Limits)
    ));
    assert!(matches!(
        bind_projected_subject(
            &r,
            &h,
            subject(1),
            SubjectBindingLimits {
                max_record_bytes: 1,
                ..limits()
            }
        ),
        Err(SubjectBindingError::Wire(_))
    ));
}
#[test]
fn explicit_supersession_retains_all_subject_versions() {
    let key = alias();
    let w = make(
        &[
            event(1, 1, [false; 2], None),
            event(2, 1, [true; 2], Some(1)),
        ],
        &[key.clone(), key.clone()],
    );
    let h = history(&w);
    let r = receipt(&w, &key);
    let b = bind_projected_subject(&r, &h, subject(1), limits()).unwrap();
    assert_eq!(b.matched_sources(), 2);
    assert_eq!(b.generation(), 2);
    assert_eq!(b.observed_state(), ObservedState::Agreement);
}

#[test]
fn bound_screen_preserves_subject_and_every_original_candidate() {
    let key = alias();
    let w = make(
        &[event(1, 1, [true; 2], None), event(2, 2, [false; 2], None)],
        &[key.clone(), key.clone()],
    );
    let h = history(&w);
    let r = receipt(&w, &key);
    for candidate in [
        None,
        Some([false; 2]),
        Some([false, true]),
        Some([true, false]),
        Some([true; 2]),
    ] {
        let bound = bind_projected_subject(&r, &h, subject(1), limits()).unwrap();
        assert_eq!(bound.excluded_sources(), 1);
        let screened = bound.screen(candidate);
        assert_eq!(screened.key(), subject(1));
        assert_eq!(screened.generation(), h.generation());
        assert_eq!(screened.original(), candidate);
        assert_eq!(screened.value(), candidate);
        assert_eq!(
            screened.reason(),
            screen_candidate(&h, subject(1), candidate).reason()
        );
    }
}

#[test]
fn bound_screen_cannot_use_an_agreeing_alias_to_hide_subject_conflict() {
    let key = alias();
    let w = make(
        &[
            event(1, 1, [false; 2], None),
            event(2, 1, [true; 2], None),
            event(3, 2, [true; 2], None),
        ],
        &[key.clone(), key.clone(), key.clone()],
    );
    let h = history(&w);
    let r = receipt(&w, &key);
    for candidate in [
        None,
        Some([false; 2]),
        Some([false, true]),
        Some([true, false]),
        Some([true; 2]),
    ] {
        let bound = bind_projected_subject(&r, &h, subject(1), limits()).unwrap();
        let screened = bound.screen(candidate);
        assert_eq!(screened.key(), subject(1));
        assert_eq!(screened.value(), None);
        assert_eq!(screened.original(), candidate);
        assert_eq!(screened.reason(), ScreeningReason::ConflictingObservations);
    }
    let other = bind_projected_subject(&r, &h, subject(2), limits())
        .unwrap()
        .screen(Some([false; 2]));
    assert_eq!(other.key(), subject(2));
    assert_eq!(other.value(), Some([false; 2]));
}

#[test]
fn bound_screen_retains_supersession_and_does_not_fill_unknown() {
    let key = alias();
    let w = make(
        &[
            event(1, 1, [false; 2], None),
            event(2, 1, [true; 2], Some(1)),
        ],
        &[key.clone(), key.clone()],
    );
    let h = history(&w);
    let r = receipt(&w, &key);
    let bound = bind_projected_subject(&r, &h, subject(1), limits()).unwrap();
    assert_eq!(bound.matched_sources(), 2);
    let screened = bound.screen(None);
    assert_eq!(screened.value(), None);
    assert_eq!(screened.reason(), ScreeningReason::CandidateUnknown);
    let bound = bind_projected_subject(&r, &h, subject(1), limits()).unwrap();
    let screened = bound.screen(Some([false; 2]));
    assert_eq!(screened.value(), Some([false; 2]));
    assert_eq!(screened.reason(), ScreeningReason::ObservedAgreement);
}
