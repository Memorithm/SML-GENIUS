//! Integer-only semantic metrics for Semantic Boolean Geometry.

use core::fmt;

use super::SemanticBitField;

/// Exact masked comparison counts.
///
/// `active_bits` is the number of coordinates selected by the context mask.
/// `differing_bits` and `matching_bits` partition that selected set exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaskedSemanticScore {
    pub active_bits: u64,
    pub differing_bits: u64,
    pub matching_bits: u64,
}

impl MaskedSemanticScore {
    /// Integer distance in parts per million.
    #[must_use]
    pub fn distance_ppm(self) -> u32 {
        ratio_ppm(self.differing_bits, self.active_bits)
    }

    /// Integer similarity in parts per million.
    #[must_use]
    pub fn similarity_ppm(self) -> u32 {
        ratio_ppm(self.matching_bits, self.active_bits)
    }
}

/// Exact Hamming distance over the entire logical semantic field.
#[must_use]
pub fn hamming_distance<const BITS: usize>(
    left: &SemanticBitField<BITS>,
    right: &SemanticBitField<BITS>,
) -> u64 {
    left.xor_popcount(right)
}

/// Exact XNOR similarity count over the entire logical semantic field.
#[must_use]
pub fn xnor_similarity<const BITS: usize>(
    left: &SemanticBitField<BITS>,
    right: &SemanticBitField<BITS>,
) -> u64 {
    left.xnor_popcount(right)
}

/// Compares two semantic fields only on coordinates selected by `mask`.
///
/// No temporary semantic field is constructed. The operation scans packed words
/// and performs XOR/AND/POPCOUNT directly.
pub fn masked_score<const BITS: usize>(
    left: &SemanticBitField<BITS>,
    right: &SemanticBitField<BITS>,
    mask: &SemanticBitField<BITS>,
) -> Result<MaskedSemanticScore, SemanticMetricError> {
    let mut active_bits = 0_u64;
    let mut differing_bits = 0_u64;

    for ((left_word, right_word), mask_word) in left
        .words()
        .iter()
        .zip(right.words())
        .zip(mask.words())
    {
        active_bits = active_bits.saturating_add(u64::from(mask_word.count_ones()));
        differing_bits = differing_bits
            .saturating_add(u64::from(((left_word ^ right_word) & mask_word).count_ones()));
    }

    if active_bits == 0 {
        return Err(SemanticMetricError::EmptyMask);
    }

    Ok(MaskedSemanticScore {
        active_bits,
        differing_bits,
        matching_bits: active_bits - differing_bits,
    })
}

/// Exact masked Hamming distance.
pub fn masked_hamming_distance<const BITS: usize>(
    left: &SemanticBitField<BITS>,
    right: &SemanticBitField<BITS>,
    mask: &SemanticBitField<BITS>,
) -> Result<u64, SemanticMetricError> {
    Ok(masked_score(left, right, mask)?.differing_bits)
}

/// Exact masked XNOR similarity count.
pub fn masked_xnor_similarity<const BITS: usize>(
    left: &SemanticBitField<BITS>,
    right: &SemanticBitField<BITS>,
    mask: &SemanticBitField<BITS>,
) -> Result<u64, SemanticMetricError> {
    Ok(masked_score(left, right, mask)?.matching_bits)
}

fn ratio_ppm(numerator: u64, denominator: u64) -> u32 {
    if denominator == 0 {
        return 0;
    }
    ((u128::from(numerator) * 1_000_000_u128) / u128::from(denominator)) as u32
}

/// Invalid semantic-metric input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticMetricError {
    /// A contextual metric requires at least one active mask coordinate.
    EmptyMask,
}

impl fmt::Display for SemanticMetricError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticMetricError {}

#[cfg(test)]
mod tests {
    use super::{
        hamming_distance, masked_hamming_distance, masked_score, masked_xnor_similarity,
        xnor_similarity, SemanticMetricError,
    };
    use crate::semantic::SemanticBitField;

    #[test]
    fn full_metrics_partition_the_logical_width() {
        let left = SemanticBitField::<128>::from_set_indices(&[0, 1, 64]).expect("left");
        let right = SemanticBitField::<128>::from_set_indices(&[0, 2, 64]).expect("right");

        assert_eq!(hamming_distance(&left, &right), 2);
        assert_eq!(xnor_similarity(&left, &right), 126);
    }

    #[test]
    fn empty_context_mask_fails_closed() {
        let left = SemanticBitField::<64>::zeroed().expect("left");
        let right = SemanticBitField::<64>::zeroed().expect("right");
        let mask = SemanticBitField::<64>::zeroed().expect("mask");

        assert_eq!(
            masked_score(&left, &right, &mask),
            Err(SemanticMetricError::EmptyMask)
        );
    }

    #[test]
    fn masked_score_is_exact_and_integer_only() {
        let left = SemanticBitField::<8>::from_set_indices(&[0, 2, 4, 6]).expect("left");
        let right = SemanticBitField::<8>::from_set_indices(&[0, 3, 4, 7]).expect("right");
        let mask = SemanticBitField::<8>::from_set_indices(&[0, 1, 2, 3]).expect("mask");

        let score = masked_score(&left, &right, &mask).expect("score");
        assert_eq!(score.active_bits, 4);
        assert_eq!(score.differing_bits, 2);
        assert_eq!(score.matching_bits, 2);
        assert_eq!(score.distance_ppm(), 500_000);
        assert_eq!(score.similarity_ppm(), 500_000);
        assert_eq!(masked_hamming_distance(&left, &right, &mask), Ok(2));
        assert_eq!(masked_xnor_similarity(&left, &right, &mask), Ok(2));
    }

    #[test]
    fn context_mask_can_reverse_semantic_neighbourhood() {
        // One ambiguous anchor has equal global Hamming distance to both
        // candidates. Context masks select which half of the Boolean geometry
        // matters, making a different candidate nearest in each context.
        let anchor = SemanticBitField::<8>::from_set_indices(&[0, 2, 4, 6]).expect("anchor");
        let finance = SemanticBitField::<8>::from_set_indices(&[0, 2, 5, 7]).expect("finance");
        let river = SemanticBitField::<8>::from_set_indices(&[1, 3, 4, 6]).expect("river");

        assert_eq!(hamming_distance(&anchor, &finance), 4);
        assert_eq!(hamming_distance(&anchor, &river), 4);

        let finance_context =
            SemanticBitField::<8>::from_set_indices(&[0, 1, 2, 3]).expect("finance mask");
        let river_context =
            SemanticBitField::<8>::from_set_indices(&[4, 5, 6, 7]).expect("river mask");

        assert_eq!(
            masked_hamming_distance(&anchor, &finance, &finance_context),
            Ok(0)
        );
        assert_eq!(
            masked_hamming_distance(&anchor, &river, &finance_context),
            Ok(4)
        );

        assert_eq!(
            masked_hamming_distance(&anchor, &finance, &river_context),
            Ok(4)
        );
        assert_eq!(
            masked_hamming_distance(&anchor, &river, &river_context),
            Ok(0)
        );
    }
}
