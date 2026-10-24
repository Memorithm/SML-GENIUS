//! GB-11 P1: Boolean-addressed count banks on the S5h manifest, against the
//! `--stream-50m-msb` witness. Protocol: docs/GB11-P1-S5H-PREREGISTRATION.md.
//!
//! `run DATA_ROOT MANIFEST.tsv NEW_OUTPUT_DIR [EXPECTED_MANIFEST_SHA256]` runs
//! the frozen protocol; `smoke` takes the same arguments with tiny search and
//! table budgets (labelled SMOKE, never a result). TEST is never read: the
//! manifest admits only TRAIN then DEV rows. Research only, NOT_PROMOTED.

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
}

const RUN: Plan = Plan {
    label: "PREREGISTERED",
    fit_bytes: 16 * 1024 * 1024,
    holdout_bytes: 2 * 1024 * 1024,
    small_table: Q4_EQUAL_BYTES,
    large_table: F32_EQUAL_BYTES,
};

const SMOKE: Plan = Plan {
    label: "SMOKE",
    fit_bytes: 64 * 1024,
    holdout_bytes: 16 * 1024,
    small_table: 1 << 20,
    large_table: 1 << 22,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Train,
    Dev,
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

fn load_roster(root: &Path, manifest: &Path) -> Result<Roster> {
    if fs::symlink_metadata(root)?.file_type().is_symlink() || !root.is_dir() {
        return Err("data root must be a directory, not a symlink".into());
    }
    let root = fs::canonicalize(root)?;
    let info = fs::symlink_metadata(manifest)?;
    if !info.is_file() || info.len() == 0 || info.len() > MAX_MANIFEST_BYTES {
        return Err("manifest must be a bounded regular file".into());
    }
    let contents = fs::read(manifest)?;
    let text = std::str::from_utf8(&contents)?;
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Err("invalid document manifest header".into());
    }
    let mut docs = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut seen_digests = HashSet::new();
    let mut dev_started = false;
    for line in lines {
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
        let path = checked_path(&root, f[1])?;
        if !seen_digests.insert(digest) || !seen_paths.insert(path.clone()) {
            return Err("duplicate document path or digest".into());
        }
        docs.push(Document {
            index: docs.len(),
            role,
            path,
            length,
            digest,
        });
    }
    if !dev_started || docs.first().is_none_or(|d| d.role != Role::Train) {
        return Err("manifest needs TRAIN then DEV rows".into());
    }
    Ok(Roster {
        docs,
        manifest_sha: hex(&sha256(&contents)),
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
    table: usize,
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

fn run_arm(roster: &Roster, arm: &Arm, out: &Path, manifest_sha: &str) -> Result<ArmResult> {
    let tables = Tables::new();
    let mut model = Model::new(arm.masks.clone(), arm.rule, arm.update, arm.table);
    let mut train_trace = if arm.trace {
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
    if arm.trace {
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

fn cli(args: &[String]) -> Result<()> {
    let usage =
        "usage: sml-gb11-p1-s5h (run|smoke) DATA_ROOT MANIFEST.tsv NEW_OUTPUT_DIR [MANIFEST_SHA256]";
    let (plan, root, manifest, out, expected) = match args {
        [_, mode, root, manifest, out, rest @ ..] if rest.len() <= 1 => {
            let plan = match mode.as_str() {
                "run" => RUN,
                "smoke" => SMOKE,
                _ => return Err(usage.into()),
            };
            (plan, root, manifest, out, rest.first())
        }
        _ => return Err(usage.into()),
    };
    let (root, out) = (Path::new(root), Path::new(out));
    let roster = load_roster(root, Path::new(manifest))?;
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
                        let mut model = Model::new(masks.clone(), rule, update, plan.small_table);
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
            table: plan.small_table,
            rule,
            update,
            trace: *name == "search_q4",
        })
        .collect();
    arms.push(Arm {
        name: "search_f32",
        masks: searched,
        table: plan.large_table,
        rule: chosen[0].0,
        update: chosen[0].1,
        trace: false,
    });

    let results: Vec<Result<ArmResult>> = std::thread::scope(|scope| {
        let handles: Vec<_> = arms
            .iter()
            .map(|arm| {
                let (roster, sha) = (&roster, roster.manifest_sha.as_str());
                scope.spawn(move || run_arm(roster, arm, out, sha))
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
        "SML_GB11_P1_DONE plan={} manifest={} search_s={search_seconds:.1} total_s={:.1} search_min_gain_bpc={SEARCH_MIN_GAIN_BPC} search_max_bits={SEARCH_MAX_BITS} test=UNOPENED model_promoted=false\n",
        plan.label,
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

    #[test]
    fn smoke_run_writes_metrics_and_reproducible_traces() {
        let home = std::env::temp_dir().join(format!("gb11-p1-smoke-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
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
