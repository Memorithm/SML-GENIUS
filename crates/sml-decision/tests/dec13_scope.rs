use sml_decision::dec10_grammar;
use sml_decision::dec11_capacity;
use sml_decision::dec12_router;
use sml_decision::dec13_scope::{reserved, scope_grammar, training_wordings};
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

fn norm(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect()
}

#[test]
fn reserved_set_is_new_and_absent_from_training() {
    let mut n = 0;
    let mut all = Vec::new();
    for i in ALL {
        assert_eq!(reserved(i).len(), 10);
        for q in reserved(i) {
            n += 1;
            all.push(norm(q));
            for j in ALL {
                let spent: Vec<String> = training_wordings(j)
                    .iter()
                    .chain(dec12_router::training_wordings(j).iter())
                    .cloned()
                    .chain(
                        [
                            seen(j),
                            held_out(j),
                            fresh(j),
                            dec9_grammar::reserved(j),
                            dec10_grammar::reserved(j),
                            dec11_capacity::reserved(j),
                            dec12_router::reserved(j),
                        ]
                        .into_iter()
                        .flatten()
                        .map(|s| s.to_string()),
                    )
                    .collect();
                assert!(!spent.iter().any(|w| norm(w) == norm(q)), "{q} spent");
            }
        }
    }
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 50);
    assert_eq!(n, 50);
}

#[test]
fn training_grows_by_the_scope_grammar() {
    let sizes: Vec<usize> = ALL.iter().map(|&i| scope_grammar(i).len()).collect();
    assert_eq!(sizes, vec![48, 48, 54, 54, 54]);
    for i in ALL {
        let t = training_wordings(i);
        assert!(t.len() >= dec12_router::training_wordings(i).len() + 48);
    }
}
