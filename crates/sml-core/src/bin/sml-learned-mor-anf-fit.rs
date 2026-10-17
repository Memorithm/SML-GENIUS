//! Real SEC-document pilot for the 21-scalar learned MOR+Boolean+recurrent core.
//! This deliberately trains only a tiny bounded prefix, not 25B/50M.
//! All model weights are saved as explicit F64LE; no raw filings are published.
use sml_core::{
    elastic_ngram_paged_sha::{hex, parse_hex, Sha256},
    learned_mor_anf_recurrent::RecurrentMorAnf,
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
const TRAIN_DOCS: u64 = 4;
const HOLDOUT_DOCS: u64 = 2;
const BYTES_PER_DOC: usize = 32 * 1024;
const SEQUENCE_STEPS: usize = 512;

fn digest(path: &Path) -> Result<String> {
    let mut input = BufReader::with_capacity(1024 * 1024, File::open(path)?);
    let mut sum = Sha256::default();
    let mut block = [0u8; 1024 * 1024];
    loop {
        let n = input.read(&mut block)?;
        if n == 0 { break; }
        sum.update(&block[..n]);
    }
    Ok(hex(&sum.finish()))
}

fn sequences<F>(bytes: &[u8], mut evaluate: F) -> Result<(f64, u64)>
where
    F: FnMut(&[(bool, bool)], &[bool]) -> Result<f64>,
{
    let mut loss_nats = 0.0;
    let mut events = 0u64;
    for bit in 0..8 {
        let mut inputs = Vec::with_capacity(SEQUENCE_STEPS);
        let mut labels = Vec::with_capacity(SEQUENCE_STEPS);
        for block in bytes.windows(3) {
            inputs.push((block[0] & (1 << bit) != 0, block[1] & (1 << bit) != 0));
            labels.push(block[2] & (1 << bit) != 0);
            if inputs.len() == SEQUENCE_STEPS {
                loss_nats += evaluate(&inputs, &labels)? * inputs.len() as f64;
                events += inputs.len() as u64;
                inputs.clear();
                labels.clear();
            }
        }
        if !inputs.is_empty() {
            loss_nats += evaluate(&inputs, &labels)? * inputs.len() as f64;
            events += inputs.len() as u64;
        }
    }
    if !loss_nats.is_finite() { return Err("MOR ANF sequence loss not finite".into()); }
    Ok((loss_nats, events))
}

fn run(corpus: &Path, ledger: &Path, required_sha: &str, output: &Path) -> Result<()> {
    if parse_hex(required_sha).is_none() || corpus == ledger || output.exists() {
        return Err("bad input hash or output exists".into());
    }
    for path in [corpus, ledger] {
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("input must be an ordinary non-symlink file".into());
        }
    }
    if digest(corpus)? != required_sha { return Err("whole SEC corpus SHA mismatch".into()); }
    let mut source = BufReader::new(File::open(corpus)?);
    let mut manifest = BufReader::new(File::open(ledger)?).lines();
    if manifest.next().transpose()?.as_deref() != Some(HEADER) {
        return Err("incorrect provenance schema".into());
    }
    let mut seen = HashSet::new();
    let mut model = RecurrentMorAnf::default();
    let mut train_docs = 0u64;
    let mut holdout_docs = 0u64;
    let mut train_loss = 0.0;
    let mut train_events = 0u64;
    let mut holdout = Vec::new();
    for line in manifest {
        if train_docs >= TRAIN_DOCS && holdout_docs >= HOLDOUT_DOCS { break; }
        let line = line?;
        let columns: Vec<_> = line.split('\t').collect();
        if columns.len() != 8 || columns[5].is_empty() || columns[6].is_empty()
            || !matches!(columns[4], "CC0" | "PD_CLAIM" | "CC_BY_ATTRIBUTION")
        {
            return Err("invalid or disallowed claimed rights in SEC ledger".into());
        }
        let len: usize = columns[3].parse()?;
        if !(128..=8 * 1024 * 1024).contains(&len) { return Err("SEC document size out of bounds".into()); }
        let mut bytes = vec![0; len];
        source.read_exact(&mut bytes)?;
        let mut boundary = [0; 1];
        source.read_exact(&mut boundary)?;
        if boundary != [b'\n'] { return Err("invalid SEC document separator".into()); }
        let expected = parse_hex(columns[2]).ok_or("incorrect document SHA")?;
        let mut hasher = Sha256::default();
        hasher.update(&bytes);
        if hasher.finish() != expected || !seen.insert(expected) {
            return Err("SEC document SHA mismatch or duplicate".into());
        }
        let len = bytes.len().min(BYTES_PER_DOC);
        if len < 128 { continue; }
        bytes.truncate(len);
        if train_docs < TRAIN_DOCS {
            let (loss, events) = sequences(&bytes, |x, y| {
                Ok(model.train_sequence(x, y, 0.12)?)
            })?;
            train_loss += loss;
            train_events += events;
            train_docs += 1;
        } else if holdout_docs < HOLDOUT_DOCS {
            holdout.push(bytes);
            holdout_docs += 1;
        }
    }
    if train_docs != TRAIN_DOCS || holdout_docs != HOLDOUT_DOCS || train_events < 1000 {
        return Err("insufficient separate SEC training and held-out filings".into());
    }
    let initial = RecurrentMorAnf::default();
    let mut initial_loss = 0.0;
    let mut held_loss = 0.0;
    let mut held_events = 0u64;
    for bytes in &holdout {
        let (a, n) = sequences(bytes, |x, y| Ok(initial.loss(x, y)?))?;
        let (b, m) = sequences(bytes, |x, y| Ok(model.loss(x, y)?))?;
        if n != m { return Err("holdout accounting mismatch".into()); }
        initial_loss += a;
        held_loss += b;
        held_events += m;
    }
    if !held_loss.is_finite() || held_events < 1000 {
        return Err("invalid unseen SEC sample".into());
    }
    fs::create_dir(output)?;
    let mut weights = OpenOptions::new().create_new(true).write(true)
        .open(output.join("weights.f64le"))?;
    weights.write_all(&model.weights_le_bytes())?;
    weights.sync_all()?;
    let mut evidence = OpenOptions::new().create_new(true).write(true)
        .open(output.join("metrics.tsv"))?;
    let metrics = format!(
        "model\tparameters\ttrain_docs\ttrain_supervised_bits\ttrain_online_bpb\tholdout_docs\tholdout_supervised_bits\tholdout_initial_bpb\tholdout_trained_bpb\tuniform_bpb\trights\ttest\tsource_sha256\nmor_anf_recurrent_bptt_v1\t{}\t{train_docs}\t{train_events}\t{:.9}\t{holdout_docs}\t{held_events}\t{:.9}\t{:.9}\t1.000000000\tNOT_ADMITTED\tUNOPENED\t{required_sha}\n",
        RecurrentMorAnf::PARAMETERS,
        train_loss / (train_events as f64 * std::f64::consts::LN_2),
        initial_loss / (held_events as f64 * std::f64::consts::LN_2),
        held_loss / (held_events as f64 * std::f64::consts::LN_2),
    );
    evidence.write_all(metrics.as_bytes())?;
    evidence.sync_all()?;
    print!("{metrics}");
    println!("SML_MOR_ANF_PILOT_DONE parameters={} doc_split=TRAIN_THEN_HELDOUT rights=NOT_ADMITTED TEST=UNOPENED", RecurrentMorAnf::PARAMETERS);
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let result = match args.as_slice() {
        [_, flag, source, manifest, sha, dest] if flag == "--sec-pilot" =>
            run(Path::new(source), Path::new(manifest), sha, Path::new(dest)),
        _ => Err("usage: sml-learned-mor-anf-fit --sec-pilot CORPUS.utf8 provenance.tsv SOURCE_SHA OUT".into()),
    };
    if let Err(e) = result {
        eprintln!("SML_MOR_ANF_PILOT_FAIL {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recurrence_streaming_positions_are_not_concatenated_across_bits() {
        let input = b"abcdefghijk";
        let (_, n) = sequences(input, |x, y| {
            assert_eq!(x.len(), y.len());
            assert!(x.len() <= SEQUENCE_STEPS);
            Ok(1.0)
        }).unwrap();
        assert_eq!(n, (input.len() - 2) as u64 * 8);
    }
}
