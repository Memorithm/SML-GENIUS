//! CYCLOID-SML-2: physically execute and compare two byte-identical page reads.
//!
//! The two sources are a caller-authorized on-disk SML weight artifact and
//! the same previously committed, validated SML ElasticRamRuntime page.
//! Only a MOR-selected page can be profiled. No inference routing, eviction,
//! data mutation, or synthetic faster-than-direct claim occurs here.
//!
//! Timings are observations of these host calls (filesystem read+verification
//! and resident RAM copy+CRC). They do NOT measure block-device I/O, DRAM bus
//! bytes, cache misses, page faults, energy, or a model-level speedup.

use core::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::time::Instant;

use sml_core::{elastic_weights::WeightPrecision, mor::MorDecision, PageId};

use crate::{
    artifact::{crc32c, parse_weight_artifact, WeightArtifactError, WEIGHT_ARTIFACT_HEADER_BYTES},
    ram::{ElasticRamRuntime, WeightArtifactEntry},
};

pub const MAX_CYCLOID_PROFILE_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycloidPageProfile {
    pub page_id: PageId,
    pub representation_version: u64,
    pub precision: WeightPrecision,
    pub payload_bytes: u64,
    /// Filesystem artifact bytes, including the versioned 64-byte header.
    pub artifact_bytes_read: u64,
    /// Monotonic host duration for bounded file read + artifact parse + CRC.
    pub file_read_verify_ns: u128,
    /// Monotonic host duration for RAM payload copy + CRC.
    pub resident_copy_verify_ns: u128,
    /// True means every payload byte matched after independent CRC checks.
    pub byte_exact_match: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycloidPageProfileError {
    InvalidBudget,
    PageNotSelected,
    PageNotResident,
    ArtifactIdentityMismatch,
    NotRegularFile,
    FileReplaced,
    ArtifactLengthMismatch,
    ResidentChecksumMismatch,
    PayloadMismatch,
    Io(io::ErrorKind),
    Artifact(WeightArtifactError),
    ArithmeticOverflow,
}

impl fmt::Display for CycloidPageProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CycloidPageProfileError {}

impl From<io::Error> for CycloidPageProfileError {
    fn from(err: io::Error) -> Self {
        Self::Io(err.kind())
    }
}

impl From<WeightArtifactError> for CycloidPageProfileError {
    fn from(err: WeightArtifactError) -> Self {
        Self::Artifact(err)
    }
}

/// The order of two independently verified, physically executed source reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycloidProbeOrder {
    FileThenRam,
    RamThenFile,
}

impl CycloidProbeOrder {
    #[must_use]
    pub const fn reverse(self) -> Self {
        match self {
            Self::FileThenRam => Self::RamThenFile,
            Self::RamThenFile => Self::FileThenRam,
        }
    }
}

/// Retains the order so results cannot be accidentally presented as randomized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycloidOrderedProfile {
    pub order: CycloidProbeOrder,
    pub profile: CycloidPageProfile,
}

pub const MIN_CYCLOID_BALANCED_PAIRS: u8 = 3;
pub const MAX_CYCLOID_BALANCED_PAIRS: u8 = 64;

/// A bounded deterministic AB/BA collection of actual file/RAM read timings.
/// Every pair contains each order exactly once; between pairs, the leading
/// order alternates. This is **counterbalancing**, not randomization, a
/// hardware performance claim, or a substitute for independent repetitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycloidBalancedProfiles {
    pub pairs: u8,
    pub samples: Vec<CycloidOrderedProfile>,
}

impl CycloidBalancedProfiles {
    /// Strip order metadata only when the caller separately retains the
    /// experimental protocol/provenance. A frozen path selector accepts
    /// CycloidPageProfile observations, not presentation conclusions.
    #[must_use]
    pub fn profile_values(&self) -> Vec<CycloidPageProfile> {
        self.samples.iter().map(|sample| sample.profile).collect()
    }
}

