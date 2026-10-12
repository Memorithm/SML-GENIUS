//! A6-I6: budgeted, EXACT fourth-order TRAIN pattern support for
//! caller-provided watchlist tuples. NOT full C(k,4) coverage.
//!
//! Watchlists are a prior supplied externally BEFORE training or testing;
//! the true hidden teacher monomial is NEVER recovered by this guard.
//! An unseen interaction outside the watchlist can trigger confident errors.
//! This module does not change Bayesian weights or gate coefficients.

use crate::anf_passive_committee::{PassiveCommittee, PassiveCommitteeError, PassivePrediction};
use std::mem::size_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FourWatchError {
    Width,
    Budget,
    Allocation,
    DuplicateTuple,
    InvalidTuple,
    EmptyTraining,
    Sealed,
    NotSealed,
    OutOfSync,
    Committee(PassiveCommitteeError),
}
impl From<PassiveCommitteeError> for FourWatchError {
    fn from(error: PassiveCommitteeError) -> Self {
        Self::Committee(error)
    }
}
impl std::fmt::Display for FourWatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FourWatchError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WatchedTuple {
    /// Four canonical physical indices, WITHOUT revealing a true ANF term.
    indices: [usize; 4],
    /// 16 exact Boolean input assignments observed in TRAIN.
    seen: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FourWatchEvidence {
    /// How many of the PREDECLARED watch tuples have an unseen query pattern.
    pub novel_watches: usize,
    pub watched_supported: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FourWatchPrediction {
    pub committee: PassivePrediction,
    pub support: FourWatchEvidence,
    pub bank_adequate: bool,
    /// None means ABSTAIN; this is not counted as a correct prediction.
    pub guarded_prediction: Option<bool>,
}

/// Exact O(watched tuples) retained history; k may be 1024 without
/// allocating or enumerating the 45,545,029,376 possible fourth-order tuples.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnfBudgetedFourSupportGuard {
    width: usize,
    watch: Vec<WatchedTuple>,
    max_train_samples: usize,
    samples: usize,
    sealed: bool,
}

impl AnfBudgetedFourSupportGuard {
    pub fn new(
        width: usize,
        watched: Vec<[usize; 4]>,
        max_watches: usize,
        max_train_samples: usize,
    ) -> Result<Self, FourWatchError> {
        if !(8..=1024).contains(&width) {
            return Err(FourWatchError::Width);
        }
        if watched.is_empty()
            || max_watches == 0
            || max_watches > 16
            || watched.len() > max_watches
            || max_train_samples == 0
            || max_train_samples > 512
        {
            return Err(FourWatchError::Budget);
        }
        let mut watch = Vec::new();
        watch
            .try_reserve_exact(watched.len())
            .map_err(|_| FourWatchError::Allocation)?;
        for mut indices in watched {
            indices.sort_unstable();
            if indices[3] >= width || indices.windows(2).any(|w| w[0] == w[1]) {
                return Err(FourWatchError::InvalidTuple);
            }
            watch.push(WatchedTuple { indices, seen: 0 });
        }
        watch.sort_unstable_by_key(|tuple| tuple.indices);
        if watch.windows(2).any(|w| w[0].indices == w[1].indices) {
            return Err(FourWatchError::DuplicateTuple);
        }
        Ok(Self {
            width,
            watch,
            max_train_samples,
            samples: 0,
            sealed: false,
        })
    }

    #[must_use]
    pub fn fan_in(&self) -> usize {
        self.width
    }
    #[must_use]
    pub fn samples(&self) -> usize {
        self.samples
    }
    #[must_use]
    pub fn watched_count(&self) -> usize {
        self.watch.len()
    }
    #[must_use]
    pub fn is_sealed(&self) -> bool {
        self.sealed
    }
    /// Logical retained tuple array bytes only, not allocator RSS/capacity.
    #[must_use]
    pub fn logical_payload_bytes(&self) -> usize {
        self.watch.len() * size_of::<WatchedTuple>()
    }
    #[must_use]
    pub fn observed_patterns(&self, index: usize) -> Option<u32> {
        self.watch.get(index).map(|tuple| tuple.seen.count_ones())
    }

    fn code(indices: [usize; 4], input: &[bool]) -> u8 {
        indices
            .into_iter()
            .fold(0u8, |code, index| (code << 1) | u8::from(input[index]))
    }

