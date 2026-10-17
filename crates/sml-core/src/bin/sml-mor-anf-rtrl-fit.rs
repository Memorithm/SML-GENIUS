//! Native physical SEC pilot for the first jointly trained MOR + ANF + RTRL cell.
//! Only bounded corpus prefix; source is exploratory and not G0-admitted.
use sml_core::{
    elastic_ngram_paged_sha::{hex, parse_hex, Sha256},
    sparse_gradient_atlas::SparseConfig,
    sparse_mor_anf_recurrent::{HybridWindowCounts, MorAnfRecurrent},
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
const MATCHED_TRAIN_TARGET: u64 = 64 * 1024;
const MATCHED_DEV_TARGET: u64 = 32 * 1024;
const MATCHED_WINDOW: usize = 128;
const MATCHED_SAMPLE_MAX: usize = 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PilotMode {
    LegacyRtrl,
    LegacyBptt,
    MatchedRtrl,
    MatchedBptt,
    MatchedHybrid,
}

impl PilotMode {
    fn is_bptt(self) -> bool {
        matches!(self, Self::LegacyBptt | Self::MatchedBptt)
    }

    fn is_matched(self) -> bool {
        matches!(
            self,
            Self::MatchedRtrl | Self::MatchedBptt | Self::MatchedHybrid
        )
    }
}

fn sha(path: &Path) -> Result<String> {
    let mut reader = BufReader::with_capacity(1024 * 1024, File::open(path)?);
    let mut state = Sha256::default();
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        state.update(&buf[..n]);
    }
    Ok(hex(&state.finish()))
}

