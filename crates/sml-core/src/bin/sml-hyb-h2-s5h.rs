//! HYB H-2: MOR routing over H-1d bank-head experts, judged on EVAL4.
//! Protocol: docs/HYB-H2-S5H-MOR-EXPERTS-PREREGISTRATION.md.
//!
//! Reads the `bytes_q4/` output of `sml-gb11-p1-s5h run-h1d` (the H-1d P1
//! run: `trace-cal.bin`, its `.banks.bin` sidecar, `trace.json`) and the
//! finished H-1d run (`h1d.tsv`, `head-a1d-seed{11,23}.tsv`), whose heads
//! are the A1d control, unchanged. Per seed, on CAL-FIT
//! (`doc_index % 32 == 7`):
//!
//! * A2, `EXPERTS` bank heads behind the learned Boolean router
//!   (`hyb_h2_mor_experts`, hard EM over `ROUNDS + 1` trainings);
//! * A2r, the same experts behind the fixed hash router (control).
//!
//! (tau, lambda), shared by the experts of an arm, are picked on CAL-SEL
//! (`doc_index % 32 == 23`).
//!
//! * `gate P1_H1D_OUT H1D_OUT NEW_OUTPUT_DIR` is stage 1: `CAL_GATE_PASS`
//!   when the CAL-SEL gain of A2 over A1d reaches `CAL_GATE_BPC` for both
//!   seeds, otherwise `NO_GO_CAL_GATE`. EVAL4 does not exist yet: P1
//!   traces it (`trace-h2-eval4`) only after a pass.
//! * `run P1_H1D_OUT H1D_OUT NEW_OUTPUT_DIR` redoes stage 1
//!   (deterministic) and, if the gate passes, reads `trace-eval4.bin`, its
//!   sidecar and `eval4.json` once.
//!
//! The S5h DEV, EVAL2 and EVAL3 are never opened. `replay EXPERTS.tsv`
//! prints the digest of a router and its experts on synthetic records
//! (cross-host check); `smoke` runs on generated traces and is labelled
//! SMOKE, never a result. TEST is never read. NOT_PROMOTED.

use sml_core::{
    elastic_ngram_paged_sha::{hex, sha256},
    gb11_p1_stream::{read_trace, TraceRecord, MAX_BANKS, TRACE_RECORD_BYTES},
    hyb_h1_residual::{a0_cost, splitmix, DEPTHS},
    hyb_h1b_calibrated::{LAMBDA_EIGHTHS, TAUS},
    hyb_h1c_stable::{BURN_IN_EPOCHS, EPOCHS},
    hyb_h1d_bank_head::{
        replay_bank_records, BankHead, BankMaster, BankPlane, BankRecord, FusedBankHead,
        BANK_PLANE_BYTES, ROW,
    },
    hyb_h2_mor_experts::{
        best_expert, hash_route, initial_group, quartile_edges, route_bits, router_bytes,
        train_routed, Majority, Router, EXPERTS, ROUNDS, ROUTE_WORDS,
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
/// Stage 1: smallest CAL-SEL gain of A2 over A1d that opens EVAL4.
const CAL_GATE_BPC: f64 = 0.01;
/// EVAL4: A2 must beat A1d by this much...
const GO_MARGIN_A1D_BPC: f64 = 0.01;
/// ... and the hash-routed control A2r by this much.
const GO_MARGIN_A2R_BPC: f64 = 0.005;
const MAX_COST_RATIO: f64 = 1.5;
const REPLAY_RECORDS: usize = 65_536;
const CAL_FIT: u32 = 7;
const CAL_SEL: u32 = 23;
/// `bytes_q4` weights of the H-1b, H-1c and H-1d runs.
const P1_WEIGHTS_SHA256: &str = "7f4c233d6c4d67e1d300068799f4653cfcaedda4158587478c99ec1d31c3c418";
/// `head-a1d-seed11.tsv` and `head-a1d-seed23.tsv` of the H-1d run
/// (RemoteOps run 40126579964): the A1d control.
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

/// The number after `"field": ` on the line of `key` in a JSON summary.
fn reported(json: &str, key: &str, field: &str) -> Result<f64> {
    let line = json
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("\"{key}\"")))
        .ok_or_else(|| format!("summary has no {key}"))?;
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
        return Err(format!("summary {key} has no {field}").into());
    }
    Ok(rest[..end].parse()?)
}

/// The string value of `"key": "..."`.
fn json_str<'a>(json: &'a str, key: &str) -> Result<&'a str> {
    let pattern = format!("\"{key}\": \"");
    let rest = &json[json
        .find(&pattern)
        .ok_or_else(|| format!("summary has no {key}"))?
        + pattern.len()..];
    Ok(&rest[..rest.find('"').ok_or("unterminated string")?])
}

