use sml_decision::dec4_instr;
use sml_decision::dec5_meaning::hamming;
use sml_decision::dec6_lexicon::{features, lemma_candidates, lexicon_codes, row_code};

const LEX: &str = "beginning\tS-begin,S-x\tS-end\tS-point\n\
end\tS-end,S-y\tS-begin\tS-point\n\
start\tS-begin,S-z\t-\tS-point\n\
continue\tS-cont\tS-stop\t-\n\
kiwi\tS-kiwi\t-\tS-fruit\n";

#[test]
fn lemma_rules_cover_common_inflections() {
    assert!(lemma_candidates("continues").contains(&"continue".to_string()));
    assert!(lemma_candidates("taken").is_empty());
    assert_eq!(lemma_candidates("studies")[0], "study");
    assert!(lemma_candidates("closing").contains(&"close".to_string()));
}

#[test]
fn antonyms_disagree_and_synonyms_agree() {
    let lex = lexicon_codes(LEX).unwrap();
    let syn = hamming(&lex, "beginning", "start").unwrap();
    let ant = hamming(&lex, "beginning", "end").unwrap();
    let far = hamming(&lex, "beginning", "kiwi").unwrap();
    // Synonyms agree; antonyms no longer collapse (DEC-5 had beginning/end at 2).
    assert!(syn + 8 < far, "syn={syn} far={far} ant={ant}");
    assert!(ant > syn + 16, "syn={syn} far={far} ant={ant}");
    // Inflected forms are looked up through the lemma rules.
    assert_eq!(lex.code("continues"), lex.code("continue"));
    assert_eq!(row_code("S-a", "-", "-"), row_code("S-a", "", ""));
}

#[test]
fn bad_lexicon_line_is_refused() {
    assert!(lexicon_codes("word\tS-a\n").is_err());
}

#[test]
fn no_codes_is_the_blind_dec4_feature_set() {
    let c = dec4_instr::content_bits("t3", &["we conclude that the method works"]);
    assert_eq!(
        features("t3", &c, &[]),
        dec4_instr::features("t3", &c, None)
    );
    let a = features("t3", &c, &[(b"lex", 0b101)]);
    let b = features("t3", &c, &[(b"ctx", 0b101)]);
    assert_ne!(a, b);
    assert!(a.windows(2).all(|w| w[0] < w[1]));
}