pub fn collect_balanced_mor_page_profiles(
    runtime: &ElasticRamRuntime,
    selected: &MorDecision,
    artifact: &WeightArtifactEntry,
    max_payload_bytes: u64,
    pairs: u8,
) -> Result<CycloidBalancedProfiles, CycloidPageProfileError> {
    if !(MIN_CYCLOID_BALANCED_PAIRS..=MAX_CYCLOID_BALANCED_PAIRS).contains(&pairs) {
        return Err(CycloidPageProfileError::InvalidBudget);
    }
    let mut samples = Vec::with_capacity(usize::from(pairs) * 2);
    for pair in 0..pairs {
        let first = if pair % 2 == 0 {
            CycloidProbeOrder::FileThenRam
        } else {
            CycloidProbeOrder::RamThenFile
        };
        samples.push(profile_same_mor_page_ordered(
            runtime,
            selected,
            artifact,
            max_payload_bytes,
            first,
        )?);
        samples.push(profile_same_mor_page_ordered(
            runtime,
            selected,
            artifact,
            max_payload_bytes,
            first.reverse(),
        )?);
    }
    Ok(CycloidBalancedProfiles { pairs, samples })
}

/// Legacy one-pair call preserved for existing clients. Its order is explicitly
/// FileThenRam; use ordered/balanced collection for comparison evidence.
pub fn profile_same_mor_page(
    runtime: &ElasticRamRuntime,
    selected: &MorDecision,
    artifact: &WeightArtifactEntry,
    max_payload_bytes: u64,
) -> Result<CycloidPageProfile, CycloidPageProfileError> {
    Ok(profile_same_mor_page_ordered(
        runtime,
        selected,
        artifact,
        max_payload_bytes,
        CycloidProbeOrder::FileThenRam,
    )?
    .profile)
}

