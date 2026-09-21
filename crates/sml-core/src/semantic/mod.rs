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
pub mod metric;
pub mod mor_routing;
pub mod state;

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
pub use metric::{
    hamming_distance, masked_hamming_distance, masked_score, masked_xnor_similarity,
    xnor_similarity, MaskedSemanticScore, SemanticMetricError,
};
pub use mor_routing::{
    SemanticExpertSignature, SemanticMorBridge, SemanticMorError, SemanticMorRoute,
    SemanticMorTopK, SemanticRouterSignature,
};
pub use state::SemanticState;

/// Initial experimental semantic width.
///
/// This is a benchmark hypothesis, not a frozen architecture constant.  All
/// core types remain const-generic so later experiments can compare widths.
pub const DEFAULT_SEMANTIC_BITS: usize = 16_384;
