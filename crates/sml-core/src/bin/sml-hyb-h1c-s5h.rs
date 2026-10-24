//! HYB H-1c: averaged ternary residual head, popcount kernel, judged on
//! EVAL2. Protocol: docs/HYB-H1C-S5H-STABLE-RESIDUAL-PREREGISTRATION.md.
//!
//! `run P1_H1C_OUT NEW_OUTPUT_DIR` evaluates both bases (`search_q4`,
//! `bytes_q4`) written by `sml-gb11-p1-s5h run-h1c`: each base directory has
//! `trace-cal.bin` (split 2), `trace-eval2.bin` (split 3) and `trace.json`,
//! whose P1 BPC is recomputed first (the run stops on any mismatch). Each
//! seed trains `EPOCHS` permuted epochs on CAL-FIT (`doc_index % 32 == 7`)
//! and exports its averaged master; (tau, lambda) are picked on CAL-SEL
//! (`doc_index % 32 == 23`); EVAL2 is read once at the end. The S5h DEV
//! trace is never opened. `replay HEAD.tsv` prints the i32 digest of an
//! exported head on synthetic records (cross-host check); `smoke` runs on
//! generated traces and is labelled SMOKE, never a result. TEST is never
//! read. NOT_PROMOTED.

use sml_core::{
    elastic_ngram_paged_sha::{hex, sha256},
    gb11_p1_stream::{read_trace, TraceRecord, TRACE_RECORD_BYTES},
    hyb_h1_residual::{a0_cost, replay_records, splitmix, CountTable, DEPTHS, INPUTS},
    hyb_h1b_calibrated::{ScaledHead, LAMBDA_EIGHTHS, TABLE_LAMBDAS, TAUS},
    hyb_h1c_stable::{train_averaged, FastHead, BURN_IN_EPOCHS, EPOCHS},
};
use std::{
    error::Error,
    fs::{self, File},
    io::{BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const BASES: [&str; 2] = ["search_q4", "bytes_q4"];
const SEEDS: [u64; 2] = [11, 23];
const GO_MARGIN_BPC: f64 = 0.02;
const MAX_COST_RATIO: f64 = 1.5;
const REPLAY_RECORDS: usize = 65_536;
const CAL_FIT: u32 = 7;
const CAL_SEL: u32 = 23;

/// One document's contiguous records in a trace file.
#[derive(Clone, Copy)]
struct Span {
    doc: u32,
    start: u64,
    count: u64,
}

/// Order, split, document spans and P1 cost of one trace file.
struct Scan {
    spans: Vec<Span>,
    records: u64,
    bits: f64,
}

fn scan(path: &Path, split: u8) -> Result<Scan> {
    let mut spans: Vec<Span> = Vec::new();
    let mut last: Option<(u32, u32)> = None;
    let mut error = None;
    let mut index = 0u64;
    let (records, bits) = read_trace(&mut File::open(path)?, |r| {
        let key = (r.doc_index, r.position);
        if error.is_none() {
            if r.split != split {
                error = Some(format!("{}: split {} != {split}", path.display(), r.split));
            } else if last.is_some_and(|prev| key <= prev) {
                error = Some(format!("{}: duplicate or unsorted {key:?}", path.display()));
            }
        }
        last = Some(key);
        match spans.last_mut() {
            Some(span) if span.doc == r.doc_index => span.count += 1,
            _ => spans.push(Span {
                doc: r.doc_index,
                start: index,
                count: 1,
            }),
        }
        index += 1;
    })?;
    if let Some(error) = error {
        return Err(error.into());
    }
    if records == 0 {
        return Err(format!("{}: empty trace", path.display()).into());
    }
    Ok(Scan {
        spans,
        records,
        bits,
    })
}

fn bpc(bits: f64, records: u64) -> f64 {
    bits / records as f64
}

/// The number after `"field": ` on the line of `key` in `trace.json`.
fn reported(json: &str, key: &str, field: &str) -> Result<f64> {
    let line = json
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("\"{key}\"")))
        .ok_or_else(|| format!("trace.json has no {key}"))?;
    let rest = line
        .split_once(&format!("\"{field}\": "))
        .map_or(line, |(_, rest)| rest);
    let rest = rest
        .strip_prefix(&format!("\"{key}\": "))
        .unwrap_or(rest)
        .trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    if end == 0 {
        return Err(format!("trace.json {key} has no {field}").into());
    }
    Ok(rest[..end].parse()?)
}

