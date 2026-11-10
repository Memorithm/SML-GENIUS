//! CYCLOID-3: freeze calibrated access choices and execute verified page reads.
//!
//! This opt-in SML runtime adapter connects the bounded Cycloid-1 offline
//! graph planner to Cycloid-2 verified disk/RAM access observations. It binds
//! exact MOR page, representation version, precision, CRC and committed RAM
//! generation. A frozen choice is executed without any graph search.
//!
//! Caller-supplied timing samples and relay/dispatch estimates remain
//! *untrusted cost evidence*, not physical speed measurements or proof of
//! better model throughput. No default MOR routing, model weights or memory
//! residency are modified.

use core::fmt;
use std::{
    fs::{self, File},
    io::{self, Read},
    path::PathBuf,
    time::Instant,
};

use sml_core::{
    cycloid_route::{
        CycloidBudget, CycloidEdge, CycloidError, CycloidGraph, CycloidNode, CycloidNodeId,
        CycloidPlan,
    },
    mor::MorDecision,
    PageId,
};

use crate::{
    artifact::{
        crc32c, parse_weight_artifact, WeightArtifactError, WeightArtifactHeader,
        WEIGHT_ARTIFACT_HEADER_BYTES,
    },
    cycloid_access::{
        profile_same_mor_page, CycloidPageProfile, CycloidPageProfileError,
        MAX_CYCLOID_PROFILE_PAYLOAD_BYTES,
    },
    ram::{ElasticRamRuntime, WeightArtifactEntry},
};

