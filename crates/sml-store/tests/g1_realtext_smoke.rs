//! CONTEXT-3 G1 real-text **smoke fixture** (hand-written, in-repo). Exercises
//! `load_pinned` -> `build_eval_set(PARAMS_V1)` -> `simhash_v1` end to end so
//! CI covers the real-text path without downloading any corpus. Not a
//! measurement: no score is computed or asserted, and the fixture is not a
//! benchmark corpus. Any real corpus remains NOT_MEASURED.

use sml_store::g1_latency_aggregate::aggregate_runs;
use sml_store::g1_realtext_corpus::{load_pinned, RealTextError};
use sml_store::g1_realtext_eval::{build_eval_set, segment_doc, PARAMS_V1};
use sml_store::g1_realtext_key::{simhash_v1, KEY_ENCODER_V1};

const MANIFEST: &str = include_str!("fixtures/g1_realtext/smoke.manifest");
const CORPUS: &[u8] = include_bytes!("fixtures/g1_realtext/smoke.tsv");

#[test]
fn smoke_manifest_is_labelled_fixture_only() {
    assert!(MANIFEST.contains("SMOKE FIXTURE ONLY"));
    assert!(MANIFEST.contains("corpus_id g1-smoke-fixture-handwritten"));
}

#[test]
fn smoke_loader_accepts_pin_and_rejects_tamper() {
    let c = load_pinned(MANIFEST, CORPUS).unwrap();
    assert_eq!(c.manifest().byte_len, CORPUS.len() as u64);
    assert_eq!(c.train().len() + c.eval().len(), 8);
    assert!(!c.train().is_empty() && !c.eval().is_empty());
    let mut bad = CORPUS.to_vec();
    bad[10] ^= 1;
    assert!(matches!(
        load_pinned(MANIFEST, &bad),
        Err(RealTextError::ShaMismatch { .. })
    ));
}

#[test]
fn smoke_eval_set_builds_under_frozen_v1_with_nonempty_keys() {
    let c = load_pinned(MANIFEST, CORPUS).unwrap();
    let set = build_eval_set(&c, &PARAMS_V1).unwrap();
    assert_eq!(set.queries.len(), c.eval().len());
    for d in c.eval() {
        let segs = segment_doc(d, &PARAMS_V1).unwrap();
        assert!(!segs.is_empty(), "{} too short for PARAMS_V1", d.doc_id);
        assert_eq!(segs[0].tokens.len(), PARAMS_V1.segment_tokens);
    }
    for s in &set.segments {
        assert!(s.tokens.len() >= PARAMS_V1.min_tail_tokens);
        simhash_v1(&s.tokens).unwrap();
    }
    for q in &set.queries {
        assert_eq!(q.query_tokens.len(), PARAMS_V1.query_tokens);
        assert!(set
            .segments
            .iter()
            .any(|s| s.segment_id == q.target_segment_id));
        simhash_v1(&q.query_tokens).unwrap();
    }
    assert_eq!(KEY_ENCODER_V1, "simhash64-sha256-v1");
}

#[test]
fn smoke_meta_is_refused_by_synthetic_aggregator() {
    let c = load_pinned(MANIFEST, CORPUS).unwrap();
    let set = build_eval_set(&c, &PARAMS_V1).unwrap();
    let meta = set.meta_fields();
    assert!(meta.contains("synthetic=false"));
    assert!(meta.contains("corpus_id=g1-smoke-fixture-handwritten"));
    let run = format!(
        "G1_LAT_META profile=release os=linux arch=x86_64 segments={} queries={} warmup=1 \
         prefix_bits=8 top_k=1 hwm_reset=true faults=false {meta}\n",
        set.segments.len(),
        set.queries.len()
    );
    assert!(aggregate_runs(&[&run], 1).is_err());
}
