//! Exploratory Rust-native Boolean context learning over separate repositories.
//! No GRU-v2 G0 admission, no TEST access and no 25B claim.
use sml_core::elastic_ngram_paged_sha::{hex, sha256};
use std::{
    collections::HashSet,
    env,
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const CLASSES: usize = 256;
const PAIRS: usize = 4;
const BITS: usize = 8;
const SLOTS: usize = BITS * CLASSES * PAIRS;
const MAX_FILE: u64 = 2 * 1024 * 1024;
const MAX_TOTAL: u64 = 256 * 1024 * 1024;
const MIN_TRAIN: u64 = 8 * 1024 * 1024;
const MIN_TRAIN_MULTI: u64 = 16 * 1024 * 1024;
const MIN_DEV: u64 = 64 * 1024;
const MAX_FILES: usize = 50_000;
// Frozen Laplace smoothing; no DEV-driven model choice.
const ALPHA: f64 = 1.0;

#[derive(Clone)]
struct Bank {
    counts: Vec<[u64; 2]>,
}

impl Bank {
    fn new() -> Self {
        Self {
            counts: vec![[0; 2]; SLOTS],
        }
    }

    fn slot(prev2: u8, prev1: u8, bit: usize) -> usize {
        let upper_nibbles = usize::from(prev2 >> 4) * 16 + usize::from(prev1 >> 4);
        let pair = (usize::from((prev2 >> bit) & 1) << 1) | usize::from((prev1 >> bit) & 1);
        bit * CLASSES * PAIRS + upper_nibbles * PAIRS + pair
    }

    fn learn(&mut self, bytes: &[u8]) -> Result<u64> {
        let mut events = 0_u64;
        for triple in bytes.windows(3) {
            for bit in 0..BITS {
                let slot = Self::slot(triple[0], triple[1], bit);
                let target = usize::from((triple[2] >> bit) & 1);
                self.counts[slot][target] = self.counts[slot][target]
                    .checked_add(1)
                    .ok_or("counter overflow")?;
            }
            events += 1;
        }
        Ok(events)
    }

    // Scores never mutate the frozen TRAIN bank.
    fn score(&self, bytes: &[u8]) -> Result<(f64, u64)> {
        let mut neg_log2 = 0.0_f64;
        let mut events = 0_u64;
        for triple in bytes.windows(3) {
            for bit in 0..BITS {
                let counts = self.counts[Self::slot(triple[0], triple[1], bit)];
                let target = usize::from((triple[2] >> bit) & 1);
                let denom = (counts[0] + counts[1]) as f64 + 2.0 * ALPHA;
                let prob = (counts[target] as f64 + ALPHA) / denom;
                neg_log2 -= prob.log2();
            }
            events += 1;
        }
        if !neg_log2.is_finite() {
            return Err("non-finite loss".into());
        }
        Ok((neg_log2, events))
    }

    fn changed_slots(&self) -> usize {
        self.counts
            .iter()
            .filter(|counts| counts[0] + counts[1] > 0)
            .count()
    }
}

struct Corpus {
    bytes: u64,
    events: u64,
    files: usize,
    provenance: String,
    fingerprints: HashSet<[u8; 32]>,
    duplicates_skipped: u64,
}

impl Corpus {
    fn new() -> Self {
        Self {
            bytes: 0,
            events: 0,
            files: 0,
            provenance: String::from("role\trepository\tpath\tbytes\tsha256\n"),
            fingerprints: HashSet::new(),
            duplicates_skipped: 0,
        }
    }

    fn take(&mut self, role: &str, repo: &str, rel: &Path, bytes: &[u8]) -> Result<bool> {
        let fingerprint = sha256(bytes);
        if !self.fingerprints.insert(fingerprint) {
            self.duplicates_skipped = self.duplicates_skipped.checked_add(1).ok_or("duplicate overflow")?;
            return Ok(false);
        }
        self.files += 1;
        if self.files > MAX_FILES {
            return Err("corpus has too many files".into());
        }
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or("byte overflow")?;
        if self.bytes > MAX_TOTAL {
            return Err("corpus exceeds 256 MiB".into());
        }
        self.provenance.push_str(&format!(
            "{role}\t{repo}\t{}\t{}\t{}\n",
            rel.display(),
            bytes.len(),
            hex(&fingerprint)
        ));
        Ok(true)
    }
}

fn selected_root_segment(name: &str) -> bool {
    matches!(
        name,
        "src" | "crates" | "docs" | "README.md" | "BOOTSTRAP.md"
    )
}
fn excluded_segment(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "vendor"
            | "third_party"
            | "third-party"
            | "target"
            | "node_modules"
            | "fixtures"
            | "testdata"
            | "tests"
            | "examples"
            | "generated"
            | "assets"
    )
}
fn gather(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.is_dir() || root.symlink_metadata()?.file_type().is_symlink() {
        return Err("invalid repository root".into());
    }
    let mut stack = vec![(root.to_path_buf(), PathBuf::new())];
    let mut paths = Vec::new();
    while let Some((dir, rel_dir)) = stack.pop() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_str().ok_or("non-UTF8 path")?;
            if excluded_segment(name) || name.starts_with('.') {
                continue;
            }
            if rel_dir.as_os_str().is_empty() && !selected_root_segment(name) {
                continue;
            }
            let relative = rel_dir.join(name);
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                stack.push((entry.path(), relative));
            } else if metadata.is_file()
                && metadata.len() >= 128
                && metadata.len() <= MAX_FILE
                && matches!(
                    entry.path().extension().and_then(|v| v.to_str()),
                    Some("rs" | "md")
                )
            {
                paths.push(relative);
            }
        }
    }
    paths.sort();
    if paths.is_empty() {
        return Err("no eligible files in repository".into());
    }
    Ok(paths)
}

