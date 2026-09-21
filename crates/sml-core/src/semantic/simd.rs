//! SBG-0.12 SIMD qualification surface.
//!
//! This module keeps an independent bit-by-bit scalar oracle next to a packed
//! word kernel. The packed kernel uses stable safe Rust only and deliberately
//! makes no claim that the compiler emitted a particular SIMD instruction.
//! Architecture-specific acceleration is promoted only after measured evidence.

use core::fmt;

use super::{SemanticBitField, SemanticBitFieldError};

/// Reference/optimized semantic comparison path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticKernelKind {
    /// Independent bit-by-bit oracle.
    ScalarReference,
    /// Four-word grouped packed kernel using XOR/AND/POPCOUNT on u64 words.
    PackedWord4,
}

/// Exact result and deterministic work accounting for one semantic comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticKernelEvidence {
    pub kernel: SemanticKernelKind,
    pub logical_bits: u64,
    pub packed_words: u64,
    pub full_differing_bits: u64,
    pub full_matching_bits: u64,
    pub masked_active_bits: u64,
    pub masked_differing_bits: u64,
    pub masked_matching_bits: u64,
    /// Logical coordinates visited by the bit-by-bit oracle.
    pub logical_coordinate_steps: u64,
    /// Groups of four packed words processed by the packed kernel.
    pub packed_word_groups: u64,
    /// Remaining words after four-word grouping.
    pub packed_tail_words: u64,
    /// Exact number of u64::count_ones calls made by the packed kernel.
    pub popcount_calls: u64,
}

/// Side-by-side qualification evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticKernelQualification {
    pub scalar: SemanticKernelEvidence,
    pub packed: SemanticKernelEvidence,
}

/// Executes the independent bit-by-bit semantic comparison oracle.
pub fn scalar_reference_score<const BITS: usize>(
    left: &SemanticBitField<BITS>,
    right: &SemanticBitField<BITS>,
    mask: &SemanticBitField<BITS>,
) -> Result<SemanticKernelEvidence, SemanticSimdError> {
    let mut full_differing_bits = 0_u64;
    let mut masked_active_bits = 0_u64;
    let mut masked_differing_bits = 0_u64;

    for index in 0..BITS {
        let left_bit = left.get(index)?;
        let right_bit = right.get(index)?;
        let mask_bit = mask.get(index)?;
        let differs = left_bit != right_bit;

        if differs {
            full_differing_bits = full_differing_bits.saturating_add(1);
        }
        if mask_bit {
            masked_active_bits = masked_active_bits.saturating_add(1);
            if differs {
                masked_differing_bits = masked_differing_bits.saturating_add(1);
            }
        }
    }

    if masked_active_bits == 0 {
        return Err(SemanticSimdError::EmptyMask);
    }

    Ok(SemanticKernelEvidence {
        kernel: SemanticKernelKind::ScalarReference,
        logical_bits: BITS as u64,
        packed_words: left.words().len() as u64,
        full_differing_bits,
        full_matching_bits: BITS as u64 - full_differing_bits,
        masked_active_bits,
        masked_differing_bits,
        masked_matching_bits: masked_active_bits - masked_differing_bits,
        logical_coordinate_steps: BITS as u64,
        packed_word_groups: 0,
        packed_tail_words: 0,
        popcount_calls: 0,
    })
}

