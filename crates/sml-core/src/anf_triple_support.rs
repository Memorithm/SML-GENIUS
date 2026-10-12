//! A6-I5: exact third-order TRAIN input-support envelope for a supplied
//! native ANF committee. This detects novelty, not hidden teacher structure.
//!
//! Fourth-order terms can remain observationally invisible even if all
//! 3-coordinate Boolean patterns were seen. Full-width construction is
//! intentionally refused when the combinatorial budget is inadequate.

use crate::anf_passive_committee::{PassiveCommittee, PassiveCommitteeError, PassivePrediction};
use std::mem::size_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnfTripleSupportError {
    Width,
    Budget,
    Allocation,
    EmptyTraining,
    NotSealed,
    Sealed,
    OutOfSync,
    Committee(PassiveCommitteeError),
}

impl From<PassiveCommitteeError> for AnfTripleSupportError {
    fn from(error: PassiveCommitteeError) -> Self {
        Self::Committee(error)
    }
}

impl std::fmt::Display for AnfTripleSupportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AnfTripleSupportError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MissingTriple {
    first: usize,
    second: usize,
    third: usize,
    /// Bit index: 4*first_value + 2*second_value + third_value.
    missing_mask: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TripleSupportEvidence {
    pub novel_triples: usize,
    pub triplewise_supported: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TripleGuardedPrediction {
    pub committee: PassivePrediction,
    pub triple: TripleSupportEvidence,
    pub bank_adequate: bool,
    /// Accepted only if confidence, TRAIN bank fit and input coverage all pass.
    pub guarded_prediction: Option<bool>,
}

/// A full-width strength-three input-support summary. No dimensional subset
/// is silently used; all C(width,3) combinations must fit the caller budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnfTripleSupportGuard {
    columns: Vec<Vec<u64>>,
    max_train_samples: usize,
    max_missing_triples: usize,
    samples: usize,
    missing: Option<Vec<MissingTriple>>,
}

impl AnfTripleSupportGuard {
    pub fn new(
        width: usize,
        max_train_samples: usize,
        max_triple_slots: usize,
        max_missing_triples: usize,
    ) -> Result<Self, AnfTripleSupportError> {
        if !(3..=1024).contains(&width) {
            return Err(AnfTripleSupportError::Width);
        }
        if max_train_samples == 0
            || max_train_samples > 512
            || max_triple_slots == 0
            || max_missing_triples == 0
        {
            return Err(AnfTripleSupportError::Budget);
        }
        let slots = width
            .checked_mul(width - 1)
            .and_then(|n| n.checked_mul(width - 2))
            .and_then(|n| n.checked_div(6))
            .ok_or(AnfTripleSupportError::Budget)?;
        if slots > max_triple_slots {
            return Err(AnfTripleSupportError::Budget);
        }

        let words = max_train_samples.div_ceil(64);
        let mut columns = Vec::new();
        columns
            .try_reserve_exact(width)
            .map_err(|_| AnfTripleSupportError::Allocation)?;
        for _ in 0..width {
            let mut column = Vec::new();
            column
                .try_reserve_exact(words)
                .map_err(|_| AnfTripleSupportError::Allocation)?;
            column.resize(words, 0u64);
            columns.push(column);
        }
        Ok(Self {
            columns,
            max_train_samples,
            max_missing_triples,
            samples: 0,
            missing: None,
        })
    }

    #[must_use]
    pub fn fan_in(&self) -> usize {
        self.columns.len()
    }

    #[must_use]
    pub fn samples(&self) -> usize {
        self.samples
    }

    #[must_use]
    pub fn missing_triple_count(&self) -> Option<usize> {
        self.missing.as_ref().map(Vec::len)
    }

    /// Only logical primitive array payload; excludes Vec structures,
    /// unused capacity, temporary sealing memory and process RSS.
    #[must_use]
    pub fn logical_payload_bytes(&self) -> usize {
        let columns = self.columns.len() * self.columns[0].len() * size_of::<u64>();
        let missing = self
            .missing
            .as_ref()
            .map_or(0, |entries| entries.len() * size_of::<MissingTriple>());
        columns + missing
    }

    /// Append a TRAIN-only input after validating width and capacity.
    pub fn observe(&mut self, input: &[bool]) -> Result<(), AnfTripleSupportError> {
        if input.len() != self.fan_in() {
            return Err(AnfTripleSupportError::Width);
        }
        if self.missing.is_some() {
            return Err(AnfTripleSupportError::Sealed);
        }
        if self.samples == self.max_train_samples {
            return Err(AnfTripleSupportError::Budget);
        }
        let word = self.samples / 64;
        let mask = 1u64 << (self.samples % 64);
        for (column, &value) in self.columns.iter_mut().zip(input.iter()) {
            if value {
                column[word] |= mask;
            }
        }
        self.samples += 1;
        Ok(())
    }

