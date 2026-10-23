//! H-ALT-0: an independent, SBG-read-only ternary residual scorer.
//!
//! This control arm deliberately does not touch MOR, PVP6, SBG write-back,
//! the Claude "shared bus" implementation, or an existing model checkpoint.
//! TRAIN exports ternary weights; runtime is exactly integer arithmetic.
//! No model-quality, SIMD, energy or attention-replacement claim.

use std::collections::BTreeSet;

use crate::{
    pvp6_sbg::{SbgCoordinate, SbgField},
    semantic::{SemanticBitFieldError, SemanticState},
};

pub const MAX_INPUT_BITS: usize = 4096;
pub const MAX_OUTPUT_CLASSES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HybridAltError {
    InvalidFeatures,
    InvalidOutputs,
    DuplicateCoordinate(SbgCoordinate),
    CoordinateOutOfRange(SbgCoordinate),
    InvalidWeight { index: usize, value: i8 },
    InvalidWeightCount { expected: usize, actual: usize },
    OverlappingWeights,
    NonzeroPadding,
    BiasCouldOverflow,
    InvalidBaselineWidth { expected: usize, actual: usize },
    IntegerOverflow,
    Field(SemanticBitFieldError),
}

/// Immutable ternary coefficients of one output neuron, exactly ±1 or zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TernaryRow {
    width: usize,
    positive: Vec<u64>,
    negative: Vec<u64>,
    bias: i32,
}

impl TernaryRow {
    pub fn from_weights(weights: &[i8], bias: i32) -> Result<Self, HybridAltError> {
        let width = weights.len();
        if !(1..=MAX_INPUT_BITS).contains(&width) {
            return Err(HybridAltError::InvalidFeatures);
        }
        let mut positive = vec![0_u64; width.div_ceil(64)];
        let mut negative = vec![0_u64; width.div_ceil(64)];
        for (index, &weight) in weights.iter().enumerate() {
            let slot = index / 64;
            let bit = 1_u64 << (index % 64);
            match weight {
                1 => positive[slot] |= bit,
                -1 => negative[slot] |= bit,
                0 => {}
                _ => {
                    return Err(HybridAltError::InvalidWeight {
                        index,
                        value: weight,
                    })
                }
            }
        }
        Self::from_masks(width, positive, negative, bias)
    }

    /// Validation also applies to masks imported from an immutable checkpoint.
    pub fn from_masks(
        width: usize,
        positive: Vec<u64>,
        negative: Vec<u64>,
        bias: i32,
    ) -> Result<Self, HybridAltError> {
        if !(1..=MAX_INPUT_BITS).contains(&width) {
            return Err(HybridAltError::InvalidFeatures);
        }
        let words = width.div_ceil(64);
        if positive.len() != words || negative.len() != words {
            return Err(HybridAltError::InvalidWeightCount {
                expected: words,
                actual: positive.len().max(negative.len()),
            });
        }
        if positive.iter().zip(&negative).any(|(&p, &n)| p & n != 0) {
            return Err(HybridAltError::OverlappingWeights);
        }
        let tail = width % 64;
        if tail != 0 {
            let allowed = (1_u64 << tail) - 1;
            if (positive[words - 1] | negative[words - 1]) & !allowed != 0 {
                return Err(HybridAltError::NonzeroPadding);
            }
        }
        // Safe for all active patterns: score ∈ [bias - width, bias + width].
        let max_magnitude = width as i32;
        if bias.checked_add(max_magnitude).is_none() || bias.checked_sub(max_magnitude).is_none() {
            return Err(HybridAltError::BiasCouldOverflow);
        }
        Ok(Self {
            width,
            positive,
            negative,
            bias,
        })
    }

    #[must_use]
    pub fn weight_count(&self) -> usize {
        self.width
    }

    #[must_use]
    pub fn weight_at(&self, index: usize) -> Option<i8> {
        if index >= self.width {
            return None;
        }
        let p = (self.positive[index / 64] >> (index % 64)) & 1;
        let n = (self.negative[index / 64] >> (index % 64)) & 1;
        Some(p as i8 - n as i8)
    }

