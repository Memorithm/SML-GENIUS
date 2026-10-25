//! HYB H-1d: averaged ternary head that sees every P1 bank, judged on EVAL3.
//! Protocol: docs/HYB-H1D-S5H-BANK-HEAD-PREREGISTRATION.md.
//!
//! Reads `bytes_q4/` written by `sml-gb11-p1-s5h`: `trace-cal.bin` (split
//! 2), its `.banks.bin` sidecar and `trace.json` from `run-h1d`, whose P1
//! BPC is recomputed first and whose weights must be the H-1b/H-1c
//! `bytes_q4` weights (`P1_WEIGHTS_SHA256`, smoke runs exempt). Per seed,
//! both heads train `EPOCHS` permuted epochs on CAL-FIT
//! (`doc_index % 32 == 7`) and export their averaged master:
//!
//! * A1c, the H-1c head (124 inputs), as a control on this run's CAL;
//! * A1d, the bank head (`hyb_h1d_bank_head`).
//!
//! (tau, lambda) are picked on CAL-SEL (`doc_index % 32 == 23`).
//!
//! * `gate P1_H1D_OUT NEW_OUTPUT_DIR` is stage 1: `CAL_GATE_PASS` when the
//!   CAL-SEL gain of A1d over P1 reaches `CAL_GATE_BPC` for both seeds,
//!   otherwise `NO_GO_CAL_GATE`. EVAL3 does not exist yet: P1 traces it
//!   (`trace-h1d-eval3`) only after a pass.
//! * `run P1_H1D_OUT NEW_OUTPUT_DIR` redoes stage 1 (deterministic) and,
//!   if the gate passes, reads `trace-eval3.bin`, its sidecar and
//!   `eval3.json` once.
//!
//! * `cost P1_H1D_OUT H1D_OUT NEW_OUTPUT_DIR` is HYB H-1d-cost
//!   (docs/HYB-H1D-COST-PREREGISTRATION.md): it re-times the two A1d heads
//!   of a finished H-1d run (`H1D_OUT`) with `FusedBankHead`. EVAL3 is read
//!   again only to check that every correction and both EVAL3 BPC are
//!   those of H-1d bit for bit; nothing is selected or judged on it. The
//!   old kernel is timed in the same loop as a control.
//!
//! The S5h DEV is never opened. `replay
//! HEAD.tsv` prints the i32 digest of an A1d head on synthetic records
//! (cross-host check); `smoke` runs on generated traces and is labelled
//! SMOKE, never a result. TEST is never read. NOT_PROMOTED.