    /// Examine every i<j<k, then store only the combinations missing from
    /// TRAIN. Build a temporary vector and commit atomically after all
    /// resource checks, never leaving a partially sealed guard.
    pub fn seal(&mut self) -> Result<(), AnfTripleSupportError> {
        if self.missing.is_some() {
            return Err(AnfTripleSupportError::Sealed);
        }
        if self.samples == 0 {
            return Err(AnfTripleSupportError::EmptyTraining);
        }
        let width = self.fan_in();
        let words = self.samples.div_ceil(64);
        let remaining = self.samples % 64;
        let mut missing = Vec::new();
        for first in 0..width {
            for second in first + 1..width {
                for third in second + 1..width {
                    let mut seen = 0u8;
                    for word in 0..words {
                        let valid = if word + 1 == words && remaining != 0 {
                            (1u64 << remaining) - 1
                        } else {
                            u64::MAX
                        };
                        let a = self.columns[first][word];
                        let b = self.columns[second][word];
                        let c = self.columns[third][word];
                        for code in 0..8u8 {
                            let aa = if code & 4 != 0 { a } else { !a };
                            let bb = if code & 2 != 0 { b } else { !b };
                            let cc = if code & 1 != 0 { c } else { !c };
                            if aa & bb & cc & valid != 0 {
                                seen |= 1u8 << code;
                            }
                        }
                        if seen == u8::MAX {
                            break;
                        }
                    }
                    if seen != u8::MAX {
                        if missing.len() >= self.max_missing_triples {
                            return Err(AnfTripleSupportError::Budget);
                        }
                        missing
                            .try_reserve(1)
                            .map_err(|_| AnfTripleSupportError::Allocation)?;
                        missing.push(MissingTriple {
                            first,
                            second,
                            third,
                            missing_mask: !seen,
                        });
                    }
                }
            }
        }
        self.missing = Some(missing);
        Ok(())
    }

    /// Input-only novelty, not a posterior or teacher-truth certificate.
    pub fn assess(&self, input: &[bool]) -> Result<TripleSupportEvidence, AnfTripleSupportError> {
        if input.len() != self.fan_in() {
            return Err(AnfTripleSupportError::Width);
        }
        let missing = self
            .missing
            .as_ref()
            .ok_or(AnfTripleSupportError::NotSealed)?;
        let novel_triples = missing
            .iter()
            .filter(|triple| {
                let code = (usize::from(input[triple.first]) << 2)
                    | (usize::from(input[triple.second]) << 1)
                    | usize::from(input[triple.third]);
                triple.missing_mask & (1u8 << code) != 0
            })
            .count();
        Ok(TripleSupportEvidence {
            novel_triples,
            triplewise_supported: novel_triples == 0,
        })
    }

