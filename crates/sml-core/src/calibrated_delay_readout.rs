//! TRAIN-calibrated Boolean readout over jointly addressed ANF delays.
//!
//! The control predictor for recurrence studies must see the same delayed
//! information as the recurrent candidate and must emit calibrated
//! probabilities. A fixed `0.999 / 0.001` decoder is only optimal when the
//! label is a noiseless copy of one delayed bit; with label noise each miss
//! costs about 10 bits, and a single-lag decoder cannot express a
//! composition of several delays. This readout jointly addresses up to
//! `MAX_LAGS` delayed bits of an [`AnfDelayLine`] and predicts
//! `(positives + alpha) / (seen + 2 alpha)` per address, with counts taken
//! from TRAIN only and frozen before any DEV query.
use crate::anf_delay_line::AnfDelayLine;

type Result<T> = std::result::Result<T, &'static str>;

/// Largest number of jointly addressed delays (2^4 count cells).
pub const MAX_LAGS: usize = 4;

/// Largest accepted Dirichlet pseudo-count; keeps `seen + 2 alpha` finite.
pub const MAX_ALPHA: f64 = 1.0e9;

#[derive(Clone, Debug)]
pub struct CalibratedDelayReadout {
    lags: Vec<usize>,
    alpha: f64,
    seen: Vec<u64>,
    positives: Vec<u64>,
    frozen: bool,
}

impl CalibratedDelayReadout {
    /// `lags` are distinct `AnfDelayLine::read_lag` offsets; an empty set is
    /// the TRAIN base-rate prior. `alpha` is the Dirichlet pseudo-count in
    /// `(0, MAX_ALPHA]`.
    pub fn new(lags: &[usize], alpha: f64) -> Result<Self> {
        let mut sorted = lags.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        if lags.len() > MAX_LAGS
            || sorted.len() != lags.len()
            || lags.contains(&0)
            || !alpha.is_finite()
            || alpha <= 0.0
            || alpha > MAX_ALPHA
        {
            return Err("invalid calibrated delay readout policy");
        }
        let cells = 1_usize << lags.len();
        Ok(Self {
            lags: lags.to_vec(),
            alpha,
            seen: vec![0; cells],
            positives: vec![0; cells],
            frozen: false,
        })
    }

    #[must_use]
    pub fn lags(&self) -> &[usize] {
        &self.lags
    }

    #[must_use]
    pub fn logical_bytes(&self) -> usize {
        (self.seen.len() + self.positives.len()) * size_of::<u64>()
    }

    fn address(&self, ring: &AnfDelayLine) -> Result<usize> {
        let mut address = 0_usize;
        for (index, &lag) in self.lags.iter().enumerate() {
            let bit = ring
                .read_lag(lag)
                .ok_or("delay readout lag exceeds observed causal history")?;
            address |= usize::from(bit) << index;
        }
        Ok(address)
    }

    /// Counts one supervised TRAIN target at the ring's current position.
    pub fn observe_train(&mut self, ring: &AnfDelayLine, target: bool) -> Result<()> {
        if self.frozen {
            return Err("calibrated delay readout is frozen");
        }
        let address = self.address(ring)?;
        self.seen[address] = self.seen[address]
            .checked_add(1)
            .ok_or("delay readout count overflow")?;
        self.positives[address] += u64::from(target);
        Ok(())
    }

    /// Ends TRAIN fitting; later targets cannot change the counts.
    pub fn freeze(&mut self) -> Result<()> {
        if self.frozen {
            return Err("calibrated delay readout already frozen");
        }
        if self.seen.iter().all(|&n| n == 0) {
            return Err("calibrated delay readout has no TRAIN evidence");
        }
        self.frozen = true;
        Ok(())
    }

    /// Frozen probability that the target is true at the current position.
    pub fn probability(&self, ring: &AnfDelayLine) -> Result<f64> {
        if !self.frozen {
            return Err("calibrated delay readout must be frozen before inference");
        }
        let address = self.address(ring)?;
        Ok((self.positives[address] as f64 + self.alpha)
            / (self.seen[address] as f64 + 2.0 * self.alpha))
    }
}

#[cfg(test)]
mod tests {
    use super::{AnfDelayLine, CalibratedDelayReadout, MAX_ALPHA};

    fn bytes(n: usize, mut x: u32) -> Vec<u8> {
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect()
    }

    fn feature(data: &[u8], i: usize, lag: usize) -> bool {
        ((data[i - lag - 1] & data[i - lag]).count_ones() & 1) != 0
    }

    fn bits(p: f64, y: bool) -> f64 {
        let p = p.clamp(1e-12, 1.0 - 1e-12);
        -if y { p } else { 1.0 - p }.log2()
    }

