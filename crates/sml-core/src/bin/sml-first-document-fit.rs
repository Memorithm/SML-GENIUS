//! First bounded *exploratory* SML Boolean training on project-authored documents.
//! This trains eight existing LatentGate2 selectors. It is not GRU-v2 or a
//! language-model qualification; it never accesses a TEST corpus.
use sml_core::{
    elastic_ngram_paged_sha::{hex, sha256},
    train::{GateSample2, GateTrainingConfig, LatentGate2},
};
use std::{
    env,
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TRAIN: [&str; 3] = ["README.md", "BOOTSTRAP.md", "docs/ROADMAP.md"];
const DEV: [&str; 2] = ["docs/SML-0-SPEC.md", "docs/SML-ARCHITECTURE-DECISION-GATES.md"];
const EPOCHS: u32 = 24;
const LEARNING_RATE: f64 = 0.25;
const SAMPLES_PER_TRAIN_DOCUMENT: usize = 2_048;
const MAX_FILE_BYTES: u64 = 256 * 1024;
const MIN_DOC_BYTES: usize = 128;

fn read_doc(root: &Path, rel: &str) -> Result<Vec<u8>> {
    let path = root.join(rel);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink()
        || metadata.len() > MAX_FILE_BYTES || metadata.len() < MIN_DOC_BYTES as u64
    {
        return Err(format!("invalid project-owned source file: {rel}").into());
    }
    let bytes = fs::read(&path)?;
    if bytes.len() as u64 != metadata.len() || !bytes.iter().all(u8::is_ascii) && std::str::from_utf8(&bytes).is_err() {
        return Err(format!("invalid or changed source: {rel}").into());
    }
    Ok(bytes)
}

fn samples_for_bit(bytes: &[u8], bit: u8, limit: usize) -> Vec<GateSample2> {
    if bytes.len() < 3 {
        return Vec::new();
    }
    let step = ((bytes.len() - 2) / limit.max(1)).max(1);
    (2..bytes.len())
        .step_by(step)
        .take(limit)
        .map(|i| GateSample2 {
            left: (bytes[i - 2] >> bit) & 1 != 0,
            right: (bytes[i - 1] >> bit) & 1 != 0,
            target: (bytes[i] >> bit) & 1 != 0,
        })
        .collect()
}

fn loss_bpc(gates: &[LatentGate2; 8], documents: &[Vec<u8>]) -> Result<(f64, usize)> {
    let mut bits = 0_f64;
    let mut tokens = 0_usize;
    for doc in documents {
        for i in 2..doc.len() {
            for (bit, gate) in gates.iter().enumerate() {
                let p = gate.soft_probability(
                    (doc[i - 2] >> bit) & 1 != 0,
                    (doc[i - 1] >> bit) & 1 != 0,
                )?.clamp(1.0e-9, 1.0 - 1.0e-9);
                let target = (doc[i] >> bit) & 1 != 0;
                bits -= if target { p } else { 1.0 - p }.log2();
            }
            tokens += 1;
        }
    }
    if tokens == 0 || !bits.is_finite() {
        return Err("no valid score samples".into());
    }
    Ok((bits / tokens as f64, tokens))
}

fn train_model(train: &[Vec<u8>], dev: &[Vec<u8>]) -> Result<(String, String)> {
    let mut gates: [LatentGate2; 8] = std::array::from_fn(|_| LatentGate2::default());
    let (initial_train_bpc, _) = loss_bpc(&gates, train)?;
    let (initial_dev_bpc, dev_targets) = loss_bpc(&gates, dev)?;
    let mut weights = String::from("bit\thard_truth_table\tlatent_logits_16\n");
    let mut report = format!(
        "phase\ttrain_bpc\tdev_bpc\tdev_target_bytes\tmodel_kind\ttrain_domains\ttest_status\n\
         before\t{initial_train_bpc:.9}\t{initial_dev_bpc:.9}\t{dev_targets}\t8x_LatentGate2\tPROJECT_DOCS\tNOT_OPENED\n"
    );
    let mut total_training_samples = 0_usize;
    for (bit, gate) in gates.iter_mut().enumerate() {
        let mut samples = Vec::new();
        for doc in train {
            samples.extend(samples_for_bit(
                doc,
                bit as u8,
                SAMPLES_PER_TRAIN_DOCUMENT,
            ));
        }
        if samples.len() < 1_000 {
            return Err("too few document-derived samples".into());
        }
        let metrics = gate.train_samples(
            &samples,
            GateTrainingConfig {
                epochs: EPOCHS,
                learning_rate: LEARNING_RATE,
                epsilon: 1.0e-6,
            },
        )?;
        if !metrics.initial_loss.is_finite() || !metrics.final_loss.is_finite()
            || metrics.final_loss > metrics.initial_loss
        {
            return Err("training failed to reduce in-sample objective".into());
        }
        total_training_samples += samples.len();
        let values = gate.logits().iter()
            .map(|value| format!("{value:.17e}"))
            .collect::<Vec<_>>().join(",");
        weights.push_str(&format!(
            "{bit}\t{}\t{values}\n",
            gate.hard_table()?.code(),
        ));
    }
    let (trained_bpc, _) = loss_bpc(&gates, train)?;
    let (dev_bpc, trained_dev_targets) = loss_bpc(&gates, dev)?;
    if !trained_bpc.is_finite() || !dev_bpc.is_finite()
        || trained_bpc > initial_train_bpc || dev_targets != trained_dev_targets
        || total_training_samples > TRAIN.len() * SAMPLES_PER_TRAIN_DOCUMENT * 8
    {
        return Err("invalid training or scoring counters".into());
    }
    report.push_str(&format!(
        "after\t{trained_bpc:.9}\t{dev_bpc:.9}\t{dev_targets}\t8x_LatentGate2\tPROJECT_DOCS\tNOT_OPENED\n"
    ));
    report.push_str(&format!(
        "# epochs={EPOCHS} learning_rate={LEARNING_RATE} \
         samples_per_gate_max={} total_bit_samples={total_training_samples} \
         training_scope=EXPLORATORY_NOT_GRU_V2 G0_admitted=false\n",
        TRAIN.len() * SAMPLES_PER_TRAIN_DOCUMENT,
    ));
    Ok((report, weights))
}

fn run(root: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        return Err("output path must not exist".into());
    }
    let mut train = Vec::new();
    let mut dev = Vec::new();
    let mut provenance = String::from("role\trepository_relative_path\tsize_bytes\tsha256_raw\n");
    for (role, paths) in [("TRAIN", TRAIN.as_slice()), ("DEV", DEV.as_slice())] {
        for rel in paths {
            let bytes = read_doc(root, rel)?;
            provenance.push_str(&format!(
                "{role}\t{rel}\t{}\t{}\n",
                bytes.len(), hex(&sha256(&bytes)),
            ));
            if role == "TRAIN" { train.push(bytes); } else { dev.push(bytes); }
        }
    }
    fs::create_dir(dest)?;
    let (report, weights) = train_model(&train, &dev)?;
    for (name, content) in [
        ("provenance.tsv", provenance),
        ("first_fit.tsv", report),
        ("weights.tsv", weights),
    ] {
        let path = dest.join(name);
        let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        println!("FIRST_FIT_FILE name={name} bytes={} sha256={}",content.len(),hex(&sha256(content.as_bytes())));
        if name == "first_fit.tsv" {
            print!("{content}");
        }
    }
    println!("FIRST_FIT_DONE data=PROJECT_AUTHORED_DOCUMENTS test=UNOPENED scope=EXPLORATORY_NOT_GRU_V2 G0_admitted=false");
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: sml-first-document-fit SOURCE_CHECKOUT NEW_OUTPUT_DIR");
        std::process::exit(2);
    }
    if let Err(e) = run(Path::new(&args[1]), Path::new(&args[2])) {
        eprintln!("FIRST_FIT_REJECTED {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn train_loss_decreases_on_document_like_samples() {
        let train = vec![b"source code source code source code\n".repeat(100)];
        let dev = vec![b"source code code source code source\n".repeat(60)];
        let (metrics, weights) = train_model(&train, &dev).unwrap();
        let records: Vec<_> = metrics.lines().collect();
        assert!(records[1].starts_with("before\t"));
        assert!(records[2].starts_with("after\t"));
        assert_eq!(weights.lines().count(), 9);
        assert!(metrics.contains("G0_admitted=false"));
    }

    #[test]
    fn tiny_documents_fail_training() {
        assert!(train_model(&[b"abc".to_vec()], &[b"xy".to_vec()]).is_err());
        assert!(samples_for_bit(b"xy", 0, 8).is_empty());
    }
}
