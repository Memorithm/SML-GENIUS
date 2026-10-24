//! GB-11 P1: Boolean-addressed count banks on the S5h manifest, against the
//! `--stream-50m-msb` witness. Protocol: docs/GB11-P1-S5H-PREREGISTRATION.md.
//!
//! `run DATA_ROOT MANIFEST.tsv NEW_OUTPUT_DIR [EXPECTED_MANIFEST_SHA256]` runs
//! the frozen protocol; `smoke` takes the same arguments with tiny search and
//! table budgets (labelled SMOKE, never a result). TEST is never read: the
//! manifest admits only TRAIN then DEV rows. Research only, NOT_PROMOTED.
//!
//! `run-h1b` / `smoke-h1b` serve HYB H-1b
//! (docs/HYB-H1B-S5H-CALIBRATED-RESIDUAL-PREREGISTRATION.md): TRAIN files
//! with `doc_index % 16 == 7` (CAL) are kept out of the search and of
//! training, and the `search_q4` and `bytes_q4` arms trace CAL and DEV with
//! the frozen model into `NEW_OUTPUT_DIR/<arm>/`.
//!
//! `run-h1c` / `smoke-h1c` serve HYB H-1c
//! (docs/HYB-H1C-S5H-STABLE-RESIDUAL-PREREGISTRATION.md): `run-h1b`, plus
//! `EVAL2_MANIFEST.tsv` (EVAL2 rows only, never trained on) traced with the
//! frozen model as split 3 into `<arm>/trace-eval2.bin`, after an untraced
//! timing pass over EVAL2.
//!
//! `run-h1d` / `smoke-h1d` serve HYB H-1d
//! (docs/HYB-H1D-S5H-BANK-HEAD-PREREGISTRATION.md), in two stages so that
//! EVAL3 stays unopened unless the CAL-SEL gate passes:
//!
//! * `run-h1d DATA_ROOT MANIFEST.tsv NEW_OUTPUT_DIR MANIFEST_SHA256|-`
//!   trains only `bytes_q4` (no search), CAL held out as in `run-h1b`, and
//!   traces CAL (split 2) with the frozen model, with a `.banks.bin` sidecar
//!   of per-bank stretches (`BankPlane`). It saves `weights.bin`. EVAL3 is
//!   not taken and the S5h DEV is never opened.
//! * `trace-h1d-eval3 DATA_ROOT MANIFEST.tsv P1_OUTPUT_DIR
//!   MANIFEST_SHA256|- EVAL3_MANIFEST.tsv` (run only after the gate passed)
//!   restores the frozen model from `weights.bin`, times an untraced pass
//!   over EVAL3 (EVAL3 rows only, never trained on), then traces it as
//!   split 4 with its sidecar and writes `eval3.json`.

use sml_core::hyb_h1d_bank_head::BankPlane;
use sml_core::{
    elastic_ngram_paged_sha::{hex, parse_hex, sha256},
    gb11_p1_stream::{
        holdout_cost, order_mask, search_masks, Context, MixRule, Model, Tables, TraceRecord,
        UpdateRule, CONFIGS, F32_EQUAL_BYTES, Q4_EQUAL_BYTES, SEARCH_MAX_BITS, SEARCH_MIN_GAIN_BPC,
        STRETCH_LIMIT, STRETCH_SCALE, TRACE_RECORD_BYTES,
    },
};
use std::{
    collections::HashSet,
    error::Error,
    fs::{self, File},
    io::{BufWriter, Read, Write},
    path::{Component, Path, PathBuf},
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const HEADER: &str = "role\tpath\tbytes\tsha256\trights_review";
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FILES: usize = 100_000;
const MIN_DOCUMENT_BYTES: u64 = 128;
const MAX_DOCUMENT_BYTES: u64 = 2 * 1024 * 1024;
const SEARCH_BANKS: usize = 6;
const TRAIN_TRACE_STRIDE: usize = 20;
/// H-1b calibration split: TRAIN files with `doc_index % CAL_STRIDE == CAL_RESIDUE`.
const CAL_STRIDE: usize = 16;
const CAL_RESIDUE: usize = 7;
/// Arms traced for H-1b.
const H1B_ARMS: [&str; 2] = ["search_q4", "bytes_q4"];
const RIGHTS: [&str; 7] = [
    "OWNED_REVIEWED",
    "PUBLIC_DOMAIN_REVIEWED",
    "CC0_REVIEWED",
    "CC_BY_ATTRIBUTION_REVIEWED",
    "FR_OPEN_LICENSE_V2_ATTRIBUTION_REVIEWED",
    "APACHE_2_0_ATTRIBUTION_REVIEWED",
    "MIT_ATTRIBUTION_REVIEWED",
];

/// Frozen budgets; `smoke` shrinks them for local and CI checks.
#[derive(Clone, Copy)]
struct Plan {
    label: &'static str,
    fit_bytes: usize,
    holdout_bytes: usize,
    small_table: usize,
    large_table: usize,
    /// H-1b: hold CAL out of training and trace it with the frozen model.
    cal: bool,
    /// H-1c: also trace the EVAL2 rows of a second manifest.
    eval2: bool,
}

const RUN: Plan = Plan {
    label: "PREREGISTERED",
    fit_bytes: 16 * 1024 * 1024,
    holdout_bytes: 2 * 1024 * 1024,
    small_table: Q4_EQUAL_BYTES,
    large_table: F32_EQUAL_BYTES,
    cal: false,
    eval2: false,
};

const SMOKE: Plan = Plan {
    label: "SMOKE",
    fit_bytes: 64 * 1024,
    holdout_bytes: 16 * 1024,
    small_table: 1 << 20,
    large_table: 1 << 22,
    cal: false,
    eval2: false,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Train,
    Dev,
    /// H-1b calibration files: TRAIN rows kept out of training.
    Cal,
    /// H-1c evaluation files, from the EVAL2 manifest.
    Eval2,
}

struct Document {
    index: usize,
    role: Role,
    path: PathBuf,
    length: u64,
    digest: [u8; 32],
}

struct Roster {
    docs: Vec<Document>,
    manifest_sha: String,
    eval2_sha: Option<String>,
}

fn checked_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let rel = Path::new(relative);
    if rel.is_absolute() || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("manifest path must be relative and normal".into());
    }
    let path = fs::canonicalize(root.join(rel))?;
    if !path.starts_with(root) {
        return Err("manifest source escaped the data directory".into());
    }
    Ok(path)
}

fn manifest_text(manifest: &Path) -> Result<Vec<u8>> {
    let info = fs::symlink_metadata(manifest)?;
    if !info.is_file() || info.len() == 0 || info.len() > MAX_MANIFEST_BYTES {
        return Err("manifest must be a bounded regular file".into());
    }
    let contents = fs::read(manifest)?;
    let text = std::str::from_utf8(&contents)?;
    if text.lines().next() != Some(HEADER) {
        return Err("invalid document manifest header".into());
    }
    Ok(contents)
}

/// Checks one manifest row and appends it to `docs`.
fn push_document(
    root: &Path,
    f: &[&str],
    role: Role,
    docs: &mut Vec<Document>,
    seen: &mut (HashSet<PathBuf>, HashSet<[u8; 32]>),
) -> Result<()> {
    if !RIGHTS.contains(&f[4]) {
        return Err("document rights have not been declared reviewed".into());
    }
    let length: u64 = f[2].parse()?;
    if !(MIN_DOCUMENT_BYTES..=MAX_DOCUMENT_BYTES).contains(&length) {
        return Err("document length outside budget".into());
    }
    let digest = parse_hex(f[3]).ok_or("invalid SHA256 field")?;
    if f[3] != hex(&digest) {
        return Err("noncanonical SHA256 field".into());
    }
    let path = checked_path(root, f[1])?;
    if !seen.1.insert(digest) || !seen.0.insert(path.clone()) {
        return Err("duplicate document path or digest".into());
    }
    docs.push(Document {
        index: docs.len(),
        role,
        path,
        length,
        digest,
    });
    Ok(())
}

