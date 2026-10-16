//! Sequential Boolean-bank trainer for a provenance-ledger corpus up to 32 GiB.
//! G0 NOT admitted; this is a counting baseline, not a GRU/ANF/25B network.
use super::{scan, Bank, Corpus, MIN_DEV};
use sml_core::elastic_ngram_paged_sha::{hex, parse_hex, sha256, Sha256};
use std::{
    collections::HashSet,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const CHUNK: usize = 1024 * 1024;
const MAX_TRAIN: u64 = 32 * 1024 * 1024 * 1024;
const MIN_TRAIN: u64 = 8 * 1024 * 1024;
const MAX_DOCUMENT: u64 = 8 * 1024 * 1024;
const MAX_DOCUMENTS: u64 = 20_000_000;
const CHECKPOINT_EVERY: u64 = 256;
const VERSION: &str = "SML_BOOLEAN_STREAM_V1";
const HEADER: &str =
    "shard\trow\tsha256\tbytes\tlicense_class\tlicense_source\tidentifier\tcreator_title";

struct Entry {
    hash: [u8; 32],
    bytes: u64,
}

fn entry(line: &str) -> Result<Entry> {
    if line.len() > 262_144 {
        return Err("oversized provenance record".into());
    }
    let fields: Vec<&str> = line.trim_end_matches('\n').split('\t').collect();
    if fields.len() != 8 || fields[0].is_empty() || fields[1].parse::<u64>().is_err() {
        return Err("invalid provenance record structure".into());
    }
    if !matches!(fields[4], "PD_CLAIM" | "CC0" | "CC_BY_ATTRIBUTION") {
        return Err("rights class is not allowed for exploratory TRAIN".into());
    }
    if fields[5].is_empty() || fields[6].is_empty() {
        return Err("missing license or source identifier".into());
    }
    let bytes: u64 = fields[3].parse()?;
    if !(128..=MAX_DOCUMENT).contains(&bytes) {
        return Err("invalid document length".into());
    }
    let hash = parse_hex(fields[2]).ok_or("invalid document SHA-256")?;
    Ok(Entry { hash, bytes })
}

// Never allocate an unbounded line from an untrusted future (25-GiB) ledger.
fn next_record(reader: &mut BufReader<File>) -> Result<Option<String>> {
    let mut line = Vec::<u8>::new();
    loop {
        let bytes = reader.fill_buf()?;
        if bytes.is_empty() {
            if line.is_empty() {
                return Ok(None);
            }
            return Err("unterminated provenance record".into());
        }
        let consumed = bytes
            .iter()
            .position(|b| *b == b'\n')
            .map_or(bytes.len(), |p| p + 1);
        if line
            .len()
            .checked_add(consumed)
            .ok_or("line length overflow")?
            > 262_144
        {
            return Err("provenance line exceeds bounded memory policy".into());
        }
        line.extend_from_slice(&bytes[..consumed]);
        reader.consume(consumed);
        if line.last() == Some(&b'\n') {
            break;
        }
    }
    Ok(Some(String::from_utf8(line)?))
}

fn checked_manifest(path: &Path) -> Result<(u64, u64, u64, [u8; 32])> {
    let digest = hash_file(path)?;
    let mut lines = BufReader::with_capacity(8192, File::open(path)?);
    if next_record(&mut lines)?
        .as_deref()
        .map(|line| line.trim_end_matches('\n'))
        != Some(HEADER)
    {
        return Err("unrecognized exact provenance header".into());
    }
    let mut docs = 0u64;
    let mut payload = 0u64;
    let mut physical = 0u64;
    while let Some(line) = next_record(&mut lines)? {
        let row = entry(&line)?;
        docs = docs.checked_add(1).ok_or("document counter overflow")?;
        payload = payload.checked_add(row.bytes).ok_or("payload overflow")?;
        physical = physical
            .checked_add(row.bytes + 1)
            .ok_or("physical size overflow")?;
        if docs > MAX_DOCUMENTS || payload > MAX_TRAIN || physical > MAX_TRAIN + MAX_DOCUMENTS {
            return Err("32-GiB source capacity exceeded".into());
        }
    }
    if docs == 0 || payload < MIN_TRAIN {
        return Err("insufficient TRAIN text".into());
    }
    Ok((docs, payload, physical, digest))
}

fn hash_file(path: &Path) -> Result<[u8; 32]> {
    if path.symlink_metadata()?.file_type().is_symlink() {
        return Err("symlinked input forbidden".into());
    }
    let mut file = BufReader::with_capacity(CHUNK, File::open(path)?);
    let mut hash = Sha256::default();
    let mut chunk = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        hash.update(&chunk[..n]);
    }
    Ok(hash.finish())
}