fn check_reported(json: &str, key: &str, got: f64) -> Result<()> {
    let want = reported(json, key, "bpc")?;
    if (got - want).abs() > 1e-6 {
        return Err(format!("{key} P1 BPC {got:.9} != trace.json {want:.9}").into());
    }
    Ok(())
}

/// Reads the records of `span` from an open trace file.
fn each_in_span(file: &mut File, span: Span, mut each: impl FnMut(&TraceRecord)) -> Result<()> {
    file.seek(SeekFrom::Start(span.start * TRACE_RECORD_BYTES as u64))?;
    let mut reader = BufReader::with_capacity(1 << 20, file.take(span.count * 40));
    let mut raw = [0u8; TRACE_RECORD_BYTES];
    for _ in 0..span.count {
        reader.read_exact(&mut raw)?;
        each(&TraceRecord::decode(&raw));
    }
    Ok(())
}

/// Exported head plus its chosen multiplier.
struct Chosen {
    tau: f32,
    eighths: i32,
    head: ScaledHead,
    fast: FastHead,
    sel_bpc: f64,
    snapshots: u64,
}

fn head_tsv(chosen: &Chosen) -> String {
    let join = |v: &[i32]| v.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
    let weights: Vec<String> = chosen.head.weights().iter().map(i8::to_string).collect();
    format!(
        "inputs\t{INPUTS}\ndepths\t{DEPTHS}\nlambda_eighths\t{}\nunit\t{}\nbias\t{}\nweights\t{}\n",
        chosen.eighths,
        join(&chosen.head.unit()),
        join(&chosen.head.bias()),
        weights.join(",")
    )
}

fn parse_head(text: &str) -> Result<(ScaledHead, i32)> {
    let field = |name: &str| -> Result<&str> {
        text.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix('\t'))
            .ok_or_else(|| format!("missing {name}").into())
    };
    if field("inputs")?.parse::<usize>()? != INPUTS || field("depths")?.parse::<usize>()? != DEPTHS
    {
        return Err("head shape mismatch".into());
    }
    let ints = |name: &str| -> Result<[i32; DEPTHS]> {
        let v: Vec<i32> = field(name)?
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        v.try_into()
            .map_err(|_| format!("{name} needs {DEPTHS} values").into())
    };
    let weights: Vec<i8> = field("weights")?
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    if weights.len() != DEPTHS * INPUTS || weights.iter().any(|w| !(-1..=1).contains(w)) {
        return Err("weights must be DEPTHS*INPUTS ternary values".into());
    }
    let eighths: i32 = field("lambda_eighths")?.parse()?;
    if !LAMBDA_EIGHTHS.contains(&eighths) {
        return Err("lambda outside the registered grid".into());
    }
    Ok((
        ScaledHead::new(weights, ints("unit")?, ints("bias")?),
        eighths,
    ))
}

/// SHA-256 of every i32 stretch correction of the popcount kernel on fixed
/// synthetic records; stops if the kernel and the oracle disagree.
fn replay_digest(head: &ScaledHead, eighths: i32) -> Result<String> {
    let fast = FastHead::new(head);
    let mut bytes = Vec::with_capacity(REPLAY_RECORDS * DEPTHS * 4);
    for record in replay_records(REPLAY_RECORDS) {
        for d in 0..DEPTHS {
            let raw = fast.raw(&record, d);
            if raw != head.oracle_raw(&record, d) {
                return Err("replay: kernel/oracle mismatch".into());
            }
            bytes.extend_from_slice(&fast.correction(raw, d, eighths).to_le_bytes());
        }
    }
    Ok(hex(&sha256(&bytes)))
}

/// One seed: averaged master over CAL-FIT, then CAL-SEL code lengths of
/// every exported head (one per tau) at every lambda.
type SelBits = [[f64; LAMBDA_EIGHTHS.len()]; TAUS.len()];

