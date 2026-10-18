//! Bounded supervised discovery of previously unlisted Boolean ANF delays.
//! Only TRAIN labels update votes; no DEV-derived tuning or future bytes.
//!
//! Each lag keeps a 2x2 contingency table (delayed feature x target) per
//! partition. A lag is retained only when the phi coefficient rejects
//! *independence* in both partitions with the same sign, so a skewed target
//! base rate or a constant feature cannot pass, and an inverted dependence
//! (`target = NOT feature`, the degree-1 ANF `f + 1`) is kept with
//! `inverted = true`. Discovery and confirmation use alternating contiguous
//! blocks of sampled positions rather than alternating single samples.
//! The threshold is a Bonferroni-corrected sub-Gaussian bound on the
//! asymptotically standard-normal `sqrt(n) * phi`; it assumes the strided
//! samples are approximately independent and is not a proof for strongly
//! autocorrelated streams.
use crate::anf_delay_line::AnfDelayLine;
use std::cmp::Ordering;

type Result<T> = std::result::Result<T, &'static str>;

/// Sampled positions per discovery/confirmation block.
pub const PARTITION_BLOCK_SAMPLES: u64 = 256;

#[derive(Clone, Copy, Default, Debug)]
struct Contingency {
    seen: u64,
    feature_ones: u64,
    target_ones: u64,
    both_ones: u64,
}

impl Contingency {
    fn add(&mut self, feature: bool, target: bool) {
        self.seen += 1;
        self.feature_ones += u64::from(feature);
        self.target_ones += u64::from(target);
        self.both_ones += u64::from(feature && target);
    }

    /// Phi coefficient, or `None` when either margin is constant.
    fn phi(&self) -> Option<f64> {
        let n = self.seen as f64;
        let f = self.feature_ones as f64;
        let t = self.target_ones as f64;
        let variance = f * (n - f) * t * (n - t);
        if variance <= 0.0 {
            return None;
        }
        Some((n * self.both_ones as f64 - f * t) / variance.sqrt())
    }

    /// Fraction of samples where `target == feature XOR inverted`.
    fn agreement(&self, inverted: bool) -> f64 {
        let both_zero = self.seen + self.both_ones - self.feature_ones - self.target_ones;
        let equal = (self.both_ones + both_zero) as f64 / self.seen as f64;
        if inverted {
            1.0 - equal
        } else {
            equal
        }
    }
}

#[derive(Clone, Copy, Default, Debug)]
struct Votes {
    discovery: Contingency,
    confirm: Contingency,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LagCandidate {
    pub lag: usize,
    /// True when the target follows the negated delayed feature.
    pub inverted: bool,
    /// Polarity-adjusted agreement on the discovery partition.
    pub discovery_accuracy: f64,
    /// Polarity-adjusted agreement on the confirmation partition.
    pub confirmation_accuracy: f64,
    pub discovery_phi: f64,
    pub confirmation_phi: f64,
    pub min_samples: u64,
}

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
            || familywise_alpha <= 0.0
            || familywise_alpha >= 1.0
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

    #[must_use]
    pub fn logical_discovery_bytes(&self) -> usize {
        self.bit_backing_bytes() + self.votes.len() * size_of::<Votes>()
    }

    #[must_use]
    pub fn training_positions(&self) -> u64 {
        self.train_positions
    }

    pub fn reset_history(&mut self) -> Result<()> {
        if self.pending || self.frozen {
            return Err("invalid TRAIN reset state");
        }
        self.history.reset();
        Ok(())
    }

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

