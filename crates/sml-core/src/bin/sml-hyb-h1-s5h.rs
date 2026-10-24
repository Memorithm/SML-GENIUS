//! HYB H-1: ternary residual head on GB-11 P1 S5h traces.
//! Protocol: docs/HYB-H1-S5H-TERNARY-RESIDUAL-PREREGISTRATION.md.
//!
//! `run TRACE_DIR NEW_OUTPUT_DIR` runs the frozen arms on `trace-train.bin` /
//! `trace-dev.bin`, after recomputing the P1 BPC that `trace.json` reports for
//! both traces (the run stops on any mismatch); `replay HEAD.tsv`
//! prints the i32 score digest of an exported head on synthetic records
//! (cross-host check); `smoke` runs the pipeline on generated traces and is
//! labelled SMOKE, never a result. TEST is never read. NOT_PROMOTED.

use sml_core::{
    elastic_ngram_paged_sha::{hex, sha256},
    gb11_p1_stream::{read_trace, TraceRecord},
    hyb_h1_residual::{
        a0_cost, replay_records, splitmix, CountTable, Head, Master, DEPTHS, INPUTS, LAMBDAS, TAUS,
    },
};
use std::{
    collections::BTreeSet,
    error::Error,
    fs::{self, File},
    io::Write,
    path::Path,
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const SEEDS: [u64; 2] = [11, 23];
const GO_MARGIN_BPC: f64 = 0.02;
const REPLAY_RECORDS: usize = 65_536;

/// Order, split and P1 cost of one trace file.
struct Scan {
    docs: BTreeSet<u32>,
    records: u64,
    bits: f64,
}

fn scan(path: &Path, split: u8) -> Result<Scan> {
    let mut docs = BTreeSet::new();
    let mut last: Option<(u32, u32)> = None;
    let mut error = None;
    let (records, bits) = read_trace(&mut File::open(path)?, |r| {
        if error.is_some() {
            return;
        }
        let key = (r.doc_index, r.position);
        if r.split != split {
            error = Some(format!("{}: split {} != {split}", path.display(), r.split));
        } else if last.is_some_and(|prev| key <= prev) {
            error = Some(format!("{}: duplicate or unsorted {key:?}", path.display()));
        }
        last = Some(key);
        docs.insert(r.doc_index);
    })?;
    if let Some(error) = error {
        return Err(error.into());
    }
    if records == 0 {
        return Err(format!("{}: empty trace", path.display()).into());
    }
    Ok(Scan {
        docs,
        records,
        bits,
    })
}

fn bpc(bits: f64, records: u64) -> f64 {
    bits / records as f64
}

/// The `"bpc"` that `trace.json` reports on the line of `key` (the P1
/// producer writes one line per trace).
fn reported_bpc(json: &str, key: &str) -> Result<f64> {
    let line = json
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("\"{key}\"")))
        .ok_or_else(|| format!("trace.json has no {key}"))?;
    let rest = line
        .split_once("\"bpc\": ")
        .ok_or_else(|| format!("trace.json {key} has no bpc"))?
        .1;
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    Ok(rest[..end].parse()?)
}

fn check_reported(json: &str, key: &str, got: f64) -> Result<()> {
    let want = reported_bpc(json, key)?;
    if (got - want).abs() > 1e-6 {
        return Err(format!("{key} P1 BPC {got:.9} != trace.json {want:.9}").into());
    }
    Ok(())
}

fn head_tsv(head: &Head) -> String {
    let weights: Vec<String> = head.weights().iter().map(i8::to_string).collect();
    let bias: Vec<String> = head.bias().iter().map(i32::to_string).collect();
    format!(
        "inputs\t{INPUTS}\ndepths\t{DEPTHS}\nbias\t{}\nweights\t{}\n",
        bias.join(","),
        weights.join(",")
    )
}