fn seed_heads(
    cal_path: &Path,
    seed: u64,
    fit: &[Span],
    sel: &[Span],
) -> Result<(Vec<ScaledHead>, SelBits, u64)> {
    let mut file = File::open(cal_path)?;
    let docs: Vec<(u32, Span)> = fit.iter().map(|s| (s.doc, *s)).collect();
    let (master, snapshots) = train_averaged(seed, &docs, |span, train| {
        each_in_span(&mut file, span, |r| train(r))
    })?
    .ok_or("no averaged snapshot")?;
    let heads: Vec<ScaledHead> = TAUS
        .iter()
        .map(|&t| ScaledHead::export(&master, t))
        .collect();
    let fast: Vec<FastHead> = heads.iter().map(FastHead::new).collect();
    let mut bits = [[0.0f64; LAMBDA_EIGHTHS.len()]; TAUS.len()];
    for &span in sel {
        each_in_span(&mut file, span, |r| {
            for (t, head) in fast.iter().enumerate() {
                for (k, c) in head.costs(r).iter().enumerate() {
                    bits[t][k] += c;
                }
            }
        })?;
    }
    Ok((heads, bits, snapshots))
}

fn evaluate(dir: &Path, out: &Path, base: &str) -> Result<(String, &'static str)> {
    let json = fs::read_to_string(dir.join("trace.json"))?;
    let cal_path = dir.join("trace-cal.bin");
    let eval_path = dir.join("trace-eval2.bin");
    let cal = scan(&cal_path, 2)?;
    let eval2 = scan(&eval_path, 3)?;
    let eval_docs: std::collections::BTreeSet<u32> = eval2.spans.iter().map(|s| s.doc).collect();
    if cal.spans.iter().any(|s| eval_docs.contains(&s.doc)) {
        return Err("a doc_index appears in both CAL and EVAL2".into());
    }
    let a0_cal = bpc(cal.bits, cal.records);
    let a0_eval = bpc(eval2.bits, eval2.records);
    check_reported(&json, "cal_trace", a0_cal)?;
    check_reported(&json, "eval2_trace", a0_eval)?;
    let p1_us = reported(&json, "eval2_us_per_position", "eval2_us_per_position")?;
    if p1_us.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return Err("trace.json eval2_us_per_position must be positive".into());
    }
    let (mut fit, mut sel) = (Vec::new(), Vec::new());
    for span in &cal.spans {
        match span.doc % 32 {
            CAL_FIT => fit.push(*span),
            CAL_SEL => sel.push(*span),
            _ => return Err(format!("CAL doc {} outside CAL-FIT/CAL-SEL", span.doc).into()),
        }
    }
    if fit.is_empty() || sel.is_empty() {
        return Err("CAL-FIT and CAL-SEL must both be non-empty".into());
    }
    let count = |spans: &[Span]| spans.iter().map(|s| s.count).sum::<u64>();
    let (fit_records, sel_records) = (count(&fit), count(&sel));

    // One thread per seed (averaged training, then CAL-SEL), one for the
    // count table (deterministic, document order) and A0 on CAL-SEL.
    let (seeds, table_part) = std::thread::scope(|scope| {
        let handles: Vec<_> = SEEDS
            .iter()
            .map(|&seed| {
                let (cal_path, fit, sel) = (&cal_path, &fit, &sel);
                scope.spawn(move || seed_heads(cal_path, seed, fit, sel))
            })
            .collect();
        let table = scope.spawn(
            || -> Result<(CountTable, [f64; TABLE_LAMBDAS.len()], f64)> {
                let mut file = File::open(&cal_path)?;
                let mut table = CountTable::with_bytes(ScaledHead::resident_bytes());
                for &span in &fit {
                    each_in_span(&mut file, span, |r| table.train(r))?;
                }
                let mut bits = [0.0f64; TABLE_LAMBDAS.len()];
                let mut a0 = 0.0;
                for &span in &sel {
                    each_in_span(&mut file, span, |r| {
                        a0 += a0_cost(r);
                        for (k, &l) in TABLE_LAMBDAS.iter().enumerate() {
                            bits[k] += table.cost(r, l);
                        }
                    })?;
                }
                Ok((table, bits, a0))
            },
        );
        let seeds: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().expect("seed thread"))
            .collect();
        (seeds, table.join().expect("table thread"))
    });
    let (table, table_bits, a0_sel) = table_part?;
    let mut chosen = Vec::new();
    for seed in seeds {
        let (heads, bits, snapshots) = seed?;
        let mut best = (f64::INFINITY, 0, 0);
        for (t, row) in bits.iter().enumerate() {
            for (k, &b) in row.iter().enumerate() {
                // Ties keep the smaller tau, then the smaller lambda.
                if b < best.0 {
                    best = (b, t, k);
                }
            }
        }
        let head = heads.into_iter().nth(best.1).expect("tau index");
        chosen.push(Chosen {
            tau: TAUS[best.1],
            eighths: LAMBDA_EIGHTHS[best.2],
            fast: FastHead::new(&head),
            head,
            sel_bpc: bpc(best.0, sel_records),
            snapshots,
        });
    }
    let (table_k, _) =
        table_bits.iter().enumerate().fold(
            (0, f64::INFINITY),
            |a, (k, &b)| if b < a.1 { (k, b) } else { a },
        );
    let lambda = TABLE_LAMBDAS[table_k];

    // EVAL2, read once.
    let mut a0c_bits = 0.0;
    let mut a1_bits = vec![0.0; chosen.len()];
    let mut mismatches = 0u64;
    let mut nanos = [0u128; 3];
    read_trace(&mut File::open(&eval_path)?, |r| {
        let t = Instant::now();
        std::hint::black_box(a0_cost(r));
        nanos[0] += t.elapsed().as_nanos();
        let t = Instant::now();
        a0c_bits += table.cost(r, lambda);
        nanos[1] += t.elapsed().as_nanos();
        for (k, c) in chosen.iter().enumerate() {
            let t = Instant::now();
            a1_bits[k] += c.fast.cost(r, c.eighths);
            if k == 0 {
                nanos[2] += t.elapsed().as_nanos();
            }
            for d in 0..DEPTHS {
                if c.fast.raw(r, d) != c.head.oracle_raw(r, d) {
                    mismatches += 1;
                }
            }
        }
    })?;
    if mismatches != 0 {
        return Err(format!("{base}: {mismatches} kernel/oracle mismatches").into());
    }

    let a0c_eval = bpc(a0c_bits, eval2.records);
    let a1_eval: Vec<f64> = a1_bits.iter().map(|&b| bpc(b, eval2.records)).collect();
    let reference = a0_eval.min(a0c_eval);
    let spread = (a1_eval[0] - a1_eval[1]).abs();
    let go = a1_eval
        .iter()
        .all(|&b| b < reference - GO_MARGIN_BPC && reference - b > 2.0 * spread);
    let per_pos = |n: u128| n as f64 / 1000.0 / eval2.records as f64;
    // Head cost on top of P1: A1 scoring minus the shared decision cost.
    let head_us = (per_pos(nanos[2]) - per_pos(nanos[0])).max(0.0);
    let cost_ratio = (p1_us + head_us) / p1_us;
    let verdict = match (go, cost_ratio <= MAX_COST_RATIO) {
        (true, true) => "GO",
        (true, false) => "GO_DIAGNOSTIC_COST",
        (false, _) => "NO_GO",
    };
    let mut report = format!(
        "{base}_cal_fit_records\t{fit_records}\n{base}_cal_sel_records\t{sel_records}\n\
         {base}_eval2_records\t{}\n{base}_eval2_docs\t{}\n{base}_dev_read\tfalse\n\
         {base}_a0_cal_bpc\t{a0_cal:.9}\n{base}_a0_sel_bpc\t{:.9}\n\
         {base}_a0_eval2_bpc\t{a0_eval:.9}\n{base}_a0c_eval2_bpc\t{a0c_eval:.9}\n\
         {base}_a0c_lambda\t{lambda}\n{base}_a0c_bytes\t{}\n{base}_head_bytes\t{}\n\
         {base}_epochs\t{EPOCHS}\n{base}_burn_in_epochs\t{BURN_IN_EPOCHS}\n\
         {base}_kernel_oracle_mismatches\t0\n{base}_us_per_position_p1\t{p1_us:.4}\n\
         {base}_us_per_position_head\t{head_us:.4}\n{base}_us_per_position_a0c_scoring\t{:.4}\n\
         {base}_cost_ratio\t{cost_ratio:.4}\n",
        eval2.records,
        eval_docs.len(),
        bpc(a0_sel, sel_records),
        table.bytes(),
        ScaledHead::resident_bytes(),
        per_pos(nanos[1]),
    );
    for (k, c) in chosen.iter().enumerate() {
        let seed = SEEDS[k];
        report += &format!(
            "{base}_a1_seed{seed}_eval2_bpc\t{:.9}\n{base}_a1_seed{seed}_sel_bpc\t{:.9}\n\
             {base}_a1_seed{seed}_tau\t{}\n{base}_a1_seed{seed}_lambda_eighths\t{}\n\
             {base}_a1_seed{seed}_unit\t{:?}\n{base}_a1_seed{seed}_zero_fraction\t{:.6}\n\
             {base}_a1_seed{seed}_snapshots\t{}\n{base}_a1_seed{seed}_replay_sha256\t{}\n",
            a1_eval[k],
            c.sel_bpc,
            c.tau,
            c.eighths,
            c.head.unit(),
            c.head.zero_fraction(),
            c.snapshots,
            replay_digest(&c.head, c.eighths)?,
        );
        fs::write(out.join(format!("head-{base}-seed{seed}.tsv")), head_tsv(c))?;
    }
    report += &format!("{base}_seed_spread_bpc\t{spread:.9}\nverdict_{base}\t{verdict}\n");
    Ok((report, verdict))
}

