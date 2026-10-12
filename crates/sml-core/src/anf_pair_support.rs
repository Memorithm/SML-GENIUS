//! A6-I4: conservative pairwise Boolean support envelope over PASSIVE inputs
//! for native ANF hypothesis committees. This is a coverage guard, not
//! proof of generalization, correct bank membership or learned monomials.
//!
//! Explicit scientific negative: even seeing all 4 assignments of every
//! input PAIR does not show that a specific TRIPLE (1,1,1) ever occurred.
//! A degree-three missing term may remain confidently undetected.

use crate::anf_passive_committee::{PassiveCommittee, PassiveCommitteeError, PassivePrediction};
use std::mem::size_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnfPairSupportError {
    Width,
    Budget,
    Allocation,
    EmptyTraining,
    NotSealed,
    Sealed,
    OutOfSync,
    Committee(PassiveCommitteeError),
}
impl From<PassiveCommitteeError> for AnfPairSupportError {
    fn from(e: PassiveCommitteeError) -> Self {
        Self::Committee(e)
    }
}
impl std::fmt::Display for AnfPairSupportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AnfPairSupportError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MissingPair {
    left: usize,
    right: usize,
    /// Bit indexes (left_bit*2 + right_bit), each from 0..4.
    missing_mask: u8,
}

