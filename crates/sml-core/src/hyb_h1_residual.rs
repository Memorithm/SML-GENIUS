//! HYB H-1: ternary residual head over GB-11 P1 traces on S5h.
//! Protocol: docs/HYB-H1-S5H-TERNARY-RESIDUAL-PREREGISTRATION.md.
//!
//! P1 records only the logits of the true MSB path, so every arm scores a
//! byte as 8 binary decisions. A1 adds, per depth, one `TernaryLayer` output
//! to the P1 stretch; A0c adds a hashed count-table correction of the same
//! byte size. Masters are f32 and exist only during training; inference is
//! integer up to the shared squash table. Research only, NOT_PROMOTED.

use crate::{
    gb11_p1_stream::{bit_cost, squash_value, TraceRecord, STRETCH_LIMIT, STRETCH_SCALE},
    ternary_bitplane::{ternary_forward_oracle, BitplaneActivation, TernaryLayer},
};

/// Head inputs per depth: context 64, word 16, confidence 32, prefix 7,
/// stretch sign 1, |stretch| thermometer 4.
pub const INPUTS: usize = 124;
pub const DEPTHS: usize = 8;
/// One head unit is 1/8 ln-odds, i.e. 32 stretch units.
pub const STEP_UNITS: i32 = 32;
/// Export thresholds, picked on TRAIN-HOLD.
pub const TAUS: [f32; 3] = [0.0625, 0.125, 0.25];
/// A0c interpolation weights, picked on TRAIN-HOLD.
pub const LAMBDAS: [f64; 4] = [0.0, 0.25, 0.5, 1.0];
/// Fixed SGD step for the f32 masters (one epoch, record order).
pub const LEARNING_RATE: f32 = 1.0 / 128.0;
/// |stretch| thermometer over the four registered buckets (< 128, < 512,
/// < 1024, >= 1024): bit 0 is always on, bit k is on from `MAGNITUDE_EDGES[k]`.
const MAGNITUDE_EDGES: [i32; 4] = [0, 128, 512, 1024];

/// Bit `7 - depth` of the target, the decision taken at `depth`.
#[must_use]
pub fn target_bit(record: &TraceRecord, depth: usize) -> bool {
    (record.target >> (7 - depth)) & 1 != 0
}

/// Packed head inputs for one decision (bits past `INPUTS` are zero).
#[must_use]
pub fn features(record: &TraceRecord, depth: usize) -> [u64; 2] {
    let stretch = i32::from(record.logits[depth]);
    let mut high = u64::from(record.word) | (u64::from(record.confidence) << 16);
    for j in 0..depth.min(7) {
        if target_bit(record, j) {
            high |= 1 << (48 + j);
        }
    }
    if stretch < 0 {
        high |= 1 << 55;
    }
    for (k, edge) in MAGNITUDE_EDGES.iter().enumerate() {
        if stretch.abs() >= *edge {
            high |= 1 << (56 + k);
        }
    }
    [record.context, high]
}

pub(crate) fn feature_bit(words: &[u64; 2], index: usize) -> bool {
    (words[index / 64] >> (index % 64)) & 1 != 0
}

/// Code length of one decision under a corrected stretch.
#[must_use]
pub fn decision_cost(stretch: i32, bit: bool) -> f64 {
    bit_cost(
        squash_value(stretch.clamp(-STRETCH_LIMIT, STRETCH_LIMIT)),
        bit,
    )
}

/// P1 alone (arm A0).
#[must_use]
pub fn a0_cost(record: &TraceRecord) -> f64 {
    record.cost_bits()
}

/// f32 training masters: one row of `INPUTS` weights and a bias per depth.
#[derive(Debug, Clone)]
pub struct Master {
    weights: Vec<f32>,
    bias: [f32; DEPTHS],
}