fn run(p1: &Path, out: &Path, label: &str) -> Result<()> {
    fs::create_dir(out)?;
    let mut report = format!("label\t{label}\n");
    let mut verdicts = Vec::new();
    for base in BASES {
        let (part, verdict) = evaluate(&p1.join(base), out, base)?;
        eprint!("{part}");
        report += &part;
        verdicts.push(verdict);
    }
    // Only bytes_q4 (the best Boolean) gates H-2.
    report += &format!("verdict\t{}\n", verdicts[1]);
    fs::write(out.join("h1c.tsv"), &report)?;
    print!("{report}");
    Ok(())
}

/// Writes CAL and EVAL2 traces for both bases, with a learnable residual
/// over P1 at zero logits, and their `trace.json`.
fn write_smoke(root: &Path) -> Result<()> {
    for base in BASES {
        let dir = root.join(base);
        fs::create_dir_all(&dir)?;
        let mut summary = (0.0, 0u64, 0.0, 0u64);
        for (name, split, docs) in [
            (
                "cal",
                2u8,
                (0..40u32).map(|k| 7 + 16 * k).collect::<Vec<_>>(),
            ),
            ("eval2", 3, (5000..5020).collect()),
        ] {
            let mut file =
                std::io::BufWriter::new(File::create(dir.join(format!("trace-{name}.bin")))?);
            for doc in docs {
                for position in 0..200u32 {
                    let s = splitmix(u64::from(doc) << 32 | u64::from(position));
                    let record = TraceRecord {
                        doc_index: doc,
                        position,
                        split,
                        target: (((s >> 3) & 1) << 7) as u8 | (s >> 24) as u8 & 0x7f,
                        word: (s >> 40) as u16,
                        context: s,
                        logits: [0; 8],
                        confidence: 0,
                    };
                    if split == 2 {
                        summary.0 += record.cost_bits();
                        summary.1 += 1;
                    } else {
                        summary.2 += record.cost_bits();
                        summary.3 += 1;
                    }
                    file.write_all(&record.encode())?;
                }
            }
            file.flush()?;
        }
        write_trace_json(
            &dir,
            summary.0 / summary.1 as f64,
            summary.2 / summary.3 as f64,
        )?;
    }
    Ok(())
}

