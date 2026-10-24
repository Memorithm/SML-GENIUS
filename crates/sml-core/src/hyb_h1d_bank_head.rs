//! HYB H-1d: the H-1c averaged ternary head, with the stretch of every P1
//! bank among its inputs, so that it can reweight the banks.
//! Protocol: docs/HYB-H1D-S5H-BANK-HEAD-PREREGISTRATION.md.
//!
//! H-1c saw only the mixed P1 stretch, the per-bank root confidence, the
//! context bytes and the word register; its gain stopped at 0.0196 BPC on
//! CAL-SEL. P1 mixes its banks with a fixed average. Here each decision
//! also gets, per bank, a signed thermometer of the bank's own stretch
//! (`BANK_EDGES`, 6 bits per bank). The correction becomes a sum of
//! piecewise-constant functions of each bank stretch, i.e. a learned
//! ternary mixer on top of the fixed one.
//!
//! P1 writes the bank stretches of each traced byte to a sidecar file
//! (`BankPlane`, `BANK_PLANE_BYTES` per record, same order as the trace).
//! Training, averaging and export follow H-1c exactly (same epochs, step,
//! tau and lambda grids); only the input width changes (three `u64` words).
//! Every kernel output is checked against a scalar oracle. Research only,
//! NOT_PROMOTED.

use crate::{
    gb11_p1_stream::{ByteCode, TraceRecord, MAX_BANKS, STRETCH_SCALE},
    hyb_h1_residual::{
        decision_cost, features, splitmix, target_bit, DEPTHS, LEARNING_RATE, STEP_UNITS,
    },
    hyb_h1b_calibrated::LAMBDA_EIGHTHS,
    hyb_h1c_stable::{epoch_key, BURN_IN_EPOCHS, EPOCHS},
};

/// Input words per decision: context, H-1c high word, bank thermometers.
pub const WORDS: usize = 3;
/// Master row width (bit positions of the three words).
pub const ROW: usize = 64 * WORDS;
/// Inputs actually used: the 124 H-1c inputs plus 6 bits per bank.
pub const INPUTS: usize = 124 + 6 * MAX_BANKS;
/// Bank stretch thermometer edges, in sidecar units (16 stretch units):
/// 0.25, 1 and 3 nats.
pub const BANK_EDGES: [i8; 3] = [4, 16, 48];
/// Sidecar record: doc index, position, then `DEPTHS x MAX_BANKS` i8.
pub const BANK_PLANE_BYTES: usize = 8 + DEPTHS * MAX_BANKS;

/// Bank stretch in sidecar units (truncated toward zero, saturated).
#[must_use]
pub fn quantize(stretch: i32) -> i8 {
    (stretch / 16).clamp(-127, 127) as i8
}

/// Per-bank stretches of one traced byte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BankPlane {
    pub doc_index: u32,
    pub position: u32,
    /// `stretch[depth][bank]`, unused banks 0.
    pub stretch: [[i8; MAX_BANKS]; DEPTHS],
}

impl BankPlane {
    #[must_use]
    pub fn from_code(doc_index: u32, position: u32, code: &ByteCode) -> Self {
        let mut stretch = [[0i8; MAX_BANKS]; DEPTHS];
        for (row, banks) in stretch.iter_mut().zip(&code.banks) {
            for (q, &st) in row.iter_mut().zip(banks) {
                *q = quantize(i32::from(st));
            }
        }
        Self {
            doc_index,
            position,
            stretch,
        }
    }

    #[must_use]
    pub fn encode(&self) -> [u8; BANK_PLANE_BYTES] {
        let mut out = [0u8; BANK_PLANE_BYTES];
        out[0..4].copy_from_slice(&self.doc_index.to_le_bytes());
        out[4..8].copy_from_slice(&self.position.to_le_bytes());
        for (d, row) in self.stretch.iter().enumerate() {
            for (b, &q) in row.iter().enumerate() {
                out[8 + d * MAX_BANKS + b] = q as u8;
            }
        }
        out
    }

