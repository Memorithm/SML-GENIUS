//! CONTEXT-3 G1: release-mode per-query latency (p50/p95/max) and process
//! peak RSS for the capped shortlist paths vs the uncapped exhaustive
//! reference, on the synthetic fixture. Synthetic only; NOT a LONGCTX-B1
//! measurement; numbers are host-specific and not a generalization claim.
//!
//! Run: `cargo run --release -p sml-store --example g1_latency_rss [N] [QUERIES]`
//! (defaults N=1024, QUERIES=256). Output: one `G1_LAT` row per cell.
//! `peak_rss_delta_kib` = VmHWM after the cell minus VmRSS before it, with
//! VmHWM reset per cell via /proc/self/clear_refs; `NOT_MEASURED` otherwise.
//! `minflt_delta` / `majflt_delta` = process `/proc/self/stat` fault counters
//! across the timed pass (warmup included) of that cell; `NOT_MEASURED` if
//! procfs is unavailable. `VmPeak` (not resettable) is process-level only.
//!
//! Opt-in real-text arm (no default corpus; fails closed without a pin):
//! `... --example g1_latency_rss -- --realtext <MANIFEST> <CORPUS.tsv>`.
//! Loads via `load_pinned` (length + SHA-256 verified), derives segments and
//! queries via `g1_realtext_eval::PARAMS_V1`, keys via
//! `g1_realtext_key::simhash_v1`. Meta carries `synthetic=false arm=realtext
//! derivation=v1 key=simhash64-sha256-v1 corpus_id=.. sha256=..`, which the
//! synthetic aggregator refuses. Rows use dist=`realtext` and add a trailing
//! `gold@1` column (top-1 segment == derived target).

#[path = "support/g1_fixture.rs"]
mod g1_fixture;

use g1_fixture::{policy, query, Dist, Fixture};
use sml_store::context_retrieve::ContextRetriever;
use sml_store::context_retrieve::{exhaustive_semantic_scan, ContextRetrieveError};
use sml_store::context_retrieve_bench::{
    current_rss_kib, fault_counters, measure_latency, peak_rss_kib, reset_peak_rss, vm_peak_kib,
};
use sml_store::g1_realtext_corpus::load_pinned;
use sml_store::g1_realtext_eval::{build_eval_set, PARAMS_V1};
use sml_store::g1_realtext_key::{simhash_v1, KEY_ENCODER_V1};

const PREFIX_BITS: u32 = 8;
const CAPS: [usize; 4] = [32, 64, 128, 256];
const RADII: [u32; 2] = [1, 2];
const WARMUP: usize = 1;

fn kib(v: Option<u64>) -> String {
    v.map_or_else(|| "NOT_MEASURED".to_owned(), |x| x.to_string())
}

struct RealText {
    keys: Vec<u64>,
    words: Vec<u64>,
    gold: Vec<u64>,
    meta: String,
}

/// Fail-closed: any load, pin, derivation or key error aborts the process.
fn load_realtext(manifest: &str, corpus: &str) -> Result<RealText, String> {
    let m = std::fs::read_to_string(manifest).map_err(|e| format!("manifest: {e}"))?;
    let c = std::fs::read(corpus).map_err(|e| format!("corpus: {e}"))?;
    let pinned = load_pinned(&m, &c).map_err(|e| format!("load_pinned: {e:?}"))?;
    let set = build_eval_set(&pinned, &PARAMS_V1).map_err(|e| format!("derive: {e:?}"))?;
    let keys = set
        .segments
        .iter()
        .map(|s| simhash_v1(&s.tokens))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("key: {e:?}"))?;
    let mut words = Vec::new();
    let mut gold = Vec::new();
    for q in &set.queries {
        words.push(simhash_v1(&q.query_tokens).map_err(|e| format!("key: {e:?}"))?);
        let idx = set
            .segments
            .iter()
            .position(|s| s.segment_id == q.target_segment_id)
            .ok_or("target segment missing")?;
        gold.push(idx as u64 + 1);
    }
    Ok(RealText {
        keys,
        words,
        gold,
        meta: format!("{} key={KEY_ENCODER_V1}", set.meta_fields()),
    })
}

fn main() -> Result<(), ContextRetrieveError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--realtext") {
        let [_, manifest, corpus] = args.as_slice() else {
            eprintln!("G1_LAT_REFUSE usage: --realtext <MANIFEST> <CORPUS.tsv> (no default pin)");
            std::process::exit(2);
        };
        let rt = match load_realtext(manifest, corpus) {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("G1_LAT_REFUSE {e}");
                std::process::exit(2);
            }
        };
        return run_realtext(&rt);
    }
    let mut args = args.into_iter();
    let segments: u64 = args.next().map_or(1024, |a| a.parse().expect("N"));
    let queries: u64 = args.next().map_or(256, |a| a.parse().expect("QUERIES"));
    let resettable = reset_peak_rss();
    println!(
        "G1_LAT_META profile={} os={} arch={} segments={segments} queries={queries} \
         warmup={WARMUP} prefix_bits={PREFIX_BITS} top_k=1 hwm_reset={resettable} faults={} synthetic=true",
        profile(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        fault_counters().is_some()
    );
    println!(
        "G1_LAT dist N engine radius cap R@1 mean_shortlisted p50_us p95_us max_us \
         rss_before_kib peak_rss_delta_kib minflt_delta majflt_delta"
    );
    for (i, dist) in [Dist::Clustered, Dist::NoisyClustered, Dist::Uniform]
        .into_iter()
        .enumerate()
    {
        let fixture = Fixture::build(3100 + i as u64, segments, dist);
        let words: Vec<u64> = (0..queries)
            .map(|s| fixture.dist.query(segments, s))
            .collect();
        run_cells(&fixture, dist.name(), &words, None, resettable)?;
    }
    print_process();
    Ok(())
}

