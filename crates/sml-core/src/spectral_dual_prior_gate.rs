//! F2b-4c-2: TRAIN-only spectral qualification against *both* a constant
//! label prior and a causal first-order Markov label prior.
//!
//! The caller must freeze priors from TRAIN_fit, score the *same* suffix of
//! disjoint TRAIN_cal documents for every arm, and freeze before generating DEV.
//! Unlike an autonomous LM, the teacher-forced Markov oracle reads t-1 truth.
//! Counts alone do not prove split provenance, independence, or target pairing.
use crate::spectral_baseline_gate::{BaselineSpectralGateConfig, PriorSpectralError};
use crate::spectral_method_gate::SpectralMethod;

/// All errors count the exact same paired scored target bits. When W3 search
/// was skipped (e.g. n508), BOTH of its fields must be absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PairedTrainDualPriorObservation {
    pub examples: u64,
    pub constant_errors: u64,
    pub markov_errors: u64,
    pub hybrid_errors: u64,
    pub w3_errors: Option<u64>,
    pub hybrid_ns: u64,
    pub w3_ns: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Partition {
    seen: bool,
    examples: u64,
    constant_errors: u64,
    markov_errors: u64,
    hybrid_errors: u64,
    w3_errors: u64,
    hybrid_ns: u64,
    w3_ns: u64,
    w3_available: bool,
}

impl Partition {
    fn with_observation(
        self,
        item: PairedTrainDualPriorObservation,
    ) -> Result<Self, PriorSpectralError> {
        if item.examples == 0
            || item.constant_errors > item.examples
            || item.markov_errors > item.examples
            || item.hybrid_errors > item.examples
            || item.hybrid_ns == 0
            || item.w3_errors.is_some() != item.w3_ns.is_some()
            || item.w3_errors.is_some_and(|e| e > item.examples)
            || item.w3_ns.is_some_and(|ns| ns == 0)
            || (self.seen && self.w3_available != item.w3_errors.is_some())
        {
            return Err(PriorSpectralError::InvalidCounts);
        }
        let add = |old: u64, incoming: u64| {
            old.checked_add(incoming)
                .ok_or(PriorSpectralError::CounterOverflow)
        };
        Ok(Self {
            seen: true,
            examples: add(self.examples, item.examples)?,
            constant_errors: add(self.constant_errors, item.constant_errors)?,
            markov_errors: add(self.markov_errors, item.markov_errors)?,
            hybrid_errors: add(self.hybrid_errors, item.hybrid_errors)?,
            w3_errors: add(self.w3_errors, item.w3_errors.unwrap_or(0))?,
            hybrid_ns: add(self.hybrid_ns, item.hybrid_ns)?,
            w3_ns: add(self.w3_ns, item.w3_ns.unwrap_or(0))?,
            w3_available: item.w3_errors.is_some(),
        })
    }

    fn qualifies(
        self,
        errors: u64,
        nanoseconds: u64,
        maximum_ns_per_example: u64,
        config: BaselineSpectralGateConfig,
    ) -> bool {
        let total = u128::from(self.examples);
        let improvement = u128::from(config.minimum_improvement_over_prior_ppm) * total;
        let beat = |prior_errors: u64| {
            prior_errors >= errors && u128::from(prior_errors - errors) * 1_000_000 >= improvement
        };
        beat(self.constant_errors)
            && beat(self.markov_errors)
            && u128::from(errors) * 1_000_000 <= u128::from(config.maximum_error_ppm) * total
            && u128::from(nanoseconds) <= u128::from(maximum_ns_per_example) * total
    }

    fn w3_beats_hybrid(self, margin_ppm: u32) -> bool {
        self.hybrid_errors >= self.w3_errors
            && u128::from(self.hybrid_errors - self.w3_errors) * 1_000_000
                >= u128::from(margin_ppm) * u128::from(self.examples)
    }
}

/// A new F2b-4c gate; historical F2b-4a routes and TSV schemas are untouched.
#[derive(Clone, Debug)]
pub struct TrainOnlySpectralDualPriorGate {
    config: BaselineSpectralGateConfig,
    partitions: [Partition; 2],
    frozen_route: Option<SpectralMethod>,
}

impl TrainOnlySpectralDualPriorGate {
    pub fn new(config: BaselineSpectralGateConfig) -> Result<Self, PriorSpectralError> {
        config.validate()?;
        Ok(Self {
            config,
            partitions: [Partition::default(); 2],
            frozen_route: None,
        })
    }

