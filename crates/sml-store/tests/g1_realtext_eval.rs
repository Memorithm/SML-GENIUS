//! CONTEXT-3 G1 real-text derivation rules v1: segmentation, query/target
//! derivation, provisional near-duplicate refusal. Hand-written inputs only;
//! no scores are computed.

use sml_store::g1_latency_aggregate::aggregate_runs;
use sml_store::g1_realtext_corpus::{load_pinned, RealTextDoc};
use sml_store::g1_realtext_eval::{
    build_eval_set, check_near_duplicates, derive_query, segment_doc, tokens, DerivationError,
    DerivationParams, PARAMS_V1,
};

const MANIFEST: &str = include_str!("fixtures/g1_realtext/tiny.manifest");
const CORPUS: &[u8] = include_bytes!("fixtures/g1_realtext/tiny.tsv");

const SMALL: DerivationParams = DerivationParams {
    segment_tokens: 4,
    min_tail_tokens: 2,
    query_tokens: 2,
    shingle: 2,
    near_dup_num: 8,
    near_dup_den: 10,
};

fn doc(id: &str, text: &str) -> RealTextDoc {
    RealTextDoc {
        doc_id: id.to_owned(),
        text: text.to_owned(),
    }
}

#[test]
fn normalization_lowercases_and_splits_on_non_alphanumerics() {
    assert_eq!(
        tokens("The tide's  RISE—Été, 2026!"),
        ["the", "tide", "s", "rise", "été", "2026"]
    );
    assert!(tokens(" ,.; ").is_empty());
}

#[test]
fn segmentation_is_non_overlapping_and_drops_short_tail() {
    let s = segment_doc(&doc("a", "a b c d e f g h i"), &SMALL).unwrap();
    let ids: Vec<_> = s.iter().map(|x| x.segment_id.as_str()).collect();
    assert_eq!(ids, ["a#0", "a#1"]); // tail "i" (1 < 2 tokens) dropped
    assert_eq!(s[1].tokens, ["e", "f", "g", "h"]);
    let s = segment_doc(&doc("b", "a b c d e f"), &SMALL).unwrap();
    assert_eq!(s.len(), 2);
    assert_eq!(s[1].tokens, ["e", "f"]);
}

#[test]
fn query_is_deterministic_window_inside_target() {
    let d = doc("q1", "one two three four five six seven eight nine ten");
    let q = derive_query(&d, &SMALL).unwrap().unwrap();
    assert_eq!(q, derive_query(&d, &SMALL).unwrap().unwrap());
    let segs = segment_doc(&d, &SMALL).unwrap();
    let t = segs
        .iter()
        .find(|s| s.segment_id == q.target_segment_id)
        .unwrap();
    assert_eq!(t.tokens[q.offset..q.offset + 2], q.query_tokens[..]);
    assert_eq!(derive_query(&doc("e", "x"), &SMALL).unwrap(), None);
}

#[test]
fn near_duplicates_are_refused() {
    let train = [doc(
        "t1",
        "the tide rose over the old harbour wall before dawn",
    )];
    // Normalized-exact (case/punctuation only).
    let e = [doc(
        "e1",
        "The TIDE rose, over the old harbour-wall before dawn!",
    )];
    assert!(matches!(
        check_near_duplicates(&train, &e, &SMALL),
        Err(DerivationError::NearDuplicate { .. })
    ));
    // One word changed: 2-shingle Jaccard 7/11 < 0.8 passes; at 0.6 refused.
    let e = [doc(
        "e2",
        "the tide rose over the new harbour wall before dawn",
    )];
    assert_eq!(check_near_duplicates(&train, &e, &SMALL), Ok(()));
    let loose = DerivationParams {
        near_dup_num: 6,
        ..SMALL
    };
    assert_eq!(
        check_near_duplicates(&train, &e, &loose),
        Err(DerivationError::NearDuplicate {
            train_id: "t1".into(),
            eval_id: "e2".into(),
            inter: 7,
            union: 11,
        })
    );
}

#[test]
fn bad_params_are_refused() {
    let p = DerivationParams {
        query_tokens: 3,
        ..SMALL
    };
    assert_eq!(
        segment_doc(&doc("a", "a b"), &p),
        Err(DerivationError::BadParams)
    );
}

#[test]
fn fixture_eval_set_and_meta_never_aggregate_with_synthetic() {
    let c = load_pinned(MANIFEST, CORPUS).unwrap();
    // Frozen v1 needs >= 16 tokens per segment; the tiny fixture has none.
    assert_eq!(
        build_eval_set(&c, &PARAMS_V1),
        Err(DerivationError::NoQueries)
    );
    let set = build_eval_set(&c, &SMALL).unwrap();
    assert_eq!(set.queries.len(), c.eval().len());
    assert!(set
        .segments
        .iter()
        .all(|s| c.eval().iter().any(|d| d.doc_id == s.doc_id)));
    let meta = set.meta_fields();
    assert!(meta.contains("synthetic=false"));
    assert!(meta.contains("corpus_id=g1-tiny-handwritten"));
    assert!(meta.contains(&format!("sha256={}", c.manifest().sha256_hex)));
    let run = format!(
        "G1_LAT_META profile=release os=linux arch=x86_64 segments=4 queries=4 warmup=1 \
         prefix_bits=8 top_k=1 hwm_reset=true faults=false {meta}\n"
    );
    assert!(aggregate_runs(&[&run], 1).is_err());
}