fn parse_head(text: &str) -> Result<Head> {
    let field = |name: &str| -> Result<&str> {
        text.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix('\t'))
            .ok_or_else(|| format!("missing {name}").into())
    };
    if field("inputs")?.parse::<usize>()? != INPUTS || field("depths")?.parse::<usize>()? != DEPTHS
    {
        return Err("head shape mismatch".into());
    }
    let bias: Vec<i32> = field("bias")?
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    let weights: Vec<i8> = field("weights")?
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    if weights.len() != DEPTHS * INPUTS || weights.iter().any(|w| !(-1..=1).contains(w)) {
        return Err("weights must be DEPTHS*INPUTS ternary values".into());
    }
    Ok(Head::new(
        weights,
        bias.try_into()
            .map_err(|_| "bias must have DEPTHS values")?,
    ))
}

/// SHA-256 of every i32 score of `head` on the fixed synthetic records.
fn replay_digest(head: &Head) -> String {
    let mut bytes = Vec::with_capacity(REPLAY_RECORDS * DEPTHS * 4);
    for record in replay_records(REPLAY_RECORDS) {
        for d in 0..DEPTHS {
            bytes.extend_from_slice(&head.score(&record, d).to_le_bytes());
        }
    }
    hex(&sha256(&bytes))
}

