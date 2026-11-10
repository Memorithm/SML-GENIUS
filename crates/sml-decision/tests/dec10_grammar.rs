use sml_decision::dec10_grammar::{grammar, reserved};
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
        let g = grammar(i);
        for q in reserved(i) {
            n += 1;
            assert!(!g.iter().any(|w| w == q), "{q} leaks into training");
            for j in ALL {
                assert!(!seen(j).contains(q) && !held_out(j).contains(q) && !fresh(j).contains(q));
                assert!(!dec9_grammar::reserved(j).contains(q));
            }
        }
    }
    assert_eq!(n, 14);
}

#[test]
fn grammar_extends_dec9_and_has_no_duplicates() {
    for i in ALL {
        let g = grammar(i);
        for w in dec9_grammar::grammar(i) {
            assert!(g.contains(&w));
        }
        let mut d = g.clone();
        d.sort();
        d.dedup();
        assert_eq!(d.len(), g.len());
    }
}