fn scan(
    root: &Path,
    repo: &str,
    role: &str,
    corpus: &mut Corpus,
    bank: &mut Bank,
    scoring: bool,
) -> Result<f64> {
    let mut logloss = 0.0_f64;
    for relative in gather(root)? {
        let path = root.join(&relative);
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("source changed or became symbolic link".into());
        }
        let bytes = fs::read(&path)?;
        if bytes.len() as u64 != meta.len() || std::str::from_utf8(&bytes).is_err() {
            return Err(format!("source is not stable UTF-8: {}", relative.display()).into());
        }
        if !corpus.take(role, repo, &relative, &bytes)? {
            continue;
        }
        let (loss, events) = if scoring {
            bank.score(&bytes)?
        } else {
            (0.0, bank.learn(&bytes)?)
        };
        corpus.events = corpus.events.checked_add(events).ok_or("event overflow")?;
        logloss += loss;
    }
    Ok(logloss)
}

fn write_new(dir: &Path, name: &str, value: &str) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(name))?;
    f.write_all(value.as_bytes())?;
    f.sync_all()?;
    println!(
        "BOOLEAN_FIT_EVIDENCE file={name} bytes={} sha256={}",
        value.len(),
        hex(&sha256(value.as_bytes()))
    );
    Ok(())
}

