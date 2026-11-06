use sml_decision::dec10_grammar;
use sml_decision::dec11_capacity::reserved;
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
                    !dec10_grammar::grammar(j).iter().any(|w| w == q),
                    "{q} in grammar"
                );
                assert!(!seen(j).contains(q) && !held_out(j).contains(q) && !fresh(j).contains(q));
                assert!(!dec9_grammar::reserved(j).contains(q));
                assert!(!dec10_grammar::reserved(j).contains(q));
            }
        }
    }
    assert_eq!(n, 14);
}
