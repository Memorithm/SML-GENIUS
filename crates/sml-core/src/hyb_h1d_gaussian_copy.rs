//! COPY-G: Gaussian-weighted causal copy expert against COPY-1 fixed 20/80.
//! The only learned/calibrated quantity is a **single scalar amplitude**
//! fitted to TRAIN bit-decision applicability counts. No code-loss labels,
//! no future current-byte bits, no floating-point operations in inference.
//!
//! Protocol: docs/HYB-H1D-GAUSSIAN-COPY-PREREGISTRATION.md.

use crate::hyb_h1d_copy::{CopyExpert, COPY_MAX_SUFFIX};

/// Gaussian kernel, Q1024, mu=4 matching bytes, sigma=1.5 bytes.
/// Values are rounded once in the preregistered design.
pub const GAUSSIAN_KERNEL_Q1024: [u16; COPY_MAX_SUFFIX + 1] = [0, 139, 421, 820, 1024];
pub const MIX_DENOMINATOR: u16 = 10_240;
pub const FIXED_COPY_EXPOSURE: u16 = 2_048;
/// A difference ≤0.5 percentage point per active bit is allowed.
/// Integer ratio 1/200 of the full mixture denominator = 51.2 units.
const MAX_MATCH_ERROR_NUMERATOR: u128 = 1;
const MAX_MATCH_ERROR_DENOMINATOR: u128 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GaussianCalibrationError {
    NoCopyOpportunities,
    /// Current Gaussian envelope cannot match F20 exposure fairly.
    ActivityMismatch,
    CounterOverflow,
}

/// TRAIN-only counts for *usable* copy bit decisions, by longest match length.
/// This is independent of H-1d weights, probability/loss scores and targets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyActivityHistogram {
    counts: [u64; COPY_MAX_SUFFIX + 1],
}

impl CopyActivityHistogram {
    /// Read TRAIN documents only, preserving document boundaries. All
    /// candidate lookups come from past bytes. The bit-prefix check uses
    /// solely bits that would already be known to an arithmetic decoder.
    pub fn observe_train_document(&mut self, data: &[u8]) -> Result<(), GaussianCalibrationError> {
        let mut copy = CopyExpert::default();
        for (position, &byte) in data.iter().enumerate() {
            if position >= 2 {
                copy.prepare_byte();
                let matched = copy.longest_match();
                if matched != 0 {
                    for depth in 0..8 {
                        let known_prefix = if depth == 0 {
                            0
                        } else {
                            byte & (u8::MAX << (8 - depth))
                        };
                        if copy.probability(depth, known_prefix).is_some() {
                            self.counts[matched] = self.counts[matched]
                                .checked_add(1)
                                .ok_or(GaussianCalibrationError::CounterOverflow)?;
                        }
                    }
                }
            }
            copy.push_decoded(byte);
        }
        Ok(())
    }

    #[must_use]
    pub fn counts(&self) -> [u64; COPY_MAX_SUFFIX + 1] {
        self.counts
    }

    #[must_use]
    pub fn active_decisions(&self) -> u64 {
        self.counts.iter().copied().sum()
    }
}

/// Frozen, audit-ready TRAIN-calibrated Gaussian weights. No further
/// mutation is possible after constructing this profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GaussianCopyProfile {
    weights: [u16; COPY_MAX_SUFFIX + 1],
    amplitude: u16,
}