use sml_core::{
    elastic_ngram_paged_sha::{hex, sha256},
    gb11_p1_stream::{read_trace, TraceRecord, MAX_BANKS, TRACE_RECORD_BYTES},
    hyb_h1_residual::{a0_cost, splitmix, DEPTHS},
    hyb_h1b_calibrated::{ScaledHead, LAMBDA_EIGHTHS, TAUS},
    hyb_h1c_stable::{self, FastHead, BURN_IN_EPOCHS, EPOCHS},
    hyb_h1d_bank_head::{
        replay_bank_records, train_averaged, BankHead, BankPlane, BankRecord, FusedBankHead,
        BANK_PLANE_BYTES, INPUTS, ROW,
    },
};
use std::{
    error::Error,
    fs::{self, File},
    io::{BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const BASE: &str = "bytes_q4";
const SEEDS: [u64; 2] = [11, 23];
const GO_MARGIN_BPC: f64 = 0.02;
/// Stage 1: smallest CAL-SEL gain of A1d over P1 that opens EVAL3.
const CAL_GATE_BPC: f64 = 0.03;
const MAX_COST_RATIO: f64 = 1.5;
const REPLAY_RECORDS: usize = 65_536;
const CAL_FIT: u32 = 7;
const CAL_SEL: u32 = 23;
/// `bytes_q4` weights of the H-1b and H-1c runs: a preregistered run on
/// other P1 weights is refused.
const P1_WEIGHTS_SHA256: &str = "7f4c233d6c4d67e1d300068799f4653cfcaedda4158587478c99ec1d31c3c418";
/// SHA-256 of `head-a1d-seed11.tsv` and `head-a1d-seed23.tsv` of the H-1d
/// run (RemoteOps run 40126579964): H-1d-cost re-times exactly these.
const H1D_HEAD_SHA256: [&str; 2] = [
    "b77e3f1301c414ff21246b3c5ad600b07e2adf44633a671ecd90fd1160e5e46e",
    "3c6192e8a248fb47680d941866e6cf22d6483636f539fa2aa4564430127c990f",
];

/// One document's contiguous records in a trace file.
#[derive(Clone, Copy)]
struct Span {
    doc: u32,
    start: u64,
    count: u64,
}

struct Scan {
    spans: Vec<Span>,
    records: u64,
    bits: f64,
}

/// Checks split and order of a trace and that its sidecar has one plane
/// per record.
fn scan(path: &Path, banks: &Path, split: u8) -> Result<Scan> {
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
    if fs::metadata(banks)?.len() != records * BANK_PLANE_BYTES as u64 {
        return Err(format!("{}: one bank plane per record expected", banks.display()).into());
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

/// The string value of `"key": "..."`.
fn json_str<'a>(json: &'a str, key: &str) -> Result<&'a str> {
    let pattern = format!("\"{key}\": \"");
    let rest = &json[json
        .find(&pattern)
        .ok_or_else(|| format!("trace.json has no {key}"))?
        + pattern.len()..];
    Ok(&rest[..rest.find('"').ok_or("unterminated string")?])
}

fn check_reported(json: &str, key: &str, got: f64) -> Result<()> {
    let want = reported(json, key, "bpc")?;
    if (got - want).abs() > 1e-6 {
        return Err(format!("{key} P1 BPC {got:.9} != trace.json {want:.9}").into());
    }
    Ok(())
}

/// A trace and its sidecar, opened together.
struct Pair {
    trace: File,
    banks: File,
}

impl Pair {
    fn open(dir: &Path, name: &str) -> Result<Self> {
        Ok(Self {
            trace: File::open(dir.join(format!("{name}.bin")))?,
            banks: File::open(dir.join(format!("{name}.banks.bin")))?,
        })
    }

    /// Calls `each` on the records of `span`; stops on a misaligned plane.
    fn each(&mut self, span: Span, mut each: impl FnMut(&BankRecord)) -> Result<()> {
        self.trace
            .seek(SeekFrom::Start(span.start * TRACE_RECORD_BYTES as u64))?;
        self.banks
            .seek(SeekFrom::Start(span.start * BANK_PLANE_BYTES as u64))?;
        let mut traces = BufReader::with_capacity(
            1 << 20,
            (&mut self.trace).take(span.count * TRACE_RECORD_BYTES as u64),
        );
        let mut planes = BufReader::with_capacity(
            1 << 20,
            (&mut self.banks).take(span.count * BANK_PLANE_BYTES as u64),
        );
        let mut raw = [0u8; TRACE_RECORD_BYTES];
        let mut plane = [0u8; BANK_PLANE_BYTES];
        for _ in 0..span.count {
            traces.read_exact(&mut raw)?;
            planes.read_exact(&mut plane)?;
            let r = BankRecord::pair(TraceRecord::decode(&raw), BankPlane::decode(&plane))
                .ok_or("trace and bank sidecar are misaligned")?;
            each(&r);
        }
        Ok(())
    }
}

type SelBits = [[f64; LAMBDA_EIGHTHS.len()]; TAUS.len()];

/// (tau index, lambda index, bits) of the smallest CAL-SEL code length;
/// ties keep the smaller tau, then the smaller lambda.
fn pick(bits: &SelBits) -> (usize, usize, f64) {
    let mut best = (0, 0, f64::INFINITY);
    for (t, row) in bits.iter().enumerate() {
        for (k, &b) in row.iter().enumerate() {
            if b < best.2 {
                best = (t, k, b);
            }
        }
    }
    best
}

struct ChosenD {
    tau: f32,
    eighths: i32,
    head: BankHead,
    sel_bpc: f64,
    snapshots: u64,
}

struct ChosenC {
    eighths: i32,
    fast: FastHead,
    sel_bpc: f64,
}

/// One seed: both averaged masters on CAL-FIT, exported at every tau and
/// scored on CAL-SEL at every lambda.
fn seed_heads(
    dir: &Path,
    seed: u64,
    fit: &[Span],
    sel: &[Span],
    sel_records: u64,
) -> Result<(ChosenD, ChosenC)> {
    let mut pair = Pair::open(dir, "trace-cal")?;
    let docs: Vec<(u32, Span)> = fit.iter().map(|s| (s.doc, *s)).collect();
    let (master_d, snapshots) =
        train_averaged(seed, &docs, |span, train| pair.each(span, |r| train(r)))?
            .ok_or("no averaged snapshot")?;
    let (master_c, _) = hyb_h1c_stable::train_averaged(seed, &docs, |span, train| {
        pair.each(span, |r| train(&r.record))
    })?
    .ok_or("no averaged snapshot")?;
    let heads_d: Vec<BankHead> = TAUS
        .iter()
        .map(|&t| BankHead::export(&master_d, t))
        .collect();
    let heads_c: Vec<ScaledHead> = TAUS
        .iter()
        .map(|&t| ScaledHead::export(&master_c, t))
        .collect();
    let fast_c: Vec<FastHead> = heads_c.iter().map(FastHead::new).collect();
    let (mut bits_d, mut bits_c): (SelBits, SelBits) = Default::default();
    for &span in sel {
        pair.each(span, |r| {
            for t in 0..TAUS.len() {
                for (k, c) in heads_d[t].costs(r).iter().enumerate() {
                    bits_d[t][k] += c;
                }
                for (k, c) in fast_c[t].costs(&r.record).iter().enumerate() {
                    bits_c[t][k] += c;
                }
            }
        })?;
    }
    let (td, kd, bd) = pick(&bits_d);
    let (tc, kc, bc) = pick(&bits_c);
    Ok((
        ChosenD {
            tau: TAUS[td],
            eighths: LAMBDA_EIGHTHS[kd],
            head: heads_d.into_iter().nth(td).expect("tau index"),
            sel_bpc: bpc(bd, sel_records),
            snapshots,
        },
        ChosenC {
            eighths: LAMBDA_EIGHTHS[kc],
            fast: fast_c.into_iter().nth(tc).expect("tau index"),
            sel_bpc: bpc(bc, sel_records),
        },
    ))
}

fn head_tsv(c: &ChosenD) -> String {
    let join = |v: &[i32]| v.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
    let weights: Vec<String> = c.head.weights().iter().map(i8::to_string).collect();
    format!(
        "row\t{ROW}\ndepths\t{DEPTHS}\nbanks\t{MAX_BANKS}\nlambda_eighths\t{}\nunit\t{}\nbias\t{}\nweights\t{}\n",
        c.eighths,
        join(&c.head.unit()),
        join(&c.head.bias()),
        weights.join(",")
    )
}

fn parse_head(text: &str) -> Result<(BankHead, i32)> {
    let field = |name: &str| -> Result<&str> {
        text.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix('\t'))
            .ok_or_else(|| format!("missing {name}").into())
    };
    if field("row")?.parse::<usize>()? != ROW
        || field("depths")?.parse::<usize>()? != DEPTHS
        || field("banks")?.parse::<usize>()? != MAX_BANKS
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
    if weights.len() != DEPTHS * ROW || weights.iter().any(|w| !(-1..=1).contains(w)) {
        return Err("weights must be DEPTHS*ROW ternary values".into());
    }
    let eighths: i32 = field("lambda_eighths")?.parse()?;
    if !LAMBDA_EIGHTHS.contains(&eighths) {
        return Err("lambda outside the registered grid".into());
    }
    Ok((
        BankHead::new(weights, ints("unit")?, ints("bias")?),
        eighths,
    ))
}

/// SHA-256 of every i32 correction on fixed synthetic records; stops if the
/// kernel and the oracle disagree.
fn replay_digest(head: &BankHead, eighths: i32) -> Result<String> {
    let mut bytes = Vec::with_capacity(REPLAY_RECORDS * DEPTHS * 4);
    for r in replay_bank_records(REPLAY_RECORDS) {
        for d in 0..DEPTHS {
            let raw = head.raw(&r, d);
            if raw != head.oracle_raw(&r, d) {
                return Err("replay: kernel/oracle mismatch".into());
            }
            bytes.extend_from_slice(&head.correction(raw, d, eighths).to_le_bytes());
        }
    }
    Ok(hex(&sha256(&bytes)))
}

fn evaluate(dir: &Path, out: &Path, label: &str, open_eval3: bool) -> Result<String> {
    let json = fs::read_to_string(dir.join("trace.json"))?;
    let weights = json_str(&json, "weights_sha256")?;
    if label == "PREREGISTERED" && weights != P1_WEIGHTS_SHA256 {
        return Err(format!("P1 weights {weights} are not the H-1b/H-1c bytes_q4 weights").into());
    }
    let cal = scan(
        &dir.join("trace-cal.bin"),
        &dir.join("trace-cal.banks.bin"),
        2,
    )?;
    let a0_cal = bpc(cal.bits, cal.records);
    check_reported(&json, "cal_trace", a0_cal)?;
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
    let mut a0_sel = 0.0;
    {
        let mut pair = Pair::open(dir, "trace-cal")?;
        for &span in &sel {
            pair.each(span, |r| a0_sel += a0_cost(&r.record))?;
        }
    }
    let a0_sel = bpc(a0_sel, sel_records);
    let seeds: Vec<Result<(ChosenD, ChosenC)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = SEEDS
            .iter()
            .map(|&seed| {
                let (fit, sel) = (&fit, &sel);
                scope.spawn(move || seed_heads(dir, seed, fit, sel, sel_records))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("seed thread"))
            .collect()
    });
    let mut chosen = Vec::new();
    for seed in seeds {
        chosen.push(seed?);
    }
    let mut report = format!(
        "{BASE}_p1_weights_sha256\t{weights}\n\
         {BASE}_cal_fit_records\t{fit_records}\n{BASE}_cal_sel_records\t{sel_records}\n\
         {BASE}_dev_read\tfalse\n{BASE}_a0_cal_bpc\t{a0_cal:.9}\n{BASE}_a0_sel_bpc\t{a0_sel:.9}\n\
         {BASE}_inputs\t{INPUTS}\n{BASE}_head_bytes\t{}\n{BASE}_epochs\t{EPOCHS}\n\
         {BASE}_burn_in_epochs\t{BURN_IN_EPOCHS}\n",
        BankHead::resident_bytes(),
    );
    for (k, (d, c)) in chosen.iter().enumerate() {
        let seed = SEEDS[k];
        report += &format!(
            "{BASE}_a1d_seed{seed}_sel_bpc\t{:.9}\n{BASE}_a1d_seed{seed}_sel_gain\t{:.9}\n\
             {BASE}_a1d_seed{seed}_tau\t{}\n{BASE}_a1d_seed{seed}_lambda_eighths\t{}\n\
             {BASE}_a1d_seed{seed}_unit\t{:?}\n{BASE}_a1d_seed{seed}_zero_fraction\t{:.6}\n\
             {BASE}_a1d_seed{seed}_snapshots\t{}\n{BASE}_a1d_seed{seed}_replay_sha256\t{}\n\
             {BASE}_a1c_seed{seed}_sel_bpc\t{:.9}\n",
            d.sel_bpc,
            a0_sel - d.sel_bpc,
            d.tau,
            d.eighths,
            d.head.unit(),
            d.head.zero_fraction(),
            d.snapshots,
            replay_digest(&d.head, d.eighths)?,
            c.sel_bpc,
        );
        fs::write(out.join(format!("head-a1d-seed{seed}.tsv")), head_tsv(d))?;
    }
    let gate = chosen
        .iter()
        .all(|(d, _)| a0_sel - d.sel_bpc >= CAL_GATE_BPC);
    report += &format!("{BASE}_cal_gate_bpc\t{CAL_GATE_BPC}\n{BASE}_cal_gate\t{gate}\n");
    if !gate {
        report += &format!("{BASE}_eval3_read\tfalse\nverdict\tNO_GO_CAL_GATE\n");
        return Ok(report);
    }
    if !open_eval3 {
        report += &format!("{BASE}_eval3_read\tfalse\nverdict\tCAL_GATE_PASS\n");
        return Ok(report);
    }

    // Stage 2: EVAL3, read once.
    let eval3 = scan(
        &dir.join("trace-eval3.bin"),
        &dir.join("trace-eval3.banks.bin"),
        4,
    )?;
    let eval_docs: std::collections::BTreeSet<u32> = eval3.spans.iter().map(|s| s.doc).collect();
    if cal.spans.iter().any(|s| eval_docs.contains(&s.doc)) {
        return Err("a doc_index appears in both CAL and EVAL3".into());
    }
    let eval_json = fs::read_to_string(dir.join("eval3.json"))?;
    if json_str(&eval_json, "weights_sha256")? != weights {
        return Err("eval3.json and trace.json name different P1 weights".into());
    }
    let a0_eval = bpc(eval3.bits, eval3.records);
    check_reported(&eval_json, "eval3_trace", a0_eval)?;
    let p1_us = reported(&eval_json, "eval3_us_per_position", "eval3_us_per_position")?;
    if p1_us.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return Err("eval3.json eval3_us_per_position must be positive".into());
    }
    let mut d_bits = [0.0; SEEDS.len()];
    let mut c_bits = [0.0; SEEDS.len()];
    let mut mismatches = 0u64;
    let mut nanos = [0u128; 2];
    let mut pair = Pair::open(dir, "trace-eval3")?;
    for &span in &eval3.spans {
        pair.each(span, |r| {
            let t = Instant::now();
            std::hint::black_box(a0_cost(&r.record));
            nanos[0] += t.elapsed().as_nanos();
            for (k, (d, c)) in chosen.iter().enumerate() {
                let t = Instant::now();
                d_bits[k] += d.head.cost(r, d.eighths);
                if k == 0 {
                    nanos[1] += t.elapsed().as_nanos();
                }
                c_bits[k] += c.fast.cost(&r.record, c.eighths);
                for depth in 0..DEPTHS {
                    if d.head.raw(r, depth) != d.head.oracle_raw(r, depth) {
                        mismatches += 1;
                    }
                }
            }
        })?;
    }
    if mismatches != 0 {
        return Err(format!("{mismatches} kernel/oracle mismatches").into());
    }
    let a1d: Vec<f64> = d_bits.iter().map(|&b| bpc(b, eval3.records)).collect();
    let a1c: Vec<f64> = c_bits.iter().map(|&b| bpc(b, eval3.records)).collect();
    let spread = (a1d[0] - a1d[1]).abs();
    let go = a1d
        .iter()
        .all(|&b| b < a0_eval - GO_MARGIN_BPC && a0_eval - b > 2.0 * spread);
    let per_pos = |n: u128| n as f64 / 1000.0 / eval3.records as f64;
    let head_us = (per_pos(nanos[1]) - per_pos(nanos[0])).max(0.0);
    let cost_ratio = (p1_us + head_us) / p1_us;
    let verdict = match (go, cost_ratio <= MAX_COST_RATIO) {
        (true, true) => "GO",
        (true, false) => "GO_DIAGNOSTIC_COST",
        (false, _) => "NO_GO",
    };
    report += &format!(
        "{BASE}_eval3_read\ttrue\n{BASE}_eval3_records\t{}\n{BASE}_eval3_docs\t{}\n\
         {BASE}_a0_eval3_bpc\t{a0_eval:.9}\n{BASE}_kernel_oracle_mismatches\t0\n\
         {BASE}_us_per_position_p1\t{p1_us:.4}\n{BASE}_us_per_position_head\t{head_us:.4}\n\
         {BASE}_cost_ratio\t{cost_ratio:.4}\n",
        eval3.records,
        eval_docs.len(),
    );
    for (k, seed) in SEEDS.iter().enumerate() {
        report += &format!(
            "{BASE}_a1d_seed{seed}_eval3_bpc\t{:.9}\n{BASE}_a1c_seed{seed}_eval3_bpc\t{:.9}\n",
            a1d[k], a1c[k]
        );
    }
    report += &format!("{BASE}_seed_spread_bpc\t{spread:.9}\nverdict\t{verdict}\n");
    Ok(report)
}

