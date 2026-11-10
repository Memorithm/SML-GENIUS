use sml_decision::dec4_instr::{held_out, seen, Intent};
use sml_decision::dec8_fresh::fresh;

const ALL: [Intent; 5] = [
    Intent::Continues,
    Intent::NotContinues,
    Intent::Opening,
    Intent::Middle,
    Intent::End,
];

#[test]
fn fresh_wordings_are_disjoint_from_every_earlier_set() {
    let mut old: Vec<&str> = Vec::new();
    for i in ALL {
        old.extend(seen(i));
        old.extend(held_out(i));
    }
    let mut all_fresh: Vec<&str> = Vec::new();
    for i in ALL {
        assert!(!fresh(i).is_empty());
        for q in fresh(i) {
            assert!(!old.contains(q), "{q} was already used");
            assert!(!all_fresh.contains(q), "{q} appears twice");
            all_fresh.push(q);
        }
    }
    assert_eq!(all_fresh.len(), 14);
}
