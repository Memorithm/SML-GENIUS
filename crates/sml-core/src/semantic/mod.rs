//! Semantic Boolean Geometry (SBG) reference primitives.
//!
//! SBG represents linguistic/semantic state as packed Boolean fields.  This
//! module is intentionally integer-only: no floating-point representation is
//! required by the reference state or its primitive Boolean operations.

pub mod bitfield;
pub mod state;

pub use bitfield::{SemanticBitField, SemanticBitFieldError};
pub use state::SemanticState;

/// Initial experimental semantic width.
///
/// This is a benchmark hypothesis, not a frozen architecture constant.  All
/// core types remain const-generic so later experiments can compare widths.
pub const DEFAULT_SEMANTIC_BITS: usize = 16_384;
