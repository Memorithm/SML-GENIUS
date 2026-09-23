//! Research-only bounded rewritable Delta-KV reference memory.
//!
//! This module is a deterministic fixed-point associative matrix, not a
//! Transformer KV cache and not a performance implementation. It adapts the
//! general delta-correction idea to SML-GENIUS by using bounded Q12 arithmetic,
//! exact storage accounting, explicit saturation counters, and no external
//! dependencies.

use core::fmt;

/// Schema version for the Delta-KV reference contract.
pub const DELTA_KV_SCHEMA_VERSION: u32 = 1;
/// Fixed-point scale for signed Q12 values.
pub const DELTA_KV_SCALE: i32 = 1 << 12;
/// Q12 representation of 1.0.
pub const DELTA_KV_ONE: i16 = DELTA_KV_SCALE as i16;

/// Exact operation counters for Delta-KV reference execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeltaKvCounters {
    /// Public read calls.
    pub reads: u64,
    /// Update calls.
    pub updates: u64,
    /// Multiply-accumulate terms evaluated while reading predictions.
    pub read_macs: u64,
    /// Matrix entries considered for a delta update.
    pub weight_updates: u64,
    /// Matrix entries clamped to the signed i16 storage range.
    pub saturations: u64,
}

/// One bounded rewritable associative matrix.
///
/// Keys and stored matrix coefficients use signed Q12 values. Reads return Q12
/// outputs in i32 so accumulation is not silently clamped. Updates use a
/// normalized delta rule:
///
/// M <- M + eta * k^T (target - kM) / ||k||^2
///
/// The normalization is an SML-specific stability choice. It makes a full-rate
/// update correct the current key exactly in the absence of storage saturation,
/// while still exposing interference when keys overlap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvMemory {
    key_dim: usize,
    value_dim: usize,
    weights_q12: Vec<i16>,
}

impl DeltaKvMemory {
    /// Construct a zero-initialized bounded Delta-KV matrix.
    pub fn new(key_dim: usize, value_dim: usize) -> Result<Self, DeltaKvError> {
        if key_dim == 0 {
            return Err(DeltaKvError::ZeroKeyDimension);
        }
        if value_dim == 0 {
            return Err(DeltaKvError::ZeroValueDimension);
        }
        let len = key_dim
            .checked_mul(value_dim)
            .ok_or(DeltaKvError::ShapeOverflow)?;
        Ok(Self {
            key_dim,
            value_dim,
            weights_q12: vec![0; len],
        })
    }

    /// Key width.
    #[must_use]
    pub const fn key_dim(&self) -> usize {
        self.key_dim
    }

    /// Value width.
    #[must_use]
    pub const fn value_dim(&self) -> usize {
        self.value_dim
    }

    /// Stored signed-Q12 matrix coefficients in row-major key/value order.
    #[must_use]
    pub fn weights_q12(&self) -> &[i16] {
        &self.weights_q12
    }

    /// Canonical logical storage size including fixed schema metadata.
    ///
    /// This is not Rust heap allocation size. It counts 16 bits per matrix
    /// coefficient plus schema version, key dimension, value dimension and Q
    /// scale metadata.
    #[must_use]
    pub fn semantic_bits(&self) -> u128 {
        const METADATA_BITS: u128 = 32 + 64 + 64 + 32;
        METADATA_BITS + self.weights_q12.len() as u128 * 16
    }

    /// Read one associative value in signed Q12 units.
    pub fn read(
        &self,
        key_q12: &[i16],
        counters: &mut DeltaKvCounters,
    ) -> Result<Vec<i32>, DeltaKvError> {
        self.validate_key(key_q12)?;
        counters.reads = counters.reads.saturating_add(1);
        self.predict(key_q12, counters)
    }