fn write_trace_json(dir: &Path, cal_bpc: f64, eval2_bpc: f64) -> Result<()> {
    fs::write(
        dir.join("trace.json"),
        format!(
            "{{\n  \"cal_trace\": {{\"file\": \"trace-cal.bin\", \"bpc\": {cal_bpc:.9}}},\n  \"eval2_trace\": {{\"file\": \"trace-eval2.bin\", \"bpc\": {eval2_bpc:.9}}},\n  \"eval2_us_per_position\": 1.000000\n}}\n"
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
            let (head, eighths) = parse_head(&fs::read_to_string(&args[2])?)?;
            println!("replay_sha256\t{}", replay_digest(&head, eighths)?);
            Ok(())
        }
        Some("smoke") if args.len() == 3 => {
            let root = Path::new(&args[2]);
            write_smoke(&root.join("p1"))?;
            run(&root.join("p1"), &root.join("out"), "SMOKE")
        }
        _ => Err("usage: run P1_H1C_OUT NEW_OUT | replay HEAD.tsv | smoke NEW_DIR".into()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(error) = cli(&args) {
        eprintln!("sml-hyb-h1c-s5h: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("hyb-h1c-{tag}-{}", std::process::id()))
    }

    fn value(report: &str, key: &str) -> String {
        report
            .lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('\t'))
            .unwrap()
            .to_owned()
    }

    #[test]
    fn smoke_pipeline_learns_selects_and_replays() {
        let root = temp("smoke");
        let _ = fs::remove_dir_all(&root);
        cli(&["bin".into(), "smoke".into(), root.display().to_string()]).unwrap();
        let report = fs::read_to_string(root.join("out/h1c.tsv")).unwrap();
        for base in BASES {
            let a0: f64 = value(&report, &format!("{base}_a0_eval2_bpc"))
                .parse()
                .unwrap();
            assert!((a0 - 8.0).abs() < 1e-9);
            for seed in SEEDS {
                let a1: f64 = value(&report, &format!("{base}_a1_seed{seed}_eval2_bpc"))
                    .parse()
                    .unwrap();
                assert!(a1 < a0 - 0.5, "{base} seed {seed}: {a1}");
                let path = root.join(format!("out/head-{base}-seed{seed}.tsv"));
                let (head, eighths) = parse_head(&fs::read_to_string(path).unwrap()).unwrap();
                assert_eq!(
                    value(&report, &format!("{base}_a1_seed{seed}_replay_sha256")),
                    replay_digest(&head, eighths).unwrap()
                );
            }
        }
        assert!(report.contains("\nverdict\t"));
        assert_eq!(value(&report, "bytes_q4_dev_read"), "false");
        // DEV traces are never opened: the run works without them.
        assert!(!root.join("p1/bytes_q4/trace-dev.bin").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn invalid_inputs_are_refused() {
        let root = temp("invalid");
        let _ = fs::remove_dir_all(&root);
        write_smoke(&root).unwrap();
        // trace.json reporting another CAL BPC.
        write_trace_json(&root.join("bytes_q4"), 7.5, 8.0).unwrap();
        assert!(run(&root, &root.join("out1"), "SMOKE").is_err());
        // Missing trace.json.
        fs::remove_file(root.join("bytes_q4/trace.json")).unwrap();
        assert!(run(&root, &root.join("out2"), "SMOKE").is_err());
        // A CAL doc outside both CAL halves.
        write_smoke(&root).unwrap();
        let mut cal = fs::OpenOptions::new()
            .append(true)
            .open(root.join("search_q4/trace-cal.bin"))
            .unwrap();
        let stray = TraceRecord {
            doc_index: 4998,
            split: 2,
            ..TraceRecord::default()
        };
        cal.write_all(&stray.encode()).unwrap();
        drop(cal);
        assert!(run(&root, &root.join("out3"), "SMOKE").is_err());
        assert!(cli(&["bin".into(), "run".into(), "a".into()]).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reported_reads_nested_and_top_level_numbers() {
        let json = "{\n  \"cal_trace\": {\"file\": \"x\", \"bpc\": 3.25},\n  \"eval2_us_per_position\": 0.512000,\n}";
        assert_eq!(reported(json, "cal_trace", "bpc").unwrap(), 3.25);
        assert_eq!(
            reported(json, "eval2_us_per_position", "eval2_us_per_position").unwrap(),
            0.512
        );
        assert!(reported(json, "cal_trace", "records").is_err());
    }
}