/// Perform exactly one bounded file read and one resident RAM copy, in the
/// requested order, requiring byte-for-byte agreement and independent CRC32C
/// checks. Both observations include their source's parsing/copy and integrity
/// verification overhead; they are not bare I/O latency counters.
///
/// The path must be authorized by the caller; CRC32C does not authenticate
/// the publisher or attest that the measured samples were made on one host.
pub fn profile_same_mor_page_ordered(
    runtime: &ElasticRamRuntime,
    selected: &MorDecision,
    artifact: &WeightArtifactEntry,
    max_payload_bytes: u64,
    order: CycloidProbeOrder,
) -> Result<CycloidOrderedProfile, CycloidPageProfileError> {
    if max_payload_bytes == 0 || max_payload_bytes > MAX_CYCLOID_PROFILE_PAYLOAD_BYTES {
        return Err(CycloidPageProfileError::InvalidBudget);
    }
    if !selected.selected_experts.contains(&artifact.page_id) {
        return Err(CycloidPageProfileError::PageNotSelected);
    }
    let resident = runtime
        .ram_page(artifact.page_id)
        .ok_or(CycloidPageProfileError::PageNotResident)?;
    if resident.header.page_id != artifact.page_id
        || resident.header.precision != artifact.precision
        || resident.header.representation_version != artifact.representation_version
    {
        return Err(CycloidPageProfileError::ArtifactIdentityMismatch);
    }
    let expected_payload_bytes = resident.header.payload_bytes;
    if expected_payload_bytes > max_payload_bytes {
        return Err(CycloidPageProfileError::InvalidBudget);
    }
    let header_bytes = u64::try_from(WEIGHT_ARTIFACT_HEADER_BYTES)
        .map_err(|_| CycloidPageProfileError::ArithmeticOverflow)?;
    let expected_file_bytes = expected_payload_bytes
        .checked_add(header_bytes)
        .ok_or(CycloidPageProfileError::ArithmeticOverflow)?;

    // lstat rejects a symlink at the moment of this check. Compare the opened
    // file's inode/device on Unix to detect a source swap before the open.
    // Concurrent in-place file mutation is not prevented by these checks.
    let metadata = fs::symlink_metadata(&artifact.path)?;
    if !metadata.file_type().is_file() {
        return Err(CycloidPageProfileError::NotRegularFile);
    }
    if metadata.len() != expected_file_bytes {
        return Err(CycloidPageProfileError::ArtifactLengthMismatch);
    }

    let file_probe = || -> Result<(Vec<u8>, u128), CycloidPageProfileError> {
        let started = Instant::now();
        let file = File::open(&artifact.path)?;
        let opened = file.metadata()?;
        if !opened.is_file() || opened.len() != expected_file_bytes {
            return Err(CycloidPageProfileError::ArtifactLengthMismatch);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
                return Err(CycloidPageProfileError::FileReplaced);
            }
        }
        let limit = expected_file_bytes
            .checked_add(1)
            .ok_or(CycloidPageProfileError::ArithmeticOverflow)?;
        let mut bytes = Vec::new();
        file.take(limit).read_to_end(&mut bytes)?;
        if u64::try_from(bytes.len())
            .map_err(|_| CycloidPageProfileError::ArithmeticOverflow)?
            != expected_file_bytes
        {
            return Err(CycloidPageProfileError::ArtifactLengthMismatch);
        }
        let parsed = parse_weight_artifact(&bytes)?;
        if parsed.header != resident.header {
            return Err(CycloidPageProfileError::ArtifactIdentityMismatch);
        }
        Ok((bytes, started.elapsed().as_nanos()))
    };

    let ram_probe = || -> Result<(Vec<u8>, u128), CycloidPageProfileError> {
        let started = Instant::now();
        let bytes = resident.payload().to_vec();
        let checksum = crc32c(&bytes);
        if checksum != resident.header.payload_crc32c {
            return Err(CycloidPageProfileError::ResidentChecksumMismatch);
        }
        Ok((bytes, started.elapsed().as_nanos()))
    };

    let ((disk_bytes, file_read_verify_ns), (ram_bytes, resident_copy_verify_ns)) = match order {
        CycloidProbeOrder::FileThenRam => (file_probe()?, ram_probe()?),
        CycloidProbeOrder::RamThenFile => {
            let ram = ram_probe()?;
            let disk = file_probe()?;
            (disk, ram)
        }
    };
    if &disk_bytes[WEIGHT_ARTIFACT_HEADER_BYTES..] != ram_bytes.as_slice() {
        return Err(CycloidPageProfileError::PayloadMismatch);
    }

    Ok(CycloidOrderedProfile {
        order,
        profile: CycloidPageProfile {
            page_id: artifact.page_id,
            representation_version: artifact.representation_version,
            precision: artifact.precision,
            payload_bytes: expected_payload_bytes,
            artifact_bytes_read: expected_file_bytes,
            file_read_verify_ns,
            resident_copy_verify_ns,
            byte_exact_match: true,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::{
        artifact::build_weight_artifact,
        ram::{RamMaterializationBudget, WeightArtifactCatalog},
    };
    use sml_core::elastic_weights::{
        ElasticWeightPage, ElasticWeightPlanner, ElasticWeightPolicy, ResidencyTier,
    };

    static TEST_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn selected(page: PageId) -> MorDecision {
        MorDecision {
            active_routers: vec![],
            ranked_experts: vec![],
            selected_experts: vec![page],
            noise_ppm: 0,
            structural_disagreement_ppm: 0,
            expert_mass_threshold_ppm: 0,
            request_precision_escalation: false,
            hysteresis_retained_previous_primary: false,
        }
    }

    fn resident_runtime() -> ElasticRamRuntime {
        let page = ElasticWeightPage::new(
            PageId(0),
            8,
            WeightPrecision::Boolean,
            ResidencyTier::Disk,
            0,
        )
        .unwrap();
        let planner = ElasticWeightPlanner::new(
            vec![page],
            ElasticWeightPolicy {
                ram_limit_bytes: 16,
                vram_limit_bytes: 0,
                max_active_parameters: 8,
                ternary_threshold_ppm: 500_000,
                residual4_threshold_ppm: 900_000,
                vram_threshold_ppm: 1_000_000,
            },
        )
        .unwrap();
        let mut runtime = ElasticRamRuntime::new(planner).unwrap();
        let bytes =
            build_weight_artifact(PageId(0), 0, WeightPrecision::Boolean, 8, &[0xa5]).unwrap();
        let catalog = WeightArtifactCatalog::from_retained_bytes(vec![bytes]).unwrap();
        let plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();
        let staged = runtime
            .stage(&plan, &catalog, RamMaterializationBudget::default())
            .unwrap();
        runtime.commit(staged).unwrap();
        runtime
    }

    fn temporary_artifact() -> (std::path::PathBuf, WeightArtifactEntry) {
        let suffix = TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "sml-cycloid-page-{}-{suffix}.smlwgt",
            std::process::id(),
        ));
        let artifact =
            build_weight_artifact(PageId(0), 0, WeightPrecision::Boolean, 8, &[0xa5]).unwrap();
        fs::write(&path, artifact).unwrap();
        let entry = WeightArtifactEntry {
            page_id: PageId(0),
            representation_version: 0,
            precision: WeightPrecision::Boolean,
            path: path.clone(),
        };
        (path, entry)
    }

    #[test]
    fn real_disk_and_committed_ram_payloads_match_byte_exactly() {
        let runtime = resident_runtime();
        let (path, artifact) = temporary_artifact();
        let result = profile_same_mor_page(&runtime, &selected(PageId(0)), &artifact, 32).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(result.page_id, PageId(0));
        assert_eq!(result.payload_bytes, 1);
        assert_eq!(result.artifact_bytes_read, 65);
        assert!(result.byte_exact_match);
    }

    #[test]
    fn unsanctioned_page_and_bad_budget_fail_before_io() {
        let runtime = resident_runtime();
        let (_, mut artifact) = temporary_artifact();
        let path = artifact.path.clone();
        assert_eq!(
            profile_same_mor_page(&runtime, &selected(PageId(1)), &artifact, 32),
            Err(CycloidPageProfileError::PageNotSelected)
        );
        assert_eq!(
            profile_same_mor_page(&runtime, &selected(PageId(0)), &artifact, 0),
            Err(CycloidPageProfileError::InvalidBudget)
        );
        artifact.representation_version = 42;
        assert_eq!(
            profile_same_mor_page(&runtime, &selected(PageId(0)), &artifact, 32),
            Err(CycloidPageProfileError::ArtifactIdentityMismatch)
        );
        fs::remove_file(path).unwrap();
    }


    #[test]
    fn both_read_orders_execute_and_verify_identical_bytes() {
        let runtime = resident_runtime();
        let (path, artifact) = temporary_artifact();
        let file_first = profile_same_mor_page_ordered(
            &runtime,
            &selected(PageId(0)),
            &artifact,
            32,
            CycloidProbeOrder::FileThenRam,
        )
        .unwrap();
        let ram_first = profile_same_mor_page_ordered(
            &runtime,
            &selected(PageId(0)),
            &artifact,
            32,
            CycloidProbeOrder::RamThenFile,
        )
        .unwrap();
        assert_eq!(file_first.order, CycloidProbeOrder::FileThenRam);
        assert_eq!(ram_first.order, CycloidProbeOrder::RamThenFile);
        assert!(file_first.profile.byte_exact_match);
        assert!(ram_first.profile.byte_exact_match);
        assert_eq!(file_first.profile.artifact_bytes_read, 65);
        assert_eq!(ram_first.profile.artifact_bytes_read, 65);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn balanced_pairs_emit_exactly_both_orders_without_dropping_samples() {
        let runtime = resident_runtime();
        let (path, artifact) = temporary_artifact();
        let batch =
            collect_balanced_mor_page_profiles(&runtime, &selected(PageId(0)), &artifact, 32, 3)
                .unwrap();
        assert_eq!(batch.pairs, 3);
        assert_eq!(batch.samples.len(), 6);
        assert_eq!(
            batch.samples.iter().filter(|item| item.order == CycloidProbeOrder::FileThenRam).count(),
            3
        );
        assert_eq!(
            batch.samples.iter().filter(|item| item.order == CycloidProbeOrder::RamThenFile).count(),
            3
        );
        assert_eq!(batch.profile_values().len(), 6);
        assert!(batch.samples.iter().all(|item| item.profile.byte_exact_match));
        assert_eq!(batch.samples[0].order, CycloidProbeOrder::FileThenRam);
        assert_eq!(batch.samples[1].order, CycloidProbeOrder::RamThenFile);
        assert_eq!(batch.samples[2].order, CycloidProbeOrder::RamThenFile);
        assert_eq!(batch.samples[3].order, CycloidProbeOrder::FileThenRam);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn malformed_pair_budget_fails_without_source_access() {
        let runtime = resident_runtime();
        let (_, artifact) = temporary_artifact();
        let path = artifact.path.clone();
        assert_eq!(
            collect_balanced_mor_page_profiles(&runtime, &selected(PageId(0)), &artifact, 32, 2),
            Err(CycloidPageProfileError::InvalidBudget)
        );
        assert_eq!(
            collect_balanced_mor_page_profiles(&runtime, &selected(PageId(0)), &artifact, 32, 65),
            Err(CycloidPageProfileError::InvalidBudget)
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn corrupt_artifact_fails_without_emitting_a_profile() {
        let runtime = resident_runtime();
        let (path, artifact) = temporary_artifact();
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            profile_same_mor_page(&runtime, &selected(PageId(0)), &artifact, 32),
            Err(CycloidPageProfileError::Artifact(
                WeightArtifactError::PayloadChecksumMismatch { .. }
            ))
        ));
        fs::remove_file(path).unwrap();
    }
}
