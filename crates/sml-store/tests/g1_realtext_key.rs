//! CONTEXT-3 G1 search-key encoder v1 (`simhash64-sha256-v1`). Hand-written
//! inputs only; properties of the frozen rule, no retrieval-quality claims.

use sml_store::g1_latency_aggregate::aggregate_runs;
use sml_store::g1_realtext_key::{simhash_v1, token_hash, KeyError, KEY_ENCODER_V1};

#[test]
fn frozen_identifier() {
    assert_eq!(KEY_ENCODER_V1, "simhash64-sha256-v1");
}

#[test]
fn empty_refuses() {
    assert_eq!(simhash_v1::<&str>(&[]), Err(KeyError::Empty));
}

#[test]
fn single_token_key_is_its_hash() {
    // One token: c_b = +1 where bit set, -1 otherwise -> key == hash.
    let h = token_hash("river").unwrap();
    assert_eq!(simhash_v1(&["river"]).unwrap(), h);
}

#[test]
fn token_hash_is_domain_separated_sha256_prefix() {
    // u64_be(sha256("g1k-tok|river")[0..8]), computed independently.
    let d = sml_store::raw_context::sha256_raw_bytes(b"g1k-tok|river").unwrap();
    let mut b = [0_u8; 8];
    b.copy_from_slice(&d.bytes()[..8]);
    assert_eq!(token_hash("river").unwrap(), u64::from_be_bytes(b));
    assert_ne!(token_hash("river").unwrap(), token_hash("River").unwrap());
}

#[test]
fn order_invariant_multiset_and_ties_clear() {
    let a = simhash_v1(&["a", "b", "c"]).unwrap();
    assert_eq!(a, simhash_v1(&["c", "a", "b"]).unwrap());
    // Two distinct tokens: bit set iff both hashes set it (ties -> 0) = AND.
    let (x, y) = (token_hash("a").unwrap(), token_hash("b").unwrap());
    assert_eq!(simhash_v1(&["a", "b"]).unwrap(), x & y);
    // Repeats count: three copies of "a" vs one "b" -> majority "a".
    assert_eq!(simhash_v1(&["a", "a", "a", "b"]).unwrap(), x);
}

#[test]
fn realtext_meta_with_key_is_refused_by_synthetic_aggregator() {
    let run = format!(
        "G1_LAT_META profile=release os=linux arch=x86_64 segments=9 queries=3 warmup=1 \
         prefix_bits=8 top_k=1 hwm_reset=true faults=false synthetic=false arm=realtext \
         derivation=v1 corpus_id=x sha256={} key={KEY_ENCODER_V1}\n",
        "0".repeat(64)
    );
    assert!(aggregate_runs(&[&run], 1).is_err());
}