fn dev_fingerprints(dev: &Path) -> Result<HashSet<[u8; 32]>> {
    let mut fingerprints = HashSet::new();
    for relative in super::gather(dev)? {
        let path = dev.join(relative);
        if path.symlink_metadata()?.file_type().is_symlink() {
            return Err("symlinked DEV file forbidden".into());
        }
        fingerprints.insert(sha256(&fs::read(path)?));
    }
    Ok(fingerprints)
}

#[derive(Default)]
struct State {
    docs: u64,
    offset: u64,
    payload: u64,
    events: u64,
}

fn checkpoint(
    out: &Path,
    corpus_hash: &[u8; 32],
    manifest_hash: &[u8; 32],
    state: &State,
    bank: &Bank,
) -> Result<()> {
    let temp = out.join("checkpoint.v1.partial");
    if temp.exists() {
        fs::remove_file(&temp)?;
    }
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    writeln!(f, "{VERSION}")?;
    writeln!(f, "{}", hex(corpus_hash))?;
    writeln!(f, "{}", hex(manifest_hash))?;
    writeln!(
        f,
        "{}\t{}\t{}\t{}",
        state.docs, state.offset, state.payload, state.events
    )?;
    for counts in &bank.counts {
        writeln!(f, "{}\t{}", counts[0], counts[1])?;
    }
    writeln!(f, "END")?;
    f.sync_all()?;
    drop(f);
    fs::rename(temp, out.join("checkpoint.v1"))?;
    Ok(())
}

fn checkpoint_line<I: Iterator<Item = std::io::Result<String>>>(lines: &mut I) -> Result<String> {
    Ok(lines.next().ok_or("short checkpoint")??)
}

fn restore(
    out: &Path,
    corpus_hash: &[u8; 32],
    manifest_hash: &[u8; 32],
    bank: &mut Bank,
) -> Result<State> {
    let path = out.join("checkpoint.v1");
    if !path.exists() {
        return Ok(State::default());
    }
    if path.symlink_metadata()?.file_type().is_symlink() {
        return Err("checkpoint symlink forbidden".into());
    }
    let mut lines = BufReader::new(File::open(path)?).lines();
    if checkpoint_line(&mut lines)? != VERSION
        || checkpoint_line(&mut lines)? != hex(corpus_hash)
        || checkpoint_line(&mut lines)? != hex(manifest_hash)
    {
        return Err("checkpoint/source identity mismatch".into());
    }
    let stats = checkpoint_line(&mut lines)?;
    let values: Vec<u64> = stats
        .split('\t')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<_, _>>()?;
    if values.len() != 4 {
        return Err("invalid checkpoint counters".into());
    }
    for counts in &mut bank.counts {
        let line = checkpoint_line(&mut lines)?;
        let pair: Vec<u64> = line
            .split('\t')
            .map(str::parse::<u64>)
            .collect::<std::result::Result<_, _>>()?;
        if pair.len() != 2 {
            return Err("invalid checkpoint weight row".into());
        }
        *counts = [pair[0], pair[1]];
    }
    if checkpoint_line(&mut lines)? != "END" || lines.next().is_some() {
        return Err("invalid checkpoint terminator".into());
    }
    let state = State {
        docs: values[0],
        offset: values[1],
        payload: values[2],
        events: values[3],
    };
    let learned: u64 = bank
        .counts
        .iter()
        .try_fold(0u64, |total, item| {
            total.checked_add(item[0])?.checked_add(item[1])
        })
        .ok_or("checkpoint counter overflow")?;
    if learned != state.events.checked_mul(8).ok_or("event overflow")? {
        return Err("checkpoint bit totals mismatch".into());
    }
    Ok(state)
}

fn feed(bank: &mut Bank, tail: &mut [u8; 2], carry: &mut usize, input: &[u8]) -> Result<u64> {
    let mut combined = Vec::with_capacity(*carry + input.len());
    combined.extend_from_slice(&tail[..*carry]);
    combined.extend_from_slice(input);
    let n = bank.learn(&combined)?;
    *carry = combined.len().min(2);
    tail[..*carry].copy_from_slice(&combined[combined.len() - *carry..]);
    Ok(n)
}

