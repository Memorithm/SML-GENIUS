#[path = "../examples/support/counterpart_c_store.rs"]
mod store;
use sml_store::{delta_kv_context::*, observed_history::*};
use store::World;
fn h(w: &World) -> VerifiedHistory<'_> {
    verify_history(
        &w.ledger,
        HistoryLimits {
            max_events: 16,
            max_keys: 8,
            max_element_bytes: 16384,
        },
        HistoryVerificationLimits {
            max_segments: 16,
            max_raw_bytes: 65536,
            max_record_bytes: MAX_EVENT_BYTES,
        },
        |r| Ok(w.raw[(r.identity.segment_id - 1) as usize].clone()),
    )
    .unwrap()
}
fn raw() -> Vec<Vec<u8>> {
    let s = SubjectKey::new(1, 0).unwrap();
    [
        ObservationEvent::new(1, s, [false; 2], None).unwrap(),
        ObservationEvent::new(2, s, [true; 2], None).unwrap(),
    ]
    .into_iter()
    .map(encode_event)
    .collect()
}
#[test]
fn a_valid_single_record_view_does_not_hide_another_subject_version() {
    let k = vec![4096; 16];
    let w = World::with_raw(&[1, 2], raw(), &[Some(k.clone()), Some(k.clone())], 16, 1);
    let r = w.receipt(&k).unwrap();
    let history = h(&w);
    for id in [1, 2] {
        let record = r
            .bind_record(
                RecordRequest::new(w.ledger.stream_id(), id),
                RecordBindingLimits {
                    max_sources: 16,
                    max_write_records: 16,
                },
            )
            .unwrap();
        assert_eq!(record.source_count(), 1);
    }
    let subject = bind_projected_subject(
        &r,
        &history,
        SubjectKey::new(1, 0).unwrap(),
        SubjectBindingLimits {
            max_candidate_sources: 16,
            max_record_bytes: MAX_EVENT_BYTES,
        },
    )
    .unwrap();
    assert_eq!(subject.observed_state(), ObservedState::Conflict);
    assert_eq!(subject.screen(Some([true; 2])).value(), None);
}
#[test]
fn record_success_cannot_upgrade_an_incomplete_subject_candidate_set() {
    let k = vec![4096; 16];
    let other = vec![-4096; 16];
    let w = World::with_raw(&[1, 2], raw(), &[Some(k.clone()), Some(other)], 16, 1);
    let r = w.receipt(&k).unwrap();
    let history = h(&w);
    assert!(r
        .bind_record(
            RecordRequest::new(w.ledger.stream_id(), 1),
            RecordBindingLimits {
                max_sources: 16,
                max_write_records: 16
            }
        )
        .is_ok());
    assert!(matches!(
        bind_projected_subject(
            &r,
            &history,
            SubjectKey::new(1, 0).unwrap(),
            SubjectBindingLimits {
                max_candidate_sources: 16,
                max_record_bytes: MAX_EVENT_BYTES
            }
        ),
        Err(SubjectBindingError::IncompleteSubject)
    ));
}