/// `eval` is the held-out manifest and the only role its rows may carry
/// (`EVAL2` for H-1c, `EVAL3` for H-1d).
fn load_roster(root: &Path, manifest: &Path, eval: Option<(&Path, &str)>) -> Result<Roster> {
    if fs::symlink_metadata(root)?.file_type().is_symlink() || !root.is_dir() {
        return Err("data root must be a directory, not a symlink".into());
    }
    let root = fs::canonicalize(root)?;
    let contents = manifest_text(manifest)?;
    let mut lines = std::str::from_utf8(&contents)?.lines().skip(1);
    let mut docs = Vec::new();
    let mut seen = (HashSet::new(), HashSet::new());
    let mut dev_started = false;
    for line in lines.by_ref() {
        if docs.len() == MAX_FILES {
            return Err("too many documents".into());
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 5 {
            return Err("invalid manifest record".into());
        }
        let role = match f[0] {
            "TRAIN" if !dev_started => Role::Train,
            "DEV" => {
                dev_started = true;
                Role::Dev
            }
            _ => return Err("invalid document role/order (TEST is never admitted)".into()),
        };
        push_document(&root, &f, role, &mut docs, &mut seen)?;
    }
    if !dev_started || docs.first().is_none_or(|d| d.role != Role::Train) {
        return Err("manifest needs TRAIN then DEV rows".into());
    }
    // EVAL2 rows follow, with fresh indices; a path or digest already in
    // the main manifest is refused.
    let eval2_sha = match eval {
        Some((path, label)) => {
            let contents = manifest_text(path)?;
            let before = docs.len();
            for line in std::str::from_utf8(&contents)?.lines().skip(1) {
                if docs.len() == MAX_FILES {
                    return Err("too many documents".into());
                }
                let f: Vec<&str> = line.split('\t').collect();
                if f.len() != 5 || f[0] != label {
                    return Err(format!("{label} manifest admits only {label} rows").into());
                }
                push_document(&root, &f, Role::Eval2, &mut docs, &mut seen)?;
            }
            if docs.len() == before {
                return Err(format!("empty {label} manifest").into());
            }
            Some(hex(&sha256(&contents)))
        }
        None => None,
    };
    Ok(Roster {
        docs,
        manifest_sha: hex(&sha256(&contents)),
        eval2_sha,
    })
}

fn load(doc: &Document) -> Result<Vec<u8>> {
    let mut data = Vec::with_capacity(doc.length as usize);
    File::open(&doc.path)?.read_to_end(&mut data)?;
    if data.len() as u64 != doc.length || sha256(&data) != doc.digest {
        return Err(format!("document {} changed or is invalid", doc.index).into());
    }
    std::str::from_utf8(&data)?;
    Ok(data)
}

struct Arm {
    name: &'static str,
    masks: Vec<u128>,
    /// Bytes per bank: `table / SEARCH_BANKS` for searched masks (the
    /// allocation the search scored), `table / banks` otherwise.
    bank_bytes: usize,
    rule: MixRule,
    update: UpdateRule,
    trace: bool,
}

struct ArmResult {
    row: String,
    weights: Option<Vec<u8>>,
    trace_summary: Option<String>,
}

fn masks_text(masks: &[u128]) -> String {
    masks
        .iter()
        .map(|m| format!("{m:x}"))
        .collect::<Vec<_>>()
        .join("/")
}

fn trace_writer(out: &Path, name: &str) -> Result<BufWriter<File>> {
    Ok(BufWriter::with_capacity(
        1 << 20,
        File::create_new(out.join(name))?,
    ))
}

fn file_sha(path: &Path) -> Result<String> {
    let mut sha = sml_core::elastic_ngram_paged_sha::Sha256::default();
    let mut input = File::open(path)?;
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            return Ok(hex(&sha.finish()));
        }
        sha.update(&buffer[..n]);
    }
}