    #[must_use]
    pub fn decode(raw: &[u8; BANK_PLANE_BYTES]) -> Self {
        let mut stretch = [[0i8; MAX_BANKS]; DEPTHS];
        for (d, row) in stretch.iter_mut().enumerate() {
            for (b, q) in row.iter_mut().enumerate() {
                *q = raw[8 + d * MAX_BANKS + b] as i8;
            }
        }
        Self {
            doc_index: u32::from_le_bytes(raw[0..4].try_into().expect("4 bytes")),
            position: u32::from_le_bytes(raw[4..8].try_into().expect("4 bytes")),
            stretch,
        }
    }
}

/// A trace record with the bank stretches of the same byte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BankRecord {
    pub record: TraceRecord,
    pub banks: BankPlane,
}

impl BankRecord {
    /// Pairs a trace record with its sidecar record, or `None` when they
    /// are not the same byte.
    #[must_use]
    pub fn pair(record: TraceRecord, banks: BankPlane) -> Option<Self> {
        (record.doc_index == banks.doc_index && record.position == banks.position)
            .then_some(Self { record, banks })
    }
}

/// Signed thermometers of the bank stretches: bit `6b + k` is
/// `q_b >= BANK_EDGES[k]`, bit `6b + 3 + k` is `q_b <= -BANK_EDGES[k]`.
#[inline]
#[must_use]
pub fn bank_word(banks: &[i8; MAX_BANKS]) -> u64 {
    let mut word = 0u64;
    for (b, &q) in banks.iter().enumerate() {
        for (k, &edge) in BANK_EDGES.iter().enumerate() {
            word |= u64::from(q >= edge) << (6 * b + k);
            word |= u64::from(q <= -edge) << (6 * b + 3 + k);
        }
    }
    word
}

/// The three input words of one decision.
#[inline]
#[must_use]
pub fn bank_features(r: &BankRecord, depth: usize) -> [u64; WORDS] {
    let [context, high] = features(&r.record, depth);
    [context, high, bank_word(&r.banks.stretch[depth])]
}

fn for_each_set_bit(words: &[u64; WORDS], mut each: impl FnMut(usize)) {
    for (k, &word) in words.iter().enumerate() {
        let mut rest = word;
        while rest != 0 {
            each(k * 64 + rest.trailing_zeros() as usize);
            rest &= rest - 1;
        }
    }
}

/// f32 master: one row of `ROW` weights and a bias per depth (the same
/// initialization and logistic SGD step as the H-1 master).
#[derive(Debug, Clone)]
pub struct BankMaster {
    weights: Vec<f32>,
    bias: [f32; DEPTHS],
}

impl BankMaster {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut state = seed;
        let weights = (0..DEPTHS * ROW)
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

    pub fn train(&mut self, r: &BankRecord) {
        for depth in 0..DEPTHS {
            let words = bank_features(r, depth);
            let row = &mut self.weights[depth * ROW..(depth + 1) * ROW];
            let mut score = self.bias[depth];
            for_each_set_bit(&words, |i| score += row[i]);
            let z = f64::from(r.record.logits[depth]) / STRETCH_SCALE
                + f64::from(score) * f64::from(STEP_UNITS) / STRETCH_SCALE;
            let p = 1.0 / (1.0 + (-z).exp());
            let y = if target_bit(&r.record, depth) {
                1.0
            } else {
                0.0
            };
            let grad = ((p - y) * f64::from(STEP_UNITS) / STRETCH_SCALE) as f32;
            let step = LEARNING_RATE * grad;
            for_each_set_bit(&words, |i| row[i] -= step);
            self.bias[depth] -= step;
        }
    }

    #[must_use]
    pub fn weights(&self) -> &[f32] {
        &self.weights
    }

    #[must_use]
    pub fn bias(&self) -> [f32; DEPTHS] {
        self.bias
    }
}