fn valid_repository(name: &str) -> bool {
    let Some(short) = name.strip_prefix("Memorithm/") else {
        return false;
    };
    !short.is_empty()
        && short.len() <= 64
        && short.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

// The prior four-root invocation is preserved; --multi introduces frozen 16 MiB scale gate.
fn run(roots: &[(&str, &Path)], dev_root: &Path, output: &Path) -> Result<()> {
    if output.exists() {
        return Err("new output directory must not already exist".into());
    }
    if !(4..=24).contains(&roots.len()) {
        return Err("expected 4 to 24 pinned TRAIN repositories".into());
    }
    let mut names = HashSet::new();
    for (repo, path) in roots {
        if !valid_repository(repo) || !names.insert(*repo) {
            return Err("invalid or duplicate TRAIN repository name".into());
        }
        if path.symlink_metadata()?.file_type().is_symlink() || !path.is_dir() {
            return Err("TRAIN repository root must be a non-symlink directory".into());
        }
    }
    if dev_root.symlink_metadata()?.file_type().is_symlink() || !dev_root.is_dir() {
        return Err("DEV repository root must be a non-symlink directory".into());
    }
    let mut bank = Bank::new();
    let mut train = Corpus::new();
    for (repo, root) in roots {
        scan(root, repo, "TRAIN", &mut train, &mut bank, false)?;
    }
    let floor = if roots.len() > 4 { MIN_TRAIN_MULTI } else { MIN_TRAIN };
    if train.bytes < floor {
        return Err(format!("large corpus minimum not met: {} < {floor}", train.bytes).into());
    }
    let changed = bank.changed_slots();
    if changed < 100 {
        return Err("Boolean bank has insufficient learned states".into());
    }

    // Identical content cannot be used by both TRAIN and frozen DEV.
    // Keep this exclusion explicit in reported counters rather than silently scoring leakage.
    let mut dev = Corpus::new();
    dev.fingerprints.extend(train.fingerprints.iter().copied());
    let neglog = scan(
        dev_root,
        "Memorithm/SML-GENIUS",
        "DEV",
        &mut dev,
        &mut bank,
        true,
    )?;
    if dev.bytes < MIN_DEV || dev.events == 0 || train.events == 0 {
        return Err("insufficient DEV or TRAIN bytes/events".into());
    }
    let dev_bpc = neglog / dev.events as f64;
    let summary = format!(
        "model\ttrain_roots\ttrain_bytes\ttrain_files\ttrain_target_bytes\ttrain_duplicates_skipped\tdev_bytes\tdev_files\tdev_target_bytes\tdev_identical_files_excluded\tlearned_slots\tdev_bpc\tuniform_baseline_bpc\ttest\tg0\nboolean_context_8192\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{dev_bpc:.9}\t8.000000000\tUNOPENED\tNOT_ADMITTED\n",
        roots.len(),
        train.bytes,
        train.files,
        train.events,
        train.duplicates_skipped,
        dev.bytes,
        dev.files,
        dev.events,
        dev.duplicates_skipped,
        changed
    );
    let mut weights = String::from("slot\tzeros\tones\talpha\n");
    for (idx, counts) in bank.counts.iter().enumerate() {
        weights.push_str(&format!("{idx}\t{}\t{}\t{ALPHA}\n", counts[0], counts[1]));
    }
    fs::create_dir(output)?;
    write_new(
        output,
        "provenance.tsv",
        &(train.provenance
            + &dev.provenance.lines().skip(1).map(|line| format!("{line}\n")).collect::<String>()),
    )?;
    write_new(output, "summary.tsv", &summary)?;
    write_new(output, "weights.tsv", &weights)?;
    print!("{summary}");
    println!(
        "BOOLEAN_LARGE_FIT_DONE scope=EXPLORATORY_RUST_NATIVE test=UNOPENED G0_admitted=false"
    );
    Ok(())
}

fn execute_args(args: &[String]) -> Result<()> {
    if args.len() == 7 && args[1] != "--multi" {
        let repos = [
            ("Memorithm/SciRust", Path::new(&args[1])),
            ("Memorithm/TDI", Path::new(&args[2])),
            ("Memorithm/FLAT-ATTENTION", Path::new(&args[3])),
            ("Memorithm/CCOS-Enterprise", Path::new(&args[4])),
        ];
        return run(&repos, Path::new(&args[5]), Path::new(&args[6]));
    }
    if args.len() < 12 || args[1] != "--multi" || (args.len() - 4) % 2 != 0 {
        return Err("usage: sml-boolean-large-corpus-fit --multi REPO ROOT [REPO ROOT ...] DEV_ROOT NEW_OUTPUT_DIR".into());
    }
    let repos: Vec<(&str, &Path)> = args[2..args.len() - 2]
        .chunks_exact(2)
        .map(|part| (part[0].as_str(), Path::new(&part[1])))
        .collect();
    run(&repos, Path::new(&args[args.len() - 2]), Path::new(&args[args.len() - 1]))
}

fn main() {
    let a: Vec<String> = env::args().collect();
    if let Err(e) = execute_args(&a) {
        eprintln!("BOOLEAN_LARGE_FIT_STOP {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn learned_counts_are_frozen_during_dev_scoring() {
        let mut bank = Bank::new();
        let data = b"aaabbaaaaaabbaabbaaaabbaaaab";
        let n = bank.learn(data).unwrap();
        assert_eq!(n, (data.len() - 2) as u64);
        assert!(bank.changed_slots() > 0);
        let before = bank.counts.clone();
        let (loss, scored) = bank.score(data).unwrap();
        assert_eq!(scored, n);
        assert!(loss.is_finite() && loss >= 0.0);
        assert_eq!(before, bank.counts);
    }
    #[test]
    fn cross_split_identical_file_is_excluded_before_dev_scoring() {
        let mut train = Corpus::new();
        let bytes = b"independent repeated text bytes";
        assert!(train.take("TRAIN", "Memorithm/SciRust", Path::new("src/a.rs"), bytes).unwrap());
        let mut dev = Corpus::new();
        dev.fingerprints.extend(train.fingerprints.iter().copied());
        assert!(!dev.take("DEV", "Memorithm/SML-GENIUS", Path::new("docs/a.md"), bytes).unwrap());
        assert_eq!(dev.bytes, 0);
        assert_eq!(dev.duplicates_skipped, 1);
    }
    #[test]
    fn rejects_wrong_sources_and_contexts_remain_bounded() {
        assert!(!selected_root_segment("vendor"));
        assert!(excluded_segment("third_party"));
        assert!(Bank::slot(255, 255, 7) < SLOTS);
        assert_eq!(Bank::slot(0, 0, 0), 0);
    }
}
