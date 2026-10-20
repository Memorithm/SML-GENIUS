//! TRAIN-only constant-prior-relative qualification for spectral candidates.
//!
//! This *supplements* the older 49%-chance F2b3 diagnostic. An imbalanced
//! IID label stream can be predicted with low error by a constant classifier;
//! passing a fixed 49% gate alone must not certify discovered input structure.
//!
//! This module cannot authenticate temporal splits or external data rights.
//! Its caller must fit the constant prior on TRAIN_fit only, evaluate two
//! disjoint TRAIN_cal partitions, and freeze before constructing DEV.
use crate::spectral_method_gate::SpectralMethod;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriorSpectralError {
    InvalidConfig,
    InvalidCounts,
    CounterOverflow,
    AlreadyFrozen,
}
impl std::fmt::Display for PriorSpectralError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PriorSpectralError {}

/// The only learned statistic is the majority label on TRAIN_fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrozenConstantBitPrior {
    predicts_one: bool,
}
impl FrozenConstantBitPrior {
    pub fn fit_train(train_ones: u64, train_examples: u64) -> Result<Self, PriorSpectralError> {
        if train_examples == 0 || train_ones > train_examples {
            return Err(PriorSpectralError::InvalidCounts);
        }
        Ok(Self {
            predicts_one: 2_u128 * u128::from(train_ones) > u128::from(train_examples),
        })
    }
    #[must_use]
    pub fn predicts_one(&self) -> bool {
        self.predicts_one
    }
    /// Count errors on paired calibration labels without fitting to them.
    pub fn count_errors(&self, ones: u64, examples: u64) -> Result<u64, PriorSpectralError> {
        if examples == 0 || ones > examples {
            return Err(PriorSpectralError::InvalidCounts);
        }
        Ok(if self.predicts_one {
            examples - ones
        } else {
            ones
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaselineSpectralGateConfig {
    pub minimum_examples_per_partition: u64,
    pub minimum_improvement_over_prior_ppm: u32,
    pub minimum_w3_advantage_ppm: u32,
    pub maximum_error_ppm: u32,
    pub maximum_hybrid_ns_per_example: u64,
    pub maximum_w3_ns_per_example: u64,
}
impl BaselineSpectralGateConfig {
    pub fn validate(self) -> Result<(), PriorSpectralError> {
        if self.minimum_examples_per_partition == 0
            || self.minimum_improvement_over_prior_ppm == 0
            || self.minimum_improvement_over_prior_ppm > 1_000_000
            || self.minimum_w3_advantage_ppm == 0
            || self.minimum_w3_advantage_ppm > 1_000_000
            || self.maximum_error_ppm > 1_000_000
            || self.maximum_hybrid_ns_per_example == 0
            || self.maximum_w3_ns_per_example == 0
        {
            return Err(PriorSpectralError::InvalidConfig);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Evidence {
    examples: u64,
    baseline_errors: u64,
    hybrid_errors: u64,
    w3_errors: u64,
    hybrid_ns: u64,
    w3_ns: u64,
}
impl Evidence {
    fn observe(
        self,
        examples: u64,
        baseline_errors: u64,
        hybrid_errors: u64,
        w3_errors: u64,
        hybrid_ns: u64,
        w3_ns: u64,
    ) -> Result<Self, PriorSpectralError> {
        if examples == 0
            || baseline_errors > examples
            || hybrid_errors > examples
            || w3_errors > examples
            || hybrid_ns == 0
            || w3_ns == 0
        {
            return Err(PriorSpectralError::InvalidCounts);
        }
        Ok(Self {
            examples: self
                .examples
                .checked_add(examples)
                .ok_or(PriorSpectralError::CounterOverflow)?,
            baseline_errors: self
                .baseline_errors
                .checked_add(baseline_errors)
                .ok_or(PriorSpectralError::CounterOverflow)?,
            hybrid_errors: self
                .hybrid_errors
                .checked_add(hybrid_errors)
                .ok_or(PriorSpectralError::CounterOverflow)?,
            w3_errors: self
                .w3_errors
                .checked_add(w3_errors)
                .ok_or(PriorSpectralError::CounterOverflow)?,
            hybrid_ns: self
                .hybrid_ns
                .checked_add(hybrid_ns)
                .ok_or(PriorSpectralError::CounterOverflow)?,
            w3_ns: self
                .w3_ns
                .checked_add(w3_ns)
                .ok_or(PriorSpectralError::CounterOverflow)?,
        })
    }
    fn within_budget(&self, errors: u64, elapsed: u64, max_ppm: u32, max_ns: u64) -> bool {
        u128::from(errors) * 1_000_000 <= u128::from(max_ppm) * u128::from(self.examples)
            && u128::from(elapsed) <= u128::from(max_ns) * u128::from(self.examples)
    }
    fn beats_prior(&self, candidate_errors: u64, margin_ppm: u32) -> bool {
        self.baseline_errors >= candidate_errors
            && u128::from(self.baseline_errors - candidate_errors) * 1_000_000
                >= u128::from(margin_ppm) * u128::from(self.examples)
    }
    fn w3_beats_hybrid(&self, margin_ppm: u32) -> bool {
        self.hybrid_errors >= self.w3_errors
            && u128::from(self.hybrid_errors - self.w3_errors) * 1_000_000
                >= u128::from(margin_ppm) * u128::from(self.examples)
    }
}

/// A single paired TRAIN-cal observation with the prior frozen from TRAIN_fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PairedTrainBaselineObservation {
    pub examples: u64,
    pub baseline_errors: u64,
    pub hybrid_errors: u64,
    pub w3_errors: u64,
    pub hybrid_ns: u64,
    pub w3_ns: u64,
}

#[derive(Clone, Debug)]
pub struct TrainOnlySpectralBaselineGate {
    config: BaselineSpectralGateConfig,
    partitions: [Evidence; 2],
    route: Option<SpectralMethod>,
}
impl TrainOnlySpectralBaselineGate {
    pub fn new(config: BaselineSpectralGateConfig) -> Result<Self, PriorSpectralError> {
        config.validate()?;
        Ok(Self {
            config,
            partitions: [Evidence::default(); 2],
            route: None,
        })
    }
    /// Add paired *supervised TRAIN-only* CAL evidence, never labels from DEV.
    /// Input baseline errors MUST come from a frozen TRAIN_fit label prior.
    pub fn observe_train(
        &mut self,
        partition: usize,
        evidence: PairedTrainBaselineObservation,
    ) -> Result<(), PriorSpectralError> {
        if self.route.is_some() {
            return Err(PriorSpectralError::AlreadyFrozen);
        }
        let previous = self
            .partitions
            .get(partition)
            .ok_or(PriorSpectralError::InvalidCounts)?;
        let updated = previous.observe(
            evidence.examples,
            evidence.baseline_errors,
            evidence.hybrid_errors,
            evidence.w3_errors,
            evidence.hybrid_ns,
            evidence.w3_ns,
        )?;
        self.partitions[partition] = updated;
        Ok(())
    }
    pub fn freeze(&mut self) -> Result<SpectralMethod, PriorSpectralError> {
        if self.route.is_some() {
            return Err(PriorSpectralError::AlreadyFrozen);
        }
        let enough = self
            .partitions
            .iter()
            .all(|p| p.examples >= self.config.minimum_examples_per_partition);
        let hybrid_qualified = enough
            && self.partitions.iter().all(|p| {
                p.within_budget(
                    p.hybrid_errors,
                    p.hybrid_ns,
                    self.config.maximum_error_ppm,
                    self.config.maximum_hybrid_ns_per_example,
                ) && p.beats_prior(
                    p.hybrid_errors,
                    self.config.minimum_improvement_over_prior_ppm,
                )
            });
        let w3_qualified = enough
            && self.partitions.iter().all(|p| {
                p.within_budget(
                    p.w3_errors,
                    p.w3_ns,
                    self.config.maximum_error_ppm,
                    self.config.maximum_w3_ns_per_example,
                ) && p.beats_prior(p.w3_errors, self.config.minimum_improvement_over_prior_ppm)
                    && p.w3_beats_hybrid(self.config.minimum_w3_advantage_ppm)
            });
        let chosen = if w3_qualified {
            SpectralMethod::Walsh3Exhaustive
        } else if hybrid_qualified {
            SpectralMethod::HybridW2SketchMitm
        } else {
            SpectralMethod::Abstain
        };
        self.route = Some(chosen);
        Ok(chosen)
    }
    #[must_use]
    pub fn route(&self) -> Option<SpectralMethod> {
        self.route
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> BaselineSpectralGateConfig {
        BaselineSpectralGateConfig {
            minimum_examples_per_partition: 40_960,
            minimum_improvement_over_prior_ppm: 10_000,
            minimum_w3_advantage_ppm: 10_000,
            maximum_error_ppm: 490_000,
            maximum_hybrid_ns_per_example: 500_000,
            maximum_w3_ns_per_example: 1_000_000,
        }
    }
    #[test]
    fn imbalanced_iid_majority_is_not_spectral_gain() {
        let prior = FrozenConstantBitPrior::fit_train(16_384, 81_920).unwrap();
        assert!(!prior.predicts_one());
        let mut gate = TrainOnlySpectralBaselineGate::new(config()).unwrap();
        for part in 0..2 {
            let errors = prior.count_errors(8_192, 40_960).unwrap();
            assert_eq!(errors, 8_192);
            gate.observe_train(
                part,
                PairedTrainBaselineObservation {
                    examples: 40_960,
                    baseline_errors: errors,
                    hybrid_errors: errors,
                    w3_errors: errors,
                    hybrid_ns: 10_000,
                    w3_ns: 20_000,
                },
            )
            .unwrap();
        }
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::Abstain);
        assert!(gate
            .observe_train(
                0,
                PairedTrainBaselineObservation {
                    examples: 40_960,
                    baseline_errors: 10,
                    hybrid_errors: 9,
                    w3_errors: 8,
                    hybrid_ns: 10,
                    w3_ns: 20,
                }
            )
            .is_err());
    }
    #[test]
    fn balanced_iid_small_improvement_is_not_evidence() {
        let prior = FrozenConstantBitPrior::fit_train(20_480, 40_960).unwrap();
        assert!(!prior.predicts_one());
        let mut gate = TrainOnlySpectralBaselineGate::new(config()).unwrap();
        for part in 0..2 {
            gate.observe_train(
                part,
                PairedTrainBaselineObservation {
                    examples: 40_960,
                    baseline_errors: 20_480,
                    hybrid_errors: 20_400,
                    w3_errors: 20_360,
                    hybrid_ns: 10_000,
                    w3_ns: 20_000,
                },
            )
            .unwrap();
        }
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::Abstain);
    }
    #[test]
    fn forty_percent_noisy_parity_routes_real_w3_gain() {
        let prior = FrozenConstantBitPrior::fit_train(20_480, 40_960).unwrap();
        let mut gate = TrainOnlySpectralBaselineGate::new(config()).unwrap();
        for part in 0..2 {
            let constant = prior.count_errors(20_480, 40_960).unwrap();
            gate.observe_train(
                part,
                PairedTrainBaselineObservation {
                    examples: 40_960,
                    baseline_errors: constant,
                    hybrid_errors: 20_300,
                    w3_errors: 16_400,
                    hybrid_ns: 10_000,
                    w3_ns: 20_000,
                },
            )
            .unwrap();
        }
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::Walsh3Exhaustive);
        assert_eq!(gate.route(), Some(SpectralMethod::Walsh3Exhaustive));
        assert_eq!(gate.freeze(), Err(PriorSpectralError::AlreadyFrozen));
    }
    #[test]
    fn imbalanced_genuine_gain_selects_hybrid_without_false_abstention() {
        let prior = FrozenConstantBitPrior::fit_train(8_192, 40_960).unwrap();
        assert!(!prior.predicts_one());
        let mut gate = TrainOnlySpectralBaselineGate::new(config()).unwrap();
        for part in 0..2 {
            gate.observe_train(
                part,
                PairedTrainBaselineObservation {
                    examples: 40_960,
                    baseline_errors: 8_192,
                    hybrid_errors: 6_000,
                    w3_errors: 6_000,
                    hybrid_ns: 10_000,
                    w3_ns: 20_000,
                },
            )
            .unwrap();
        }
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::HybridW2SketchMitm);
    }
    #[test]
    fn over_budget_w3_and_missing_evidence_fail_closed() {
        let mut gate = TrainOnlySpectralBaselineGate::new(config()).unwrap();
        gate.observe_train(
            0,
            PairedTrainBaselineObservation {
                examples: 40_960,
                baseline_errors: 20_480,
                hybrid_errors: 12_000,
                w3_errors: 8_000,
                hybrid_ns: 10_000,
                w3_ns: 100_000_000_000,
            },
        )
        .unwrap();
        gate.observe_train(
            1,
            PairedTrainBaselineObservation {
                examples: 40_960,
                baseline_errors: 20_480,
                hybrid_errors: 12_000,
                w3_errors: 8_000,
                hybrid_ns: 10_000,
                w3_ns: 100_000_000_000,
            },
        )
        .unwrap();
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::HybridW2SketchMitm);
        let mut missing = TrainOnlySpectralBaselineGate::new(config()).unwrap();
        missing
            .observe_train(
                0,
                PairedTrainBaselineObservation {
                    examples: 40_960,
                    baseline_errors: 20_480,
                    hybrid_errors: 12_000,
                    w3_errors: 8_000,
                    hybrid_ns: 10_000,
                    w3_ns: 20_000,
                },
            )
            .unwrap();
        assert_eq!(missing.freeze().unwrap(), SpectralMethod::Abstain);
    }
    #[test]
    fn invalid_counts_and_overflow_cannot_mutate_existing_evidence() {
        assert!(FrozenConstantBitPrior::fit_train(10, 0).is_err());
        assert!(FrozenConstantBitPrior::fit_train(11, 10).is_err());
        let prior = FrozenConstantBitPrior::fit_train(u64::MAX, u64::MAX).unwrap();
        assert!(prior.predicts_one());
        assert_eq!(prior.count_errors(10, 30).unwrap(), 20);
        assert!(prior.count_errors(31, 30).is_err());
        let mut bad = config();
        bad.minimum_improvement_over_prior_ppm = 0;
        assert!(TrainOnlySpectralBaselineGate::new(bad).is_err());
        bad = config();
        bad.maximum_error_ppm = 1_000_001;
        assert!(TrainOnlySpectralBaselineGate::new(bad).is_err());
        let mut gate = TrainOnlySpectralBaselineGate::new(config()).unwrap();
        assert_eq!(
            gate.observe_train(
                2,
                PairedTrainBaselineObservation {
                    examples: 10,
                    baseline_errors: 0,
                    hybrid_errors: 0,
                    w3_errors: 0,
                    hybrid_ns: 10,
                    w3_ns: 10,
                }
            ),
            Err(PriorSpectralError::InvalidCounts),
        );
        assert!(gate
            .observe_train(
                0,
                PairedTrainBaselineObservation {
                    examples: 10,
                    baseline_errors: 11,
                    hybrid_errors: 10,
                    w3_errors: 10,
                    hybrid_ns: 10,
                    w3_ns: 10,
                }
            )
            .is_err());
        assert!(gate
            .observe_train(
                0,
                PairedTrainBaselineObservation {
                    examples: 10,
                    baseline_errors: 0,
                    hybrid_errors: 0,
                    w3_errors: 0,
                    hybrid_ns: 0,
                    w3_ns: 10,
                }
            )
            .is_err());
        gate.observe_train(
            0,
            PairedTrainBaselineObservation {
                examples: u64::MAX,
                baseline_errors: 0,
                hybrid_errors: 0,
                w3_errors: 0,
                hybrid_ns: u64::MAX,
                w3_ns: u64::MAX,
            },
        )
        .unwrap();
        assert_eq!(
            gate.observe_train(
                0,
                PairedTrainBaselineObservation {
                    examples: 1,
                    baseline_errors: 0,
                    hybrid_errors: 0,
                    w3_errors: 0,
                    hybrid_ns: 1,
                    w3_ns: 1,
                }
            ),
            Err(PriorSpectralError::CounterOverflow),
        );
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::Abstain);
    }
}