    /// Record a TRAIN-only tuple pattern; checks happen before mutation.
    pub fn observe(&mut self, input: &[bool]) -> Result<(), FourWatchError> {
        if input.len() != self.width {
            return Err(FourWatchError::Width);
        }
        if self.sealed {
            return Err(FourWatchError::Sealed);
        }
        if self.samples == self.max_train_samples {
            return Err(FourWatchError::Budget);
        }
        for tuple in &mut self.watch {
            tuple.seen |= 1u16 << Self::code(tuple.indices, input);
        }
        self.samples += 1;
        Ok(())
    }

    pub fn seal(&mut self) -> Result<(), FourWatchError> {
        if self.sealed {
            return Err(FourWatchError::Sealed);
        }
        if self.samples == 0 {
            return Err(FourWatchError::EmptyTraining);
        }
        self.sealed = true;
        Ok(())
    }

    /// Test input-only novelty on the EXPLICIT watchlist, no TEST label access.
    pub fn assess(&self, input: &[bool]) -> Result<FourWatchEvidence, FourWatchError> {
        if input.len() != self.width {
            return Err(FourWatchError::Width);
        }
        if !self.sealed {
            return Err(FourWatchError::NotSealed);
        }
        let novel = self
            .watch
            .iter()
            .filter(|tuple| {
                let bit = 1u16 << Self::code(tuple.indices, input);
                tuple.seen & bit == 0
            })
            .count();
        Ok(FourWatchEvidence {
            novel_watches: novel,
            watched_supported: novel == 0,
        })
    }

