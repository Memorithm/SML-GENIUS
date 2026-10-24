//! HYB H-1b: the H-1 ternary residual head, trained on frozen-model
//! calibration traces and exported with a per-depth integer scale.
//! Protocol: docs/HYB-H1B-S5H-CALIBRATED-RESIDUAL-PREREGISTRATION.md.
//!
//! H-1 exported every surviving master weight as one full head unit (32
//! stretch units) whatever its magnitude. Here each depth keeps the mean
//! magnitude of its surviving weights as an integer step `unit[d]`, the bias
//! is kept in stretch units, and a global multiplier `lambda / 8` (picked on
//! CAL-SEL, 0 included) scales the whole correction. Inference stays
//! integer up to the shared squash table. Research only, NOT_PROMOTED.

use crate::{
    gb11_p1_stream::TraceRecord,
    hyb_h1_residual::{
        decision_cost, feature_bit, features, target_bit, Master, DEPTHS, INPUTS, STEP_UNITS,
    },
    ternary_bitplane::{ternary_forward_oracle, BitplaneActivation, TernaryLayer},
};

/// Export thresholds, picked on CAL-SEL. The grid reaches high values so
/// that selection can keep only the weights that stand out of the noise.
pub const TAUS: [f32; 6] = [0.0625, 0.125, 0.25, 0.5, 1.0, 2.0];
/// Global multiplier on the head correction in eighths:
/// lambda in {0, 1/8, 1/4, 1/2, 1, 2}, picked on CAL-SEL.
pub const LAMBDA_EIGHTHS: [i32; 6] = [0, 1, 2, 4, 8, 16];
/// A0c interpolation weights, picked on CAL-SEL.
pub const TABLE_LAMBDAS: [f64; 6] = [0.0, 0.125, 0.25, 0.5, 1.0, 2.0];

/// Exported integer head with a per-depth step and a stretch-unit bias.
#[derive(Debug, Clone)]
pub struct ScaledHead {
    weights: Vec<i8>,
    unit: [i32; DEPTHS],
    bias: [i32; DEPTHS],
    layers: Vec<TernaryLayer>,
}

impl ScaledHead {
    /// Ternary export of `master` at `tau`: `unit[d]` is the mean magnitude
    /// of the surviving weights of depth `d`, in stretch units (at least 1
    /// when any weight survives, 0 otherwise).
    #[must_use]
    pub fn export(master: &Master, tau: f32) -> Self {
        let scale = STEP_UNITS as f32;
        let mut weights = Vec::with_capacity(DEPTHS * INPUTS);
        let mut unit = [0; DEPTHS];
        for (d, row) in master.weights().chunks(INPUTS).enumerate() {
            let (mut sum, mut kept) = (0.0f32, 0u32);
            for &w in row {
                let t = if w >= tau {
                    1
                } else if w <= -tau {
                    -1
                } else {
                    0
                };
                if t != 0 {
                    sum += w.abs();
                    kept += 1;
                }
                weights.push(t);
            }
            if kept > 0 {
                unit[d] = ((sum / kept as f32 * scale).round() as i32).max(1);
            }
        }
        let bias = master.bias().map(|b| (b * scale).round() as i32);
        Self::new(weights, unit, bias)
    }

    #[must_use]
    pub fn new(weights: Vec<i8>, unit: [i32; DEPTHS], bias: [i32; DEPTHS]) -> Self {
        assert_eq!(weights.len(), DEPTHS * INPUTS);
        let layers = (0..DEPTHS)
            .map(|d| {
                TernaryLayer::from_weights(&weights[d * INPUTS..(d + 1) * INPUTS], INPUTS, 1)
                    .expect("validated ternary row")
            })
            .collect();
        Self {
            weights,
            unit,
            bias,
            layers,
        }
    }

    #[must_use]
    pub fn weights(&self) -> &[i8] {
        &self.weights
    }

    #[must_use]
    pub fn unit(&self) -> [i32; DEPTHS] {
        self.unit
    }

    #[must_use]
    pub fn bias(&self) -> [i32; DEPTHS] {
        self.bias
    }

    /// Resident inference bytes: two `u64` planes of two words, one `i32`
    /// step and one `i32` bias per depth.
    #[must_use]
    pub fn resident_bytes() -> usize {
        DEPTHS * (2 * INPUTS.div_ceil(64) * 8 + 4 + 4)
    }

    #[must_use]
    pub fn zero_fraction(&self) -> f64 {
        self.weights.iter().filter(|&&w| w == 0).count() as f64 / self.weights.len() as f64
    }

    /// Ternary dot product through the bitplane kernel.
    #[must_use]
    pub fn raw(&self, record: &TraceRecord, depth: usize) -> i32 {
        let words = features(record, depth);
        let activation = BitplaneActivation::from_bits(&words, INPUTS).expect("fixed width");
        self.layers[depth]
            .forward(&activation)
            .expect("fixed width")[0]
    }

    /// Same dot product through the independent scalar oracle.
    #[must_use]
    pub fn oracle_raw(&self, record: &TraceRecord, depth: usize) -> i32 {
        let words = features(record, depth);
        let values: Vec<u8> = (0..INPUTS)
            .map(|i| u8::from(feature_bit(&words, i)))
            .collect();
        let row = &self.weights[depth * INPUTS..(depth + 1) * INPUTS];
        ternary_forward_oracle(row, &values, 1)[0]
    }