fn execute(
    corpus: &Path,
    provenance: &Path,
    expected_sha: &str,
    output: &Path,
    mode: PilotMode,
) -> Result<()> {
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
    if digest != expected_sha {
        return Err("source changed since admission".into());
    }
    let bptt = mode.is_bptt();
    let matched = mode.is_matched();
    let config = SparseConfig {
        pages: if bptt || matched { 48_829 } else { 4096 },
        max_cached_pages: 32,
        learning_rate: 0.20,
    };
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
    let train_target = if matched {
        MATCHED_TRAIN_TARGET
    } else if bptt {
        12 * 1024
    } else {
        TRAIN_TARGET
    };
    let dev_target = if matched {
        MATCHED_DEV_TARGET
    } else if bptt {
        8 * 1024
    } else {
        DEV_TARGET
    };
    let mut train_fingerprint = Sha256::default();
    let mut dev_fingerprint = Sha256::default();
    let mut hybrid_counts = HybridWindowCounts::default();
    for line in lines.take(1000) {
        if train_bytes >= train_target && dev_bytes >= dev_target {
            break;
        }
        let line = line?;
        let cols: Vec<_> = line.split('\t').collect();
        if cols.len() != 8
            || cols[6].is_empty()
            || !matches!(cols[4], "CC0" | "PD_CLAIM" | "CC_BY_ATTRIBUTION")
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
        if separator != [b'\n'] {
            return Err("corrupt document boundary".into());
        }
        let expected = parse_hex(cols[2]).ok_or("bad SHA ledger")?;
        let mut h = Sha256::default();
        h.update(&bytes);
        if h.finish() != expected || !hashes.insert(expected) {
            return Err("duplicate or altered SEC document".into());
        }
        if length > DOCUMENT_MAX {
            continue;
        }
        // The BPTT arm intentionally limits each verified source report to a
        // small document prefix. Report consumed bytes, not original file size.
        let sample = if bptt || matched {
            &bytes[..bytes.len().min(MATCHED_SAMPLE_MAX)]
        } else {
            &bytes
        };
        if train_bytes < train_target {
            let (loss, events) = if mode == PilotMode::MatchedHybrid {
                let (loss, events, counts) =
                    model.train_document_hybrid_windowed(sample, MATCHED_WINDOW)?;
                accumulate_hybrid(&mut hybrid_counts, counts)?;
                (loss, events)
            } else if matched && bptt {
                model.train_document_bptt(sample, MATCHED_WINDOW)?
            } else if matched {
                model.train_document_rtrl_windowed(sample, MATCHED_WINDOW)?
            } else if bptt {
                model.train_document_bptt(sample, MATCHED_WINDOW)?
            } else {
                model.train_document(sample)?
            };
            if matched {
                append_selection_fingerprint(&mut train_fingerprint, &expected, sample);
            }
            train_docs += 1;
            train_bytes += sample.len() as u64;
            train_events += events;
            train_loss += loss;
        } else if dev_bytes < dev_target {
            let (loss, events) = if matched {
                model.evaluate_document_windowed(sample, MATCHED_WINDOW)?
            } else {
                model.evaluate_document(sample)?
            };
            if matched {
                append_selection_fingerprint(&mut dev_fingerprint, &expected, sample);
            }
            dev_docs += 1;
            dev_bytes += sample.len() as u64;
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
    if mode == PilotMode::MatchedHybrid
        && (hybrid_counts.rtrl_windows == 0
            || hybrid_counts.bptt_windows == 0
            || hybrid_counts.rtrl_target_bits + hybrid_counts.bptt_target_bits != train_events * 8)
    {
        return Err("hybrid did not execute both update methods".into());
    }
    model.checkpoint(output)?;
    if mode == PilotMode::MatchedHybrid {
        let mut manifest = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("hybrid_schedule.tsv"))?;
        writeln!(manifest, "version\trtrl_windows\tbptt_windows\trtrl_target_bits\tbptt_target_bits\twindow\trtrl_per_cycle\tcycle_windows")?;
        writeln!(
            manifest,
            "HYBRID_3R_1B_V1\t{}\t{}\t{}\t{}\t{MATCHED_WINDOW}\t3\t4",
            hybrid_counts.rtrl_windows,
            hybrid_counts.bptt_windows,
            hybrid_counts.rtrl_target_bits,
            hybrid_counts.bptt_target_bits
        )?;
        manifest.sync_all()?;
    }
    let architecture = match mode {
        PilotMode::LegacyRtrl => "sml_mor_anf_rtrl_v1",
        PilotMode::LegacyBptt => "sml_mor_anf_paged_bptt_v1",
        PilotMode::MatchedRtrl => "sml_mor_anf_paged_rtrl_matched_v2",
        PilotMode::MatchedBptt => "sml_mor_anf_paged_bptt_matched_v2",
        PilotMode::MatchedHybrid => "sml_mor_anf_paged_hybrid_3r_1b_v1",
    };
    let metrics = format!(
        "architecture\tatlas_parameter_capacity\texpert_parameters_updated\texpert_updates\trouter_parameter_capacity\trouter_parameters_updated\trouter_updates\trecurrent_parameter_capacity\trecurrent_updates\ttrain_docs\ttrain_bytes\ttrain_events\ttrain_bpc\theldout_docs\theldout_bytes\theldout_events\theldout_bpc\tuniform_bpc\ttraining_elapsed_seconds\tg0\ttest\n{architecture}\t{}\t{}\t{}\t1024\t{}\t{}\t3\t{}\t{train_docs}\t{train_bytes}\t{train_events}\t{:.9}\t{dev_docs}\t{dev_bytes}\t{dev_events}\t{:.9}\t8.000000000\t{:.4}\tNOT_ADMITTED\tUNOPENED\n",
        config.total_parameters(), experts.distinct_updated_parameters, experts.parameter_updates,
        counters.unique_router_parameters, counters.router_parameter_updates,
        counters.recurrent_parameter_updates,
        train_loss / train_events as f64,
        dev_loss / dev_events as f64,
        start.elapsed().as_secs_f64()
    );
    let mut report = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("mor_anf_pilot.tsv"))?;
    report.write_all(metrics.as_bytes())?;
    report.sync_all()?;
    print!("{metrics}");
    if matched {
        let mut selection = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("paired_selection.tsv"))?;
        writeln!(
            selection,
            "version\ttrain_docs\ttrain_bytes\ttrain_events\ttrain_fingerprint\theldout_docs\theldout_bytes\theldout_events\theldout_fingerprint\twindow"
        )?;
        writeln!(
            selection,
            "PAIRED_V2\t{train_docs}\t{train_bytes}\t{train_events}\t{}\t{dev_docs}\t{dev_bytes}\t{dev_events}\t{}\t{MATCHED_WINDOW}",
            hex(&train_fingerprint.finish()),
            hex(&dev_fingerprint.finish())
        )?;
        selection.sync_all()?;
    }
    let marker = match mode {
        PilotMode::LegacyRtrl => "SML_MOR_ANF_RTRL_DONE",
        PilotMode::LegacyBptt => "SML_MOR_ANF_BPTT_DONE",
        PilotMode::MatchedRtrl => "SML_MOR_ANF_MATCHED_RTRL_DONE",
        PilotMode::MatchedBptt => "SML_MOR_ANF_MATCHED_BPTT_DONE",
        PilotMode::MatchedHybrid => "SML_MOR_ANF_MATCHED_HYBRID_DONE",
    };
    println!("{marker} source_sha256={digest} trainable_expert_capacity={} distinct_expert_weights={} router_updated={} recurrent_updates={} test=UNOPENED g0=NOT_ADMITTED",
        config.total_parameters(), experts.distinct_updated_parameters,
        counters.unique_router_parameters, counters.recurrent_parameter_updates);
    Ok(())
}