impl Master {
    /// Small deterministic initialization from `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut state = seed;
        let weights = (0..DEPTHS * INPUTS)
            .map(|_| {
                state = splitmix(state);
                ((state >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 0.02
            })
            .collect();
        Self {
            weights,
            bias: [0.0; DEPTHS],
        }
    }

    /// One logistic-loss SGD step on every decision of `record`.
    pub fn train(&mut self, record: &TraceRecord) {
        for depth in 0..DEPTHS {
            let words = features(record, depth);
            let row = &mut self.weights[depth * INPUTS..(depth + 1) * INPUTS];
            let mut score = self.bias[depth];
            for (i, w) in row.iter().enumerate() {
                if feature_bit(&words, i) {
                    score += w;
                }
            }
            let z = f64::from(record.logits[depth]) / STRETCH_SCALE
                + f64::from(score) * f64::from(STEP_UNITS) / STRETCH_SCALE;
            let p = 1.0 / (1.0 + (-z).exp());
            let y = if target_bit(record, depth) { 1.0 } else { 0.0 };
            let grad = ((p - y) * f64::from(STEP_UNITS) / STRETCH_SCALE) as f32;
            let step = LEARNING_RATE * grad;
            for (i, w) in row.iter_mut().enumerate() {
                if feature_bit(&words, i) {
                    *w -= step;
                }
            }
            self.bias[depth] -= step;
        }
    }

    /// f32 weights, `DEPTHS` rows of `INPUTS`.
    #[must_use]
    pub fn weights(&self) -> &[f32] {
        &self.weights
    }

    /// f32 bias per depth, in head units (1/8 ln-odds).
    #[must_use]
    pub fn bias(&self) -> [f32; DEPTHS] {
        self.bias
    }

    /// Ternary export: `±1` where `|w| >= tau`, bias rounded to `i32`.
    #[must_use]
    pub fn export(&self, tau: f32) -> Head {
        let weights: Vec<i8> = self
            .weights
            .iter()
            .map(|&w| {
                if w >= tau {
                    1
                } else if w <= -tau {
                    -1
                } else {
                    0
                }
            })
            .collect();
        let bias = self.bias.map(|b| b.round() as i32);
        Head::new(weights, bias)
    }
}

/// Exported integer head: one single-output `TernaryLayer` per depth.
#[derive(Debug, Clone)]
pub struct Head {
    weights: Vec<i8>,
    bias: [i32; DEPTHS],
    layers: Vec<TernaryLayer>,
}

impl Head {
    #[must_use]
    pub fn new(weights: Vec<i8>, bias: [i32; DEPTHS]) -> Self {
        assert_eq!(weights.len(), DEPTHS * INPUTS);
        let layers = (0..DEPTHS)
            .map(|d| {
                TernaryLayer::from_weights(&weights[d * INPUTS..(d + 1) * INPUTS], INPUTS, 1)
                    .expect("validated ternary row")
            })
            .collect();
        Self {
            weights,
            bias,
            layers,
        }
    }

    #[must_use]
    pub fn weights(&self) -> &[i8] {
        &self.weights
    }

    #[must_use]
    pub fn bias(&self) -> [i32; DEPTHS] {
        self.bias
    }

    /// Resident inference bytes: two `u64` planes of two words per depth,
    /// plus one `i32` bias per depth.
    #[must_use]
    pub fn resident_bytes() -> usize {
        DEPTHS * (2 * INPUTS.div_ceil(64) * 8 + 4)
    }

    #[must_use]
    pub fn zero_fraction(&self) -> f64 {
        self.weights.iter().filter(|&&w| w == 0).count() as f64 / self.weights.len() as f64
    }

    /// Integer score through the bitplane kernel.
    #[must_use]
    pub fn score(&self, record: &TraceRecord, depth: usize) -> i32 {
        let words = features(record, depth);
        let activation = BitplaneActivation::from_bits(&words, INPUTS).expect("fixed width");
        self.layers[depth]
            .forward(&activation)
            .expect("fixed width")[0]
            + self.bias[depth]
    }

