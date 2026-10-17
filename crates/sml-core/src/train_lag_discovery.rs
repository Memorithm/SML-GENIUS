//! Bounded supervised discovery of previously unlisted Boolean ANF delays.
//! Only TRAIN labels update votes; no DEV-derived tuning or future bytes.
use crate::anf_delay_line::AnfDelayLine;
use std::cmp::Ordering;

type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Copy, Default, Debug)]
struct Votes {
    discovery_seen: u64,
    discovery_correct: u64,
    confirm_seen: u64,
    confirm_correct: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LagCandidate {
    pub lag: usize,
    pub discovery_accuracy: f64,
    pub confirmation_accuracy: f64,
    pub min_samples: u64,
}

/// TRAIN-only causal scan of up to 4,096 potential feature delays.
/// Scoring is subsampled and partitioned into alternating independent
/// TRAIN sample sets for discovery and confirmation, never DEV.
#[derive(Clone, Debug)]
pub struct TrainLagDiscovery {
    history: AnfDelayLine,
    votes: Vec<Votes>,
    stride: usize,
    retain: usize,
    min_samples: u64,
    alpha: f64,
    train_positions: u64,
    pending: bool,
    frozen: bool,
}

impl TrainLagDiscovery {
    pub fn new(
        capacity: usize,
        sample_stride: usize,
        retain: usize,
        minimum_partition_samples: u64,
        familywise_alpha: f64,
    ) -> Result<Self> {
        if !(1..=4096).contains(&capacity)
            || !(1..=4096).contains(&sample_stride)
            || !(1..=16).contains(&retain)
            || retain > capacity
            || minimum_partition_samples < 32
            || !familywise_alpha.is_finite()
            || !(0.0..1.0).contains(&familywise_alpha)
        {
            return Err("invalid bounded TRAIN-only lag discovery policy");
        }
        Ok(Self {
            history: AnfDelayLine::new(capacity).map_err(|_| "invalid ANF capacity")?,
            votes: vec![Votes::default(); capacity],
            stride: sample_stride,
            retain,
            min_samples: minimum_partition_samples,
            alpha: familywise_alpha,
            train_positions: 0,
            pending: false,
            frozen: false,
        })
    }

    #[must_use]
    pub fn bit_backing_bytes(&self) -> usize {
        self.history.storage_bytes()
    }

    /// Excludes Vec headers, allocator overhead, recurrent model and buffers.
    #[must_use]
    pub fn logical_discovery_bytes(&self) -> usize {
        self.bit_backing_bytes() + self.votes.len() * size_of::<Votes>()
    }

    #[must_use]
    pub fn training_positions(&self) -> u64 {
        self.train_positions
    }

    /// Clear volatile document history but not training evidence.
    pub fn reset_history(&mut self) -> Result<()> {
        if self.pending || self.frozen {
            return Err("invalid TRAIN reset state");
        }
        self.history.reset();
        Ok(())
    }

    /// Past-only halo priming, without a target or sample count.
    pub fn prime_pair(&mut self, older: u8, newer: u8) -> Result<()> {
        if self.pending || self.frozen {
            return Err("invalid causal warmup state");
        }
        self.history.observe_pair(older, newer);
        Ok(())
    }

    pub fn observe_train_pair(&mut self, older: u8, newer: u8) -> Result<()> {
        if self.pending || self.frozen {
            return Err("cannot overwrite uncommitted TRAIN position");
        }
        self.history.observe_pair(older, newer);
        self.pending = true;
        Ok(())
    }

    /// Commit the one current supervised target, never an inference label.
    pub fn learn_train_target(&mut self, positive: bool) -> Result<()> {
        if !self.pending || self.frozen {
            return Err("missing TRAIN observation");
        }
        if self.train_positions % self.stride as u64 == 0 {
            let confirmation = (self.train_positions / self.stride as u64) % 2 == 1;
            for (index, v) in self.votes.iter_mut().enumerate() {
                if let Some(bit) = self.history.read_lag(index + 1) {
                    let correct = u64::from(bit == positive);
                    if confirmation {
                        v.confirm_seen += 1;
                        v.confirm_correct += correct;
                    } else {
                        v.discovery_seen += 1;
                        v.discovery_correct += correct;
                    }
                }
            }
        }
        self.train_positions = self
            .train_positions
            .checked_add(1)
            .ok_or("TRAIN position counter overflow")?;
        self.pending = false;
        Ok(())
    }

    fn confidence_margin(&self, n: u64) -> f64 {
        let comparisons = (2 * self.votes.len()) as f64;
        (f64::ln(comparisons / self.alpha) / (2.0 * n as f64)).sqrt()
    }

