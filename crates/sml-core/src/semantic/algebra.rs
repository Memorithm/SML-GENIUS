//! Native Boolean algebra for Semantic Boolean Geometry.
//!
//! This slice provides deterministic role binding, exact unbinding, reversible
//! rotation permutation, and odd-cardinality majority bundling. It is a
//! reference semantic algebra, not yet a performance-optimized implementation.

use core::fmt;

use super::{SemanticBitField, SemanticBitFieldError};

/// Applies a deterministic cyclic permutation to a semantic field.
///
/// The permutation is a logical-bit rotation and is exactly reversible for any
/// non-zero field width, including widths that are not multiples of 64.
pub fn rotate_permute<const BITS: usize>(
    input: &SemanticBitField<BITS>,
    shift: usize,
) -> Result<SemanticBitField<BITS>, SemanticAlgebraError> {
    let mut output = SemanticBitField::zeroed()?;
    let shift = shift % BITS;

    for source in 0..BITS {
        if input.get(source)? {
            output.set((source + shift) % BITS, true)?;
        }
    }
    Ok(output)
}

/// Applies the inverse cyclic permutation.
pub fn inverse_rotate_permute<const BITS: usize>(
    input: &SemanticBitField<BITS>,
    shift: usize,
) -> Result<SemanticBitField<BITS>, SemanticAlgebraError> {
    let shift = shift % BITS;
    let inverse = if shift == 0 { 0 } else { BITS - shift };
    rotate_permute(input, inverse)
}

/// Binds a semantic value to a role key.
///
/// Reference contract: BIND(role, value) = P_role(value) XOR role,
/// where P_role is a deterministic role-derived cyclic permutation.
pub fn bind<const BITS: usize>(
    role: &SemanticBitField<BITS>,
    value: &SemanticBitField<BITS>,
) -> Result<SemanticBitField<BITS>, SemanticAlgebraError> {
    let mut bound = rotate_permute(value, role_rotation(role))?;
    bound.xor_assign(role);
    Ok(bound)
}

/// Recovers a value from a binding when supplied with the same role key.
///
/// For a valid binding produced by bind, this operation is exact.
pub fn unbind<const BITS: usize>(
    role: &SemanticBitField<BITS>,
    bound: &SemanticBitField<BITS>,
) -> Result<SemanticBitField<BITS>, SemanticAlgebraError> {
    let mut permuted = bound.clone();
    permuted.xor_assign(role);
    inverse_rotate_permute(&permuted, role_rotation(role))
}

/// Bundles an odd number of fields by coordinate-wise strict majority.
///
/// Odd cardinality is required so no tie-breaking rule is hidden inside the
/// primitive. A later learned bundling policy may deliberately define ties.
pub fn bundle_majority<const BITS: usize>(
    fields: &[&SemanticBitField<BITS>],
) -> Result<SemanticBitField<BITS>, SemanticAlgebraError> {
    if fields.is_empty() {
        return Err(SemanticAlgebraError::EmptyBundle);
    }
    if fields.len() % 2 == 0 {
        return Err(SemanticAlgebraError::EvenBundleCardinality(fields.len()));
    }

    let threshold = fields.len() / 2 + 1;
    let mut output = SemanticBitField::zeroed()?;

    for index in 0..BITS {
        let mut votes = 0_usize;
        for field in fields {
            if field.get(index)? {
                votes += 1;
            }
        }
        if votes >= threshold {
            output.set(index, true)?;
        }
    }

    Ok(output)
}

/// Derives the permutation rotation from the role key itself.
///
/// This is deterministic mixing for semantic role separation, not a
/// cryptographic hash. The XOR with the full role key remains part of binding.
fn role_rotation<const BITS: usize>(role: &SemanticBitField<BITS>) -> usize {
    if BITS <= 1 {
        return 0;
    }

    let mut mixed = 0xcbf2_9ce4_8422_2325_u64;
    for (index, word) in role.words().iter().copied().enumerate() {
        mixed ^= word.rotate_left((index % 64) as u32);
        mixed = mixed.wrapping_mul(0x0000_0100_0000_01b3);
    }

    1 + (mixed as usize % (BITS - 1))
}