/// HYB H-1d-cost: re-times the A1d heads of `h1d` with `FusedBankHead`
/// on EVAL3, after checking that both kernels give the same corrections
/// on every record and the published EVAL3 BPC.
fn cost(p1: &Path, h1d: &Path, out: &Path, label: &str) -> Result<()> {
    let report = fs::read_to_string(h1d.join("h1d.tsv"))?;
    let field = |key: &str| -> Result<&str> {
        report
            .lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('\t'))
            .ok_or_else(|| format!("h1d.tsv has no {key}").into())
    };
    if field("label")? != label
        || field("stage")? != "run"
        || field(&format!("{BASE}_eval3_read"))? != "true"
    {
        return Err("h1d.tsv is not a finished H-1d run with this label".into());
    }
    let mut heads = Vec::new();
    for (k, seed) in SEEDS.iter().enumerate() {
        let path = h1d.join(format!("head-a1d-seed{seed}.tsv"));
        let text = fs::read_to_string(&path)?;
        let sha = hex(&sha256(text.as_bytes()));
        if label == "PREREGISTERED" && sha != H1D_HEAD_SHA256[k] {
            return Err(format!("{} is not the H-1d head ({sha})", path.display()).into());
        }
        let (head, eighths) = parse_head(&text)?;
        let fused = FusedBankHead::new(&head);
        heads.push((head, fused, eighths, sha));
    }
    let dir = p1.join(BASE);
    let eval_json = fs::read_to_string(dir.join("eval3.json"))?;
    let p1_us = reported(&eval_json, "eval3_us_per_position", "eval3_us_per_position")?;
    if p1_us.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return Err("eval3.json eval3_us_per_position must be positive".into());
    }
    // The denominator is frozen: it must be the P1 time of this H-1d run.
    let p1_key = format!("{BASE}_us_per_position_p1");
    if format!("{p1_us:.4}") != field(&p1_key)? {
        return Err(format!("eval3.json P1 time {p1_us:.4} is not {p1_key} of h1d.tsv").into());
    }
    let eval3 = scan(
        &dir.join("trace-eval3.bin"),
        &dir.join("trace-eval3.banks.bin"),
        4,
    )?;
    let a0_eval = bpc(eval3.bits, eval3.records);
    check_reported(&eval_json, "eval3_trace", a0_eval)?;
    fs::create_dir(out)?;
    let mut bits = [0.0; SEEDS.len()];
    let mut mismatches = 0u64;
    // a0, fused head, bank head (control).
    let mut nanos = [0u128; 3];
    let mut pair = Pair::open(&dir, "trace-eval3")?;
    let mut index = 0u64;
    for &span in &eval3.spans {
        pair.each(span, |r| {
            let t = Instant::now();
            std::hint::black_box(a0_cost(&r.record));
            nanos[0] += t.elapsed().as_nanos();
            // Alternate which kernel runs first.
            let order = if index % 2 == 0 { [1, 2] } else { [2, 1] };
            index += 1;
            let (head, fused, eighths, _) = &heads[0];
            for which in order {
                let t = Instant::now();
                if which == 1 {
                    bits[0] += std::hint::black_box(fused.cost(r, *eighths));
                } else {
                    std::hint::black_box(head.cost(r, *eighths));
                }
                nanos[which] += t.elapsed().as_nanos();
            }
            let (_, fused, eighths, _) = &heads[1];
            bits[1] += fused.cost(r, *eighths);
            for (head, fused, eighths, _) in &heads {
                let got = fused.corrections(r, *eighths);
                for (d, &g) in got.iter().enumerate() {
                    if g != head.correction(head.raw(r, d), d, *eighths) {
                        mismatches += 1;
                    }
                }
            }
        })?;
    }
    if mismatches != 0 {
        return Err(format!("{mismatches} fused/bank head mismatches").into());
    }
    let mut lines = format!(
        "label\t{label}\n{BASE}_eval3_records\t{}\n{BASE}_a0_eval3_bpc\t{a0_eval:.9}\n\
         {BASE}_fused_mismatches\t0\n",
        eval3.records
    );
    for (k, seed) in SEEDS.iter().enumerate() {
        let got = format!("{:.9}", bpc(bits[k], eval3.records));
        let key = format!("{BASE}_a1d_seed{seed}_eval3_bpc");
        if got != field(&key)? {
            return Err(format!("{key} {got} differs from H-1d {}", field(&key)?).into());
        }
        lines += &format!(
            "{key}\t{got}\n{BASE}_a1d_seed{seed}_head_sha256\t{}\n",
            heads[k].3
        );
    }
    let per_pos = |n: u128| n as f64 / 1000.0 / eval3.records as f64;
    let fused_us = (per_pos(nanos[1]) - per_pos(nanos[0])).max(0.0);
    let bank_us = (per_pos(nanos[2]) - per_pos(nanos[0])).max(0.0);
    let ratio = (p1_us + fused_us) / p1_us;
    let pass = ratio <= MAX_COST_RATIO;
    // Only the cost criterion is re-measured: a quality verdict stays as it was.
    let h1d_verdict = match field("verdict")? {
        "GO" | "GO_DIAGNOSTIC_COST" if pass => "GO",
        "GO" | "GO_DIAGNOSTIC_COST" => "GO_DIAGNOSTIC_COST",
        other => other,
    };
    lines += &format!(
        "{BASE}_us_per_position_p1\t{p1_us:.4}\n{BASE}_us_per_position_head_fused\t{fused_us:.4}\n\
         {BASE}_us_per_position_head_bank_control\t{bank_us:.4}\n{BASE}_cost_ratio\t{ratio:.4}\n\
         cost_verdict\t{}\nh1d_verdict\t{}\n",
        if pass { "COST_PASS" } else { "COST_FAIL" },
        h1d_verdict,
    );
    fs::write(out.join("cost.tsv"), &lines)?;
    print!("{lines}");
    Ok(())
}