impl GaussianCopyProfile {
    /// Deterministic, label-free matching of aggregate copy exposure to
    /// the fixed 20/80 competitor. Evaluate all admissible amplitudes
    /// (only 10,241 candidates once at TRAIN calibration time).
    ///
    /// As a fail-closed rule, reject when a 0.5 percentage-point match
    /// cannot be achieved with the frozen Gaussian family.
    pub fn fit_train_only(
        histogram: &CopyActivityHistogram,
    ) -> Result<Self, GaussianCalibrationError> {
        let counts = histogram.counts();
        let active = u128::from(histogram.active_decisions());
        if active == 0 {
            return Err(GaussianCalibrationError::NoCopyOpportunities);
        }
        let target = active * u128::from(FIXED_COPY_EXPOSURE);
        let mut best_amplitude = 0u16;
        let mut smallest_error = u128::MAX;
        for amplitude in 0..=MIX_DENOMINATOR {
            let effective = (1..=COPY_MAX_SUFFIX)
                .map(|length| {
                    u128::from(counts[length]) * u128::from(weight_at_amplitude(length, amplitude))
                })
                .sum::<u128>();
            let error = effective.abs_diff(target);
            if error < smallest_error {
                best_amplitude = amplitude;
                smallest_error = error;
            }
        }
        let maximum_allowed = active * u128::from(MIX_DENOMINATOR) * MAX_MATCH_ERROR_NUMERATOR
            / MAX_MATCH_ERROR_DENOMINATOR;
        if smallest_error > maximum_allowed {
            return Err(GaussianCalibrationError::ActivityMismatch);
        }
        let mut weights = [0u16; COPY_MAX_SUFFIX + 1];
        for (length, weight) in weights.iter_mut().enumerate().skip(1) {
            *weight = weight_at_amplitude(length, best_amplitude);
        }
        Ok(Self {
            weights,
            amplitude: best_amplitude,
        })
    }

    /// Frozen intensity per match length, with denominator 10,240.
    #[must_use]
    pub fn weight_for_match(&self, length: usize) -> u16 {
        self.weights[length.min(COPY_MAX_SUFFIX)]
    }

    #[must_use]
    pub fn weights(&self) -> [u16; COPY_MAX_SUFFIX + 1] {
        self.weights
    }

    #[must_use]
    pub fn amplitude(&self) -> u16 {
        self.amplitude
    }

    #[must_use]
    pub fn exposure_units(&self, hist: &CopyActivityHistogram) -> u128 {
        hist.counts()
            .iter()
            .enumerate()
            .map(|(length, &count)| u128::from(count) * u128::from(self.weights[length]))
            .sum()
    }
}

fn weight_at_amplitude(length: usize, amplitude: u16) -> u16 {
    let numerator = u32::from(GAUSSIAN_KERNEL_Q1024[length]) * u32::from(amplitude) + 512;
    (numerator / 1024).min(u32::from(MIX_DENOMINATOR)) as u16
}

/// Integer Q32 probability mixture, with a Gaussian-profile copy strength.
/// Missing/unusable copy remains the exact neural probability.
#[must_use]
pub fn mix_gaussian_probability(neural: u32, copy: Option<u32>, copy_weight: u16) -> u32 {
    assert!(copy_weight <= MIX_DENOMINATOR);
    if copy_weight == 0 {
        return neural;
    }
    let Some(copy) = copy else {
        return neural;
    };
    let numerator = u64::from(neural) * u64::from(MIX_DENOMINATOR - copy_weight)
        + u64::from(copy) * u64::from(copy_weight)
        + u64::from(MIX_DENOMINATOR / 2);
    (numerator / u64::from(MIX_DENOMINATOR)).clamp(1, u64::from(u32::MAX) - 1) as u32
}

