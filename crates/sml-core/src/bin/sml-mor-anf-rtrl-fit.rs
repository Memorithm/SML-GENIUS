//! Native physical SEC pilot for the first jointly trained MOR + ANF + RTRL cell.
//! Only bounded corpus prefix; source is exploratory and not G0-admitted.
use sml_core::{
    elastic_ngram_paged_sha::{hex, parse_hex, Sha256},
    sparse_gradient_atlas::SparseConfig,
    sparse_mor_anf_recurrent::MorAnfRecurrent,
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
const TRAIN_TARGET: u64 = 384 * 1024;
const DEV_TARGET: u64 = 192 * 1024;
const DOCUMENT_MAX: usize = 2 * 1024 * 1024;

fn sha(path: &Path) -> Result<String> {
    let mut reader = BufReader::with_capacity(1024 * 1024, File::open(path)?);
    let mut state = Sha256::default();
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 { break; }
        state.update(&buf[..n]);
    }
    Ok(hex(&state.finish()))
}

fn execute(corpus: &Path, provenance: &Path, expected_sha: &str, output: &Path) -> Result<()> {
    if parse_hex(expected_sha).is_none() || corpus == provenance || output.exists() {
        return Err("overlapping paths, existing output or invalid source SHA".into());
    }
    for path in [corpus, provenance] {
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("refuse nonregular/symlink SEC sources".into());
        }
    }
    let digest = sha(corpus)?;
    if digest != expected_sha { return Err("source changed since admission".into()); }
    let config = SparseConfig { pages: 4096, max_cached_pages: 32, learning_rate: 0.20 };
    let mut model = MorAnfRecurrent::create(output, config)?;
    let mut reader = BufReader::new(File::open(corpus)?);
    let mut lines = BufReader::new(File::open(provenance)?).lines();
    if lines.next().transpose()?.as_deref() != Some(HEADER) {
        return Err("incorrect provenance schema".into());
    }
    let mut hashes = HashSet::new();
    let mut train_docs = 0u64;
    let mut train_bytes = 0u64;
    let mut train_events = 0u64;
    let mut train_loss = 0f64;
    let mut dev_docs = 0u64;
    let mut dev_bytes = 0u64;
    let mut dev_events = 0u64;
    let mut dev_loss = 0f64;
    let start = std::time::Instant::now();
    for line in lines.take(1000) {
        if train_bytes >= TRAIN_TARGET && dev_bytes >= DEV_TARGET { break; }
        let line = line?;
        let cols: Vec<_> = line.split('\t').collect();
        if cols.len() != 8 || cols[6].is_empty() ||
            !matches!(cols[4], "CC0" | "PD_CLAIM" | "CC_BY_ATTRIBUTION")
        {
            return Err("invalid rights/filing metadata".into());
        }
        let length: usize = cols[3].parse()?;
        if !(128..=8 * 1024 * 1024).contains(&length) {
            return Err("unexpected SEC document length".into());
        }
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes)?;
        let mut separator = [0u8; 1];
        reader.read_exact(&mut separator)?;
        if separator != [b'\n'] { return Err("corrupt document boundary".into()); }
        let expected = parse_hex(cols[2]).ok_or("bad SHA ledger")?;
        let mut h = Sha256::default();
        h.update(&bytes);
        if h.finish() != expected || !hashes.insert(expected) {
            return Err("duplicate or altered SEC document".into());
        }
        if length > DOCUMENT_MAX { continue; }
        if train_bytes < TRAIN_TARGET {
            let (loss, events) = model.train_document(&bytes)?;
            train_docs += 1;
            train_bytes += length as u64;
            train_events += events;
            train_loss += loss;
        } else if dev_bytes < DEV_TARGET {
            let (loss, events) = model.evaluate_document(&bytes)?;
            dev_docs += 1;
            dev_bytes += length as u64;
            dev_events += events;
            dev_loss += loss;
        }
    }
    if train_docs == 0 || dev_docs == 0 || train_events < 1000 || dev_events < 1000 {
        return Err("missing finite train/heldout samples".into());
    }
    let counters = model.counters();
    let experts = model.atlas_counters();
    if counters.unique_router_parameters == 0
        || counters.recurrent_parameter_updates == 0
        || experts.distinct_updated_parameters == 0
        || counters.supervised_target_bits != train_events * 8
        || counters.routed_expert_updates != train_events * 16
    {
        return Err("joint gradient pathway not confirmed".into());
    }
    model.checkpoint(output)?;
    let metrics = format!(
        "architecture\tatlas_parameter_capacity\texpert_parameters_updated\texpert_updates\trouter_parameter_capacity\trouter_parameters_updated\trouter_updates\trecurrent_parameter_capacity\trecurrent_updates\ttrain_docs\ttrain_bytes\ttrain_events\ttrain_bpc\theldout_docs\theldout_bytes\theldout_events\theldout_bpc\tuniform_bpc\ttraining_elapsed_seconds\tg0\ttest\nsml_mor_anf_rtrl_v1\t{}\t{}\t{}\t1024\t{}\t{}\t3\t{}\t{train_docs}\t{train_bytes}\t{train_events}\t{:.9}\t{dev_docs}\t{dev_bytes}\t{dev_events}\t{:.9}\t8.000000000\t{:.4}\tNOT_ADMITTED\tUNOPENED\n",
        config.total_parameters(), experts.distinct_updated_parameters, experts.parameter_updates,
        counters.unique_router_parameters, counters.router_parameter_updates,
        counters.recurrent_parameter_updates,
        train_loss / train_events as f64,
        dev_loss / dev_events as f64,
        start.elapsed().as_secs_f64()
    );
    let mut report = OpenOptions::new().write(true).create_new(true)
        .open(output.join("mor_anf_pilot.tsv"))?;
    report.write_all(metrics.as_bytes())?;
    report.sync_all()?;
    print!("{metrics}");
    println!("SML_MOR_ANF_RTRL_DONE source_sha256={digest} trainable_expert_capacity={} distinct_expert_weights={} router_updated={} recurrent_updates={} test=UNOPENED g0=NOT_ADMITTED",
        config.total_parameters(), experts.distinct_updated_parameters,
        counters.unique_router_parameters, counters.recurrent_parameter_updates);
    Ok(())
}

fn main() {
    let args: Vec<_> = env::args().collect();
    let result = match args.as_slice() {
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot" =>
            execute(Path::new(corpus), Path::new(ledger), digest, Path::new(output)),
        _ => Err("usage: sml-mor-anf-rtrl-fit --pilot CORPUS provenance SOURCE_SHA256 NEW_OUTPUT_DIR".into()),
    };
    if let Err(error) = result {
        eprintln!("SML_MOR_ANF_RTRL_FAIL {error}");
        std::process::exit(1);
    }
}
