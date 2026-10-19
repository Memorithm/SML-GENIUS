//! Causal model-selection policy for the next-byte MSB tree.
//!
//! The policy is fitted using two disjoint, *supervised TRAIN-only*
//! calibration partitions. On DEV it is a frozen choice. Both f32 master
//! and *real* Q4 deployment losses must improve independently.
//! It never reads data or models itself: callers must preserve disjointness.
use crate::byte_factorization::ByteFactorization;

#[derive(Debug, Clone, Copy)]
struct Evidence {
    positions: u64,
    independent_f32_total_bits: f64,
    msb_f32_total_bits: f64,
    independent_q4_total_bits: f64,
    msb_q4_total_bits: f64,
}
impl Default for Evidence {
    fn default() -> Self {
        Self {
            positions: 0,
            independent_f32_total_bits: 0.0,
            msb_f32_total_bits: 0.0,
            independent_q4_total_bits: 0.0,
            msb_q4_total_bits: 0.0,
        }
    }
}

/// A separate evidence partition must approve the tree in both precisions.
/// No selection from DEV results, and no mid-document route changes.
#[derive(Debug, Clone)]
pub struct TrainOnlyByteFactorizationGate {
    partitions: [Evidence; 2],
    minimum_samples_per_partition: u64,
    minimum_gain_bits_per_byte: f64,
    frozen: Option<ByteFactorization>,
}
impl TrainOnlyByteFactorizationGate {
    pub fn new(
        minimum_samples_per_partition: u64,
        minimum_gain_bits_per_byte: f64,
    ) -> Result<Self, &'static str> {
        if minimum_samples_per_partition == 0
            || !minimum_gain_bits_per_byte.is_finite()
            || minimum_gain_bits_per_byte < 0.0
        {
            return Err("invalid TRAIN-only byte factorization gate policy");
        }
        Ok(Self {
            partitions: [Evidence::default(); 2],
            minimum_samples_per_partition,
            minimum_gain_bits_per_byte,
            frozen: None,
        })
    }
    /// Add paired aggregate loss on identical supervised TRAIN byte targets,
    /// in *bits*, not bits/byte. Callers validate source identity and phase.
    pub fn observe_train(
        &mut self,
        partition: usize,
        positions: u64,
        independent_f32_total_bits: f64,
        msb_f32_total_bits: f64,
        independent_q4_total_bits: f64,
        msb_q4_total_bits: f64,
    ) -> Result<(), &'static str> {
        let values = [
            independent_f32_total_bits,
            msb_f32_total_bits,
            independent_q4_total_bits,
            msb_q4_total_bits,
        ];
        if self.frozen.is_some()
            || partition >= 2
            || positions == 0
            || values.iter().any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err("invalid or frozen supervised TRAIN evidence");
        }
        let prev = self.partitions[partition];
        let next = Evidence {
            positions: prev.positions.checked_add(positions).ok_or("TRAIN sample overflow")?,
            independent_f32_total_bits: prev.independent_f32_total_bits + values[0],
            msb_f32_total_bits: prev.msb_f32_total_bits + values[1],
            independent_q4_total_bits: prev.independent_q4_total_bits + values[2],
            msb_q4_total_bits: prev.msb_q4_total_bits + values[3],
        };
        if [
            next.independent_f32_total_bits,
            next.msb_f32_total_bits,
            next.independent_q4_total_bits,
            next.msb_q4_total_bits,
        ]
        .iter()
        .any(|v| !v.is_finite())
        {
            return Err("TRAIN loss sum overflow");
        }
        self.partitions[partition] = next;
        Ok(())
    }
    pub fn freeze(&mut self) -> Result<ByteFactorization, &'static str> {
        if self.frozen.is_some() {
            return Err("byte factorization route already frozen");
        }
        let better_on_both = self.partitions.iter().all(|p| {
            if p.positions < self.minimum_samples_per_partition {
                return false;
            }
            let threshold = self.minimum_gain_bits_per_byte * p.positions as f64;
            p.independent_f32_total_bits - p.msb_f32_total_bits > threshold
                && p.independent_q4_total_bits - p.msb_q4_total_bits > threshold
        });
        let selected = if better_on_both {
            ByteFactorization::MsbTree
        } else {
            ByteFactorization::IndependentBits
        };
        self.frozen = Some(selected);
        Ok(selected)
    }
    #[must_use]
    pub fn route(&self) -> Option<ByteFactorization> {
        self.frozen
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strong_msb_f32_q4_gain_in_both_partitions_admits() {
        let mut g = TrainOnlyByteFactorizationGate::new(128, 0.05).unwrap();
        for p in 0..2 {
            g.observe_train(p, 256, 6.2 * 256.0, 5.0 * 256.0, 6.3 * 256.0, 5.2 * 256.0)
                .unwrap();
        }
        assert_eq!(g.freeze().unwrap(), ByteFactorization::MsbTree);
        assert_eq!(g.route(), Some(ByteFactorization::MsbTree));
        assert!(g.freeze().is_err());
        assert!(g.observe_train(0, 128, 1.0, 1.0, 1.0, 1.0).is_err());
    }
    #[test]
    fn negative_iid_or_one_q4_regression_defaults_independent() {
        let mut g = TrainOnlyByteFactorizationGate::new(128, 0.01).unwrap();
        g.observe_train(0, 256, 8.2 * 256.0, 8.3 * 256.0, 8.2 * 256.0, 8.4 * 256.0)
            .unwrap();
        g.observe_train(1, 256, 6.0 * 256.0, 5.0 * 256.0, 6.0 * 256.0, 5.0 * 256.0)
            .unwrap();
        assert_eq!(g.freeze().unwrap(), ByteFactorization::IndependentBits);
        let mut q4 = TrainOnlyByteFactorizationGate::new(128, 0.01).unwrap();
        for p in 0..2 {
            q4.observe_train(p, 256, 6.0 * 256.0, 5.0 * 256.0, 6.0 * 256.0, 6.2 * 256.0)
                .unwrap();
        }
        assert_eq!(q4.freeze().unwrap(), ByteFactorization::IndependentBits);
    }
    #[test]
    fn missing_partition_margin_and_invalid_inputs_are_fail_closed() {
        let mut g = TrainOnlyByteFactorizationGate::new(128, 0.1).unwrap();
        assert!(g.observe_train(2, 128, 100.0, 50.0, 100.0, 50.0).is_err());
        assert!(g.observe_train(0, 128, 100.0, f64::NAN, 100.0, 50.0).is_err());
        assert!(g.observe_train(0, 0, 100.0, 50.0, 100.0, 50.0).is_err());
        g.observe_train(0, 128, 100.0, 50.0, 100.0, 50.0).unwrap();
        assert_eq!(g.freeze().unwrap(), ByteFactorization::IndependentBits);
        let mut equal = TrainOnlyByteFactorizationGate::new(128, 0.1).unwrap();
        for p in 0..2 {
            equal.observe_train(p, 128, 100.0, 90.0, 100.0, 90.0).unwrap();
        }
        assert_eq!(equal.freeze().unwrap(), ByteFactorization::IndependentBits);
        assert!(TrainOnlyByteFactorizationGate::new(0, 0.1).is_err());
        assert!(TrainOnlyByteFactorizationGate::new(128, f64::INFINITY).is_err());
    }
    #[test]
    fn overflow_rejects_without_poisoning_counter() {
        let mut g = TrainOnlyByteFactorizationGate::new(128, 0.1).unwrap();
        g.observe_train(0, u64::MAX, 1.0, 1.0, 1.0, 1.0).unwrap();
        assert!(g.observe_train(0, 1, 1.0, 1.0, 1.0, 1.0).is_err());
        assert!(g.observe_train(1, 128, f64::MAX, f64::MAX, f64::MAX, f64::MAX).is_ok());
        assert!(g.observe_train(1, 128, f64::MAX, f64::MAX, f64::MAX, f64::MAX).is_err());
    }
}