fn run_arm(
    roster: &Roster,
    arm: &Arm,
    out: &Path,
    manifest_sha: &str,
    plan: Plan,
) -> Result<ArmResult> {
    let cal = plan.cal;
    let tables = Tables::new();
    let mut model = Model::with_bank_bytes(arm.masks.clone(), arm.rule, arm.update, arm.bank_bytes);
    let arm_dir = if cal && arm.trace {
        let dir = out.join(arm.name);
        fs::create_dir(&dir)?;
        dir
    } else {
        out.to_path_buf()
    };
    let out = arm_dir.as_path();
    let mut train_trace = if arm.trace && !cal {
        Some(trace_writer(out, "trace-train.bin")?)
    } else {
        None
    };
    let start = Instant::now();
    let (mut train_bits, mut train_events, mut train_bytes) = (0.0, 0u64, 0u64);
    let mut traced_train = (0u64, 0.0f64);
    for doc in roster.docs.iter().filter(|d| d.role == Role::Train) {
        let data = load(doc)?;
        let trace_this = doc.index % TRAIN_TRACE_STRIDE == 0;
        let mut io_error = None;
        let (bits, events) = model.code_document(&tables, &data, true, |pos, ctx, byte, code| {
            if let (true, Some(w)) = (trace_this, train_trace.as_mut()) {
                let record = record(doc.index, pos, 0, byte, ctx, code);
                traced_train.0 += 1;
                traced_train.1 += record.cost_bits();
                if let Err(e) = w.write_all(&record.encode()) {
                    io_error.get_or_insert(e);
                }
            }
        });
        if let Some(e) = io_error {
            return Err(e.into());
        }
        train_bits += bits;
        train_events += events;
        train_bytes += doc.length;
    }
    let train_seconds = start.elapsed().as_secs_f64();
    if let Some(mut w) = train_trace.take() {
        w.flush()?;
    }
    let trained_sha = model.weights_sha();
    // H-1b cost baseline: one untraced frozen DEV pass.
    let mut dev_seconds = 0.0;
    if cal && arm.trace {
        let start = Instant::now();
        for doc in roster.docs.iter().filter(|d| d.role == Role::Dev) {
            let data = load(doc)?;
            let t = Instant::now();
            std::hint::black_box(model.code_document(&tables, &data, false, |_, _, _, _| {}));
            dev_seconds += t.elapsed().as_secs_f64();
        }
        eprintln!(
            "GB11_P1_H1B_TIMING arm={} dev_pass_s={:.1}",
            arm.name,
            start.elapsed().as_secs_f64()
        );
    }
    let mut dev_trace = if arm.trace {
        Some(trace_writer(out, "trace-dev.bin")?)
    } else {
        None
    };
    let (mut dev_bits, mut dev_events, mut dev_bytes, mut dev_files) = (0.0, 0u64, 0u64, 0usize);
    for doc in roster.docs.iter().filter(|d| d.role == Role::Dev) {
        let data = load(doc)?;
        let mut io_error = None;
        let (bits, events) = model.code_document(&tables, &data, false, |pos, ctx, byte, code| {
            if let Some(w) = dev_trace.as_mut() {
                if let Err(e) = w.write_all(&record(doc.index, pos, 1, byte, ctx, code).encode()) {
                    io_error.get_or_insert(e);
                }
            }
        });
        if let Some(e) = io_error {
            return Err(e.into());
        }
        dev_bits += bits;
        dev_events += events;
        dev_bytes += doc.length;
        dev_files += 1;
    }
    if model.weights_sha() != trained_sha {
        return Err("DEV changed the weights".into());
    }
    if let Some(mut w) = dev_trace.take() {
        w.flush()?;
    }
    let mut cal_summary = (0u64, 0.0f64);
    if cal && arm.trace {
        let mut w = trace_writer(out, "trace-cal.bin")?;
        for doc in roster.docs.iter().filter(|d| d.role == Role::Cal) {
            let data = load(doc)?;
            let mut io_error = None;
            model.code_document(&tables, &data, false, |pos, ctx, byte, code| {
                let record = record(doc.index, pos, 2, byte, ctx, code);
                cal_summary.0 += 1;
                cal_summary.1 += record.cost_bits();
                if let Err(e) = w.write_all(&record.encode()) {
                    io_error.get_or_insert(e);
                }
            });
            if let Some(e) = io_error {
                return Err(e.into());
            }
        }
        w.flush()?;
        if model.weights_sha() != trained_sha {
            return Err("CAL changed the weights".into());
        }
        if cal_summary.0 == 0 {
            return Err("empty CAL trace".into());
        }
    }
    // H-1c: untraced EVAL2 timing pass, then the frozen EVAL2 trace.
    let mut eval2_json = String::new();
    if plan.eval2 && arm.trace {
        let eval2_docs = || roster.docs.iter().filter(|d| d.role == Role::Eval2);
        let mut eval2_seconds = 0.0;
        for doc in eval2_docs() {
            let data = load(doc)?;
            let t = Instant::now();
            std::hint::black_box(model.code_document(&tables, &data, false, |_, _, _, _| {}));
            eval2_seconds += t.elapsed().as_secs_f64();
        }
        eprintln!(
            "GB11_P1_H1C_TIMING arm={} eval2_pass_s={eval2_seconds:.1}",
            arm.name
        );
        let mut w = trace_writer(out, "trace-eval2.bin")?;
        let mut summary = (0u64, 0.0f64);
        for doc in eval2_docs() {
            let data = load(doc)?;
            let mut io_error = None;
            model.code_document(&tables, &data, false, |pos, ctx, byte, code| {
                let record = record(doc.index, pos, 3, byte, ctx, code);
                summary.0 += 1;
                summary.1 += record.cost_bits();
                if let Err(e) = w.write_all(&record.encode()) {
                    io_error.get_or_insert(e);
                }
            });
            if let Some(e) = io_error {
                return Err(e.into());
            }
        }
        w.flush()?;
        if model.weights_sha() != trained_sha {
            return Err("EVAL2 changed the weights".into());
        }
        if summary.0 == 0 {
            return Err("empty EVAL2 trace".into());
        }
        eval2_json = format!(
            concat!(
                "  \"eval2_trace\": {{\"file\": \"trace-eval2.bin\", \"rule\": \"EVAL2 manifest rows, never trained on, frozen model\", \"records\": {}, \"bpc\": {:.9}, \"sha256\": \"{}\"}},\n",
                "  \"eval2_us_per_position\": {:.6},\n",
                "  \"eval2_manifest_sha256\": \"{}\",\n"
            ),
            summary.0,
            summary.1 / summary.0 as f64,
            file_sha(&out.join("trace-eval2.bin"))?,
            eval2_seconds * 1e6 / summary.0 as f64,
            roster.eval2_sha.as_deref().unwrap_or("none"),
        );
    }
    if train_events == 0 || dev_events == 0 || !dev_bits.is_finite() {
        return Err("empty or nonfinite run".into());
    }
    let weights_sha = hex(&trained_sha);
    let dev_bpc = dev_bits / dev_events as f64;
    let row = format!(
        "SML_GB11_P1_V1\t{manifest_sha}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{train_bytes}\t{train_events}\t{:.9}\t{dev_files}\t{dev_bytes}\t{dev_events}\t{dev_bpc:.9}\t{train_seconds:.3}\t{weights_sha}\tUNOPENED\tNOT_PROMOTED",
        arm.name,
        model.table_bytes(),
        arm.masks.len(),
        masks_text(&arm.masks),
        arm.rule.name(),
        arm.update.name(),
        model.touched_slots(),
        train_bits / train_events as f64,
    );
    let mut weights = None;
    let mut trace_summary = None;
    if cal && arm.trace {
        let mut buffer = Vec::with_capacity(model.table_bytes());
        model.write_weights(&mut buffer)?;
        fs::write(out.join("weights.bin"), &buffer)?;
        let cal_sha = file_sha(&out.join("trace-cal.bin"))?;
        let dev_sha = file_sha(&out.join("trace-dev.bin"))?;
        let summary = format!(
            concat!(
                "{{\n  \"schema\": \"{}\",\n  \"arm\": \"{}\",\n",
                "  \"record_bytes\": {},\n  \"endianness\": \"little\",\n",
                "  \"layout\": \"as SML_GB11_P1_TRACE_V1; split 2 = CAL frozen, 1 = DEV frozen, 3 = EVAL2 frozen\",\n",
                "  \"logit_scale\": \"ln-odds of P(bit = 1) times {}\",\n",
                "  \"cal_trace\": {{\"file\": \"trace-cal.bin\", \"rule\": \"TRAIN rows with doc_index % {} == {}, kept out of search and training, frozen model\", \"records\": {}, \"bpc\": {:.9}, \"sha256\": \"{}\"}},\n",
                "  \"dev_trace\": {{\"file\": \"trace-dev.bin\", \"records\": {}, \"bpc\": {:.9}, \"sha256\": \"{}\"}},\n",
                "  \"dev_us_per_position\": {:.6},\n",
                "{}",
                "  \"weights_sha256\": \"{}\",\n  \"manifest_sha256\": \"{}\",\n  \"source_commit\": \"{}\",\n  \"test\": \"UNOPENED\"\n}}\n"
            ),
            if plan.eval2 {
                "SML_GB11_P1_H1C_TRACE_V1"
            } else {
                "SML_GB11_P1_H1B_TRACE_V1"
            },
            arm.name,
            TRACE_RECORD_BYTES,
            STRETCH_SCALE,
            CAL_STRIDE,
            CAL_RESIDUE,
            cal_summary.0,
            cal_summary.1 / cal_summary.0 as f64,
            cal_sha,
            dev_events,
            dev_bpc,
            dev_sha,
            dev_seconds * 1e6 / dev_events as f64,
            eval2_json,
            weights_sha,
            manifest_sha,
            std::env::var("SML_SOURCE_COMMIT").unwrap_or_else(|_| "unknown".into()),
        );
        fs::write(out.join("trace.json"), summary)?;
    } else if arm.trace {
        let mut buffer = Vec::with_capacity(model.table_bytes());
        model.write_weights(&mut buffer)?;
        weights = Some(buffer);
        let dev_sha = file_sha(&out.join("trace-dev.bin"))?;
        let train_sha = file_sha(&out.join("trace-train.bin"))?;
        trace_summary = Some(format!(
            concat!(
                "{{\n  \"schema\": \"SML_GB11_P1_TRACE_V1\",\n  \"arm\": \"{}\",\n",
                "  \"record_bytes\": {},\n  \"endianness\": \"little\",\n",
                "  \"layout\": [\"u32 doc_index (0-based manifest row)\", \"u32 position (predicted byte offset in the file)\", ",
                "\"u8 split (0 TRAIN prequential, 1 DEV frozen)\", \"u8 target\", \"u16 W (word register before target)\", ",
                "\"u64 context (c1 = previous byte in the low byte; 0 before file start)\", ",
                "\"8 x i16 logit of decision d on the true MSB path, d = 0 is bit 7\", ",
                "\"u32 confidence (4 bits per bank, min(15, n0 + n1) at decision 0)\"],\n",
                "  \"logit_scale\": \"ln-odds of P(bit = 1) times {}\",\n",
                "  \"logit_saturation\": \"clamped to [-{}, {}] before squash\",\n",
                "  \"squash\": \"p1_q16 = clamp(round(65536 / (1 + exp(-logit / {}))), 1, 65535); cost = 16 - log2(bit ? p1_q16 : 65536 - p1_q16)\",\n",
                "  \"train_trace\": {{\"file\": \"trace-train.bin\", \"rule\": \"TRAIN rows with doc_index % {} == 0, logits before the update\", \"records\": {}, \"bpc\": {:.9}, \"sha256\": \"{}\"}},\n",
                "  \"dev_trace\": {{\"file\": \"trace-dev.bin\", \"records\": {}, \"bpc\": {:.9}, \"sha256\": \"{}\"}},\n",
                "  \"weights_sha256\": \"{}\",\n  \"manifest_sha256\": \"{}\",\n  \"source_commit\": \"{}\",\n  \"test\": \"UNOPENED\"\n}}\n"
            ),
            arm.name,
            TRACE_RECORD_BYTES,
            STRETCH_SCALE,
            STRETCH_LIMIT,
            STRETCH_LIMIT,
            STRETCH_SCALE,
            TRAIN_TRACE_STRIDE,
            traced_train.0,
            traced_train.1 / traced_train.0.max(1) as f64,
            train_sha,
            dev_events,
            dev_bpc,
            dev_sha,
            weights_sha,
            manifest_sha,
            std::env::var("SML_SOURCE_COMMIT").unwrap_or_else(|_| "unknown".into()),
        ));
    }
    Ok(ArmResult {
        row,
        weights,
        trace_summary,
    })
}

