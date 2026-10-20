//! SML-50M S0: rights-reviewed, SHA-pinned, per-document streaming fit.
//!
//! A research-only train/DEV experiment. 50M *addressable* slots are not 50M
//! learned weights and an implementation PASS is not a language-model PASS.
//! Existing SEC-prefix pilots remain unchanged.
use sml_core::{
    byte_factorization::ByteFactorization,
    elastic_ngram_paged_sha::{hex, parse_hex, sha256, Sha256},
    sparse_gradient_atlas::{QuantizedSparseAtlas, SparseConfig, SparseGradientAtlas},
};
use std::{
    collections::HashSet,
    env,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::{Component, Path, PathBuf},
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const HEADER: &str = "role\tpath\tbytes\tsha256\trights_review";
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FILES: usize = 100_000;
const MIN_DOCUMENT_BYTES: u64 = 128;
const MAX_DOCUMENT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TRAIN_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_DEV_BYTES: u64 = 256 * 1024 * 1024;
const MIN_TRAIN_BYTES: u64 = 64 * 1024 * 1024;
const MIN_DEV_BYTES: u64 = 4 * 1024 * 1024;
const PAGES_50M: u64 = 48_829;
const BATCH_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Train,
    Dev,
}

#[derive(Clone, Debug)]
struct Document {
    role: Role,
    path: PathBuf,
    length: u64,
    digest: [u8; 32],
}

struct Roster {
    entries: Vec<Document>,
    manifest_sha: String,
    train_bytes: u64,
    dev_bytes: u64,
    train_files: usize,
    dev_files: usize,
}

fn checked_file(path: &Path) -> Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("expected a regular non-symlink file".into());
    }
    Ok(meta)
}

fn checked_path(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty() || relative.len() > 1024 || relative.chars().any(char::is_control) {
        return Err("unsafe source-relative path".into());
    }
    let p = Path::new(relative);
    if !p.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err("absolute or non-normal source path".into());
    }
    let joined = root.join(p);
    checked_file(&joined)?;
    let canonical = fs::canonicalize(&joined)?;
    if !canonical.starts_with(root) {
        return Err("manifest source escaped the data directory".into());
    }
    Ok(canonical)
}

fn load_roster(
    root: &Path,
    manifest: &Path,
    minimum_train: u64,
    minimum_dev: u64,
) -> Result<Roster> {
    let canonical_root = fs::canonicalize(root)?;
    if !fs::symlink_metadata(root)?.is_dir() || fs::symlink_metadata(root)?.file_type().is_symlink()
    {
        return Err("data root must be a directory, not a symlink".into());
    }
    let info = checked_file(manifest)?;
    if info.len() == 0 || info.len() > MAX_MANIFEST_BYTES {
        return Err("manifest outside bounded size".into());
    }
    let contents = fs::read(manifest)?;
    let text = std::str::from_utf8(&contents)?;
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Err("invalid document manifest header".into());
    }
    let mut entries = Vec::new();
    let mut seen_paths = HashSet::new();
    let mut seen_hashes = HashSet::new();
    let mut train_bytes = 0u64;
    let mut dev_bytes = 0u64;
    let mut train_files = 0usize;
    let mut dev_files = 0usize;
    let mut dev_started = false;
    for line in lines {
        if entries.len() == MAX_FILES {
            return Err("too many documents in manifest".into());
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 5 {
            return Err("invalid document manifest record".into());
        }
        let role = match fields[0] {
            "TRAIN" if !dev_started => Role::Train,
            "DEV" => {
                dev_started = true;
                Role::Dev
            }
            _ => return Err("invalid document role/order".into()),
        };
        if !matches!(
            fields[4],
            "OWNED_REVIEWED"
                | "PUBLIC_DOMAIN_REVIEWED"
                | "CC0_REVIEWED"
                | "CC_BY_ATTRIBUTION_REVIEWED"
        ) {
            return Err("document rights have not been declared reviewed".into());
        }
        let length: u64 = fields[2].parse()?;
        if !(MIN_DOCUMENT_BYTES..=MAX_DOCUMENT_BYTES).contains(&length) {
            return Err("document byte length outside budget".into());
        }
        let digest = parse_hex(fields[3]).ok_or("invalid lowercase SHA256 field")?;
        if fields[3] != hex(&digest) {
            return Err("noncanonical SHA256 field".into());
        }
        let source = checked_path(&canonical_root, fields[1])?;
        if !seen_paths.insert(source.clone()) || !seen_hashes.insert(digest) {
            return Err("duplicate document path or source digest".into());
        }
        if checked_file(&source)?.len() != length {
            return Err("declared source size does not match on-disk file".into());
        }
        match role {
            Role::Train => {
                train_files += 1;
                train_bytes = train_bytes
                    .checked_add(length)
                    .ok_or("TRAIN size overflow")?;
                if train_bytes > MAX_TRAIN_BYTES {
                    return Err("TRAIN byte budget exceeded".into());
                }
            }
            Role::Dev => {
                dev_files += 1;
                dev_bytes = dev_bytes.checked_add(length).ok_or("DEV size overflow")?;
                if dev_bytes > MAX_DEV_BYTES {
                    return Err("DEV byte budget exceeded".into());
                }
            }
        }
        entries.push(Document {
            role,
            path: source,
            length,
            digest,
        });
    }
    if train_bytes < minimum_train || dev_bytes < minimum_dev {
        return Err("TRAIN or DEV lacks preregistered minimum data".into());
    }
    Ok(Roster {
        entries,
        manifest_sha: hex(&sha256(&contents)),
        train_bytes,
        dev_bytes,
        train_files,
        dev_files,
    })
}

