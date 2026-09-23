//! Transactional disk-to-RAM materialization for native Elastic Weight plans.
//!
//! This slice supports unchanged-precision pages targeting RAM. VRAM targets
//! and precision transitions fail closed rather than being simulated.

use core::fmt;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io,
    path::PathBuf,
};

use sml_core::{
    elastic_weights::{
        ElasticWeightError, ElasticWeightPlan, ElasticWeightPlanner, ResidencyTier, WeightPrecision,
    },
    PageId,
};

use crate::artifact::{
    crc32c, parse_weight_artifact, WeightArtifactError, WeightArtifactHeader,
    WEIGHT_ARTIFACT_HEADER_BYTES,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightArtifactEntry {
    pub page_id: PageId,
    pub representation_version: u64,
    pub precision: WeightPrecision,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ArtifactKey {
    page_id: u32,
    representation_version: u64,
    precision: WeightPrecision,
}

impl From<&WeightArtifactEntry> for ArtifactKey {
    fn from(entry: &WeightArtifactEntry) -> Self {
        Self {
            page_id: entry.page_id.0,
            representation_version: entry.representation_version,
            precision: entry.precision,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightArtifactCatalog {
    entries: BTreeMap<ArtifactKey, PathBuf>,
}

impl WeightArtifactCatalog {
    pub fn new(entries: Vec<WeightArtifactEntry>) -> Result<Self, RamMaterializationError> {
        let mut indexed = BTreeMap::new();
        for entry in entries {
            let key = ArtifactKey::from(&entry);
            if indexed.insert(key, entry.path).is_some() {
                return Err(RamMaterializationError::DuplicateArtifact {
                    page_id: PageId(key.page_id),
                    representation_version: key.representation_version,
                    precision: key.precision,
                });
            }
        }
        Ok(Self { entries: indexed })
    }

    fn path(
        &self,
        page_id: PageId,
        representation_version: u64,
        precision: WeightPrecision,
    ) -> Result<&PathBuf, RamMaterializationError> {
        self.entries
            .get(&ArtifactKey {
                page_id: page_id.0,
                representation_version,
                precision,
            })
            .ok_or(RamMaterializationError::MissingArtifact {
                page_id,
                representation_version,
                precision,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamMaterializationBudget {
    pub max_files_read: usize,
    pub max_total_payload_bytes: u64,
    pub max_total_artifact_bytes: u64,
}

impl Default for RamMaterializationBudget {
    fn default() -> Self {
        Self {
            max_files_read: 4096,
            max_total_payload_bytes: 8 * 1024 * 1024 * 1024,
            max_total_artifact_bytes: 8 * 1024 * 1024 * 1024 + 4096 * 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RamWeightPage {
    pub header: WeightArtifactHeader,
    payload: Vec<u8>,
}

impl RamWeightPage {
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamMaterializationReport {
    pub base_generation: u64,
    pub epoch: u64,
    pub files_read: usize,
    pub artifacts_reused_from_ram: usize,
    pub artifact_bytes_read: u64,
    pub ram_pages: usize,
    pub ram_payload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedRamStage {
    plan: ElasticWeightPlan,
    ram_pages: BTreeMap<u32, RamWeightPage>,
    report: RamMaterializationReport,
}

impl MaterializedRamStage {
    #[must_use]
    pub const fn report(&self) -> RamMaterializationReport {
        self.report
    }

    #[must_use]
    pub fn plan(&self) -> &ElasticWeightPlan {
        &self.plan
    }

    #[must_use]
    pub fn page(&self, page_id: PageId) -> Option<&RamWeightPage> {
        self.ram_pages.get(&page_id.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamCommitEvidence {
    pub generation: u64,
    pub ram_pages: usize,
    pub ram_payload_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamRollbackEvidence {
    pub planner_generation: u64,
    pub discarded_pages: usize,
    pub discarded_payload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElasticRamRuntime {
    planner: ElasticWeightPlanner,
    ram_pages: BTreeMap<u32, RamWeightPage>,
}

impl ElasticRamRuntime {
    pub fn new(planner: ElasticWeightPlanner) -> Result<Self, RamMaterializationError> {
        if planner
            .pages()
            .iter()
            .any(|page| page.active || page.residency != ResidencyTier::Disk)
        {
            return Err(RamMaterializationError::NonColdInitialPlanner);
        }
        Ok(Self {
            planner,
            ram_pages: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn planner(&self) -> &ElasticWeightPlanner {
        &self.planner
    }

    #[must_use]
    pub fn ram_page(&self, page_id: PageId) -> Option<&RamWeightPage> {
        self.ram_pages.get(&page_id.0)
    }

    #[must_use]
    pub fn ram_page_count(&self) -> usize {
        self.ram_pages.len()
    }

    pub fn stage(
        &self,
        plan: &ElasticWeightPlan,
        catalog: &WeightArtifactCatalog,
        budget: RamMaterializationBudget,
    ) -> Result<MaterializedRamStage, RamMaterializationError> {
        plan.validate_contract_v1()?;
        validate_supported_transitions(plan)?;

        if plan.base_generation != self.planner.generation() {
            return Err(RamMaterializationError::StaleStage {
                expected_generation: self.planner.generation(),
                actual_generation: plan.base_generation,
            });
        }

        let mut probe = self.planner.clone();
        probe.commit(plan)?;

        let active_ram = plan
            .transitions
            .iter()
            .filter(|transition| transition.to_active)
            .collect::<Vec<_>>();
        if active_ram.len() > budget.max_files_read.saturating_add(self.ram_pages.len()) {
            return Err(RamMaterializationError::MaterializationBudget);
        }

        let mut staged = BTreeMap::new();
        let mut files_read = 0_usize;
        let mut reused = 0_usize;
        let mut artifact_bytes_read = 0_u64;
        let mut ram_payload_bytes = 0_u64;

        for transition in active_ram {
            if transition.to_residency != ResidencyTier::Ram {
                return Err(RamMaterializationError::UnsupportedTargetResidency {
                    page_id: transition.page_id,
                    residency: transition.to_residency,
                });
            }

            let page = if let Some(existing) = self.ram_pages.get(&transition.page_id.0) {
                validate_ram_page(existing, transition)?;
                reused = reused.saturating_add(1);
                existing.clone()
            } else {
                if files_read >= budget.max_files_read {
                    return Err(RamMaterializationError::MaterializationBudget);
                }
                let path = catalog.path(
                    transition.page_id,
                    transition.to_representation_version,
                    transition.to_precision,
                )?;
                let expected_file_bytes = transition
                    .target_payload_bytes
                    .checked_add(
                        u64::try_from(WEIGHT_ARTIFACT_HEADER_BYTES)
                            .map_err(|_| RamMaterializationError::ArithmeticOverflow)?,
                    )
                    .ok_or(RamMaterializationError::ArithmeticOverflow)?;
                let metadata = fs::metadata(path)?;
                if metadata.len() != expected_file_bytes {
                    return Err(RamMaterializationError::ArtifactFileLength {
                        page_id: transition.page_id,
                        expected: expected_file_bytes,
                        actual: metadata.len(),
                    });
                }
                let next_artifact_bytes = artifact_bytes_read
                    .checked_add(expected_file_bytes)
                    .ok_or(RamMaterializationError::ArithmeticOverflow)?;
                if next_artifact_bytes > budget.max_total_artifact_bytes {
                    return Err(RamMaterializationError::MaterializationBudget);
                }
                let bytes = fs::read(path)?;
                let parsed = parse_weight_artifact(&bytes)?;
                validate_artifact_header(&parsed.header, transition)?;
                artifact_bytes_read = next_artifact_bytes;
                files_read = files_read.saturating_add(1);
                RamWeightPage {
                    header: parsed.header,
                    payload: parsed.payload.to_vec(),
                }
            };

            ram_payload_bytes = ram_payload_bytes
                .checked_add(page.header.payload_bytes)
                .ok_or(RamMaterializationError::ArithmeticOverflow)?;
            if ram_payload_bytes > budget.max_total_payload_bytes {
                return Err(RamMaterializationError::MaterializationBudget);
            }
            if staged.insert(transition.page_id.0, page).is_some() {
                return Err(RamMaterializationError::DuplicateActivePage(
                    transition.page_id,
                ));
            }
        }

        if ram_payload_bytes != plan.ram_bytes {
            return Err(RamMaterializationError::RamByteMismatch {
                planned: plan.ram_bytes,
                staged: ram_payload_bytes,
            });
        }

        let stage = MaterializedRamStage {
            plan: plan.clone(),
            ram_pages: staged,
            report: RamMaterializationReport {
                base_generation: plan.base_generation,
                epoch: plan.epoch,
                files_read,
                artifacts_reused_from_ram: reused,
                artifact_bytes_read,
                ram_pages: plan
                    .transitions
                    .iter()
                    .filter(|transition| transition.to_active)
                    .count(),
                ram_payload_bytes,
            },
        };
        self.verify_stage(&stage)?;
        Ok(stage)
    }

    pub fn verify_stage(
        &self,
        stage: &MaterializedRamStage,
    ) -> Result<(), RamMaterializationError> {
        stage.plan.validate_contract_v1()?;
        validate_supported_transitions(&stage.plan)?;

        let mut probe = self.planner.clone();
        probe.commit(&stage.plan)?;

        let expected = stage
            .plan
            .transitions
            .iter()
            .filter(|transition| transition.to_active)
            .map(|transition| transition.page_id.0)
            .collect::<BTreeSet<_>>();
        let actual = stage.ram_pages.keys().copied().collect::<BTreeSet<_>>();
        if expected != actual {
            return Err(RamMaterializationError::StagePageSetMismatch);
        }

        let mut payload_bytes = 0_u64;
        for transition in stage
            .plan
            .transitions
            .iter()
            .filter(|transition| transition.to_active)
        {
            let page = stage
                .ram_pages
                .get(&transition.page_id.0)
                .ok_or(RamMaterializationError::StagePageSetMismatch)?;
            validate_ram_page(page, transition)?;
            payload_bytes = payload_bytes
                .checked_add(page.header.payload_bytes)
                .ok_or(RamMaterializationError::ArithmeticOverflow)?;
        }
        if payload_bytes != stage.plan.ram_bytes
            || payload_bytes != stage.report.ram_payload_bytes
        {
            return Err(RamMaterializationError::RamByteMismatch {
                planned: stage.plan.ram_bytes,
                staged: payload_bytes,
            });
        }
        Ok(())
    }

    pub fn commit(
        &mut self,
        stage: MaterializedRamStage,
    ) -> Result<RamCommitEvidence, RamMaterializationError> {
        self.verify_stage(&stage)?;
        self.planner.commit(&stage.plan)?;
        self.ram_pages = stage.ram_pages;
        Ok(RamCommitEvidence {
            generation: self.planner.generation(),
            ram_pages: self.ram_pages.len(),
            ram_payload_bytes: stage.report.ram_payload_bytes,
        })
    }

    #[must_use]
    pub fn rollback(&self, stage: MaterializedRamStage) -> RamRollbackEvidence {
        RamRollbackEvidence {
            planner_generation: self.planner.generation(),
            discarded_pages: stage.ram_pages.len(),
            discarded_payload_bytes: stage.report.ram_payload_bytes,
        }
    }
}

fn validate_supported_transitions(
    plan: &ElasticWeightPlan,
) -> Result<(), RamMaterializationError> {
    for transition in &plan.transitions {
        if transition.from_precision != transition.to_precision {
            return Err(RamMaterializationError::UnsupportedPrecisionTransition {
                page_id: transition.page_id,
                from: transition.from_precision,
                to: transition.to_precision,
            });
        }
        if transition.to_active && transition.to_residency != ResidencyTier::Ram {
            return Err(RamMaterializationError::UnsupportedTargetResidency {
                page_id: transition.page_id,
                residency: transition.to_residency,
            });
        }
    }
    Ok(())
}

fn validate_artifact_header(
    header: &WeightArtifactHeader,
    transition: &sml_core::elastic_weights::ElasticWeightTransition,
) -> Result<(), RamMaterializationError> {
    if header.page_id != transition.page_id
        || header.precision != transition.to_precision
        || header.representation_version != transition.to_representation_version
        || header.parameters != transition.parameters
        || header.payload_bytes != transition.target_payload_bytes
    {
        return Err(RamMaterializationError::ArtifactTransitionMismatch(
            transition.page_id,
        ));
    }
    Ok(())
}

fn validate_ram_page(
    page: &RamWeightPage,
    transition: &sml_core::elastic_weights::ElasticWeightTransition,
) -> Result<(), RamMaterializationError> {
    validate_artifact_header(&page.header, transition)?;
    let actual_crc = crc32c(&page.payload);
    if actual_crc != page.header.payload_crc32c {
        return Err(RamMaterializationError::RamChecksumMismatch {
            page_id: transition.page_id,
            expected: page.header.payload_crc32c,
            actual: actual_crc,
        });
    }
    let actual_bytes =
        u64::try_from(page.payload.len()).map_err(|_| RamMaterializationError::ArithmeticOverflow)?;
    if actual_bytes != page.header.payload_bytes {
        return Err(RamMaterializationError::RamByteMismatch {
            planned: page.header.payload_bytes,
            staged: actual_bytes,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RamMaterializationError {
    NonColdInitialPlanner,
    DuplicateArtifact {
        page_id: PageId,
        representation_version: u64,
        precision: WeightPrecision,
    },
    MissingArtifact {
        page_id: PageId,
        representation_version: u64,
        precision: WeightPrecision,
    },
    UnsupportedPrecisionTransition {
        page_id: PageId,
        from: WeightPrecision,
        to: WeightPrecision,
    },
    UnsupportedTargetResidency {
        page_id: PageId,
        residency: ResidencyTier,
    },
    StaleStage {
        expected_generation: u64,
        actual_generation: u64,
    },
    ArtifactFileLength {
        page_id: PageId,
        expected: u64,
        actual: u64,
    },
    ArtifactTransitionMismatch(PageId),
    DuplicateActivePage(PageId),
    StagePageSetMismatch,
    RamChecksumMismatch {
        page_id: PageId,
        expected: u32,
        actual: u32,
    },
    RamByteMismatch {
        planned: u64,
        staged: u64,
    },
    MaterializationBudget,
    ArithmeticOverflow,
    Io(io::ErrorKind),
    Artifact(WeightArtifactError),
    Elastic(ElasticWeightError),
}

impl From<io::Error> for RamMaterializationError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

impl From<WeightArtifactError> for RamMaterializationError {
    fn from(error: WeightArtifactError) -> Self {
        Self::Artifact(error)
    }
}

impl From<ElasticWeightError> for RamMaterializationError {
    fn from(error: ElasticWeightError) -> Self {
        Self::Elastic(error)
    }
}

impl fmt::Display for RamMaterializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RamMaterializationError {}