    /// Both disjoint TRAIN partitions must beat chance + Bonferroni-
    /// adjusted Hoeffding margin. This is a conservative heuristic gate,
    /// not a guarantee under correlated observations or adaptive sampling.
    pub fn freeze(&mut self) -> Result<Vec<LagCandidate>> {
        if self.pending || self.frozen {
            return Err("pending label or duplicate freeze");
        }
        let mut candidates = Vec::new();
        for (index, votes) in self.votes.iter().enumerate() {
            if votes.discovery_seen < self.min_samples || votes.confirm_seen < self.min_samples {
                continue;
            }
            let discovery_accuracy = votes.discovery_correct as f64 / votes.discovery_seen as f64;
            let confirmation_accuracy = votes.confirm_correct as f64 / votes.confirm_seen as f64;
            if discovery_accuracy > 0.5 + self.confidence_margin(votes.discovery_seen)
                && confirmation_accuracy > 0.5 + self.confidence_margin(votes.confirm_seen)
            {
                candidates.push(LagCandidate {
                    lag: index + 1,
                    discovery_accuracy,
                    confirmation_accuracy,
                    min_samples: votes.discovery_seen.min(votes.confirm_seen),
                });
            }
        }
        candidates.sort_by(|a, b| {
            b.discovery_accuracy
                .min(b.confirmation_accuracy)
                .partial_cmp(&a.discovery_accuracy.min(a.confirmation_accuracy))
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.lag.cmp(&b.lag))
        });
        candidates.truncate(self.retain);
        self.frozen = true;
        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::TrainLagDiscovery;

    fn bytes(n: usize, seed: u32) -> Vec<u8> {
        let mut x = seed;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect()
    }

    #[test]
    fn rejects_invalid_configuration_and_target_order() {
        assert!(TrainLagDiscovery::new(0, 32, 4, 64, 0.01).is_err());
        assert!(TrainLagDiscovery::new(4097, 32, 4, 64, 0.01).is_err());
        assert!(TrainLagDiscovery::new(1024, 0, 4, 64, 0.01).is_err());
        assert!(TrainLagDiscovery::new(1024, 32, 17, 64, 0.01).is_err());
        assert!(TrainLagDiscovery::new(1024, 32, 4, 64, f64::NAN).is_err());
        let mut m = TrainLagDiscovery::new(1024, 32, 4, 64, 0.01).unwrap();
        assert_eq!(m.bit_backing_bytes(), 128);
        assert_eq!(m.logical_discovery_bytes(), 32896);
        assert!(m.learn_train_target(true).is_err());
        m.observe_train_pair(0xff, 1).unwrap();
        assert!(m.observe_train_pair(0xff, 1).is_err());
        assert!(m.reset_history().is_err());
        m.learn_train_target(true).unwrap();
        assert_eq!(m.training_positions(), 1);
        assert!(m.freeze().unwrap().is_empty());
        assert!(m.observe_train_pair(0xff, 1).is_err());
        assert!(m.learn_train_target(true).is_err());
        assert!(m.freeze().is_err());
    }

    #[test]
    fn discovers_previously_unlisted_lags_from_train() {
        let data = bytes(67_000, 0x1636_49be);
        for target_lag in [64_usize, 1023] {
            let mut m = TrainLagDiscovery::new(1024, 32, 3, 64, 0.01).unwrap();
            for i in 2..1024 {
                m.prime_pair(data[i - 2], data[i - 1]).unwrap();
            }
            for i in 1024..66_560 {
                m.observe_train_pair(data[i - 2], data[i - 1]).unwrap();
                let target =
                    ((data[i - target_lag - 1] & data[i - target_lag]).count_ones() & 1) != 0;
                m.learn_train_target(target).unwrap();
            }
            let candidates = m.freeze().unwrap();
            assert_eq!(candidates[0].lag, target_lag);
            assert!(candidates[0].discovery_accuracy > 0.99);
            assert!(candidates[0].confirmation_accuracy > 0.99);
        }
    }

    #[test]
    fn unrelated_labels_fail_conservative_gate() {
        let input = bytes(66_000, 0x812a_6671);
        let independent = bytes(66_000, 0xaf03_5ac1);
        let mut m = TrainLagDiscovery::new(1024, 32, 4, 64, 0.001).unwrap();
        for i in 1024..65_536 {
            m.observe_train_pair(input[i - 2], input[i - 1]).unwrap();
            m.learn_train_target(independent[i] & 1 != 0).unwrap();
        }
        assert!(m.freeze().unwrap().is_empty());
    }
}