    /// Word-at-a-time Boolean activation and ternary dot product.
    /// Shape is checked even for externally provided packed activations.
    pub fn score_packed(&self, activations: &[u64]) -> Result<i32, HybridAltError> {
        if activations.len() != self.positive.len() {
            return Err(HybridAltError::InvalidWeightCount {
                expected: self.positive.len(),
                actual: activations.len(),
            });
        }
        let tail = self.width % 64;
        if tail != 0 && (activations[activations.len() - 1] >> tail) != 0 {
            return Err(HybridAltError::NonzeroPadding);
        }
        let mut positive = 0_i32;
        let mut negative = 0_i32;
        for ((&x, &p), &n) in activations.iter().zip(&self.positive).zip(&self.negative) {
            positive += (x & p).count_ones() as i32;
            negative += (x & n).count_ones() as i32;
        }
        // Constructor validated bound for every possible active subset.
        Ok(self.bias + positive - negative)
    }

    /// Independent scalar oracle; does not call popcount or packed score.
    pub fn score_reference(&self, activations: &[bool]) -> Result<i32, HybridAltError> {
        if activations.len() != self.width {
            return Err(HybridAltError::InvalidWeightCount {
                expected: self.width,
                actual: activations.len(),
            });
        }
        let mut result = self.bias;
        for (index, &active) in activations.iter().enumerate() {
            if active {
                result += i32::from(self.weight_at(index).expect("index in range"));
            }
        }
        Ok(result)
    }
}

/// A sidecar observes immutable semantic state and emits integer corrections.
/// In particular, it cannot update routing, memory, or any SBG coordinate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TernaryResidualSidecar<const S: usize> {
    features: Vec<SbgCoordinate>,
    rows: Vec<TernaryRow>,
}

impl<const S: usize> TernaryResidualSidecar<S> {
    pub fn new(
        features: Vec<SbgCoordinate>,
        rows: Vec<TernaryRow>,
    ) -> Result<Self, HybridAltError> {
        if !(1..=MAX_INPUT_BITS).contains(&features.len()) {
            return Err(HybridAltError::InvalidFeatures);
        }
        if !(1..=MAX_OUTPUT_CLASSES).contains(&rows.len()) {
            return Err(HybridAltError::InvalidOutputs);
        }
        let mut seen = BTreeSet::new();
        for &coordinate in &features {
            if coordinate.bit >= S {
                return Err(HybridAltError::CoordinateOutOfRange(coordinate));
            }
            if !seen.insert(coordinate) {
                return Err(HybridAltError::DuplicateCoordinate(coordinate));
            }
        }
        for row in &rows {
            if row.weight_count() != features.len() {
                return Err(HybridAltError::InvalidWeightCount {
                    expected: features.len(),
                    actual: row.weight_count(),
                });
            }
        }
        Ok(Self { features, rows })
    }

    #[must_use]
    pub fn class_count(&self) -> usize {
        self.rows.len()
    }

    #[must_use]
    pub fn input_bits(&self) -> usize {
        self.features.len()
    }

    fn read_feature(
        coordinate: SbgCoordinate,
        state: &SemanticState<S>,
    ) -> Result<bool, HybridAltError> {
        let source = match coordinate.field {
            SbgField::Value => &state.value,
            SbgField::Meaning => &state.meaning_mask,
            SbgField::Ambiguity => &state.ambiguity,
            SbgField::Routing => &state.routing,
        };
        source.get(coordinate.bit).map_err(HybridAltError::Field)
    }

    /// Explicit typed projection from four SBG fields to packed feature bits.
    pub fn project(&self, state: &SemanticState<S>) -> Result<Vec<u64>, HybridAltError> {
        let mut x = vec![0_u64; self.features.len().div_ceil(64)];
        for (index, &coordinate) in self.features.iter().enumerate() {
            if Self::read_feature(coordinate, state)? {
                x[index / 64] |= 1_u64 << (index % 64);
            }
        }
        Ok(x)
    }

    /// Exact integer corrections. Does not mutate state or maintain hidden state.
    pub fn scores(&self, state: &SemanticState<S>) -> Result<Vec<i32>, HybridAltError> {
        let x = self.project(state)?;
        self.rows.iter().map(|row| row.score_packed(&x)).collect()
    }