    /// Only the real native committee computes predicted probabilities.
    /// Both components must contain an equal number of TRAIN observations.
    pub fn predict_guarded(
        &self,
        committee: &PassiveCommittee,
        input: &[bool],
        confidence_threshold: f64,
        mismatch_threshold: f64,
    ) -> Result<TripleGuardedPrediction, AnfTripleSupportError> {
        if input.len() != self.fan_in() || committee.fan_in() != self.fan_in() {
            return Err(AnfTripleSupportError::Width);
        }
        if committee.samples() != self.samples {
            return Err(AnfTripleSupportError::OutOfSync);
        }
        let triple = self.assess(input)?;
        let prediction = committee.predict(input, confidence_threshold)?;
        let bank_adequate = committee.bank_adequate(mismatch_threshold)?;
        let guarded_prediction = if triple.triplewise_supported && bank_adequate {
            prediction.selective_prediction
        } else {
            None
        };
        Ok(TripleGuardedPrediction {
            committee: prediction,
            triple,
            bank_adequate,
            guarded_prediction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        anf_pair_support::AnfPairSupportGuard,
        anf_wide_sparse::{WideAnfGate, WideAnfLimits},
    };

    fn committee() -> PassiveCommittee {
        let limits = WideAnfLimits::new(2048, 256, 8192, 8192, 8192);
        let candidates = [
            vec![vec![2], vec![3]],
            vec![vec![2], vec![3], vec![0, 1]],
            vec![vec![2], vec![3], vec![0]],
            vec![vec![2], vec![3], vec![1]],
        ]
        .into_iter()
        .map(|terms| WideAnfGate::from_monomials((0..8).collect(), terms, 8, limits).unwrap())
        .collect::<Vec<_>>();
        PassiveCommittee::new(candidates, vec![4.0, 2.0, 2.0, 1.0], 1.0 / 16.0, 512).unwrap()
    }

    #[test]
    fn missing_triple_is_detected_when_every_pair_pattern_is_present() {
        let mut guard = AnfTripleSupportGuard::new(8, 512, 350_000, 100_000).unwrap();
        let mut pair = AnfPairSupportGuard::new(8, 512, 600_000, 8192).unwrap();
        let mut committee = committee();
        let training = (0..256usize)
            .filter(|value| value >> 5 != 7)
            .collect::<Vec<_>>();
        assert_eq!(training.len(), 224);
        for step in 0..512 {
            let bits = training[step % training.len()];
            let input = (0..8).map(|i| bits & (1 << i) != 0).collect::<Vec<_>>();
            committee.observe(&input, input[2] ^ input[3]).unwrap();
            guard.observe(&input).unwrap();
            pair.observe(&input).unwrap();
        }
        guard.seal().unwrap();
        pair.seal().unwrap();
        assert_eq!(guard.missing_triple_count(), Some(1));
        assert_eq!(pair.missing_pair_count(), Some(0));
        let query = vec![true; 8];
        assert!(pair.assess(&query).unwrap().pairwise_supported);
        let result = guard.predict_guarded(&committee, &query, 0.9, 0.125).unwrap();
        assert!(!result.triple.triplewise_supported);
        assert_eq!(result.triple.novel_triples, 1);
        assert_eq!(result.committee.selective_prediction, Some(false));
        assert_eq!(result.guarded_prediction, None);
        assert_ne!(result.committee.selective_prediction, Some(true));
        assert_eq!(guard.samples(), 512);
    }

    #[test]
    fn fourth_order_hidden_term_passes_every_triple_guard() {
        let mut guard = AnfTripleSupportGuard::new(8, 512, 350_000, 100_000).unwrap();
        let training = (0..256usize)
            .filter(|value| value >> 4 != 15)
            .collect::<Vec<_>>();
        assert_eq!(training.len(), 240);
        for step in 0..512 {
            let bits = training[step % training.len()];
            let input = (0..8).map(|i| bits & (1 << i) != 0).collect::<Vec<_>>();
            guard.observe(&input).unwrap();
        }
        guard.seal().unwrap();
        assert_eq!(guard.missing_triple_count(), Some(0));
        let query = vec![true; 8];
        assert!(guard.assess(&query).unwrap().triplewise_supported);
        let h0 = query[2] ^ query[3];
        let hidden = h0 ^ (query[4] && query[5] && query[6] && query[7]);
        assert_ne!(h0, hidden);
    }

    #[test]
    fn budgets_and_failed_seal_do_not_commit_partial_state() {
        assert_eq!(
            AnfTripleSupportGuard::new(256, 512, 350_000, 100_000).unwrap_err(),
            AnfTripleSupportError::Budget
        );
        assert_eq!(
            AnfTripleSupportGuard::new(1024, 512, 350_000, 100_000).unwrap_err(),
            AnfTripleSupportError::Budget
        );
        let mut guard = AnfTripleSupportGuard::new(8, 1, 350_000, 1).unwrap();
        assert_eq!(guard.seal().unwrap_err(), AnfTripleSupportError::EmptyTraining);
        guard.observe(&[false; 8]).unwrap();
        let before = guard.clone();
        assert_eq!(guard.seal().unwrap_err(), AnfTripleSupportError::Budget);
        assert_eq!(guard, before);
        assert_eq!(
            guard.assess(&[false; 8]).unwrap_err(),
            AnfTripleSupportError::NotSealed
        );
        assert_eq!(
            guard.observe(&[false; 8]).unwrap_err(),
            AnfTripleSupportError::Budget
        );
        let mut completed = AnfTripleSupportGuard::new(3, 1, 350_000, 1).unwrap();
        completed.observe(&[false; 3]).unwrap();
        completed.seal().unwrap();
        assert_eq!(
            completed.observe(&[false; 3]).unwrap_err(),
            AnfTripleSupportError::Sealed
        );
    }

    #[test]
    fn committee_and_guard_train_histories_must_be_synchronized() {
        let mut guard = AnfTripleSupportGuard::new(8, 2, 350_000, 100_000).unwrap();
        let committee = committee();
        guard.observe(&[false; 8]).unwrap();
        guard.seal().unwrap();
        assert_eq!(
            guard.predict_guarded(&committee, &[false; 8], 0.9, 0.125)
                .unwrap_err(),
            AnfTripleSupportError::OutOfSync
        );
    }
}
