use sml_decision::dec4_instr::{held_out, seen, Intent};
use sml_decision::dec7_polarity::{cues_in, negated, negators, SYNTACTIC};

const LEX: &str = "true\tS-true\tS-false\t-\n\
false\tS-false\tS-true\t-\n\
right\tS-right,S-rdir\tS-wrong,S-ldir\t-\n\
wrong\tS-wrong\tS-right\t-\n\
left\tS-ldir\tS-rdir\t-\n\
big\tS-big\tS-small\t-\n\
small\tS-small\tS-big\t-\n";

#[test]
fn cue_set_is_syntactic_plus_antonyms_of_truth() {
    let cues = negators(LEX);
    for w in SYNTACTIC {
        assert!(cues.contains(w));
    }
    assert!(cues.contains("false") && cues.contains("wrong"));
    // "left" is an antonym of a sense of "right": it is a cue too (stated limit).
    assert!(cues.contains("left"));
    assert!(!cues.contains("small") && !cues.contains("true"));
}

#[test]
fn polarity_counts_cues_with_parity() {
    let cues = negators(LEX);
    assert!(negated(&cues, "Is it false that B continues A?"));
    assert!(!negated(&cues, "Is it not false?"));
    assert_eq!(cues_in(&cues, "Does B not continue A?"), ["not"]);
    assert!(!negated(&cues, "Does B follow A?"));
}

#[test]
fn polarity_of_the_dec4_wordings_is_reported_not_tuned() {
    // Only one seen NotContinues wording has a cue; this is logged by the runner.
    let cues = negators(LEX);
    let n: usize = seen(Intent::NotContinues)
        .iter()
        .filter(|q| negated(&cues, q))
        .count();
    assert_eq!(n, 1);
    assert!(seen(Intent::Continues).iter().all(|q| !negated(&cues, q)));
    assert!(held_out(Intent::NotContinues)
        .iter()
        .all(|q| negated(&cues, q)));
}