fn run(p1: &Path, out: &Path, label: &str, open_eval3: bool) -> Result<()> {
    fs::create_dir(out)?;
    let report = format!(
        "label\t{label}\nstage\t{}\n{}",
        if open_eval3 { "run" } else { "gate" },
        evaluate(&p1.join(BASE), out, label, open_eval3)?
    );
    fs::write(out.join("h1d.tsv"), &report)?;
    print!("{report}");
    Ok(())
}

/// Writes CAL and EVAL3 traces and sidecars where the target MSB follows
/// bank 2's stretch while the mixed logit is 0, and their `trace.json`.
/// `signal` scales how often the bank is right.
fn write_smoke(root: &Path, signal: u64) -> Result<()> {
    let dir = root.join(BASE);
    fs::create_dir_all(&dir)?;
    let mut summary = [(0.0, 0u64); 2];
    for (slot, name, split, docs) in [
        (
            0,
            "cal",
            2u8,
            (0..40u32).map(|k| 7 + 16 * k).collect::<Vec<_>>(),
        ),
        (1, "eval3", 4, (5000..5020).collect()),
    ] {
        let mut trace =
            std::io::BufWriter::new(File::create(dir.join(format!("trace-{name}.bin")))?);
        let mut banks =
            std::io::BufWriter::new(File::create(dir.join(format!("trace-{name}.banks.bin")))?);
        for doc in docs {
            for position in 0..200u32 {
                let s = splitmix(u64::from(doc) << 32 | u64::from(position));
                let q = ((s >> 8) % 120) as i8 - 60;
                let right = (s >> 50) % 16 < signal;
                let top = u8::from((q > 0) == right);
                let record = TraceRecord {
                    doc_index: doc,
                    position,
                    split,
                    target: (top << 7) | splitmix(s) as u8 & 0x7f,
                    word: (s >> 40) as u16,
                    context: s >> 17,
                    logits: [0; 8],
                    confidence: 0,
                };
                let mut plane = BankPlane {
                    doc_index: doc,
                    position,
                    ..BankPlane::default()
                };
                plane.stretch[0][2] = q;
                summary[slot].0 += record.cost_bits();
                summary[slot].1 += 1;
                trace.write_all(&record.encode())?;
                banks.write_all(&plane.encode())?;
            }
        }
        trace.flush()?;
        banks.flush()?;
    }
    write_trace_json(
        &dir,
        summary[0].0 / summary[0].1 as f64,
        summary[1].0 / summary[1].1 as f64,
    )
}

