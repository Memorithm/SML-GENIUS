//! Causal, TRAIN-frozen first-order bit-Markov baseline for supervised
//! research diagnostics. Evaluation reads only *past observed* true labels,
//! so teacher-forced scoring is NOT equivalent to autonomous LM generation.
//! Each u64 word contains 64 independent lane labels at one time step.
//! Never join adjacent documents or fit from DEV.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkovPriorError {
    TooFewSteps,
    CountOverflow,
}
impl std::fmt::Display for MarkovPriorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MarkovPriorError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrozenBitMarkovPrior {
    after_zero_is_one: bool,
    after_one_is_one: bool,
}
impl FrozenBitMarkovPrior {
    /// Fit exclusively on TRAIN_fit. Do not pass a concatenation of documents.
    pub fn fit_train(targets: &[u64]) -> Result<Self, MarkovPriorError> {
        if targets.len() < 2 {
            return Err(MarkovPriorError::TooFewSteps);
        }
        let total = u64::try_from(targets.len())
            .map_err(|_| MarkovPriorError::CountOverflow)?
            .checked_mul(64)
            .ok_or(MarkovPriorError::CountOverflow)?;
        let positive = targets.iter().try_fold(0_u64, |sum, word| {
            sum.checked_add(u64::from(word.count_ones()))
                .ok_or(MarkovPriorError::CountOverflow)
        })?;
        let global_one = u128::from(positive) * 2 > u128::from(total);
        let mut counts = [[0_u64; 2]; 2];
        for pair in targets.windows(2) {
            let (previous, current) = (pair[0], pair[1]);
            let transition_counts = [
                [
                    (!previous & !current).count_ones(),
                    (!previous & current).count_ones(),
                ],
                [
                    (previous & !current).count_ones(),
                    (previous & current).count_ones(),
                ],
            ];
            for from in 0..2 {
                for to in 0..2 {
                    counts[from][to] = counts[from][to]
                        .checked_add(u64::from(transition_counts[from][to]))
                        .ok_or(MarkovPriorError::CountOverflow)?;
                }
            }
        }
        let choose_one = |zero: u64, one: u64| -> bool {
            if zero == 0 && one == 0 {
                global_one
            } else {
                one > zero
            }
        };
        Ok(Self {
            after_zero_is_one: choose_one(counts[0][0], counts[0][1]),
            after_one_is_one: choose_one(counts[1][0], counts[1][1]),
        })
    }

    /// Conditional prediction from the previous observed bit only.
    #[must_use]
    pub fn predict_one(&self, previous_one: bool) -> bool {
        if previous_one {
            self.after_one_is_one
        } else {
            self.after_zero_is_one
        }
    }

    /// Causal evaluation on a disjoint document. Its first time step is
    /// *not* scored and no previous label crosses document boundaries.
    pub fn score_document(&self, targets: &[u64]) -> Result<(u64, u64), MarkovPriorError> {
        if targets.len() < 2 {
            return Err(MarkovPriorError::TooFewSteps);
        }
        let positions = u64::try_from(targets.len() - 1)
            .map_err(|_| MarkovPriorError::CountOverflow)?
            .checked_mul(64)
            .ok_or(MarkovPriorError::CountOverflow)?;
        let mut errors = 0_u64;
        for pair in targets.windows(2) {
            let mut prediction = 0_u64;
            if self.after_zero_is_one {
                prediction |= !pair[0];
            }
            if self.after_one_is_one {
                prediction |= pair[0];
            }
            errors = errors
                .checked_add(u64::from((prediction ^ pair[1]).count_ones()))
                .ok_or(MarkovPriorError::CountOverflow)?;
        }
        Ok((errors, positions))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_labels_prefer_persistence() {
        let stream = [0, 0, 0, u64::MAX, u64::MAX, u64::MAX];
        let prior = FrozenBitMarkovPrior::fit_train(&stream).unwrap();
        assert!(!prior.predict_one(false));
        assert!(prior.predict_one(true));
        assert_eq!(
            prior.score_document(&[0, 0, u64::MAX, u64::MAX]).unwrap(),
            (64, 192)
        );
    }

    #[test]
    fn alternating_state_is_learned() {
        let stream = [0, u64::MAX, 0, u64::MAX, 0, u64::MAX];
        let prior = FrozenBitMarkovPrior::fit_train(&stream).unwrap();
        assert!(prior.predict_one(false));
        assert!(!prior.predict_one(true));
        assert_eq!(prior.score_document(&stream).unwrap(), (0, 320));
    }

    #[test]
    fn absent_previous_state_uses_frozen_global_majority() {
        let prior = FrozenBitMarkovPrior::fit_train(&[0, 0, 0, 0]).unwrap();
        assert!(!prior.predict_one(false));
        assert!(!prior.predict_one(true));
        assert_eq!(prior.score_document(&[u64::MAX, 0, 0]).unwrap(), (0, 128));
    }

    #[test]
    fn conditional_ties_deterministically_choose_zero() {
        let prior = FrozenBitMarkovPrior::fit_train(&[0, u64::MAX, 0, 0]).unwrap();
        assert!(!prior.predict_one(false));
        assert!(!prior.predict_one(true));
    }

    #[test]
    fn score_matches_independent_scalar_bit_lane_oracle() {
        let train = [
            0x59a0_f402_734e_c166,
            0xca7b_0912_55d1_22ff,
            0x132a_c983_d081_bc32,
            0xead4_33b9_9948_a159,
            0x808f_c26c_0d01_1351,
        ];
        let dev = [
            0x0023_48af_55ff_1234,
            0xffdd_aa62_a861_4539,
            0x8292_5521_aac9_0011,
            0xdead_beef_a543_8830,
        ];
        let prior = FrozenBitMarkovPrior::fit_train(&train).unwrap();
        let (native_errors, count) = prior.score_document(&dev).unwrap();
        assert_eq!(count, (dev.len() as u64 - 1) * 64);
        let mut scalar_errors = 0_u64;
        for lane in 0..64 {
            for time in 1..dev.len() {
                let prev = ((dev[time - 1] >> lane) & 1) == 1;
                let truth = ((dev[time] >> lane) & 1) == 1;
                scalar_errors += u64::from(prior.predict_one(prev) != truth);
            }
        }
        assert_eq!(native_errors, scalar_errors);
    }

    #[test]
    fn document_boundaries_never_create_transitions() {
        let prior = FrozenBitMarkovPrior::fit_train(&[0, 0, 0]).unwrap();
        assert_eq!(prior.score_document(&[0, u64::MAX]).unwrap(), (64, 64));
        assert_eq!(prior.score_document(&[u64::MAX, 0]).unwrap(), (0, 64));
        assert_eq!(prior.score_document(&[0, 0]).unwrap(), (0, 64));
        assert_eq!(
            prior.score_document(&[u64::MAX]),
            Err(MarkovPriorError::TooFewSteps)
        );
        assert_eq!(
            FrozenBitMarkovPrior::fit_train(&[]),
            Err(MarkovPriorError::TooFewSteps)
        );
    }
}