    /// Only paired TRAIN_cal evidence; no DEV labels or model re-fitting.
    /// Invalid observations cannot partially mutate accumulated counts.
    pub fn observe_train(
        &mut self,
        partition: usize,
        item: PairedTrainDualPriorObservation,
    ) -> Result<(), PriorSpectralError> {
        if self.frozen_route.is_some() {
            return Err(PriorSpectralError::AlreadyFrozen);
        }
        let old = *self
            .partitions
            .get(partition)
            .ok_or(PriorSpectralError::InvalidCounts)?;
        let next = old.with_observation(item)?;
        self.partitions[partition] = next;
        Ok(())
    }

    /// Reject mismatched arm availability rather than implicitly inventing
    /// a W3 score. An insufficient partition freezes a safe abstention.
    pub fn freeze(&mut self) -> Result<SpectralMethod, PriorSpectralError> {
        if self.frozen_route.is_some() {
            return Err(PriorSpectralError::AlreadyFrozen);
        }
        let [one, two] = self.partitions;
        if one.seen && two.seen && one.w3_available != two.w3_available {
            return Err(PriorSpectralError::InvalidCounts);
        }
        let enough = self
            .partitions
            .iter()
            .all(|p| p.seen && p.examples >= self.config.minimum_examples_per_partition);
        let hybrid_eligible = enough
            && self.partitions.iter().all(|p| {
                p.qualifies(
                    p.hybrid_errors,
                    p.hybrid_ns,
                    self.config.maximum_hybrid_ns_per_example,
                    self.config,
                )
            });
        let w3_eligible = enough
            && self.partitions.iter().all(|p| {
                p.w3_available
                    && p.qualifies(
                        p.w3_errors,
                        p.w3_ns,
                        self.config.maximum_w3_ns_per_example,
                        self.config,
                    )
                    && p.w3_beats_hybrid(self.config.minimum_w3_advantage_ppm)
            });
        let result = if w3_eligible {
            SpectralMethod::Walsh3Exhaustive
        } else if hybrid_eligible {
            SpectralMethod::HybridW2SketchMitm
        } else {
            SpectralMethod::Abstain
        };
        self.frozen_route = Some(result);
        Ok(result)
    }

    #[must_use]
    pub fn route(&self) -> Option<SpectralMethod> {
        self.frozen_route
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

    fn sample(
        constant: u64,
        markov: u64,
        hybrid: u64,
        w3: Option<u64>,
    ) -> PairedTrainDualPriorObservation {
        PairedTrainDualPriorObservation {
            examples: 40_960,
            constant_errors: constant,
            markov_errors: markov,
            hybrid_errors: hybrid,
            w3_errors: w3,
            hybrid_ns: 10_000,
            w3_ns: w3.map(|_| 20_000),
        }
    }

    fn paired(
        first: PairedTrainDualPriorObservation,
        second: PairedTrainDualPriorObservation,
    ) -> TrainOnlySpectralDualPriorGate {
        let mut gate = TrainOnlySpectralDualPriorGate::new(config()).unwrap();
        gate.observe_train(0, first).unwrap();
        gate.observe_train(1, second).unwrap();
        gate
    }

    #[test]
    fn markov_null_that_beats_constant_but_not_markov_must_abstain() {
        let mut gate = paired(
            sample(20_480, 4_096, 4_096, Some(3_800)),
            sample(20_480, 4_096, 4_096, Some(3_800)),
        );
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::Abstain);
    }