fn record(
    index: usize,
    position: usize,
    split: u8,
    target: u8,
    ctx: Context,
    code: &sml_core::gb11_p1_stream::ByteCode,
) -> TraceRecord {
    TraceRecord {
        doc_index: index as u32,
        position: position as u32,
        split,
        target,
        word: ctx.word,
        context: ctx.bytes,
        logits: code.logits,
        confidence: code.confidence,
    }
}

/// Searched arms keep the per-bank bytes `search_masks` scored.
fn bank_bytes(arm: &str, table: usize, banks: usize) -> usize {
    if arm.starts_with("search") {
        table / SEARCH_BANKS
    } else {
        table / banks
    }
}

type Documents = Vec<Vec<u8>>;

fn prefix(roster: &Roster, plan: Plan) -> Result<(Documents, Documents)> {
    let (mut fit, mut holdout) = (Vec::new(), Vec::new());
    let (mut fit_bytes, mut holdout_bytes) = (0, 0);
    for doc in roster.docs.iter().filter(|d| d.role == Role::Train) {
        if fit_bytes < plan.fit_bytes {
            fit_bytes += doc.length as usize;
            fit.push(load(doc)?);
        } else if holdout_bytes < plan.holdout_bytes {
            holdout_bytes += doc.length as usize;
            holdout.push(load(doc)?);
        } else {
            break;
        }
    }
    if holdout_bytes < plan.holdout_bytes {
        return Err("TRAIN too small for the search prefix".into());
    }
    Ok((fit, holdout))
}

/// Traces `docs` with the frozen model as `split` into `<name>.bin` and its
/// `<name>.banks.bin` sidecar; returns (records, bits).
fn trace_with_banks<'a>(
    model: &mut Model,
    tables: &Tables,
    docs: impl Iterator<Item = &'a Document>,
    split: u8,
    out: &Path,
    name: &str,
) -> Result<(u64, f64)> {
    let mut trace = trace_writer(out, &format!("{name}.bin"))?;
    let mut banks = trace_writer(out, &format!("{name}.banks.bin"))?;
    let mut summary = (0u64, 0.0f64);
    for doc in docs {
        let data = load(doc)?;
        let mut io_error = None;
        model.code_document(tables, &data, false, |pos, ctx, byte, code| {
            let record = record(doc.index, pos, split, byte, ctx, code);
            summary.0 += 1;
            summary.1 += record.cost_bits();
            let plane = BankPlane::from_code(doc.index as u32, pos as u32, code);
            if let Err(e) = trace
                .write_all(&record.encode())
                .and_then(|()| banks.write_all(&plane.encode()))
            {
                io_error.get_or_insert(e);
            }
        });
        if let Some(e) = io_error {
            return Err(e.into());
        }
    }
    trace.flush()?;
    banks.flush()?;
    if summary.0 == 0 {
        return Err(format!("empty {name} trace").into());
    }
    Ok(summary)
}

/// HYB H-1d stage 1: trains `bytes_q4` on TRAIN without CAL, then traces
/// CAL with bank sidecars. Neither EVAL3 nor DEV is opened.
fn run_arm_h1d(roster: &Roster, arm: &Arm, out: &Path) -> Result<String> {
    let tables = Tables::new();
    let mut model = Model::with_bank_bytes(arm.masks.clone(), arm.rule, arm.update, arm.bank_bytes);
    let dir = out.join(arm.name);
    fs::create_dir(&dir)?;
    let start = Instant::now();
    let (mut train_bits, mut train_events, mut train_bytes) = (0.0, 0u64, 0u64);
    for doc in roster.docs.iter().filter(|d| d.role == Role::Train) {
        let data = load(doc)?;
        let (bits, events) = model.code_document(&tables, &data, true, |_, _, _, _| {});
        train_bits += bits;
        train_events += events;
        train_bytes += doc.length;
    }
    let train_seconds = start.elapsed().as_secs_f64();
    if train_events == 0 {
        return Err("empty TRAIN".into());
    }
    let trained_sha = model.weights_sha();
    let cal = trace_with_banks(
        &mut model,
        &tables,
        roster.docs.iter().filter(|d| d.role == Role::Cal),
        2,
        &dir,
        "trace-cal",
    )?;
    if model.weights_sha() != trained_sha {
        return Err("CAL changed the weights".into());
    }
    let mut buffer = Vec::with_capacity(model.table_bytes());
    model.write_weights(&mut buffer)?;
    fs::write(dir.join("weights.bin"), &buffer)?;
    let weights_sha = hex(&trained_sha);
    let summary = format!(
        concat!(
            "{{\n  \"schema\": \"SML_GB11_P1_H1D_TRACE_V1\",\n  \"arm\": \"{}\",\n",
            "  \"rule\": \"{}\",\n  \"update\": \"{}\",\n",
            "  \"record_bytes\": {},\n  \"bank_plane_bytes\": {},\n  \"endianness\": \"little\",\n",
            "  \"layout\": \"as SML_GB11_P1_TRACE_V1; split 2 = CAL frozen; *.banks.bin = BankPlane per record, same order\",\n",
            "  \"logit_scale\": \"ln-odds of P(bit = 1) times {}\",\n",
            "  \"cal_trace\": {{\"file\": \"trace-cal.bin\", \"rule\": \"TRAIN rows with doc_index % {} == {}, kept out of training, frozen model\", \"records\": {}, \"bpc\": {:.9}, \"sha256\": \"{}\", \"banks_sha256\": \"{}\"}},\n",
            "  \"eval3\": \"NOT_OPENED\",\n",
            "  \"dev_read\": false,\n",
            "  \"weights_sha256\": \"{}\",\n  \"manifest_sha256\": \"{}\",\n  \"source_commit\": \"{}\",\n  \"test\": \"UNOPENED\"\n}}\n"
        ),
        arm.name,
        arm.rule.name(),
        arm.update.name(),
        TRACE_RECORD_BYTES,
        sml_core::hyb_h1d_bank_head::BANK_PLANE_BYTES,
        STRETCH_SCALE,
        CAL_STRIDE,
        CAL_RESIDUE,
        cal.0,
        cal.1 / cal.0 as f64,
        file_sha(&dir.join("trace-cal.bin"))?,
        file_sha(&dir.join("trace-cal.banks.bin"))?,
        weights_sha,
        roster.manifest_sha,
        std::env::var("SML_SOURCE_COMMIT").unwrap_or_else(|_| "unknown".into()),
    );
    fs::write(dir.join("trace.json"), summary)?;
    Ok(format!(
        "SML_GB11_P1_H1D_V1\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{train_bytes}\t{train_events}\t{:.9}\t{}\t{:.9}\t{train_seconds:.3}\t{weights_sha}\tUNOPENED\tNOT_PROMOTED",
        roster.manifest_sha,
        arm.name,
        model.table_bytes(),
        arm.masks.len(),
        masks_text(&arm.masks),
        arm.rule.name(),
        arm.update.name(),
        train_bits / train_events as f64,
        cal.0,
        cal.1 / cal.0 as f64,
    ))
}

