//! Self-describing, fail-closed loader for a Q4 checkpoint written by
//! `sml-sparse-gradient-stream-fit` (`SML50M_STREAM_V1` or the S2a
//! continuation `SML50M_STREAM_CONTINUE_V1`).
//!
//! A raw `weights.q4` file has no header: page count, cache size and byte
//! factorization live in `snapshot.tsv` and `metrics.tsv`, sealed by
//! `COMPLETE`. [`Q4Checkpoint::open_verified`] reads those three files,
//! re-hashes `weights.q4` and `snapshot.tsv`, cross-checks every dimension
//! and the factorization, and only then hands out a
//! [`QuantizedSparseAtlas`]. Any partial or altered output is rejected.
//!
//! Read-only: nothing in the checkpoint directory is created or modified.
//! The f32 master (`master.f32`) is not read; a Q4 checkpoint here is an
//! inference artifact, not a resumable training state.
use crate::{
    byte_factorization::ByteFactorization,
    elastic_ngram_paged_sha::{hex, parse_hex, Sha256},
    sparse_gradient_atlas::{QuantizedSparseAtlas, SparseConfig, PAGE_PARAMETERS},
};
use std::{
    error::Error,
    fs::{self, File},
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Snapshot version strings written by `SparseGradientAtlas::checkpoint`.
pub const SNAPSHOT_VERSION_INDEPENDENT_BITS: &str = "SML_SPARSE_GRADIENT_V1";
pub const SNAPSHOT_VERSION_MSB_TREE: &str = "SML_SPARSE_GRADIENT_V2_MSB_TREE";
pub const STREAM_SCHEMA: &str = "SML50M_STREAM_V1";
pub const CONTINUE_SCHEMA: &str = "SML50M_STREAM_CONTINUE_V1";

/// Exact `metrics.tsv` header of a fresh stream fit (24 columns).
pub const STREAM_METRICS_HEADER: &str = "schema\tmanifest_sha256\tcapacity_slots\tlearned_unique\tparameter_updates\ttrain_files\ttrain_bytes\ttrain_events\ttrain_bpc\tdev_files\tdev_bytes\tdev_events\tdev_f32_bpc\tdev_q4_bpc\tq4_minus_f32_bpc\ttrain_elapsed_s\ttrain_events_per_s\tmaster_bytes\tq4_bytes\tfactorization\tq4_sha256\tsnapshot_sha256\ttest\tpublication";
/// Extra columns appended by the eight-epoch continuation (31 in total).
pub const CONTINUE_METRICS_SUFFIX: &str = "\tparent_metrics_sha256\tparent_q4_sha256\tadditional_epochs\tadditional_train_bytes\tcumulative_exposure_bytes\tadditional_parameter_updates\tnew_distinct_touched";

const MAX_SMALL_FILE_BYTES: u64 = 64 * 1024;
const REQUIRED_FILES: [&str; 4] = ["COMPLETE", "metrics.tsv", "snapshot.tsv", "weights.q4"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointSchema {
    Stream,
    Continue,
}

impl CheckpointSchema {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stream => STREAM_SCHEMA,
            Self::Continue => CONTINUE_SCHEMA,
        }
    }
}

/// A verified, self-describing Q4 checkpoint. Metric values are kept as the
/// verbatim strings of `metrics.tsv`; this loader never recomputes them.
#[derive(Clone, Debug, PartialEq)]
pub struct Q4Checkpoint {
    pub root: PathBuf,
    pub schema: CheckpointSchema,
    pub metrics_sha256: String,
    pub manifest_sha256: String,
    pub factorization: ByteFactorization,
    pub config: SparseConfig,
    pub q4_path: PathBuf,
    pub q4_sha256: String,
    pub snapshot_sha256: String,
    pub learned_unique: u64,
    pub parameter_updates: u64,
    pub dev_files: u64,
    pub dev_events: u64,
    pub dev_f32_bpc: String,
    pub dev_q4_bpc: String,
    /// `Some` only for the continuation schema.
    pub parent_q4_sha256: Option<String>,
}

fn regular_file(path: &Path) -> Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(format!("{} must be a regular non-symlink file", path.display()).into());
    }
    Ok(meta)
}