    #[test]
    fn predictive_noisy_xor_can_select_w3_over_both_frozen_priors() {
        let mut gate = paired(
            sample(20_480, 20_400, 19_800, Some(16_384)),
            sample(20_480, 20_400, 19_800, Some(16_384)),
        );
        assert_eq!(gate.route(), None);
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::Walsh3Exhaustive);
        assert_eq!(gate.route(), Some(SpectralMethod::Walsh3Exhaustive));
        assert_eq!(gate.freeze(), Err(PriorSpectralError::AlreadyFrozen));
        assert_eq!(
            gate.observe_train(0, sample(20_480, 20_400, 19_800, Some(16_384))),
            Err(PriorSpectralError::AlreadyFrozen)
        );
    }

    #[test]
    fn absent_w3_can_still_select_valid_hybrid() {
        let mut gate = paired(
            sample(8_192, 8_500, 7_000, None),
            sample(8_192, 8_500, 7_000, None),
        );
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::HybridW2SketchMitm);
    }

    #[test]
    fn markov_advantage_is_required_in_each_calibration_partition() {
        let mut gate = paired(
            sample(20_480, 6_000, 5_600, None),
            sample(20_480, 6_000, 5_000, None),
        );
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::Abstain);
    }

    #[test]
    fn exact_integer_margin_is_inclusive_and_below_margin_fails() {
        let mut pass = paired(
            sample(10_000, 9_000, 8_590, None),
            sample(10_000, 9_000, 8_590, None),
        );
        assert_eq!(pass.freeze().unwrap(), SpectralMethod::HybridW2SketchMitm);
        let mut fail = paired(
            sample(10_000, 9_000, 8_591, None),
            sample(10_000, 9_000, 8_591, None),
        );
        assert_eq!(fail.freeze().unwrap(), SpectralMethod::Abstain);
    }

    #[test]
    fn expensive_w3_falls_back_to_hybrid() {
        let mut large = sample(20_480, 20_480, 17_000, Some(14_000));
        large.w3_ns = Some(50_000_000_000);
        let mut gate = paired(large, large);
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::HybridW2SketchMitm);
    }

    #[test]
    fn missing_and_mismatched_availability_cannot_promote() {
        let mut empty = TrainOnlySpectralDualPriorGate::new(config()).unwrap();
        empty
            .observe_train(0, sample(20_480, 20_480, 10_000, Some(8_000)))
            .unwrap();
        assert_eq!(empty.freeze().unwrap(), SpectralMethod::Abstain);
        let mut mismatch = paired(
            sample(20_480, 20_480, 10_000, Some(8_000)),
            sample(20_480, 20_480, 10_000, None),
        );
        assert_eq!(mismatch.freeze(), Err(PriorSpectralError::InvalidCounts));
        assert_eq!(mismatch.route(), None);
    }

    #[test]
    fn invalid_counts_and_overflow_leave_partition_unchanged() {
        let mut gate = TrainOnlySpectralDualPriorGate::new(config()).unwrap();
        let valid = sample(20_480, 20_480, 10_000, Some(8_000));
        assert_eq!(
            gate.observe_train(2, valid),
            Err(PriorSpectralError::InvalidCounts)
        );
        let mut invalid = valid;
        invalid.markov_errors = 40_961;
        assert_eq!(
            gate.observe_train(0, invalid),
            Err(PriorSpectralError::InvalidCounts)
        );
        invalid = valid;
        invalid.w3_ns = None;
        assert_eq!(
            gate.observe_train(0, invalid),
            Err(PriorSpectralError::InvalidCounts)
        );
        gate.observe_train(0, valid).unwrap();
        invalid = valid;
        invalid.w3_errors = None;
        invalid.w3_ns = None;
        assert_eq!(
            gate.observe_train(0, invalid),
            Err(PriorSpectralError::InvalidCounts)
        );
        gate.observe_train(1, valid).unwrap();
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::Walsh3Exhaustive);

        let mut overflow = TrainOnlySpectralDualPriorGate::new(config()).unwrap();
        overflow
            .observe_train(
                0,
                PairedTrainDualPriorObservation {
                    examples: u64::MAX,
                    constant_errors: u64::MAX,
                    markov_errors: u64::MAX,
                    hybrid_errors: 0,
                    w3_errors: None,
                    hybrid_ns: u64::MAX,
                    w3_ns: None,
                },
            )
            .unwrap();
        assert_eq!(
            overflow.observe_train(0, sample(1, 1, 1, None)),
            Err(PriorSpectralError::CounterOverflow)
        );
        assert_eq!(overflow.freeze().unwrap(), SpectralMethod::Abstain);
    }

    #[test]
    fn invalid_configuration_and_zero_cpu_are_rejected() {
        let mut bad = config();
        bad.minimum_improvement_over_prior_ppm = 0;
        assert!(TrainOnlySpectralDualPriorGate::new(bad).is_err());
        let mut gate = TrainOnlySpectralDualPriorGate::new(config()).unwrap();
        let mut sample = sample(20_480, 20_480, 10_000, Some(8_000));
        sample.hybrid_ns = 0;
        assert_eq!(
            gate.observe_train(0, sample),
            Err(PriorSpectralError::InvalidCounts)
        );
        sample.hybrid_ns = 10_000;
        sample.w3_ns = Some(0);
        assert_eq!(
            gate.observe_train(0, sample),
            Err(PriorSpectralError::InvalidCounts)
        );
        assert_eq!(gate.route(), None);
    }
}