pub const CYCLOID_FROZEN_ACCESS_CONTRACT: &str = "sml.cycloid.frozen-access@1.0.0";
pub const MIN_CYCLOID_PROFILE_SAMPLES: usize = 5;
pub const MAX_CYCLOID_PROFILE_SAMPLES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycloidAccessMode {
    /// Read the original authorized SMLWGT01 artifact from the filesystem.
    VerifiedFile,
    /// Copy the currently committed page from RAM and recheck its CRC.
    VerifiedResident,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycloidFrozenCostEvidence {
    pub samples: usize,
    pub file_median_ns: u64,
    pub file_p95_ns: u64,
    pub resident_median_ns: u64,
    pub resident_p95_ns: u64,
    /// Caller-supplied, separately measured route/address lookup cost.
    pub relay_lookup_ns: u64,
    /// Caller-supplied, separately measured online dispatch overhead.
    pub online_dispatch_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenCycloidAccess {
    contract: &'static str,
    expected_header: WeightArtifactHeader,
    expected_file_bytes: u64,
    planner_generation: u64,
    source_path: PathBuf,
    mode: CycloidAccessMode,
    plan: CycloidPlan,
    cost: CycloidFrozenCostEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycloidAccessRead {
    pub page_id: PageId,
    pub mode: CycloidAccessMode,
    pub payload: Vec<u8>,
    /// Logical bytes obtained by reading the source file, including header.
    pub file_bytes_read: u64,
    /// Logical bytes explicitly copied from committed RAM.
    pub resident_bytes_copied: u64,
    /// Measured duration of this one access (not system/DRAM I/O telemetry).
    pub host_access_ns: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycloidDispatchError {
    InvalidSampleCount,
    InvalidSample,
    InvalidRelayCost,
    PageNotSelected,
    PageNotResident,
    IdentityMismatch,
    StalePlannerGeneration { expected: u64, actual: u64 },
    InvalidFileType,
    FileReplaced,
    ArtifactLengthMismatch,
    ResidentChecksumMismatch,
    ArithmeticOverflow,
    Graph(CycloidError),
    Profile(CycloidPageProfileError),
    Artifact(WeightArtifactError),
    Io(io::ErrorKind),
}

impl fmt::Display for CycloidDispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CycloidDispatchError {}

impl From<CycloidError> for CycloidDispatchError {
    fn from(value: CycloidError) -> Self {
        Self::Graph(value)
    }
}

impl From<CycloidPageProfileError> for CycloidDispatchError {
    fn from(value: CycloidPageProfileError) -> Self {
        Self::Profile(value)
    }
}

impl From<WeightArtifactError> for CycloidDispatchError {
    fn from(value: WeightArtifactError) -> Self {
        Self::Artifact(value)
    }
}

impl From<io::Error> for CycloidDispatchError {
    fn from(value: io::Error) -> Self {
        Self::Io(value.kind())
    }
}

impl FrozenCycloidAccess {
    /// Freeze a deterministic direct-file versus two-hop RAM access decision.
    ///
    /// At least five bounded, byte-verified observations of the same page are
    /// required. Upper medians (and p95 evidence) are computed without
    /// allocation proportional to page contents. Explicit positive lookup and
    /// dispatch costs are supplied by the caller; the profiler does not measure
    /// them, and they must never be invented or labeled device measurements.
    ///
    /// The exact disk/RAM contents are reverified once during this *offline*
    /// freeze. The bounded graph is NOT searched at execution time.
    pub fn freeze(
        runtime: &ElasticRamRuntime,
        decision: &MorDecision,
        artifact: &WeightArtifactEntry,
        observations: &[CycloidPageProfile],
        budget: CycloidBudget,
        relay_lookup_ns: u64,
        online_dispatch_ns: u64,
    ) -> Result<Self, CycloidDispatchError> {
        if !(MIN_CYCLOID_PROFILE_SAMPLES..=MAX_CYCLOID_PROFILE_SAMPLES)
            .contains(&observations.len())
        {
            return Err(CycloidDispatchError::InvalidSampleCount);
        }
        if relay_lookup_ns == 0 {
            return Err(CycloidDispatchError::InvalidRelayCost);
        }
        if !decision.selected_experts.contains(&artifact.page_id) {
            return Err(CycloidDispatchError::PageNotSelected);
        }

        let resident = runtime
            .ram_page(artifact.page_id)
            .ok_or(CycloidDispatchError::PageNotResident)?;
        let header = resident.header;
        if header.page_id != artifact.page_id
            || header.precision != artifact.precision
            || header.representation_version != artifact.representation_version
        {
            return Err(CycloidDispatchError::IdentityMismatch);
        }
        let expected_file_bytes = header
            .payload_bytes
            .checked_add(
                u64::try_from(WEIGHT_ARTIFACT_HEADER_BYTES)
                    .map_err(|_| CycloidDispatchError::ArithmeticOverflow)?,
            )
            .ok_or(CycloidDispatchError::ArithmeticOverflow)?;
        if header.payload_bytes == 0 || header.payload_bytes > MAX_CYCLOID_PROFILE_PAYLOAD_BYTES {
            return Err(CycloidDispatchError::InvalidSample);
        }

        let mut file_ns = Vec::with_capacity(observations.len());
        let mut resident_ns = Vec::with_capacity(observations.len());
        for sample in observations {
            if sample.page_id != header.page_id
                || sample.precision != header.precision
                || sample.representation_version != header.representation_version
                || sample.payload_bytes != header.payload_bytes
                || sample.artifact_bytes_read != expected_file_bytes
                || !sample.byte_exact_match
                || sample.file_read_verify_ns == 0
                || sample.resident_copy_verify_ns == 0
            {
                return Err(CycloidDispatchError::InvalidSample);
            }
            file_ns.push(
                u64::try_from(sample.file_read_verify_ns)
                    .map_err(|_| CycloidDispatchError::ArithmeticOverflow)?,
            );
            resident_ns.push(
                u64::try_from(sample.resident_copy_verify_ns)
                    .map_err(|_| CycloidDispatchError::ArithmeticOverflow)?,
            );
        }
        file_ns.sort_unstable();
        resident_ns.sort_unstable();
        let middle = file_ns.len() / 2;
        let p95_index = (file_ns.len() * 95).div_ceil(100) - 1;

        // Revalidate source identity with a real bounded disk read and resident
        // RAM copy. The previous samples alone do not authenticate a file.
        profile_same_mor_page(runtime, decision, artifact, header.payload_bytes)?;

        let graph = CycloidGraph::new(
            vec![
                CycloidNode::Entry,
                CycloidNode::Relay,
                CycloidNode::Page(header.page_id),
            ],
            vec![
                CycloidEdge {
                    from: CycloidNodeId(0),
                    to: CycloidNodeId(2),
                    estimated_ns: file_ns[middle],
                    estimated_read_bytes: expected_file_bytes,
                },
                CycloidEdge {
                    from: CycloidNodeId(0),
                    to: CycloidNodeId(1),
                    estimated_ns: relay_lookup_ns,
                    estimated_read_bytes: 0,
                },
                CycloidEdge {
                    from: CycloidNodeId(1),
                    to: CycloidNodeId(2),
                    estimated_ns: resident_ns[middle],
                    estimated_read_bytes: header.payload_bytes,
                },
            ],
        )?;
        let plan = graph.plan_for_mor(decision, header.page_id, budget, online_dispatch_ns)?;
        let mode = if plan.indirect_selected {
            CycloidAccessMode::VerifiedResident
        } else {
            CycloidAccessMode::VerifiedFile
        };
        Ok(Self {
            contract: CYCLOID_FROZEN_ACCESS_CONTRACT,
            expected_header: header,
            expected_file_bytes,
            planner_generation: runtime.planner().generation(),
            source_path: artifact.path.clone(),
            mode,
            plan,
            cost: CycloidFrozenCostEvidence {
                samples: observations.len(),
                file_median_ns: file_ns[middle],
                file_p95_ns: file_ns[p95_index],
                resident_median_ns: resident_ns[middle],
                resident_p95_ns: resident_ns[p95_index],
                relay_lookup_ns,
                online_dispatch_ns,
            },
        })
    }

    #[must_use]
    pub const fn mode(&self) -> CycloidAccessMode {
        self.mode
    }

    #[must_use]
    pub fn plan(&self) -> &CycloidPlan {
        &self.plan
    }

    #[must_use]
    pub const fn cost_evidence(&self) -> CycloidFrozenCostEvidence {
        self.cost
    }

    #[must_use]
    pub const fn planner_generation(&self) -> u64 {
        self.planner_generation
    }

    /// Execute the frozen selection with no graph search or profile refresh.
    /// The exact page, committed-generation and versioned artifact header are
    /// still checked on every call, and a corruption or drift fails closed.
    ///
    /// This is deliberately opt-in; a correct isolated read is NOT evidence
    /// of a faster end-to-end SML model.
    pub fn read(
        &self,
        runtime: &ElasticRamRuntime,
        decision: &MorDecision,
    ) -> Result<CycloidAccessRead, CycloidDispatchError> {
        if self.contract != CYCLOID_FROZEN_ACCESS_CONTRACT {
            return Err(CycloidDispatchError::IdentityMismatch);
        }
        let page = self.expected_header.page_id;
        if !decision.selected_experts.contains(&page) {
            return Err(CycloidDispatchError::PageNotSelected);
        }
        let current_generation = runtime.planner().generation();
        if current_generation != self.planner_generation {
            return Err(CycloidDispatchError::StalePlannerGeneration {
                expected: self.planner_generation,
                actual: current_generation,
            });
        }
        let resident = runtime
            .ram_page(page)
            .ok_or(CycloidDispatchError::PageNotResident)?;
        if resident.header != self.expected_header {
            return Err(CycloidDispatchError::IdentityMismatch);
        }

        let started = Instant::now();
        let (payload, file_bytes_read, resident_bytes_copied) = match self.mode {
            CycloidAccessMode::VerifiedResident => {
                let payload = resident.payload().to_vec();
                if crc32c(&payload) != self.expected_header.payload_crc32c {
                    return Err(CycloidDispatchError::ResidentChecksumMismatch);
                }
                (payload, 0, self.expected_header.payload_bytes)
            }
            CycloidAccessMode::VerifiedFile => {
                let metadata = fs::symlink_metadata(&self.source_path)?;
                if !metadata.file_type().is_file() {
                    return Err(CycloidDispatchError::InvalidFileType);
                }
                if metadata.len() != self.expected_file_bytes {
                    return Err(CycloidDispatchError::ArtifactLengthMismatch);
                }
                let file = File::open(&self.source_path)?;
                let opened = file.metadata()?;
                if !opened.is_file() || opened.len() != self.expected_file_bytes {
                    return Err(CycloidDispatchError::ArtifactLengthMismatch);
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
                        return Err(CycloidDispatchError::FileReplaced);
                    }
                }
                let mut artifact_bytes = Vec::new();
                let max_read = self
                    .expected_file_bytes
                    .checked_add(1)
                    .ok_or(CycloidDispatchError::ArithmeticOverflow)?;
                file.take(max_read).read_to_end(&mut artifact_bytes)?;
                if u64::try_from(artifact_bytes.len())
                    .map_err(|_| CycloidDispatchError::ArithmeticOverflow)?
                    != self.expected_file_bytes
                {
                    return Err(CycloidDispatchError::ArtifactLengthMismatch);
                }
                let parsed = parse_weight_artifact(&artifact_bytes)?;
                if parsed.header != self.expected_header {
                    return Err(CycloidDispatchError::IdentityMismatch);
                }
                (parsed.payload.to_vec(), self.expected_file_bytes, 0)
            }
        };
        if u64::try_from(payload.len()).map_err(|_| CycloidDispatchError::ArithmeticOverflow)?
            != self.expected_header.payload_bytes
        {
            return Err(CycloidDispatchError::IdentityMismatch);
        }
        Ok(CycloidAccessRead {
            page_id: page,
            mode: self.mode,
            payload,
            file_bytes_read,
            resident_bytes_copied,
            host_access_ns: started.elapsed().as_nanos(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU64, Ordering};

    use sml_core::elastic_weights::{
        ElasticWeightPage, ElasticWeightPlanner, ElasticWeightPolicy, ResidencyTier,
        WeightPrecision,
    };

    use crate::{
        artifact::build_weight_artifact,
        ram::{RamMaterializationBudget, WeightArtifactCatalog},
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn runtime() -> ElasticRamRuntime {
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
        let artifact =
            build_weight_artifact(PageId(0), 0, WeightPrecision::Boolean, 8, &[0xa5]).unwrap();
        let catalog = WeightArtifactCatalog::from_retained_bytes(vec![artifact]).unwrap();
        let plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();
        let stage = runtime
            .stage(&plan, &catalog, RamMaterializationBudget::default())
            .unwrap();
        runtime.commit(stage).unwrap();
        runtime
    }

    fn authorized_file() -> WeightArtifactEntry {
        let suffix = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "sml-cycloid-3-{}-{suffix}.smlwgt",
            std::process::id()
        ));
        fs::write(
            &path,
            build_weight_artifact(PageId(0), 0, WeightPrecision::Boolean, 8, &[0xa5]).unwrap(),
        )
        .unwrap();
        WeightArtifactEntry {
            page_id: PageId(0),
            representation_version: 0,
            precision: WeightPrecision::Boolean,
            path,
        }
    }

    fn mor(page: PageId) -> MorDecision {
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

    fn samples(file_ns: u128, resident_ns: u128) -> [CycloidPageProfile; 5] {
        [CycloidPageProfile {
            page_id: PageId(0),
            representation_version: 0,
            precision: WeightPrecision::Boolean,
            payload_bytes: 1,
            artifact_bytes_read: 65,
            file_read_verify_ns: file_ns,
            resident_copy_verify_ns: resident_ns,
            byte_exact_match: true,
        }; 5]
    }

    fn budget() -> CycloidBudget {
        CycloidBudget {
            max_hops: 3,
            max_expansions: 20,
            max_read_bytes: 100,
        }
    }

    #[test]
    fn longer_ram_route_freezes_and_reads_original_payload() {
        let resident = runtime();
        let artifact = authorized_file();
        let choice = FrozenCycloidAccess::freeze(
            &resident,
            &mor(PageId(0)),
            &artifact,
            &samples(1_000, 100),
            budget(),
            10,
            20,
        )
        .unwrap();
        assert_eq!(choice.mode(), CycloidAccessMode::VerifiedResident);
        assert!(choice.plan().indirect_selected);
        assert_eq!(choice.cost_evidence().samples, 5);
        let read = choice.read(&resident, &mor(PageId(0))).unwrap();
        assert_eq!(read.payload, vec![0xa5]);
        assert_eq!(read.file_bytes_read, 0);
        assert_eq!(read.resident_bytes_copied, 1);
        fs::remove_file(artifact.path).unwrap();
    }

    #[test]
    fn cheaper_direct_file_is_executed_and_corruption_fails_closed() {
        let resident = runtime();
        let artifact = authorized_file();
        let choice = FrozenCycloidAccess::freeze(
            &resident,
            &mor(PageId(0)),
            &artifact,
            &samples(50, 100),
            budget(),
            10,
            20,
        )
        .unwrap();
        assert_eq!(choice.mode(), CycloidAccessMode::VerifiedFile);
        let read = choice.read(&resident, &mor(PageId(0))).unwrap();
        assert_eq!(read.payload, vec![0xa5]);
        assert_eq!(read.file_bytes_read, 65);
        let mut bytes = fs::read(&artifact.path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&artifact.path, bytes).unwrap();
        assert!(matches!(
            choice.read(&resident, &mor(PageId(0))),
            Err(CycloidDispatchError::Artifact(
                WeightArtifactError::PayloadChecksumMismatch { .. }
            ))
        ));
        fs::remove_file(artifact.path).unwrap();
    }

    #[test]
    fn forged_profiles_and_mor_reselection_fail_closed() {
        let resident = runtime();
        let artifact = authorized_file();
        let mut forged = samples(1_000, 100);
        forged[0].byte_exact_match = false;
        assert_eq!(
            FrozenCycloidAccess::freeze(
                &resident,
                &mor(PageId(0)),
                &artifact,
                &forged,
                budget(),
                10,
                20,
            ),
            Err(CycloidDispatchError::InvalidSample)
        );
        let choice = FrozenCycloidAccess::freeze(
            &resident,
            &mor(PageId(0)),
            &artifact,
            &samples(1_000, 100),
            budget(),
            10,
            20,
        )
        .unwrap();
        assert_eq!(
            choice.read(&resident, &mor(PageId(1))),
            Err(CycloidDispatchError::PageNotSelected)
        );
        fs::remove_file(artifact.path).unwrap();
    }

    #[test]
    fn committed_generation_change_invalidates_frozen_access() {
        let mut resident = runtime();
        let artifact = authorized_file();
        let choice = FrozenCycloidAccess::freeze(
            &resident,
            &mor(PageId(0)),
            &artifact,
            &samples(1_000, 100),
            budget(),
            10,
            20,
        )
        .unwrap();
        let catalog = WeightArtifactCatalog::from_retained_bytes(vec![]).unwrap();
        let plan = resident.planner().plan_cycle(&[PageId(0)], 2).unwrap();
        let stage = resident
            .stage(&plan, &catalog, RamMaterializationBudget::default())
            .unwrap();
        resident.commit(stage).unwrap();
        assert!(matches!(
            choice.read(&resident, &mor(PageId(0))),
            Err(CycloidDispatchError::StalePlannerGeneration { .. })
        ));
        fs::remove_file(artifact.path).unwrap();
    }

    #[test]
    fn insufficient_samples_or_unknown_relay_cost_are_rejected() {
        let resident = runtime();
        let artifact = authorized_file();
        assert_eq!(
            FrozenCycloidAccess::freeze(
                &resident,
                &mor(PageId(0)),
                &artifact,
                &samples(1_000, 100)[..4],
                budget(),
                10,
                20,
            ),
            Err(CycloidDispatchError::InvalidSampleCount)
        );
        assert_eq!(
            FrozenCycloidAccess::freeze(
                &resident,
                &mor(PageId(0)),
                &artifact,
                &samples(1_000, 100),
                budget(),
                0,
                20,
            ),
            Err(CycloidDispatchError::InvalidRelayCost)
        );
        fs::remove_file(artifact.path).unwrap();
    }
}