    /// Independently reads and scores each feature with scalar ternary weights.
    pub fn reference_scores(&self, state: &SemanticState<S>) -> Result<Vec<i32>, HybridAltError> {
        let x = self
            .features
            .iter()
            .map(|&coordinate| Self::read_feature(coordinate, state))
            .collect::<Result<Vec<_>, _>>()?;
        self.rows
            .iter()
            .map(|row| row.score_reference(&x))
            .collect()
    }

    /// Add integer residual logits to a frozen baseline without saturation.
    /// scale=0 is an exact no-op even if a nonzero sidecar was provided.
    pub fn add_to_baseline(
        &self,
        state: &SemanticState<S>,
        baseline: &[i32],
        scale: i32,
    ) -> Result<Vec<i32>, HybridAltError> {
        if baseline.len() != self.rows.len() {
            return Err(HybridAltError::InvalidBaselineWidth {
                expected: self.rows.len(),
                actual: baseline.len(),
            });
        }
        if scale == 0 {
            return Ok(baseline.to_vec());
        }
        let scores = self.scores(state)?;
        baseline
            .iter()
            .zip(scores)
            .map(|(&b, correction)| {
                let scaled = correction
                    .checked_mul(scale)
                    .ok_or(HybridAltError::IntegerOverflow)?;
                b.checked_add(scaled).ok_or(HybridAltError::IntegerOverflow)
            })
            .collect()
    }
}

