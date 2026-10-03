#[path = "../examples/support/counterpart_c.rs"]
mod counterpart_c;
#[path = "../examples/support/counterpart_c_identity.rs"]
mod identity;
#[path = "../examples/support/counterpart_c_store.rs"]
mod store;
use counterpart_c::*;
use identity::*;
use sml_store::delta_kv_context::*;
use store::World;
#[test]
fn legacy_surrogate_omits_ambiguity_and_is_not_complete_state_identity() {
    let s = random_state(0, 0);
    let rid = legacy_record_id(&s);
    let index = StateIndex::new(&[(s.clone(), rid)], 1).unwrap();
    for bit in 0..64 {
        let mut changed = s.clone();
        changed.ambiguity.set(bit, true).unwrap();
        assert_eq!(legacy_record_id(&changed), rid);
        assert_ne!(canonical(&changed), canonical(&s));
        assert_eq!(index.lookup(&changed).0, None);
    }
    // Outside C's original all-A-zero panel. This is not a discovered FNV collision
    // over its 192-bit input; it is an explicit full-state contract distinction.
}
#[test]
fn raw_corruption_fails_before_a_record_view_can_exist() {
    let k = vec![4096; 16];
    let mut w = World::new(&[7], &[Some(k.clone())], 16, 1);
    w.raw[0][0] = b'F';
    assert!(matches!(w.receipt(&k), Err(ContextError::SourceMismatch)));
}
#[test]
fn same_record_id_never_substitutes_for_the_requested_full_key() {
    let k = vec![4096; 16];
    let other = vec![-4096; 16];
    let w = World::new(&[7], &[Some(k)], 16, 1);
    assert!(matches!(
        w.receipt(&other),
        Err(ContextError::NoWriteEvidence)
    ));
    assert!(matches!(
        w.receipt(&[4096]),
        Err(ContextError::ShapeMismatch)
    ));
}
