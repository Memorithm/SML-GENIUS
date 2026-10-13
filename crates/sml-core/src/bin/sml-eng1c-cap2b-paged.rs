//! ENG-1c/CAP2b: immutable corpus-frozen TRAIN-only paged Witten-Bell store.
//!
//! Full real-language TRAIN is fitted and indexed; this harness only performs
//! fixed, TRAIN-derived structural/probability probes. It does NOT compute
//! TRAIN/VAL/TEST loss or select depth/quality policies.

use std::{error::Error, fs, hint::black_box, path::Path, time::Instant};

use sml_core::{
    anf_char::LANES,
    elastic_ngram_paged::{build_paged, training_sha256, PagedBuild, PagedLimits, PagedNGram},
    elastic_ngram_paged_sha::hex,
};

const CHOICES: [usize; 4] = [4, 7, 12, 32];
const WARMUP: usize = 64;
const TIMED_PROBES: usize = 256;

struct Corpus {
    language: &'static str,
    train_bytes: usize,
    events: u64,
}

fn frozen(language: &str) -> Option<Corpus> {
    match language {
        "FR" => Some(Corpus {
            language: "FR",
            train_bytes: 598_284,
            events: 598_208,
        }),
        "EN" => Some(Corpus {
            language: "EN",
            train_bytes: 590_360,
            events: 590_272,
        }),
        _ => None,
    }
}

fn expected_cardinality(language: &str, depth: usize) -> Option<(u64, u64)> {
    match (language, depth) {
        ("FR", 4) => Some((40_329, 117_282)),
        ("FR", 7) => Some((491_517, 799_782)),
        ("FR", 12) => Some((2_597_815, 3_131_101)),
        ("FR", 32) => Some((14_281_533, 14_878_280)),
        ("EN", 4) => Some((41_802, 118_117)),
        ("EN", 7) => Some((471_036, 759_560)),
        ("EN", 12) => Some((2_477_266, 2_995_126)),
        ("EN", 32) => Some((13_932_625, 14_519_708)),
        _ => None,
    }
}

fn vmhwm_kib() -> Result<u64, Box<dyn Error>> {
    let status = fs::read_to_string("/proc/self/status")?;
    let line = status
        .lines()
        .find(|line| line.starts_with("VmHWM:"))
        .ok_or("Linux VmHWM unavailable")?;
    let mut parts = line.split_whitespace();
    if parts.next() != Some("VmHWM:") {
        return Err("invalid VmHWM field".into());
    }
    let kib = parts
        .next()
        .ok_or("missing memory quantity")?
        .parse::<u64>()?;
    if kib == 0 || parts.next() != Some("kB") || parts.next().is_some() {
        return Err("invalid physical RSS peak quantity".into());
    }
    Ok(kib)
}

fn train_history<'a>(bytes: &'a [u8], index: usize) -> &'a [u8] {
    let len = bytes.len() / LANES;
    let lane = (index * 17) % LANES;
    let t = 1 + (index * 13) % (len - 1);
    &bytes[lane * len..lane * len + t]
}

struct ProbeStats {
    digest: u64,
    p50_ns: u128,
    p95_ns: u128,
}

/// The fixed probes use TRAIN histories with no labels/score and cannot
/// establish language prediction quality or a memory policy.
fn probe(model: &mut PagedNGram, bytes: &[u8], depth: usize) -> Result<ProbeStats, Box<dyn Error>> {
    for i in 0..WARMUP {
        black_box(model.distribution(train_history(bytes, i), depth)?);
    }
    let mut checksum = 0x517c_c1b7_d529_a4e3u64;
    let mut timings = Vec::with_capacity(TIMED_PROBES);
    for i in 0..TIMED_PROBES {
        let history = train_history(bytes, (i * 13 + 5) % (LANES * (bytes.len() / LANES - 1)));
        let now = Instant::now();
        let probs = model.distribution(history, depth)?;
        let time = now.elapsed().as_nanos();
        if time == 0 {
            return Err("zero-duration lookup".into());
        }
        timings.push(time);
        for p in probs {
            checksum = checksum.rotate_left(5) ^ p.to_bits();
        }
    }
    timings.sort_unstable();
    Ok(ProbeStats {
        digest: checksum,
        p50_ns: timings[TIMED_PROBES / 2],
        p95_ns: timings[TIMED_PROBES * 95 / 100],
    })
}

fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if args.len() != 6 {
        return Err(
            "usage: sml-eng1c-cap2b-paged build|reopen FR|EN 4|7|12|32 <TRAIN-file> <output-store-directory>"
                .into(),
        );
    }
    let action = args[1].as_str();
    if action != "build" && action != "reopen" {
        return Err("unknown CAP2b action".into());
    }
    let book = frozen(&args[2]).ok_or("unknown frozen book")?;
    let depth = args[3].parse::<usize>()?;
    if !CHOICES.contains(&depth) {
        return Err("depth not in predeclared CAP2b full-TRAIN ladder".into());
    }
    let train = fs::read(&args[4])?;
    if train.len() != book.train_bytes {
        return Err("frozen TRAIN prefix size mismatch".into());
    }
    let sha = training_sha256(&train);
    let output = Path::new(&args[5]);

    let build_status = if action == "build" {
        let scratch = output.with_extension("sort-scratch");
        let outcome = build_paged(&train, depth, output, &scratch, PagedLimits::default())?;
        match outcome {
            PagedBuild::Published(meta) => {
                if meta.source_sha256 != sha {
                    return Err("paged source SHA mismatch".into());
                }
                "PUBLISHED"
            }
            PagedBuild::CapacityStop(quota) => {
                println!(
                    "ENG1C_CAP2B,language={},depth={},status=CAPACITY_STOP,quota={quota:?},train_events={},contexts=UNKNOWN,follower_pairs=UNKNOWN,quality=NOT_EVALUATED",
                    book.language, depth, book.events
                );
                return Ok(());
            }
        }
    } else {
        "REOPENED"
    };

    let mut model = PagedNGram::open(output, sha)?;
    let meta = model.metadata().clone();
    if meta.train_events != book.events
        || meta.train_bytes != book.train_bytes as u64
        || meta.max_depth != depth
    {
        return Err("paged source/depth/train event mismatch".into());
    }
    let (expected_nodes, expected_pairs) =
        expected_cardinality(book.language, depth).ok_or("unknown frozen cardinality oracle")?;
    if (meta.contexts, meta.follower_pairs) != (expected_nodes, expected_pairs) {
        return Err("CAP2b exact model cardinality disagrees with preregistered CAP2a".into());
    }

    let evidence = probe(&mut model, &train, depth)?;
    let rss = vmhwm_kib()?;
    println!(
        "ENG1C_CAP2B,language={},depth={},status={},train_events={},train_sha256={},source_records={},contexts={},follower_pairs={},counts_bytes={},index_bytes={},index_entries={},scratch_peak_bytes={},sort_runs={},vmhwm_kib={},p50_ns={},p95_ns={},probe_checksum={:016x},quality=NOT_EVALUATED,integrity=PASS",
        book.language, depth, build_status, meta.train_events, hex(&meta.source_sha256),
        meta.source_records, meta.contexts, meta.follower_pairs,
        meta.count_store_bytes, meta.sparse_index_bytes, meta.sparse_entries,
        meta.scratch_peak_bytes, meta.sorted_runs, rss,
        evidence.p50_ns, evidence.p95_ns, evidence.digest
    );
    Ok(())
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if let Err(error) = run(&args) {
        eprintln!("ENG1C_CAP2B_FAILURE,{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap2b_pinned_cardinality_contract_and_probes_are_deterministic() {
        assert_eq!(
            expected_cardinality("FR", 32),
            Some((14_281_533, 14_878_280))
        );
        assert_eq!(expected_cardinality("EN", 12), Some((2_477_266, 2_995_126)));
        assert_eq!(expected_cardinality("EN", 3), None);
        assert_eq!(frozen("FR").unwrap().train_bytes, 598_284);
        assert_eq!(frozen("EN").unwrap().events, 590_272);
        let data = (0..LANES * 40).map(|t| (t % 251) as u8).collect::<Vec<_>>();
        assert_eq!(train_history(&data, 0).len(), 1);
        assert_eq!(train_history(&data, 17).len(), 27);
    }
}