/// Evaluation-only BPC from integer byte logits; f64 log-sum-exp is deliberately
/// outside the bit-exact inference contract and is not guaranteed byte-identical
/// between architectures. No labels may be used to fit the residual on DEV.
pub fn byte_log_loss_bits(
    logits: &[i32; 256],
    target: u8,
    logit_quantum: f64,
) -> Result<f64, &'static str> {
    if !logit_quantum.is_finite() || logit_quantum <= 0.0 {
        return Err("invalid logit quantum");
    }
    let max = i64::from(*logits.iter().max().expect("nonempty byte logits"));
    let shifted_sum = logits.iter().fold(0.0_f64, |sum, &logit| {
        let scaled = (i64::from(logit) - max) as f64 / logit_quantum;
        sum + scaled.exp()
    });
    let target_logit = (max - i64::from(logits[usize::from(target)])) as f64 / logit_quantum;
    let loss = (shifted_sum.ln() + target_logit) / std::f64::consts::LN_2;
    if !loss.is_finite() {
        return Err("nonfinite byte loss");
    }
    Ok(loss)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coordinates<const S: usize>(count: usize) -> Vec<SbgCoordinate> {
        let fields = [
            SbgField::Value,
            SbgField::Meaning,
            SbgField::Ambiguity,
            SbgField::Routing,
        ];
        (0..count)
            .map(|index| SbgCoordinate {
                field: fields[index / S],
                bit: index % S,
            })
            .collect()
    }

    #[test]
    fn all_4096_patterns_match_packed_ternary_and_scalar_oracle() {
        let rows = [
            TernaryRow::from_weights(&[1, -1, 0, 1, 1, -1, 0, 1, -1, 1, 0, -1], -3).unwrap(),
            TernaryRow::from_weights(&[-1, 1, 1, 0, -1, 1, 0, 1, 1, 0, -1, 1], 2).unwrap(),
        ];
        for bits in 0..4096_u64 {
            let logical: Vec<_> = (0..12).map(|j| (bits & (1 << j)) != 0).collect();
            for row in &rows {
                assert_eq!(
                    row.score_packed(&[bits]),
                    row.score_reference(&logical),
                    "exact ternary parity failed at pattern {bits}"
                );
            }
        }
    }

    #[test]
    fn four_fields_and_unaligned_sbg_width_do_not_mutate_state() {
        let mut state = SemanticState::<65>::zeroed().unwrap();
        for (field, index) in [
            (SbgField::Value, 64),
            (SbgField::Meaning, 0),
            (SbgField::Ambiguity, 63),
            (SbgField::Routing, 64),
        ] {
            let target = match field {
                SbgField::Value => &mut state.value,
                SbgField::Meaning => &mut state.meaning_mask,
                SbgField::Ambiguity => &mut state.ambiguity,
                SbgField::Routing => &mut state.routing,
            };
            target.set(index, true).unwrap();
        }
        let mut weights = vec![0_i8; 129];
        for (i, weight) in weights.iter_mut().enumerate() {
            *weight = match i % 3 {
                0 => -1,
                1 => 0,
                _ => 1,
            };
        }
        let head = TernaryResidualSidecar::<65>::new(
            coordinates::<65>(129),
            vec![TernaryRow::from_weights(&weights, -1).unwrap()],
        )
        .unwrap();
        let original = state.clone();
        assert_eq!(head.scores(&state), head.reference_scores(&state));
        assert_eq!(state, original);
        assert_eq!(head.add_to_baseline(&state, &[27], 0), Ok(vec![27]));
        assert_eq!(state, original);
    }

    #[test]
    fn rejects_overlap_padding_wrong_shapes_and_weight_domain() {
        assert_eq!(
            TernaryRow::from_masks(3, vec![1], vec![1], 0),
            Err(HybridAltError::OverlappingWeights)
        );
        assert_eq!(
            TernaryRow::from_masks(65, vec![0, 2], vec![0, 0], 0),
            Err(HybridAltError::NonzeroPadding)
        );
        assert_eq!(
            TernaryRow::from_weights(&[1, 2], 0),
            Err(HybridAltError::InvalidWeight { index: 1, value: 2 })
        );
        assert_eq!(
            TernaryRow::from_weights(&[], 0),
            Err(HybridAltError::InvalidFeatures)
        );
        assert_eq!(
            TernaryRow::from_weights(&[1], i32::MAX),
            Err(HybridAltError::BiasCouldOverflow)
        );
        assert_eq!(
            TernaryRow::from_weights(&[1], 0)
                .unwrap()
                .score_packed(&[2]),
            Err(HybridAltError::NonzeroPadding)
        );
        let feature = SbgCoordinate {
            field: SbgField::Value,
            bit: 0,
        };
        let row = TernaryRow::from_weights(&[1, 0], 0).unwrap();
        assert_eq!(
            TernaryResidualSidecar::<65>::new(vec![feature, feature], vec![row]),
            Err(HybridAltError::DuplicateCoordinate(feature))
        );
        assert!(TernaryResidualSidecar::<0>::new(
            vec![feature],
            vec![TernaryRow::from_weights(&[0], 0).unwrap()],
        )
        .is_err());
    }

    #[test]
    fn residual_overflow_is_explicit_never_saturated() {
        let state = SemanticState::<1>::zeroed().unwrap();
        let head = TernaryResidualSidecar::new(
            coordinates::<1>(1),
            vec![TernaryRow::from_weights(&[0], 1).unwrap()],
        )
        .unwrap();
        assert_eq!(
            head.add_to_baseline(&state, &[i32::MAX], 1),
            Err(HybridAltError::IntegerOverflow)
        );
        assert_eq!(
            head.add_to_baseline(&state, &[0], i32::MAX),
            Ok(vec![i32::MAX])
        );
        assert_eq!(
            head.add_to_baseline(&state, &[1], i32::MIN),
            Ok(vec![i32::MIN + 1])
        );
    }

    #[test]
    fn byte_bpc_scoring_is_evaluation_only_and_stable() {
        let uniform = [0_i32; 256];
        let loss = byte_log_loss_bits(&uniform, 42, 1.0).unwrap();
        assert!((loss - 8.0).abs() < 1e-12);
        let mut informed = [0_i32; 256];
        informed[42] = 16;
        assert!(byte_log_loss_bits(&informed, 42, 1.0).unwrap() < loss);
        assert!(byte_log_loss_bits(&informed, 42, 0.0).is_err());
    }

    #[test]
    fn feature_output_capacity_is_bounded() {
        let head = TernaryResidualSidecar::<128>::new(
            coordinates::<128>(128),
            (0..256)
                .map(|i| {
                    TernaryRow::from_weights(&[if i % 2 == 0 { 1 } else { -1 }; 128], 0)
                        .unwrap()
                })
                .collect(),
        )
        .unwrap();
        assert_eq!(head.class_count(), 256);
        assert_eq!(head.input_bits(), 128);
        let state = SemanticState::<128>::zeroed().unwrap();
        assert_eq!(head.scores(&state), Ok(vec![0; 256]));
    }
}
