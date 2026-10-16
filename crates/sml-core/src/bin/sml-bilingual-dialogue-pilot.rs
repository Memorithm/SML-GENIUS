//! Experimental FR/EN assistant-target fit using the existing Rust 50M sparse atlas.
//! Two-byte hash routing is NOT instruction following; emitted reports say so.
//! TSV: split<TAB>lang<TAB>tree_id<TAB>prompt<TAB>answer (one line per pair).
use sml_core::{
    elastic_ngram_paged_sha::{hex, parse_hex, Sha256},
    sparse_gradient_atlas::{SparseConfig, SparseGradientAtlas},
};
use std::{
    collections::{HashMap, HashSet},
    env,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const HEADER: &str = "split\tlang\ttree_id\tprompt\tanswer";
const MAX_BYTES: u64 = 24 * 1024 * 1024;
const MAX_LINES: usize = 20_000;
const PAGES: u64 = 48_829;
const WINDOW: usize = 4096;

#[derive(Debug)]
struct Pair {
    language: usize,
    bytes: Vec<u8>,
    answer_start: usize,
}

fn parse_line(line: &str) -> Result<(&str, &str, &str, &str, &str)> {
    let parts: Vec<_> = line.split('\t').collect();
    if parts.len() != 5 {
        return Err("invalid dialogue TSV columns".into());
    }
    let (split, lang, tree, prompt, answer) = (parts[0], parts[1], parts[2], parts[3], parts[4]);
    if !matches!(split, "train" | "dev") || !matches!(lang, "fr" | "en") {
        return Err("invalid dialogue split/language".into());
    }
    if tree.len() < 8
        || tree.len() > 80
        || !tree.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
    {
        return Err("invalid conversation tree identifier".into());
    }
    for text in [prompt, answer] {
        if text.len() < 4
            || text.len() > 6000
            || text
                .bytes()
                .any(|b| b == 0 || b == b'\n' || b == b'\r' || b == b'\t')
        {
            return Err("invalid dialogue turn".into());
        }
    }
    Ok((split, lang, tree, prompt, answer))
}

fn digest_file(path: &Path) -> Result<String> {
    let mut read = BufReader::new(File::open(path)?);
    let mut hash = Sha256::default();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = read.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hex(&hash.finish()))
}

fn intake(source: &Path, expected: &str) -> Result<(Vec<Pair>, Vec<Pair>, [u64; 4])> {
    let info = fs::symlink_metadata(source)?;
    if !info.is_file() || info.file_type().is_symlink() || info.len() > MAX_BYTES {
        return Err("unqualified dialogue source file".into());
    }
    if parse_hex(expected).is_none() || digest_file(source)? != expected {
        return Err("dialogue file SHA-256 mismatch".into());
    }
    let mut lines = BufReader::new(File::open(source)?).lines();
    if lines.next().transpose()?.as_deref() != Some(HEADER) {
        return Err("unrecognized dialogue data schema".into());
    }
    let mut train = Vec::new();
    let mut dev = Vec::new();
    let mut trees = HashMap::<String, String>::new();
    let mut pairs = HashSet::<(String, String, String)>::new();
    let mut counts = [0u64; 4];
    let mut dev_started = false;
    for (index, line) in lines.enumerate() {
        if index >= MAX_LINES {
            return Err("dialogue row ceiling exceeded".into());
        }
        let line = line?;
        let (split, lang, tree, prompt, answer) = parse_line(&line)?;
        if split == "train" && dev_started {
            return Err("training rows after DEV block".into());
        }
        if split == "dev" {
            dev_started = true;
        }
        if let Some(original) = trees.insert(tree.into(), split.into()) {
            if original != split {
                return Err("conversation tree overlaps TRAIN and DEV".into());
            }
        }
        if !pairs.insert((lang.into(), prompt.into(), answer.into())) {
            return Err("exact dialogue pair duplicate".into());
        }
        let idx = match (split, lang) {
            ("train", "fr") => 0,
            ("train", "en") => 1,
            ("dev", "fr") => 2,
            ("dev", "en") => 3,
            _ => return Err("unexpected split/language".into()),
        };
        counts[idx] += 1;
        let prefix = format!("[LANG={lang}]\n[USER]\n{prompt}\n[ASSISTANT]\n");
        let mut bytes = prefix.as_bytes().to_vec();
        bytes.extend_from_slice(answer.as_bytes());
        bytes.extend_from_slice(b"\n[END]\n");
        let pair = Pair {
            language: usize::from(lang == "en"),
            bytes,
            answer_start: prefix.len(),
        };
        if split == "train" {
            train.push(pair);
        } else {
            dev.push(pair);
        }
    }
    if counts.iter().any(|&n| n < 8) {
        return Err("minimum FR/EN TRAIN/DEV coverage missing".into());
    }
    Ok((train, dev, counts))
}

