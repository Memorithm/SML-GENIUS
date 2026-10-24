//! Fused inference path of the scaled ternary residual head (H-1b/H-1c).
//!
//! `FastHead` (H-1c) rebuilds the 124 input bits from scratch at every
//! depth and the evaluation code scores each decision with `exp` and `log2`.
//! A coder needs neither: it needs, per byte, the eight corrected P(bit = 1)
//! values on the true MSB path. `FusedHead` computes them with
//!
//! * the depth-independent input words (context, word register, confidence)
//!   built once per byte, and only the prefix, sign and |stretch| bits added
//!   per depth (prefix bits come from one bit reversal of the target);
//! * four popcounts per depth over two `u64` planes;
//! * the correction `floor((raw * unit + bias) * eighths / 8)` as an `i64`
//!   multiply and arithmetic shift (`>> 3` is a floor division by 8);
//! * the shared squash table of the P1 model.
//!
//! Every output is checked against `FastHead` and the scalar oracle in the
//! tests. `CostTable` gives the exact code length of a decision from a
//! clamped stretch by lookup, bit-identical to `decision_cost`.
//! Research only, NOT_PROMOTED.

use crate::{
    gb11_p1_stream::{bit_cost, squash_value, TraceRecord, STRETCH_LIMIT},
    hyb_h1_residual::DEPTHS,
    hyb_h1b_calibrated::ScaledHead,
    hyb_h1c_stable::FastHead,
};

const SPAN: usize = (2 * STRETCH_LIMIT + 1) as usize;

/// P1 squash table over the clamped stretch range.
#[derive(Debug, Clone)]
pub struct SquashTable {
    p1: Vec<u32>,
}

impl Default for SquashTable {
    fn default() -> Self {
        Self {
            p1: (-STRETCH_LIMIT..=STRETCH_LIMIT).map(squash_value).collect(),
        }
    }
}

impl SquashTable {
    /// P(bit = 1) in 1/65536 units for a stretch, clamped as in P1.
    #[inline]
    #[must_use]
    pub fn p1(&self, stretch: i32) -> u32 {
        self.p1[(stretch.clamp(-STRETCH_LIMIT, STRETCH_LIMIT) + STRETCH_LIMIT) as usize]
    }

    #[must_use]
    pub fn bytes(&self) -> usize {
        self.p1.len() * 4
    }
}

/// Exact code length of one decision by lookup: `bits[2 * index + bit]`.
#[derive(Debug, Clone)]
pub struct CostTable {
    bits: Vec<f64>,
}

impl Default for CostTable {
    fn default() -> Self {
        let mut bits = Vec::with_capacity(2 * SPAN);
        for st in -STRETCH_LIMIT..=STRETCH_LIMIT {
            let p = squash_value(st);
            bits.push(bit_cost(p, false));
            bits.push(bit_cost(p, true));
        }
        Self { bits }
    }
}

impl CostTable {
    #[inline]
    #[must_use]
    pub fn cost(&self, stretch: i32, bit: bool) -> f64 {
        let index = (stretch.clamp(-STRETCH_LIMIT, STRETCH_LIMIT) + STRETCH_LIMIT) as usize;
        self.bits[2 * index + usize::from(bit)]
    }

    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bits.len() * 8
    }
}

/// Depth-independent part of the head inputs of one byte, plus the
/// bit-reversed target (bit `j` = decision `j`).
#[derive(Debug, Clone, Copy)]
pub struct SharedInputs {
    context: u64,
    high: u64,
    prefix: u64,
}

impl SharedInputs {
    #[inline]
    #[must_use]
    pub fn new(record: &TraceRecord) -> Self {
        Self {
            context: record.context,
            high: u64::from(record.word) | (u64::from(record.confidence) << 16),
            prefix: u64::from(record.target.reverse_bits()),
        }
    }

    /// The same two words as `hyb_h1_residual::features(record, depth)`.
    #[inline]
    #[must_use]
    pub fn words(&self, stretch: i32, depth: usize) -> [u64; 2] {
        let known = (1u64 << depth.min(7)) - 1;
        let magnitude = stretch.unsigned_abs();
        let thermometer = 1
            | (u64::from(magnitude >= 128) << 1)
            | (u64::from(magnitude >= 512) << 2)
            | (u64::from(magnitude >= 1024) << 3);
        [
            self.context,
            self.high
                | ((self.prefix & known) << 48)
                | (u64::from(stretch < 0) << 55)
                | (thermometer << 56),
        ]
    }
}

/// `FastHead` with the multiplier fixed and the inputs shared across depths.
#[derive(Debug, Clone)]
pub struct FusedHead {
    plus: [[u64; 2]; DEPTHS],
    minus: [[u64; 2]; DEPTHS],
    unit: [i32; DEPTHS],
    bias: [i32; DEPTHS],
    eighths: i32,
}

impl FusedHead {
    #[must_use]
    pub fn new(head: &ScaledHead, eighths: i32) -> Self {
        let fast = FastHead::new(head);
        let mut plus = [[0u64; 2]; DEPTHS];
        let mut minus = [[0u64; 2]; DEPTHS];
        for d in 0..DEPTHS {
            (plus[d], minus[d]) = fast.planes(d);
        }
        Self {
            plus,
            minus,
            unit: head.unit(),
            bias: head.bias(),
            eighths,
        }
    }