fn small_text(path: &Path) -> Result<String> {
    let meta = regular_file(path)?;
    if meta.len() == 0 || meta.len() > MAX_SMALL_FILE_BYTES {
        return Err(format!("{} is empty or oversized", path.display()).into());
    }
    Ok(String::from_utf8(fs::read(path)?)?)
}

fn file_sha256(path: &Path) -> Result<String> {
    regular_file(path)?;
    let mut reader = BufReader::new(File::open(path)?);
    let mut state = Sha256::default();
    let mut block = vec![0u8; 1024 * 1024];
    loop {
        let n = reader.read(&mut block)?;
        if n == 0 {
            break;
        }
        state.update(&block[..n]);
    }
    Ok(hex(&state.finish()))
}

fn canonical_sha(field: &str) -> Result<&str> {
    let digest = parse_hex(field).ok_or("malformed SHA-256 field")?;
    if hex(&digest) != field {
        return Err("noncanonical SHA-256 field".into());
    }
    Ok(field)
}

fn finite_metric(field: &str) -> Result<String> {
    let value: f64 = field.parse()?;
    if !value.is_finite() || value < 0.0 {
        return Err("nonfinite or negative bits/byte metric".into());
    }
    Ok(field.to_string())
}

impl Q4Checkpoint {
    /// Open and verify `dir`; see the module documentation for every check.
    pub fn open_verified(dir: &Path) -> Result<Self> {
        let info = fs::symlink_metadata(dir)?;
        if !info.is_dir() || info.file_type().is_symlink() {
            return Err("checkpoint must be a regular directory, not a symlink".into());
        }
        for name in REQUIRED_FILES {
            regular_file(&dir.join(name))?;
        }
        // Leftover temporaries mean an interrupted writer: fail closed.
        for entry in fs::read_dir(dir)? {
            let name = entry?.file_name();
            if name.to_string_lossy().ends_with(".tmp") {
                return Err("partial checkpoint: temporary file present".into());
            }
        }

        // COMPLETE seals metrics.tsv and is written last by the trainer.
        let metrics_path = dir.join("metrics.tsv");
        let metrics_sha256 = file_sha256(&metrics_path)?;
        let complete = small_text(&dir.join("COMPLETE"))?;
        let schema = if complete == format!("{STREAM_SCHEMA}\t{metrics_sha256}\n") {
            CheckpointSchema::Stream
        } else if complete == format!("{CONTINUE_SCHEMA}\t{metrics_sha256}\n") {
            CheckpointSchema::Continue
        } else {
            return Err("COMPLETE does not seal the current metrics.tsv".into());
        };

        let metrics = small_text(&metrics_path)?;
        let expected_header = match schema {
            CheckpointSchema::Stream => STREAM_METRICS_HEADER.to_string(),
            CheckpointSchema::Continue => {
                format!("{STREAM_METRICS_HEADER}{CONTINUE_METRICS_SUFFIX}")
            }
        };
        let lines = metrics.split('\n').collect::<Vec<_>>();
        if lines.len() != 3 || !lines[2].is_empty() || lines[0] != expected_header {
            return Err("metrics.tsv header or line structure invalid".into());
        }
        let row = lines[1].split('\t').collect::<Vec<_>>();
        let columns = expected_header.split('\t').count();
        if row.len() != columns || row[0] != schema.as_str() {
            return Err("metrics.tsv row does not match its schema".into());
        }
        if row[22] != "UNOPENED" || row[23] != "NOT_PROMOTED" {
            return Err("metrics.tsv reports an opened TEST split or a promotion".into());
        }
        let metrics_factorization = match row[19] {
            "independent_bits" => ByteFactorization::IndependentBits,
            "msb_tree" => ByteFactorization::MsbTree,
            _ => return Err("unknown factorization in metrics.tsv".into()),
        };

        // snapshot.tsv carries the dimensions the raw Q4 file lacks.
        let snapshot_path = dir.join("snapshot.tsv");
        let snapshot = small_text(&snapshot_path)?;
        let snap = snapshot
            .strip_suffix('\n')
            .ok_or("snapshot.tsv must end with one newline")?
            .split('\t')
            .collect::<Vec<_>>();
        if snap.len() != 10 || snapshot.matches('\n').count() != 1 {
            return Err("snapshot.tsv must be one 10-field record".into());
        }
        let snapshot_factorization = match snap[0] {
            SNAPSHOT_VERSION_INDEPENDENT_BITS => ByteFactorization::IndependentBits,
            SNAPSHOT_VERSION_MSB_TREE => ByteFactorization::MsbTree,
            _ => return Err("unknown snapshot version".into()),
        };
        if snapshot_factorization != metrics_factorization {
            return Err("factorization differs between snapshot.tsv and metrics.tsv".into());
        }
        let config = SparseConfig {
            pages: snap[1].parse()?,
            max_cached_pages: snap[2].parse()?,
            learning_rate: f32::from_bits(snap[3].parse()?),
        }
        .validate()?;
        let snapshot_updates: u64 = snap[5].parse()?;
        let snapshot_unique: u64 = snap[6].parse()?;
        canonical_sha(snap[8])?;
        canonical_sha(snap[9])?;

        // Every dimension recorded in metrics.tsv must follow from the pages.
        let learned_unique: u64 = row[3].parse()?;
        let parameter_updates: u64 = row[4].parse()?;
        if row[2].parse::<u64>()? != config.total_parameters()
            || row[17].parse::<u64>()? != config.train_master_bytes()
            || row[18].parse::<u64>()? != config.packed_q4_bytes()
            || config.total_parameters() != config.pages * PAGE_PARAMETERS as u64
        {
            return Err("page count differs between snapshot.tsv and metrics.tsv".into());
        }
        if learned_unique != snapshot_unique
            || parameter_updates != snapshot_updates
            || learned_unique > config.total_parameters()
            || parameter_updates < learned_unique
        {
            return Err("update accounting differs between snapshot.tsv and metrics.tsv".into());
        }

        // Re-hash the artifacts named by the sealed metrics.
        let q4_path = dir.join("weights.q4");
        if regular_file(&q4_path)?.len() != config.packed_q4_bytes() {
            return Err("weights.q4 length does not match the page count".into());
        }
        let q4_sha256 = canonical_sha(row[20])?.to_string();
        if file_sha256(&q4_path)? != q4_sha256 {
            return Err("weights.q4 SHA-256 differs from metrics.tsv".into());
        }
        let snapshot_sha256 = canonical_sha(row[21])?.to_string();
        if file_sha256(&snapshot_path)? != snapshot_sha256 {
            return Err("snapshot.tsv SHA-256 differs from metrics.tsv".into());
        }
        let parent_q4_sha256 = match schema {
            CheckpointSchema::Stream => None,
            CheckpointSchema::Continue => {
                canonical_sha(row[24])?;
                Some(canonical_sha(row[25])?.to_string())
            }
        };
        Ok(Self {
            root: dir.to_path_buf(),
            schema,
            metrics_sha256,
            manifest_sha256: canonical_sha(row[1])?.to_string(),
            factorization: metrics_factorization,
            config,
            q4_path,
            q4_sha256,
            snapshot_sha256,
            learned_unique,
            parameter_updates,
            dev_files: row[9].parse()?,
            dev_events: row[11].parse()?,
            dev_f32_bpc: finite_metric(row[12])?,
            dev_q4_bpc: finite_metric(row[13])?,
            parent_q4_sha256,
        })
    }

    /// Like [`Self::open_verified`], additionally requiring an exact
    /// pre-registered Q4 SHA-256.
    pub fn open_verified_pinned(dir: &Path, q4_sha256: &str) -> Result<Self> {
        let checkpoint = Self::open_verified(dir)?;
        if checkpoint.q4_sha256 != q4_sha256 {
            return Err("checkpoint Q4 SHA-256 differs from the pinned value".into());
        }
        Ok(checkpoint)
    }

    /// Re-hash `weights.q4` (it may have changed since verification) and open
    /// it with the recorded page count, cache size and factorization.
    pub fn open_atlas(&self) -> Result<QuantizedSparseAtlas> {
        if file_sha256(&self.q4_path)? != self.q4_sha256 {
            return Err("weights.q4 changed after verification".into());
        }
        QuantizedSparseAtlas::open_with_factorization(
            &self.q4_path,
            self.config,
            self.factorization,
        )
    }
}