fn run(dir: &Path, out: &Path, label: &str) -> Result<()> {
    let json = fs::read_to_string(dir.join("trace.json"))?;
    fs::create_dir(out)?;
    let train_path = dir.join("trace-train.bin");
    let dev_path = dir.join("trace-dev.bin");
    let train = scan(&train_path, 0)?;
    let dev = scan(&dev_path, 1)?;
    if train.docs.intersection(&dev.docs).next().is_some() {
        return Err("a doc_index appears in both TRAIN and DEV".into());
    }
    let a0_train = bpc(train.bits, train.records);
    let a0_dev = bpc(dev.bits, dev.records);
    check_reported(&json, "train_trace", a0_train)?;
    check_reported(&json, "dev_trace", a0_dev)?;
    // TRAIN-HOLD: the last 1/16 of traced TRAIN files, by doc_index.
    let hold_count = train.docs.len().div_ceil(16);
    let hold_from = *train
        .docs
        .iter()
        .nth(train.docs.len() - hold_count)
        .expect("non-empty");

    let mut masters: Vec<Master> = SEEDS.iter().map(|&s| Master::new(s)).collect();
    let mut table = CountTable::with_bytes(Head::resident_bytes());
    let mut hold = Vec::new();
    read_trace(&mut File::open(&train_path)?, |r| {
        if r.doc_index >= hold_from {
            hold.push(*r);
        } else {
            for master in &mut masters {
                master.train(r);
            }
            table.train(r);
        }
    })?;
    let hold_cost = |cost: &dyn Fn(&TraceRecord) -> f64| -> f64 {
        hold.iter().map(cost).sum::<f64>() / hold.len() as f64
    };
    let (lambda, _) = LAMBDAS
        .iter()
        .map(|&l| (l, hold_cost(&|r| table.cost(r, l))))
        .fold((0.0, f64::INFINITY), |a, b| if b.1 < a.1 { b } else { a });
    let heads: Vec<(f32, Head)> = masters
        .iter()
        .map(|master| {
            TAUS.iter()
                .map(|&tau| {
                    let head = master.export(tau);
                    (hold_cost(&|r| head.cost(r)), tau, head)
                })
                .fold(None, |best: Option<(f64, f32, Head)>, c| match best {
                    Some(b) if b.0 <= c.0 => Some(b),
                    _ => Some(c),
                })
                .map(|(_, tau, head)| (tau, head))
                .expect("TAUS is non-empty")
        })
        .collect();
    drop(hold);

    let mut a0c_bits = 0.0;
    let mut a1_bits = vec![0.0; heads.len()];
    let mut mismatches = 0u64;
    let mut nanos = [0u128; 3];
    read_trace(&mut File::open(&dev_path)?, |r| {
        let t = Instant::now();
        std::hint::black_box(a0_cost(r));
        nanos[0] += t.elapsed().as_nanos();
        let t = Instant::now();
        a0c_bits += table.cost(r, lambda);
        nanos[1] += t.elapsed().as_nanos();
        for (k, (_, head)) in heads.iter().enumerate() {
            let t = Instant::now();
            a1_bits[k] += head.cost(r);
            if k == 0 {
                nanos[2] += t.elapsed().as_nanos();
            }
            for d in 0..DEPTHS {
                if head.score(r, d) != head.oracle_score(r, d) {
                    mismatches += 1;
                }
            }
        }
    })?;
    if mismatches != 0 {
        return Err(format!("{mismatches} kernel/oracle score mismatches").into());
    }

    let a0c_dev = bpc(a0c_bits, dev.records);
    let a1_dev: Vec<f64> = a1_bits.iter().map(|&b| bpc(b, dev.records)).collect();
    let reference = a0_dev.min(a0c_dev);
    let spread = (a1_dev[0] - a1_dev[1]).abs();
    let go = a1_dev
        .iter()
        .all(|&b| b < reference - GO_MARGIN_BPC && reference - b > 2.0 * spread);
    let per_pos = |n: u128| n as f64 / 1000.0 / dev.records as f64;
    let mut report = format!(
        "label\t{label}\ntrain_records\t{}\ndev_records\t{}\ntrain_hold_files\t{hold_count}\n\
         a0_train_bpc\t{a0_train:.9}\na0_dev_bpc\t{a0_dev:.9}\n\
         a0c_dev_bpc\t{a0c_dev:.9}\na0c_lambda\t{lambda}\na0c_bytes\t{}\n\
         head_bytes\t{}\nkernel_oracle_mismatches\t0\n\
         us_per_position_a0\t{:.4}\nus_per_position_a0c\t{:.4}\nus_per_position_a1\t{:.4}\n",
        train.records,
        dev.records,
        table.bytes(),
        Head::resident_bytes(),
        per_pos(nanos[0]),
        per_pos(nanos[1]),
        per_pos(nanos[2]),
    );
    for (k, (tau, head)) in heads.iter().enumerate() {
        let seed = SEEDS[k];
        report += &format!(
            "a1_seed{seed}_dev_bpc\t{:.9}\na1_seed{seed}_tau\t{tau}\n\
             a1_seed{seed}_zero_fraction\t{:.6}\na1_seed{seed}_replay_sha256\t{}\n",
            a1_dev[k],
            head.zero_fraction(),
            replay_digest(head),
        );
        fs::write(out.join(format!("head-seed{seed}.tsv")), head_tsv(head))?;
    }
    report += &format!(
        "witness_s5h_dev_bpc\t3.655\nseed_spread_bpc\t{spread:.9}\nverdict\t{}\n",
        if go { "GO" } else { "NO_GO" }
    );
    fs::write(out.join("h1.tsv"), &report)?;
    print!("{report}");
    Ok(())
}

/// Writes a small synthetic TRAIN/DEV trace pair with a learnable residual,
/// and a `trace.json` reporting their P1 BPC (zero logits: 8 bits).
fn write_smoke_traces(dir: &Path) -> Result<()> {
    write_trace_json(dir, 8.0)?;
    fs::create_dir_all(dir)?;
    for (name, split, docs, base) in [("train", 0u8, 160u32, 0u32), ("dev", 1, 20, 1000)] {
        let mut file =
            std::io::BufWriter::new(File::create(dir.join(format!("trace-{name}.bin")))?);
        for doc in 0..docs {
            for position in 0..200u32 {
                let s = splitmix(u64::from(base + doc) << 32 | u64::from(position));
                let record = TraceRecord {
                    doc_index: base + doc,
                    position,
                    split,
                    target: (((s >> 3) & 1) << 7) as u8 | (s >> 24) as u8 & 0x7f,
                    word: (s >> 40) as u16,
                    context: s,
                    logits: [0; 8],
                    confidence: 0,
                };
                file.write_all(&record.encode())?;
            }
        }
        file.flush()?;
    }
    Ok(())
}

