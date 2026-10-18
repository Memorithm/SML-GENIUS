//! Cost-aware TRAIN-only admission of an optional recurrent inference path.
//! Routing is frozen at a *document boundary* and cannot switch recurrent
//! state in mid-document. Does not instantiate, skip, or train a model.
//!
//! Two cost currencies exist. `observe_train_batch` takes measured wall-clock
//! nanoseconds: the route then depends on the host, its load and timer noise,
//! so two machines can freeze different routes from identical data.
//! `observe_train_batch_costed` takes an integer *logical cost* per position
//! (for example logical payload bytes touched, see
//! `RtlrBooleanHybrid::logical_payload_bytes`); the route is then a pure
//! function of the supplied evidence and reproducible everywhere. A gate must
//! use one currency only: mixing them is rejected.
//!
//! The losses passed in must be *out-of-fold*: measured on supervised TRAIN
//! positions that no candidate was fitted on (the V6 bench uses documents
//! disjoint from the fit documents). The gate cannot verify this; feeding
//! in-sample losses favours the richer recurrent model, the opposite of the
//! conservative intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InferenceRoute {
    BooleanOnly,
    RecurrentHybrid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CostCurrency {
    MeasuredNanoseconds,
    LogicalUnits,
}

#[derive(Clone, Copy, Debug, Default)]
struct TrainingEvidence {
    count: u64,
    boolean_bpc_sum: f64,
    recurrent_bpc_sum: f64,
    boolean_ns_sum: f64,
    recurrent_ns_sum: f64,
}

/// Two TRAIN partitions must *each* justify recurrence after paying its
/// measured incremental compute cost. No DEV/TEST labels can change route.
#[derive(Clone, Debug)]
pub struct TrainOnlyCostGate {
    partitions: [TrainingEvidence; 2],
    minimum_samples: u64,
    minimum_gain_bpc: f64,
    cpu_penalty_bpc_per_ns: f64,
    currency: Option<CostCurrency>,
    frozen: Option<InferenceRoute>,
}