/// Marks TRAIN rows with `doc_index % CAL_STRIDE == CAL_RESIDUE` as CAL
/// and checks the manifest SHA.
fn h1d_roster(
    root: &Path,
    manifest: &Path,
    expected: Option<&str>,
    eval3: Option<&Path>,
) -> Result<Roster> {
    let mut roster = load_roster(root, manifest, eval3.map(|p| (p, "EVAL3")))?;
    for doc in &mut roster.docs {
        if doc.role == Role::Train && doc.index % CAL_STRIDE == CAL_RESIDUE {
            doc.role = Role::Cal;
        }
    }
    if expected.is_some_and(|e| e != roster.manifest_sha) {
        return Err(format!(
            "manifest SHA256 {} differs from expected",
            roster.manifest_sha
        )
        .into());
    }
    Ok(roster)
}

fn h1d_masks() -> Vec<u128> {
    [0, 1, 2, 3, 4, 6].map(order_mask).to_vec()
}

/// `run-h1d` / `smoke-h1d`: `bytes_q4` only, configuration picked as in
/// every P1 run (TRAIN prefix, CAL excluded), then `run_arm_h1d`.
fn cli_h1d(
    plan: Plan,
    root: &Path,
    manifest: &Path,
    out: &Path,
    expected: Option<&str>,
) -> Result<()> {
    let roster = h1d_roster(root, manifest, expected, None)?;
    let canonical_root = fs::canonicalize(root)?;
    fs::create_dir(out)?;
    if fs::canonicalize(out)?.starts_with(&canonical_root) {
        return Err("output must not be inside the data root".into());
    }
    let start = Instant::now();
    let (fit, holdout) = prefix(&roster, plan)?;
    let fit: Vec<&[u8]> = fit.iter().map(Vec::as_slice).collect();
    let holdout: Vec<&[u8]> = holdout.iter().map(Vec::as_slice).collect();
    let masks = h1d_masks();
    let bank_bytes = bank_bytes("bytes_q4", plan.small_table, masks.len());
    let costs: Vec<f64> = std::thread::scope(|scope| {
        let handles: Vec<_> = CONFIGS
            .iter()
            .map(|&(rule, update)| {
                let (fit, holdout, masks) = (&fit, &holdout, &masks);
                scope.spawn(move || {
                    let mut model = Model::with_bank_bytes(masks.clone(), rule, update, bank_bytes);
                    holdout_cost(&mut model, fit, holdout)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("config"))
            .collect()
    });
    let mut best = 0;
    for (i, &cost) in costs.iter().enumerate() {
        if cost < costs[best] {
            best = i;
        }
    }
    let (rule, update) = CONFIGS[best];
    let arm = Arm {
        name: "bytes_q4",
        masks,
        bank_bytes,
        rule,
        update,
        trace: true,
    };
    let row = run_arm_h1d(&roster, &arm, out)?;
    println!("{row}");
    fs::write(
        out.join("metrics.tsv"),
        format!("schema\tmanifest_sha256\tarm\ttable_bytes\tbanks\tmasks_hex\trule\tupdate\ttrain_bytes\ttrain_events\ttrain_prequential_bpc\tcal_events\tcal_bpc\ttrain_elapsed_s\tweights_sha256\ttest\tpublication\n{row}\n"),
    )?;
    let done = format!(
        "SML_GB11_P1_DONE plan={}_H1D manifest={} total_s={:.1} dev_read=false eval3_read=false test=UNOPENED model_promoted=false\n",
        plan.label,
        roster.manifest_sha,
        start.elapsed().as_secs_f64(),
    );
    print!("{done}");
    fs::write(out.join("COMPLETE"), done)?;
    Ok(())
}

/// The string value of `"key": "..."` in a `trace.json`.
fn json_str<'a>(json: &'a str, key: &str) -> Result<&'a str> {
    let pattern = format!("\"{key}\": \"");
    let rest = &json[json
        .find(&pattern)
        .ok_or_else(|| format!("trace.json has no {key}"))?
        + pattern.len()..];
    Ok(&rest[..rest.find('"').ok_or("unterminated string")?])
}

/// `trace-h1d-eval3` / `smoke-h1d-eval3`: HYB H-1d stage 2, run only after
/// the CAL-SEL gate passed. Restores the frozen `bytes_q4` model of
/// `run-h1d`, times an untraced EVAL3 pass, traces EVAL3 as split 4 with
/// bank sidecars and writes `eval3.json` next to `trace.json`.
fn cli_h1d_eval3(
    plan: Plan,
    root: &Path,
    manifest: &Path,
    p1: &Path,
    expected: Option<&str>,
    eval3: &Path,
) -> Result<()> {
    let roster = h1d_roster(root, manifest, expected, Some(eval3))?;
    let dir = p1.join("bytes_q4");
    let json = fs::read_to_string(dir.join("trace.json"))?;
    if json_str(&json, "schema")? != "SML_GB11_P1_H1D_TRACE_V1"
        || json_str(&json, "manifest_sha256")? != roster.manifest_sha
    {
        return Err("trace.json is not a run-h1d output of this manifest".into());
    }
    let (rule, update) = CONFIGS
        .into_iter()
        .find(|(r, u)| {
            json_str(&json, "rule").is_ok_and(|n| n == r.name())
                && json_str(&json, "update").is_ok_and(|n| n == u.name())
        })
        .ok_or("trace.json names no registered configuration")?;
    let masks = h1d_masks();
    let bank_bytes = bank_bytes("bytes_q4", plan.small_table, masks.len());
    let mut model = Model::with_bank_bytes(masks, rule, update, bank_bytes);
    model.read_weights(&fs::read(dir.join("weights.bin"))?)?;
    let weights_sha = hex(&model.weights_sha());
    if weights_sha != json_str(&json, "weights_sha256")? {
        return Err("weights.bin differs from trace.json".into());
    }
    let tables = Tables::new();
    let eval_docs = || roster.docs.iter().filter(|d| d.role == Role::Eval2);
    let mut eval_seconds = 0.0;
    for doc in eval_docs() {
        let data = load(doc)?;
        let t = Instant::now();
        std::hint::black_box(model.code_document(&tables, &data, false, |_, _, _, _| {}));
        eval_seconds += t.elapsed().as_secs_f64();
    }
    let eval = trace_with_banks(&mut model, &tables, eval_docs(), 4, &dir, "trace-eval3")?;
    if hex(&model.weights_sha()) != weights_sha {
        return Err("EVAL3 changed the weights".into());
    }
    let us_per_position = eval_seconds * 1e6 / eval.0 as f64;
    let summary = format!(
        concat!(
            "{{\n  \"schema\": \"SML_GB11_P1_H1D_EVAL3_V1\",\n",
            "  \"layout\": \"as SML_GB11_P1_TRACE_V1; split 4 = EVAL3 frozen; *.banks.bin = BankPlane per record, same order\",\n",
            "  \"eval3_trace\": {{\"file\": \"trace-eval3.bin\", \"rule\": \"EVAL3 manifest rows, never trained on, frozen model restored from weights.bin\", \"records\": {}, \"bpc\": {:.9}, \"sha256\": \"{}\", \"banks_sha256\": \"{}\"}},\n",
            "  \"eval3_us_per_position\": {:.6},\n",
            "  \"eval3_manifest_sha256\": \"{}\",\n",
            "  \"dev_read\": false,\n",
            "  \"weights_sha256\": \"{}\",\n  \"manifest_sha256\": \"{}\",\n  \"source_commit\": \"{}\",\n  \"test\": \"UNOPENED\"\n}}\n"
        ),
        eval.0,
        eval.1 / eval.0 as f64,
        file_sha(&dir.join("trace-eval3.bin"))?,
        file_sha(&dir.join("trace-eval3.banks.bin"))?,
        us_per_position,
        roster.eval2_sha.as_deref().unwrap_or("none"),
        weights_sha,
        roster.manifest_sha,
        std::env::var("SML_SOURCE_COMMIT").unwrap_or_else(|_| "unknown".into()),
    );
    let mut file = File::create_new(dir.join("eval3.json"))?;
    file.write_all(summary.as_bytes())?;
    println!(
        "SML_GB11_P1_H1D_EVAL3_DONE plan={} records={} bpc={:.9} us_per_position={us_per_position:.4} weights={weights_sha} dev_read=false",
        plan.label,
        eval.0,
        eval.1 / eval.0 as f64,
    );
    Ok(())
}

fn cli(args: &[String]) -> Result<()> {
    let usage = "usage: sml-gb11-p1-s5h (run|smoke|run-h1b|smoke-h1b) DATA_ROOT MANIFEST.tsv NEW_OUTPUT_DIR [MANIFEST_SHA256] | (run-h1c|smoke-h1c) DATA_ROOT MANIFEST.tsv NEW_OUTPUT_DIR MANIFEST_SHA256|- EVAL2_MANIFEST.tsv | (run-h1d|smoke-h1d) DATA_ROOT MANIFEST.tsv NEW_OUTPUT_DIR MANIFEST_SHA256|- | (trace-h1d-eval3|smoke-h1d-eval3) DATA_ROOT MANIFEST.tsv P1_OUTPUT_DIR MANIFEST_SHA256|- EVAL3_MANIFEST.tsv";
    match args {
        [_, mode, root, manifest, out, sha] if mode == "run-h1d" || mode == "smoke-h1d" => {
            let base = if mode == "run-h1d" { RUN } else { SMOKE };
            let plan = Plan { cal: true, ..base };
            let expected = (sha != "-").then_some(sha.as_str());
            return cli_h1d(
                plan,
                Path::new(root),
                Path::new(manifest),
                Path::new(out),
                expected,
            );
        }
        [_, mode, root, manifest, p1, sha, eval3]
            if mode == "trace-h1d-eval3" || mode == "smoke-h1d-eval3" =>
        {
            let base = if mode == "trace-h1d-eval3" {
                RUN
            } else {
                SMOKE
            };
            let plan = Plan { cal: true, ..base };
            let expected = (sha != "-").then_some(sha.as_str());
            return cli_h1d_eval3(
                plan,
                Path::new(root),
                Path::new(manifest),
                Path::new(p1),
                expected,
                Path::new(eval3),
            );
        }
        _ => {}
    }
    let (plan, root, manifest, out, expected, eval2) = match args {
        [_, mode, root, manifest, out, sha, eval2] if mode == "run-h1c" || mode == "smoke-h1c" => {
            let base = if mode == "run-h1c" { RUN } else { SMOKE };
            let plan = Plan {
                cal: true,
                eval2: true,
                ..base
            };
            let expected = (sha != "-").then_some(sha);
            (plan, root, manifest, out, expected, Some(Path::new(eval2)))
        }
        [_, mode, root, manifest, out, rest @ ..] if rest.len() <= 1 => {
            let plan = match mode.as_str() {
                "run" => RUN,
                "smoke" => SMOKE,
                "run-h1b" => Plan { cal: true, ..RUN },
                "smoke-h1b" => Plan { cal: true, ..SMOKE },
                _ => return Err(usage.into()),
            };
            (plan, root, manifest, out, rest.first(), None)
        }
        _ => return Err(usage.into()),
    };
    let (root, out) = (Path::new(root), Path::new(out));
    let mut roster = load_roster(root, Path::new(manifest), eval2.map(|p| (p, "EVAL2")))?;
    if plan.cal {
        for doc in &mut roster.docs {
            if doc.role == Role::Train && doc.index % CAL_STRIDE == CAL_RESIDUE {
                doc.role = Role::Cal;
            }
        }
    }
    if expected.is_some_and(|e| *e != roster.manifest_sha) {
        return Err(format!(
            "manifest SHA256 {} differs from expected",
            roster.manifest_sha
        )
        .into());
    }
    let canonical_root = fs::canonicalize(root)?;
    fs::create_dir(out)?;
    if fs::canonicalize(out)?.starts_with(&canonical_root) {
        return Err("output must not be inside the data root".into());
    }
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let start = Instant::now();
    let (fit, holdout) = prefix(&roster, plan)?;
    let fit: Vec<&[u8]> = fit.iter().map(Vec::as_slice).collect();
    let holdout: Vec<&[u8]> = holdout.iter().map(Vec::as_slice).collect();

    let mut search_log = String::from("bank\tbit\tholdout_bpc\n");
    let searched = search_masks(
        SEARCH_BANKS,
        plan.small_table,
        &fit,
        &holdout,
        threads,
        |s| {
            let bit = if s.bank == 0 {
                "none".to_owned()
            } else {
                s.bit.to_string()
            };
            let line = format!("{}\t{bit}\t{:.6}\n", s.bank, s.holdout_bpc);
            eprint!(
                "GB11_P1_SEARCH t={:.0}s {line}",
                start.elapsed().as_secs_f64()
            );
            search_log.push_str(&line);
        },
    );
    fs::write(out.join("search.tsv"), &search_log)?;
    let search_seconds = start.elapsed().as_secs_f64();

    let candidates: [(&'static str, Vec<u128>); 3] = [
        ("search_q4", searched.clone()),
        ("bytes_q4", [0, 1, 2, 3, 4, 6].map(order_mask).to_vec()),
        ("order2_q4", [0, 1, 2].map(order_mask).to_vec()),
    ];
    let mut config_log = String::from("arm\trule\tupdate\tholdout_bpc\n");
    let holdout_events: usize = holdout.iter().map(|d| d.len().saturating_sub(2)).sum();
    let mut chosen = Vec::new();
    for (name, masks) in &candidates {
        let costs: Vec<f64> = std::thread::scope(|scope| {
            let handles: Vec<_> = CONFIGS
                .iter()
                .map(|&(rule, update)| {
                    let (fit, holdout) = (&fit, &holdout);
                    scope.spawn(move || {
                        let mut model = Model::with_bank_bytes(
                            masks.clone(),
                            rule,
                            update,
                            bank_bytes(name, plan.small_table, masks.len()),
                        );
                        holdout_cost(&mut model, fit, holdout)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("config"))
                .collect()
        });
        let mut best = 0;
        for (i, (&(rule, update), &cost)) in CONFIGS.iter().zip(&costs).enumerate() {
            config_log.push_str(&format!(
                "{name}\t{}\t{}\t{:.6}\n",
                rule.name(),
                update.name(),
                cost / holdout_events as f64
            ));
            if cost < costs[best] {
                best = i;
            }
        }
        chosen.push(CONFIGS[best]);
    }
    fs::write(out.join("configs.tsv"), &config_log)?;
    let mut arms: Vec<Arm> = candidates
        .iter()
        .zip(&chosen)
        .map(|((name, masks), &(rule, update))| Arm {
            name,
            masks: masks.clone(),
            bank_bytes: bank_bytes(name, plan.small_table, masks.len()),
            rule,
            update,
            trace: if plan.cal {
                H1B_ARMS.contains(name)
            } else {
                *name == "search_q4"
            },
        })
        .collect();
    if plan.cal {
        arms.retain(|arm| arm.trace);
    } else {
        arms.push(Arm {
            name: "search_f32",
            bank_bytes: bank_bytes("search_f32", plan.large_table, searched.len()),
            masks: searched,
            rule: chosen[0].0,
            update: chosen[0].1,
            trace: false,
        });
    }

    let results: Vec<Result<ArmResult>> = std::thread::scope(|scope| {
        let handles: Vec<_> = arms
            .iter()
            .map(|arm| {
                let (roster, sha) = (&roster, roster.manifest_sha.as_str());
                scope.spawn(move || run_arm(roster, arm, out, sha, plan))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("arm"))
            .collect()
    });
    let mut metrics = String::from(
        "schema\tmanifest_sha256\tarm\ttable_bytes\tbanks\tmasks_hex\trule\tupdate\ttouched_slots\ttrain_bytes\ttrain_events\ttrain_prequential_bpc\tdev_files\tdev_bytes\tdev_events\tdev_bpc\ttrain_elapsed_s\tweights_sha256\ttest\tpublication\n",
    );
    for result in results {
        let result = result?;
        println!("{}", result.row);
        metrics.push_str(&result.row);
        metrics.push('\n');
        if let Some(weights) = result.weights {
            fs::write(out.join("weights-search_q4.bin"), weights)?;
        }
        if let Some(summary) = result.trace_summary {
            fs::write(out.join("trace.json"), summary)?;
        }
    }
    fs::write(out.join("metrics.tsv"), &metrics)?;
    let done = format!(
        "SML_GB11_P1_DONE plan={}{} manifest={} search_s={search_seconds:.1} total_s={:.1} search_min_gain_bpc={SEARCH_MIN_GAIN_BPC} search_max_bits={SEARCH_MAX_BITS} test=UNOPENED model_promoted=false\n",
        plan.label,
        match (plan.cal, plan.eval2) {
            (_, true) => "_H1C",
            (true, false) => "_H1B",
            _ => "",
        },
        roster.manifest_sha,
        start.elapsed().as_secs_f64(),
    );
    print!("{done}");
    fs::write(out.join("COMPLETE"), done)?;
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(error) = cli(&args) {
        eprintln!("SML_GB11_P1_ERROR {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Twelve generated documents (ten TRAIN, two DEV) and their manifest.
    fn smoke_corpus(home: &Path) -> (PathBuf, PathBuf) {
        let _ = fs::remove_dir_all(home);
        let data = home.join("data");
        fs::create_dir_all(&data).unwrap();
        let mut manifest = format!("{HEADER}\n");
        let words = ["the ", "bank ", "routes ", "a ", "bit ", "slice ", "word "];
        let mut state = 7u64;
        for i in 0..12 {
            let mut text = String::new();
            while text.len() < 24_000 {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                text.push_str(words[(state >> 33) as usize % words.len()]);
            }
            text.push_str(&format!("doc {i}"));
            let name = format!("d{i}.txt");
            fs::write(data.join(&name), &text).unwrap();
            let role = if i < 10 { "TRAIN" } else { "DEV" };
            manifest.push_str(&format!(
                "{role}\t{name}\t{}\t{}\tOWNED_REVIEWED\n",
                text.len(),
                hex(&sha256(text.as_bytes()))
            ));
        }
        let manifest_path = home.join("manifest.tsv");
        fs::write(&manifest_path, &manifest).unwrap();
        (data, manifest_path)
    }

    #[test]
    fn smoke_h1b_holds_cal_out_and_traces_it_frozen() {
        let home = std::env::temp_dir().join(format!("gb11-p1-h1b-{}", std::process::id()));
        let (data, manifest_path) = smoke_corpus(&home);
        let out = home.join("out");
        let args: Vec<String> = ["bin", "smoke-h1b"]
            .iter()
            .map(|s| s.to_string())
            .chain([&data, &manifest_path, &out].map(|p| p.display().to_string()))
            .collect();
        cli(&args).unwrap();
        let metrics = fs::read_to_string(out.join("metrics.tsv")).unwrap();
        assert_eq!(metrics.lines().count(), 3, "header + search_q4 + bytes_q4");
        for arm in H1B_ARMS {
            let dir = out.join(arm);
            let json = fs::read_to_string(dir.join("trace.json")).unwrap();
            assert!(json.contains("\"cal_trace\"") && json.contains("dev_us_per_position"));
            assert!(!dir.join("trace-train.bin").exists());
            let row = metrics
                .lines()
                .find(|l| l.contains(&format!("\t{arm}\t")))
                .unwrap();
            // Nine TRAIN files are trained on: doc 7 is CAL.
            let train_bytes: u64 = row.split('\t').nth(9).unwrap().parse().unwrap();
            let all_train: u64 = (0..10)
                .map(|i| fs::metadata(data.join(format!("d{i}.txt"))).unwrap().len())
                .sum();
            let doc7 = fs::metadata(data.join("d7.txt")).unwrap().len();
            assert_eq!(train_bytes, all_train - doc7);
            let (records, bits) = sml_core::gb11_p1_stream::read_trace(
                &mut File::open(dir.join("trace-cal.bin")).unwrap(),
                |r| assert_eq!((r.split, r.doc_index), (2, 7)),
            )
            .unwrap();
            assert_eq!(records, doc7 - 2);
            let reported = json
                .lines()
                .find(|l| l.contains("\"cal_trace\""))
                .and_then(|l| l.split("\"bpc\": ").nth(1))
                .and_then(|r| r.split(',').next())
                .unwrap();
            assert!((bits / records as f64 - reported.parse::<f64>().unwrap()).abs() < 1e-6);
        }
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn smoke_h1c_traces_eval2_as_split_3() {
        let home = std::env::temp_dir().join(format!("gb11-p1-h1c-{}", std::process::id()));
        let (data, manifest_path) = smoke_corpus(&home);
        let mut eval2 = format!("{HEADER}\n");
        for i in 0..2 {
            let text = format!("an unseen evaluation article {i} ").repeat(600);
            let name = format!("e{i}.txt");
            fs::write(data.join(&name), &text).unwrap();
            eval2.push_str(&format!(
                "EVAL2\t{name}\t{}\t{}\tOWNED_REVIEWED\n",
                text.len(),
                hex(&sha256(text.as_bytes()))
            ));
        }
        let eval2_path = home.join("eval2.tsv");
        fs::write(&eval2_path, &eval2).unwrap();
        let args = |out: &str, eval2: &Path| -> Vec<String> {
            ["bin", "smoke-h1c"]
                .iter()
                .map(|s| s.to_string())
                .chain([&data, &manifest_path, &home.join(out)].map(|p| p.display().to_string()))
                .chain(["-".to_string(), eval2.display().to_string()])
                .collect()
        };
        cli(&args("out", &eval2_path)).unwrap();
        let out = home.join("out");
        assert!(fs::read_to_string(out.join("COMPLETE"))
            .unwrap()
            .contains("plan=SMOKE_H1C "));
        for arm in H1B_ARMS {
            let dir = out.join(arm);
            let json = fs::read_to_string(dir.join("trace.json")).unwrap();
            assert!(
                json.contains("SML_GB11_P1_H1C_TRACE_V1") && json.contains("eval2_us_per_position")
            );
            let (records, bits) = sml_core::gb11_p1_stream::read_trace(
                &mut File::open(dir.join("trace-eval2.bin")).unwrap(),
                |r| assert!(r.split == 3 && (12..14).contains(&r.doc_index)),
            )
            .unwrap();
            let expected: u64 = (0..2)
                .map(|i| fs::metadata(data.join(format!("e{i}.txt"))).unwrap().len() - 2)
                .sum();
            assert_eq!(records, expected);
            let reported = json
                .lines()
                .find(|l| l.contains("\"eval2_trace\""))
                .and_then(|l| l.split("\"bpc\": ").nth(1))
                .and_then(|r| r.split(',').next())
                .unwrap();
            assert!((bits / records as f64 - reported.parse::<f64>().unwrap()).abs() < 1e-6);
        }
        // An EVAL2 row that repeats a main-manifest document is refused, and
        // so is any other role.
        let dev_row = fs::read_to_string(&manifest_path)
            .unwrap()
            .lines()
            .last()
            .unwrap()
            .replacen("DEV", "EVAL2", 1);
        let bad = home.join("bad.tsv");
        fs::write(&bad, format!("{HEADER}\n{dev_row}\n")).unwrap();
        assert!(cli(&args("out2", &bad)).is_err());
        fs::write(&bad, eval2.replace("EVAL2\t", "DEV\t")).unwrap();
        assert!(cli(&args("out3", &bad)).is_err());
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn smoke_h1d_traces_cal_then_eval3_with_bank_sidecars() {
        use sml_core::hyb_h1d_bank_head::BANK_PLANE_BYTES;
        let home = std::env::temp_dir().join(format!("gb11-p1-h1d-{}", std::process::id()));
        let (data, manifest_path) = smoke_corpus(&home);
        let mut eval3 = format!("{HEADER}\n");
        for i in 0..2 {
            let text = format!("a fresh evaluation article {i} ").repeat(500);
            let name = format!("f{i}.txt");
            fs::write(data.join(&name), &text).unwrap();
            eval3.push_str(&format!(
                "EVAL3\t{name}\t{}\t{}\tOWNED_REVIEWED\n",
                text.len(),
                hex(&sha256(text.as_bytes()))
            ));
        }
        let eval3_path = home.join("eval3.tsv");
        fs::write(&eval3_path, &eval3).unwrap();
        let args = |mode: &str, out: &str, tail: &[&Path]| -> Vec<String> {
            let mut v: Vec<String> = ["bin", mode]
                .iter()
                .map(|s| s.to_string())
                .chain([&data, &manifest_path, &home.join(out)].map(|p| p.display().to_string()))
                .collect();
            if mode.contains("h1d") {
                v.push("-".into());
            }
            v.extend(tail.iter().map(|p| p.display().to_string()));
            v
        };
        // Stage 1: CAL only, EVAL3 not taken.
        cli(&args("smoke-h1d", "out", &[])).unwrap();
        let dir = home.join("out/bytes_q4");
        let json = fs::read_to_string(dir.join("trace.json")).unwrap();
        assert!(json.contains("SML_GB11_P1_H1D_TRACE_V1") && json.contains("\"dev_read\": false"));
        assert!(json.contains("\"eval3\": \"NOT_OPENED\""));
        assert!(!dir.join("trace-dev.bin").exists() && !dir.join("trace-eval3.bin").exists());
        // The same bytes_q4 model as run-h1b (same CAL rule and config).
        cli(&args("smoke-h1b", "h1b", &[])).unwrap();
        assert_eq!(
            file_sha(&dir.join("weights.bin")).unwrap(),
            file_sha(&home.join("h1b/bytes_q4/weights.bin")).unwrap()
        );
        // Stage 2 refuses EVAL2 rows, then traces EVAL3 once.
        let bad = home.join("bad.tsv");
        fs::write(&bad, eval3.replace("EVAL3\t", "EVAL2\t")).unwrap();
        assert!(cli(&args("smoke-h1d-eval3", "out", &[&bad])).is_err());
        assert!(!dir.join("trace-eval3.bin").exists());
        cli(&args("smoke-h1d-eval3", "out", &[&eval3_path])).unwrap();
        let eval_json = fs::read_to_string(dir.join("eval3.json")).unwrap();
        assert!(eval_json.contains("SML_GB11_P1_H1D_EVAL3_V1"));
        assert_eq!(
            json_str(&eval_json, "weights_sha256").unwrap(),
            json_str(&json, "weights_sha256").unwrap()
        );
        assert!(
            cli(&args("smoke-h1d-eval3", "out", &[&eval3_path])).is_err(),
            "no overwrite"
        );
        for (name, split, docs) in [("trace-cal", 2u8, 7..8u32), ("trace-eval3", 4, 12..14)] {
            let mut keys = Vec::new();
            let (records, _) = sml_core::gb11_p1_stream::read_trace(
                &mut File::open(dir.join(format!("{name}.bin"))).unwrap(),
                |r| {
                    assert!(r.split == split && docs.contains(&r.doc_index));
                    keys.push((r.doc_index, r.position));
                },
            )
            .unwrap();
            let banks = fs::read(dir.join(format!("{name}.banks.bin"))).unwrap();
            assert_eq!(banks.len() as u64, records * BANK_PLANE_BYTES as u64);
            for (chunk, key) in banks.chunks_exact(BANK_PLANE_BYTES).zip(&keys) {
                let plane = BankPlane::decode(chunk.try_into().unwrap());
                assert_eq!((plane.doc_index, plane.position), *key);
                // bytes_q4 has six banks: the last two stay empty.
                assert!(plane.stretch.iter().all(|row| row[6] == 0 && row[7] == 0));
            }
        }
        // Tampered weights are refused.
        let mut weights = fs::read(dir.join("weights.bin")).unwrap();
        weights[0] ^= 1;
        fs::write(dir.join("weights.bin"), &weights).unwrap();
        fs::remove_file(dir.join("eval3.json")).unwrap();
        fs::remove_file(dir.join("trace-eval3.bin")).unwrap();
        fs::remove_file(dir.join("trace-eval3.banks.bin")).unwrap();
        assert!(cli(&args("smoke-h1d-eval3", "out", &[&eval3_path])).is_err());
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn smoke_run_writes_metrics_and_reproducible_traces() {
        let home = std::env::temp_dir().join(format!("gb11-p1-smoke-{}", std::process::id()));
        let (data, manifest_path) = smoke_corpus(&home);
        let out = home.join("out");
        let args: Vec<String> = ["bin", "smoke"]
            .iter()
            .map(|s| s.to_string())
            .chain([&data, &manifest_path, &out].map(|p| p.display().to_string()))
            .collect();
        cli(&args).unwrap();
        let metrics = fs::read_to_string(out.join("metrics.tsv")).unwrap();
        assert_eq!(metrics.lines().count(), 5);
        let search_row = metrics
            .lines()
            .find(|l| l.contains("\tsearch_q4\t"))
            .unwrap();
        let dev_bpc: f64 = search_row.split('\t').nth(15).unwrap().parse().unwrap();
        assert!(dev_bpc < 3.0, "{dev_bpc}");
        let (records, bits) = sml_core::gb11_p1_stream::read_trace(
            &mut File::open(out.join("trace-dev.bin")).unwrap(),
            |r| {
                assert_eq!(r.split, 1);
            },
        )
        .unwrap();
        let dev_events: u64 = search_row.split('\t').nth(14).unwrap().parse().unwrap();
        assert_eq!(records, dev_events);
        assert!((bits / records as f64 - dev_bpc).abs() < 1e-6);
        let (train_records, _) = sml_core::gb11_p1_stream::read_trace(
            &mut File::open(out.join("trace-train.bin")).unwrap(),
            |r| {
                assert_eq!((r.split, r.doc_index % 20), (0, 0));
            },
        )
        .unwrap();
        assert!(train_records > 0);
        assert!(fs::read_to_string(out.join("trace.json"))
            .unwrap()
            .contains("ln-odds"));
        // A second run into the same directory is refused.
        assert!(cli(&args).is_err());
        let mut wrong = args.clone();
        wrong[4] = home.join("out2").display().to_string();
        wrong.push("00".repeat(32));
        assert!(cli(&wrong).is_err());
        fs::remove_dir_all(&home).unwrap();
    }
}
