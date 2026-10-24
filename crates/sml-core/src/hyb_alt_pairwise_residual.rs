//! H-ALT-2: independent nonlinear ternary residual readout on frozen GB-11
//! traces. Alternates raw bit features with Boolean AND/XOR interactions.
//! All training is on TRAIN; integer output is exact and SBG is immutable.
//! No MOR/page router, GPU code, or measured language BPC is introduced here.

use crate::{
    gb11_p1_stream::{bit_cost, squash_value, TraceRecord, STRETCH_LIMIT, STRETCH_SCALE},
    hyb_alt_ternary_sidecar::{HybridAltError, TernaryRow},
};

pub const FEATURES: usize = 124;
pub const DEPTHS: usize = 8;
pub const MAX_ACTIVE_WEIGHTS: usize = 12;
pub const HEAD_BYTES: usize = DEPTHS * (2 * FEATURES.div_ceil(64) * 8 + 4);
pub const SEEDS: [u64; 2] = [11, 23];
pub const THRESHOLDS: [f32; 3] = [0.125, 0.25, 0.5];
pub const SCALES: [i32; 3] = [16, 32, 64];
const LEARNING_RATE: f32 = 1.0 / 128.0;
const STRETCH_BUCKETS: [i32; 4] = [0, 128, 512, 1024];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    Raw,
    Pair,
}

impl Basis {
    pub const ALL: [Self; 2] = [Self::Raw, Self::Pair];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Pair => "and_xor",
        }
    }
}

/// All 124 logical features are packed as 2 u64 words; high padding is zero.
/// This routine deliberately does not call the existing Claude feature mapper.
#[must_use]
pub fn features(record: &TraceRecord, depth: usize, basis: Basis) -> [u64; 2] {
    assert!(depth < DEPTHS, "MSB decision must be 0..8");
    let mut high = u64::from(record.word);
    match basis {
        Basis::Raw => high |= u64::from(record.confidence) << 16,
        Basis::Pair => {
            for i in 0..16 {
                let and_pair = ((record.context >> i) & (record.context >> (i + 1))) & 1;
                let xor_pair = ((record.context >> i) ^ (record.context >> (i + 32))) & 1;
                high |= and_pair << (16 + i);
                high |= xor_pair << (32 + i);
            }
        }
    }
    for i in 0..depth {
        high |= u64::from((record.target >> (7 - i)) & 1) << (48 + i);
    }
    let stretch = i32::from(record.logits[depth]);
    if stretch < 0 {
        high |= 1 << 55;
    }
    for (i, &edge) in STRETCH_BUCKETS.iter().enumerate() {
        if stretch.abs() >= edge {
            high |= 1 << (56 + i);
        }
    }
    [record.context, high]
}

#[must_use]
pub fn target_bit(record: &TraceRecord, depth: usize) -> bool {
    (record.target >> (7 - depth)) & 1 != 0
}