    /// Fits on positions `[warm, split)` and scores `[split, end)` for both
    /// the calibrated readout and the fixed 0.999/0.001 single-lag decoder.
    fn fit_and_score(
        lags: &[usize],
        label: impl Fn(&[u8], usize) -> bool,
        data: &[u8],
        warm: usize,
        split: usize,
    ) -> (f64, f64) {
        let mut readout = CalibratedDelayReadout::new(lags, 0.5).unwrap();
        let mut ring = AnfDelayLine::new(warm).unwrap();
        for i in 2..warm {
            ring.observe_pair(data[i - 2], data[i - 1]);
        }
        for i in warm..split {
            ring.observe_pair(data[i - 2], data[i - 1]);
            readout.observe_train(&ring, label(data, i)).unwrap();
        }
        readout.freeze().unwrap();
        let (mut calibrated, mut fixed) = (0.0, 0.0);
        for i in split..data.len() {
            ring.observe_pair(data[i - 2], data[i - 1]);
            let y = label(data, i);
            calibrated += bits(readout.probability(&ring).unwrap(), y);
            let hard = ring.read_lag(lags[0]).unwrap();
            fixed += bits(if hard { 0.999 } else { 0.001 }, y);
        }
        let n = (data.len() - split) as f64;
        (calibrated / n, fixed / n)
    }

    #[test]
    fn noiseless_single_lag_matches_fixed_decoder_quality() {
        let data = bytes(40_000, 0x1636_49be);
        let (calibrated, fixed) =
            fit_and_score(&[64], |d, i| feature(d, i, 64), &data, 1024, 30_000);
        assert!(calibrated < 0.001, "calibrated={calibrated}");
        assert!(fixed < 0.0015, "fixed={fixed}");
    }

    #[test]
    fn label_noise_breaks_the_fixed_decoder_but_not_the_calibrated_one() {
        let data = bytes(80_000, 0x5eed_1234);
        let noise = bytes(80_000, 0x0bad_cafe);
        // 10% of labels flipped: the optimum is H(0.1) = 0.469 bits.
        let label = |d: &[u8], i: usize| feature(d, i, 64) ^ (noise[i] < 26);
        let (calibrated, fixed) = fit_and_score(&[64], label, &data, 1024, 60_000);
        assert!((calibrated - 0.469).abs() < 0.03, "calibrated={calibrated}");
        assert!(fixed > 0.9, "fixed={fixed}");
    }

    #[test]
    fn composed_delays_need_the_joint_address() {
        let data = bytes(80_000, 0x2468_ace0);
        let label = |d: &[u8], i: usize| feature(d, i, 16) ^ feature(d, i, 300);
        let (single, _) = fit_and_score(&[16], label, &data, 1024, 60_000);
        let (joint, _) = fit_and_score(&[16, 300], label, &data, 1024, 60_000);
        assert!(single > 0.98, "single={single}");
        assert!(joint < 0.001, "joint={joint}");
    }

    #[test]
    fn empty_lag_set_is_the_train_base_rate_prior() {
        let mut readout = CalibratedDelayReadout::new(&[], 1.0).unwrap();
        let mut ring = AnfDelayLine::new(8).unwrap();
        ring.observe_pair(1, 1);
        for target in [true, false, false, false] {
            readout.observe_train(&ring, target).unwrap();
        }
        readout.freeze().unwrap();
        let p = readout.probability(&ring).unwrap();
        assert!((p - 2.0 / 6.0).abs() < 1e-15);
        assert_eq!(readout.logical_bytes(), 16);
    }

    #[test]
    fn invalid_policy_and_lifecycle_fail_closed() {
        assert!(CalibratedDelayReadout::new(&[0], 0.5).is_err());
        assert!(CalibratedDelayReadout::new(&[3, 3], 0.5).is_err());
        assert!(CalibratedDelayReadout::new(&[1, 2, 3, 4, 5], 0.5).is_err());
        assert!(CalibratedDelayReadout::new(&[1], 0.0).is_err());
        assert!(CalibratedDelayReadout::new(&[1], f64::NAN).is_err());
        assert!(CalibratedDelayReadout::new(&[1], f64::MAX).is_err());
        let mut strong = CalibratedDelayReadout::new(&[], MAX_ALPHA).unwrap();
        let mut empty_ring = AnfDelayLine::new(1).unwrap();
        empty_ring.observe_pair(0, 0);
        strong.observe_train(&empty_ring, true).unwrap();
        strong.freeze().unwrap();
        let p = strong.probability(&empty_ring).unwrap();
        assert!(p.is_finite() && (p - 0.5).abs() < 1e-9);

        let mut readout = CalibratedDelayReadout::new(&[2], 0.5).unwrap();
        let mut ring = AnfDelayLine::new(8).unwrap();
        ring.observe_pair(1, 1);
        assert!(readout.observe_train(&ring, true).is_err(), "lag 2 unseen");
        assert!(readout.freeze().is_err(), "no TRAIN evidence");
        ring.observe_pair(3, 3);
        assert!(readout.probability(&ring).is_err(), "not frozen");
        readout.observe_train(&ring, true).unwrap();
        readout.freeze().unwrap();
        assert!(readout.observe_train(&ring, false).is_err());
        assert!(readout.freeze().is_err());
        assert!((readout.probability(&ring).unwrap() - 0.75).abs() < 1e-15);
    }
}
