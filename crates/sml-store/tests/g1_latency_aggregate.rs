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

fn fault_run(faults: &str, mi: &str, ma: &str) -> String {
    format!(
        "{} faults={faults}\n{HDR} minflt_delta majflt_delta\n\
         G1_LAT clustered 16384 exhaustive 0 none 1.000 16384.0 100.0 200.0 900.0 5000 12 {mi} {ma}\n\
         G1_LAT_PROCESS peak_rss_kib=9000 vm_peak_kib=20000\n",
        META.replace(" synthetic=true", ""),
    )
}

#[test]
fn fault_columns_aggregate_max_and_never_impute() {
    let a = fault_run("true", "40", "0");
    let b = fault_run("true", "55", "1");
    let agg = aggregate_runs(&[&a, &b], 2);
    // `synthetic=true` was stripped by the helper above: must refuse.
    assert!(matches!(agg, Err(AggregateError::BadMeta { .. })));
    let fix = |s: String| s.replace(" faults=", " synthetic=true faults=");
    let (a, b) = (fix(a), fix(b));
    let agg = aggregate_runs(&[&a, &b], 2).unwrap();
    assert_eq!(
        (agg.cells[0].minflt_delta_max, agg.cells[0].majflt_delta_max),
        (Some(55), Some(1))
    );
    let c = fix(fault_run("true", "NOT_MEASURED", "0"));
    let agg = aggregate_runs(&[&a, &c], 2).unwrap();
    assert_eq!(agg.cells[0].minflt_delta_max, None);
    assert_eq!(agg.cells[0].majflt_delta_max, Some(0));
}

#[test]
fn fault_columns_fail_closed() {
    let fix = |s: String| s.replace(" faults=", " synthetic=true faults=");
    let good = fix(fault_run("true", "1", "0"));
    // faults=false with numbers is contradictory.
    let lie = fix(fault_run("false", "1", "0"));
    assert!(matches!(
        aggregate_runs(&[&lie], 1),
        Err(AggregateError::MalformedRow { .. })
    ));
    let honest_off = fix(fault_run("false", "NOT_MEASURED", "NOT_MEASURED"));
    assert!(aggregate_runs(&[&honest_off], 1).is_ok());
    // Mixing instrumented and legacy runs is a meta mismatch.
    assert_eq!(
        aggregate_runs(&[&good, &honest_off], 2),
        Err(AggregateError::MetaMismatch { run: 1 })
    );
    // Legacy 12-column rows under a faults meta, and garbage values.
    let short = good.replace(" 12 1 0\n", " 12\n");
    assert!(matches!(
        aggregate_runs(&[&short], 1),
        Err(AggregateError::MalformedRow { .. })
    ));
    let neg = good.replace(" 12 1 0\n", " 12 -1 0\n");
    assert!(matches!(
        aggregate_runs(&[&neg], 1),
        Err(AggregateError::MalformedRow { .. })
    ));
    let badflag = good.replace("faults=true", "faults=maybe");
    assert!(matches!(
        aggregate_runs(&[&badflag], 1),
        Err(AggregateError::BadMeta { .. })
    ));
    // Legacy meta with 14 columns.
    let legacy14 = good.replace(" faults=true", "");
    assert!(matches!(
        aggregate_runs(&[&legacy14], 1),
        Err(AggregateError::MalformedRow { .. })
    ));
    let twice = format!("{}\n{good}", good.lines().next().unwrap());
    assert!(matches!(
        aggregate_runs(&[&twice], 1),
        Err(AggregateError::BadMeta { .. })
    ));
}