    /// Combine genuine TRAIN-fitted PassiveCommittee predictions with
    /// posterior confidence, TRAIN bank adequacy and input-only watch support.
    ///
    /// No use of the hidden teacher or TEST labels; a true function missing
    /// outside the watched coordinates remains undetectable here.
    pub fn predict_guarded(
        &self,
        committee: &PassiveCommittee,
        input: &[bool],
        confidence_threshold: f64,
        mismatch_threshold: f64,
    ) -> Result<FourWatchPrediction, FourWatchError> {
        if input.len() != self.width || committee.fan_in() != self.width {
            return Err(FourWatchError::Width);
        }
        if committee.samples() != self.samples {
            return Err(FourWatchError::OutOfSync);
        }
        let support = self.assess(input)?;
        let prediction = committee.predict(input, confidence_threshold)?;
        let bank_adequate = committee.bank_adequate(mismatch_threshold)?;
        let guarded_prediction = if support.watched_supported && bank_adequate {
            prediction.selective_prediction
        } else {
            None
        };
        Ok(FourWatchPrediction {
            committee: prediction,
            support,
            bank_adequate,
            guarded_prediction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anf_wide_sparse::{WideAnfGate, WideAnfLimits};

    fn candidates() -> Vec<WideAnfGate> {
        let limits = WideAnfLimits::new(2048, 256, 8192, 8192, 8192);
        [
            vec![vec![0], vec![1]],
            vec![vec![0], vec![1], vec![0, 1]],
            vec![vec![0], vec![1], vec![2]],
            vec![vec![0], vec![1], vec![3]],
        ]
        .into_iter()
        .map(|terms| WideAnfGate::from_monomials((0..8).collect(), terms, 8, limits).unwrap())
        .collect()
    }

    fn committee() -> PassiveCommittee {
        PassiveCommittee::new(candidates(), vec![4.0, 2.0, 2.0, 1.0], 1.0 / 16.0, 512).unwrap()
    }

    #[test]
    fn observes_all_pattern_bits_except_registered_quad1111() {
        let mut guard = AnfBudgetedFourSupportGuard::new(1024, vec![[7, 4, 6, 5]], 1, 512).unwrap();
        for pattern in 0..16 {
            if pattern == 15 {
                continue;
            }
            let mut input = vec![false; 1024];
            for (i, physical) in [4, 5, 6, 7].into_iter().enumerate() {
                input[physical] = pattern & (1 << (3 - i)) != 0;
            }
            guard.observe(&input).unwrap();
        }
        assert_eq!(guard.observed_patterns(0), Some(15));
        guard.seal().unwrap();
        let mut holdout = vec![false; 1024];
        for i in 4..8 {
            holdout[i] = true;
        }
        assert_eq!(
            guard.assess(&holdout).unwrap(),
            FourWatchEvidence {
                novel_watches: 1,
                watched_supported: false
            }
        );
        holdout[4] = false;
        assert_eq!(guard.assess(&holdout).unwrap().novel_watches, 0);
        assert_eq!(guard.watched_count(), 1);
        assert_eq!(
            guard.logical_payload_bytes(),
            std::mem::size_of::<WatchedTuple>()
        );
    }

    #[test]
    fn an_unwatched_quad_can_be_invisible_with_all_watched_patterns_seen() {
        let mut guard = AnfBudgetedFourSupportGuard::new(8, vec![[4, 5, 6, 7]], 1, 512).unwrap();
        for pattern in 0..16 {
            let mut input = [false; 8];
            for (i, physical) in [4, 5, 6, 7].into_iter().enumerate() {
                input[physical] = pattern & (1 << (3 - i)) != 0;
            }
            guard.observe(&input).unwrap();
        }
        guard.seal().unwrap();
        assert_eq!(guard.observed_patterns(0), Some(16));
        let all = [true; 8];
        assert_eq!(guard.assess(&all).unwrap().novel_watches, 0);
        // h0 XOR x0x1x2x3 differs from h0 at this query.
        let h0 = candidates().remove(0);
        assert_ne!(
            h0.evaluate_scalar(&all).unwrap(),
            !h0.evaluate_scalar(&all).unwrap()
        );
    }

    #[test]
    fn all_reject_paths_preserve_state_and_budgets() {
        assert_eq!(
            AnfBudgetedFourSupportGuard::new(7, vec![[0, 1, 2, 3]], 1, 512),
            Err(FourWatchError::Width)
        );
        assert_eq!(
            AnfBudgetedFourSupportGuard::new(8, vec![[0, 0, 2, 3]], 1, 512),
            Err(FourWatchError::InvalidTuple)
        );
        assert_eq!(
            AnfBudgetedFourSupportGuard::new(8, vec![[0, 1, 2, 3], [3, 2, 1, 0]], 2, 512),
            Err(FourWatchError::DuplicateTuple)
        );
        assert_eq!(
            AnfBudgetedFourSupportGuard::new(8, vec![[0, 1, 2, 8]], 1, 512),
            Err(FourWatchError::InvalidTuple)
        );
        assert_eq!(
            AnfBudgetedFourSupportGuard::new(8, vec![], 1, 512),
            Err(FourWatchError::Budget)
        );
        assert_eq!(
            AnfBudgetedFourSupportGuard::new(8, vec![[0, 1, 2, 3]], 1, 0),
            Err(FourWatchError::Budget)
        );
        let mut guard = AnfBudgetedFourSupportGuard::new(8, vec![[4, 5, 6, 7]], 1, 1).unwrap();
        let previous = guard.clone();
        assert_eq!(guard.seal(), Err(FourWatchError::EmptyTraining));
        assert_eq!(guard, previous);
        assert_eq!(guard.assess(&[false; 8]), Err(FourWatchError::NotSealed));
        assert_eq!(guard.observe(&[false; 7]), Err(FourWatchError::Width));
        assert_eq!(guard, previous);
        guard.observe(&[false; 8]).unwrap();
        let after = guard.clone();
        assert_eq!(guard.observe(&[true; 8]), Err(FourWatchError::Budget));
        assert_eq!(guard, after);
        guard.seal().unwrap();
        let sealed = guard.clone();
        assert_eq!(guard.observe(&[true; 8]), Err(FourWatchError::Sealed));
        assert_eq!(guard.seal(), Err(FourWatchError::Sealed));
        assert_eq!(guard, sealed);
    }

    #[test]
    fn trained_committee_must_be_synchronized_with_watched_history() {
        let mut guard = AnfBudgetedFourSupportGuard::new(8, vec![[4, 5, 6, 7]], 1, 512).unwrap();
        let mut committee = committee();
        let state = [false; 8];
        guard.observe(&state).unwrap();
        guard.seal().unwrap();
        assert_eq!(
            guard.predict_guarded(&committee, &state, 0.90, 0.125),
            Err(FourWatchError::OutOfSync)
        );
        committee.observe(&state, false).unwrap();
        let p = guard
            .predict_guarded(&committee, &state, 0.90, 0.125)
            .unwrap();
        assert!(p.bank_adequate);
        assert!(p.support.watched_supported);
        assert_eq!(p.guarded_prediction, Some(false));
    }
}