/// Trains one seed over `docs` for `EPOCHS` permuted epochs (H-1c order
/// keys) and returns the Polyak average of the masters sampled after every
/// document past `BURN_IN_EPOCHS`, with the snapshot count.
pub fn train_averaged<D: Copy, E>(
    seed: u64,
    docs: &[(u32, D)],
    mut replay: impl FnMut(D, &mut dyn FnMut(&BankRecord)) -> Result<(), E>,
) -> Result<Option<(BankMaster, u64)>, E> {
    let mut master = BankMaster::new(seed);
    let mut sum_w = vec![0.0f64; DEPTHS * ROW];
    let mut sum_b = [0.0f64; DEPTHS];
    let mut samples = 0u64;
    let mut order = docs.to_vec();
    for epoch in 0..EPOCHS {
        order.sort_by_key(|&(doc, _)| (epoch_key(seed, epoch, doc), doc));
        for &(_, handle) in &order {
            replay(handle, &mut |r| master.train(r))?;
            if epoch >= BURN_IN_EPOCHS {
                for (s, &w) in sum_w.iter_mut().zip(&master.weights) {
                    *s += f64::from(w);
                }
                for (s, &b) in sum_b.iter_mut().zip(&master.bias) {
                    *s += f64::from(b);
                }
                samples += 1;
            }
        }
    }
    if samples == 0 {
        return Ok(None);
    }
    let n = samples as f64;
    Ok(Some((
        BankMaster {
            weights: sum_w.iter().map(|&s| (s / n) as f32).collect(),
            bias: sum_b.map(|s| (s / n) as f32),
        },
        samples,
    )))
}

/// Exported head: `+1`/`-1` planes of three words per depth, an integer
/// step and a stretch-unit bias per depth (the H-1b export rule).
#[derive(Debug, Clone)]
pub struct BankHead {
    weights: Vec<i8>,
    plus: [[u64; WORDS]; DEPTHS],
    minus: [[u64; WORDS]; DEPTHS],
    unit: [i32; DEPTHS],
    bias: [i32; DEPTHS],
}