fn load_document(entry: &Document) -> Result<Vec<u8>> {
    let info = checked_file(&entry.path)?;
    if info.len() != entry.length {
        return Err("source file resized after roster admission".into());
    }
    let mut input = BufReader::new(File::open(&entry.path)?);
    let mut data = Vec::with_capacity(usize::try_from(entry.length)?);
    input.read_to_end(&mut data)?;
    if data.len() as u64 != entry.length || sha256(&data) != entry.digest {
        return Err("source document SHA256 changed or is invalid".into());
    }
    std::str::from_utf8(&data)?;
    Ok(data)
}

fn file_sha(path: &Path) -> Result<String> {
    checked_file(path)?;
    let mut file = BufReader::new(File::open(path)?);
    let mut state = Sha256::default();
    let mut block = [0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut block)?;
        if n == 0 {
            break;
        }
        state.update(&block[..n]);
    }
    Ok(hex(&state.finish()))
}

#[derive(Clone, Copy)]
struct Limits {
    min_train: u64,
    min_dev: u64,
}

fn train_stream(
    data_root: &Path,
    manifest: &Path,
    output: &Path,
    config: SparseConfig,
    factorization: ByteFactorization,
    limits: Limits,
) -> Result<()> {
    if output.exists() || output == data_root || output == manifest {
        return Err("model output already exists or overlaps input".into());
    }
    // Prevent writing inside the source tree, even when a different pathname
    // is used for the same physical root.
    let root_canonical = fs::canonicalize(data_root)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent_canonical = fs::canonicalize(parent)?;
    if parent_canonical.starts_with(&root_canonical) {
        return Err("output must not contaminate the source document tree".into());
    }
    let roster = load_roster(data_root, manifest, limits.min_train, limits.min_dev)?;
    let start = Instant::now();
    let mut atlas = SparseGradientAtlas::create_with_factorization(output, config, factorization)?;
    let mut train_loss = 0f64;
    let mut train_events = 0u64;
    for entry in roster.entries.iter().filter(|d| d.role == Role::Train) {
        let bytes = load_document(entry)?;
        let (loss, events) = atlas.train_document_grouped(&bytes, BATCH_BYTES)?;
        train_loss += loss;
        train_events = train_events
            .checked_add(events)
            .ok_or("TRAIN events overflow")?;
    }
    let elapsed_train = start.elapsed().as_secs_f64();
    if train_events == 0 || atlas.counters().distinct_updated_parameters == 0 {
        return Err("no supervised training achieved".into());
    }
    // Commit only a complete TRAIN epoch. An interrupted write leaves no
    // COMPLETE marker and is not an admitted recoverable checkpoint.
    atlas.checkpoint(output)?;
    let trained = atlas.counters();
    let mut dev_f32_loss = 0f64;
    let mut dev_events = 0u64;
    for entry in roster.entries.iter().filter(|d| d.role == Role::Dev) {
        let bytes = load_document(entry)?;
        let (loss, events) = atlas.evaluate_document_grouped(&bytes, BATCH_BYTES)?;
        dev_f32_loss += loss;
        dev_events = dev_events
            .checked_add(events)
            .ok_or("DEV events overflow")?;
    }
    let assessed = atlas.counters();
    if dev_events == 0
        || assessed.supervised_bits != trained.supervised_bits
        || assessed.parameter_updates != trained.parameter_updates
        || assessed.distinct_updated_parameters != trained.distinct_updated_parameters
    {
        return Err("DEV changed learning state or contains no predictions".into());
    }
    let q4_path = output.join("weights.q4");
    atlas.export_q4(&q4_path)?;
    let mut quantized =
        QuantizedSparseAtlas::open_with_factorization(&q4_path, config, factorization)?;
    let mut dev_q4_loss = 0f64;
    let mut dev_q4_events = 0u64;
    for entry in roster.entries.iter().filter(|d| d.role == Role::Dev) {
        let bytes = load_document(entry)?;
        let (loss, events) = quantized.evaluate_document_grouped(&bytes, BATCH_BYTES)?;
        dev_q4_loss += loss;
        dev_q4_events = dev_q4_events
            .checked_add(events)
            .ok_or("Q4 events overflow")?;
    }
    if dev_q4_events != dev_events || !dev_f32_loss.is_finite() || !dev_q4_loss.is_finite() {
        return Err("F32/Q4 DEV comparison incomplete".into());
    }
    // A separate reopening checks final checkpoint hashes and configuration.
    let reopened = SparseGradientAtlas::reopen(output, config)?;
    if reopened.factorization() != factorization
        || reopened.counters().distinct_updated_parameters != trained.distinct_updated_parameters
    {
        return Err("checkpoint roundtrip failed".into());
    }
    let train_bpc = train_loss / train_events as f64;
    let dev_f32_bpc = dev_f32_loss / dev_events as f64;
    let dev_q4_bpc = dev_q4_loss / dev_q4_events as f64;
    let throughput = train_events as f64 / elapsed_train.max(1e-9);
    let q4_sha = file_sha(&q4_path)?;
    let snapshot_sha = file_sha(&output.join("snapshot.tsv"))?;
    let metric = format!(
        "schema\tmanifest_sha256\tcapacity_slots\tlearned_unique\tparameter_updates\ttrain_files\ttrain_bytes\ttrain_events\ttrain_bpc\tdev_files\tdev_bytes\tdev_events\tdev_f32_bpc\tdev_q4_bpc\tq4_minus_f32_bpc\ttrain_elapsed_s\ttrain_events_per_s\tmaster_bytes\tq4_bytes\tfactorization\tq4_sha256\tsnapshot_sha256\ttest\tpublication\nSML50M_STREAM_V1\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{train_bpc:.9}\t{}\t{}\t{}\t{dev_f32_bpc:.9}\t{dev_q4_bpc:.9}\t{:.9}\t{elapsed_train:.3}\t{throughput:.3}\t{}\t{}\t{}\t{}\t{}\tUNOPENED\tNOT_PROMOTED\n",
        roster.manifest_sha,
        config.total_parameters(),
        trained.distinct_updated_parameters,
        trained.parameter_updates,
        roster.train_files,
        roster.train_bytes,
        train_events,
        roster.dev_files,
        roster.dev_bytes,
        dev_events,
        dev_q4_bpc - dev_f32_bpc,
        config.train_master_bytes(),
        config.packed_q4_bytes(),
        match factorization {
            ByteFactorization::IndependentBits => "independent_bits",
            ByteFactorization::MsbTree => "msb_tree",
        },
        q4_sha,
        snapshot_sha,
    );
    let mut report = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("metrics.tsv"))?;
    report.write_all(metric.as_bytes())?;
    report.sync_all()?;
    // MUST be written last; publication/SCIENCE remain deliberately false.
    let mut complete = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("COMPLETE"))?;
    complete.write_all(
        format!(
            "SML50M_STREAM_V1\t{}\n",
            file_sha(&output.join("metrics.tsv"))?
        )
        .as_bytes(),
    )?;
    complete.sync_all()?;
    File::open(output)?.sync_all()?;
    print!("{metric}");
    println!("SML50M_STREAM_DONE source_docs={} train_bytes={} dev_bytes={} updated={} model_promoted=false test=UNOPENED", roster.entries.len(), roster.train_bytes, roster.dev_bytes, trained.distinct_updated_parameters);
    Ok(())
}