fn write_trace_json(dir: &Path, bpc: f64) -> Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(
        dir.join("trace.json"),
        format!(
            "{{\n  \"train_trace\": {{\"file\": \"trace-train.bin\", \"bpc\": {bpc:.9}}},\n  \"dev_trace\": {{\"file\": \"trace-dev.bin\", \"bpc\": {bpc:.9}}}\n}}\n"
        ),
    )?;
    Ok(())
}

fn cli(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str) {
        Some("run") if args.len() == 4 => {
            run(Path::new(&args[2]), Path::new(&args[3]), "PREREGISTERED")
        }
        Some("replay") if args.len() == 3 => {
            let head = parse_head(&fs::read_to_string(&args[2])?)?;
            println!("replay_sha256\t{}", replay_digest(&head));
            Ok(())
        }
        Some("smoke") if args.len() == 3 => {
            let root = Path::new(&args[2]);
            write_smoke_traces(&root.join("traces"))?;
            run(&root.join("traces"), &root.join("out"), "SMOKE")
        }
        _ => Err("usage: run TRACE_DIR NEW_OUT | replay HEAD.tsv | smoke NEW_DIR".into()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(error) = cli(&args) {
        eprintln!("sml-hyb-h1-s5h: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("hyb-h1-{tag}-{}", std::process::id()))
    }

    #[test]
    fn smoke_pipeline_learns_and_replays_heads() {
        let root = temp("smoke");
        let _ = fs::remove_dir_all(&root);
        cli(&["bin".into(), "smoke".into(), root.display().to_string()]).unwrap();
        let report = fs::read_to_string(root.join("out/h1.tsv")).unwrap();
        let value = |key: &str| -> f64 {
            report
                .lines()
                .find_map(|l| l.strip_prefix(key)?.strip_prefix('\t'))
                .unwrap()
                .parse()
                .unwrap()
        };
        assert!(
            (value("a0_dev_bpc") - 8.0).abs() < 1e-9,
            "stretch 0 costs 1 bit per decision"
        );
        assert!(value("a1_seed11_dev_bpc") < value("a0_dev_bpc") - 0.2);
        let head =
            parse_head(&fs::read_to_string(root.join("out/head-seed11.tsv")).unwrap()).unwrap();
        assert!(report.contains(&format!(
            "a1_seed11_replay_sha256\t{}",
            replay_digest(&head)
        )));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn invalid_traces_are_refused() {
        let root = temp("invalid");
        let _ = fs::remove_dir_all(&root);
        write_smoke_traces(&root).unwrap();
        // DEV reusing a TRAIN doc_index.
        let mut dev = File::create(root.join("trace-dev.bin")).unwrap();
        let record = TraceRecord {
            split: 1,
            ..TraceRecord::default()
        };
        dev.write_all(&record.encode()).unwrap();
        drop(dev);
        assert!(run(&root, &root.join("out1"), "SMOKE").is_err());
        // Out-of-order records.
        let mut dev = File::create(root.join("trace-dev.bin")).unwrap();
        for position in [5u32, 4] {
            let r = TraceRecord {
                doc_index: 9999,
                position,
                split: 1,
                ..TraceRecord::default()
            };
            dev.write_all(&r.encode()).unwrap();
        }
        drop(dev);
        assert!(run(&root, &root.join("out2"), "SMOKE").is_err());
        // trace.json reporting another P1 BPC, or missing.
        write_smoke_traces(&root).unwrap();
        write_trace_json(&root, 7.5).unwrap();
        assert!(run(&root, &root.join("out3"), "SMOKE").is_err());
        fs::remove_file(root.join("trace.json")).unwrap();
        assert!(run(&root, &root.join("out4"), "SMOKE").is_err());
        assert!(cli(&[
            "bin".into(),
            "run".into(),
            "a".into(),
            "b".into(),
            "1".into(),
            "2".into()
        ])
        .is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
