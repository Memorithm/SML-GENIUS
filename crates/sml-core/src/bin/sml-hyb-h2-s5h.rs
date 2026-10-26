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
//! (`doc_index % 32 == 23`). An expert is then admitted only if, on its
//! own CAL-SEL bytes, it beats P1 by at least `ADMIT_BPC`; the bytes of an
//! expert that is not admitted are coded by P1 alone. Every `gate` or
//! `run` first runs the same pipeline on a synthetic task with no signal
//! (null control): if it admits any expert, the verdict is
//! `NO_GO_NULL_CONTROL`.
//!
//! * `gate P1_H1D_OUT H1D_OUT NEW_OUTPUT_DIR` is stage 1: `CAL_GATE_PASS`
//!   when the CAL-SEL gain of A2 over A1d reaches `CAL_GATE_BPC` for both
//!   seeds, otherwise `NO_GO_CAL_GATE`. EVAL4 does not exist yet: P1
//!   traces it (`trace-h2-eval4`) only after a pass.
//! * `run P1_H1D_OUT H1D_OUT NEW_OUTPUT_DIR` redoes stage 1
//!   (deterministic) and, if the gate passes, reads `trace-eval4.bin`, its
//!   sidecar and `eval4.json` once.
//!
//! * `templates P1_H1D_OUT H1D_OUT NEW_OUTPUT_DIR` is the H-2t template
//!   feasibility diagnostic (docs/HYB-H2T-TEMPLATE-FEASIBILITY.md): CAL
//!   only, flat templates from the shape of the decoded text
//!   (`hyb_h2t_templates`), one expert per template, a match-model
//!   estimate; it writes `h2t.tsv` and never opens EVAL4.
//! * `copy P1_H1D_OUT H1D_OUT NEW_OUTPUT_DIR` is the H-2c copy-expert
//!   feasibility diagnostic (docs/HYB-H2C-COPY-FEASIBILITY.md): CAL only,
//!   a per-document match model mixed after each A1d head
//!   (`hyb_h2c_copy`) with null, timing, donor and lag controls, a
//!   FIT/SEL swap, a document bootstrap, a poisoned-future causality check
//!   and COPY-1 for comparison; it writes
//!   `h2c.tsv` and never opens EVAL4.
//!
//! The S5h DEV, EVAL2 and EVAL3 are never opened. `replay EXPERTS.tsv`
//! prints the digest of a router and its experts on synthetic records
//! (cross-host check); `smoke`, `smoke-templates` and `smoke-copy` run on generated
//! traces and are labelled SMOKE, never a result. TEST is never read. NOT_PROMOTED.

use sml_core::{
    elastic_ngram_paged_sha::{hex, sha256},
    gb11_p1_stream::{
        read_trace, squash_value, TraceRecord, MAX_BANKS, STRETCH_LIMIT, TRACE_RECORD_BYTES,
    },
    hyb_h1_residual::{a0_cost, decision_cost, splitmix, target_bit, DEPTHS},
    hyb_h1b_calibrated::{LAMBDA_EIGHTHS, TAUS},
    hyb_h1c_stable::{BURN_IN_EPOCHS, EPOCHS},
    hyb_h1d_bank_head::{
        replay_bank_records, BankHead, BankMaster, BankPlane, BankRecord, FusedBankHead,
        BANK_PLANE_BYTES, ROW,
    },
    hyb_h1d_copy::{mix_probability, CopyExpert, DEFAULT_COPY_WEIGHT_TENTHS},
    hyb_h2_mor_experts::{
        best_expert, hash_route, initial_group, quartile_edges, route_bits, router_bytes,
        train_routed, Majority, Router, EXPERTS, ROUNDS, ROUTE_WORDS,
    },
    hyb_h2c_copy::{copy_bucket, expected_bit, CopyFit, CopyMixer, DonorModel, COPY_BUCKETS},
    hyb_h2t_templates::{
        agreement, cluster, length_bucket, nearest, MatchModel, Shape, ShapeState,
    },
    hyb_h3_engine::{parse_experts, parse_h1d_head, Exported},
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
/// Admission: smallest CAL-SEL gain over P1, in bits per byte routed to
/// the expert, for the expert to run.
const ADMIT_BPC: f64 = 0.01;
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
    /// Experts whose CAL-SEL gain over P1 on their own bytes reaches
    /// `ADMIT_BPC`; the bytes of the others keep P1 alone.
    admitted: [bool; EXPERTS],
    /// CAL-SEL gain over P1, in bits per routed byte, per expert.
    gains: [f64; EXPERTS],
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
    let s = select_by(pair, masters, |r| routing.route(r), sel, sel_records)?;
    Ok(Arm {
        routing,
        heads: s.heads,
        admitted: s.admitted,
        gains: s.gains,
        tau: s.tau,
        eighths: s.eighths,
        sel_bpc: s.sel_bpc,
        snapshots,
    })
}

/// What `select_by` keeps: heads at the chosen tau, lambda, admission.
struct Selected {
    heads: Vec<BankHead>,
    admitted: [bool; EXPERTS],
    gains: [f64; EXPERTS],
    tau: f32,
    eighths: i32,
    sel_bpc: f64,
}