impl TrainOnlyCostGate {
    pub fn new(
        minimum_samples_per_partition: u64,
        minimum_gain_bpc: f64,
        cpu_penalty_bpc_per_ns: f64,
    ) -> Result<Self, &'static str> {
        if minimum_samples_per_partition < 32
            || !minimum_gain_bpc.is_finite()
            || minimum_gain_bpc < 0.0
            || !cpu_penalty_bpc_per_ns.is_finite()
            || cpu_penalty_bpc_per_ns < 0.0
        {
            return Err("invalid RTLR-V6 cost gate policy");
        }
        Ok(Self {
            partitions: [TrainingEvidence::default(); 2],
            minimum_samples: minimum_samples_per_partition,
            minimum_gain_bpc,
            cpu_penalty_bpc_per_ns,
            currency: None,
            frozen: None,
        })
    }

    /// The route is a pure function of the observed evidence only when the
    /// gate was fed integer logical costs.
    #[must_use]
    pub fn is_deterministic(&self) -> bool {
        self.currency == Some(CostCurrency::LogicalUnits)
    }

    /// Like [`Self::observe_train_batch`], with exact integer logical cost
    /// units per position instead of measured nanoseconds. The policy's
    /// `cpu_penalty_bpc_per_ns` is then interpreted per logical unit.
    pub fn observe_train_batch_costed(
        &mut self,
        partition: usize,
        positions: u64,
        boolean_mean_bpc: f64,
        recurrent_mean_bpc: f64,
        boolean_cost_units: u64,
        recurrent_cost_units: u64,
    ) -> Result<(), &'static str> {
        self.observe(
            CostCurrency::LogicalUnits,
            partition,
            positions,
            boolean_mean_bpc,
            recurrent_mean_bpc,
            boolean_cost_units as f64,
            recurrent_cost_units as f64,
        )
    }

    /// Caller must supply *paired* TRAIN-only losses/timings from exactly
    /// the same labeled positions. Partition IDs refer to disjoint TRAIN
    /// windows; partition membership is not inferred from DEV statistics.
    pub fn observe_train_batch(
        &mut self,
        partition: usize,
        positions: u64,
        boolean_mean_bpc: f64,
        recurrent_mean_bpc: f64,
        boolean_ns_per_position: f64,
        recurrent_ns_per_position: f64,
    ) -> Result<(), &'static str> {
        self.observe(
            CostCurrency::MeasuredNanoseconds,
            partition,
            positions,
            boolean_mean_bpc,
            recurrent_mean_bpc,
            boolean_ns_per_position,
            recurrent_ns_per_position,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn observe(
        &mut self,
        currency: CostCurrency,
        partition: usize,
        positions: u64,
        boolean_mean_bpc: f64,
        recurrent_mean_bpc: f64,
        boolean_ns_per_position: f64,
        recurrent_ns_per_position: f64,
    ) -> Result<(), &'static str> {
        if self.currency.is_some_and(|used| used != currency) {
            return Err("cost gate mixes measured nanoseconds with logical cost units");
        }
        if self.frozen.is_some()
            || partition > 1
            || positions == 0
            || [
                boolean_mean_bpc,
                recurrent_mean_bpc,
                boolean_ns_per_position,
                recurrent_ns_per_position,
            ]
            .iter()
            .any(|x| !x.is_finite() || *x < 0.0)
        {
            return Err("invalid or frozen TRAIN-only evidence");
        }
        let n = positions as f64;
        let previous = self.partitions[partition];
        let next = TrainingEvidence {
            count: previous
                .count
                .checked_add(positions)
                .ok_or("cost gate sample count overflow")?,
            boolean_bpc_sum: previous.boolean_bpc_sum + n * boolean_mean_bpc,
            recurrent_bpc_sum: previous.recurrent_bpc_sum + n * recurrent_mean_bpc,
            boolean_ns_sum: previous.boolean_ns_sum + n * boolean_ns_per_position,
            recurrent_ns_sum: previous.recurrent_ns_sum + n * recurrent_ns_per_position,
        };
        if [
            next.boolean_bpc_sum,
            next.recurrent_bpc_sum,
            next.boolean_ns_sum,
            next.recurrent_ns_sum,
        ]
        .iter()
        .any(|x| !x.is_finite())
        {
            return Err("cost gate evidence overflow");
        }
        self.partitions[partition] = next;
        self.currency = Some(currency);
        Ok(())
    }

    /// A candidate without enough TRAIN-only validation defaults to the
    /// simpler Boolean route; recurrence must earn admission twice.
    pub fn freeze_for_next_document(&mut self) -> Result<InferenceRoute, &'static str> {
        if self.frozen.is_some() {
            return Err("RTLR-V6 route already frozen");
        }
        let admitted = self.partitions.iter().all(|p| {
            if p.count < self.minimum_samples {
                return false;
            }
            let n = p.count as f64;
            let gain_bpc = (p.boolean_bpc_sum - p.recurrent_bpc_sum) / n;
            let extra_ns = ((p.recurrent_ns_sum - p.boolean_ns_sum) / n).max(0.0);
            gain_bpc > self.minimum_gain_bpc + self.cpu_penalty_bpc_per_ns * extra_ns
        });
        let route = if admitted {
            InferenceRoute::RecurrentHybrid
        } else {
            InferenceRoute::BooleanOnly
        };
        self.frozen = Some(route);
        Ok(route)
    }

    /// Read-only document route. A caller may not change paths mid-stream:
    /// a new document must reset its volatile recurrent/Boolean state.
    #[must_use]
    pub const fn route(&self) -> Option<InferenceRoute> {
        self.frozen
    }
}

#[cfg(test)]
mod tests {
    use super::{InferenceRoute, TrainOnlyCostGate};

    #[test]
    fn equal_accuracy_prefers_low_cost_boolean() {
        let mut gate = TrainOnlyCostGate::new(1024, 0.001, 0.0001).unwrap();
        for partition in 0..2 {
            gate.observe_train_batch(partition, 4096, 0.00144, 0.00144, 2.0, 12.0)
                .unwrap();
        }
        assert_eq!(gate.route(), None);
        assert_eq!(
            gate.freeze_for_next_document().unwrap(),
            InferenceRoute::BooleanOnly
        );
        assert_eq!(gate.route(), Some(InferenceRoute::BooleanOnly));
        assert!(gate
            .observe_train_batch(0, 1024, 1.0, 0.0, 2.0, 3.0)
            .is_err());
        assert!(gate.freeze_for_next_document().is_err());
    }