fn run_realtext(rt: &RealText) -> Result<(), ContextRetrieveError> {
    let resettable = reset_peak_rss();
    let segments = rt.keys.len() as u64;
    println!(
        "G1_LAT_META profile={} os={} arch={} segments={segments} queries={} \
         warmup={WARMUP} prefix_bits={PREFIX_BITS} top_k=1 hwm_reset={resettable} faults={} {}",
        profile(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        rt.words.len(),
        fault_counters().is_some(),
        rt.meta
    );
    println!(
        "G1_LAT dist N engine radius cap R@1 mean_shortlisted p50_us p95_us max_us \
         rss_before_kib peak_rss_delta_kib minflt_delta majflt_delta gold@1"
    );
    let fixture = Fixture::build_explicit(3200, rt.keys.clone());
    run_cells(&fixture, "realtext", &rt.words, Some(&rt.gold), resettable)?;
    print_process();
    Ok(())
}

fn profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

fn print_process() {
    println!(
        "G1_LAT_PROCESS peak_rss_kib={} vm_peak_kib={}",
        kib(peak_rss_kib()),
        kib(vm_peak_kib())
    );
}

fn run_cells(
    fixture: &Fixture,
    dist_name: &str,
    words: &[u64],
    gold: Option<&[u64]>,
    resettable: bool,
) -> Result<(), ContextRetrieveError> {
    let segments = fixture.segments;
    let queries = words.len() as u64;
    let retriever: ContextRetriever<{ g1_fixture::BITS }> = fixture.open(PREFIX_BITS);
    let best: Vec<u64> = words.iter().map(|&w| fixture.oracle_best(w)).collect();
    let mut cells: Vec<(&str, u32, usize)> = vec![("exhaustive", 0, 0)];
    for radius in RADII {
        for cap in CAPS {
            cells.push(("neighbor", radius, cap));
            cells.push(("global_wfull", radius, cap));
        }
    }
    for (engine, radius, cap) in cells {
        let run = |word: u64| match engine {
            "exhaustive" => {
                exhaustive_semantic_scan(&retriever, &g1q(word).bitfield, &policy(1, 1))
            }
            "neighbor" => {
                retriever.retrieve_with_neighbor_radius(&g1q(word), &policy(cap, 1), radius)
            }
            _ => retriever.retrieve_global_probe(
                &g1q(word),
                &policy(cap, 1),
                radius,
                64 - PREFIX_BITS,
            ),
        };
        // Quality pass (untimed): R@1 = top-1 distance equals oracle best.
        let mut hit = 0_u64;
        let mut gold_hit = 0_u64;
        let mut shortlisted = 0_u64;
        for (qi, (word, &b)) in words.iter().zip(&best).enumerate() {
            let (hits, acc) = run(*word)?;
            if let (Some(g), Some(h)) = (gold, hits.first()) {
                if h.segment.segment_id == g[qi] {
                    gold_hit += 1;
                }
            }
            shortlisted += acc.shortlisted;
            if hits.first().is_some_and(|h| h.hamming_distance == b) {
                hit += 1;
            }
        }
        let _ = reset_peak_rss();
        let before = current_rss_kib();
        let faults_before = fault_counters();
        let summary =
            measure_latency(words, WARMUP, |&w| run(w).map(|_| ()))?.expect("non-empty query set");
        let faults = faults_before
            .zip(fault_counters())
            .and_then(|(b, a)| a.delta_since(b));
        let delta = match (peak_rss_kib(), before) {
            (Some(p), Some(b)) if resettable => Some(p.saturating_sub(b)),
            _ => None,
        };
        println!(
            "G1_LAT {dist_name} {segments} {engine} {radius} {} {:.3} {:.1} {:.1} {:.1} {:.1} {} {} {} {}{}",
            if cap == 0 {
                "none".to_owned()
            } else {
                cap.to_string()
            },
            hit as f64 / queries as f64,
            shortlisted as f64 / queries as f64,
            summary.p50_ns as f64 / 1e3,
            summary.p95_ns as f64 / 1e3,
            summary.max_ns as f64 / 1e3,
            kib(before),
            kib(delta),
            kib(faults.map(|f| f.minor)),
            kib(faults.map(|f| f.major)),
            gold.map_or_else(String::new, |_| format!(
                " {:.3}",
                gold_hit as f64 / queries as f64
            )),
        );
    }
    Ok(())
}

fn g1q(word: u64) -> sml_store::context_retrieve::RetrieveQuery<{ g1_fixture::BITS }> {
    query(word)
}