fn feature_active(packed: &[u64; 2], index: usize) -> bool {
    (packed[index / 64] >> (index % 64)) & 1 != 0
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

#[derive(Clone, Debug)]
pub struct Master {
    basis: Basis,
    weights: Vec<f32>,
    biases: [f32; DEPTHS],
    steps: u64,
}

impl Master {
    #[must_use]
    pub fn new(seed: u64, basis: Basis) -> Self {
        let mut random = seed ^ if basis == Basis::Raw { 0 } else { 0x5041_4952 };
        let weights = (0..DEPTHS * FEATURES)
            .map(|_| {
                random = splitmix(random);
                ((random >> 40) as f32 / (1_u64 << 24) as f32 - 0.5) * 0.004
            })
            .collect();
        Self {
            basis,
            weights,
            biases: [0.0; DEPTHS],
            steps: 0,
        }
    }

    /// Streaming one-pass logistic adjustment; P1's predictor is immutable.
    pub fn train(&mut self, record: &TraceRecord) {
        assert_eq!(record.split, 0, "cannot train on DEV or TEST");
        for depth in 0..DEPTHS {
            let packed = features(record, depth, self.basis);
            let row = &mut self.weights[depth * FEATURES..(depth + 1) * FEATURES];
            let mut residual = self.biases[depth];
            for (index, &weight) in row.iter().enumerate() {
                if feature_active(&packed, index) {
                    residual += weight;
                }
            }
            let z = f64::from(record.logits[depth]) / STRETCH_SCALE + f64::from(residual);
            let probability = 1.0 / (1.0 + (-z).exp());
            let target = f64::from(u8::from(target_bit(record, depth)));
            let step = (LEARNING_RATE as f64 * (probability - target)) as f32;
            for (index, weight) in row.iter_mut().enumerate() {
                if feature_active(&packed, index) {
                    *weight -= step;
                }
            }
            self.biases[depth] -= step;
        }
        self.steps += 1;
    }

    #[must_use]
    pub fn trained_positions(&self) -> u64 {
        self.steps
    }

    pub fn export(&self, tau: f32, scale: i32) -> Result<Head, HybridAltError> {
        if !tau.is_finite() || tau <= 0.0 || !SCALES.contains(&scale) {
            return Err(HybridAltError::InvalidFeatures);
        }
        let mut rows = Vec::with_capacity(DEPTHS);
        for depth in 0..DEPTHS {
            let row = &self.weights[depth * FEATURES..(depth + 1) * FEATURES];
            if row.iter().any(|weight| !weight.is_finite()) || !self.biases[depth].is_finite() {
                return Err(HybridAltError::BiasCouldOverflow);
            }
            let mut ranked: Vec<usize> = (0..FEATURES).filter(|&i| row[i].abs() >= tau).collect();
            ranked.sort_by(|&a, &b| {
                row[b]
                    .abs()
                    .total_cmp(&row[a].abs())
                    .then_with(|| a.cmp(&b))
            });
            let mut weights = [0_i8; FEATURES];
            for &index in ranked.iter().take(MAX_ACTIVE_WEIGHTS) {
                weights[index] = if row[index] >= 0.0 { 1 } else { -1 };
            }
            let bias = (f64::from(self.biases[depth]) * STRETCH_SCALE / f64::from(scale)).round();
            if !(-1_000_000.0..=1_000_000.0).contains(&bias) {
                return Err(HybridAltError::BiasCouldOverflow);
            }
            rows.push(TernaryRow::from_weights(&weights, bias as i32)?);
        }
        Ok(Head {
            basis: self.basis,
            scale,
            rows,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Head {
    basis: Basis,
    scale: i32,
    rows: Vec<TernaryRow>,
}

impl Head {
    #[must_use]
    pub fn basis(&self) -> Basis {
        self.basis
    }

    #[must_use]
    pub fn scale(&self) -> i32 {
        self.scale
    }

    #[must_use]
    pub fn weight_bytes(&self) -> usize {
        HEAD_BYTES
    }

    #[must_use]
    pub fn active_weights(&self) -> usize {
        self.rows
            .iter()
            .map(|row| {
                (0..FEATURES)
                    .filter(|&i| row.weight_at(i) != Some(0))
                    .count()
            })
            .sum()
    }

    /// Always small and checked: score bounded by 124 plus an exported bias.
    pub fn score(&self, record: &TraceRecord, depth: usize) -> i32 {
        self.rows[depth]
            .score_packed(&features(record, depth, self.basis))
            .expect("two zero-padded 124-bit feature words")
    }

    /// Independent scalar oracle through one ternary weight at a time.
    pub fn oracle_score(&self, record: &TraceRecord, depth: usize) -> i32 {
        let words = features(record, depth, self.basis);
        let bools: Vec<bool> = (0..FEATURES)
            .map(|index| feature_active(&words, index))
            .collect();
        self.rows[depth]
            .score_reference(&bools)
            .expect("matching 124-bit feature count")
    }

    #[must_use]
    pub fn cost(&self, record: &TraceRecord) -> f64 {
        (0..DEPTHS)
            .map(|depth| {
                let add = self
                    .score(record, depth)
                    .checked_mul(self.scale)
                    .expect("bounded score and scale");
                let logit = i32::from(record.logits[depth])
                    .checked_add(add)
                    .expect("bounded integer correction")
                    .clamp(-STRETCH_LIMIT, STRETCH_LIMIT);
                bit_cost(squash_value(logit), target_bit(record, depth))
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(context: u64, target: u8, split: u8) -> TraceRecord {
        TraceRecord {
            doc_index: 1,
            position: 2,
            split,
            target,
            word: 0x413a,
            context,
            logits: [-1024, 512, 0, 4, 256, -128, 1024, -2047],
            confidence: 0x0123_4567,
        }
    }

    #[test]
    fn pair_features_are_real_boolean_interactions() {
        let base = sample((1 << 0) | (1 << 1) | (1 << 32), 0b1100_0000, 0);
        let raw = features(&base, 2, Basis::Raw);
        let pair = features(&base, 2, Basis::Pair);
        assert_eq!(raw[0], pair[0]);
        assert_eq!(pair[1] & 0xffff, u64::from(base.word));
        assert_eq!((pair[1] >> 16) & 1, 1); // x0 AND x1
        assert_eq!((pair[1] >> 32) & 1, 0); // x0 XOR x32
        assert_eq!((pair[1] >> 48) & 3, 3); // known prefix
        assert_eq!(pair[1] >> 60, 0); // zero padding
        assert_ne!(raw[1], pair[1]);
    }

    #[test]
    fn exact_packed_vs_scalar_oracle_on_all_12_bit_patterns() {
        for basis in Basis::ALL {
            let mut master = Master::new(11, basis);
            for i in 0..16 {
                master.train(&sample(i, i as u8, 0));
            }
            let head = master.export(0.125, 32).unwrap();
            assert_eq!(head.weight_bytes(), 288);
            assert!(head.active_weights() <= DEPTHS * MAX_ACTIVE_WEIGHTS);
            for input in 0..4096 {
                let record = sample(input, input as u8, 1);
                for depth in 0..DEPTHS {
                    assert_eq!(
                        head.score(&record, depth),
                        head.oracle_score(&record, depth),
                        "basis={basis:?} input={input} depth={depth}"
                    );
                }
            }
        }
    }

    #[test]
    fn zero_ternary_export_is_an_exact_noop_over_p1() {
        for basis in Basis::ALL {
            let head = Master::new(11, basis).export(0.5, 32).unwrap();
            let record = sample(0x9876_5432_1020, 0x42, 1);
            assert_eq!(head.active_weights(), 0);
            // Initialization is tiny and quantizes both the weights and bias to zero.
            assert_eq!(head.cost(&record), record.cost_bits());
        }
    }

    #[test]
    fn refuses_dev_training_and_invalid_export_parameters() {
        let mut master = Master::new(23, Basis::Pair);
        master.train(&sample(0, 42, 0));
        assert_eq!(master.trained_positions(), 1);
        assert!(master.export(f32::NAN, 32).is_err());
        assert!(master.export(0.125, 0).is_err());
        assert!(master.export(0.125, 128).is_err());
    }

    #[test]
    #[should_panic(expected = "cannot train on DEV or TEST")]
    fn dev_labels_cannot_reach_optimizer() {
        Master::new(23, Basis::Raw).train(&sample(0, 42, 1));
    }

    #[test]
    fn test_positions_reproduce_frozen_true_prefix() {
        let record = sample(u64::MAX, 0b1010_1100, 1);
        for depth in 0..DEPTHS {
            let raw = features(&record, depth, Basis::Pair);
            let expected = (0..depth).fold(0_u64, |bits, i| {
                bits | (u64::from((record.target >> (7 - i)) & 1) << i)
            });
            assert_eq!((raw[1] >> 48) & ((1 << depth) - 1), expected);
        }
    }
}