    #[test]
    fn recurrent_path_requires_benefit_on_both_train_partitions() {
        let mut gate = TrainOnlyCostGate::new(1024, 0.002, 0.0001).unwrap();
        gate.observe_train_batch(0, 2048, 0.9, 0.7, 10.0, 100.0)
            .unwrap();
        gate.observe_train_batch(1, 2048, 0.8, 0.6, 10.0, 100.0)
            .unwrap();
        assert_eq!(
            gate.freeze_for_next_document().unwrap(),
            InferenceRoute::RecurrentHybrid
        );
    }

    #[test]
    fn lack_of_evidence_or_partition_failure_is_fail_closed() {
        let mut insufficient = TrainOnlyCostGate::new(1024, 0.001, 0.0).unwrap();
        insufficient
            .observe_train_batch(0, 2048, 0.9, 0.1, 10.0, 100.0)
            .unwrap();
        assert_eq!(
            insufficient.freeze_for_next_document().unwrap(),
            InferenceRoute::BooleanOnly
        );
        let mut one_bad = TrainOnlyCostGate::new(1024, 0.001, 0.0).unwrap();
        one_bad
            .observe_train_batch(0, 2048, 0.9, 0.1, 10.0, 100.0)
            .unwrap();
        one_bad
            .observe_train_batch(1, 2048, 0.8, 0.9, 10.0, 100.0)
            .unwrap();
        assert_eq!(
            one_bad.freeze_for_next_document().unwrap(),
            InferenceRoute::BooleanOnly
        );
    }

    #[test]
    fn rejects_invalid_values_and_overflows() {
        assert!(TrainOnlyCostGate::new(31, 0.0, 0.0).is_err());
        assert!(TrainOnlyCostGate::new(1024, f64::NAN, 0.0).is_err());
        assert!(TrainOnlyCostGate::new(1024, 0.0, -1.0).is_err());
        let mut gate = TrainOnlyCostGate::new(1024, 0.0, 0.0).unwrap();
        assert!(gate
            .observe_train_batch(2, 128, 0.2, 0.1, 2.0, 3.0)
            .is_err());
        assert!(gate
            .observe_train_batch(0, 128, f64::INFINITY, 0.1, 2.0, 3.0)
            .is_err());
        assert!(gate.observe_train_batch(0, 0, 0.2, 0.1, 2.0, 3.0).is_err());
        assert!(gate
            .observe_train_batch(0, u64::MAX, 0.2, 0.1, 2.0, 3.0)
            .is_ok());
        assert!(gate.observe_train_batch(0, 1, 0.2, 0.1, 2.0, 3.0).is_err());
    }

    #[test]
    fn measured_nanoseconds_can_flip_the_route_but_logical_costs_cannot() {
        // Same losses on every host; only the timer noise differs.
        let route_with_ns = |recurrent_ns: f64| {
            let mut gate = TrainOnlyCostGate::new(1024, 0.001, 0.0001).unwrap();
            for partition in 0..2 {
                gate.observe_train_batch(partition, 4096, 0.9, 0.7, 10.0, recurrent_ns)
                    .unwrap();
            }
            assert!(!gate.is_deterministic());
            gate.freeze_for_next_document().unwrap()
        };
        assert_eq!(route_with_ns(100.0), InferenceRoute::RecurrentHybrid);
        assert_eq!(route_with_ns(5000.0), InferenceRoute::BooleanOnly);

        let route_with_units = || {
            let mut gate = TrainOnlyCostGate::new(1024, 0.001, 0.0001).unwrap();
            for partition in 0..2 {
                gate.observe_train_batch_costed(partition, 4096, 0.9, 0.7, 16, 1_100)
                    .unwrap();
            }
            assert!(gate.is_deterministic());
            gate.freeze_for_next_document().unwrap()
        };
        // gain 0.2 > 0.001 + 0.0001 * (1100 - 16) = 0.1094: admitted, every time.
        assert_eq!(route_with_units(), InferenceRoute::RecurrentHybrid);
        assert_eq!(route_with_units(), route_with_units());
    }