/// Executes a safe packed-word kernel grouped in four-u64 batches.
///
/// Grouping is explicit to expose a stable benchmark unit and instruction-level
/// parallelism opportunity while preserving the crate-wide unsafe-code ban.
pub fn packed_word4_score<const BITS: usize>(
    left: &SemanticBitField<BITS>,
    right: &SemanticBitField<BITS>,
    mask: &SemanticBitField<BITS>,
) -> Result<SemanticKernelEvidence, SemanticSimdError> {
    let left_words = left.words();
    let right_words = right.words();
    let mask_words = mask.words();

    let mut full_differing_bits = 0_u64;
    let mut masked_active_bits = 0_u64;
    let mut masked_differing_bits = 0_u64;
    let mut index = 0_usize;

    while index + 4 <= left_words.len() {
        for offset in 0..4 {
            accumulate_word(
                left_words[index + offset],
                right_words[index + offset],
                mask_words[index + offset],
                &mut full_differing_bits,
                &mut masked_active_bits,
                &mut masked_differing_bits,
            );
        }
        index += 4;
    }

    while index < left_words.len() {
        accumulate_word(
            left_words[index],
            right_words[index],
            mask_words[index],
            &mut full_differing_bits,
            &mut masked_active_bits,
            &mut masked_differing_bits,
        );
        index += 1;
    }

    if masked_active_bits == 0 {
        return Err(SemanticSimdError::EmptyMask);
    }

    let packed_words = left_words.len() as u64;
    Ok(SemanticKernelEvidence {
        kernel: SemanticKernelKind::PackedWord4,
        logical_bits: BITS as u64,
        packed_words,
        full_differing_bits,
        full_matching_bits: BITS as u64 - full_differing_bits,
        masked_active_bits,
        masked_differing_bits,
        masked_matching_bits: masked_active_bits - masked_differing_bits,
        logical_coordinate_steps: 0,
        packed_word_groups: packed_words / 4,
        packed_tail_words: packed_words % 4,
        popcount_calls: packed_words.saturating_mul(3),
    })
}

/// Fixed-width entry point used for reproducible SBG-0.12 code-generation audits.
///
/// Keeping this wrapper non-generic and non-inlined gives release builds a stable
/// semantic target to inspect without changing the generic reference API.
#[inline(never)]
pub fn packed_word4_score_16384(
    left: &SemanticBitField<16_384>,
    right: &SemanticBitField<16_384>,
    mask: &SemanticBitField<16_384>,
) -> Result<SemanticKernelEvidence, SemanticSimdError> {
    packed_word4_score(left, right, mask)
}

/// Qualifies the packed kernel against the independent scalar oracle.
pub fn qualify_packed_word4<const BITS: usize>(
    left: &SemanticBitField<BITS>,
    right: &SemanticBitField<BITS>,
    mask: &SemanticBitField<BITS>,
) -> Result<SemanticKernelQualification, SemanticSimdError> {
    let scalar = scalar_reference_score(left, right, mask)?;
    let packed = packed_word4_score(left, right, mask)?;

    if !same_semantics(scalar, packed) {
        return Err(SemanticSimdError::KernelMismatch);
    }

    Ok(SemanticKernelQualification { scalar, packed })
}

fn accumulate_word(
    left: u64,
    right: u64,
    mask: u64,
    full_differing_bits: &mut u64,
    masked_active_bits: &mut u64,
    masked_differing_bits: &mut u64,
) {
    let differing = left ^ right;
    *full_differing_bits = full_differing_bits.saturating_add(u64::from(differing.count_ones()));
    *masked_active_bits = masked_active_bits.saturating_add(u64::from(mask.count_ones()));
    *masked_differing_bits =
        masked_differing_bits.saturating_add(u64::from((differing & mask).count_ones()));
}

const fn same_semantics(scalar: SemanticKernelEvidence, packed: SemanticKernelEvidence) -> bool {
    scalar.logical_bits == packed.logical_bits
        && scalar.packed_words == packed.packed_words
        && scalar.full_differing_bits == packed.full_differing_bits
        && scalar.full_matching_bits == packed.full_matching_bits
        && scalar.masked_active_bits == packed.masked_active_bits
        && scalar.masked_differing_bits == packed.masked_differing_bits
        && scalar.masked_matching_bits == packed.masked_matching_bits
}

/// SBG packed-kernel qualification failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticSimdError {
    EmptyMask,
    KernelMismatch,
    BitField(SemanticBitFieldError),
}

impl fmt::Display for SemanticSimdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticSimdError {}

impl From<SemanticBitFieldError> for SemanticSimdError {
    fn from(value: SemanticBitFieldError) -> Self {
        Self::BitField(value)
    }
}