    /// Same score through the independent scalar oracle.
    #[must_use]
    pub fn oracle_score(&self, record: &TraceRecord, depth: usize) -> i32 {
        let words = features(record, depth);
        let values: Vec<u8> = (0..INPUTS)
            .map(|i| u8::from(feature_bit(&words, i)))
            .collect();
        let row = &self.weights[depth * INPUTS..(depth + 1) * INPUTS];
        ternary_forward_oracle(row, &values, 1)[0] + self.bias[depth]
    }

    /// Code length of `record` under P1 plus this head (arm A1).
    #[must_use]
    pub fn cost(&self, record: &TraceRecord) -> f64 {
        (0..DEPTHS)
            .map(|d| {
                let stretch = i32::from(record.logits[d])
                    .saturating_add(self.score(record, d).saturating_mul(STEP_UNITS));
                decision_cost(stretch, target_bit(record, d))
            })
            .sum()
    }
}

/// Equal-byte Boolean control (arm A0c): `(n0, n1)` u16 cells indexed by a
/// fixed hash of the head inputs and the depth.
#[derive(Debug, Clone)]
pub struct CountTable {
    cells: Vec<[u16; 2]>,
}

impl CountTable {
    #[must_use]
    pub fn with_bytes(bytes: usize) -> Self {
        Self {
            cells: vec![[0; 2]; (bytes / 4).max(1)],
        }
    }

    #[must_use]
    pub fn bytes(&self) -> usize {
        self.cells.len() * 4
    }

    fn slot(&self, record: &TraceRecord, depth: usize) -> usize {
        let words = features(record, depth);
        let h = splitmix(words[0] ^ splitmix(words[1] ^ (depth as u64).wrapping_mul(0x9e37)));
        (h % self.cells.len() as u64) as usize
    }

    pub fn train(&mut self, record: &TraceRecord) {
        for depth in 0..DEPTHS {
            let slot = self.slot(record, depth);
            let cell = &mut self.cells[slot];
            let bit = usize::from(target_bit(record, depth));
            if cell[bit] == u16::MAX {
                cell[0] /= 2;
                cell[1] /= 2;
            }
            cell[bit] += 1;
        }
    }

    /// Stretch correction of one cell, in stretch units.
    fn correction(&self, record: &TraceRecord, depth: usize) -> f64 {
        let [n0, n1] = self.cells[self.slot(record, depth)];
        STRETCH_SCALE * ((f64::from(n1) + 0.5) / (f64::from(n0) + 0.5)).ln()
    }

    #[must_use]
    pub fn cost(&self, record: &TraceRecord, lambda: f64) -> f64 {
        (0..DEPTHS)
            .map(|d| {
                let shift = (lambda * self.correction(record, d)).round() as i32;
                let stretch = i32::from(record.logits[d]).saturating_add(shift);
                decision_cost(stretch, target_bit(record, d))
            })
            .sum()
    }
}

/// Deterministic synthetic records for cross-host replay of a head.
#[must_use]
pub fn replay_records(count: usize) -> Vec<TraceRecord> {
    let mut state = 0x4859_4231_u64;
    (0..count)
        .map(|i| {
            state = splitmix(state);
            let mut logits = [0i16; 8];
            for (d, logit) in logits.iter_mut().enumerate() {
                *logit = ((splitmix(state ^ d as u64) % 4095) as i32 - 2047) as i16;
            }
            TraceRecord {
                doc_index: 0,
                position: i as u32,
                split: 1,
                target: (state >> 8) as u8,
                word: (state >> 16) as u16,
                context: splitmix(state ^ 0xc0),
                logits,
                confidence: (state >> 32) as u32,
            }
        })
        .collect()
}

#[must_use]
pub fn splitmix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P1 is uninformative (stretch 0); the target's top bit copies context
    /// bit 3, which a linear head can learn and A0 cannot.
    fn learnable(count: usize, seed: u64) -> Vec<TraceRecord> {
        let mut state = seed;
        (0..count)
            .map(|i| {
                state = splitmix(state);
                let top = (state >> 3) & 1;
                TraceRecord {
                    doc_index: (i / 100) as u32,
                    position: (i % 100) as u32,
                    split: 0,
                    target: ((top << 7) as u8) | ((state >> 20) as u8 & 0x7f),
                    word: 0,
                    context: state,
                    logits: [0; 8],
                    confidence: 0,
                }
            })
            .collect()
    }