/// `select` for any routing function.
fn select_by(
    pair: &mut Pair,
    masters: &[BankMaster],
    route: impl Fn(&BankRecord) -> usize,
    sel: &[Span],
    sel_records: u64,
) -> Result<Selected> {
    let heads: Vec<Vec<BankHead>> = TAUS
        .iter()
        .map(|&t| masters.iter().map(|m| BankHead::export(m, t)).collect())
        .collect();
    let mut bits: SelBits = Default::default();
    pair.all(sel, |r| {
        let k = route(r);
        for (t, row) in heads.iter().enumerate() {
            for (l, c) in row[k].costs(r).iter().enumerate() {
                bits[t][l] += c;
            }
        }
    })?;
    let (t, l, _) = pick(&bits);
    let heads = heads.into_iter().nth(t).expect("tau index");
    let eighths = LAMBDA_EIGHTHS[l];
    // Admission per expert at the chosen (tau, lambda).
    let (mut a0, mut head, mut n) = ([0.0; EXPERTS], [0.0; EXPERTS], [0u64; EXPERTS]);
    pair.all(sel, |r| {
        let k = route(r);
        a0[k] += a0_cost(&r.record);
        head[k] += heads[k].cost(r, eighths);
        n[k] += 1;
    })?;
    let mut gains = [0.0; EXPERTS];
    let mut admitted = [false; EXPERTS];
    let mut total = 0.0;
    for k in 0..EXPERTS {
        if n[k] > 0 {
            gains[k] = (a0[k] - head[k]) / n[k] as f64;
        }
        admitted[k] = n[k] > 0 && gains[k] >= ADMIT_BPC;
        total += if admitted[k] { head[k] } else { a0[k] };
    }
    Ok(Selected {
        heads,
        admitted,
        gains,
        tau: TAUS[t],
        eighths,
        sel_bpc: bpc(total, sel_records),
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
    let admitted: Vec<&str> = arm
        .admitted
        .iter()
        .map(|&a| if a { "1" } else { "0" })
        .collect();
    let mut text = format!(
        "row\t{ROW}\ndepths\t{DEPTHS}\nbanks\t{MAX_BANKS}\nexperts\t{EXPERTS}\nlambda_eighths\t{}\nrouter\t{}\nadmitted\t{}\n",
        arm.eighths,
        router_hex(&router),
        admitted.join(","),
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

/// SHA-256 of the route and every i32 correction (0 for an expert that
/// is not admitted) on fixed synthetic records; stops if a kernel and its
/// oracle disagree.
fn replay_digest(e: &Exported) -> Result<String> {
    let mut bytes = router_bytes(&e.router);
    bytes.extend(e.admitted.iter().map(|&a| u8::from(a)));
    bytes.reserve(REPLAY_RECORDS * (1 + DEPTHS * 4));
    for r in replay_bank_records(REPLAY_RECORDS) {
        let k = e.router.route(&r);
        if k != e.router.oracle_route(&r) {
            return Err("replay: router/oracle mismatch".into());
        }
        bytes.push(k as u8);
        for d in 0..DEPTHS {
            let raw = e.heads[k].raw(&r, d);
            if raw != e.heads[k].oracle_raw(&r, d) {
                return Err("replay: kernel/oracle mismatch".into());
            }
            let correction = if e.admitted[k] {
                e.heads[k].correction(raw, d, e.eighths)
            } else {
                0
            };
            bytes.extend_from_slice(&correction.to_le_bytes());
        }
    }
    Ok(hex(&sha256(&bytes)))
}

impl Arm {
    fn exported(&self) -> Result<Exported> {
        let Routing::Learned(router) = self.routing else {
            return Err("only a learned router is exported".into());
        };
        Ok(Exported {
            router,
            heads: self.heads.clone(),
            admitted: self.admitted,
            eighths: self.eighths,
        })
    }
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

fn gains_text(gains: &[f64; EXPERTS]) -> String {
    let parts: Vec<String> = gains.iter().map(|g| format!("{g:.6}")).collect();
    parts.join(",")
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
             {BASE}_a2r_seed{seed}_lambda_eighths\t{}\n{BASE}_a2r_seed{seed}_snapshots\t{}\n\
             {BASE}_a2_seed{seed}_admitted\t{:?}\n{BASE}_a2_seed{seed}_expert_sel_gains\t{}\n\
             {BASE}_a2r_seed{seed}_admitted\t{:?}\n{BASE}_a2r_seed{seed}_expert_sel_gains\t{}\n",
            a1d_sel[k],
            a.a2.sel_bpc,
            a1d_sel[k] - a.a2.sel_bpc,
            a.a2.tau,
            a.a2.eighths,
            a.a2.snapshots,
            rounds.join(";"),
            router_hex(&router),
            a.a2.heads.iter().map(BankHead::zero_fraction).sum::<f64>() / EXPERTS as f64,
            replay_digest(&a.a2.exported()?)?,
            a.a2r.sel_bpc,
            a.a2r.tau,
            a.a2r.eighths,
            a.a2r.snapshots,
            a.a2.admitted,
            gains_text(&a.a2.gains),
            a.a2r.admitted,
            gains_text(&a.a2r.gains),
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
            bits[s][0] += if a.a2.admitted[k] {
                fused[s].0[k].cost(r, a.a2.eighths)
            } else {
                a0_cost(&r.record)
            };
            if s == 0 {
                nanos[1] += t.elapsed().as_nanos();
            }
            used[s][k] += 1;
            let j = a.a2r.routing.route(r);
            bits[s][1] += if a.a2r.admitted[j] {
                fused[s].1[j].cost(r, a.a2r.eighths)
            } else {
                a0_cost(&r.record)
            };
            bits[s][2] += a1d_fused[s].0.cost(r, a1d_fused[s].1);
            if k != a.a2.routing.oracle_route(r) || j != a.a2r.routing.oracle_route(r) {
                mismatches += 1;
            }
            // The fused kernel that scores EVAL4 against the scalar oracle.
            for (arm, fused, e) in [(&a.a2, &fused[s].0[k], k), (&a.a2r, &fused[s].1[j], j)] {
                let got = fused.corrections(r, arm.eighths);
                let head = &arm.heads[e];
                for (depth, &c) in got.iter().enumerate() {
                    if c != head.correction(head.oracle_raw(r, depth), depth, arm.eighths) {
                        mismatches += 1;
                    }
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

/// Runs the whole CAL pipeline on a synthetic task with no signal and
/// returns how many experts (A2 and A2r, both seeds) it admits: a sound
/// admission rule admits none.
fn null_control(out: &Path) -> Result<usize> {
    let root = out.join("null-control");
    write_smoke(&root, 8)?;
    let task = root.join("out");
    fs::create_dir(&task)?;
    let report = evaluate(
        &root.join("p1").join(BASE),
        &root.join("h1d"),
        &task,
        "SMOKE",
        false,
    )?;
    let mut admitted = 0;
    for line in report.lines() {
        let Some((key, value)) = line.split_once('\t') else {
            continue;
        };
        if key.ends_with("_admitted") {
            admitted += value.matches("true").count();
        }
    }
    Ok(admitted)
}

fn run(p1: &Path, h1d: &Path, out: &Path, label: &str, open_eval4: bool) -> Result<()> {
    fs::create_dir(out)?;
    let null_admitted = null_control(out)?;
    let report = evaluate(&p1.join(BASE), h1d, out, label, open_eval4)?;
    let (body, verdict) = report
        .trim_end()
        .rsplit_once("verdict\t")
        .ok_or("the report has no verdict")?;
    let verdict = if null_admitted == 0 {
        verdict
    } else {
        "NO_GO_NULL_CONTROL"
    };
    let report = format!(
        "label\t{label}\nstage\t{}\n{body}{BASE}_admit_bpc\t{ADMIT_BPC}\n\
         {BASE}_null_control_admitted\t{null_admitted}\nverdict\t{verdict}\n",
        if open_eval4 { "run" } else { "gate" },
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

// ---- HYB H-2t: template feasibility diagnostic (CAL only) ----------------
//
// Protocol: docs/HYB-H2T-TEMPLATE-FEASIBILITY.md. Reads only `trace-cal.bin`
// and its sidecar (CAL-FIT and CAL-SEL) and the A1d heads; never EVAL4,
// the S5h DEV or TEST. Writes `h2t.tsv`, labelled DIAGNOSTIC: it decides
// whether a template-routing H-2b is worth preregistering, nothing else.

/// Every `TEMPLATE_SAMPLE_STRIDE`-th CAL-FIT byte feeds the clustering.
const TEMPLATE_SAMPLE_STRIDE: u64 = 64;
const TEMPLATE_ROUNDS: usize = 8;
/// Template counts whose stability is reported; experts use `EXPERTS`.
const TEMPLATE_COUNTS: [usize; 2] = [EXPERTS, 16];
/// Feasibility: CAL-SEL gain of template-routed experts over A1d, both
/// seeds (the H-2 gate)...
const TEMPLATE_GAIN_BPC: f64 = CAL_GATE_BPC;
/// ... and agreement of the two seeds' `EXPERTS`-template clusterings.
const TEMPLATE_MIN_AGREEMENT: f64 = 0.8;
/// Within-document match model table size.
const DOC_MATCH_BITS: u32 = 16;

/// Template id of every CAL record, looked up by document and position.
struct TemplateIds {
    docs: std::collections::HashMap<u32, (u32, usize)>,
    ids: Vec<u8>,
}

impl TemplateIds {
    #[inline]
    fn id(&self, r: &BankRecord) -> usize {
        let (first, offset) = self.docs[&r.record.doc_index];
        usize::from(self.ids[offset + (r.record.position - first) as usize])
    }
}

/// Replays `spans` document by document with a fresh `ShapeState` each,
/// calling `each(record, signature before the byte)`. Positions must be
/// contiguous inside a document.
fn shapes(
    pair: &mut Pair,
    spans: &[Span],
    mut each: impl FnMut(&BankRecord, &Shape),
) -> Result<()> {
    for &span in spans {
        let mut state = ShapeState::new();
        let mut next: Option<u32> = None;
        let mut gap = false;
        pair.each(span, |r| {
            if next.is_some_and(|p| p != r.record.position) {
                gap = true;
            }
            next = Some(r.record.position + 1);
            each(r, &state.signature());
            state.push(r.record.target);
        })?;
        if gap {
            return Err(format!("CAL doc {}: positions are not contiguous", span.doc).into());
        }
    }
    Ok(())
}

fn template_ids(pair: &mut Pair, spans: &[Span], centres: &[Shape]) -> Result<TemplateIds> {
    let mut docs = std::collections::HashMap::new();
    let mut ids = Vec::new();
    for &span in spans {
        let mut first = None;
        shapes(pair, &[span], |r, s| {
            first.get_or_insert(r.record.position);
            ids.push(nearest(centres, s) as u8);
        })?;
        let first = first.ok_or("empty CAL document")?;
        docs.insert(span.doc, (first, ids.len() - span.count as usize));
    }
    Ok(TemplateIds { docs, ids })
}

fn shapes_hex(centres: &[Shape]) -> String {
    centres
        .iter()
        .map(|c| format!("{:016x}{:016x}", c[0], c[1]))
        .collect::<Vec<_>>()
        .join(",")
}

/// Mean length of the runs of equal ids inside documents.
fn mean_run(t: &TemplateIds, spans: &[Span]) -> f64 {
    let (mut runs, mut bytes) = (0u64, 0u64);
    for span in spans {
        let (_, offset) = t.docs[&span.doc];
        let ids = &t.ids[offset..offset + span.count as usize];
        runs += 1 + ids.windows(2).filter(|w| w[0] != w[1]).count() as u64;
        bytes += span.count;
    }
    bytes as f64 / runs.max(1) as f64
}

struct TemplateSeed {
    centres: Vec<Shape>,
    selected: Selected,
    counts: [u64; EXPERTS],
    p1_bpc: [f64; EXPERTS],
    a1d_gain: [f64; EXPERTS],
    oracle_bpc: f64,
    snapshots: u64,
}

/// One seed: templates from CAL-FIT shapes, one H-1d-shaped expert per
/// template trained on CAL-FIT, selected and admitted on CAL-SEL.
fn template_seed(
    dir: &Path,
    seed: u64,
    samples: &[Shape],
    fit: &[Span],
    sel: &[Span],
    sel_records: u64,
    a1d: &(BankHead, i32),
) -> Result<(TemplateSeed, TemplateIds)> {
    let mut pair = Pair::open(dir, "trace-cal")?;
    let centres = cluster(samples, EXPERTS, seed, TEMPLATE_ROUNDS);
    let all: Vec<Span> = fit.iter().chain(sel).copied().collect();
    let ids = template_ids(&mut pair, &all, &centres)?;
    let docs: Vec<(u32, Span)> = fit.iter().map(|s| (s.doc, *s)).collect();
    let (masters, snapshots) = train_routed(
        seed,
        &docs,
        |r| ids.id(r),
        |span, train| pair.each(span, |r| train(r)),
    )?
    .ok_or("no averaged snapshot")?;
    let selected = select_by(&mut pair, &masters, |r| ids.id(r), sel, sel_records)?;
    let (mut counts, mut p1, mut head) = ([0u64; EXPERTS], [0.0; EXPERTS], [0.0; EXPERTS]);
    let mut oracle = 0.0;
    pair.all(sel, |r| {
        let k = ids.id(r);
        counts[k] += 1;
        p1[k] += a0_cost(&r.record);
        head[k] += a1d.0.cost(r, a1d.1);
        oracle += selected
            .heads
            .iter()
            .map(|h| h.cost(r, selected.eighths))
            .fold(a0_cost(&r.record), f64::min);
    })?;
    let per = |b: f64, n: u64| if n == 0 { 0.0 } else { b / n as f64 };
    Ok((
        TemplateSeed {
            centres,
            counts,
            p1_bpc: std::array::from_fn(|k| per(p1[k], counts[k])),
            a1d_gain: std::array::from_fn(|k| per(p1[k] - head[k], counts[k])),
            oracle_bpc: bpc(oracle, sel_records),
            snapshots,
            selected,
        },
        ids,
    ))
}

/// Match-model estimate: per length bucket, the CAL-FIT hit rate `p`; on
/// CAL-SEL a covered byte costs `-log2 p` on a hit and `-log2(1 - p)`
/// plus its P1 cost on a miss, or its P1 cost if that is smaller.
/// Returns report lines for one variant (`cross` over the CAL stream,
/// `doc` within each document).
fn match_report(pair: &mut Pair, fit: &[Span], sel: &[Span], variant: &str) -> Result<String> {
    let cross = variant == "cross";
    let mut model = MatchModel::new();
    let mut seen = [[0u64; 2]; 4];
    for &span in fit {
        if !cross {
            model = MatchModel::with_bits(DOC_MATCH_BITS);
        }
        pair.each(span, |r| {
            let p = model.predict();
            let b = length_bucket(p);
            seen[b][0] += 1;
            seen[b][1] += u64::from(p.is_some_and(|(q, _)| q == r.record.target));
            model.push(r.record.target);
        })?;
    }
    let rate: [f64; 4] = std::array::from_fn(|b| {
        ((seen[b][1] as f64 + 0.5) / (seen[b][0] as f64 + 1.0)).clamp(1e-6, 1.0 - 1e-6)
    });
    let (mut n, mut covered, mut hits, mut p1_all, mut p1_cov, mut est) =
        (0u64, 0u64, 0u64, 0.0, 0.0, 0.0);
    for &span in sel {
        if !cross {
            model = MatchModel::with_bits(DOC_MATCH_BITS);
        }
        pair.each(span, |r| {
            let c = a0_cost(&r.record);
            let p = model.predict();
            n += 1;
            p1_all += c;
            if let Some((q, _)) = p {
                let rb = rate[length_bucket(p)];
                covered += 1;
                p1_cov += c;
                let cost = if q == r.record.target {
                    hits += 1;
                    -rb.log2()
                } else {
                    -(1.0 - rb).log2() + c
                };
                est += cost.min(c);
            } else {
                est += c;
            }
            model.push(r.record.target);
        })?;
    }
    let gain = (p1_all - est) / n.max(1) as f64;
    Ok(format!(
        "{BASE}_match_{variant}_fit_hit_rate_by_bucket\t{}\n\
         {BASE}_match_{variant}_sel_coverage\t{:.6}\n{BASE}_match_{variant}_sel_hit_rate\t{:.6}\n\
         {BASE}_match_{variant}_sel_p1_bpc_on_covered\t{:.6}\n\
         {BASE}_match_{variant}_sel_gain_estimate_bpc\t{gain:.6}\n",
        rate.iter()
            .map(|r| format!("{r:.4}"))
            .collect::<Vec<_>>()
            .join(","),
        covered as f64 / n.max(1) as f64,
        hits as f64 / covered.max(1) as f64,
        p1_cov / covered.max(1) as f64,
    ))
}

/// The H-1d calibration trace split into CAL-FIT and CAL-SEL, with the
/// published A1d heads. Nothing else is opened.
struct Cal {
    dir: std::path::PathBuf,
    weights: String,
    heads: Vec<(BankHead, i32)>,
    fit: Vec<Span>,
    sel: Vec<Span>,
    sel_records: u64,
}

fn load_cal(p1: &Path, h1d: &Path, label: &str) -> Result<Cal> {
    let dir = p1.join(BASE);
    let json = fs::read_to_string(dir.join("trace.json"))?;
    let weights = json_str(&json, "weights_sha256")?.to_string();
    if label == "PREREGISTERED" && weights != P1_WEIGHTS_SHA256 {
        return Err(format!("P1 weights {weights} are not the H-1d bytes_q4 weights").into());
    }
    let heads = h1d_heads(h1d, label, &weights)?;
    let cal = scan(
        &dir.join("trace-cal.bin"),
        &dir.join("trace-cal.banks.bin"),
        2,
    )?;
    check_reported(&json, "cal_trace", bpc(cal.bits, cal.records))?;
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
    let sel_records = sel.iter().map(|s| s.count).sum();
    Ok(Cal {
        dir,
        weights,
        heads,
        fit,
        sel,
        sel_records,
    })
}

fn templates(p1: &Path, h1d: &Path, out: &Path, label: &str) -> Result<()> {
    fs::create_dir(out)?;
    let Cal {
        dir,
        weights,
        heads: a1d_heads,
        fit,
        sel,
        sel_records,
    } = load_cal(p1, h1d, label)?;
    let mut pair = Pair::open(&dir, "trace-cal")?;
    let (mut a0_sel, mut a1d_sel) = (0.0, [0.0; SEEDS.len()]);
    pair.all(&sel, |r| {
        a0_sel += a0_cost(&r.record);
        for (k, (head, eighths)) in a1d_heads.iter().enumerate() {
            a1d_sel[k] += head.cost(r, *eighths);
        }
    })?;
    let a1d_sel = a1d_sel.map(|b| bpc(b, sel_records));
    let mut samples = Vec::new();
    let mut index = 0u64;
    shapes(&mut pair, &fit, |_, s| {
        if index % TEMPLATE_SAMPLE_STRIDE == 0 {
            samples.push(*s);
        }
        index += 1;
    })?;
    let seeds: Vec<Result<(TemplateSeed, TemplateIds)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = SEEDS
            .iter()
            .enumerate()
            .map(|(k, &seed)| {
                let (samples, fit, sel, dir, head) = (&samples, &fit, &sel, &dir, &a1d_heads[k]);
                scope.spawn(move || template_seed(dir, seed, samples, fit, sel, sel_records, head))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("seed thread"))
            .collect()
    });
    let mut arms = Vec::new();
    for s in seeds {
        arms.push(s?);
    }
    let mut report = format!(
        "label\t{label}\nstage\ttemplates\nschema\tSML_HYB_H2T_TEMPLATES_V1\n\
         {BASE}_p1_weights_sha256\t{weights}\n{BASE}_dev_read\tfalse\n{BASE}_eval4_read\tfalse\n\
         {BASE}_cal_sel_records\t{sel_records}\n{BASE}_a0_sel_bpc\t{:.9}\n\
         {BASE}_template_samples\t{}\n{BASE}_template_rounds\t{TEMPLATE_ROUNDS}\n",
        bpc(a0_sel, sel_records),
        samples.len(),
    );
    // Stability: the two seeds' clusterings on CAL-SEL, at each count.
    let mut agreement_experts = 0.0;
    for &k in &TEMPLATE_COUNTS {
        let labels: Vec<Vec<u8>> = SEEDS
            .iter()
            .map(|&seed| {
                let centres = cluster(&samples, k, seed, TEMPLATE_ROUNDS);
                let mut ids = Vec::with_capacity(sel_records as usize);
                shapes(&mut pair, &sel, |_, s| ids.push(nearest(&centres, s) as u8))?;
                Ok(ids)
            })
            .collect::<Result<_>>()?;
        let a = agreement(&labels[0], &labels[1], k);
        if k == EXPERTS {
            agreement_experts = a;
        }
        report += &format!("{BASE}_templates{k}_seed_agreement\t{a:.6}\n");
    }
    let mut feasible = agreement_experts >= TEMPLATE_MIN_AGREEMENT;
    for (k, (a, ids)) in arms.iter().enumerate() {
        let seed = SEEDS[k];
        let gain = a1d_sel[k] - a.selected.sel_bpc;
        feasible &= gain >= TEMPLATE_GAIN_BPC;
        let list = |v: &[f64; EXPERTS]| {
            v.iter()
                .map(|x| format!("{x:.6}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        report += &format!(
            "{BASE}_a1d_seed{seed}_sel_bpc\t{:.9}\n{BASE}_t_seed{seed}_sel_bpc\t{:.9}\n\
             {BASE}_t_seed{seed}_sel_gain_over_a1d\t{gain:.9}\n\
             {BASE}_t_seed{seed}_oracle_sel_bpc\t{:.9}\n{BASE}_t_seed{seed}_tau\t{}\n\
             {BASE}_t_seed{seed}_lambda_eighths\t{}\n{BASE}_t_seed{seed}_snapshots\t{}\n\
             {BASE}_t_seed{seed}_admitted\t{:?}\n{BASE}_t_seed{seed}_expert_sel_gains\t{}\n\
             {BASE}_t_seed{seed}_sel_bytes\t{:?}\n{BASE}_t_seed{seed}_sel_p1_bpc\t{}\n\
             {BASE}_t_seed{seed}_sel_a1d_gain\t{}\n{BASE}_t_seed{seed}_sel_mean_run\t{:.2}\n\
             {BASE}_t_seed{seed}_centres\t{}\n",
            a1d_sel[k],
            a.selected.sel_bpc,
            a.oracle_bpc,
            a.selected.tau,
            a.selected.eighths,
            a.snapshots,
            a.selected.admitted,
            gains_text(&a.selected.gains),
            a.counts,
            list(&a.p1_bpc),
            list(&a.a1d_gain),
            mean_run(ids, &sel),
            shapes_hex(&a.centres),
        );
    }
    for variant in ["cross", "doc"] {
        report += &match_report(&mut pair, &fit, &sel, variant)?;
    }
    report += &format!(
        "{BASE}_template_gain_bpc\t{TEMPLATE_GAIN_BPC}\n\
         {BASE}_template_min_agreement\t{TEMPLATE_MIN_AGREEMENT}\nverdict\t{}\n",
        if feasible {
            "TEMPLATES_FEASIBLE"
        } else {
            "TEMPLATES_NOT_FEASIBLE"
        }
    );
    fs::write(out.join("h2t.tsv"), &report)?;
    print!("{report}");
    Ok(())
}

// H-2c: a per-document copy expert mixed after the A1d head
// (`HYB-H2C-COPY-FEASIBILITY.md`). CAL only: shifts are fitted on
// CAL-FIT, the context length is chosen and the gain measured on CAL-SEL,
// then the roles are swapped once as a check.

/// Context lengths hashed by the copy expert; one is chosen on CAL-SEL.
const COPY_MINS: [usize; 3] = [6, 8, 12];
/// Hash table of one document's copy expert.
const COPY_TABLE_BITS: u32 = 18;
/// Both seeds must gain this much over A1d on CAL-SEL.
const COPY_GAIN_BPC: f64 = CAL_GATE_BPC;
/// The null control must stay within this.
const COPY_NULL_BPC: f64 = 0.002;
/// The lag control must stay under this share of the gain.
const COPY_CONTROL_SHARE: f64 = 0.1;
/// The chosen context must beat the donor by this much, on the whole of
/// CAL-SEL and at the 2.5 % quantile of the document bootstrap.
const COPY_BEYOND_DONOR_BPC: f64 = CAL_GATE_BPC;
/// With CAL-FIT and CAL-SEL swapped, the gain must keep this share.
const COPY_SWAP_SHARE: f64 = 0.5;
/// The lag control uses the prediction made this many bytes earlier.
const COPY_LAG: usize = 4096;
/// Document bootstrap resamples; the 2.5 % quantile must reach the gate.
const COPY_BOOTSTRAP: usize = 1000;
/// One CAL-SEL document in this many gets the poisoned-future check.
const COPY_POISON_STRIDE: usize = 16;
/// COPY-1 (SML #725) weight in tenths: its preregistered 20 % recopy.
const COPY1_TENTHS: u8 = DEFAULT_COPY_WEIGHT_TENTHS;
/// The variants: each context length, then five controls.
/// * `timing`: the longest-context expert's positions and lengths with a
///   random byte (reported only: it knows where the text repeats);
/// * `null`: a random byte and a random length at every position;
/// * `donor`: context 6 looked up in another document of the same split;
/// * `lag`: context 6's prediction made `COPY_LAG` bytes earlier.
const COPY_TIMING: usize = COPY_MINS.len();
const COPY_NULL: usize = COPY_MINS.len() + 1;
const COPY_DONOR: usize = COPY_MINS.len() + 2;
const COPY_LAGGED: usize = COPY_MINS.len() + 3;
const COPY_VARIANTS: usize = COPY_MINS.len() + 4;

type Prediction = Option<(u8, usize)>;
/// One CAL-SEL document: bytes, then bits per base and per base and variant.
type DocBits = (u64, Vec<f64>, Vec<[f64; COPY_VARIANTS]>);

fn copy_name(v: usize) -> String {
    match v {
        COPY_TIMING => "timing".to_string(),
        COPY_NULL => "null".to_string(),
        COPY_DONOR => "donor".to_string(),
        COPY_LAGGED => "lag".to_string(),
        _ => format!("min{}", COPY_MINS[v]),
    }
}

/// Stretches of the eight decisions after one A1d head, or of P1 alone.
fn base_stretches(head: Option<&(BankHead, i32)>, r: &BankRecord) -> [i32; DEPTHS] {
    std::array::from_fn(|d| {
        let p1 = i32::from(r.record.logits[d]);
        head.map_or(p1, |(h, eighths)| {
            p1.saturating_add(h.correction(h.raw(r, d), d, *eighths))
        })
    })
}

/// The bytes of each document, in order.
fn span_bytes(pair: &mut Pair, spans: &[Span]) -> Result<Vec<Vec<u8>>> {
    spans
        .iter()
        .map(|&span| {
            let mut bytes = Vec::with_capacity(span.count as usize);
            pair.each(span, |r| bytes.push(r.record.target))?;
            Ok(bytes)
        })
        .collect()
}

/// Every variant's prediction before each byte of `bytes`, computed from
/// the bytes before it only (and, for the donor, from `donor`).
fn span_predictions(doc: u32, bytes: &[u8], donor: &[u8]) -> Vec<[Prediction; COPY_VARIANTS]> {
    let mut models = COPY_MINS.map(|m| MatchModel::with_min(COPY_TABLE_BITS, m));
    let mut other = DonorModel::new(donor.to_vec(), COPY_TABLE_BITS, COPY_MINS[0]);
    let mut out: Vec<[Prediction; COPY_VARIANTS]> = Vec::with_capacity(bytes.len());
    for (i, &b) in bytes.iter().enumerate() {
        let mut p = [None; COPY_VARIANTS];
        for (v, model) in models.iter().enumerate() {
            p[v] = model.predict();
        }
        let noise = splitmix((u64::from(doc) << 32) | i as u64);
        p[COPY_TIMING] = p[COPY_MINS.len() - 1].map(|(_, n)| (noise as u8, n));
        p[COPY_NULL] = Some(((noise >> 8) as u8, (noise >> 16) as usize % 512));
        p[COPY_DONOR] = other.predict();
        p[COPY_LAGGED] = i.checked_sub(COPY_LAG).and_then(|j| out[j][0]);
        out.push(p);
        for model in &mut models {
            model.push(b);
        }
        other.push(b);
    }
    out
}

/// Code lengths of one measurement: fit on `fit`, measured on `sel`.
struct CopyMeasure {
    fits: Vec<Vec<CopyFit>>,
    mixers: Vec<Vec<CopyMixer>>,
    singles: Vec<Vec<CopyMixer>>,
    /// Per base: no copy, each variant, single shift per context length,
    /// COPY-1; then per document the same, for the bootstrap.
    base: Vec<f64>,
    mixed: Vec<[f64; COPY_VARIANTS]>,
    single: Vec<[f64; COPY_MINS.len()]>,
    copy1: Vec<f64>,
    copy1_active: u64,
    docs: Vec<DocBits>,
    covered: [u64; COPY_VARIANTS],
    hits: [u64; COPY_VARIANTS],
    records: u64,
}

/// Bases: each A1d head, then P1 alone.
fn copy_measure(
    pair: &mut Pair,
    heads: &[(BankHead, i32)],
    (fit, fit_bytes): (&[Span], &[Vec<u8>]),
    (sel, sel_bytes): (&[Span], &[Vec<u8>]),
    with_copy1: bool,
) -> Result<CopyMeasure> {
    let bases = heads.len() + 1;
    let head = |b: usize| heads.get(b);
    let mut fits: Vec<Vec<CopyFit>> = (0..bases)
        .map(|_| (0..COPY_VARIANTS).map(|_| CopyFit::new()).collect())
        .collect();
    for (i, &span) in fit.iter().enumerate() {
        let donor = &fit_bytes[(i + 1) % fit.len()];
        let predictions = span_predictions(span.doc, &fit_bytes[i], donor);
        let mut at = 0;
        pair.each(span, |r| {
            let p = &predictions[at];
            at += 1;
            for (b, f) in fits.iter_mut().enumerate() {
                let s = base_stretches(head(b), r);
                for (v, fv) in f.iter_mut().enumerate() {
                    let Some((q, n)) = p[v] else { continue };
                    for (d, &stretch) in s.iter().enumerate() {
                        if let Some(e) = expected_bit(q, r.record.target, d) {
                            fv.add(stretch, e, target_bit(&r.record, d), copy_bucket(n), d);
                        }
                    }
                }
            }
        })?;
    }
    let mixers: Vec<Vec<CopyMixer>> = fits
        .iter()
        .map(|f| f.iter().map(CopyFit::fit).collect())
        .collect();
    let singles: Vec<Vec<CopyMixer>> = fits
        .iter()
        .map(|f| {
            f[..COPY_MINS.len()]
                .iter()
                .map(CopyFit::fit_single)
                .collect()
        })
        .collect();
    let mut m = CopyMeasure {
        fits,
        mixers,
        singles,
        base: vec![0.0; bases],
        mixed: vec![[0.0; COPY_VARIANTS]; bases],
        single: vec![[0.0; COPY_MINS.len()]; bases],
        copy1: vec![0.0; bases],
        copy1_active: 0,
        docs: Vec::with_capacity(sel.len()),
        covered: [0; COPY_VARIANTS],
        hits: [0; COPY_VARIANTS],
        records: 0,
    };
    for (i, &span) in sel.iter().enumerate() {
        let donor = &sel_bytes[(i + 1) % sel.len()];
        let predictions = span_predictions(span.doc, &sel_bytes[i], donor);
        let mut doc_base = vec![0.0; bases];
        let mut doc_mixed = vec![[0.0; COPY_VARIANTS]; bases];
        let mut expert = CopyExpert::default();
        let mut at = 0;
        pair.each(span, |r| {
            let p = &predictions[at];
            at += 1;
            m.records += 1;
            for (v, pv) in p.iter().enumerate() {
                if let Some((q, _)) = *pv {
                    m.covered[v] += 1;
                    m.hits[v] += u64::from(q == r.record.target);
                }
            }
            if with_copy1 {
                expert.prepare_byte();
            }
            for b in 0..bases {
                let s = base_stretches(head(b), r);
                for (d, &stretch) in s.iter().enumerate() {
                    let bit = target_bit(&r.record, d);
                    let plain = decision_cost(stretch, bit);
                    m.base[b] += plain;
                    doc_base[b] += plain;
                    for (v, pv) in p.iter().enumerate() {
                        let bits = match *pv {
                            Some((q, n)) => {
                                expected_bit(q, r.record.target, d).map_or(plain, |e| {
                                    let w = m.mixers[b][v].stretch(stretch, e, copy_bucket(n), d);
                                    decision_cost(w, bit)
                                })
                            }
                            None => plain,
                        };
                        m.mixed[b][v] += bits;
                        doc_mixed[b][v] += bits;
                        if v < COPY_MINS.len() {
                            m.single[b][v] += match *pv {
                                Some((q, n)) => {
                                    expected_bit(q, r.record.target, d).map_or(plain, |e| {
                                        let w =
                                            m.singles[b][v].stretch(stretch, e, copy_bucket(n), d);
                                        decision_cost(w, bit)
                                    })
                                }
                                None => plain,
                            };
                        }
                    }
                    if with_copy1 {
                        let copy = expert.probability(d, r.record.target);
                        if b == 0 {
                            m.copy1_active += u64::from(copy.is_some());
                        }
                        // The H-1d probability in Q32, from the 16-bit squash.
                        let neural =
                            squash_value(stretch.clamp(-STRETCH_LIMIT, STRETCH_LIMIT)) << 16;
                        let q = f64::from(mix_probability(neural, copy, COPY1_TENTHS))
                            / 4_294_967_296.0;
                        m.copy1[b] -= if bit { q.log2() } else { (1.0 - q).log2() };
                    }
                }
            }
            if with_copy1 {
                expert.push_decoded(r.record.target);
            }
        })?;
        m.docs.push((span.count, doc_base, doc_mixed));
    }
    Ok(m)
}

/// Poisoned-future check: the predictions before the middle of a document
/// (and the one for the middle byte itself) must not change when every
/// byte from the middle on is replaced by noise. Returns (checked, mismatches).
fn copy_poison(sel: &[Span], sel_bytes: &[Vec<u8>]) -> (u64, u64) {
    let (mut checked, mut mismatches) = (0u64, 0u64);
    for i in (0..sel.len()).step_by(COPY_POISON_STRIDE) {
        let bytes = &sel_bytes[i];
        let donor = &sel_bytes[(i + 1) % sel.len()];
        let mid = bytes.len() / 2;
        let mut poisoned = bytes.clone();
        for (j, b) in poisoned.iter_mut().enumerate().skip(mid) {
            *b = splitmix(0x9e37 ^ j as u64) as u8;
        }
        let a = span_predictions(sel[i].doc, bytes, donor);
        let b = span_predictions(sel[i].doc, &poisoned, donor);
        let (mut ea, mut eb) = (CopyExpert::default(), CopyExpert::default());
        for j in 0..=mid.min(bytes.len().saturating_sub(1)) {
            checked += 1;
            mismatches += u64::from(a[j] != b[j]);
            ea.prepare_byte();
            eb.prepare_byte();
            for d in 0..DEPTHS {
                mismatches += u64::from(ea.probability(d, bytes[j]) != eb.probability(d, bytes[j]));
            }
            ea.push_decoded(bytes[j]);
            eb.push_decoded(poisoned[j]);
        }
    }
    (checked, mismatches)
}

/// Gain of variant `v` in bits per byte for the documents `pick`, over
/// the base or, with `over`, over another variant.
fn doc_gain(
    docs: &[DocBits],
    k: usize,
    v: usize,
    over: Option<usize>,
    pick: impl Iterator<Item = usize>,
) -> f64 {
    let (mut n, mut gain) = (0u64, 0.0);
    for i in pick {
        let (count, base, mixed) = &docs[i];
        n += count;
        gain += over.map_or(base[k], |o| mixed[k][o]) - mixed[k][v];
    }
    gain / n.max(1) as f64
}

fn copy(p1: &Path, h1d: &Path, out: &Path, label: &str) -> Result<()> {
    fs::create_dir(out)?;
    let cal = load_cal(p1, h1d, label)?;
    // The donor control needs another document than the one being coded.
    if cal.fit.len() < 2 || cal.sel.len() < 2 {
        return Err("the donor control needs two documents in CAL-FIT and in CAL-SEL".into());
    }
    let mut pair = Pair::open(&cal.dir, "trace-cal")?;
    let seeds = cal.heads.len();
    let fit_bytes = span_bytes(&mut pair, &cal.fit)?;
    let sel_bytes = span_bytes(&mut pair, &cal.sel)?;
    let m = copy_measure(
        &mut pair,
        &cal.heads,
        (&cal.fit, &fit_bytes),
        (&cal.sel, &sel_bytes),
        true,
    )?;
    let swapped = copy_measure(
        &mut pair,
        &cal.heads,
        (&cal.sel, &sel_bytes),
        (&cal.fit, &fit_bytes),
        false,
    )?;
    let (poison_checked, poison_mismatches) = copy_poison(&cal.sel, &sel_bytes);
    let n = m.records;
    let p1_base = seeds;
    let gain = |k: usize, v: usize| bpc(m.base[k] - m.mixed[k][v], n);
    let mut report = format!(
        "label\t{label}\nstage\tcopy\nschema\tSML_HYB_H2C_COPY_V3\n\
         {BASE}_p1_weights_sha256\t{}\n{BASE}_dev_read\tfalse\n{BASE}_eval4_read\tfalse\n\
         {BASE}_cal_sel_records\t{n}\n{BASE}_a0_sel_bpc\t{:.9}\n\
         {BASE}_copy_table_bits\t{COPY_TABLE_BITS}\n",
        cal.weights,
        bpc(m.base[p1_base], n),
    );
    for (k, seed) in SEEDS.iter().take(seeds).enumerate() {
        report += &format!("{BASE}_a1d_seed{seed}_sel_bpc\t{:.9}\n", bpc(m.base[k], n));
    }
    for v in 0..COPY_VARIANTS {
        let name = copy_name(v);
        let seen = &m.fits[0][v].seen;
        let rates: Vec<String> = (0..COPY_BUCKETS)
            .map(|b| format!("{:.4}", seen[b][1] as f64 / seen[b][0].max(1) as f64))
            .collect();
        let counts: Vec<String> = (0..COPY_BUCKETS).map(|b| seen[b][0].to_string()).collect();
        report += &format!(
            "{BASE}_copy_{name}_fit_decisions_by_bucket\t{}\n\
             {BASE}_copy_{name}_fit_right_rate_by_bucket\t{}\n\
             {BASE}_copy_{name}_sel_coverage\t{:.6}\n{BASE}_copy_{name}_sel_hit_rate\t{:.6}\n",
            counts.join(","),
            rates.join(","),
            m.covered[v] as f64 / n.max(1) as f64,
            m.hits[v] as f64 / m.covered[v].max(1) as f64,
        );
        for (k, seed) in SEEDS.iter().take(seeds).enumerate() {
            report += &format!(
                "{BASE}_copy_{name}_seed{seed}_sel_bpc\t{:.9}\n\
                 {BASE}_copy_{name}_seed{seed}_sel_gain_over_a1d\t{:.9}\n\
                 {BASE}_copy_{name}_seed{seed}_shifts\t{}\n",
                bpc(m.mixed[k][v], n),
                gain(k, v),
                m.mixers[k][v].text(),
            );
        }
        report += &format!(
            "{BASE}_copy_{name}_p1_sel_gain_over_p1\t{:.9}\n",
            gain(p1_base, v)
        );
    }
    for (j, min) in COPY_MINS.iter().enumerate() {
        for (k, seed) in SEEDS.iter().take(seeds).enumerate() {
            report += &format!(
                "{BASE}_copy_min{min}_single_seed{seed}_shift\t{}\n\
                 {BASE}_copy_min{min}_single_seed{seed}_sel_gain_over_a1d\t{:.9}\n",
                m.singles[k][j].shifts[0],
                bpc(m.base[k] - m.single[k][j], n),
            );
        }
    }
    let decisions = n * DEPTHS as u64;
    report += &format!(
        "{BASE}_copy1_weight_tenths\t{COPY1_TENTHS}\n{BASE}_copy1_active_decisions\t{:.6}\n",
        m.copy1_active as f64 / decisions.max(1) as f64
    );
    for (k, seed) in SEEDS.iter().take(seeds).enumerate() {
        report += &format!(
            "{BASE}_copy1_seed{seed}_sel_gain_over_a1d\t{:.9}\n",
            bpc(m.base[k] - m.copy1[k], n)
        );
    }
    report += &format!(
        "{BASE}_copy1_p1_sel_gain_over_p1\t{:.9}\n",
        bpc(m.base[p1_base] - m.copy1[p1_base], n)
    );
    let mean = |v: usize| (0..seeds).map(|k| bpc(m.mixed[k][v], n)).sum::<f64>() / seeds as f64;
    let chosen = (0..COPY_MINS.len())
        .min_by(|&a, &b| mean(a).total_cmp(&mean(b)))
        .unwrap_or(0);
    let mut gate = true;
    let mut null = true;
    let mut donor = true;
    let mut lag = true;
    let mut bootstrap = true;
    let mut swap = true;
    let sn = swapped.records;
    for (k, seed) in SEEDS.iter().take(seeds).enumerate() {
        let g = gain(k, chosen);
        gate &= g >= COPY_GAIN_BPC;
        null &= gain(k, COPY_NULL).abs() < COPY_NULL_BPC;
        let beyond = g - gain(k, COPY_DONOR);
        donor &= beyond >= COPY_BEYOND_DONOR_BPC;
        lag &= gain(k, COPY_LAGGED) < COPY_CONTROL_SHARE * g;
        // Document bootstrap of the chosen context's gain.
        let docs = &m.docs;
        let mut state = 0x5eed_0000 ^ *seed;
        let mut draws: Vec<(f64, f64)> = (0..COPY_BOOTSTRAP)
            .map(|_| {
                let pick: Vec<usize> = (0..docs.len())
                    .map(|_| {
                        state = splitmix(state);
                        (state % docs.len() as u64) as usize
                    })
                    .collect();
                (
                    doc_gain(docs, k, chosen, None, pick.iter().copied()),
                    doc_gain(docs, k, chosen, Some(COPY_DONOR), pick.into_iter()),
                )
            })
            .collect();
        draws.sort_by(|a, b| a.0.total_cmp(&b.0));
        let low = draws[COPY_BOOTSTRAP / 40].0;
        let high = draws[COPY_BOOTSTRAP - 1 - COPY_BOOTSTRAP / 40].0;
        bootstrap &= low >= COPY_GAIN_BPC;
        draws.sort_by(|a, b| a.1.total_cmp(&b.1));
        let beyond_low = draws[COPY_BOOTSTRAP / 40].1;
        donor &= beyond_low >= COPY_BEYOND_DONOR_BPC;
        let mut per_doc: Vec<f64> = (0..docs.len())
            .map(|i| doc_gain(docs, k, chosen, None, std::iter::once(i)))
            .collect();
        per_doc.sort_by(f64::total_cmp);
        let positive =
            per_doc.iter().filter(|&&g| g > 0.0).count() as f64 / per_doc.len().max(1) as f64;
        let swap_gain = bpc(swapped.base[k] - swapped.mixed[k][chosen], sn);
        let swap_null = bpc(swapped.base[k] - swapped.mixed[k][COPY_NULL], sn);
        swap &= swap_gain >= COPY_SWAP_SHARE * g && swap_null.abs() < COPY_NULL_BPC;
        report += &format!(
            "{BASE}_copy_seed{seed}_bootstrap_low\t{low:.9}\n{BASE}_copy_seed{seed}_bootstrap_high\t{high:.9}\n\
             {BASE}_copy_seed{seed}_beyond_donor\t{beyond:.9}\n\
             {BASE}_copy_seed{seed}_beyond_donor_bootstrap_low\t{beyond_low:.9}\n\
             {BASE}_copy_seed{seed}_docs\t{}\n{BASE}_copy_seed{seed}_docs_gaining\t{positive:.6}\n\
             {BASE}_copy_seed{seed}_doc_gain_p10\t{:.9}\n{BASE}_copy_seed{seed}_doc_gain_median\t{:.9}\n\
             {BASE}_copy_seed{seed}_swap_gain_over_a1d\t{swap_gain:.9}\n\
             {BASE}_copy_seed{seed}_swap_null_gain_over_a1d\t{swap_null:.9}\n",
            per_doc.len(),
            per_doc[per_doc.len() / 10],
            per_doc[per_doc.len() / 2],
        );
    }
    let causal = poison_mismatches == 0;
    report += &format!(
        "{BASE}_copy_poison_positions\t{poison_checked}\n{BASE}_copy_poison_mismatches\t{poison_mismatches}\n\
         {BASE}_copy_chosen_min\t{}\n{BASE}_copy_gain_bpc\t{COPY_GAIN_BPC}\n\
         {BASE}_copy_null_bpc\t{COPY_NULL_BPC}\n{BASE}_copy_control_share\t{COPY_CONTROL_SHARE}\n\
         {BASE}_copy_beyond_donor_bpc\t{COPY_BEYOND_DONOR_BPC}\n\
         {BASE}_copy_swap_share\t{COPY_SWAP_SHARE}\n\
         {BASE}_copy_pass_gate\t{gate}\n{BASE}_copy_pass_null\t{null}\n{BASE}_copy_pass_donor\t{donor}\n\
         {BASE}_copy_pass_lag\t{lag}\n{BASE}_copy_pass_bootstrap\t{bootstrap}\n\
         {BASE}_copy_pass_swap\t{swap}\n{BASE}_copy_pass_causality\t{causal}\nverdict\t{}\n",
        COPY_MINS[chosen],
        if !causal {
            "COPY_CAUSALITY_FAIL"
        } else if gate && null && donor && lag && bootstrap && swap {
            "COPY_FEASIBLE"
        } else {
            "COPY_NOT_FEASIBLE"
        }
    );
    fs::write(out.join("h2c.tsv"), &report)?;
    print!("{report}");
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
        Some("templates") if args.len() == 5 => templates(
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
            "PREREGISTERED",
        ),
        Some("copy") if args.len() == 5 => copy(
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
            "PREREGISTERED",
        ),
        Some("smoke-copy") if args.len() == 3 => {
            let root = Path::new(&args[2]);
            write_smoke(root, 14)?;
            copy(
                &root.join("p1"),
                &root.join("h1d"),
                &root.join("out"),
                "SMOKE",
            )
        }
        Some("smoke-templates") if args.len() == 3 => {
            let root = Path::new(&args[2]);
            write_smoke(root, 14)?;
            templates(
                &root.join("p1"),
                &root.join("h1d"),
                &root.join("out"),
                "SMOKE",
            )
        }
        Some("replay") if args.len() == 3 => {
            let exported = parse_experts(&fs::read_to_string(&args[2])?)?;
            println!("replay_sha256\t{}", replay_digest(&exported)?);
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
            "usage: (gate|run|templates|copy) P1_H1D_OUT H1D_OUT NEW_OUT | replay EXPERTS.tsv | \
             (smoke|smoke-templates|smoke-copy) NEW_DIR"
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
            let exported = parse_experts(&fs::read_to_string(path).unwrap()).unwrap();
            assert!(exported.admitted.iter().any(|&a| a));
            assert_eq!(
                value(&report, &format!("bytes_q4_a2_seed{seed}_replay_sha256")),
                replay_digest(&exported).unwrap()
            );
            assert_eq!(
                value(&report, &format!("bytes_q4_a2_seed{seed}_router")),
                router_hex(&exported.router)
            );
        }
        assert_eq!(value(&report, "bytes_q4_null_control_admitted"), "0");
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
    fn the_copy_diagnostic_reads_cal_only_and_reports_every_field() {
        let root = temp("copy");
        let _ = fs::remove_dir_all(&root);
        write_smoke(&root, 14).unwrap();
        // EVAL4 is never needed: hide it.
        let dir = root.join("p1").join(BASE);
        for f in ["trace-eval4.bin", "trace-eval4.banks.bin", "eval4.json"] {
            fs::remove_file(dir.join(f)).unwrap();
        }
        let run = |name: &str, label: &str| {
            copy(&root.join("p1"), &root.join("h1d"), &root.join(name), label)
        };
        run("out", "SMOKE").unwrap();
        let report = fs::read_to_string(root.join("out/h2c.tsv")).unwrap();
        assert_eq!(value(&report, "label"), "SMOKE");
        assert_eq!(value(&report, "bytes_q4_eval4_read"), "false");
        assert_eq!(value(&report, "bytes_q4_dev_read"), "false");
        for v in 0..COPY_VARIANTS {
            let name = copy_name(v);
            let c = num(&report, &format!("bytes_q4_copy_{name}_sel_coverage"));
            assert!((0.0..=1.0).contains(&c));
            for seed in SEEDS {
                let shifts = value(&report, &format!("bytes_q4_copy_{name}_seed{seed}_shifts"));
                assert_eq!(
                    shifts.split(',').count(),
                    sml_core::hyb_h2c_copy::COPY_CELLS
                );
                // Random smoke bytes leave every copy cell under the
                // minimum count, so nothing is learned and nothing is
                // gained; only the null, active everywhere, fits noise.
                if v != COPY_NULL {
                    assert!(shifts.split(',').all(|w| w == "0"));
                }
                let g = num(
                    &report,
                    &format!("bytes_q4_copy_{name}_seed{seed}_sel_gain_over_a1d"),
                );
                if v == COPY_NULL {
                    assert!(g.abs() < COPY_NULL_BPC, "{g}");
                } else {
                    assert!(g.abs() < 1e-9);
                }
            }
        }
        assert!(COPY_MINS.contains(&(num(&report, "bytes_q4_copy_chosen_min") as usize)));
        assert_eq!(num(&report, "bytes_q4_copy_poison_mismatches"), 0.0);
        assert!(num(&report, "bytes_q4_copy_poison_positions") > 0.0);
        assert_eq!(value(&report, "bytes_q4_copy_pass_causality"), "true");
        for seed in SEEDS {
            for key in [
                "bootstrap_low",
                "beyond_donor",
                "beyond_donor_bootstrap_low",
                "doc_gain_median",
                "swap_gain_over_a1d",
            ] {
                let g = num(&report, &format!("bytes_q4_copy_seed{seed}_{key}"));
                assert!(g.abs() < 1e-9, "{key}");
            }
            let c1 = num(
                &report,
                &format!("bytes_q4_copy1_seed{seed}_sel_gain_over_a1d"),
            );
            assert!(c1.is_finite());
        }
        assert_eq!(value(&report, "verdict"), "COPY_NOT_FEASIBLE");
        // Deterministic, never overwrites, and refuses smoke weights when
        // preregistered.
        run("again", "SMOKE").unwrap();
        assert_eq!(
            fs::read(root.join("again/h2c.tsv")).unwrap(),
            fs::read(root.join("out/h2c.tsv")).unwrap()
        );
        assert!(run("out", "SMOKE").is_err());
        assert!(run("pre", "PREREGISTERED").is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_template_diagnostic_reads_cal_only_and_reports_every_field() {
        let root = temp("templates");
        let _ = fs::remove_dir_all(&root);
        write_smoke(&root, 14).unwrap();
        // EVAL4 is never needed: hide it.
        let dir = root.join("p1").join(BASE);
        for f in ["trace-eval4.bin", "trace-eval4.banks.bin", "eval4.json"] {
            fs::remove_file(dir.join(f)).unwrap();
        }
        templates(
            &root.join("p1"),
            &root.join("h1d"),
            &root.join("out"),
            "SMOKE",
        )
        .unwrap();
        let report = fs::read_to_string(root.join("out/h2t.tsv")).unwrap();
        assert_eq!(value(&report, "label"), "SMOKE");
        assert_eq!(value(&report, "bytes_q4_eval4_read"), "false");
        assert_eq!(value(&report, "bytes_q4_dev_read"), "false");
        for k in TEMPLATE_COUNTS {
            let a = num(&report, &format!("bytes_q4_templates{k}_seed_agreement"));
            assert!((0.0..=1.0).contains(&a));
        }
        for seed in SEEDS {
            let t = num(&report, &format!("bytes_q4_t_seed{seed}_sel_bpc"));
            let oracle = num(&report, &format!("bytes_q4_t_seed{seed}_oracle_sel_bpc"));
            assert!(oracle <= t + 1e-9, "the oracle bound is a lower bound");
            let bytes = value(&report, &format!("bytes_q4_t_seed{seed}_sel_bytes"));
            let total: u64 = bytes
                .trim_matches(['[', ']'])
                .split(", ")
                .map(|n| n.parse::<u64>().unwrap())
                .sum();
            assert_eq!(total, num(&report, "bytes_q4_cal_sel_records") as u64);
        }
        for variant in ["cross", "doc"] {
            let c = num(&report, &format!("bytes_q4_match_{variant}_sel_coverage"));
            assert!((0.0..=1.0).contains(&c));
        }
        assert!(["TEMPLATES_FEASIBLE", "TEMPLATES_NOT_FEASIBLE"]
            .contains(&value(&report, "verdict").as_str()));
        // Deterministic, never overwrites, and refuses smoke weights when
        // preregistered.
        templates(
            &root.join("p1"),
            &root.join("h1d"),
            &root.join("again"),
            "SMOKE",
        )
        .unwrap();
        assert_eq!(
            fs::read(root.join("again/h2t.tsv")).unwrap(),
            fs::read(root.join("out/h2t.tsv")).unwrap()
        );
        assert!(templates(
            &root.join("p1"),
            &root.join("h1d"),
            &root.join("out"),
            "SMOKE"
        )
        .is_err());
        assert!(templates(
            &root.join("p1"),
            &root.join("h1d"),
            &root.join("pre"),
            "PREREGISTERED"
        )
        .is_err());
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
        // Without signal, no expert of either arm is admitted.
        for seed in SEEDS {
            for arm in ["a2", "a2r"] {
                assert_eq!(
                    value(&report, &format!("bytes_q4_{arm}_seed{seed}_admitted")),
                    format!("{:?}", [false; EXPERTS])
                );
            }
        }
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
            admitted: [true, false, true, true],
            gains: [0.0; EXPERTS],
        };
        let text = experts_tsv(&arm).unwrap();
        let parsed = parse_experts(&text).unwrap();
        assert_eq!(parsed.router, router);
        assert_eq!(parsed.eighths, 8);
        assert_eq!(parsed.admitted, arm.admitted);
        let digest = replay_digest(&arm.exported().unwrap()).unwrap();
        assert_eq!(digest, replay_digest(&parsed).unwrap());
        let mut other = arm.exported().unwrap();
        other.router.prototypes[0][0] ^= 1 << 7;
        assert_ne!(digest, replay_digest(&other).unwrap());
        let mut other = arm.exported().unwrap();
        other.admitted[1] = true;
        assert_ne!(digest, replay_digest(&other).unwrap());
        assert!(parse_experts(&text.replace("admitted\t1,0,1,1", "admitted\t1,0,1")).is_err());
        assert!(parse_experts(&text.replace("experts\t4", "experts\t3")).is_err());
        assert!(parse_experts(&text.replace("lambda_eighths\t8", "lambda_eighths\t3")).is_err());
    }
}