fn train(source: &Path, expected: &str, output: &Path) -> Result<()> {
    if output.exists() {
        return Err("output already exists, no implicit overwrite".into());
    }
    // Complete and validate both splits before allocating any model file.
    let (train, dev, counts) = intake(source, expected)?;
    let config = SparseConfig {
        pages: PAGES,
        max_cached_pages: 32,
        learning_rate: 0.15,
    };
    let mut model = SparseGradientAtlas::create(output, config)?;
    let mut train_loss = [0.0f64; 2];
    let mut train_events = [0u64; 2];
    for pair in &train {
        let (loss, n) = model.train_assistant_bytes(&pair.bytes, pair.answer_start, WINDOW)?;
        train_loss[pair.language] += loss;
        train_events[pair.language] += n;
    }
    let mut dev_loss = [0.0f64; 2];
    let mut dev_events = [0u64; 2];
    for pair in &dev {
        let (loss, n) = model.evaluate_assistant_bytes(&pair.bytes, pair.answer_start, WINDOW)?;
        dev_loss[pair.language] += loss;
        dev_events[pair.language] += n;
    }
    if train_events.contains(&0) || dev_events.contains(&0) {
        return Err("no answer tokens scored in FR/EN".into());
    }
    model.checkpoint(output)?;
    model.export_q4(&output.join("weights.q4"))?;
    let updated = model.counters().distinct_updated_parameters;
    if updated == 0 {
        return Err("no physical parameters updated".into());
    }
    let mut report = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("dialogue_pilot.tsv"))?;
    writeln!(
        report,
        "lang\ttrain_pairs\tdev_pairs\ttrain_target_bytes\tdev_target_bytes\ttrain_bits_per_byte\tdev_bits_per_byte\tparams_capacity\tparams_updated_unique\tcorpus_sha256\tg0\ttest\tconversation_ready"
    )?;
    for (lang, i) in [("fr", 0usize), ("en", 1usize)] {
        writeln!(
            report,
            "{lang}\t{}\t{}\t{}\t{}\t{:.9}\t{:.9}\t{}\t{}\t{expected}\tNOT_ADMITTED\tUNOPENED\tNO",
            counts[i],
            counts[i + 2],
            train_events[i],
            dev_events[i],
            train_loss[i] / train_events[i] as f64,
            dev_loss[i] / dev_events[i] as f64,
            config.total_parameters(),
            updated
        )?;
        println!(
            "DIALOGUE_PILOT lang={lang} train_pairs={} dev_pairs={} dev_bpc={:.6} updated_unique={} conversation_ready=false",
            counts[i],
            counts[i + 2],
            dev_loss[i] / dev_events[i] as f64,
            updated
        );
    }
    report.sync_all()?;
    println!("SML_BILINGUAL_PILOT_DONE model=two_byte_sparse_gradient_50m corpus_sha256={expected} G0=NOT_ADMITTED TEST=UNOPENED NOT_CHAT_READY");
    Ok(())
}

fn main() {
    let args: Vec<_> = env::args().collect();
    let result = if let [_, source, sha, output] = args.as_slice() {
        train(Path::new(source), sha, Path::new(output))
    } else {
        Err("usage: sml-bilingual-dialogue-pilot FR_EN_TRAIN_DEV.tsv SHA256 NEW_OUTPUT_DIR".into())
    };
    if let Err(err) = result {
        eprintln!("SML_BILINGUAL_PILOT_FAIL {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::parse_line;

    #[test]
    fn rejects_unknown_languages_and_bad_fields() {
        assert!(parse_line("train\tfr\t12345678\tBonjour !\tSalut !").is_ok());
        assert!(parse_line("dev\ten\t12345678\tHello there\tHi there").is_ok());
        assert!(parse_line("test\tfr\t12345678\tBonjour !\tSalut !").is_err());
        assert!(parse_line("train\tde\t12345678\tHello there\tHi there").is_err());
        assert!(parse_line("train\tfr\tinvalid tree\tBonjour !\tSalut !").is_err());
        assert!(parse_line("train\tfr\t12345678\tHi\tAnswer").is_err());
    }
}
