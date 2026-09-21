//! Four-field Semantic Boolean Geometry state.

use super::{SemanticBitField, SemanticBitFieldError};

/// Native SML semantic state.
///
/// - `value`: distributed semantic value field (V)
/// - `meaning_mask`: context-dependent relevance field (M)
/// - `ambiguity`: unresolved/contested semantic coordinates (A)
/// - `routing`: direct semantic routing field (R)
///
/// A single const-generic width makes dimension mismatches unrepresentable
/// between the four fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticState<const BITS: usize> {
    pub value: SemanticBitField<BITS>,
    pub meaning_mask: SemanticBitField<BITS>,
    pub ambiguity: SemanticBitField<BITS>,
    pub routing: SemanticBitField<BITS>,
}

impl<const BITS: usize> SemanticState<BITS> {
    /// Builds a fully clear state.
    pub fn zeroed() -> Result<Self, SemanticBitFieldError> {
        Ok(Self {
            value: SemanticBitField::zeroed()?,
            meaning_mask: SemanticBitField::zeroed()?,
            ambiguity: SemanticBitField::zeroed()?,
            routing: SemanticBitField::zeroed()?,
        })
    }

    /// Logical bits represented across V, M, A and R.
    #[must_use]
    pub const fn payload_bits() -> u128 {
        BITS as u128 * 4
    }

    /// Packed payload bytes across V, M, A and R, excluding container metadata.
    #[must_use]
    pub const fn payload_bytes_ceil() -> u128 {
        Self::payload_bits().div_ceil(8)
    }
}

#[cfg(test)]
mod tests {
    use super::SemanticState;
    use crate::semantic::DEFAULT_SEMANTIC_BITS;

    #[test]
    fn default_hypothesis_is_exactly_eight_kibibytes_of_boolean_payload() {
        type State = SemanticState<DEFAULT_SEMANTIC_BITS>;
        assert_eq!(State::payload_bits(), 65_536);
        assert_eq!(State::payload_bytes_ceil(), 8_192);
    }

    #[test]
    fn all_four_fields_share_one_dimension() {
        let state = SemanticState::<257>::zeroed().expect("valid state");
        assert_eq!(state.value.len(), 257);
        assert_eq!(state.meaning_mask.len(), 257);
        assert_eq!(state.ambiguity.len(), 257);
        assert_eq!(state.routing.len(), 257);
    }
}