fn one_document(file: &mut File, record: &Entry, bank: &mut Bank) -> Result<u64> {
    let mut digest = Sha256::default();
    let mut chunk = vec![0u8; CHUNK];
    let mut remaining = record.bytes;
    let mut tail = [0u8; 2];
    let mut carry = 0usize;
    let mut events = 0u64;
    while remaining != 0 {
        let n = usize::try_from(remaining.min(CHUNK as u64))?;
        file.read_exact(&mut chunk[..n])?;
        digest.update(&chunk[..n]);
        events = events
            .checked_add(feed(bank, &mut tail, &mut carry, &chunk[..n])?)
            .ok_or("event overflow")?;
        remaining -= n as u64;
    }
    if digest.finish() != record.hash {
        return Err("document SHA-256 differs from pinned provenance".into());
    }
    let mut separator = [0u8; 1];
    file.read_exact(&mut separator)?;
    if separator[0] != b'\n' {
        return Err("missing document separator".into());
    }
    if events != record.bytes - 2 {
        return Err("document event count differs from expected".into());
    }
    Ok(events)
}

fn write_artifact(out: &Path, name: &str, data: &[u8]) -> Result<()> {
    let path = out.join(name);
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(data)?;
    f.sync_all()?;
    println!(
        "BOOLEAN_STREAM_ARTIFACT name={name} bytes={} sha256={}",
        data.len(),
        hex(&sha256(data))
    );
    Ok(())
}

