//! SML-50M S4d: bounded, read-only normalized-text overlap preflight.
//!
//! This is NOT a licensing decision, a semantic near-deduplication test,
//! a training authorization, or a model-quality experiment.
//! Source files are inert UTF-8 bytes and are never executed or displayed.
use sml_core::elastic_ngram_paged_sha::{hex, parse_hex, sha256};
use std::{
    collections::{HashMap, HashSet},
    env,
    error::Error,
    fs,
    path::{Component, Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const HEADER: &str = "role\tpath\tbytes\tsha256\trights_review";
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FILES: usize = 100_000;
const MAX_TRAIN_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_DEV_BYTES: u64 = 256 * 1024 * 1024;
const MIN_DOCUMENT_BYTES: u64 = 128;
const MAX_DOCUMENT_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
struct Report {
    train_docs: usize,
    dev_docs: usize,
    normalized_groups: usize,
    within_split_groups: usize,
    cross_split_groups: usize,
}

fn regular_file(path: &Path) -> Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("expected a regular non-symlink file".into());
    }
    Ok(meta)
}

fn relative_document(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty() || relative.len() > 1024 || relative.chars().any(char::is_control) {
        return Err("unsafe source-relative path".into());
    }
    if !Path::new(relative)
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err("absolute or non-normal source path".into());
    }
    let input = root.join(relative);
    regular_file(&input)?;
    let canonical = fs::canonicalize(input)?;
    if !canonical.starts_with(root) {
        return Err("source escaped the data root".into());
    }
    Ok(canonical)
}

/// Exact and deliberately narrow canonicalization:
/// ASCII case-insensitive and ASCII whitespace-run-insensitive only.
/// Non-ASCII UTF-8 bytes are preserved, NOT case folded.
fn normalized_digest(content: &[u8]) -> Result<[u8; 32]> {
    std::str::from_utf8(content)?;
    let mut normalized = Vec::with_capacity(content.len());
    let mut pending_space = false;
    for byte in content {
        if byte.is_ascii_whitespace() {
            pending_space = !normalized.is_empty();
        } else {
            if pending_space {
                normalized.push(b' ');
                pending_space = false;
            }
            normalized.push(byte.to_ascii_lowercase());
        }
    }
    if normalized.is_empty() {
        return Err("empty text after ASCII whitespace normalization".into());
    }
    Ok(sha256(&normalized))
}

fn reviewed_label(value: &str) -> bool {
    matches!(
        value,
        "OWNED_REVIEWED"
            | "PUBLIC_DOMAIN_REVIEWED"
            | "CC0_REVIEWED"
            | "CC_BY_ATTRIBUTION_REVIEWED"
            | "FR_OPEN_LICENSE_V2_ATTRIBUTION_REVIEWED"
            | "APACHE_2_0_ATTRIBUTION_REVIEWED"
            | "MIT_ATTRIBUTION_REVIEWED"
    )
}