    /// Apply one normalized delta correction.
    ///
    /// learning_rate_q12 must be in 0..=DELTA_KV_SCALE. A zero key is rejected
    /// because the normalized correction would have no defined direction.
    pub fn update(
        &mut self,
        key_q12: &[i16],
        target_q12: &[i16],
        learning_rate_q12: u16,
        counters: &mut DeltaKvCounters,
    ) -> Result<(), DeltaKvError> {
        self.validate_key(key_q12)?;
        if target_q12.len() != self.value_dim {
            return Err(DeltaKvError::ValueDimensionMismatch {
                expected: self.value_dim,
                actual: target_q12.len(),
            });
        }
        if usize::from(learning_rate_q12) > DELTA_KV_SCALE as usize {
            return Err(DeltaKvError::InvalidLearningRate {
                actual: learning_rate_q12,
                maximum: DELTA_KV_SCALE as u16,
            });
        }

        let mut norm_sq = 0_i128;
        for &component in key_q12 {
            let component = i128::from(component);
            norm_sq = norm_sq
                .checked_add(component * component)
                .ok_or(DeltaKvError::ArithmeticOverflow)?;
        }
        if norm_sq == 0 {
            return Err(DeltaKvError::ZeroKeyNorm);
        }

        let prediction = self.predict(key_q12, counters)?;
        counters.updates = counters.updates.saturating_add(1);

        for key_index in 0..self.key_dim {
            let key_component = i128::from(key_q12[key_index]);
            for value_index in 0..self.value_dim {
                counters.weight_updates = counters.weight_updates.saturating_add(1);
                let error_q12 =
                    i128::from(target_q12[value_index]) - i128::from(prediction[value_index]);
                let numerator = i128::from(learning_rate_q12)
                    .checked_mul(key_component)
                    .and_then(|value| value.checked_mul(error_q12))
                    .ok_or(DeltaKvError::ArithmeticOverflow)?;
                let delta_q12 = div_round_nearest(numerator, norm_sq);
                let index = key_index * self.value_dim + value_index;
                let proposed = i128::from(self.weights_q12[index])
                    .checked_add(delta_q12)
                    .ok_or(DeltaKvError::ArithmeticOverflow)?;
                let clamped = proposed.clamp(i128::from(i16::MIN), i128::from(i16::MAX));
                if clamped != proposed {
                    counters.saturations = counters.saturations.saturating_add(1);
                }
                self.weights_q12[index] =
                    i16::try_from(clamped).map_err(|_| DeltaKvError::ArithmeticOverflow)?;
            }
        }

        Ok(())
    }

    fn validate_key(&self, key_q12: &[i16]) -> Result<(), DeltaKvError> {
        if key_q12.len() != self.key_dim {
            return Err(DeltaKvError::KeyDimensionMismatch {
                expected: self.key_dim,
                actual: key_q12.len(),
            });
        }
        Ok(())
    }

    fn predict(
        &self,
        key_q12: &[i16],
        counters: &mut DeltaKvCounters,
    ) -> Result<Vec<i32>, DeltaKvError> {
        let mut output = vec![0_i32; self.value_dim];
        for value_index in 0..self.value_dim {
            let mut accumulator = 0_i128;
            for (key_index, &key_component) in key_q12.iter().enumerate() {
                counters.read_macs = counters.read_macs.saturating_add(1);
                let weight = self.weights_q12[key_index * self.value_dim + value_index];
                accumulator = accumulator
                    .checked_add(i128::from(key_component) * i128::from(weight))
                    .ok_or(DeltaKvError::ArithmeticOverflow)?;
            }
            let q12 = div_round_nearest(accumulator, i128::from(DELTA_KV_SCALE));
            output[value_index] =
                i32::try_from(q12).map_err(|_| DeltaKvError::PredictionOverflow)?;
        }
        Ok(output)
    }
}

fn div_round_nearest(numerator: i128, denominator: i128) -> i128 {
    debug_assert!(denominator > 0);
    if numerator >= 0 {
        (numerator + denominator / 2) / denominator
    } else {
        (numerator - denominator / 2) / denominator
    }
}