    /// Resident bytes of the parameters read at inference: two planes of
    /// two `u64` words, one `i32` step and one `i32` bias per depth, plus
    /// the `i32` multiplier.
    #[must_use]
    pub const fn resident_bytes() -> usize {
        DEPTHS * (2 * 2 * 8 + 4 + 4) + 4
    }

    /// Corrected stretch of each decision of `record` on the true MSB path.
    #[inline]
    #[must_use]
    pub fn stretches(&self, record: &TraceRecord) -> [i32; DEPTHS] {
        let shared = SharedInputs::new(record);
        let mut out = [0i32; DEPTHS];
        for (d, slot) in out.iter_mut().enumerate() {
            let base = i32::from(record.logits[d]);
            let w = shared.words(base, d);
            let (p, m) = (&self.plus[d], &self.minus[d]);
            let raw = i64::from((w[0] & p[0]).count_ones() + (w[1] & p[1]).count_ones())
                - i64::from((w[0] & m[0]).count_ones() + (w[1] & m[1]).count_ones());
            // Floor division by 8 is an arithmetic shift.
            let full = raw * i64::from(self.unit[d]) + i64::from(self.bias[d]);
            let shift = (full * i64::from(self.eighths)) >> 3;
            let shift = shift.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            *slot = base.saturating_add(shift);
        }
        out
    }

    /// The eight P(bit = 1) values a coder would consume, in 1/65536 units.
    #[inline]
    pub fn probabilities(&self, record: &TraceRecord, table: &SquashTable, out: &mut [u32; 8]) {
        for (slot, st) in out.iter_mut().zip(self.stretches(record)) {
            *slot = table.p1(st);
        }
    }

    /// Code length of `record`, bit-identical to `FastHead::cost`.
    #[must_use]
    pub fn cost(&self, record: &TraceRecord, table: &CostTable) -> f64 {
        let mut bits = 0.0;
        for (d, st) in self.stretches(record).into_iter().enumerate() {
            bits += table.cost(st, (record.target >> (7 - d)) & 1 != 0);
        }
        bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        hyb_h1_residual::{decision_cost, features, replay_records, Master, INPUTS},
        hyb_h1b_calibrated::{LAMBDA_EIGHTHS, TAUS},
    };

    fn trained() -> Master {
        let mut master = Master::new(5);
        for r in replay_records(20_000) {
            master.train(&r);
        }
        master
    }

    #[test]
    fn shared_inputs_equal_the_reference_features() {
        for r in replay_records(5_000) {
            let shared = SharedInputs::new(&r);
            for d in 0..DEPTHS {
                assert_eq!(
                    shared.words(i32::from(r.logits[d]), d),
                    features(&r, d),
                    "depth {d}"
                );
            }
        }
        // Thermometer edges.
        let mut r = TraceRecord::default();
        for logit in [
            -2047i16, -1024, -1023, -512, -511, -128, -127, 0, 127, 128, 2047,
        ] {
            r.logits[3] = logit;
            assert_eq!(
                SharedInputs::new(&r).words(i32::from(logit), 3),
                features(&r, 3)
            );
        }
    }

    #[test]
    fn fused_head_matches_fast_head_and_oracle() {
        let master = trained();
        let squash = SquashTable::default();
        let costs = CostTable::default();
        for &tau in &TAUS {
            let scaled = ScaledHead::export(&master, tau);
            let fast = FastHead::new(&scaled);
            for &e in &LAMBDA_EIGHTHS {
                let fused = FusedHead::new(&scaled, e);
                for r in replay_records(3_000) {
                    let st = fused.stretches(&r);
                    let mut p = [0u32; 8];
                    fused.probabilities(&r, &squash, &mut p);
                    for d in 0..DEPTHS {
                        let raw = scaled.oracle_raw(&r, d);
                        let want =
                            i32::from(r.logits[d]).saturating_add(fast.correction(raw, d, e));
                        assert_eq!(st[d], want);
                        assert_eq!(p[d], squash_value(want));
                    }
                    assert_eq!(fused.cost(&r, &costs).to_bits(), fast.cost(&r, e).to_bits());
                }
            }
        }
        assert_eq!(FusedHead::resident_bytes(), 324);
        assert_eq!(INPUTS, 124);
    }

    #[test]
    fn extreme_parameters_saturate_like_the_reference() {
        let scaled = ScaledHead::new(
            vec![1; DEPTHS * INPUTS],
            [i32::MAX; DEPTHS],
            [i32::MIN; DEPTHS],
        );
        let fast = FastHead::new(&scaled);
        for &e in &LAMBDA_EIGHTHS {
            let fused = FusedHead::new(&scaled, e);
            for r in replay_records(200) {
                let st = fused.stretches(&r);
                for (d, &s) in st.iter().enumerate() {
                    let want = i32::from(r.logits[d]).saturating_add(fast.correction(
                        fast.raw(&r, d),
                        d,
                        e,
                    ));
                    assert_eq!(s, want);
                }
            }
        }
    }

    #[test]
    fn cost_table_equals_decision_cost() {
        let table = CostTable::default();
        for st in -3000..3000 {
            for bit in [false, true] {
                assert_eq!(
                    table.cost(st, bit).to_bits(),
                    decision_cost(st, bit).to_bits()
                );
            }
        }
        assert_eq!(table.bytes(), 2 * SPAN * 8);
    }
}