fn cli(args: &[String]) -> Result<()> {
    let usage = "usage: sml-sparse-gradient-stream-fit --stream-50m-msb|--stream-50m-bits DATA_ROOT MANIFEST.tsv NEW_OUTPUT_DIR";
    let [_, flag, root, manifest, output] = args else {
        return Err(usage.into());
    };
    let factorization = match flag.as_str() {
        "--stream-50m-msb" => ByteFactorization::MsbTree,
        "--stream-50m-bits" => ByteFactorization::IndependentBits,
        _ => return Err(usage.into()),
    };
    train_stream(
        Path::new(root),
        Path::new(manifest),
        Path::new(output),
        SparseConfig {
            pages: PAGES_50M,
            max_cached_pages: 32,
            learning_rate: 0.15,
        },
        factorization,
        Limits {
            min_train: MIN_TRAIN_BYTES,
            min_dev: MIN_DEV_BYTES,
        },
    )
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if let Err(error) = cli(&args) {
        eprintln!("SML50M_STREAM_FAIL {error}");
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
        output: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let home = env::temp_dir().join(format!(
                "sml50m-stream-{}-{seed}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed),
            ));
            let data = home.join("data");
            fs::create_dir_all(&data).unwrap();
            Self {
                manifest: home.join("manifest.tsv"),
                output: home.join("model"),
                home,
                data,
            }
        }
        fn make_doc(&self, name: &str, content: &[u8]) -> String {
            let path = self.data.join(name);
            fs::write(path, content).unwrap();
            format!(
                "{}\t{}\t{}\t{}\tOWNED_REVIEWED\n",
                if name.starts_with("train") {
                    "TRAIN"
                } else {
                    "DEV"
                },
                name,
                content.len(),
                hex(&sha256(content)),
            )
        }
        fn write_manifest(&self, body: &str) {
            fs::write(&self.manifest, format!("{HEADER}\n{body}")).unwrap();
        }
        fn run(&self) -> Result<()> {
            train_stream(
                &self.data,
                &self.manifest,
                &self.output,
                SparseConfig {
                    pages: 8,
                    max_cached_pages: 2,
                    learning_rate: 0.15,
                },
                ByteFactorization::MsbTree,
                Limits {
                    min_train: 256,
                    min_dev: 128,
                },
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.home).ok();
        }
    }

    #[test]
    fn bounded_document_stream_produces_reopenable_f32_and_q4() {
        let fx = Fixture::new();
        let mut rows = fx.make_doc("train1.bin", &vec![b'a'; 512]);
        rows.push_str(&fx.make_doc("train2.bin", &vec![b'b'; 512]));
        rows.push_str(&fx.make_doc("dev1.bin", &vec![b'a'; 256]));
        fx.write_manifest(&rows);
        fx.run().unwrap();
        let report = fs::read_to_string(fx.output.join("metrics.tsv")).unwrap();
        assert!(report.contains("SML50M_STREAM_V1"));
        assert!(report.contains("UNOPENED\tNOT_PROMOTED"));
        assert_eq!(report.lines().nth(1).unwrap().split('\t').count(), 24);
        assert_eq!(
            fs::metadata(fx.output.join("master.f32")).unwrap().len(),
            8 * 4096
        );
        assert_eq!(
            fs::metadata(fx.output.join("weights.q4")).unwrap().len(),
            8 * 512
        );
        assert!(fx.output.join("COMPLETE").is_file());
        let original = fs::read(fx.output.join("master.f32")).unwrap();
        assert!(fx.run().is_err());
        assert_eq!(original, fs::read(fx.output.join("master.f32")).unwrap());
    }

    #[test]
    fn manifest_validation_rejects_leakage_and_unreviewed_corpora() {
        let fx = Fixture::new();
        let a = vec![b'a'; 512];
        let b = vec![b'b'; 256];
        let train = fx.make_doc("train.bin", &a);
        let dev = fx.make_doc("dev.bin", &b);
        fx.write_manifest(&(train.clone() + &dev));
        assert!(load_roster(&fx.data, &fx.manifest, 256, 128).is_ok());
        fx.write_manifest(&(dev.clone() + &train));
        assert!(load_roster(&fx.data, &fx.manifest, 256, 128).is_err());
        fx.write_manifest(&(train.clone() + &train));
        assert!(load_roster(&fx.data, &fx.manifest, 256, 128).is_err());
        fx.write_manifest(&(train.clone() + &dev.replace("OWNED_REVIEWED", "UNREVIEWED")));
        assert!(load_roster(&fx.data, &fx.manifest, 256, 128).is_err());
        fx.write_manifest(&(train.clone() + &dev));
        assert!(load_roster(&fx.data, &fx.manifest, 10000, 128).is_err());
        assert!(!fx.output.exists());
    }

    #[test]
    fn scoped_open_license_labels_are_opt_in_and_unqualified_labels_fail() {
        let fx = Fixture::new();
        let train = fx.make_doc("train.bin", &vec![b'a'; 512]);
        let dev = fx.make_doc("dev.bin", &vec![b'b'; 256]);
        for label in ["CC0_REVIEWED", "CC_BY_ATTRIBUTION_REVIEWED"] {
            let approved = train.replace("OWNED_REVIEWED", label);
            fx.write_manifest(&(approved + &dev));
            assert!(load_roster(&fx.data, &fx.manifest, 256, 128).is_ok());
        }
        for label in [
            "PD_CLAIM",
            "CC_BY_ATTRIBUTION",
            "CC_BY_SA",
            "CC_BY_NC",
            "CC0",
            "UNREVIEWED",
        ] {
            let unqualified = train.replace("OWNED_REVIEWED", label);
            fx.write_manifest(&(unqualified + &dev));
            assert!(load_roster(&fx.data, &fx.manifest, 256, 128).is_err());
        }
        assert!(!fx.output.exists());
    }

    #[test]
    fn corrupt_source_and_path_escape_never_complete() {
        let fx = Fixture::new();
        let a = vec![b'0'; 512];
        let b = vec![b'1'; 256];
        let train = fx.make_doc("train.bin", &a);
        let dev = fx.make_doc("dev.bin", &b);
        let bad = train.replace(&hex(&sha256(&a)), &"0".repeat(64));
        fx.write_manifest(&(bad + &dev));
        assert!(fx.run().is_err());
        assert!(!fx.output.join("COMPLETE").exists());
        fx.write_manifest(&(train.clone() + &dev));
        fs::write(fx.data.join("train.bin"), vec![b'z'; 512]).unwrap();
        let admitted = load_roster(&fx.data, &fx.manifest, 256, 128).unwrap();
        assert!(load_document(&admitted.entries[0]).is_err());
        assert!(!fx.output.join("COMPLETE").exists());
        let traversal = train.replace("train.bin", "../outside.bin");
        fx.write_manifest(&(traversal + &dev));
        assert!(load_roster(&fx.data, &fx.manifest, 256, 128).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_outside_data_root_is_denied() {
        use std::os::unix::fs::symlink;
        let fx = Fixture::new();
        fs::write(fx.home.join("outside.bin"), vec![b'x'; 512]).unwrap();
        symlink(fx.home.join("outside.bin"), fx.data.join("trainlink.bin")).unwrap();
        assert!(checked_path(&fs::canonicalize(&fx.data).unwrap(), "trainlink.bin").is_err());
    }
}