pub(super) fn run(
    corpus: &Path,
    provenance: &Path,
    expected_hex: &str,
    dev: &Path,
    out: &Path,
) -> Result<()> {
    let expected = parse_hex(expected_hex).ok_or("invalid expected corpus SHA-256")?;
    if corpus == provenance || corpus == dev || provenance == dev {
        return Err("TRAIN/manifest/DEV paths must differ".into());
    }
    if out.exists() && (!out.is_dir() || out.symlink_metadata()?.file_type().is_symlink()) {
        return Err("output must be a real directory".into());
    }
    if out.join("summary.tsv").exists() {
        return Err("training output already sealed; refusing overwrite".into());
    }
    if !out.exists() {
        fs::create_dir(out)?;
    }
    let (docs, payload, physical, manifest_hash) = checked_manifest(provenance)?;
    if fs::metadata(corpus)?.len() != physical {
        return Err("corpus physical length differs from ledger".into());
    }
    let actual_hash = hash_file(corpus)?;
    if actual_hash != expected {
        return Err("corpus identity does not match expected SHA-256".into());
    }
    let dev_hashes = dev_fingerprints(dev)?;
    let mut bank = Bank::new();
    let mut state = restore(out, &expected, &manifest_hash, &mut bank)?;
    if state.docs > docs || state.offset > physical || state.payload > payload {
        return Err("checkpoint outside corpus bounds".into());
    }
    let mut file = File::open(corpus)?;
    let mut lines = BufReader::with_capacity(8192, File::open(provenance)?);
    if next_record(&mut lines)?
        .as_deref()
        .map(|line| line.trim_end_matches('\n'))
        != Some(HEADER)
    {
        return Err("manifest changed".into());
    }
    let mut prefix_offset = 0u64;
    let mut prefix_bytes = 0u64;
    let mut prefix_events = 0u64;
    let mut resumed = false;
    let mut number = 0u64;
    while let Some(line) = next_record(&mut lines)? {
        let record = entry(&line)?;
        if dev_hashes.contains(&record.hash) {
            return Err("TRAIN/DEV exact document overlap rejected".into());
        }
        if number < state.docs {
            prefix_offset += record.bytes + 1;
            prefix_bytes += record.bytes;
            prefix_events += record.bytes - 2;
        } else {
            if !resumed {
                if state.offset != prefix_offset
                    || state.payload != prefix_bytes
                    || state.events != prefix_events
                {
                    return Err("checkpoint progress differs from manifest prefix".into());
                }
                file.seek(SeekFrom::Start(state.offset))?;
                resumed = true;
                println!(
                    "BOOLEAN_STREAM_RESUME documents={} offset={}",
                    state.docs, state.offset
                );
            }
            let n = one_document(&mut file, &record, &mut bank)?;
            state.docs = state.docs.checked_add(1).ok_or("document overflow")?;
            state.offset = state
                .offset
                .checked_add(record.bytes + 1)
                .ok_or("offset overflow")?;
            state.payload = state
                .payload
                .checked_add(record.bytes)
                .ok_or("payload overflow")?;
            state.events = state.events.checked_add(n).ok_or("event overflow")?;
            if state.docs % CHECKPOINT_EVERY == 0 {
                checkpoint(out, &expected, &manifest_hash, &state, &bank)?;
                println!(
                    "BOOLEAN_STREAM_CHECKPOINT docs={} bytes={} events={}",
                    state.docs, state.payload, state.events
                );
            }
        }
        number += 1;
    }
    if !resumed
        && (state.offset != prefix_offset
            || state.payload != prefix_bytes
            || state.events != prefix_events)
    {
        return Err("final checkpoint does not match complete manifest".into());
    }
    if state.docs != docs || state.offset != physical || state.payload != payload {
        return Err("TRAIN traversal differs from preflight".into());
    }
    checkpoint(out, &expected, &manifest_hash, &state, &bank)?;
    if bank.changed_slots() < 100 {
        return Err("insufficient Boolean model coverage".into());
    }
    let mut dev_corpus = Corpus::new();
    let dev_loss = scan(
        dev,
        "Memorithm/SML-GENIUS",
        "DEV",
        &mut dev_corpus,
        &mut bank,
        true,
    )?;
    if dev_corpus.bytes < MIN_DEV || dev_corpus.events == 0 {
        return Err("DEV has no sufficient events".into());
    }
    let bits = dev_loss / dev_corpus.events as f64;
    let summary = format!(
        "model\ttrain_docs\ttrain_bytes\ttrain_events\tmax_capacity_bytes\tdev_bytes\tdev_events\tlearned_slots\tdev_bpc\ttest\tg0\nboolean_context_8192_stream\t{}\t{}\t{}\t{MAX_TRAIN}\t{}\t{}\t{}\t{bits:.9}\tUNOPENED\tNOT_ADMITTED\n",
        state.docs, state.payload, state.events, dev_corpus.bytes, dev_corpus.events, bank.changed_slots()
    );
    let mut weights = String::from("slot\tzeros\tones\talpha\n");
    for (idx, counts) in bank.counts.iter().enumerate() {
        weights.push_str(&format!("{idx}\t{}\t{}\t1.0\n", counts[0], counts[1]));
    }
    let qualification = format!(
        "corpus_sha256\t{}\nmanifest_sha256\t{}\ntrain_documents\t{}\ntrain_payload_bytes\t{}\ncap_bytes\t{}\ntest\tUNOPENED\ng0\tNOT_ADMITTED\n",
        hex(&expected), hex(&manifest_hash), state.docs, state.payload, MAX_TRAIN
    );
    if !out.join("weights.tsv").exists() {
        write_artifact(out, "weights.tsv", weights.as_bytes())?;
    }
    if !out.join("qualification.tsv").exists() {
        write_artifact(out, "qualification.tsv", qualification.as_bytes())?;
    }
    write_artifact(out, "summary.tsv", summary.as_bytes())?;
    print!("{summary}");
    println!("BOOLEAN_STREAM_DONE scope=RESEARCH source=HF_STREAM train_payload_bytes={} test=UNOPENED G0_admitted=false", state.payload);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supports_25_gib_without_allocating_corpus() {
        let capacity = std::hint::black_box(MAX_TRAIN);
        assert!(capacity >= 25 * 1024 * 1024 * 1024);
        assert_eq!(super::super::SLOTS, 8192);
        assert_eq!(CHECKPOINT_EVERY, 256);
    }
    #[test]
    fn streaming_chunks_match_whole_document_training() {
        let data: Vec<u8> = (0..3001).map(|i| (i % 251) as u8).collect();
        let mut a = Bank::new();
        let mut b = Bank::new();
        assert_eq!(a.learn(&data).unwrap(), 2999);
        let mut tail = [0u8; 2];
        let mut count = 0;
        let mut events = 0u64;
        for piece in data.chunks(37) {
            events += feed(&mut b, &mut tail, &mut count, piece).unwrap();
        }
        assert_eq!(events, 2999);
        assert_eq!(a.counts, b.counts);
    }
    #[test]
    fn checkpoint_roundtrip_preserves_weights_and_rejects_new_source() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("sml-stream-{}-{nonce}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let input = b"abcdefgh1234";
        let mut bank = Bank::new();
        let events = bank.learn(input).unwrap();
        let state = State {
            docs: 1,
            offset: input.len() as u64 + 1,
            payload: input.len() as u64,
            events,
        };
        let corpus = sha256(input);
        let manifest = sha256(b"pinned manifest");
        checkpoint(&dir, &corpus, &manifest, &state, &bank).unwrap();
        let mut recovered = Bank::new();
        let saved = restore(&dir, &corpus, &manifest, &mut recovered).unwrap();
        assert_eq!(saved.docs, 1);
        assert_eq!(saved.events, events);
        assert_eq!(saved.offset, state.offset);
        assert_eq!(bank.counts, recovered.counts);
        assert!(restore(&dir, &sha256(b"changed source"), &manifest, &mut Bank::new()).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn fails_closed_on_bad_rights_or_manifest() {
        let row = "a.parquet\t1\tdeadbeef\t200\tCC-BY-NC\tCC-BY-NC\tsource\tcreator";
        assert!(entry(row).is_err());
        let row2 = "a.parquet\t1\tdeadbeef\t200\tCC0\tCC0\tsource\tcreator";
        assert!(entry(row2).is_err());
    }
}
