//! CONTEXT-3 G1: fail-closed cross-run aggregation of raw `g1_latency_rss`
//! stdout (one string per independent process).
//!
//! Pure text processing; it measures nothing and emits no verdict. It refuses
//! (returns an error) unless every run is a release, synthetic, hwm-resettable
//! run with identical parameters, the required number of runs is present, every
//! run reports the identical cell set, and quality columns (`R@1`,
//! `mean_shortlisted`) are bit-identical across runs (the fixture is
//! deterministic, so divergence means the inputs are not comparable). Latency
//! is summarised per cell as min / median / max of each run's p50 and p95, so
//! cross-run spread is visible rather than hidden. `NOT_MEASURED` RSS stays
//! `None`; it is never imputed.
//!
//! Optional page-fault columns (`minflt_delta majflt_delta`) are accepted only
//! when the run's meta declares `faults=true|false`; such rows must then have
//! 14 columns, legacy rows (no `faults` key) exactly 12. A run declaring
//! `faults=false` must report `NOT_MEASURED` for both. Because meta must be
//! identical across runs, legacy and fault-instrumented runs never mix. Per
//! cell the aggregate keeps the max delta across runs, or `None` if any run
//! did not measure it.

use std::collections::BTreeMap;

/// Minimum N accepted for a CONTEXT-3 long-context aggregate.
pub const MIN_SEGMENTS: u64 = 16_000;

/// Why a set of runs cannot be aggregated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateError {
    TooFewRuns { got: usize, need: usize },
    MissingMeta { run: usize },
    BadMeta { run: usize, reason: String },
    MetaMismatch { run: usize },
    MalformedRow { run: usize, line: String },
    NoRows { run: usize },
    DuplicateCell { run: usize, cell: String },
    CellSetMismatch { run: usize },
    QualityDiverged { cell: String },
    MissingProcessLine { run: usize },
}

/// Identity of one measured cell.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellKey {
    pub dist: String,
    pub engine: String,
    pub radius: u32,
    pub cap: String,
}

impl CellKey {
    fn label(&self) -> String {
        format!(
            "{}/{}/r{}/cap{}",
            self.dist, self.engine, self.radius, self.cap
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Row {
    recall_at_1: String,
    mean_shortlisted: String,
    p50_us: f64,
    p95_us: f64,
    peak_rss_delta_kib: Option<u64>,
    minflt_delta: Option<u64>,
    majflt_delta: Option<u64>,
}

/// min / median / max across runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spread {
    pub min: f64,
    pub median: f64,
    pub max: f64,
}

fn spread(mut v: Vec<f64>) -> Spread {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    let median = if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    };
    Spread {
        min: v[0],
        median,
        max: v[n - 1],
    }
}

/// Aggregated cell across runs.
#[derive(Debug, Clone, PartialEq)]
pub struct CellAggregate {
    pub key: CellKey,
    pub recall_at_1: String,
    pub mean_shortlisted: String,
    pub p50_us: Spread,
    pub p95_us: Spread,
    /// `None` if any run reported `NOT_MEASURED`.
    pub peak_rss_delta_kib_max: Option<u64>,
    /// Max minor-fault delta across runs; `None` if any run lacks it.
    pub minflt_delta_max: Option<u64>,
    /// Max major-fault delta across runs; `None` if any run lacks it.
    pub majflt_delta_max: Option<u64>,
}

/// Aggregate over all runs. `segments`/`queries` come from the shared meta.
#[derive(Debug, Clone, PartialEq)]
pub struct Aggregate {
    pub runs: usize,
    pub segments: u64,
    pub queries: u64,
    pub cells: Vec<CellAggregate>,
}

fn parse_meta(run: usize, line: &str) -> Result<BTreeMap<String, String>, AggregateError> {
    let kv: BTreeMap<String, String> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|t| t.split_once('=').map(|(k, v)| (k.to_owned(), v.to_owned())))
        .collect();
    let bad = |reason: &str| AggregateError::BadMeta {
        run,
        reason: reason.to_owned(),
    };
    if kv.get("profile").map(String::as_str) != Some("release") {
        return Err(bad("profile != release"));
    }
    if kv.get("synthetic").map(String::as_str) != Some("true") {
        return Err(bad("synthetic != true"));
    }
    if kv.get("hwm_reset").map(String::as_str) != Some("true") {
        return Err(bad("hwm_reset != true"));
    }
    let n: u64 = kv
        .get("segments")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| bad("segments missing"))?;
    if n < MIN_SEGMENTS {
        return Err(bad("segments below MIN_SEGMENTS"));
    }
    if kv
        .get("queries")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
        == 0
    {
        return Err(bad("queries missing or zero"));
    }
    if let Some(f) = kv.get("faults") {
        if f != "true" && f != "false" {
            return Err(bad("faults not true|false"));
        }
    }
    Ok(kv)
}

fn opt_u64(s: &str) -> Result<Option<u64>, ()> {
    match s {
        "NOT_MEASURED" => Ok(None),
        s => s.parse().map(Some).map_err(|_| ()),
    }
}

type ParsedRun = (BTreeMap<String, String>, BTreeMap<CellKey, Row>);