fn audit(root: &Path, manifest: &Path) -> Result<Report> {
    let root_meta = fs::symlink_metadata(root)?;
    if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
        return Err("data root must be a non-symlink directory".into());
    }
    let root = fs::canonicalize(root)?;
    let size = regular_file(manifest)?.len();
    if size == 0 || size > MAX_MANIFEST_BYTES {
        return Err("manifest outside bounded size".into());
    }
    let raw = fs::read(manifest)?;
    let text = std::str::from_utf8(&raw)?;
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Err("invalid document manifest header".into());
    }

    let mut seen_paths = HashSet::new();
    let mut seen_raw_hashes = HashSet::new();
    // The counters count source documents, not near-duplicate pairs.
    let mut canonical_groups: HashMap<[u8; 32], [usize; 2]> = HashMap::new();
    let mut counts = [0usize; 2];
    let mut split_bytes = [0u64; 2];
    let mut dev_started = false;
    for line in lines {
        if counts[0] + counts[1] == MAX_FILES {
            return Err("too many documents in manifest".into());
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 5 {
            return Err("invalid document manifest record".into());
        }
        let split = match fields[0] {
            "TRAIN" if !dev_started => 0,
            "DEV" => {
                dev_started = true;
                1
            }
            _ => return Err("invalid document role/order".into()),
        };
        if !reviewed_label(fields[4]) {
            return Err("document rights not declared reviewed".into());
        }
        let claimed_len = fields[2].parse::<u64>()?;
        if !(MIN_DOCUMENT_BYTES..=MAX_DOCUMENT_BYTES).contains(&claimed_len) {
            return Err("document byte length outside budget".into());
        }
        split_bytes[split] = split_bytes[split]
            .checked_add(claimed_len)
            .ok_or("split byte count overflow")?;
        let limit = if split == 0 { MAX_TRAIN_BYTES } else { MAX_DEV_BYTES };
        if split_bytes[split] > limit {
            return Err("TRAIN or DEV byte budget exceeded".into());
        }
        let claimed_hash = parse_hex(fields[3]).ok_or("invalid SHA256")?;
        if fields[3] != hex(&claimed_hash) {
            return Err("noncanonical SHA256 field".into());
        }
        let path = relative_document(&root, fields[1])?;
        if !seen_paths.insert(path.clone()) || !seen_raw_hashes.insert(claimed_hash) {
            return Err("duplicate document path or raw digest".into());
        }
        if regular_file(&path)?.len() != claimed_len {
            return Err("declared source size does not match file".into());
        }
        let content = fs::read(&path)?;
        if content.len() as u64 != claimed_len || sha256(&content) != claimed_hash {
            return Err("source document SHA256 mismatch".into());
        }
        let norm = normalized_digest(&content)?;
        let entry = canonical_groups.entry(norm).or_insert([0usize; 2]);
        entry[split] += 1;
        counts[split] += 1;
    }
    if counts.contains(&0) {
        return Err("both TRAIN and DEV are required".into());
    }
    let within_split_groups = canonical_groups
        .values()
        .filter(|count| count[0] > 1 || count[1] > 1)
        .count();
    let cross_split_groups = canonical_groups
        .values()
        .filter(|count| count[0] > 0 && count[1] > 0)
        .count();
    Ok(Report {
        train_docs: counts[0],
        dev_docs: counts[1],
        normalized_groups: canonical_groups.len(),
        within_split_groups,
        cross_split_groups,
    })
}