impl BankHead {
    #[must_use]
    pub fn export(master: &BankMaster, tau: f32) -> Self {
        let scale = STEP_UNITS as f32;
        let mut weights = Vec::with_capacity(DEPTHS * ROW);
        let mut unit = [0; DEPTHS];
        for (d, row) in master.weights.chunks(ROW).enumerate() {
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
        let bias = master.bias.map(|b| (b * scale).round() as i32);
        Self::new(weights, unit, bias)
    }

    #[must_use]
    pub fn new(weights: Vec<i8>, unit: [i32; DEPTHS], bias: [i32; DEPTHS]) -> Self {
        assert_eq!(weights.len(), DEPTHS * ROW);
        assert!(weights.iter().all(|w| (-1..=1).contains(w)));
        let mut plus = [[0u64; WORDS]; DEPTHS];
        let mut minus = [[0u64; WORDS]; DEPTHS];
        for (d, row) in weights.chunks(ROW).enumerate() {
            for (i, &w) in row.iter().enumerate() {
                let plane = match w {
                    1 => &mut plus[d],
                    -1 => &mut minus[d],
                    _ => continue,
                };
                plane[i / 64] |= 1 << (i % 64);
            }
        }
        Self {
            weights,
            plus,
            minus,
            unit,
            bias,
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

    /// Two planes of three `u64` words, one `i32` step and one `i32` bias
    /// per depth.
    #[must_use]
    pub const fn resident_bytes() -> usize {
        DEPTHS * (2 * WORDS * 8 + 4 + 4)
    }

    /// Share of zero weights among the `INPUTS` used inputs.
    #[must_use]
    pub fn zero_fraction(&self) -> f64 {
        let used = |i: usize| i < 124 || (128..128 + 6 * MAX_BANKS).contains(&i);
        let (mut zeros, mut total) = (0usize, 0usize);
        for row in self.weights.chunks(ROW) {
            for (_, &w) in row.iter().enumerate().filter(|&(i, _)| used(i)) {
                total += 1;
                zeros += usize::from(w == 0);
            }
        }
        zeros as f64 / total as f64
    }

    /// Ternary dot product with six popcounts.
    #[inline]
    #[must_use]
    pub fn raw_words(&self, words: &[u64; WORDS], depth: usize) -> i32 {
        let (p, m) = (&self.plus[depth], &self.minus[depth]);
        let mut raw = 0i32;
        for k in 0..WORDS {
            raw += (words[k] & p[k]).count_ones() as i32 - (words[k] & m[k]).count_ones() as i32;
        }
        raw
    }

    #[must_use]
    pub fn raw(&self, r: &BankRecord, depth: usize) -> i32 {
        self.raw_words(&bank_features(r, depth), depth)
    }

    /// Scalar oracle: the sum over every row position of weight times bit.
    #[must_use]
    pub fn oracle_raw(&self, r: &BankRecord, depth: usize) -> i32 {
        let words = bank_features(r, depth);
        self.weights[depth * ROW..(depth + 1) * ROW]
            .iter()
            .enumerate()
            .map(|(i, &w)| i32::from(w) * i32::from((words[i / 64] >> (i % 64)) & 1 == 1))
            .sum()
    }

    /// Same floor-and-saturate rule as `ScaledHead::correction`.
    #[inline]
    #[must_use]
    pub fn correction(&self, raw: i32, depth: usize, eighths: i32) -> i32 {
        let full = i64::from(raw) * i64::from(self.unit[depth]) + i64::from(self.bias[depth]);
        (full * i64::from(eighths))
            .div_euclid(8)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }

    #[must_use]
    pub fn cost(&self, r: &BankRecord, eighths: i32) -> f64 {
        (0..DEPTHS)
            .map(|d| {
                let shift = self.correction(self.raw(r, d), d, eighths);
                let stretch = i32::from(r.record.logits[d]).saturating_add(shift);
                decision_cost(stretch, target_bit(&r.record, d))
            })
            .sum()
    }

    #[must_use]
    pub fn costs(&self, r: &BankRecord) -> [f64; LAMBDA_EIGHTHS.len()] {
        let mut out = [0.0; LAMBDA_EIGHTHS.len()];
        for d in 0..DEPTHS {
            let raw = self.raw(r, d);
            let bit = target_bit(&r.record, d);
            for (k, &e) in LAMBDA_EIGHTHS.iter().enumerate() {
                let stretch =
                    i32::from(r.record.logits[d]).saturating_add(self.correction(raw, d, e));
                out[k] += decision_cost(stretch, bit);
            }
        }
        out
    }
}

/// Deterministic synthetic records with bank stretches, for cross-host
/// replay and tests.
#[must_use]
pub fn replay_bank_records(count: usize) -> Vec<BankRecord> {
    crate::hyb_h1_residual::replay_records(count)
        .into_iter()
        .map(|record| {
            let mut state = splitmix(record.context ^ u64::from(record.position));
            let mut stretch = [[0i8; MAX_BANKS]; DEPTHS];
            for row in &mut stretch {
                for q in row.iter_mut() {
                    state = splitmix(state);
                    *q = (state >> 56) as i8;
                }
            }
            BankRecord {
                record,
                banks: BankPlane {
                    doc_index: record.doc_index,
                    position: record.position,
                    stretch,
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        hyb_h1_residual::{a0_cost, Master},
        hyb_h1b_calibrated::{ScaledHead, TAUS},
    };

    /// The target MSB follows bank 2's own stretch sign, while the mixed
    /// logit is zero: only the bank inputs can learn it.
    fn bank_docs(count: usize, seed: u64) -> Vec<Vec<BankRecord>> {
        let mut state = seed;
        (0..count)
            .map(|doc| {
                (0..100u32)
                    .map(|position| {
                        state = splitmix(state);
                        let q = ((state >> 8) % 120) as i8 - 60;
                        let noise = (state >> 50) % 6 == 0;
                        let top = u8::from((q > 0) ^ noise);
                        let mut stretch = [[0i8; MAX_BANKS]; DEPTHS];
                        stretch[0][2] = q;
                        let record = TraceRecord {
                            doc_index: doc as u32,
                            position,
                            split: 2,
                            target: (top << 7) | (state >> 24) as u8 & 0x7f,
                            context: state >> 17,
                            ..TraceRecord::default()
                        };
                        BankRecord {
                            record,
                            banks: BankPlane {
                                doc_index: doc as u32,
                                position,
                                stretch,
                            },
                        }
                    })
                    .collect()
            })
            .collect()
    }

    fn train(seed: u64, corpus: &[Vec<BankRecord>]) -> (BankMaster, u64) {
        let handles: Vec<(u32, usize)> = (0..corpus.len()).map(|i| (i as u32, i)).collect();
        train_averaged(seed, &handles, |i, each| {
            for r in &corpus[i] {
                each(r);
            }
            Ok::<(), ()>(())
        })
        .unwrap()
        .unwrap()
    }

    #[test]
    fn plane_round_trip_quantize_and_pairing() {
        assert_eq!(quantize(2047), 127);
        assert_eq!(quantize(-2047), -127);
        assert_eq!(quantize(-15), 0);
        assert_eq!(quantize(-16), -1);
        for r in replay_bank_records(300) {
            assert_eq!(BankPlane::decode(&r.banks.encode()), r.banks);
            assert!(BankRecord::pair(r.record, r.banks).is_some());
            let mut other = r.banks;
            other.position += 1;
            assert!(BankRecord::pair(r.record, other).is_none());
        }
        let mut code = ByteCode::default();
        code.banks[3][1] = 800;
        code.banks[7][5] = -2047;
        let plane = BankPlane::from_code(4, 9, &code);
        assert_eq!((plane.stretch[3][1], plane.stretch[7][5]), (50, -127));
    }

    #[test]
    fn bank_word_thermometers() {
        let mut banks = [0i8; MAX_BANKS];
        assert_eq!(bank_word(&banks), 0);
        banks[0] = 4;
        banks[1] = -48;
        banks[7] = 127;
        let w = bank_word(&banks);
        assert_eq!(w & 0x3f, 0b000_001);
        assert_eq!((w >> 6) & 0x3f, 0b111_000);
        assert_eq!((w >> 42) & 0x3f, 0b000_111);
        assert_eq!(w >> 48, 0);
        let r = replay_bank_records(50);
        for x in &r {
            let f = bank_features(x, 3);
            assert_eq!([f[0], f[1]], features(&x.record, 3));
        }
    }

    #[test]
    fn kernel_matches_oracle_and_h1c_rule_without_bank_bits() {
        let records = replay_bank_records(2_000);
        let (master, _) = train(11, &bank_docs(100, 3));
        for &tau in &TAUS {
            let head = BankHead::export(&master, tau);
            for r in &records {
                for d in 0..DEPTHS {
                    assert_eq!(head.raw(r, d), head.oracle_raw(r, d));
                }
            }
        }
        // With no bank bits, the export of an H-1 master copied into the
        // first 128 positions gives the H-1c head's corrections.
        let h1 = Master::new(5);
        let mut copy = BankMaster::new(0);
        copy.weights.iter_mut().for_each(|w| *w = 0.0);
        for d in 0..DEPTHS {
            copy.weights[d * ROW..d * ROW + 124]
                .copy_from_slice(&h1.weights()[d * 124..(d + 1) * 124]);
        }
        copy.bias = h1.bias();
        let (ours, theirs) = (
            BankHead::export(&copy, 0.005),
            ScaledHead::export(&h1, 0.005),
        );
        assert_eq!(ours.unit(), theirs.unit());
        for x in records.iter().take(500) {
            let plain = BankRecord {
                banks: BankPlane::default(),
                ..*x
            };
            for &e in &LAMBDA_EIGHTHS {
                assert_eq!(
                    ours.cost(&plain, e).to_bits(),
                    theirs.cost(&plain.record, e).to_bits()
                );
            }
        }
        assert_eq!(BankHead::resident_bytes(), 448);
    }

    #[test]
    fn bank_inputs_learn_what_the_mixed_logit_hides() {
        let corpus = bank_docs(300, 1);
        let dev: Vec<BankRecord> = bank_docs(50, 9).into_iter().flatten().collect();
        let a0: f64 = dev.iter().map(|r| a0_cost(&r.record)).sum();
        let best = |m: &BankMaster| {
            TAUS.iter()
                .map(|&t| {
                    let head = BankHead::export(m, t);
                    let mut bits = [0.0; LAMBDA_EIGHTHS.len()];
                    for r in &dev {
                        for (k, c) in head.costs(r).iter().enumerate() {
                            bits[k] += c;
                        }
                    }
                    bits.into_iter().fold(f64::INFINITY, f64::min)
                })
                .fold(f64::INFINITY, f64::min)
        };
        let (m11, s) = train(11, &corpus);
        assert_eq!(s, u64::from(EPOCHS - BURN_IN_EPOCHS) * 300);
        let (m23, _) = train(23, &corpus);
        let (b11, b23) = (best(&m11), best(&m23));
        assert!(b11 < a0 - 0.3 * dev.len() as f64, "{b11} vs {a0}");
        assert!((b11 - b23).abs() < 0.01 * dev.len() as f64);
    }
}