    #[test]
    fn logical_cost_threshold_is_exact_and_ties_fall_back_to_boolean() {
        let route = |recurrent_units: u64| {
            let mut gate = TrainOnlyCostGate::new(1024, 0.0, 0.01).unwrap();
            for partition in 0..2 {
                gate.observe_train_batch_costed(partition, 2048, 1.0, 0.5, 100, recurrent_units)
                    .unwrap();
            }
            gate.freeze_for_next_document().unwrap()
        };
        // Gain 0.5 against 0.01 per extra unit over a Boolean cost of 100:
        // 49 extra units cost 0.49 (admitted), 50 extra units tie at 0.5.
        assert_eq!(route(149), InferenceRoute::RecurrentHybrid);
        assert_eq!(route(150), InferenceRoute::BooleanOnly);
        assert_eq!(route(250), InferenceRoute::BooleanOnly);
        // A cheaper recurrent path earns no bonus: negative extra cost is 0.
        assert_eq!(route(10), InferenceRoute::RecurrentHybrid);
    }

    #[test]
    fn the_two_cost_currencies_cannot_be_mixed_in_one_gate() {
        let mut gate = TrainOnlyCostGate::new(1024, 0.0, 0.0).unwrap();
        gate.observe_train_batch(0, 128, 0.5, 0.4, 1.0, 2.0)
            .unwrap();
        assert!(gate
            .observe_train_batch_costed(1, 128, 0.5, 0.4, 1, 2)
            .is_err());
        let mut gate = TrainOnlyCostGate::new(1024, 0.0, 0.0).unwrap();
        gate.observe_train_batch_costed(0, 128, 0.5, 0.4, 1, 2)
            .unwrap();
        assert!(gate
            .observe_train_batch(1, 128, 0.5, 0.4, 1.0, 2.0)
            .is_err());
        // A rejected observation leaves the currency untouched and usable.
        assert!(gate
            .observe_train_batch_costed(2, 128, 0.5, 0.4, 1, 2)
            .is_err());
        assert!(gate
            .observe_train_batch_costed(1, 128, 0.5, 0.4, 1, 2)
            .is_ok());
    }

    #[test]
    fn costs_come_from_real_model_payloads() {
        use crate::{
            rtlr_boolean_hybrid::RtlrBooleanHybrid,
            rtlr_elastic::{RtlrElastic, RtlrElasticConfig, RtlrV2Policy},
        };
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("sml-cost-gate-{}-{nonce}", std::process::id()));
        let hybrid = RtlrBooleanHybrid::new(
            RtlrElastic::new_v3(
                RtlrElasticConfig {
                    max_active: 8,
                    learning_rate: 0.005,
                    grow_above_bpc: 4.1,
                    shrink_below_bpc: 3.8,
                    patience: 2,
                    warmup_documents: 2,
                    seed: 11,
                },
                RtlrV2Policy {
                    ewma_alpha: 0.25,
                    plateau_tolerance_bpc: 0.05,
                    marginal_cost_bpc_per_cell: 0.001,
                },
                &dir,
            )
            .unwrap(),
            1024,
            vec![1, 16],
            0.25,
        )
        .unwrap();
        let boolean_units = hybrid.boolean_bytes() as u64;
        let recurrent_units = hybrid.logical_payload_bytes() as u64;
        assert_eq!(boolean_units, 128);
        assert!(recurrent_units > boolean_units);
        let mut gate = TrainOnlyCostGate::new(1024, 0.001, 1.0e-4).unwrap();
        for partition in 0..2 {
            // Equal quality: the extra payload alone must keep it Boolean-only.
            gate.observe_train_batch_costed(
                partition,
                2048,
                0.3,
                0.3,
                boolean_units,
                recurrent_units,
            )
            .unwrap();
        }
        assert_eq!(
            gate.freeze_for_next_document().unwrap(),
            InferenceRoute::BooleanOnly
        );
        drop(hybrid);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
