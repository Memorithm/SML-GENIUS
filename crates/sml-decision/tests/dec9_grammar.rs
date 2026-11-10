use sml_decision::dec4_instr::{held_out, seen, Intent};
use sml_decision::dec8_fresh::fresh;
use sml_decision::dec9_grammar::{grammar, reserved};

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
            }
        }
    }
    assert_eq!(n, 14);
}

#[test]
fn grammar_sizes_are_fixed() {
    let sizes: Vec<usize> = ALL.iter().map(|&i| grammar(i).len()).collect();
    // Grammar plus spent wordings, minus duplicates and the one reserved match.
    assert_eq!(sizes, vec![80, 120, 44, 39, 50]);
    for i in ALL {
        let g = grammar(i);
        let mut d = g.clone();
        d.sort();
        d.dedup();
        assert_eq!(d.len(), g.len());
    }
}