fn cli(args: &[String]) -> Result<()> {
    if args.len() != 5 || args[1] != "--data-dir" || args[3] != "--manifest" {
        return Err(
            "usage: sml-corpus-overlap-audit --data-dir DIR --manifest MANIFEST.tsv".into(),
        );
    }
    let report = audit(Path::new(&args[2]), Path::new(&args[4]))?;
    let verdict = if report.within_split_groups == 0 && report.cross_split_groups == 0 {
        "PASS_NARROW"
    } else {
        "REJECT_OVERLAP"
    };
    println!(
        "SML50M_NORMALIZED_OVERLAP_V1 verdict={} train_docs={} dev_docs={} normalized_groups={} within_split_groups={} cross_split_groups={} rights_certified=false training_started=false test=UNOPENED",
        verdict,
        report.train_docs,
        report.dev_docs,
        report.normalized_groups,
        report.within_split_groups,
        report.cross_split_groups
    );
    if verdict != "PASS_NARROW" {
        return Err("normalized-text duplicate group detected; sources withheld".into());
    }
    Ok(())
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if let Err(error) = cli(&args) {
        eprintln!("SML50M_NORMALIZED_OVERLAP_FAIL {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        home: PathBuf,
        data: PathBuf,
        manifest: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let home = env::temp_dir().join(format!(
                "sml50m-overlap-{}-{now}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            let data = home.join("data");
            fs::create_dir_all(&data).unwrap();
            let manifest = home.join("manifest.tsv");
            Self {
                home,
                data,
                manifest,
            }
        }

        fn doc(&self, role: &str, name: &str, bytes: &[u8]) -> String {
            fs::write(self.data.join(name), bytes).unwrap();
            format!(
                "{role}\t{name}\t{}\t{}\tOWNED_REVIEWED\n",
                bytes.len(),
                hex(&sha256(bytes))
            )
        }

        fn manifest(&self, rows: &str) {
            fs::write(&self.manifest, format!("{HEADER}\n{rows}")).unwrap();
        }

        fn check(&self) -> Result<Report> {
            audit(&self.data, &self.manifest)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.home).ok();
        }
    }

    #[test]
    fn distinct_sources_pass_narrow_without_authorizing_training() {
        let fixture = Fixture::new();
        let train = fixture.doc("TRAIN", "a.txt", &vec![b'a'; 256]);
        let dev = fixture.doc("DEV", "b.txt", &vec![b'b'; 256]);
        fixture.manifest(&(train + &dev));
        assert_eq!(
            fixture.check().unwrap(),
            Report {
                train_docs: 1,
                dev_docs: 1,
                normalized_groups: 2,
                within_split_groups: 0,
                cross_split_groups: 0
            }
        );
    }

    #[test]
    fn ascii_case_and_whitespace_variants_overlap_across_splits() {
        let fixture = Fixture::new();
        let train = fixture.doc("TRAIN", "a.txt", "Alpha  BETA\t".repeat(20).as_bytes());
        let dev = fixture.doc("DEV", "b.txt", "alpha beta ".repeat(20).as_bytes());
        fixture.manifest(&(train + &dev));
        let report = fixture.check().unwrap();
        assert_eq!(report.cross_split_groups, 1);
        assert!(cli(&[
            "sml-corpus-overlap-audit".into(),
            "--data-dir".into(),
            fixture.data.display().to_string(),
            "--manifest".into(),
            fixture.manifest.display().to_string()
        ])
        .is_err());
    }

    #[test]
    fn aliases_within_train_are_rejected_too() {
        let fixture = Fixture::new();
        let one = fixture.doc("TRAIN", "a.txt", "Alpha  Beta ".repeat(20).as_bytes());
        let two = fixture.doc("TRAIN", "b.txt", "alpha beta  ".repeat(20).as_bytes());
        let dev = fixture.doc("DEV", "c.txt", &vec![b'z'; 256]);
        fixture.manifest(&(one + &two + &dev));
        let report = fixture.check().unwrap();
        assert_eq!(report.within_split_groups, 1);
        assert_eq!(report.cross_split_groups, 0);
    }

    #[test]
    fn changed_digest_and_traversal_fail_closed() {
        let fixture = Fixture::new();
        let train = fixture.doc("TRAIN", "a.txt", &vec![b'a'; 256]);
        let dev = fixture.doc("DEV", "b.txt", &vec![b'b'; 256]);
        fixture.manifest(&(train.clone() + &dev));
        fs::write(fixture.data.join("a.txt"), vec![b'c'; 256]).unwrap();
        assert!(fixture.check().is_err());
        fs::write(fixture.data.join("a.txt"), vec![b'a'; 256]).unwrap();
        fixture.manifest(&(train.replace("a.txt", "../a.txt") + &dev));
        assert!(fixture.check().is_err());
    }

    #[test]
    fn unknown_rights_and_missing_split_fail_closed() {
        let fixture = Fixture::new();
        let train = fixture.doc("TRAIN", "a.txt", &vec![b'a'; 256]);
        let dev = fixture.doc("DEV", "b.txt", &vec![b'b'; 256]);
        fixture.manifest(&(train.clone() + &dev.replace("OWNED_REVIEWED", "MIT_CLAIM")));
        assert!(fixture.check().is_err());
        fixture.manifest(&train);
        assert!(fixture.check().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_is_rejected() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let train = fixture.doc("TRAIN", "a.txt", &vec![b'a'; 256]);
        fixture.doc("DEV", "original.txt", &vec![b'b'; 256]);
        symlink(
            fixture.data.join("original.txt"),
            fixture.data.join("linked.txt"),
        )
        .unwrap();
        let dev = format!(
            "DEV\tlinked.txt\t256\t{}\tOWNED_REVIEWED\n",
            hex(&sha256(&vec![b'b'; 256]))
        );
        fixture.manifest(&(train + &dev));
        assert!(fixture.check().is_err());
    }
}
