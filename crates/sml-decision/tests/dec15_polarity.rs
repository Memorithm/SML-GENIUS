use sml_decision::dec10_grammar;
use sml_decision::dec11_capacity;
use sml_decision::dec12_router;
use sml_decision::dec13_scope;
use sml_decision::dec14_anchor;
use sml_decision::dec15_polarity::{
    anchor_rule, main_clause, reserved, training_wordings, Anchors,
};
use sml_decision::dec4_instr::{held_out, seen, Intent};
use sml_decision::dec8_fresh::fresh;
use sml_decision::dec9_grammar;
use std::collections::HashSet;

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
    let mut all = Vec::new();
    for i in ALL {
        assert_eq!(reserved(i).len(), 10);
        for q in reserved(i) {
            all.push(norm(q));
            for j in ALL {
                let spent: Vec<String> = training_wordings(j)
                    .into_iter()
                    .chain(dec13_scope::training_wordings(j))
                    .chain(dec14_anchor::training_wordings(j))
                    .chain(
                        [
                            seen(j),
                            held_out(j),
                            fresh(j),
                            dec9_grammar::reserved(j),
                            dec10_grammar::reserved(j),
                            dec11_capacity::reserved(j),
                            dec12_router::reserved(j),
                            dec13_scope::reserved(j),
                            dec14_anchor::reserved(j),
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
}

#[test]
fn scope_rule_keeps_the_first_clause() {
    assert_eq!(main_clause("Is this X rather than Y?"), "Is this X");
    assert_eq!(
        main_clause("Does B a instead of b as opposed to c"),
        "Does B a"
    );
    assert_eq!(main_clause("plain?"), "plain?");
}

#[test]
fn anchors_expand_through_synsets_on_a_toy_lexicon() {
    let lex = "close\tS-end\t-\t-\nend\tS-end\t-\t-\nfinal\t-\t-\t-\nterminus\tS-t\t-\tS-end\n";
    let a = Anchors::from_lexicon(lex);
    assert!(a.word("close")[2]);
    assert!(a.word("terminus")[2]);
    assert_eq!(a.word("final"), [false; 5]);
    assert!(a.word("endings")[2]);
    let cues: HashSet<String> = ["not".to_string()].into_iter().collect();
    assert_eq!(
        anchor_rule(&a, &cues, "t3", "Is this the close?"),
        Some(Intent::End)
    );
    assert_eq!(
        anchor_rule(&a, &cues, "t1", "Does B follow A?"),
        Some(Intent::Continues)
    );
    assert_eq!(
        anchor_rule(&a, &cues, "t1", "Does B not follow A?"),
        Some(Intent::NotContinues)
    );
    assert_eq!(
        anchor_rule(&a, &cues, "t1", "Is B elsewhere rather than next?"),
        Some(Intent::NotContinues)
    );
    assert_eq!(anchor_rule(&a, &cues, "t3", "Is this here?"), None);
    let lex2 = "connected\tS-c\tS-u\t-\nunconnected\tS-u\tS-c\t-\n";
    let b = Anchors::from_lexicon(lex2);
    assert_eq!(
        anchor_rule(&b, &cues, "t1", "Is B unconnected to A?"),
        Some(Intent::NotContinues)
    );
    assert_eq!(
        anchor_rule(&b, &cues, "t1", "Is B connected to A?"),
        Some(Intent::Continues)
    );
}
