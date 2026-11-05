//! DEC-3: SML language-model features for the DEC-2 Boolean decision heads.
//!
//! A GB-11 P1 next-byte model (Boolean-addressed count banks, `sml-core`) is
//! trained once on the DEC-1 TRAIN texts, then frozen. It adds two
//! thermometer codes to the DEC-2 bits, in the `EXTRA` block:
//!
//! * `bpc`: the frozen model's bits per byte on a text. Bit k is on iff the
//!   cost is above 1 + k/8 bits per byte (k = 0..31, so 1 to 5 bpc).
//! * `boundary gain` (pairs only): the bits saved on the first `BOUNDARY`
//!   bytes of B when the model reads A then a space first, against a space
//!   alone. Bit k is on iff the gain is above (k − 8)/2 bits (−4 to +11.5).
//!   The DEC-2 bits never see the words that cross the A/B boundary.
//!
//! The banks, mixer and table size are fixed here, not searched.

use sml_core::gb11_p1_stream::{order_mask, Context, MixRule, Model, Tables, UpdateRule};

use crate::dec2_text::{EXTRA_BASE, THERMO};

/// Bytes of B scored for the boundary gain.
pub const BOUNDARY: usize = 16;
/// Byte orders of the banks (0 = order-0 bank).
pub const ORDERS: [u32; 6] = [0, 1, 2, 3, 4, 6];
/// Total table bytes, split evenly across banks.
pub const TABLE_BYTES: usize = 1 << 24;

/// A frozen-after-training SML P1 byte model.
pub struct Lm {
    tables: Tables,
    model: Model,
}

impl Default for Lm {
    fn default() -> Self {
        Self::new()
    }
}

impl Lm {
    pub fn new() -> Self {
        let masks = ORDERS.iter().map(|&o| order_mask(o)).collect();
        Self {
            tables: Tables::new(),
            model: Model::new(masks, MixRule::Average, UpdateRule::Adaptive, TABLE_BYTES),
        }
    }

    /// One prequential pass over each training text, in the given order.
    /// Returns (total bits, coded bytes).
    pub fn train<'a>(&mut self, texts: impl IntoIterator<Item = &'a str>) -> (f64, u64) {
        let (mut bits, mut n) = (0.0, 0);
        for t in texts {
            let (b, k) =
                self.model
                    .code_document(&self.tables, t.as_bytes(), true, |_, _, _, _| {});
            bits += b;
            n += k;
        }
        (bits, n)
    }

    /// Frozen bits per coded byte of `text` (0 for texts under 3 bytes).
    pub fn bpc(&mut self, text: &str) -> f64 {
        let (b, k) =
            self.model
                .code_document(&self.tables, text.as_bytes(), false, |_, _, _, _| {});
        if k == 0 {
            0.0
        } else {
            b / k as f64
        }
    }

    fn cost_from(&mut self, mut ctx: Context, bytes: &[u8]) -> f64 {
        let mut cost = 0.0;
        for &byte in bytes {
            cost += self.model.code_byte(&self.tables, ctx, byte, false, None);
            ctx.push(byte);
        }
        cost
    }

    /// Bits saved on the first `BOUNDARY` bytes of `b` by reading `a` first.
    pub fn boundary_gain(&mut self, a: &str, b: &str) -> f64 {
        let head = &b.as_bytes()[..b.len().min(BOUNDARY)];
        let mut alone = Context::default();
        alone.push(b' ');
        let mut after = Context::default();
        for &byte in a.as_bytes() {
            after.push(byte);
        }
        after.push(b' ');
        self.cost_from(alone, head) - self.cost_from(after, head)
    }

    /// Fingerprint of the trained tables.
    pub fn weights_sha(&self) -> [u8; 32] {
        self.model.weights_sha()
    }
}

fn thermo(base: u32, value: f64, lo: f64, step: f64) -> impl Iterator<Item = u32> {
    (0..THERMO)
        .filter(move |&k| value > lo + f64::from(k) * step)
        .map(move |k| base + k)
}

/// Bits-per-byte thermometer (EXTRA bits 0..32).
pub fn bpc_bits(bpc: f64) -> Vec<u32> {
    thermo(EXTRA_BASE, bpc, 1.0, 0.125).collect()
}

/// Boundary-gain thermometer (EXTRA bits 32..64).
pub fn gain_bits(gain: f64) -> Vec<u32> {
    thermo(EXTRA_BASE + THERMO, gain, -4.0, 0.5).collect()
}

/// Appends extension bits to a sorted DEC-2 bit set, keeping it sorted.
pub fn with_extra(mut bits: Vec<u32>, extra: &[u32]) -> Vec<u32> {
    bits.extend_from_slice(extra);
    bits.sort_unstable();
    bits.dedup();
    bits
}