    #[test]
    fn features_layout_matches_declared_inputs() {
        let record = TraceRecord {
            target: 0b1010_0000,
            word: 0xffff,
            context: u64::MAX,
            logits: [-2047, 0, 0, 0, 0, 0, 0, 300],
            confidence: u32::MAX,
            ..TraceRecord::default()
        };
        let words = features(&record, 0);
        assert_eq!(words[0], u64::MAX);
        assert_eq!(words[1] & ((1 << 48) - 1), (1 << 48) - 1);
        assert_eq!(words[1] >> 48 & 0x7f, 0, "no prefix at depth 0");
        assert!(feature_bit(&words, 64 + 55), "negative stretch");
        assert_eq!(words[1] >> 56, 0b1111, "saturated thermometer");
        let deep = features(&record, 7);
        assert_eq!(deep[1] >> 48 & 0x7f, 0b0000101, "prefix bits 1,0,1,0,0,0,0");
        assert_eq!(deep[1] >> 55 & 1, 0);
        assert_eq!(deep[1] >> 56, 0b0011, "300 is in the 128..512 bucket");
        let at = |logit: i16| {
            let mut r = record;
            r.logits[0] = logit;
            features(&r, 0)[1] >> 56
        };
        assert_eq!(at(0), 0b0001);
        assert_eq!(at(127), 0b0001);
        assert_eq!(at(-512), 0b0111);
        assert_eq!(at(1024), at(2047), "saturation is not a fifth bucket");
        assert_eq!(words[1] >> (INPUTS - 64), 0, "no bit past INPUTS");
    }

    #[test]
    fn kernel_score_matches_oracle_on_replay_records() {
        let mut master = Master::new(11);
        for record in learnable(2_000, 5) {
            master.train(&record);
        }
        let head = master.export(0.0625);
        for record in replay_records(500) {
            for d in 0..DEPTHS {
                assert_eq!(head.score(&record, d), head.oracle_score(&record, d));
            }
        }
    }

    #[test]
    fn zero_head_reproduces_p1_exactly() {
        let head = Head::new(vec![0; DEPTHS * INPUTS], [0; DEPTHS]);
        let table = CountTable::with_bytes(Head::resident_bytes());
        for record in replay_records(300) {
            assert_eq!(head.cost(&record).to_bits(), a0_cost(&record).to_bits());
            assert_eq!(
                table.cost(&record, 0.0).to_bits(),
                a0_cost(&record).to_bits()
            );
        }
    }

    #[test]
    fn trained_head_beats_p1_on_held_out_learnable_signal() {
        let mut master = Master::new(23);
        for record in learnable(60_000, 1) {
            master.train(&record);
        }
        let dev = learnable(5_000, 2);
        let a0: f64 = dev.iter().map(a0_cost).sum();
        let best = TAUS
            .iter()
            .map(|&tau| {
                let head = master.export(tau);
                dev.iter().map(|r| head.cost(r)).sum::<f64>()
            })
            .fold(f64::INFINITY, f64::min);
        assert!(best < a0 - 0.2 * dev.len() as f64, "a0={a0} best={best}");
    }

    #[test]
    fn count_table_has_head_size_and_learns() {
        let mut table = CountTable::with_bytes(Head::resident_bytes());
        assert_eq!(table.bytes(), Head::resident_bytes());
        assert_eq!(Head::resident_bytes(), 288);
        let train = learnable(20_000, 3);
        for record in &train {
            table.train(record);
        }
        let a0: f64 = train.iter().map(a0_cost).sum();
        let a0c: f64 = train.iter().map(|r| table.cost(r, 1.0)).sum();
        assert!(a0c.is_finite() && a0c != a0);
    }

    #[test]
    fn replay_records_are_deterministic() {
        assert_eq!(replay_records(64), replay_records(64));
    }
}