    /// Stretch correction of depth `depth` for a kernel output `raw`, at
    /// multiplier `eighths / 8` (floor division).
    #[must_use]
    pub fn correction(&self, raw: i32, depth: usize, eighths: i32) -> i32 {
        let full = i64::from(raw) * i64::from(self.unit[depth]) + i64::from(self.bias[depth]);
        (full * i64::from(eighths))
            .div_euclid(8)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }

    /// Code length of `record` under P1 plus this head at `eighths / 8`.
    #[must_use]
    pub fn cost(&self, record: &TraceRecord, eighths: i32) -> f64 {
        (0..DEPTHS)
            .map(|d| {
                let shift = self.correction(self.raw(record, d), d, eighths);
                let stretch = i32::from(record.logits[d]).saturating_add(shift);
                decision_cost(stretch, target_bit(record, d))
            })
            .sum()
    }

    /// Code lengths of `record` for every multiplier of `LAMBDA_EIGHTHS`,
    /// with one kernel call per depth.
    #[must_use]
    pub fn costs(&self, record: &TraceRecord) -> [f64; LAMBDA_EIGHTHS.len()] {
        let mut out = [0.0; LAMBDA_EIGHTHS.len()];
        for d in 0..DEPTHS {
            let raw = self.raw(record, d);
            let bit = target_bit(record, d);
            for (k, &e) in LAMBDA_EIGHTHS.iter().enumerate() {
                let stretch =
                    i32::from(record.logits[d]).saturating_add(self.correction(raw, d, e));
                out[k] += decision_cost(stretch, bit);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyb_h1_residual::{a0_cost, replay_records, splitmix};

    /// P1 at zero logits; the target MSB follows context bit 3, which a
    /// small residual should learn without saturating.
    fn learnable(count: usize, seed: u64) -> Vec<TraceRecord> {
        let mut state = seed;
        (0..count)
            .map(|i| {
                state = splitmix(state);
                let top = u8::from((state >> 3) & 1 == 1 || (state >> 50) % 5 == 0);
                TraceRecord {
                    doc_index: (i / 100) as u32,
                    position: (i % 100) as u32,
                    split: 2,
                    target: (top << 7) | (state >> 24) as u8 & 0x7f,
                    context: state,
                    ..TraceRecord::default()
                }
            })
            .collect()
    }

    #[test]
    fn kernel_matches_oracle_and_lambda_zero_is_p1() {
        let mut master = Master::new(11);
        for r in learnable(20_000, 1) {
            master.train(&r);
        }
        for &tau in &TAUS {
            let head = ScaledHead::export(&master, tau);
            for r in replay_records(2_000) {
                for d in 0..DEPTHS {
                    assert_eq!(head.raw(&r, d), head.oracle_raw(&r, d));
                }
                assert!((head.cost(&r, 0) - a0_cost(&r)).abs() < 1e-12);
                let all = head.costs(&r);
                for (k, &e) in LAMBDA_EIGHTHS.iter().enumerate() {
                    assert!((all[k] - head.cost(&r, e)).abs() < 1e-12);
                }
            }
        }
    }

    #[test]
    fn scaled_export_keeps_the_master_magnitude() {
        let mut master = Master::new(23);
        for r in learnable(60_000, 1) {
            master.train(&r);
        }
        let head = ScaledHead::export(&master, 0.0625);
        let row = &master.weights()[..INPUTS];
        let kept: Vec<f32> = row
            .iter()
            .filter(|w| w.abs() >= 0.0625)
            .map(|w| w.abs())
            .collect();
        let mean = kept.iter().sum::<f32>() / kept.len() as f32;
        assert_eq!(head.unit()[0], ((mean * 32.0).round() as i32).max(1));
        // (tau, lambda) picked on a selection set, judged on another.
        let sel = learnable(5_000, 2);
        let dev = learnable(5_000, 3);
        let (mut best, mut pick) = (f64::INFINITY, (0.0, 0));
        for &tau in &TAUS {
            let h = ScaledHead::export(&master, tau);
            let mut c = [0.0; LAMBDA_EIGHTHS.len()];
            for r in &sel {
                for (k, v) in h.costs(r).iter().enumerate() {
                    c[k] += v;
                }
            }
            for (k, &v) in c.iter().enumerate() {
                if v < best {
                    (best, pick) = (v, (tau, LAMBDA_EIGHTHS[k]));
                }
            }
        }
        let head = ScaledHead::export(&master, pick.0);
        let a0: f64 = dev.iter().map(a0_cost).sum();
        let a1: f64 = dev.iter().map(|r| head.cost(r, pick.1)).sum();
        assert!(
            a1 < a0 - 0.5 * dev.len() as f64,
            "a1 {a1} a0 {a0} pick {pick:?}"
        );
    }

    #[test]
    fn correction_floors_and_saturates() {
        let mut unit = [0; DEPTHS];
        unit[0] = 3;
        let mut bias = [0; DEPTHS];
        bias[0] = -1;
        let head = ScaledHead::new(vec![0; DEPTHS * INPUTS], unit, bias);
        assert_eq!(head.correction(1, 0, 8), 2);
        assert_eq!(head.correction(0, 0, 4), -1, "floor of -1/2");
        assert_eq!(head.correction(1, 0, 0), 0);
        assert_eq!(head.correction(i32::MAX, 0, 16), i32::MAX);
        assert_eq!(ScaledHead::resident_bytes(), 320);
    }
}
