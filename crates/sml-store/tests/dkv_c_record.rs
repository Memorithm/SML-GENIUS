#[path = "../examples/support/counterpart_c_store.rs"]
mod store;
use sml_core::context_ledger::ContextStreamId;
use sml_store::delta_kv_context::*;
use store::World;
fn limits() -> RecordBindingLimits {
    RecordBindingLimits {
        max_write_records: 16,
        max_sources: 16,
    }
}
#[test]
fn record_view_excludes_other_records_on_the_same_full_key() {
    let k = vec![4096; 16];
    let w = World::new(&[7, 9], &[Some(k.clone()), Some(k.clone())], 16, 1);
    let r = w.receipt(&k).unwrap();
    assert_eq!(r.sources().len(), 2);
    let view = r
        .bind_record(RecordRequest::new(w.ledger.stream_id(), 7), limits())
        .unwrap();
    assert_eq!(view.source_count(), 1);
    assert_eq!(view.matching_writes(), 1);
    assert_eq!(view.inspected_counts(), (2, 2));
    assert_eq!(view.generation(), 2);
    assert_eq!(view.key(), k);
    assert_eq!(view.sources().next().unwrap().bytes(), b"fact 7");
    assert_eq!(view.request().record_id(), 7);
    assert_eq!(view.request().stream_id(), w.ledger.stream_id());
    assert!(matches!(
        r.bind_record(RecordRequest::new(w.ledger.stream_id(), 8), limits()),
        Err(RecordBindingError::NoMatchingWrite)
    ));
}
#[test]
fn record_view_is_scoped_to_stream_and_full_candidate_budgets() {
    let k = vec![4096; 16];
    let w = World::new(&[7, 9], &[Some(k.clone()), Some(k.clone())], 16, 1);
    let r = w.receipt(&k).unwrap();
    assert!(matches!(
        r.bind_record(
            RecordRequest::new(ContextStreamId::new(2).unwrap(), 7),
            limits()
        ),
        Err(RecordBindingError::ForeignStream)
    ));
    for l in [
        RecordBindingLimits {
            max_write_records: 1,
            ..limits()
        },
        RecordBindingLimits {
            max_sources: 1,
            ..limits()
        },
        RecordBindingLimits {
            max_sources: 0,
            ..limits()
        },
    ] {
        assert!(matches!(
            r.bind_record(RecordRequest::new(w.ledger.stream_id(), 7), l),
            Err(RecordBindingError::Limits)
        ));
    }
}
#[test]
fn every_source_of_a_repeated_record_stays_in_the_view() {
    let k = vec![4096; 16];
    let w = World::new(&[7, 7], &[Some(k.clone()), Some(k.clone())], 16, 1);
    let r = w.receipt(&k).unwrap();
    let view = r
        .bind_record(RecordRequest::new(w.ledger.stream_id(), 7), limits())
        .unwrap();
    assert_eq!(view.source_count(), 2);
    assert_eq!(view.matching_writes(), 2);
    assert_eq!(view.sources().count(), 2);
}
