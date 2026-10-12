//! A6-I3: empirical univariate support envelope for a supplied
//! native WideAnfGate passive posterior committee.
//!
//! Important negative guarantee: observing 0 and 1 separately for every
//! input bit does NOT cover pairwise co-activation, higher-order contexts,
//! arbitrary teacher functions or shifted target labels. This is a
//! fail-closed marginal novelty alarm, NOT proof of global correctness.

use crate::anf_passive_committee::{PassiveCommittee, PassiveCommitteeError, PassivePrediction};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnfSupportError {
    Width,
    Budget,
    EmptyTraining,
    OutOfSync,
    Committee(PassiveCommitteeError),
}

impl From<PassiveCommitteeError> for AnfSupportError {
    fn from(value: PassiveCommitteeError) -> Self {
        Self::Committee(value)
    }
}
impl std::fmt::Display for AnfSupportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AnfSupportError {}

/// One byte per input coordinate. Bit 0 signals observed false, bit 1
/// signals observed true. No pairwise interaction state is retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnfMarginalSupportGuard {
    observed: Vec<u8>,
    samples: usize,
    max_samples: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarginalSupportEvidence {
    pub novel_dimensions: usize,
    pub marginally_supported: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GuardedAnfPrediction {
    /// The native committee's prediction is preserved for explicit comparison.
    pub committee: PassivePrediction,
    pub marginal: MarginalSupportEvidence,
    /// Adequacy measured only from TRAIN errors in the supplied committee.
    pub bank_adequate: bool,
    /// Predicted label only if (committee confidence ∧ bank adequacy ∧ support).
    pub guarded_prediction: Option<bool>,
}

impl AnfMarginalSupportGuard {
    pub fn new(width: usize, max_samples: usize) -> Result<Self, AnfSupportError> {
        if width == 0 {
            return Err(AnfSupportError::Width);
        }
        if max_samples == 0 {
            return Err(AnfSupportError::Budget);
        }
        let mut observed = Vec::new();
        observed
            .try_reserve_exact(width)
            .map_err(|_| AnfSupportError::Budget)?;
        observed.resize(width, 0u8);
        Ok(Self {
            observed,
            samples: 0,
            max_samples,
        })
    }

    #[must_use]
    pub fn fan_in(&self) -> usize {
        self.observed.len()
    }
    #[must_use]
    pub fn samples(&self) -> usize {
        self.samples
    }
    #[must_use]
    pub fn logical_payload_bytes(&self) -> usize {
        self.observed.len()
    }

    /// TRAIN-only, transactional: validate width and budget before mutation.
    pub fn observe(&mut self, input: &[bool]) -> Result<(), AnfSupportError> {
        if input.len() != self.observed.len() {
            return Err(AnfSupportError::Width);
        }
        if self.samples >= self.max_samples {
            return Err(AnfSupportError::Budget);
        }
        for (seen, &bit) in self.observed.iter_mut().zip(input.iter()) {
            *seen |= if bit { 2 } else { 1 };
        }
        self.samples += 1;
        Ok(())
    }

    /// INPUT-ONLY, NO LABELS: treat any previously unseen per-coordinate
    /// Boolean value as novelty. This cannot spot unseen (x_i,x_j) pairs
    /// once both marginal values of i and j have appeared somewhere.
    pub fn assess(&self, input: &[bool]) -> Result<MarginalSupportEvidence, AnfSupportError> {
        if input.len() != self.observed.len() {
            return Err(AnfSupportError::Width);
        }
        if self.samples == 0 {
            return Err(AnfSupportError::EmptyTraining);
        }
        let novel_dimensions = self
            .observed
            .iter()
            .zip(input.iter())
            .filter(|(seen, bit)| {
                let required = if **bit { 2 } else { 1 };
                **seen & required == 0
            })
            .count();
        Ok(MarginalSupportEvidence {
            novel_dimensions,
            marginally_supported: novel_dimensions == 0,
        })
    }

    /// A conservative policy ALONGSIDE the existing Bayesian committee:
    /// this is not an alternative posterior, likelihood or proof of safety.
    ///
    /// Guard and committee must have seen the same number of TRAIN inputs,
    /// and identical widths; callers are responsible for aligning inputs.
    /// This API never touches any hidden teacher bit or future output.
    pub fn predict_guarded(
        &self,
        committee: &PassiveCommittee,
        input: &[bool],
        confidence_threshold: f64,
        mismatch_threshold: f64,
    ) -> Result<GuardedAnfPrediction, AnfSupportError> {
        if committee.fan_in() != self.fan_in() || input.len() != self.fan_in() {
            return Err(AnfSupportError::Width);
        }
        if self.samples() != committee.samples() {
            return Err(AnfSupportError::OutOfSync);
        }
        let marginal = self.assess(input)?;
        let committee_prediction = committee.predict(input, confidence_threshold)?;
        let bank_adequate = committee.bank_adequate(mismatch_threshold)?;
        let guarded_prediction = if bank_adequate && marginal.marginally_supported {
            committee_prediction.selective_prediction
        } else {
            None
        };
        Ok(GuardedAnfPrediction {
            committee: committee_prediction,
            marginal,
            bank_adequate,
            guarded_prediction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        anf_wide_sparse::{WideAnfGate, WideAnfLimits},
        toy_tasks::SplitMix64,
    };

    fn committee() -> PassiveCommittee {
        let limits = WideAnfLimits::new(2048, 256, 8192, 8192, 8192);
        let funcs = [
            vec![vec![2], vec![3]],
            vec![vec![2], vec![3], vec![0, 1]],
            vec![vec![2], vec![3], vec![4, 5]],
            vec![vec![2], vec![3], vec![0, 1], vec![4, 5]],
        ]
        .into_iter()
        .map(|terms| WideAnfGate::from_monomials((0..8).collect(), terms, 8, limits).unwrap())
        .collect::<Vec<_>>();
        PassiveCommittee::new(funcs, vec![4.0, 2.0, 2.0, 1.0], 1.0 / 16.0, 512).unwrap()
    }

    #[test]
    fn unseen_coordinate_value_refused_but_known_inputs_preserved() {
        let mut guard = AnfMarginalSupportGuard::new(8, 512).unwrap();
        let mut committee = committee();
        let mut rng = SplitMix64::new(0x19de);
        for _ in 0..512 {
            let mut input = (0..8).map(|_| rng.next_u64() & 1 != 0).collect::<Vec<_>>();
            input[6] = false;
            let label = input[2] ^ input[3];
            committee.observe(&input, label).unwrap();
            guard.observe(&input).unwrap();
        }
        let mut known = vec![false; 8];
        let covered = guard
            .predict_guarded(&committee, &known, 0.90, 0.125)
            .unwrap();
        assert!(covered.marginal.marginally_supported);
        assert_eq!(covered.guarded_prediction, Some(false));
        known[6] = true;
        known[7] = true;
        let novel = guard
            .predict_guarded(&committee, &known, 0.90, 0.125)
            .unwrap();
        assert_eq!(novel.marginal.novel_dimensions, 1);
        assert!(novel.guarded_prediction.is_none());
        assert_eq!(novel.committee.selective_prediction, Some(false));
    }

    #[test]
    fn marginal_guard_does_not_certify_a_previously_unseen_pair() {
        let mut guard = AnfMarginalSupportGuard::new(8, 512).unwrap();
        let mut committee = committee();
        for step in 0..512usize {
            let mut input = (0..8)
                .map(|i| step & (1 << (i % 6)) != 0)
                .collect::<Vec<_>>();
            match step % 3 {
                0 => {
                    input[6] = false;
                    input[7] = false;
                }
                1 => {
                    input[6] = true;
                    input[7] = false;
                }
                _ => {
                    input[6] = false;
                    input[7] = true;
                }
            };
            let label = input[2] ^ input[3];
            committee.observe(&input, label).unwrap();
            guard.observe(&input).unwrap();
        }
        let mut unseen_pair = vec![false; 8];
        unseen_pair[6] = true;
        unseen_pair[7] = true;
        let result = guard
            .predict_guarded(&committee, &unseen_pair, 0.90, 0.125)
            .unwrap();
        assert!(result.marginal.marginally_supported);
        assert_eq!(result.marginal.novel_dimensions, 0);
        assert_eq!(result.guarded_prediction, Some(false));
        // A missing teacher h0 XOR x6&x7 produces TRUE here,
        // despite every individual bit value being represented in TRAIN.
    }

    #[test]
    fn shape_budget_and_async_committee_snapshots_fail_closed() {
        assert_eq!(
            AnfMarginalSupportGuard::new(0, 2),
            Err(AnfSupportError::Width)
        );
        assert_eq!(
            AnfMarginalSupportGuard::new(8, 0),
            Err(AnfSupportError::Budget)
        );
        let mut guard = AnfMarginalSupportGuard::new(8, 1).unwrap();
        let before = guard.clone();
        assert_eq!(guard.observe(&[false; 7]), Err(AnfSupportError::Width));
        assert_eq!(guard, before);
        assert_eq!(
            guard.assess(&[false; 8]),
            Err(AnfSupportError::EmptyTraining)
        );
        guard.observe(&[false; 8]).unwrap();
        let full = guard.clone();
        assert_eq!(guard.observe(&[true; 8]), Err(AnfSupportError::Budget));
        assert_eq!(guard, full);
        let c = committee();
        assert_eq!(
            guard.predict_guarded(&c, &[false; 8], 0.9, 0.125),
            Err(AnfSupportError::OutOfSync)
        );
        assert_eq!(guard.logical_payload_bytes(), 8);
    }
}
