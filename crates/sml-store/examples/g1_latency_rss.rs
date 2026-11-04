//! CONTEXT-3 G1: release-mode per-query latency (p50/p95/max) and process
//! peak RSS for the capped shortlist paths vs the uncapped exhaustive
//! reference, on the synthetic fixture. Synthetic only; NOT a LONGCTX-B1
//! measurement; numbers are host-specific and not a generalization claim.
//!
//! Run: `cargo run --release -p sml-store --example g1_latency_rss [N] [QUERIES]`
//! (defaults N=1024, QUERIES=256). Output: one `G1_LAT` row per cell.
//! `peak_rss_delta_kib` = VmHWM after the cell minus VmRSS before it, with
//! VmHWM reset per cell via /proc/self/clear_refs; `NOT_MEASURED` otherwise.

#[path = "support/g1_fixture.rs"]
mod g1_fixture;

use g1_fixture::{policy, query, Dist, Fixture};
use sml_store::context_retrieve::{exhaustive_semantic_scan, ContextRetrieveError};
use sml_store::context_retrieve_bench::{
    current_rss_kib, measure_latency, peak_rss_kib, reset_peak_rss,
};

const PREFIX_BITS: u32 = 8;
const CAPS: [usize; 4] = [32, 64, 128, 256];
const RADII: [u32; 2] = [1, 2];
const WARMUP: usize = 1;

fn kib(v: Option<u64>) -> String {
    v.map_or_else(|| "NOT_MEASURED".to_owned(), |x| x.to_string())
}

fn main() -> Result<(), ContextRetrieveError> {
    let mut args = std::env::args().skip(1);
    let segments: u64 = args.next().map_or(1024, |a| a.parse().expect("N"));
    let queries: u64 = args.next().map_or(256, |a| a.parse().expect("QUERIES"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let resettable = reset_peak_rss();
    println!(
        "G1_LAT_META profile={profile} os={} arch={} segments={segments} queries={queries} \
         warmup={WARMUP} prefix_bits={PREFIX_BITS} top_k=1 hwm_reset={resettable} synthetic=true",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!(
        "G1_LAT dist N engine radius cap R@1 mean_shortlisted p50_us p95_us max_us \
         rss_before_kib peak_rss_delta_kib"
    );
    for (i, dist) in [Dist::Clustered, Dist::NoisyClustered, Dist::Uniform]
        .into_iter()
        .enumerate()
    {
        let fixture = Fixture::build(3100 + i as u64, segments, dist);
        let retriever = fixture.open(PREFIX_BITS);
        let words: Vec<u64> = (0..queries)
            .map(|s| fixture.dist.query(segments, s))
            .collect();
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
            let mut shortlisted = 0_u64;
            for (word, &b) in words.iter().zip(&best) {
                let (hits, acc) = run(*word)?;
                shortlisted += acc.shortlisted;
                if hits.first().is_some_and(|h| h.hamming_distance == b) {
                    hit += 1;
                }
            }
            let _ = reset_peak_rss();
            let before = current_rss_kib();
            let summary = measure_latency(&words, WARMUP, |&w| run(w).map(|_| ()))?
                .expect("non-empty query set");
            let delta = match (peak_rss_kib(), before) {
                (Some(p), Some(b)) if resettable => Some(p.saturating_sub(b)),
                _ => None,
            };
            println!(
                "G1_LAT {} {segments} {engine} {radius} {} {:.3} {:.1} {:.1} {:.1} {:.1} {} {}",
                dist.name(),
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
            );
        }
    }
    println!("G1_LAT_PROCESS peak_rss_kib={}", kib(peak_rss_kib()));
    Ok(())
}

fn g1q(word: u64) -> sml_store::context_retrieve::RetrieveQuery<{ g1_fixture::BITS }> {
    query(word)
}
