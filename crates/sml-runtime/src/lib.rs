#![forbid(unsafe_code)]

//! Native physical-runtime slices for SML-GENIUS.
//!
//! D1f-a implements verified disk-to-RAM staging for unchanged-precision Elastic
//! Weight pages. It does not emulate VRAM and does not synthesize richer weights.

pub mod artifact;
pub mod ram;

pub use artifact::{
    build_weight_artifact, parse_weight_artifact, ParsedWeightArtifact, WeightArtifactError,
    WeightArtifactHeader, WEIGHT_ARTIFACT_HEADER_BYTES, WEIGHT_ARTIFACT_MAGIC,
    WEIGHT_ARTIFACT_VERSION,
};
pub use ram::{
    ElasticRamRuntime, MaterializedRamStage, RamCommitEvidence, RamMaterializationBudget,
    RamMaterializationError, RamMaterializationReport, RamRollbackEvidence, RamWeightPage,
    WeightArtifactCatalog, WeightArtifactEntry,
};
