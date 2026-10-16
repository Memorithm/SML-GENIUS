//! Finite, pinned, non-commercial-clearance pilot for genuine SML SGD on SEC 10-K.
//! This is a trainable 4,194,304-weight float master, NOT the 25B SML model.
//! The held-out slice is a diagnostic within TRAIN source years, not frozen 2023 DEV.
use sml_core::{
    elastic_ngram_paged_sha::{hex, parse_hex, Sha256},
    sparse_gradient_atlas::{SparseConfig, SparseGradientAtlas},
};
use std::{
    collections::HashSet,
    env,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const HEADER: &str =
    "shard\trow\tsha256\tbytes\tlicense_class\tlicense_source\tidentifier\tcreator_title";
const CAP_DOCUMENT: usize = 2 * 1024 * 1024;
const TRAIN_MAX: u64 = 2 * 1024 * 1024;
const HOLDOUT_MAX: u64 = 1024 * 1024;
const PAGES: u64 = 4096;
const CACHE: usize = 32;

fn full_sha256(path: &Path) -> Result<String> {
    let mut reader = BufReader::with_capacity(1024 * 1024, File::open(path)?);
    let mut hash = Sha256::default();
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hex(&hash.finish()))
}

fn run(corpus: &Path, ledger: &Path, digest: &str, out: &Path) -> Result<()> {
    if !matches!(parse_hex(digest), Some(_)) || corpus == ledger || out.exists() {
        return Err("invalid pinned SHA-256, overlapping input files, or output exists".into());
    }
    for input in [corpus, ledger] {
        let m = fs::symlink_metadata(input)?;
        if !m.is_file() || m.file_type().is_symlink() {
            return Err("SEC source must be a regular non-symlink file".into());
        }
    }
    let actual = full_sha256(corpus)?;
    if actual != digest {
        return Err("SEC TRAIN corpus SHA-256 mismatch".into());
    }
    let mut reader = BufReader::new(File::open(corpus)?);
    let mut lines = BufReader::new(File::open(ledger)?).lines();
    if lines.next().transpose()?.as_deref() != Some(HEADER) {
        return Err("wrong SEC provenance ledger schema".into());
    }
    let cfg = SparseConfig {
        pages: PAGES,
        max_cached_pages: CACHE,
        learning_rate: 0.15,
    };
    let mut model = SparseGradientAtlas::create(out, cfg)?;
    let mut train_bytes = 0u64;
    let mut dev_bytes = 0u64;
    let mut train_events = 0u64;
    let mut dev_events = 0u64;
    let mut train_loss = 0f64;
    let mut dev_loss = 0f64;
    let mut train_docs = 0u64;
    let mut dev_docs = 0u64;
    let mut seen = HashSet::new();
    for (row_index, line) in lines.enumerate() {
        // Early stop: a small finite prefix experiment, NOT a full 10GiB fit.
        if train_bytes >= TRAIN_MAX && dev_bytes >= HOLDOUT_MAX {
            break;
        }
        let line = line?;
        let f: Vec<_> = line.split('\t').collect();
        if f.len() != 8
            || f[0].is_empty()
            || f[1].parse::<usize>().is_err()
            || f[5].is_empty()
            || f[6].is_empty()
            || !matches!(f[4], "PD_CLAIM" | "CC0" | "CC_BY_ATTRIBUTION")
        {
            return Err("corrupt or unqualified training source ledger".into());
        }
        let len: usize = f[3].parse()?;
        if !(128..=8 * 1024 * 1024).contains(&len) {
            return Err("invalid SEC source document bounds".into());
        }
        if row_index > 2_000_000 {
            return Err("excessive provenance rows".into());
        }
        let mut doc = vec![0u8; len];
        reader.read_exact(&mut doc)?;
        let mut newline = [0u8; 1];
        reader.read_exact(&mut newline)?;
        if newline != [b'\n'] {
            return Err("invalid document byte boundary".into());
        }
        let expected = parse_hex(f[2]).ok_or("malformed SEC document SHA")?;
        let mut hash = Sha256::default();
        hash.update(&doc);
        if hash.finish() != expected || !seen.insert(expected) {
            return Err("duplicate or corrupted document".into());
        }
        if len > CAP_DOCUMENT {
            continue;
        }
        if train_bytes < TRAIN_MAX {
            let (loss, events) = model.train_document(&doc)?;
            train_bytes += len as u64;
            train_docs += 1;
            train_loss += loss;
            train_events += events;
        } else if dev_bytes < HOLDOUT_MAX {
            let (loss, events) = model.evaluate_document(&doc)?;
            dev_bytes += len as u64;
            dev_docs += 1;
            dev_loss += loss;
            dev_events += events;
        }
    }
    if train_docs == 0
        || dev_docs == 0
        || train_events == 0
        || dev_events == 0
        || model.counters().distinct_updated_parameters == 0
    {
        return Err("insufficient learned weights or out-of-sample slice".into());
    }
    model.checkpoint(out)?;
    model.export_q4(&out.join("weights.q4"))?;
    let counts = model.counters();
    let metrics = format!(
        "model\tcapacity_parameters\tupdated_unique\tparameter_updates\ttrain_docs\ttrain_bytes\ttrain_events\ttrain_bpc\tholdout_docs\tholdout_bytes\tholdout_events\tholdout_bpc\tbaseline_uniform_bpc\tmaster_bytes\tq4_bytes\tg0\ttest\nsml_sparse_sgd_v1\t{}\t{}\t{}\t{train_docs}\t{train_bytes}\t{train_events}\t{:.9}\t{dev_docs}\t{dev_bytes}\t{dev_events}\t{:.9}\t8.000000000\t{}\t{}\tNOT_ADMITTED\tUNOPENED\n",
        cfg.total_parameters(), counts.distinct_updated_parameters, counts.parameter_updates,
        train_loss / train_events as f64, dev_loss / dev_events as f64,
        cfg.train_master_bytes(), cfg.packed_q4_bytes()
    );
    let mut report = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out.join("pilot.tsv"))?;
    report.write_all(metrics.as_bytes())?;
    report.sync_all()?;
    print!("{metrics}");
    println!("SPARSE_GRADIENT_PILOT_DONE source_sha256={actual} training_is_real=true params_actually_updated={} fit_scope=PREFIX_ONLY g0=NOT_ADMITTED test=UNOPENED", counts.distinct_updated_parameters);
    Ok(())
}

fn main() {
    let args: Vec<_> = env::args().collect();
    let result = match args.as_slice() {
        [_, arg, corpus, ledger, sha, out] if arg == "--pilot" =>
            run(Path::new(corpus), Path::new(ledger), sha, Path::new(out)),
        _ => Err("usage: sml-sparse-gradient-fit --pilot CORPUS.utf8 provenance.tsv SOURCE_SHA256 OUT_DIR".into()),
    };
    if let Err(e) = result {
        eprintln!("SPARSE_GRADIENT_FAIL {e}");
        std::process::exit(1);
    }
}
