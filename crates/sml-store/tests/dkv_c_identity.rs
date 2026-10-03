#[path = "../examples/support/counterpart_c.rs"]
mod counterpart_c;
#[path = "../examples/support/counterpart_c_identity.rs"]
mod identity;
use counterpart_c::*;
use identity::*;
#[test]
fn full_index_distinguishes_written_and_every_unwritten_fixture() {
    for seed in 0..8 {
        let pairs: Vec<_> = (0..12)
            .map(|j| {
                let s = random_state(seed, j);
                let id = legacy_record_id(&s);
                (s, id)
            })
            .collect();
        let index = StateIndex::new(&pairs, 12).unwrap();
        assert_eq!(index.element_bytes(), 480);
        for fact in 0..12 {
            for (kind, q) in queries(seed, fact) {
                let (id, work) = index.lookup(&q);
                assert!(work > 0);
                assert_eq!(
                    id,
                    if kind == "written" {
                        Some(legacy_record_id(&q))
                    } else {
                        None
                    }
                );
            }
        }
    }
}
#[test]
fn duplicate_state_record_and_capacity_reject() {
    let a = random_state(0, 0);
    let b = random_state(0, 1);
    assert!(StateIndex::new(&[(a.clone(), 1), (a.clone(), 2)], 2).is_err());
    assert!(StateIndex::new(&[(a.clone(), 1), (b, 1)], 2).is_err());
    assert!(StateIndex::new(&[(a, 1)], 0).is_err());
}
