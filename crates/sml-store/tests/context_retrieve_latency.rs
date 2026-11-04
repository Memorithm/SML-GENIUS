//! CONTEXT-3 G1: smoke checks that the latency/RSS harness measures the
//! real capped and uncapped paths without altering their results.
//! Timing values are not asserted (host-dependent); only structure.

#[path = "../examples/support/g1_fixture.rs"]
mod g1_fixture;

use g1_fixture::{policy, query, Dist, Fixture};
use sml_store::context_retrieve::exhaustive_semantic_scan;
use sml_store::context_retrieve_bench::measure_latency;

#[test]
fn g1_latency_harness_measures_capped_and_uncapped_paths() {
    let fixture = Fixture::build(3200, 256, Dist::Clustered);
    let retriever = fixture.open(8);
    let words: Vec<u64> = (0..16).map(|s| fixture.dist.query(256, s)).collect();
    for cap in [32_usize, 64] {
        for radius in [1_u32, 2] {
            let s = measure_latency(&words, 1, |&w| {
                let (_, acc) =
                    retriever.retrieve_global_probe(&query(w), &policy(cap, 1), radius, 56)?;
                assert!(acc.shortlisted <= cap as u64);
                Ok::<(), sml_store::context_retrieve::ContextRetrieveError>(())
            })
            .unwrap()
            .unwrap();
            assert_eq!(s.samples, words.len());
            assert!(s.p50_ns <= s.p95_ns && s.p95_ns <= s.max_ns);
        }
    }
    let s = measure_latency(&words, 0, |&w| {
        let (hits, acc) = exhaustive_semantic_scan(&retriever, &query(w).bitfield, &policy(1, 1))?;
        assert_eq!(acc.shortlisted, 256);
        assert_eq!(hits[0].hamming_distance, fixture.oracle_best(w));
        Ok::<(), sml_store::context_retrieve::ContextRetrieveError>(())
    })
    .unwrap()
    .unwrap();
    assert_eq!(s.samples, 16);
}

#[test]
fn g1_latency_harness_is_deterministic_in_results() {
    let fixture = Fixture::build(3201, 256, Dist::NoisyClustered);
    let retriever = fixture.open(8);
    for seed in 0..8 {
        let w = fixture.dist.query(256, seed);
        let a = retriever
            .retrieve_global_probe(&query(w), &policy(32, 1), 2, 56)
            .unwrap();
        let b = retriever
            .retrieve_global_probe(&query(w), &policy(32, 1), 2, 56)
            .unwrap();
        assert_eq!(a, b);
    }
}
