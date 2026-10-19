//! F2a: frozen choice between the existing W2+Sketch/MITM and Walsh-3
//! search methods from paired, supervised TRAIN calibration ONLY.
//!
//! Does NOT implement either search engine or verify that caller-provided
//! data actually precede DEV. That provenance is the experiment harness's job.
//! All comparisons use checked integer arithmetic, with no floating thresholds.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpectralMethod {
    HybridW2SketchMitm,
    Walsh3Exhaustive,
    Abstain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpectralGateError {
    InvalidConfiguration,
    InvalidObservation,
    CounterOverflow,
    AlreadyFrozen,
}

impl std::fmt::Display for SpectralGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SpectralGateError {}

/// All bounds are explicit and chosen by the caller BEFORE evaluating DEV.
/// Error rates and the required Walsh-3 advantage are integer parts-per-million.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpectralGateConfig {
    pub minimum_examples_per_partition: u64,
    pub minimum_w3_advantage_ppm: u32,
    pub maximum_hybrid_error_ppm: u32,
    pub maximum_w3_error_ppm: u32,
    pub maximum_hybrid_ns_per_example: u64,
    pub maximum_w3_ns_per_example: u64,
}
impl SpectralGateConfig {
    pub fn validate(self) -> Result<(), SpectralGateError> {
        if self.minimum_examples_per_partition == 0
            || self.minimum_w3_advantage_ppm == 0
            || self.minimum_w3_advantage_ppm > 1_000_000
            || self.maximum_hybrid_error_ppm > 1_000_000
            || self.maximum_w3_error_ppm > 1_000_000
            || self.maximum_hybrid_ns_per_example == 0
            || self.maximum_w3_ns_per_example == 0
        {
            return Err(SpectralGateError::InvalidConfiguration);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PairedEvidence {
    examples: u64,
    hybrid_errors: u64,
    w3_errors: u64,
    hybrid_search_ns: u64,
    w3_search_ns: u64,
}
impl PairedEvidence {
    fn record(
        self,
        examples: u64,
        hybrid_errors: u64,
        w3_errors: u64,
        hybrid_search_ns: u64,
        w3_search_ns: u64,
    ) -> Result<Self, SpectralGateError> {
        if examples == 0
            || hybrid_errors > examples
            || w3_errors > examples
            || hybrid_search_ns == 0
            || w3_search_ns == 0
        {
            return Err(SpectralGateError::InvalidObservation);
        }
        Ok(Self {
            examples: self
                .examples
                .checked_add(examples)
                .ok_or(SpectralGateError::CounterOverflow)?,
            hybrid_errors: self
                .hybrid_errors
                .checked_add(hybrid_errors)
                .ok_or(SpectralGateError::CounterOverflow)?,
            w3_errors: self
                .w3_errors
                .checked_add(w3_errors)
                .ok_or(SpectralGateError::CounterOverflow)?,
            hybrid_search_ns: self
                .hybrid_search_ns
                .checked_add(hybrid_search_ns)
                .ok_or(SpectralGateError::CounterOverflow)?,
            w3_search_ns: self
                .w3_search_ns
                .checked_add(w3_search_ns)
                .ok_or(SpectralGateError::CounterOverflow)?,
        })
    }

    fn qualifies(self, max_error_ppm: u32, max_ns_per_example: u64, error: u64, ns: u64) -> bool {
        (error as u128) * 1_000_000 <= (max_error_ppm as u128) * (self.examples as u128)
            && (ns as u128) <= (max_ns_per_example as u128) * (self.examples as u128)
    }
}

#[derive(Clone, Debug)]
pub struct TrainOnlySpectralMethodGate {
    config: SpectralGateConfig,
    partitions: [PairedEvidence; 2],
    frozen_route: Option<SpectralMethod>,
}
impl TrainOnlySpectralMethodGate {
    pub fn new(config: SpectralGateConfig) -> Result<Self, SpectralGateError> {
        config.validate()?;
        Ok(Self {
            config,
            partitions: [PairedEvidence::default(); 2],
            frozen_route: None,
        })
    }

    /// Pair methods on identical strictly TRAIN-only target observations.
    /// The caller must guarantee independent TRAIN_fit and two TRAIN_cal splits.
    pub fn observe_train(
        &mut self,
        partition: usize,
        examples: u64,
        hybrid_errors: u64,
        w3_errors: u64,
        hybrid_search_ns: u64,
        w3_search_ns: u64,
    ) -> Result<(), SpectralGateError> {
        if self.frozen_route.is_some() {
            return Err(SpectralGateError::AlreadyFrozen);
        }
        if partition >= self.partitions.len() {
            return Err(SpectralGateError::InvalidObservation);
        }
        let next = self.partitions[partition].record(
            examples,
            hybrid_errors,
            w3_errors,
            hybrid_search_ns,
            w3_search_ns,
        )?;
        self.partitions[partition] = next;
        Ok(())
    }

    /// Freeze once. Prefer inexpensive hybrid on exact ties, missing evidence
    /// or a non-robust W3 advantage. A route never changes during DEV.
    pub fn freeze(&mut self) -> Result<SpectralMethod, SpectralGateError> {
        if self.frozen_route.is_some() {
            return Err(SpectralGateError::AlreadyFrozen);
        }
        let enough = self
            .partitions
            .iter()
            .all(|p| p.examples >= self.config.minimum_examples_per_partition);
        let w3_admitted = enough
            && self.partitions.iter().all(|p| {
                let favorable = p.hybrid_errors >= p.w3_errors
                    && (u128::from(p.hybrid_errors - p.w3_errors) * 1_000_000)
                        >= u128::from(self.config.minimum_w3_advantage_ppm)
                            * u128::from(p.examples);
                favorable
                    && p.qualifies(
                        self.config.maximum_w3_error_ppm,
                        self.config.maximum_w3_ns_per_example,
                        p.w3_errors,
                        p.w3_search_ns,
                    )
            });
        let hybrid_admitted = enough
            && self.partitions.iter().all(|p| {
                p.qualifies(
                    self.config.maximum_hybrid_error_ppm,
                    self.config.maximum_hybrid_ns_per_example,
                    p.hybrid_errors,
                    p.hybrid_search_ns,
                )
            });
        let decision = if w3_admitted {
            SpectralMethod::Walsh3Exhaustive
        } else if hybrid_admitted {
            SpectralMethod::HybridW2SketchMitm
        } else {
            SpectralMethod::Abstain
        };
        self.frozen_route = Some(decision);
        Ok(decision)
    }

    pub fn route(&self) -> Option<SpectralMethod> {
        self.frozen_route
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SpectralGateConfig {
        SpectralGateConfig {
            minimum_examples_per_partition: 100,
            minimum_w3_advantage_ppm: 20_000,
            maximum_hybrid_error_ppm: 500_000,
            maximum_w3_error_ppm: 500_000,
            maximum_hybrid_ns_per_example: 150,
            maximum_w3_ns_per_example: 700,
        }
    }

    #[test]
    fn w3_wins_each_calibration_split_and_stays_frozen() {
        let mut gate = TrainOnlySpectralMethodGate::new(config()).unwrap();
        assert_eq!(gate.route(), None);
        gate.observe_train(0, 200, 80, 30, 20_000, 100_000).unwrap();
        gate.observe_train(1, 200, 75, 30, 20_000, 100_000).unwrap();
        assert_eq!(gate.freeze().unwrap(), SpectralMethod::Walsh3Exhaustive);
        assert_eq!(gate.route(), Some(SpectralMethod::Walsh3Exhaustive));
        assert_eq!(gate.freeze(), Err(SpectralGateError::AlreadyFrozen));
        assert_eq!(
            gate.observe_train(0, 100, 50, 10, 10_000, 30_000),
            Err(SpectralGateError::AlreadyFrozen),
        );
    }

    #[test]
    fn ties_and_one_sided_advantage_favor_hybrid() {
        let mut g = TrainOnlySpectralMethodGate::new(config()).unwrap();
        g.observe_train(0, 200, 30, 30, 20_000, 90_000).unwrap();
        g.observe_train(1, 200, 30, 20, 20_000, 90_000).unwrap();
        assert_eq!(g.freeze().unwrap(), SpectralMethod::HybridW2SketchMitm);
        let mut h = TrainOnlySpectralMethodGate::new(config()).unwrap();
        h.observe_train(0, 200, 100, 5, 20_000, 110_000).unwrap();
        h.observe_train(1, 200, 5, 100, 20_000, 110_000).unwrap();
        assert_eq!(h.freeze().unwrap(), SpectralMethod::HybridW2SketchMitm);
    }

    #[test]
    fn over_budget_w3_falls_back_to_qualified_hybrid() {
        let mut g = TrainOnlySpectralMethodGate::new(config()).unwrap();
        for p in 0..2 {
            g.observe_train(p, 200, 80, 5, 20_000, 200_000).unwrap();
        }
        assert_eq!(g.freeze().unwrap(), SpectralMethod::HybridW2SketchMitm);
    }

    #[test]
    fn missing_evidence_or_both_unqualified_abstains() {
        let mut g = TrainOnlySpectralMethodGate::new(config()).unwrap();
        g.observe_train(0, 200, 60, 10, 20_000, 50_000).unwrap();
        assert_eq!(g.freeze().unwrap(), SpectralMethod::Abstain);
        let mut h = TrainOnlySpectralMethodGate::new(config()).unwrap();
        for p in 0..2 {
            h.observe_train(p, 200, 160, 160, 50_000, 200_000).unwrap();
        }
        assert_eq!(h.freeze().unwrap(), SpectralMethod::Abstain);
    }

    #[test]
    fn malformed_configuration_or_observations_cannot_enter() {
        let mut bad = config();
        bad.minimum_w3_advantage_ppm = 0;
        assert_eq!(
            TrainOnlySpectralMethodGate::new(bad).err(),
            Some(SpectralGateError::InvalidConfiguration)
        );
        bad = config();
        bad.maximum_hybrid_ns_per_example = 0;
        assert!(TrainOnlySpectralMethodGate::new(bad).is_err());
        let mut g = TrainOnlySpectralMethodGate::new(config()).unwrap();
        assert_eq!(
            g.observe_train(2, 10, 2, 1, 10, 10),
            Err(SpectralGateError::InvalidObservation)
        );
        assert!(g.observe_train(0, 0, 0, 0, 10, 10).is_err());
        assert!(g.observe_train(0, 10, 11, 1, 10, 10).is_err());
        assert!(g.observe_train(0, 10, 1, 1, 0, 10).is_err());
        assert_eq!(g.freeze().unwrap(), SpectralMethod::Abstain);
    }

    #[test]
    fn u64_counter_overflow_rejects_without_partial_mutation() {
        let mut cfg = config();
        cfg.minimum_examples_per_partition = u64::MAX;
        cfg.maximum_hybrid_ns_per_example = u64::MAX;
        cfg.maximum_w3_ns_per_example = u64::MAX;
        let mut g = TrainOnlySpectralMethodGate::new(cfg).unwrap();
        g.observe_train(0, u64::MAX, 0, 0, u64::MAX, u64::MAX)
            .unwrap();
        assert_eq!(
            g.observe_train(0, 1, 1, 0, 1, 1),
            Err(SpectralGateError::CounterOverflow)
        );
        assert_eq!(g.freeze().unwrap(), SpectralMethod::Abstain);
    }
}
