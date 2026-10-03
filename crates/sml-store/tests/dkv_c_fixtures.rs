#[path = "../examples/support/counterpart_c.rs"]
mod counterpart_c;
use counterpart_c::*;
use sml_core::semantic::SemanticStateDelta;
#[test]
fn c_hamming_is_exact_and_generation_is_deterministic() {
    for seed in 0..8 {
        for fact in 0..12 {
            let s = random_state(seed, fact);
            assert_eq!(s, random_state(seed, fact));
            for h in HAMMING {
                let n = near_miss(seed, fact, h).unwrap();
                let d = SemanticStateDelta::between(&s, &n);
                assert_eq!(d.total(), h as u128);
                assert_eq!(d.ambiguity, 0);
                assert_eq!(n, near_miss(seed, fact, h).unwrap());
            }
        }
    }
}
#[test]
fn c_independent_queries_are_not_assumed_orthogonal() {
    for seed in 0..8 {
        for fact in 0..12 {
            let qs = queries(seed, fact);
            assert_eq!(qs.len(), 8);
            assert_eq!(qs.last().unwrap().0, "independent");
            for (kind, q) in qs {
                if kind != "written" {
                    for j in 0..12 {
                        assert_ne!(q, random_state(seed, j));
                    }
                }
            }
        }
    }
    assert!(near_miss(0, 0, 193).is_err());
    assert_eq!(near_miss(0, 0, 0).unwrap(), random_state(0, 0));
}
