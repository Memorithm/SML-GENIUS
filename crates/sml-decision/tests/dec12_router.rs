use sml_decision::dec10_grammar;
use sml_decision::dec11_capacity;
use sml_decision::dec12_router::{class_of, intent_of, reserved, training_wordings};
use sml_decision::dec4_instr::{held_out, seen, Intent};
use sml_decision::dec8_fresh::fresh;
use sml_decision::dec9_grammar;

const ALL: [Intent; 5] = [
    Intent::Continues,
    Intent::NotContinues,
    Intent::Opening,
    Intent::Middle,
    Intent::End,
];

#[test]
fn reserved_set_is_new_and_absent_from_training() {
    let mut n = 0;
    for i in ALL {
        for q in reserved(i) {
            n += 1;
            for j in ALL {
                assert!(
                    !training_wordings(j).iter().any(|w| w == q),
                    "{q} in training"
                );
                assert!(!seen(j).contains(q) && !held_out(j).contains(q) && !fresh(j).contains(q));
                assert!(!dec9_grammar::reserved(j).contains(q));
                assert!(!dec10_grammar::reserved(j).contains(q));
                assert!(!dec11_capacity::reserved(j).contains(q));
            }
        }
    }
    assert_eq!(n, 14);
}

#[test]
fn classes_round_trip() {
    for (task, is) in [
        ("t1", &[Intent::Continues, Intent::NotContinues][..]),
        ("t3", &[Intent::Opening, Intent::Middle, Intent::End][..]),
    ] {
        for &i in is {
            assert_eq!(intent_of(task, class_of(i)), i);
        }
    }
}