/// TRAIN-only packed observation columns. Once sealed, only pairwise
/// unsupported combinations are scanned during a TEST prediction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnfPairSupportGuard {
    columns: Vec<Vec<u64>>,
    max_train_samples: usize,
    max_missing_pairs: usize,
    samples: usize,
    missing: Option<Vec<MissingPair>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PairSupportEvidence {
    pub novel_pairs: usize,
    pub pairwise_supported: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PairGuardedPrediction {
    pub committee: PassivePrediction,
    pub pair: PairSupportEvidence,
    pub bank_adequate: bool,
    /// Confidence >= threshold AND adequate supplied bank on TRAIN
    /// AND all input pair configurations have previously appeared.
    pub guarded_prediction: Option<bool>,
}

impl AnfPairSupportGuard {
    /// Budgets are enforced before allocation. No k×k dense matrix is kept
    /// at inference: only TRAIN columns and sparse missing pairs.
    pub fn new(
        width: usize,
        max_train_samples: usize,
        max_pair_slots: usize,
        max_missing_pairs: usize,
    ) -> Result<Self, AnfPairSupportError> {
        if !(2..=1024).contains(&width) {
            return Err(AnfPairSupportError::Width);
        }
        if max_train_samples == 0
            || max_train_samples > 512
            || max_missing_pairs == 0
            || max_pair_slots == 0
        {
            return Err(AnfPairSupportError::Budget);
        }
        let pair_slots = width
            .checked_mul(width - 1)
            .and_then(|x| x.checked_div(2))
            .ok_or(AnfPairSupportError::Budget)?;
        if pair_slots > max_pair_slots {
            return Err(AnfPairSupportError::Budget);
        }
        let words = max_train_samples.div_ceil(64);
        let mut columns = Vec::new();
        columns
            .try_reserve_exact(width)
            .map_err(|_| AnfPairSupportError::Allocation)?;
        for _ in 0..width {
            let mut col = Vec::new();
            col.try_reserve_exact(words)
                .map_err(|_| AnfPairSupportError::Allocation)?;
            col.resize(words, 0u64);
            columns.push(col);
        }
        Ok(Self {
            columns,
            max_train_samples,
            max_missing_pairs,
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
    pub fn missing_pair_count(&self) -> Option<usize> {
        self.missing.as_ref().map(Vec::len)
    }

    /// Logical primitive array payload: excludes Vec headers, allocator
    /// capacities and transient computation/scratch. Not a process RSS.
    #[must_use]
    pub fn logical_payload_bytes(&self) -> usize {
        let columns = self.columns.len() * self.columns[0].len() * size_of::<u64>();
        let absent = self
            .missing
            .as_ref()
            .map_or(0, |entries| entries.len() * size_of::<MissingPair>());
        columns + absent
    }

    /// TRAIN-only append; validate ALL preconditions before changing bits.
    pub fn observe(&mut self, input: &[bool]) -> Result<(), AnfPairSupportError> {
        if input.len() != self.fan_in() {
            return Err(AnfPairSupportError::Width);
        }
        if self.missing.is_some() {
            return Err(AnfPairSupportError::Sealed);
        }
        if self.samples >= self.max_train_samples {
            return Err(AnfPairSupportError::Budget);
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

    /// Seal a read-only support summary. Each i<j joint pattern is
    /// computed by intersections of bit-sliced TRAIN columns, so the
    /// O(k²·ceil(n/64)) pair scan happens once, not per future query.
    /// Building too many absent pair entries aborts with no state commit.
    pub fn seal(&mut self) -> Result<(), AnfPairSupportError> {
        if self.missing.is_some() {
            return Err(AnfPairSupportError::Sealed);
        }
        if self.samples == 0 {
            return Err(AnfPairSupportError::EmptyTraining);
        }
        let width = self.fan_in();
        let words = self.samples.div_ceil(64);
        let remaining = self.samples % 64;
        let mut missing = Vec::new();
        for left in 0..width {
            for right in left + 1..width {
                let mut seen = 0u8;
                for word in 0..words {
                    let valid = if word + 1 == words && remaining != 0 {
                        (1u64 << remaining) - 1
                    } else {
                        u64::MAX
                    };
                    let a = self.columns[left][word];
                    let b = self.columns[right][word];
                    if ((!a & !b) & valid) != 0 {
                        seen |= 1;
                    }
                    if ((!a & b) & valid) != 0 {
                        seen |= 2;
                    }
                    if ((a & !b) & valid) != 0 {
                        seen |= 4;
                    }
                    if ((a & b) & valid) != 0 {
                        seen |= 8;
                    }
                    if seen == 15 {
                        break;
                    }
                }
                if seen != 15 {
                    if missing.len() >= self.max_missing_pairs {
                        return Err(AnfPairSupportError::Budget);
                    }
                    missing
                        .try_reserve(1)
                        .map_err(|_| AnfPairSupportError::Allocation)?;
                    missing.push(MissingPair {
                        left,
                        right,
                        missing_mask: !seen & 15,
                    });
                }
            }
        }
        self.missing = Some(missing);
        Ok(())
    }

    /// Evaluate only input pair novelty. An entirely unseen triple
    /// whose PAIRS all occurred is intentionally INVISIBLE here.
    pub fn assess(&self, input: &[bool]) -> Result<PairSupportEvidence, AnfPairSupportError> {
        if input.len() != self.fan_in() {
            return Err(AnfPairSupportError::Width);
        }
        let missing = self.missing.as_ref().ok_or(AnfPairSupportError::NotSealed)?;
        let novel_pairs = missing
            .iter()
            .filter(|pair| {
                let mask = 1u8 << ((usize::from(input[pair.left]) << 1)
                    | usize::from(input[pair.right]));
                pair.missing_mask & mask != 0
            })
            .count();
        Ok(PairSupportEvidence {
            novel_pairs,
            pairwise_supported: novel_pairs == 0,
        })
    }

    /// The real posterior committee is neither refit nor modified by
    /// novelty assessment. TRAIN-history lengths must be synchronized.
    pub fn predict_guarded(
        &self,
        committee: &PassiveCommittee,
        input: &[bool],
        confidence_threshold: f64,
        mismatch_threshold: f64,
    ) -> Result<PairGuardedPrediction, AnfPairSupportError> {
        if input.len() != self.fan_in() || committee.fan_in() != self.fan_in() {
            return Err(AnfPairSupportError::Width);
        }
        if committee.samples() != self.samples {
            return Err(AnfPairSupportError::OutOfSync);
        }
        let pair = self.assess(input)?;
        let committee_prediction = committee.predict(input, confidence_threshold)?;
        let bank_adequate = committee.bank_adequate(mismatch_threshold)?;
        let guarded_prediction = if pair.pairwise_supported && bank_adequate {
            committee_prediction.selective_prediction
        } else {
            None
        };
        Ok(PairGuardedPrediction {
            committee: committee_prediction,
            pair,
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
        let candidates = [
            vec![vec![2], vec![3]],
            vec![vec![2], vec![3], vec![0, 1]],
            vec![vec![2], vec![3], vec![4, 5]],
            vec![vec![2], vec![3], vec![0, 1], vec![4, 5]],
        ]
        .into_iter()
        .map(|terms| WideAnfGate::from_monomials((0..8).collect(), terms, 8, limits).unwrap())
        .collect::<Vec<_>>();
        PassiveCommittee::new(candidates, vec![4.0, 2.0, 2.0, 1.0], 1.0 / 16.0, 512).unwrap()
    }

    #[test]
    fn pairwise_novelty_catches_missing_joint_activation_even_with_all_marginals_seen() {
        let mut committee = committee();
        let mut guard = AnfPairSupportGuard::new(8, 512, 600_000, 8_192).unwrap();
        let mut rng = SplitMix64::new(777);
        for step in 0..512 {
            let mut input = (0..8).map(|_| rng.next_u64() & 1 != 0).collect::<Vec<_>>();
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
            }
            committee.observe(&input, input[2] ^ input[3]).unwrap();
            guard.observe(&input).unwrap();
        }
        guard.seal().unwrap();
        assert_eq!(guard.samples(), 512);
        assert_eq!(guard.missing_pair_count(), Some(1));
        let mut query = vec![false; 8];
        query[6] = true;
        query[7] = true;
        let evidence = guard.predict_guarded(&committee, &query, 0.90, 0.125).unwrap();
        assert_eq!(evidence.pair.novel_pairs, 1);
        assert_eq!(evidence.committee.selective_prediction, Some(false));
        assert_eq!(evidence.guarded_prediction, None);
        query[6] = false;
        assert!(guard.assess(&query).unwrap().pairwise_supported);
    }

    #[test]
    fn mandatory_triple_blind_spot_despite_all_observed_pair_patterns() {
        let mut committee = committee();
        let mut guard = AnfPairSupportGuard::new(8, 512, 600_000, 8_192).unwrap();
        let mut rng = SplitMix64::new(314_159);
        for step in 0..512 {
            let mut x = (0..8).map(|_| rng.next_u64() & 1 != 0).collect::<Vec<_>>();
            let triple = step % 7;
            x[5] = triple & 4 != 0;
            x[6] = triple & 2 != 0;
            x[7] = triple & 1 != 0;
            assert!(!(x[5] && x[6] && x[7]));
            committee.observe(&x, x[2] ^ x[3]).unwrap();
            guard.observe(&x).unwrap();
        }
        guard.seal().unwrap();
        assert_eq!(guard.missing_pair_count(), Some(0));
        let mut unseen = vec![false; 8];
        unseen[5] = true;
        unseen[6] = true;
        unseen[7] = true;
        let guarded = guard.predict_guarded(&committee, &unseen, 0.90, 0.125).unwrap();
        assert!(guarded.pair.pairwise_supported);
        assert_eq!(guarded.pair.novel_pairs, 0);
        assert_eq!(guarded.guarded_prediction, Some(false));
        // Missing true function = h0 XOR x5*x6*x7 => TRUE.
        assert_ne!(guarded.guarded_prediction, Some(true));
    }

    #[test]
    fn invalid_width_unsynchronized_or_unsealed_and_sample_budgets_fail_closed() {
        assert_eq!(
            AnfPairSupportGuard::new(1, 512, 600_000, 8_192),
            Err(AnfPairSupportError::Width)
        );
        assert_eq!(
            AnfPairSupportGuard::new(1024, 512, 523_775, 8_192),
            Err(AnfPairSupportError::Budget)
        );
        let mut guard = AnfPairSupportGuard::new(8, 1, 600_000, 8_192).unwrap();
        assert_eq!(guard.assess(&[false; 8]), Err(AnfPairSupportError::NotSealed));
        assert_eq!(guard.seal(), Err(AnfPairSupportError::EmptyTraining));
        let original = guard.clone();
        assert_eq!(guard.observe(&[false; 7]), Err(AnfPairSupportError::Width));
        assert_eq!(guard, original);
        guard.observe(&[false; 8]).unwrap();
        let observed = guard.clone();
        assert_eq!(guard.observe(&[true; 8]), Err(AnfPairSupportError::Budget));
        assert_eq!(guard, observed);
        let committee = committee();
        guard.seal().unwrap();
        let frozen = guard.clone();
        assert_eq!(guard.observe(&[true; 8]), Err(AnfPairSupportError::Sealed));
        assert_eq!(
            guard.predict_guarded(&committee, &[false; 8], 0.9, 0.125),
            Err(AnfPairSupportError::OutOfSync)
        );
        assert_eq!(guard, frozen);
    }

    #[test]
    fn exceeding_missing_pair_budget_never_installs_incomplete_support() {
        let mut guard = AnfPairSupportGuard::new(8, 512, 600_000, 1).unwrap();
        guard.observe(&[false; 8]).unwrap();
        let original = guard.clone();
        assert_eq!(guard.seal(), Err(AnfPairSupportError::Budget));
        assert_eq!(guard, original);
    }
}