/// Construction, shape or arithmetic failures from Delta-KV.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeltaKvError {
    /// Key dimension must be non-zero.
    ZeroKeyDimension,
    /// Value dimension must be non-zero.
    ZeroValueDimension,
    /// Matrix dimensions overflowed usize.
    ShapeOverflow,
    /// Key length differs from the frozen matrix geometry.
    KeyDimensionMismatch { expected: usize, actual: usize },
    /// Target length differs from the frozen matrix geometry.
    ValueDimensionMismatch { expected: usize, actual: usize },
    /// Learning rate exceeds one in Q12.
    InvalidLearningRate { actual: u16, maximum: u16 },
    /// Normalized correction cannot be defined for the all-zero key.
    ZeroKeyNorm,
    /// Checked integer arithmetic overflowed.
    ArithmeticOverflow,
    /// A read result does not fit the public i32 Q12 output contract.
    PredictionOverflow,
}

impl fmt::Display for DeltaKvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DeltaKvError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn one() -> i16 {
        DELTA_KV_ONE
    }

    #[test]
    fn rejects_invalid_geometry_and_zero_key_norm() {
        assert_eq!(
            DeltaKvMemory::new(0, 1),
            Err(DeltaKvError::ZeroKeyDimension)
        );
        assert_eq!(
            DeltaKvMemory::new(1, 0),
            Err(DeltaKvError::ZeroValueDimension)
        );

        let mut memory = DeltaKvMemory::new(2, 1).unwrap();
        let mut counters = DeltaKvCounters::default();
        assert_eq!(
            memory.update(&[0, 0], &[100], DELTA_KV_SCALE as u16, &mut counters),
            Err(DeltaKvError::ZeroKeyNorm)
        );
    }

    #[test]
    fn full_rate_one_hot_update_writes_and_rewrites_exactly() {
        let mut memory = DeltaKvMemory::new(2, 2).unwrap();
        let mut counters = DeltaKvCounters::default();

        memory
            .update(
                &[one(), 0],
                &[1200, -700],
                DELTA_KV_SCALE as u16,
                &mut counters,
            )
            .unwrap();
        assert_eq!(
            memory.read(&[one(), 0], &mut counters).unwrap(),
            vec![1200, -700]
        );

        memory
            .update(
                &[one(), 0],
                &[-300, 900],
                DELTA_KV_SCALE as u16,
                &mut counters,
            )
            .unwrap();
        assert_eq!(
            memory.read(&[one(), 0], &mut counters).unwrap(),
            vec![-300, 900]
        );
        assert_eq!(counters.saturations, 0);
    }

    #[test]
    fn orthogonal_keys_do_not_interfere_without_saturation() {
        let mut memory = DeltaKvMemory::new(2, 1).unwrap();
        let mut counters = DeltaKvCounters::default();

        memory
            .update(
                &[one(), 0],
                &[1000],
                DELTA_KV_SCALE as u16,
                &mut counters,
            )
            .unwrap();
        memory
            .update(
                &[0, one()],
                &[-1500],
                DELTA_KV_SCALE as u16,
                &mut counters,
            )
            .unwrap();

        assert_eq!(
            memory.read(&[one(), 0], &mut counters).unwrap(),
            vec![1000]
        );
        assert_eq!(
            memory.read(&[0, one()], &mut counters).unwrap(),
            vec![-1500]
        );
    }

    #[test]
    fn overlapping_keys_expose_interference_then_repair_current_key() {
        let mut memory = DeltaKvMemory::new(2, 1).unwrap();
        let mut counters = DeltaKvCounters::default();

        memory
            .update(
                &[one(), 0],
                &[1000],
                DELTA_KV_SCALE as u16,
                &mut counters,
            )
            .unwrap();
        memory
            .update(
                &[one(), one()],
                &[2000],
                DELTA_KV_SCALE as u16,
                &mut counters,
            )
            .unwrap();

        let first_key_after_overlap = memory.read(&[one(), 0], &mut counters).unwrap()[0];
        let overlapping_key_after_update =
            memory.read(&[one(), one()], &mut counters).unwrap()[0];

        assert_ne!(first_key_after_overlap, 1000);
        assert!((overlapping_key_after_update - 2000).abs() <= 1);
    }

    #[test]
    fn semantic_bits_include_matrix_and_contract_metadata() {
        let memory = DeltaKvMemory::new(3, 2).unwrap();
        assert_eq!(memory.semantic_bits(), 32 + 64 + 64 + 32 + 6 * 16);
    }
}