/// Ordered SHA commitment to the exact consumed prefixes, with no SEC text
/// in the artifact. Both arms must have identical fingerprints.
fn append_selection_fingerprint(hasher: &mut Sha256, source: &[u8; 32], sample: &[u8]) {
    let mut prefix_hash = Sha256::default();
    prefix_hash.update(sample);
    hasher.update(source);
    hasher.update(&prefix_hash.finish());
    hasher.update(&(sample.len() as u64).to_le_bytes());
}

fn accumulate_hybrid(total: &mut HybridWindowCounts, add: HybridWindowCounts) -> Result<()> {
    total.rtrl_windows = total
        .rtrl_windows
        .checked_add(add.rtrl_windows)
        .ok_or("hybrid RTRL window count overflow")?;
    total.bptt_windows = total
        .bptt_windows
        .checked_add(add.bptt_windows)
        .ok_or("hybrid BPTT window count overflow")?;
    total.rtrl_target_bits = total
        .rtrl_target_bits
        .checked_add(add.rtrl_target_bits)
        .ok_or("hybrid RTRL target count overflow")?;
    total.bptt_target_bits = total
        .bptt_target_bits
        .checked_add(add.bptt_target_bits)
        .ok_or("hybrid BPTT target count overflow")?;
    Ok(())
}

fn main() {
    let args: Vec<_> = env::args().collect();
    let result = match args.as_slice() {
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot" => execute(
            Path::new(corpus),
            Path::new(ledger),
            digest,
            Path::new(output),
            PilotMode::LegacyRtrl,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-bptt" => execute(
            Path::new(corpus),
            Path::new(ledger),
            digest,
            Path::new(output),
            PilotMode::LegacyBptt,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-paired-rtrl" => execute(
            Path::new(corpus),
            Path::new(ledger),
            digest,
            Path::new(output),
            PilotMode::MatchedRtrl,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-paired-bptt" => execute(
            Path::new(corpus),
            Path::new(ledger),
            digest,
            Path::new(output),
            PilotMode::MatchedBptt,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-paired-hybrid" => execute(
            Path::new(corpus),
            Path::new(ledger),
            digest,
            Path::new(output),
            PilotMode::MatchedHybrid,
        ),
        _ => Err(
            "usage: sml-mor-anf-rtrl-fit (--pilot|--pilot-bptt|--pilot-paired-rtrl|--pilot-paired-bptt|--pilot-paired-hybrid) CORPUS provenance SOURCE_SHA256 NEW_OUTPUT_DIR"
                .into(),
        ),
    };
    if let Err(error) = result {
        eprintln!("SML_MOR_ANF_RTRL_FAIL {error}");
        std::process::exit(1);
    }
}