fn write_trace_json(dir: &Path, cal_bpc: f64, eval3_bpc: f64) -> Result<()> {
    fs::write(
        dir.join("trace.json"),
        format!(
            "{{\n  \"cal_trace\": {{\"file\": \"trace-cal.bin\", \"bpc\": {cal_bpc:.9}}},\n  \"weights_sha256\": \"smoke\"\n}}\n"
        ),
    )?;
    fs::write(
        dir.join("eval3.json"),
        format!(
            "{{\n  \"eval3_trace\": {{\"file\": \"trace-eval3.bin\", \"bpc\": {eval3_bpc:.9}}},\n  \"eval3_us_per_position\": 1.000000,\n  \"weights_sha256\": \"smoke\"\n}}\n"
        ),
    )?;
    Ok(())
}

fn cli(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str) {
        Some(mode @ ("gate" | "run")) if args.len() == 4 => run(
            Path::new(&args[2]),
            Path::new(&args[3]),
            "PREREGISTERED",
            mode == "run",
        ),
        Some("cost") if args.len() == 5 => cost(
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
            "PREREGISTERED",
        ),
        Some("replay") if args.len() == 3 => {
            let (head, eighths) = parse_head(&fs::read_to_string(&args[2])?)?;
            println!("replay_sha256\t{}", replay_digest(&head, eighths)?);
            Ok(())
        }
        Some("smoke") if args.len() == 3 => {
            let root = Path::new(&args[2]);
            write_smoke(&root.join("p1"), 14)?;
            run(&root.join("p1"), &root.join("out"), "SMOKE", true)
        }
        _ => Err("usage: (gate|run) P1_H1D_OUT NEW_OUT | cost P1_H1D_OUT H1D_OUT NEW_OUT | replay HEAD.tsv | smoke NEW_DIR".into()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(error) = cli(&args) {
        eprintln!("sml-hyb-h1d-s5h: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("hyb-h1d-{tag}-{}", std::process::id()))
    }

    fn value(report: &str, key: &str) -> String {
        report
            .lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('\t'))
            .unwrap_or_else(|| panic!("no {key}"))
            .to_owned()
    }

    #[test]
    fn smoke_pipeline_passes_the_gate_learns_and_replays() {
        let root = temp("smoke");
        let _ = fs::remove_dir_all(&root);
        cli(&["bin".into(), "smoke".into(), root.display().to_string()]).unwrap();
        let report = fs::read_to_string(root.join("out/h1d.tsv")).unwrap();
        assert_eq!(value(&report, "bytes_q4_cal_gate"), "true");
        assert_eq!(value(&report, "bytes_q4_eval3_read"), "true");
        assert_eq!(value(&report, "bytes_q4_dev_read"), "false");
        let a0: f64 = value(&report, "bytes_q4_a0_eval3_bpc").parse().unwrap();
        assert!((a0 - 8.0).abs() < 1e-9);
        for seed in SEEDS {
            let d: f64 = value(&report, &format!("bytes_q4_a1d_seed{seed}_eval3_bpc"))
                .parse()
                .unwrap();
            let c: f64 = value(&report, &format!("bytes_q4_a1c_seed{seed}_eval3_bpc"))
                .parse()
                .unwrap();
            assert!(d < a0 - 0.2 && d < c - 0.2, "seed {seed}: {d} vs {c}");
            let path = root.join(format!("out/head-a1d-seed{seed}.tsv"));
            let (head, eighths) = parse_head(&fs::read_to_string(path).unwrap()).unwrap();
            assert_eq!(
                value(&report, &format!("bytes_q4_a1d_seed{seed}_replay_sha256")),
                replay_digest(&head, eighths).unwrap()
            );
        }
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cost_retimes_the_same_heads_and_checks_identity() {
        let root = temp("cost");
        let _ = fs::remove_dir_all(&root);
        cli(&["bin".into(), "smoke".into(), root.display().to_string()]).unwrap();
        let (p1, h1d) = (root.join("p1"), root.join("out"));
        cost(&p1, &h1d, &root.join("cost"), "SMOKE").unwrap();
        let report = fs::read_to_string(root.join("cost/cost.tsv")).unwrap();
        let full = fs::read_to_string(h1d.join("h1d.tsv")).unwrap();
        for seed in SEEDS {
            let key = format!("bytes_q4_a1d_seed{seed}_eval3_bpc");
            assert_eq!(value(&report, &key), value(&full, &key));
        }
        assert_eq!(value(&report, "bytes_q4_fused_mismatches"), "0");
        assert!(["COST_PASS", "COST_FAIL"].contains(&value(&report, "cost_verdict").as_str()));
        // The preregistered label needs the published H-1d heads.
        assert!(cost(&p1, &h1d, &root.join("cost2"), "PREREGISTERED").is_err());
        // A quality verdict other than GO is never upgraded by the cost.
        let original = value(&full, "verdict");
        if !["GO", "GO_DIAGNOSTIC_COST"].contains(&original.as_str()) {
            assert_eq!(value(&report, "h1d_verdict"), original);
        }
        // The P1 time must be the one published by the H-1d run.
        let key = "bytes_q4_us_per_position_p1";
        let p1_us = value(&full, key);
        fs::write(
            h1d.join("h1d.tsv"),
            full.replace(&format!("{key}\t{p1_us}"), &format!("{key}\t9{p1_us}")),
        )
        .unwrap();
        assert!(cost(&p1, &h1d, &root.join("cost4"), "SMOKE").is_err());
        fs::write(h1d.join("h1d.tsv"), &full).unwrap();
        // A head changed after the run no longer matches the H-1d BPC.
        let path = h1d.join("head-a1d-seed11.tsv");
        let text = fs::read_to_string(&path).unwrap();
        let (head, _) = parse_head(&text).unwrap();
        let join = |v: Vec<String>| v.join(",");
        let old = join(head.unit().iter().map(i32::to_string).collect());
        let new = join(head.unit().iter().map(|u| (u + 7).to_string()).collect());
        fs::write(
            &path,
            text.replace(&format!("unit\t{old}"), &format!("unit\t{new}")),
        )
        .unwrap();
        assert!(cost(&p1, &h1d, &root.join("cost3"), "SMOKE").is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_weak_bank_signal_stops_at_the_gate_without_opening_eval3() {
        let root = temp("gate");
        let _ = fs::remove_dir_all(&root);
        // The bank is right half the time: nothing to learn.
        write_smoke(&root.join("p1"), 8).unwrap();
        // Make EVAL3 unreadable: the gate must stop before it.
        fs::write(root.join("p1/bytes_q4/trace-eval3.bin"), b"torn").unwrap();
        run(&root.join("p1"), &root.join("out"), "SMOKE", true).unwrap();
        let report = fs::read_to_string(root.join("out/h1d.tsv")).unwrap();
        assert_eq!(value(&report, "verdict"), "NO_GO_CAL_GATE");
        assert_eq!(value(&report, "bytes_q4_eval3_read"), "false");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_gate_stage_needs_no_eval3_and_matches_the_full_run() {
        let root = temp("stage");
        let _ = fs::remove_dir_all(&root);
        write_smoke(&root.join("p1"), 14).unwrap();
        let dir = root.join("p1").join(BASE);
        let eval3: Vec<_> = ["trace-eval3.bin", "trace-eval3.banks.bin", "eval3.json"]
            .iter()
            .map(|n| (dir.join(n), fs::read(dir.join(n)).unwrap()))
            .collect();
        for (path, _) in &eval3 {
            fs::remove_file(path).unwrap();
        }
        run(&root.join("p1"), &root.join("gate"), "SMOKE", false).unwrap();
        let gate = fs::read_to_string(root.join("gate/h1d.tsv")).unwrap();
        assert_eq!(value(&gate, "verdict"), "CAL_GATE_PASS");
        assert_eq!(value(&gate, "bytes_q4_eval3_read"), "false");
        // Without EVAL3, the full run cannot judge.
        assert!(run(&root.join("p1"), &root.join("early"), "SMOKE", true).is_err());
        for (path, bytes) in &eval3 {
            fs::write(path, bytes).unwrap();
        }
        run(&root.join("p1"), &root.join("out"), "SMOKE", true).unwrap();
        let full = fs::read_to_string(root.join("out/h1d.tsv")).unwrap();
        for seed in SEEDS {
            let key = format!("bytes_q4_a1d_seed{seed}_replay_sha256");
            assert_eq!(value(&gate, &key), value(&full, &key));
            let head = format!("head-a1d-seed{seed}.tsv");
            assert_eq!(
                fs::read(root.join("gate").join(&head)).unwrap(),
                fs::read(root.join("out").join(&head)).unwrap()
            );
        }
        // A preregistered run refuses P1 weights other than H-1b/H-1c's.
        assert!(run(
            &root.join("p1"),
            &root.join("prereg"),
            "PREREGISTERED",
            false
        )
        .is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn invalid_inputs_are_refused() {
        let root = temp("invalid");
        let _ = fs::remove_dir_all(&root);
        write_smoke(&root, 14).unwrap();
        let dir = root.join(BASE);
        // trace.json reporting another CAL BPC.
        write_trace_json(&dir, 7.5, 8.0).unwrap();
        assert!(run(&root, &root.join("out1"), "SMOKE", true).is_err());
        // A truncated sidecar.
        write_smoke(&root, 14).unwrap();
        let banks = fs::read(dir.join("trace-cal.banks.bin")).unwrap();
        fs::write(
            dir.join("trace-cal.banks.bin"),
            &banks[..banks.len() - BANK_PLANE_BYTES],
        )
        .unwrap();
        assert!(run(&root, &root.join("out2"), "SMOKE", true).is_err());
        // A misaligned sidecar of the right length.
        write_smoke(&root, 14).unwrap();
        let mut banks = fs::read(dir.join("trace-cal.banks.bin")).unwrap();
        banks[4] ^= 1;
        fs::write(dir.join("trace-cal.banks.bin"), &banks).unwrap();
        assert!(run(&root, &root.join("out3"), "SMOKE", true).is_err());
        assert!(cli(&["bin".into(), "run".into(), "a".into()]).is_err());
        let bad = format!(
            "row\t{ROW}\ndepths\t{DEPTHS}\nbanks\t{MAX_BANKS}\nlambda_eighths\t3\nunit\t1,1,1,1,1,1,1,1\nbias\t0,0,0,0,0,0,0,0\nweights\t{}\n",
            vec!["0"; DEPTHS * ROW].join(",")
        );
        assert!(parse_head(&bad).is_err());
        assert!(parse_head(&bad.replace("lambda_eighths\t3", "lambda_eighths\t2")).is_ok());
        fs::remove_dir_all(&root).unwrap();
    }
}