fn check_reported(json: &str, key: &str, got: f64) -> Result<()> {
    let want = reported(json, key, "bpc")?;
    if (got - want).abs() > 1e-6 {
        return Err(format!("{key} P1 BPC {got:.9} != summary {want:.9}").into());
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

    fn all(&mut self, spans: &[Span], mut each: impl FnMut(&BankRecord)) -> Result<()> {
        for &span in spans {
            self.each(span, &mut each)?;
        }
        Ok(())
    }
}

/// How an arm sends a byte to one of its experts.
#[derive(Clone, Copy)]
enum Routing {
    Learned(Router),
    Hash,
}

impl Routing {
    #[inline]
    fn route(&self, r: &BankRecord) -> usize {
        match self {
            Self::Learned(router) => router.route(r),
            Self::Hash => hash_route(r),
        }
    }

    fn oracle_route(&self, r: &BankRecord) -> usize {
        match self {
            Self::Learned(router) => router.oracle_route(r),
            Self::Hash => hash_route(r),
        }
    }
}

/// An exported arm: routing, one head per expert, shared lambda.
struct Arm {
    routing: Routing,
    heads: Vec<BankHead>,
    tau: f32,
    eighths: i32,
    sel_bpc: f64,
    snapshots: u64,
}

impl Arm {
    fn fused(&self) -> Vec<FusedBankHead> {
        self.heads.iter().map(FusedBankHead::new).collect()
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

/// Exports `masters` at every tau and keeps the (tau, lambda) with the
/// smallest CAL-SEL code length under `routing`.
fn select(
    pair: &mut Pair,
    masters: &[BankMaster],
    routing: Routing,
    sel: &[Span],
    sel_records: u64,
    snapshots: u64,
) -> Result<Arm> {
    let heads: Vec<Vec<BankHead>> = TAUS
        .iter()
        .map(|&t| masters.iter().map(|m| BankHead::export(m, t)).collect())
        .collect();
    let mut bits: SelBits = Default::default();
    pair.all(sel, |r| {
        let k = routing.route(r);
        for (t, row) in heads.iter().enumerate() {
            for (l, c) in row[k].costs(r).iter().enumerate() {
                bits[t][l] += c;
            }
        }
    })?;
    let (t, l, b) = pick(&bits);
    Ok(Arm {
        routing,
        heads: heads.into_iter().nth(t).expect("tau index"),
        tau: TAUS[t],
        eighths: LAMBDA_EIGHTHS[l],
        sel_bpc: bpc(b, sel_records),
        snapshots,
    })
}

struct SeedArms {
    a2: Arm,
    a2r: Arm,
    /// Bytes per expert under the router of each training round.
    round_counts: Vec<[u64; EXPERTS]>,
}

/// One seed: hard EM of the router with routed training, then the hash
/// control, both exported and selected on CAL-SEL.
fn seed_arms(
    dir: &Path,
    seed: u64,
    fit: &[Span],
    sel: &[Span],
    sel_records: u64,
    initial: Router,
) -> Result<SeedArms> {
    let mut pair = Pair::open(dir, "trace-cal")?;
    let docs: Vec<(u32, Span)> = fit.iter().map(|s| (s.doc, *s)).collect();
    let mut router = initial;
    let mut round_counts = Vec::new();
    let mut last = None;
    for round in 0..=ROUNDS {
        let mut counts = [0u64; EXPERTS];
        pair.all(fit, |r| counts[router.route(r)] += 1)?;
        round_counts.push(counts);
        let (masters, snapshots) = train_routed(
            seed,
            &docs,
            |r| router.route(r),
            |span, train| pair.each(span, |r| train(r)),
        )?
        .ok_or("no averaged snapshot")?;
        if round < ROUNDS {
            let mut vote = Majority::default();
            pair.all(fit, |r| vote.add(best_expert(&masters, r), &route_bits(r)))?;
            router = vote.router(&router);
        }
        last = Some((masters, snapshots));
    }
    let (masters, snapshots) = last.expect("at least one round");
    let a2 = select(
        &mut pair,
        &masters,
        Routing::Learned(router),
        sel,
        sel_records,
        snapshots,
    )?;
    let (masters_r, snapshots_r) = train_routed(seed, &docs, hash_route, |span, train| {
        pair.each(span, |r| train(r))
    })?
    .ok_or("no averaged snapshot")?;
    let a2r = select(
        &mut pair,
        &masters_r,
        Routing::Hash,
        sel,
        sel_records,
        snapshots_r,
    )?;
    Ok(SeedArms {
        a2,
        a2r,
        round_counts,
    })
}

/// Router learned from the initial partition: quartiles of the depth-0
/// P1 |stretch| over CAL-FIT, prototypes their bitwise majorities.
fn initial_router(pair: &mut Pair, fit: &[Span]) -> Result<([u16; EXPERTS - 1], Router)> {
    let mut histogram = vec![0u64; 1 << 16];
    pair.all(fit, |r| {
        histogram[usize::from(r.record.logits[0].unsigned_abs())] += 1;
    })?;
    let edges = quartile_edges(&histogram);
    let mut vote = Majority::default();
    pair.all(fit, |r| vote.add(initial_group(r, &edges), &route_bits(r)))?;
    let empty = Router {
        prototypes: [[0; ROUTE_WORDS]; EXPERTS],
    };
    Ok((edges, vote.router(&empty)))
}

fn join_i32(v: &[i32]) -> String {
    v.iter().map(i32::to_string).collect::<Vec<_>>().join(",")
}

fn router_hex(router: &Router) -> String {
    router
        .prototypes
        .iter()
        .flat_map(|p| p.iter().map(|w| format!("{w:016x}")))
        .collect::<Vec<_>>()
        .join(",")
}

/// A2 export: router prototypes, then every expert head.
fn experts_tsv(arm: &Arm) -> Result<String> {
    let Routing::Learned(router) = arm.routing else {
        return Err("only a learned router is exported".into());
    };
    let mut text = format!(
        "row\t{ROW}\ndepths\t{DEPTHS}\nbanks\t{MAX_BANKS}\nexperts\t{EXPERTS}\nlambda_eighths\t{}\nrouter\t{}\n",
        arm.eighths,
        router_hex(&router),
    );
    for (k, head) in arm.heads.iter().enumerate() {
        let weights: Vec<String> = head.weights().iter().map(i8::to_string).collect();
        text += &format!(
            "expert{k}_unit\t{}\nexpert{k}_bias\t{}\nexpert{k}_weights\t{}\n",
            join_i32(&head.unit()),
            join_i32(&head.bias()),
            weights.join(",")
        );
    }
    Ok(text)
}

fn parse_experts(text: &str) -> Result<(Router, Vec<BankHead>, i32)> {
    let field = |name: &str| -> Result<&str> {
        text.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix('\t'))
            .ok_or_else(|| format!("missing {name}").into())
    };
    if field("row")?.parse::<usize>()? != ROW
        || field("depths")?.parse::<usize>()? != DEPTHS
        || field("banks")?.parse::<usize>()? != MAX_BANKS
        || field("experts")?.parse::<usize>()? != EXPERTS
    {
        return Err("experts shape mismatch".into());
    }
    let words: Vec<u64> = field("router")?
        .split(',')
        .map(|w| u64::from_str_radix(w, 16))
        .collect::<std::result::Result<_, _>>()?;
    if words.len() != EXPERTS * ROUTE_WORDS {
        return Err("router needs EXPERTS * ROUTE_WORDS words".into());
    }
    let mut prototypes = [[0u64; ROUTE_WORDS]; EXPERTS];
    for (k, p) in prototypes.iter_mut().enumerate() {
        p.copy_from_slice(&words[k * ROUTE_WORDS..(k + 1) * ROUTE_WORDS]);
    }
    let ints = |name: &str| -> Result<[i32; DEPTHS]> {
        let v: Vec<i32> = field(name)?
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        v.try_into()
            .map_err(|_| format!("{name} needs {DEPTHS} values").into())
    };
    let mut heads = Vec::with_capacity(EXPERTS);
    for k in 0..EXPERTS {
        let weights: Vec<i8> = field(&format!("expert{k}_weights"))?
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        if weights.len() != DEPTHS * ROW || weights.iter().any(|w| !(-1..=1).contains(w)) {
            return Err("weights must be DEPTHS*ROW ternary values".into());
        }
        heads.push(BankHead::new(
            weights,
            ints(&format!("expert{k}_unit"))?,
            ints(&format!("expert{k}_bias"))?,
        ));
    }
    let eighths: i32 = field("lambda_eighths")?.parse()?;
    if !LAMBDA_EIGHTHS.contains(&eighths) {
        return Err("lambda outside the registered grid".into());
    }
    Ok((Router { prototypes }, heads, eighths))
}

/// The H-1d head format (`sml-hyb-h1d-s5h`): one head and its lambda.
fn parse_h1d_head(text: &str) -> Result<(BankHead, i32)> {
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

/// SHA-256 of the route and every i32 correction on fixed synthetic
/// records; stops if a kernel and its oracle disagree.
fn replay_digest(router: &Router, heads: &[BankHead], eighths: i32) -> Result<String> {
    let mut bytes = router_bytes(router);
    bytes.reserve(REPLAY_RECORDS * (1 + DEPTHS * 4));
    for r in replay_bank_records(REPLAY_RECORDS) {
        let k = router.route(&r);
        if k != router.oracle_route(&r) {
            return Err("replay: router/oracle mismatch".into());
        }
        bytes.push(k as u8);
        for d in 0..DEPTHS {
            let raw = heads[k].raw(&r, d);
            if raw != heads[k].oracle_raw(&r, d) {
                return Err("replay: kernel/oracle mismatch".into());
            }
            bytes.extend_from_slice(&heads[k].correction(raw, d, eighths).to_le_bytes());
        }
    }
    Ok(hex(&sha256(&bytes)))
}

/// The A1d control: the two heads of a finished H-1d run.
fn h1d_heads(h1d: &Path, label: &str, weights: &str) -> Result<Vec<(BankHead, i32)>> {
    let report = fs::read_to_string(h1d.join("h1d.tsv"))?;
    let field = |key: &str| -> Result<&str> {
        report
            .lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('\t'))
            .ok_or_else(|| format!("h1d.tsv has no {key}").into())
    };
    if field("label")? != label || field("stage")? != "run" {
        return Err("h1d.tsv is not a finished H-1d run with this label".into());
    }
    if field(&format!("{BASE}_p1_weights_sha256"))? != weights {
        return Err("the H-1d run used other P1 weights".into());
    }
    let mut heads = Vec::new();
    for (k, seed) in SEEDS.iter().enumerate() {
        let path = h1d.join(format!("head-a1d-seed{seed}.tsv"));
        let text = fs::read_to_string(&path)?;
        let sha = hex(&sha256(text.as_bytes()));
        if label == "PREREGISTERED" && sha != H1D_HEAD_SHA256[k] {
            return Err(format!("{} is not the H-1d head ({sha})", path.display()).into());
        }
        heads.push(parse_h1d_head(&text)?);
    }
    Ok(heads)
}

fn evaluate(dir: &Path, h1d: &Path, out: &Path, label: &str, open_eval4: bool) -> Result<String> {
    let json = fs::read_to_string(dir.join("trace.json"))?;
    let weights = json_str(&json, "weights_sha256")?;
    if label == "PREREGISTERED" && weights != P1_WEIGHTS_SHA256 {
        return Err(format!("P1 weights {weights} are not the H-1d bytes_q4 weights").into());
    }
    let a1d_heads = h1d_heads(h1d, label, weights)?;
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
    let mut pair = Pair::open(dir, "trace-cal")?;
    let (mut a0_sel, mut a1d_sel) = (0.0, [0.0; SEEDS.len()]);
    pair.all(&sel, |r| {
        a0_sel += a0_cost(&r.record);
        for (k, (head, eighths)) in a1d_heads.iter().enumerate() {
            a1d_sel[k] += head.cost(r, *eighths);
        }
    })?;
    let a0_sel = bpc(a0_sel, sel_records);
    let a1d_sel = a1d_sel.map(|b| bpc(b, sel_records));
    let (edges, initial) = initial_router(&mut pair, &fit)?;
    let seeds: Vec<Result<SeedArms>> = std::thread::scope(|scope| {
        let handles: Vec<_> = SEEDS
            .iter()
            .map(|&seed| {
                let (fit, sel) = (&fit, &sel);
                scope.spawn(move || seed_arms(dir, seed, fit, sel, sel_records, initial))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("seed thread"))
            .collect()
    });
    let mut arms = Vec::new();
    for seed in seeds {
        arms.push(seed?);
    }
    let mut report = format!(
        "{BASE}_p1_weights_sha256\t{weights}\n\
         {BASE}_cal_fit_records\t{fit_records}\n{BASE}_cal_sel_records\t{sel_records}\n\
         {BASE}_dev_read\tfalse\n{BASE}_a0_cal_bpc\t{a0_cal:.9}\n{BASE}_a0_sel_bpc\t{a0_sel:.9}\n\
         {BASE}_experts\t{EXPERTS}\n{BASE}_route_bits\t{}\n{BASE}_em_rounds\t{ROUNDS}\n\
         {BASE}_epochs\t{EPOCHS}\n{BASE}_burn_in_epochs\t{BURN_IN_EPOCHS}\n\
         {BASE}_initial_quartile_edges\t{edges:?}\n{BASE}_initial_router\t{}\n",
        64 * ROUTE_WORDS,
        router_hex(&initial),
    );
    for (k, a) in arms.iter().enumerate() {
        let seed = SEEDS[k];
        let Routing::Learned(router) = a.a2.routing else {
            return Err("A2 must use a learned router".into());
        };
        let rounds: Vec<String> = a.round_counts.iter().map(|c| format!("{c:?}")).collect();
        report += &format!(
            "{BASE}_a1d_seed{seed}_sel_bpc\t{:.9}\n\
             {BASE}_a2_seed{seed}_sel_bpc\t{:.9}\n{BASE}_a2_seed{seed}_sel_gain_over_a1d\t{:.9}\n\
             {BASE}_a2_seed{seed}_tau\t{}\n{BASE}_a2_seed{seed}_lambda_eighths\t{}\n\
             {BASE}_a2_seed{seed}_snapshots\t{}\n{BASE}_a2_seed{seed}_round_counts\t{}\n\
             {BASE}_a2_seed{seed}_router\t{}\n{BASE}_a2_seed{seed}_zero_fraction\t{:.6}\n\
             {BASE}_a2_seed{seed}_replay_sha256\t{}\n\
             {BASE}_a2r_seed{seed}_sel_bpc\t{:.9}\n{BASE}_a2r_seed{seed}_tau\t{}\n\
             {BASE}_a2r_seed{seed}_lambda_eighths\t{}\n{BASE}_a2r_seed{seed}_snapshots\t{}\n",
            a1d_sel[k],
            a.a2.sel_bpc,
            a1d_sel[k] - a.a2.sel_bpc,
            a.a2.tau,
            a.a2.eighths,
            a.a2.snapshots,
            rounds.join(";"),
            router_hex(&router),
            a.a2.heads.iter().map(BankHead::zero_fraction).sum::<f64>() / EXPERTS as f64,
            replay_digest(&router, &a.a2.heads, a.a2.eighths)?,
            a.a2r.sel_bpc,
            a.a2r.tau,
            a.a2r.eighths,
            a.a2r.snapshots,
        );
        fs::write(
            out.join(format!("experts-a2-seed{seed}.tsv")),
            experts_tsv(&a.a2)?,
        )?;
    }
    let gate = arms
        .iter()
        .zip(a1d_sel)
        .all(|(a, d)| d - a.a2.sel_bpc >= CAL_GATE_BPC);
    report += &format!("{BASE}_cal_gate_bpc\t{CAL_GATE_BPC}\n{BASE}_cal_gate\t{gate}\n");
    if !gate {
        report += &format!("{BASE}_eval4_read\tfalse\nverdict\tNO_GO_CAL_GATE\n");
        return Ok(report);
    }
    if !open_eval4 {
        report += &format!("{BASE}_eval4_read\tfalse\nverdict\tCAL_GATE_PASS\n");
        return Ok(report);
    }

    // Stage 2: EVAL4, read once.
    let eval4 = scan(
        &dir.join("trace-eval4.bin"),
        &dir.join("trace-eval4.banks.bin"),
        5,
    )?;
    let eval_docs: std::collections::BTreeSet<u32> = eval4.spans.iter().map(|s| s.doc).collect();
    if cal.spans.iter().any(|s| eval_docs.contains(&s.doc)) {
        return Err("a doc_index appears in both CAL and EVAL4".into());
    }
    let eval_json = fs::read_to_string(dir.join("eval4.json"))?;
    if json_str(&eval_json, "weights_sha256")? != weights {
        return Err("eval4.json and trace.json name different P1 weights".into());
    }
    let a0_eval = bpc(eval4.bits, eval4.records);
    check_reported(&eval_json, "eval4_trace", a0_eval)?;
    let p1_us = reported(&eval_json, "eval4_us_per_position", "eval4_us_per_position")?;
    if p1_us.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return Err("eval4.json eval4_us_per_position must be positive".into());
    }
    let a1d_fused: Vec<(FusedBankHead, i32)> = a1d_heads
        .iter()
        .map(|(h, e)| (FusedBankHead::new(h), *e))
        .collect();
    let fused: Vec<(Vec<FusedBankHead>, Vec<FusedBankHead>)> =
        arms.iter().map(|a| (a.a2.fused(), a.a2r.fused())).collect();
    let mut bits = [[0.0f64; 3]; SEEDS.len()];
    let mut used = [[0u64; EXPERTS]; SEEDS.len()];
    let mut mismatches = 0u64;
    // a0, A2 seed 11 (router + fused expert).
    let mut nanos = [0u128; 2];
    let mut pair = Pair::open(dir, "trace-eval4")?;
    pair.all(&eval4.spans, |r| {
        let t = Instant::now();
        std::hint::black_box(a0_cost(&r.record));
        nanos[0] += t.elapsed().as_nanos();
        for (s, a) in arms.iter().enumerate() {
            let t = Instant::now();
            let k = a.a2.routing.route(r);
            bits[s][0] += fused[s].0[k].cost(r, a.a2.eighths);
            if s == 0 {
                nanos[1] += t.elapsed().as_nanos();
            }
            used[s][k] += 1;
            let j = a.a2r.routing.route(r);
            bits[s][1] += fused[s].1[j].cost(r, a.a2r.eighths);
            bits[s][2] += a1d_fused[s].0.cost(r, a1d_fused[s].1);
            if k != a.a2.routing.oracle_route(r) || j != a.a2r.routing.oracle_route(r) {
                mismatches += 1;
            }
            for depth in 0..DEPTHS {
                let head = &a.a2.heads[k];
                if head.raw(r, depth) != head.oracle_raw(r, depth) {
                    mismatches += 1;
                }
            }
        }
    })?;
    if mismatches != 0 {
        return Err(format!("{mismatches} kernel/oracle mismatches").into());
    }
    let per = |b: f64| bpc(b, eval4.records);
    let a2: Vec<f64> = bits.iter().map(|b| per(b[0])).collect();
    let a2r: Vec<f64> = bits.iter().map(|b| per(b[1])).collect();
    let a1d: Vec<f64> = bits.iter().map(|b| per(b[2])).collect();
    let spread = (a2[0] - a2[1]).abs();
    let go = (0..SEEDS.len()).all(|s| {
        a2[s] < a1d[s] - GO_MARGIN_A1D_BPC
            && a2[s] < a2r[s] - GO_MARGIN_A2R_BPC
            && a1d[s] - a2[s] > 2.0 * spread
    });
    let per_pos = |n: u128| n as f64 / 1000.0 / eval4.records as f64;
    let head_us = (per_pos(nanos[1]) - per_pos(nanos[0])).max(0.0);
    let cost_ratio = (p1_us + head_us) / p1_us;
    let verdict = match (go, cost_ratio <= MAX_COST_RATIO) {
        (true, true) => "GO",
        (true, false) => "GO_DIAGNOSTIC_COST",
        (false, _) => "NO_GO",
    };
    report += &format!(
        "{BASE}_eval4_read\ttrue\n{BASE}_eval4_records\t{}\n{BASE}_eval4_docs\t{}\n\
         {BASE}_a0_eval4_bpc\t{a0_eval:.9}\n{BASE}_kernel_oracle_mismatches\t0\n\
         {BASE}_us_per_position_p1\t{p1_us:.4}\n{BASE}_us_per_position_router_expert\t{head_us:.4}\n\
         {BASE}_cost_ratio\t{cost_ratio:.4}\n",
        eval4.records,
        eval_docs.len(),
    );
    for (s, seed) in SEEDS.iter().enumerate() {
        report += &format!(
            "{BASE}_a1d_seed{seed}_eval4_bpc\t{:.9}\n{BASE}_a2_seed{seed}_eval4_bpc\t{:.9}\n\
             {BASE}_a2r_seed{seed}_eval4_bpc\t{:.9}\n{BASE}_a2_seed{seed}_eval4_expert_bytes\t{:?}\n",
            a1d[s], a2[s], a2r[s], used[s]
        );
    }
    report += &format!("{BASE}_a2_seed_spread_bpc\t{spread:.9}\nverdict\t{verdict}\n");
    Ok(report)
}

fn run(p1: &Path, h1d: &Path, out: &Path, label: &str, open_eval4: bool) -> Result<()> {
    fs::create_dir(out)?;
    let report = format!(
        "label\t{label}\nstage\t{}\n{}",
        if open_eval4 { "run" } else { "gate" },
        evaluate(&p1.join(BASE), h1d, out, label, open_eval4)?
    );
    fs::write(out.join("h2.tsv"), &report)?;
    print!("{report}");
    Ok(())
}

/// Writes CAL and EVAL4 traces and sidecars with two regimes the router
/// can see: in regime A (context low word all ones, large depth-0 P1
/// stretch) the target MSB follows the sign of bank 2, in regime B
/// (context low word zero, small stretch) it follows the opposite sign.
/// One head cannot fit both; two routed experts can. `signal` (out of 16)
/// is how often bank 2 is right; 8 is no signal at all (and a zero P1
/// stretch). Also writes the
/// `trace.json`, `eval4.json` and a zero-head H-1d run.
fn write_smoke(root: &Path, signal: u64) -> Result<()> {
    let dir = root.join("p1").join(BASE);
    fs::create_dir_all(&dir)?;
    let mut summary = [(0.0, 0u64); 2];
    for (slot, name, split, docs) in [
        (
            0,
            "cal",
            2u8,
            (0..40u32).map(|k| 7 + 16 * k).collect::<Vec<_>>(),
        ),
        (1, "eval4", 5, (5000..5020).collect()),
    ] {
        let mut trace =
            std::io::BufWriter::new(File::create(dir.join(format!("trace-{name}.bin")))?);
        let mut banks =
            std::io::BufWriter::new(File::create(dir.join(format!("trace-{name}.banks.bin")))?);
        for doc in docs {
            for position in 0..200u32 {
                let s = splitmix(u64::from(doc) << 32 | u64::from(position));
                let regime_a = s & 1 == 1;
                let q = ((s >> 8) % 120) as i8 - 60;
                let right = (s >> 50) % 16 < signal;
                let top = u8::from(((q > 0) == right) == regime_a);
                // Without signal the depth-0 P1 stretch is 0 too: nothing to learn.
                let magnitude = if signal > 8 {
                    ((s >> 20) % 64) as i16 + if regime_a { 400 } else { 0 }
                } else {
                    0
                };
                let sign = if (s >> 30) & 1 == 1 { 1 } else { -1 };
                let record = TraceRecord {
                    doc_index: doc,
                    position,
                    split,
                    target: (top << 7) | splitmix(s) as u8 & 0x7f,
                    word: (s >> 40) as u16,
                    context: ((s >> 17) << 32) | if regime_a { 0xffff_ffff } else { 0 },
                    logits: [sign * magnitude, 0, 0, 0, 0, 0, 0, 0],
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
    fs::write(
        dir.join("trace.json"),
        format!(
            "{{\n  \"cal_trace\": {{\"file\": \"trace-cal.bin\", \"bpc\": {:.9}}},\n  \"weights_sha256\": \"smoke\"\n}}\n",
            summary[0].0 / summary[0].1 as f64
        ),
    )?;
    fs::write(
        dir.join("eval4.json"),
        format!(
            "{{\n  \"eval4_trace\": {{\"file\": \"trace-eval4.bin\", \"bpc\": {:.9}}},\n  \"eval4_us_per_position\": 1.000000,\n  \"weights_sha256\": \"smoke\"\n}}\n",
            summary[1].0 / summary[1].1 as f64
        ),
    )?;
    let h1d = root.join("h1d");
    fs::create_dir_all(&h1d)?;
    fs::write(
        h1d.join("h1d.tsv"),
        format!("label\tSMOKE\nstage\trun\n{BASE}_p1_weights_sha256\tsmoke\nverdict\tGO\n"),
    )?;
    let zeros = vec!["0"; DEPTHS * ROW].join(",");
    let ones = ["1"; DEPTHS].join(",");
    let nil = ["0"; DEPTHS].join(",");
    for seed in SEEDS {
        fs::write(
            h1d.join(format!("head-a1d-seed{seed}.tsv")),
            format!(
                "row\t{ROW}\ndepths\t{DEPTHS}\nbanks\t{MAX_BANKS}\nlambda_eighths\t8\nunit\t{ones}\nbias\t{nil}\nweights\t{zeros}\n"
            ),
        )?;
    }
    Ok(())
}

fn cli(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str) {
        Some(mode @ ("gate" | "run")) if args.len() == 5 => run(
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
            "PREREGISTERED",
            mode == "run",
        ),
        Some("replay") if args.len() == 3 => {
            let (router, heads, eighths) = parse_experts(&fs::read_to_string(&args[2])?)?;
            println!(
                "replay_sha256\t{}",
                replay_digest(&router, &heads, eighths)?
            );
            Ok(())
        }
        Some("smoke") if args.len() == 3 => {
            let root = Path::new(&args[2]);
            write_smoke(root, 14)?;
            run(
                &root.join("p1"),
                &root.join("h1d"),
                &root.join("out"),
                "SMOKE",
                true,
            )
        }
        _ => Err(
            "usage: (gate|run) P1_H1D_OUT H1D_OUT NEW_OUT | replay EXPERTS.tsv | smoke NEW_DIR"
                .into(),
        ),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(error) = cli(&args) {
        eprintln!("sml-hyb-h2-s5h: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("hyb-h2-{tag}-{}", std::process::id()))
    }

    fn value(report: &str, key: &str) -> String {
        report
            .lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('\t'))
            .unwrap_or_else(|| panic!("no {key}"))
            .to_owned()
    }

    fn num(report: &str, key: &str) -> f64 {
        value(report, key).parse().unwrap()
    }

    #[test]
    fn smoke_routing_beats_one_head_and_the_hash_control() {
        let root = temp("smoke");
        let _ = fs::remove_dir_all(&root);
        cli(&["bin".into(), "smoke".into(), root.display().to_string()]).unwrap();
        let report = fs::read_to_string(root.join("out/h2.tsv")).unwrap();
        assert_eq!(value(&report, "label"), "SMOKE");
        assert_eq!(value(&report, "bytes_q4_cal_gate"), "true");
        assert_eq!(value(&report, "bytes_q4_eval4_read"), "true");
        assert_eq!(value(&report, "bytes_q4_dev_read"), "false");
        assert_eq!(value(&report, "bytes_q4_kernel_oracle_mismatches"), "0");
        for seed in SEEDS {
            let a1d = num(&report, &format!("bytes_q4_a1d_seed{seed}_eval4_bpc"));
            let a2 = num(&report, &format!("bytes_q4_a2_seed{seed}_eval4_bpc"));
            let a2r = num(&report, &format!("bytes_q4_a2r_seed{seed}_eval4_bpc"));
            assert!(
                a2 < a1d - 0.05 && a2 < a2r - 0.05,
                "seed {seed}: {a2} {a1d} {a2r}"
            );
            let path = root.join(format!("out/experts-a2-seed{seed}.tsv"));
            let (router, heads, eighths) =
                parse_experts(&fs::read_to_string(path).unwrap()).unwrap();
            assert_eq!(
                value(&report, &format!("bytes_q4_a2_seed{seed}_replay_sha256")),
                replay_digest(&router, &heads, eighths).unwrap()
            );
            assert_eq!(
                value(&report, &format!("bytes_q4_a2_seed{seed}_router")),
                router_hex(&router)
            );
        }
        assert!(["GO", "GO_DIAGNOSTIC_COST"].contains(&value(&report, "verdict").as_str()));
        // A preregistered run refuses smoke weights and heads, and the
        // output directory is never overwritten.
        let p1 = root.join("p1");
        assert!(run(
            &p1,
            &root.join("h1d"),
            &root.join("pre"),
            "PREREGISTERED",
            false
        )
        .is_err());
        assert!(run(&p1, &root.join("h1d"), &root.join("out"), "SMOKE", false).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_gate_stage_needs_no_eval4_and_matches_the_full_run() {
        let root = temp("gate");
        let _ = fs::remove_dir_all(&root);
        write_smoke(&root, 14).unwrap();
        let p1 = root.join("p1");
        let h1d = root.join("h1d");
        let dir = p1.join(BASE);
        for f in ["trace-eval4.bin", "trace-eval4.banks.bin", "eval4.json"] {
            fs::rename(dir.join(f), root.join(f)).unwrap();
        }
        run(&p1, &h1d, &root.join("gate"), "SMOKE", false).unwrap();
        let gate = fs::read_to_string(root.join("gate/h2.tsv")).unwrap();
        assert_eq!(value(&gate, "verdict"), "CAL_GATE_PASS");
        assert_eq!(value(&gate, "bytes_q4_eval4_read"), "false");
        for f in ["trace-eval4.bin", "trace-eval4.banks.bin", "eval4.json"] {
            fs::rename(root.join(f), dir.join(f)).unwrap();
        }
        run(&p1, &h1d, &root.join("full"), "SMOKE", true).unwrap();
        for seed in SEEDS {
            let name = format!("experts-a2-seed{seed}.tsv");
            assert_eq!(
                fs::read(root.join("gate").join(&name)).unwrap(),
                fs::read(root.join("full").join(&name)).unwrap()
            );
        }
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn without_signal_the_gate_stops_before_eval4() {
        let root = temp("weak");
        let _ = fs::remove_dir_all(&root);
        write_smoke(&root, 8).unwrap();
        run(
            &root.join("p1"),
            &root.join("h1d"),
            &root.join("out"),
            "SMOKE",
            true,
        )
        .unwrap();
        let report = fs::read_to_string(root.join("out/h2.tsv")).unwrap();
        assert_eq!(value(&report, "verdict"), "NO_GO_CAL_GATE");
        assert_eq!(value(&report, "bytes_q4_eval4_read"), "false");
        // An H-1d output that is not a finished run is refused.
        let h1d = root.join("h1d");
        let text = fs::read_to_string(h1d.join("h1d.tsv")).unwrap();
        fs::write(
            h1d.join("h1d.tsv"),
            text.replace("stage\trun", "stage\tgate"),
        )
        .unwrap();
        assert!(run(&root.join("p1"), &h1d, &root.join("second"), "SMOKE", false).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn experts_round_trip_and_replay_detects_changes() {
        let records = replay_bank_records(16);
        let router = Router {
            prototypes: [
                route_bits(&records[0]),
                route_bits(&records[1]),
                route_bits(&records[2]),
                route_bits(&records[3]),
            ],
        };
        let heads: Vec<BankHead> = (0..EXPERTS)
            .map(|k| {
                let weights = (0..DEPTHS * ROW)
                    .map(|i| ((splitmix((k * 100_000 + i) as u64) % 3) as i8) - 1)
                    .collect();
                BankHead::new(weights, [3; DEPTHS], [k as i32; DEPTHS])
            })
            .collect();
        let arm = Arm {
            routing: Routing::Learned(router),
            heads: heads.clone(),
            tau: 0.25,
            eighths: 8,
            sel_bpc: 0.0,
            snapshots: 0,
        };
        let text = experts_tsv(&arm).unwrap();
        let (r2, h2, e2) = parse_experts(&text).unwrap();
        assert_eq!(r2, router);
        assert_eq!(e2, 8);
        let digest = replay_digest(&router, &heads, 8).unwrap();
        assert_eq!(digest, replay_digest(&r2, &h2, e2).unwrap());
        let mut other = router;
        other.prototypes[0][0] ^= 1 << 7;
        assert_ne!(digest, replay_digest(&other, &heads, 8).unwrap());
        assert!(parse_experts(&text.replace("experts\t4", "experts\t3")).is_err());
        assert!(parse_experts(&text.replace("lambda_eighths\t8", "lambda_eighths\t3")).is_err());
    }
}
