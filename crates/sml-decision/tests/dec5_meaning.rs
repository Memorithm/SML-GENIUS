use sml_decision::dec4_instr;
use sml_decision::dec5_meaning::{features, hamming, signs, QuestionCoder, WordCodes};

/// "start" and "beginning" share contexts; "kiwi" lives elsewhere.
fn toy() -> Vec<String> {
    let mut t = Vec::new();
    for i in 0..60 {
        let ctx = ["at the", "from the", "near the"][i % 3];
        t.push(format!("{ctx} start of the paper we state aims a{i}"));
        t.push(format!("{ctx} beginning of the paper we state aims b{i}"));
        t.push(format!("ripe green kiwi fruit tastes sour c{i}"));
    }
    t
}

#[test]
fn sign_vectors_are_fixed_and_balanced() {
    assert_eq!(signs("word"), signs("word"));
    assert_ne!(signs("word"), signs("ward"));
    let ones = signs("word").count_ones();
    assert!((16..=48).contains(&ones));
}

#[test]
fn shared_contexts_give_close_codes() {
    let texts = toy();
    let codes = WordCodes::learn(texts.iter().map(String::as_str));
    let near = hamming(&codes, "start", "beginning").unwrap();
    let far = hamming(&codes, "start", "kiwi").unwrap();
    assert!(near + 16 <= far, "near={near} far={far}");
    // Rare words get no code.
    assert!(codes.code("a7").is_none());
    let again = WordCodes::learn(texts.iter().map(String::as_str));
    assert_eq!(codes.fingerprint(), again.fingerprint());
}

#[test]
fn question_codes_carry_meaning_to_new_words() {
    let texts = toy();
    let codes = WordCodes::learn(texts.iter().map(String::as_str));
    let train = ["is it from the start", "is it about kiwi"];
    let qc = QuestionCoder::fit(&codes, train);
    let a = qc.bits(&codes, "is it from the start");
    let b = qc.bits(&codes, "is it from the beginning");
    let c = qc.bits(&codes, "is it about kiwi");
    assert!((a ^ b).count_ones() < (a ^ c).count_ones());
}

#[test]
fn zero_code_with_identity_is_dec4() {
    let c = dec4_instr::content_bits("t3", &["we conclude that the method works"]);
    let q = "Is this passage from the end of the article?";
    assert_eq!(
        features("t3", &c, 0, Some(q)),
        dec4_instr::features("t3", &c, Some(q))
    );
    assert_eq!(
        features("t3", &c, 0, None),
        dec4_instr::features("t3", &c, None)
    );
    let f = features("t3", &c, 0b1011, None);
    assert!(f.len() > dec4_instr::features("t3", &c, None).len());
    assert!(f.windows(2).all(|w| w[0] < w[1]));
}
