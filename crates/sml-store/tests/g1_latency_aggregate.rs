//! Fail-closed tests for the G1 cross-run aggregator. All inputs below are
//! hand-written test fixtures, not measurements.

use sml_store::g1_latency_aggregate::{aggregate_runs, AggregateError};

const META: &str = "G1_LAT_META profile=release os=linux arch=aarch64 segments=16384 queries=256 warmup=1 prefix_bits=8 top_k=1 hwm_reset=true synthetic=true";
const HDR: &str = "G1_LAT dist N engine radius cap R@1 mean_shortlisted p50_us p95_us max_us rss_before_kib peak_rss_delta_kib";

fn run(p50: f64, recall: &str, rss: &str) -> String {
    format!(
        "{META}\n{HDR}\n\
         G1_LAT clustered 16384 exhaustive 0 none 1.000 16384.0 {p50:.1} {:.1} 900.0 5000 {rss}\n\
         G1_LAT clustered 16384 neighbor 1 64 {recall} 64.0 {:.1} {:.1} 50.0 5000 {rss}\n\
         G1_LAT_PROCESS peak_rss_kib=9000\n",
        p50 * 2.0,
        p50 / 10.0,
        p50 / 5.0
    )
}

#[test]
fn aggregates_five_runs_with_spread() {
    let rs: Vec<String> = [100.0, 120.0, 110.0, 90.0, 130.0]
        .iter()
        .map(|&p| run(p, "0.980", "12"))
        .collect();
    let refs: Vec<&str> = rs.iter().map(String::as_str).collect();
    let a = aggregate_runs(&refs, 5).unwrap();
    assert_eq!(
        (a.runs, a.segments, a.queries, a.cells.len()),
        (5, 16384, 256, 2)
    );
    let ex = a
        .cells
        .iter()
        .find(|c| c.key.engine == "exhaustive")
        .unwrap();
    assert_eq!(
        (ex.p50_us.min, ex.p50_us.median, ex.p50_us.max),
        (90.0, 110.0, 130.0)
    );
    assert_eq!(ex.peak_rss_delta_kib_max, Some(12));
}

#[test]
fn refuses_too_few_runs() {
    let r = run(100.0, "0.980", "12");
    assert_eq!(
        aggregate_runs(&[&r, &r], 5),
        Err(AggregateError::TooFewRuns { got: 2, need: 5 })
    );
}

#[test]
fn refuses_quality_divergence() {
    let a = run(100.0, "0.980", "12");
    let b = run(100.0, "0.970", "12");
    assert!(matches!(
        aggregate_runs(&[&a, &b], 2),
        Err(AggregateError::QualityDiverged { .. })
    ));
}

#[test]
fn not_measured_rss_is_never_imputed() {
    let a = run(100.0, "0.980", "12");
    let b = run(100.0, "0.980", "NOT_MEASURED");
    let agg = aggregate_runs(&[&a, &b], 2).unwrap();
    assert!(agg.cells.iter().all(|c| c.peak_rss_delta_kib_max.is_none()));
}

#[test]
fn refuses_debug_small_n_or_mismatched_meta() {
    let dbg = run(100.0, "0.980", "12").replace("profile=release", "profile=debug");
    assert!(matches!(
        aggregate_runs(&[&dbg], 1),
        Err(AggregateError::BadMeta { .. })
    ));
    let small = run(100.0, "0.980", "12").replace("segments=16384", "segments=1024");
    assert!(matches!(
        aggregate_runs(&[&small], 1),
        Err(AggregateError::BadMeta { .. })
    ));
    let a = run(100.0, "0.980", "12");
    let other = a.replace("arch=aarch64", "arch=x86_64");
    assert_eq!(
        aggregate_runs(&[&a, &other], 2),
        Err(AggregateError::MetaMismatch { run: 1 })
    );
}

#[test]
fn refuses_truncated_run_and_cell_mismatch() {
    let a = run(100.0, "0.980", "12");
    let trunc = a.replace("G1_LAT_PROCESS peak_rss_kib=9000\n", "");
    assert_eq!(
        aggregate_runs(&[&a, &trunc], 2),
        Err(AggregateError::MissingProcessLine { run: 1 })
    );
    let fewer: String = a
        .lines()
        .filter(|l| !l.contains(" neighbor "))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(
        aggregate_runs(&[&a, &fewer], 2),
        Err(AggregateError::CellSetMismatch { run: 1 })
    );
    let bad = a.replace("1.000 16384.0", "1.000 16384.0 extra");
    assert!(matches!(
        aggregate_runs(&[&bad], 1),
        Err(AggregateError::MalformedRow { .. })
    ));
}