/// Independent oracle: derive length from previous bytes, without a
/// CopyExpert ring or the cached candidate selection.
#[must_use]
pub fn oracle_longest_match(history: &[u8]) -> usize {
    let retained = &history[history
        .len()
        .saturating_sub(crate::hyb_h1d_copy::COPY_WINDOW)..];
    let mut best = 0;
    for end in 1..retained.len() {
        let mut matched = 0;
        for offset in 0..end.min(COPY_MAX_SUFFIX) {
            if retained[end - offset - 1] != retained[retained.len() - offset - 1] {
                break;
            }
            matched += 1;
        }
        best = best.max(matched);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyb_h1d_copy::{oracle_probability, CopyExpert};

    #[test]
    fn gaussian_kernel_is_deterministic_and_monotone() {
        assert_eq!(GAUSSIAN_KERNEL_Q1024, [0, 139, 421, 820, 1024]);
        for amplitude in [0, 17, 2048, MIX_DENOMINATOR] {
            let values: Vec<_> = (0..=COPY_MAX_SUFFIX)
                .map(|length| weight_at_amplitude(length, amplitude))
                .collect();
            assert!(values.windows(2).all(|p| p[0] <= p[1]));
            assert_eq!(values[0], 0);
            assert!(values.iter().all(|&v| v <= MIX_DENOMINATOR));
        }
    }

    #[test]
    fn exposure_matches_fixed_twenty_percent_without_labels() {
        let mut hist = CopyActivityHistogram::default();
        hist.observe_train_document(&b"abababababababab".repeat(128))
            .unwrap();
        let profile = GaussianCopyProfile::fit_train_only(&hist).unwrap();
        let active = hist.active_decisions();
        assert!(active > 0);
        let target = u128::from(active) * u128::from(FIXED_COPY_EXPOSURE);
        let discrepancy = profile.exposure_units(&hist).abs_diff(target);
        assert!(
            discrepancy <= u128::from(active) * u128::from(MIX_DENOMINATOR) / 200,
            "target matching must be train-only and within 0.5 percentage points"
        );
        assert!(profile.weights().windows(2).all(|p| p[0] <= p[1]));
    }

    #[test]
    fn empty_training_evidence_is_rejected() {
        let hist = CopyActivityHistogram::default();
        assert_eq!(
            GaussianCopyProfile::fit_train_only(&hist),
            Err(GaussianCalibrationError::NoCopyOpportunities)
        );
    }

    #[test]
    fn low_confidence_only_cannot_fake_twenty_percent_match() {
        let hist = CopyActivityHistogram {
            counts: [0, 500, 0, 0, 0],
        };
        assert_eq!(
            GaussianCopyProfile::fit_train_only(&hist),
            Err(GaussianCalibrationError::ActivityMismatch)
        );
    }

    #[test]
    fn gaussian_probability_mixing_is_bounded_and_changes_with_length() {
        let mut hist = CopyActivityHistogram::default();
        hist.observe_train_document(&b"abababab".repeat(100))
            .unwrap();
        let profile = GaussianCopyProfile::fit_train_only(&hist).unwrap();
        let neural = 2_000_000_000;
        let copy = 3_000_000_000;
        assert_eq!(
            mix_gaussian_probability(neural, None, profile.weight_for_match(4)),
            neural
        );
        assert_eq!(mix_gaussian_probability(neural, Some(copy), 0), neural);
        assert_eq!(mix_gaussian_probability(neural, Some(copy), 10_240), copy);
        assert!(
            mix_gaussian_probability(neural, Some(copy), profile.weight_for_match(4))
                > mix_gaussian_probability(neural, Some(copy), profile.weight_for_match(1))
        );
    }

    #[test]
    fn independent_match_length_oracle_agrees_across_ring_wrap() {
        let mut copy = CopyExpert::default();
        let mut history = Vec::new();
        let mut x = 0x91e10da5_u64;
        for position in 0..650 {
            copy.prepare_byte();
            assert_eq!(
                copy.longest_match(),
                oracle_longest_match(&history),
                "mismatch at position {position}"
            );
            let byte = if position % 9 <= 6 {
                b"banana_"[position % 7]
            } else {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            };
            for depth in 0..8 {
                let known = if depth == 0 {
                    0
                } else {
                    byte & (0xff << (8 - depth))
                };
                assert_eq!(
                    copy.probability(depth, known),
                    oracle_probability(&history, depth, known),
                );
            }
            copy.push_decoded(byte);
            history.push(byte);
        }
    }
}