/// Invalid semantic algebra request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticAlgebraError {
    /// A majority bundle requires at least one operand.
    EmptyBundle,
    /// Strict majority requires an odd number of operands in this reference slice.
    EvenBundleCardinality(usize),
    /// A packed-field invariant failed.
    BitField(SemanticBitFieldError),
}

impl fmt::Display for SemanticAlgebraError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticAlgebraError {}

impl From<SemanticBitFieldError> for SemanticAlgebraError {
    fn from(value: SemanticBitFieldError) -> Self {
        Self::BitField(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bind, bundle_majority, inverse_rotate_permute, rotate_permute, unbind,
        SemanticAlgebraError,
    };
    use crate::semantic::SemanticBitField;

    #[test]
    fn non_word_aligned_permutation_round_trip_is_exact() {
        let input =
            SemanticBitField::<70>::from_set_indices(&[0, 1, 17, 63, 64, 69]).expect("input");
        let permuted = rotate_permute(&input, 37).expect("permute");
        let recovered = inverse_rotate_permute(&permuted, 37).expect("inverse");

        assert_eq!(recovered, input);
    }

    #[test]
    fn binding_and_unbinding_are_exact() {
        let role = SemanticBitField::<128>::from_set_indices(&[1, 7, 63, 90]).expect("role");
        let value =
            SemanticBitField::<128>::from_set_indices(&[0, 2, 64, 65, 127]).expect("value");

        let bound = bind(&role, &value).expect("bind");
        assert_ne!(bound, value);
        assert_eq!(unbind(&role, &bound), Ok(value));
    }

    #[test]
    fn different_roles_produce_distinct_bindings_for_the_same_value() {
        let subject = SemanticBitField::<128>::from_set_indices(&[1, 5, 9]).expect("subject");
        let object = SemanticBitField::<128>::from_set_indices(&[2, 6, 10]).expect("object");
        let concept = SemanticBitField::<128>::from_set_indices(&[0, 32, 64, 96]).expect("concept");

        let subject_binding = bind(&subject, &concept).expect("subject binding");
        let object_binding = bind(&object, &concept).expect("object binding");

        assert_ne!(subject_binding, object_binding);
        assert_eq!(unbind(&subject, &subject_binding), Ok(concept.clone()));
        assert_eq!(unbind(&object, &object_binding), Ok(concept));
    }

    #[test]
    fn wrong_role_does_not_recover_the_original_value_in_fixture() {
        let subject = SemanticBitField::<64>::from_set_indices(&[1, 3, 5]).expect("subject");
        let object = SemanticBitField::<64>::from_set_indices(&[2, 4, 6]).expect("object");
        let value = SemanticBitField::<64>::from_set_indices(&[0, 16, 32, 48]).expect("value");

        let bound = bind(&subject, &value).expect("bind");
        let wrong = unbind(&object, &bound).expect("wrong unbind");

        assert_ne!(wrong, value);
    }

    #[test]
    fn majority_bundle_preserves_coordinate_consensus() {
        let a = SemanticBitField::<16>::from_set_indices(&[0, 1, 2, 8]).expect("a");
        let b = SemanticBitField::<16>::from_set_indices(&[0, 1, 3, 8]).expect("b");
        let c = SemanticBitField::<16>::from_set_indices(&[0, 2, 3, 8]).expect("c");

        let bundle = bundle_majority(&[&a, &b, &c]).expect("bundle");

        assert_eq!(bundle.get(0), Ok(true));
        assert_eq!(bundle.get(1), Ok(true));
        assert_eq!(bundle.get(2), Ok(true));
        assert_eq!(bundle.get(3), Ok(true));
        assert_eq!(bundle.get(8), Ok(true));
        assert_eq!(bundle.count_ones(), 5);
    }

    #[test]
    fn majority_bundle_rejects_hidden_tie_breaking() {
        let a = SemanticBitField::<8>::zeroed().expect("a");
        let b = SemanticBitField::<8>::ones().expect("b");

        assert_eq!(
            bundle_majority(&[&a, &b]),
            Err(SemanticAlgebraError::EvenBundleCardinality(2))
        );
        assert_eq!(
            bundle_majority::<8>(&[]),
            Err(SemanticAlgebraError::EmptyBundle)
        );
    }
}
