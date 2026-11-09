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

use sml_core::{
    elastic_weights::WeightPrecision,
    mor::MorDecision,
    PageId,
};

use crate::{
    artifact::{
        crc32c, parse_weight_artifact, WeightArtifactError, WEIGHT_ARTIFACT_HEADER_BYTES,
    },
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

/// Perform exactly one bounded filesystem read and one RAM copy, on the same
/// retained page identity, and require byte-for-byte agreement. No timing
/// conclusion should be drawn from one observation; caller must run paired
/// repetitions and reversal of read order under RemoteOps for claims.
///
/// The disk path must be independently authorized by the caller. CRC32C is
/// corruption detection, NOT publisher authentication. A changed file or
/// symlink is rejected rather than silently trusted.
pub fn profile_same_mor_page(
    runtime: &ElasticRamRuntime,
    selected: &MorDecision,
    artifact: &WeightArtifactEntry,
    max_payload_bytes: u64,
) -> Result<CycloidPageProfile, CycloidPageProfileError> {
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

    // Use lstat to reject symlink inputs; validate content again after reading
    // because metadata before open is not an authenticity or immutability proof.
    let metadata = fs::symlink_metadata(&artifact.path)?;
    if !metadata.file_type().is_file() {
        return Err(CycloidPageProfileError::NotRegularFile);
    }
    if metadata.len() != expected_file_bytes {
        return Err(CycloidPageProfileError::ArtifactLengthMismatch);
    }

    let file_started = Instant::now();
    let file = File::open(&artifact.path)?;
    let read_limit = expected_file_bytes
        .checked_add(1)
        .ok_or(CycloidPageProfileError::ArithmeticOverflow)?;
    let mut disk_bytes = Vec::new();
    file.take(read_limit).read_to_end(&mut disk_bytes)?;
    if u64::try_from(disk_bytes.len())
        .map_err(|_| CycloidPageProfileError::ArithmeticOverflow)?
        != expected_file_bytes
    {
        return Err(CycloidPageProfileError::ArtifactLengthMismatch);
    }
    let parsed = parse_weight_artifact(&disk_bytes)?;
    let file_read_verify_ns = file_started.elapsed().as_nanos();
    if parsed.header != resident.header {
        return Err(CycloidPageProfileError::ArtifactIdentityMismatch);
    }

    let resident_started = Instant::now();
    let ram_bytes = resident.payload().to_vec();
    let ram_crc = crc32c(&ram_bytes);
    let resident_copy_verify_ns = resident_started.elapsed().as_nanos();
    if ram_crc != resident.header.payload_crc32c {
        return Err(CycloidPageProfileError::ResidentChecksumMismatch);
    }
    if parsed.payload != ram_bytes {
        return Err(CycloidPageProfileError::PayloadMismatch);
    }

    Ok(CycloidPageProfile {
        page_id: artifact.page_id,
        representation_version: artifact.representation_version,
        precision: artifact.precision,
        payload_bytes: expected_payload_bytes,
        artifact_bytes_read: expected_file_bytes,
        file_read_verify_ns,
        resident_copy_verify_ns,
        byte_exact_match: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    use sml_core::elastic_weights::{
        ElasticWeightPage, ElasticWeightPlanner, ElasticWeightPolicy, ResidencyTier,
    };
    use crate::{
        artifact::build_weight_artifact,
        ram::{RamMaterializationBudget, WeightArtifactCatalog},
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
        let bytes = build_weight_artifact(
            PageId(0),
            0,
            WeightPrecision::Boolean,
            8,
            &[0xa5],
        )
        .unwrap();
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
        let artifact = build_weight_artifact(
            PageId(0),
            0,
            WeightPrecision::Boolean,
            8,
            &[0xa5],
        )
        .unwrap();
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