    pub fn learn_train_target(&mut self, positive: bool) -> Result<()> {
        if !self.pending || self.frozen {
            return Err("missing TRAIN observation");
        }
        if self.train_positions % self.stride as u64 == 0 {
            let sample = self.train_positions / self.stride as u64;
            let confirmation = (sample / PARTITION_BLOCK_SAMPLES) % 2 == 1;
            for (index, v) in self.votes.iter_mut().enumerate() {
                if let Some(bit) = self.history.read_lag(index + 1) {
                    if confirmation {
                        v.confirm.add(bit, positive);
                    } else {
                        v.discovery.add(bit, positive);
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

    /// Minimum |phi| that rejects independence at `n` samples.
    fn phi_threshold(&self, n: u64) -> f64 {
        let comparisons = (2 * self.votes.len()) as f64;
        (2.0 * f64::ln(2.0 * comparisons / self.alpha) / n as f64).sqrt()
    }

    pub fn freeze(&mut self) -> Result<Vec<LagCandidate>> {
        if self.pending || self.frozen {
            return Err("pending label or duplicate freeze");
        }
        let mut candidates = Vec::new();
        for (index, votes) in self.votes.iter().enumerate() {
            let (d, c) = (votes.discovery, votes.confirm);
            if d.seen < self.min_samples || c.seen < self.min_samples {
                continue;
            }
            let (Some(discovery_phi), Some(confirmation_phi)) = (d.phi(), c.phi()) else {
                continue;
            };
            if discovery_phi.signum() != confirmation_phi.signum()
                || discovery_phi.abs() <= self.phi_threshold(d.seen)
                || confirmation_phi.abs() <= self.phi_threshold(c.seen)
            {
                continue;
            }
            let inverted = discovery_phi < 0.0;
            candidates.push(LagCandidate {
                lag: index + 1,
                inverted,
                discovery_accuracy: d.agreement(inverted),
                confirmation_accuracy: c.agreement(inverted),
                discovery_phi,
                confirmation_phi,
                min_samples: d.seen.min(c.seen),
            });
        }
        let strength = |x: &LagCandidate| x.discovery_phi.abs().min(x.confirmation_phi.abs());
        candidates.sort_by(|a, b| {
            strength(b)
                .partial_cmp(&strength(a))
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
    use super::{TrainLagDiscovery, PARTITION_BLOCK_SAMPLES};

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
        assert!(TrainLagDiscovery::new(1024, 32, 4, 64, 0.0).is_err());
        let mut m = TrainLagDiscovery::new(1024, 32, 4, 64, 0.01).unwrap();
        assert_eq!(m.bit_backing_bytes(), 128);
        assert_eq!(m.logical_discovery_bytes(), 128 + 1024 * 64);
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
            assert!(!candidates[0].inverted);
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

    fn feature(bytes: &[u8], i: usize, lag: usize) -> bool {
        ((bytes[i - lag - 1] & bytes[i - lag]).count_ones() & 1) != 0
    }

    #[test]
    fn skewed_target_with_constant_feature_is_not_a_discovery() {
        // 0xFF & 0xFF has even parity: every delayed feature is false, and
        // a 10%-positive independent target agrees with it 90% of the time.
        let labels = bytes(70_000, 0xaf03_5ac1);
        let mut m = TrainLagDiscovery::new(1024, 32, 4, 64, 0.001).unwrap();
        for &label in &labels[1024..66_560] {
            m.observe_train_pair(0xff, 0xff).unwrap();
            m.learn_train_target(label < 26).unwrap();
        }
        assert!(m.freeze().unwrap().is_empty());
    }

    #[test]
    fn skewed_independent_target_fails_on_varying_features() {
        let input = bytes(70_000, 0x812a_6671);
        let labels = bytes(70_000, 0xaf03_5ac1);
        let mut m = TrainLagDiscovery::new(1024, 32, 4, 64, 0.001).unwrap();
        for i in 1024..66_560 {
            m.observe_train_pair(input[i - 2], input[i - 1]).unwrap();
            m.learn_train_target(labels[i] < 26).unwrap();
        }
        assert!(m.freeze().unwrap().is_empty());
    }

    #[test]
    fn inverted_dependence_is_retained_with_polarity() {
        let data = bytes(70_000, 0x1636_49be);
        let mut m = TrainLagDiscovery::new(256, 32, 3, 64, 0.01).unwrap();
        for i in 256..66_560 {
            m.observe_train_pair(data[i - 2], data[i - 1]).unwrap();
            m.learn_train_target(!feature(&data, i, 37)).unwrap();
        }
        let candidates = m.freeze().unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].lag, 37);
        assert!(candidates[0].inverted);
        assert!(candidates[0].discovery_phi < -0.99);
        assert!(candidates[0].confirmation_accuracy > 0.99);
    }

    #[test]
    fn skewed_but_dependent_target_is_found() {
        // target = feature AND coin(1/4): ~12% positive, still dependent.
        let data = bytes(140_000, 0x5eed_1234);
        let coins = bytes(140_000, 0x0bad_cafe);
        let mut m = TrainLagDiscovery::new(128, 32, 3, 64, 0.01).unwrap();
        for i in 128..133_120 {
            m.observe_train_pair(data[i - 2], data[i - 1]).unwrap();
            m.learn_train_target(feature(&data, i, 9) && coins[i] < 64)
                .unwrap();
        }
        let candidates = m.freeze().unwrap();
        assert_eq!(candidates[0].lag, 9);
        assert!(!candidates[0].inverted);
        assert!(candidates[0].discovery_phi > 0.2);
    }

    #[test]
    fn partitions_are_contiguous_sample_blocks() {
        let data = bytes(8_192, 0x2222_3333);
        let mut m = TrainLagDiscovery::new(4, 1, 1, 32, 0.01).unwrap();
        for i in 4..4 + 3 * PARTITION_BLOCK_SAMPLES as usize {
            m.observe_train_pair(data[i - 2], data[i - 1]).unwrap();
            m.learn_train_target(true).unwrap();
        }
        // Blocks: discovery, confirmation, discovery. Lag 1 is readable
        // from the first sample, so it sees every block in full.
        let block = PARTITION_BLOCK_SAMPLES;
        assert_eq!(m.votes[0].discovery.seen, 2 * block);
        assert_eq!(m.votes[0].confirm.seen, block);
    }
}
