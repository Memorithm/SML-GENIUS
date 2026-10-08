//! Semantic Boolean Geometry (SBG) reference primitives.
//!
//! SBG represents linguistic/semantic state as packed Boolean fields.  This
//! module is intentionally integer-only: no floating-point representation is
//! required by the reference state or its primitive Boolean operations.

pub mod algebra;
pub mod ambiguity;
pub mod bitfield;
pub mod composition;
pub mod context;
pub mod elastic_regions;
pub mod evidence_learning;
pub mod linguistic_bench;
pub mod metric;
pub mod mor_adaptive_hierarchy;
pub mod mor_h4a_qualification;
pub mod mor_hierarchical;
pub mod mor_hierarchy_majority;
pub mod mor_hierarchy_train;
pub mod mor_routing;
pub mod recurrent;
pub mod simd;
pub mod state;
pub mod store_format;
pub mod structural_learning;
pub mod transactional;

pub use algebra::{
    bind, bundle_majority, inverse_rotate_permute, rotate_permute, unbind, SemanticAlgebraError,
};
pub use ambiguity::{
    derive_ambiguity, update_ambiguity, AmbiguityError, AmbiguityPolicy, AmbiguityTransition,
};
pub use bitfield::{SemanticBitField, SemanticBitFieldError};
pub use composition::{
    compose_relation, SemanticComposition, SemanticCompositionError, SemanticPolarity,
    SemanticRoleKind, SemanticRoleSet,
};
pub use context::{
    derive_context_mask, update_context_mask, ContextMaskError, ContextMaskPolicy,
    ContextMaskUpdate,
};
pub use elastic_regions::{
    SemanticElasticBridge, SemanticElasticError, SemanticElasticPlan, SemanticElasticPolicy,
    SemanticElasticWorkingSet, SemanticWeightRegion, SemanticWeightRegionScore,
};
pub use evidence_learning::{
    commit_semantic_candidates, SemanticCoordinate, SemanticCoordinateAction,
    SemanticEvidenceError, SemanticEvidenceLearningOutcome, SemanticEvidenceLearningReport,
    SemanticEvidenceLedger, SemanticEvidenceObservation, SemanticLearningCandidate,
    SemanticLearningField,
};
pub use linguistic_bench::{
    run_linguistic_micro_benchmark, LinguisticMetric, LinguisticMicroBenchmarkError,
    LinguisticMicroBenchmarkReport, LinguisticProbe, LinguisticProbeResult,
    LINGUISTIC_MICRO_BENCH_BITS,
};
pub use metric::{
    hamming_distance, masked_hamming_distance, masked_score, masked_xnor_similarity,
    xnor_similarity, MaskedSemanticScore, SemanticMetricError,
};
pub use mor_adaptive_hierarchy::{
    SemanticMorAdaptiveExhaustiveControl, SemanticMorAdaptiveHierarchyError,
    SemanticMorAdaptiveHierarchyPolicy, SemanticMorAdaptiveHierarchyReport,
    SemanticMorAdaptiveStageReport, SemanticMorWidenReason,
};
pub use mor_h4a_qualification::{
    SemanticMorH4aQualificationArtifact, SemanticMorH4aQualificationConfig,
    SemanticMorH4aQualificationError, SemanticMorH4aQualificationReport, SemanticMorH4aQualifier,
    SemanticMorH4aRandomControl,
};
pub use mor_hierarchical::{
    SemanticHierarchicalMorBridge, SemanticHierarchicalMorRoute, SemanticMorClusterEvidence,
    SemanticMorClusterSignature, SemanticMorHierarchyAccounting, SemanticMorHierarchyError,
    SemanticMorHierarchyPolicy,
};
pub use mor_hierarchy_majority::{
    SemanticMorMajorityClusterReport, SemanticMorMajorityPrototypeTrainer,
    SemanticMorMajorityTrainingArtifact, SemanticMorMajorityTrainingConfig,
    SemanticMorMajorityTrainingError, SemanticMorMajorityTrainingReport,
};
pub use mor_hierarchy_train::{
    SemanticMorClusterPrototypeReport, SemanticMorHierarchyTrainer,
    SemanticMorHierarchyTrainingArtifact, SemanticMorHierarchyTrainingConfig,
    SemanticMorHierarchyTrainingError, SemanticMorHierarchyTrainingExample,
    SemanticMorHierarchyTrainingReport,
};
pub use mor_routing::{
    SemanticExpertSignature, SemanticMorBridge, SemanticMorError, SemanticMorRoute,
    SemanticMorTopK, SemanticRouterSignature,
};
pub use recurrent::{
    run_semantic_recurrent, SemanticHaltReason, SemanticRecurrentOutcome,
    SemanticRecurrentRunError, SemanticRewriteStep, SemanticRewriteTrace, SemanticRewriter,
    SemanticStateDelta,
};
pub use simd::{
    packed_word4_score, packed_word4_score_16384, qualify_packed_word4, scalar_reference_score,
    SemanticKernelEvidence, SemanticKernelKind, SemanticKernelQualification, SemanticSimdError,
};
pub use state::SemanticState;
pub use store_format::{
    build_semantic_store, SemanticStoreError, SemanticStoreHeader, SemanticStoreIndexEntry,
    SemanticStoreView, SEMANTIC_STORE_HEADER_BYTES, SEMANTIC_STORE_INDEX_ENTRY_BYTES,
    SEMANTIC_STORE_MAGIC, SEMANTIC_STORE_PAGE_ALIGNMENT, SEMANTIC_STORE_VERSION,
};
pub use structural_learning::{
    commit_structural_candidate, SemanticHypothesisPool, SemanticStructuralCandidate,
    SemanticStructuralError, SemanticStructuralEvidenceLedger, SemanticStructuralLearningReport,
    SemanticStructuralMutation, SemanticStructuralObservation, SemanticStructuralProposal,
};

/// Initial experimental semantic width.
///
/// This is a benchmark hypothesis, not a frozen architecture constant.  All
/// core types remain const-generic so later experiments can compare widths.
pub const DEFAULT_SEMANTIC_BITS: usize = 16_384;