fn parse_run(run: usize, text: &str) -> Result<ParsedRun, AggregateError> {
    let mut meta = None;
    let mut rows = BTreeMap::new();
    let mut process = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("G1_LAT_META ") {
            if meta.is_some() || !rows.is_empty() {
                return Err(AggregateError::BadMeta {
                    run,
                    reason: "meta repeated or after rows".to_owned(),
                });
            }
            meta = Some(parse_meta(run, line)?);
        } else if line.starts_with("G1_LAT_PROCESS ") {
            process = true;
        } else if let Some(rest) = line.strip_prefix("G1_LAT ") {
            if rest.starts_with("dist ") {
                continue; // header
            }
            let f: Vec<&str> = rest.split_whitespace().collect();
            let malformed = || AggregateError::MalformedRow {
                run,
                line: line.to_owned(),
            };
            let faults = meta
                .as_ref()
                .ok_or(AggregateError::MissingMeta { run })?
                .get("faults")
                .cloned();
            let want = if faults.is_some() { 14 } else { 12 };
            if f.len() != want {
                return Err(malformed());
            }
            let (minflt_delta, majflt_delta) = if want == 14 {
                let mi = opt_u64(f[12]).map_err(|_| malformed())?;
                let ma = opt_u64(f[13]).map_err(|_| malformed())?;
                if faults.as_deref() == Some("false") && (mi.is_some() || ma.is_some()) {
                    return Err(malformed());
                }
                (mi, ma)
            } else {
                (None, None)
            };
            let key = CellKey {
                dist: f[0].to_owned(),
                engine: f[2].to_owned(),
                radius: f[3].parse().map_err(|_| malformed())?,
                cap: f[4].to_owned(),
            };
            let num = |s: &str| s.parse::<f64>().ok().filter(|x| x.is_finite() && *x >= 0.0);
            let row = Row {
                recall_at_1: f[5].to_owned(),
                mean_shortlisted: f[6].to_owned(),
                p50_us: num(f[7]).ok_or_else(malformed)?,
                p95_us: num(f[8]).ok_or_else(malformed)?,
                peak_rss_delta_kib: opt_u64(f[11]).map_err(|_| malformed())?,
                minflt_delta,
                majflt_delta,
            };
            if row.p95_us < row.p50_us {
                return Err(malformed());
            }
            let label = key.label();
            if rows.insert(key, row).is_some() {
                return Err(AggregateError::DuplicateCell { run, cell: label });
            }
        }
    }
    let meta = meta.ok_or(AggregateError::MissingMeta { run })?;
    if rows.is_empty() {
        return Err(AggregateError::NoRows { run });
    }
    if !process {
        return Err(AggregateError::MissingProcessLine { run });
    }
    Ok((meta, rows))
}

/// Aggregate `runs` (raw stdout of each independent process). Fails closed.
pub fn aggregate_runs(runs: &[&str], min_runs: usize) -> Result<Aggregate, AggregateError> {
    if runs.len() < min_runs.max(1) {
        return Err(AggregateError::TooFewRuns {
            got: runs.len(),
            need: min_runs.max(1),
        });
    }
    let parsed = runs
        .iter()
        .enumerate()
        .map(|(i, t)| parse_run(i, t))
        .collect::<Result<Vec<_>, _>>()?;
    let (meta0, rows0) = &parsed[0];
    for (i, (meta, rows)) in parsed.iter().enumerate().skip(1) {
        if meta != meta0 {
            return Err(AggregateError::MetaMismatch { run: i });
        }
        if !rows.keys().eq(rows0.keys()) {
            return Err(AggregateError::CellSetMismatch { run: i });
        }
    }
    let mut cells = Vec::with_capacity(rows0.len());
    for (key, r0) in rows0 {
        let all: Vec<&Row> = parsed.iter().map(|(_, rows)| &rows[key]).collect();
        if all
            .iter()
            .any(|r| r.recall_at_1 != r0.recall_at_1 || r.mean_shortlisted != r0.mean_shortlisted)
        {
            return Err(AggregateError::QualityDiverged { cell: key.label() });
        }
        let max_all = |get: fn(&Row) -> Option<u64>| {
            all.iter()
                .map(|r| get(r))
                .collect::<Option<Vec<_>>>()
                .and_then(|v| v.into_iter().max())
        };
        cells.push(CellAggregate {
            key: key.clone(),
            recall_at_1: r0.recall_at_1.clone(),
            mean_shortlisted: r0.mean_shortlisted.clone(),
            p50_us: spread(all.iter().map(|r| r.p50_us).collect()),
            p95_us: spread(all.iter().map(|r| r.p95_us).collect()),
            peak_rss_delta_kib_max: max_all(|r| r.peak_rss_delta_kib),
            minflt_delta_max: max_all(|r| r.minflt_delta),
            majflt_delta_max: max_all(|r| r.majflt_delta),
        });
    }
    Ok(Aggregate {
        runs: runs.len(),
        segments: meta0["segments"].parse().unwrap_or(0),
        queries: meta0["queries"].parse().unwrap_or(0),
        cells,
    })
}
