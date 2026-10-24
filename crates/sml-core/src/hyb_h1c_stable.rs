//! HYB H-1c: the H-1b scaled ternary head, with masters averaged over
//! several epochs and an allocation-free popcount kernel.
//! Protocol: docs/HYB-H1C-S5H-STABLE-RESIDUAL-PREREGISTRATION.md.
//!
//! H-1b missed its gate on the seed spread (one epoch, so the head depended
//! on the document order) and on cost (the bitplane kernel allocates on
//! every call). Here each seed trains `EPOCHS` epochs, each in its own
//! permuted document order, and exports the Polyak–Ruppert average of the
//! master sampled after every document of the epochs past `BURN_IN_EPOCHS`.
//! Inference keeps two `u64` planes per depth and needs four popcounts.
//! Research only, NOT_PROMOTED.

use crate::{
    gb11_p1_stream::TraceRecord,
    hyb_h1_residual::{decision_cost, features, splitmix, target_bit, Master, DEPTHS, INPUTS},
    hyb_h1b_calibrated::{ScaledHead, LAMBDA_EIGHTHS},
};

/// Training epochs per seed over CAL-FIT.
pub const EPOCHS: u32 = 4;
/// Epochs left out of the average.
pub const BURN_IN_EPOCHS: u32 = 1;

/// Sort key of document `doc` in epoch `epoch` for `seed` (epoch 0 is the
/// H-1b order).
#[must_use]
pub fn epoch_key(seed: u64, epoch: u32, doc: u32) -> u64 {
    splitmix(seed ^ (u64::from(epoch) << 32) ^ u64::from(doc))
}

/// Running f64 sum of master snapshots.
#[derive(Debug, Clone)]
pub struct Averager {
    weights: Vec<f64>,
    bias: [f64; DEPTHS],
    samples: u64,
}

impl Default for Averager {
    fn default() -> Self {
        Self {
            weights: vec![0.0; DEPTHS * INPUTS],
            bias: [0.0; DEPTHS],
            samples: 0,
        }
    }
}

impl Averager {
    pub fn add(&mut self, master: &Master) {
        for (sum, &w) in self.weights.iter_mut().zip(master.weights()) {
            *sum += f64::from(w);
        }
        for (sum, w) in self.bias.iter_mut().zip(master.bias()) {
            *sum += f64::from(w);
        }
        self.samples += 1;
    }

    #[must_use]
    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// Mean of the snapshots, or `None` before the first one.
    #[must_use]
    pub fn average(&self) -> Option<Master> {
        if self.samples == 0 {
            return None;
        }
        let n = self.samples as f64;
        Some(Master::from_parts(
            self.weights.iter().map(|&s| (s / n) as f32).collect(),
            self.bias.map(|s| (s / n) as f32),
        ))
    }
}

/// Trains one seed over `docs` (document index plus a handle) for `EPOCHS`
/// permuted epochs and returns the averaged master and the snapshot count.
/// `replay(handle, train)` must feed every record of that document to
/// `train`, in trace order.
pub fn train_averaged<D: Copy, E>(
    seed: u64,
    docs: &[(u32, D)],
    mut replay: impl FnMut(D, &mut dyn FnMut(&TraceRecord)) -> Result<(), E>,
) -> Result<Option<(Master, u64)>, E> {
    let mut master = Master::new(seed);
    let mut averager = Averager::default();
    let mut order = docs.to_vec();
    for epoch in 0..EPOCHS {
        order.sort_by_key(|&(doc, _)| (epoch_key(seed, epoch, doc), doc));
        for &(_, handle) in &order {
            replay(handle, &mut |record| master.train(record))?;
            if epoch >= BURN_IN_EPOCHS {
                averager.add(&master);
            }
        }
    }
    Ok(averager.average().map(|m| (m, averager.samples())))
}

/// Allocation-free form of a `ScaledHead`: `+1` and `-1` input planes per
/// depth, the same integer step, bias and correction.
#[derive(Debug, Clone)]
pub struct FastHead {
    plus: [[u64; 2]; DEPTHS],
    minus: [[u64; 2]; DEPTHS],
    unit: [i32; DEPTHS],
    bias: [i32; DEPTHS],
}

impl FastHead {
    #[must_use]
    pub fn new(head: &ScaledHead) -> Self {
        let mut plus = [[0u64; 2]; DEPTHS];
        let mut minus = [[0u64; 2]; DEPTHS];
        for (d, row) in head.weights().chunks(INPUTS).enumerate() {
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
            plus,
            minus,
            unit: head.unit(),
            bias: head.bias(),
        }
    }

    /// Ternary dot product of packed inputs `words` at `depth`.
    #[inline]
    #[must_use]
    pub fn raw_words(&self, words: &[u64; 2], depth: usize) -> i32 {
        let (p, m) = (&self.plus[depth], &self.minus[depth]);
        ((words[0] & p[0]).count_ones() + (words[1] & p[1]).count_ones()) as i32
            - ((words[0] & m[0]).count_ones() + (words[1] & m[1]).count_ones()) as i32
    }

    #[must_use]
    pub fn raw(&self, record: &TraceRecord, depth: usize) -> i32 {
        self.raw_words(&features(record, depth), depth)
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

    /// Code length of `record` under P1 plus this head at `eighths / 8`.
    #[must_use]
    pub fn cost(&self, record: &TraceRecord, eighths: i32) -> f64 {
        let mut bits = 0.0;
        for d in 0..DEPTHS {
            let shift = self.correction(self.raw(record, d), d, eighths);
            let stretch = i32::from(record.logits[d]).saturating_add(shift);
            bits += decision_cost(stretch, target_bit(record, d));
        }
        bits
    }

    /// Code lengths of `record` for every multiplier of `LAMBDA_EIGHTHS`.
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
    use crate::{
        hyb_h1_residual::{a0_cost, replay_records},
        hyb_h1b_calibrated::TAUS,
    };

    /// Documents of 100 records; the target MSB follows context bit 3 with
    /// noise, over P1 at zero logits.
    fn docs(count: usize, seed: u64) -> Vec<Vec<TraceRecord>> {
        let mut state = seed;
        (0..count)
            .map(|doc| {
                (0..100u32)
                    .map(|position| {
                        state = splitmix(state);
                        let top = u8::from((state >> 3) & 1 == 1 || (state >> 50) % 5 == 0);
                        TraceRecord {
                            doc_index: doc as u32,
                            position,
                            split: 2,
                            target: (top << 7) | (state >> 24) as u8 & 0x7f,
                            context: state,
                            ..TraceRecord::default()
                        }
                    })
                    .collect()
            })
            .collect()
    }

    fn train(seed: u64, corpus: &[Vec<TraceRecord>]) -> (Master, u64) {
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
    fn fast_kernel_matches_oracle_and_scaled_head() {
        let (master, samples) = train(11, &docs(200, 1));
        assert_eq!(samples, u64::from(EPOCHS - BURN_IN_EPOCHS) * 200);
        for &tau in &TAUS {
            let scaled = ScaledHead::export(&master, tau);
            let fast = FastHead::new(&scaled);
            for r in replay_records(2_000) {
                for d in 0..DEPTHS {
                    let raw = fast.raw(&r, d);
                    assert_eq!(raw, scaled.oracle_raw(&r, d));
                    assert_eq!(raw, scaled.raw(&r, d));
                    for &e in &LAMBDA_EIGHTHS {
                        assert_eq!(fast.correction(raw, d, e), scaled.correction(raw, d, e));
                    }
                }
                let (a, b) = (fast.costs(&r), scaled.costs(&r));
                assert_eq!(a.map(f64::to_bits), b.map(f64::to_bits));
                assert_eq!(fast.cost(&r, 0).to_bits(), a0_cost(&r).to_bits());
            }
        }
        let saturating = FastHead::new(&ScaledHead::new(
            vec![1; DEPTHS * INPUTS],
            [i32::MAX; DEPTHS],
            [0; DEPTHS],
        ));
        assert_eq!(saturating.correction(INPUTS as i32, 0, 16), i32::MAX);
    }

    #[test]
    fn averager_is_the_mean_of_its_snapshots() {
        assert!(Averager::default().average().is_none());
        let (a, b) = (Master::new(1), Master::new(2));
        let mut avg = Averager::default();
        avg.add(&a);
        avg.add(&b);
        let mean = avg.average().unwrap();
        for ((&m, &x), &y) in mean.weights().iter().zip(a.weights()).zip(b.weights()) {
            assert!((f64::from(m) - (f64::from(x) + f64::from(y)) / 2.0).abs() < 1e-9);
        }
        assert_eq!(
            epoch_key(11, 0, 5),
            splitmix(11 ^ 5),
            "epoch 0 = H-1b order"
        );
    }

    #[test]
    fn averaged_heads_learn_and_agree_across_seeds() {
        let corpus = docs(300, 1);
        let (m11, _) = train(11, &corpus);
        let (m23, _) = train(23, &corpus);
        let dev: Vec<TraceRecord> = docs(50, 7).into_iter().flatten().collect();
        let a0: f64 = dev.iter().map(a0_cost).sum();
        let bpc = |m: &Master| {
            let mut best = f64::INFINITY;
            for &tau in &TAUS {
                let head = FastHead::new(&ScaledHead::export(m, tau));
                let mut bits = [0.0; LAMBDA_EIGHTHS.len()];
                for r in &dev {
                    for (k, c) in head.costs(r).iter().enumerate() {
                        bits[k] += c;
                    }
                }
                best = bits.into_iter().fold(best, f64::min);
            }
            best
        };
        let (b11, b23) = (bpc(&m11), bpc(&m23));
        assert!(b11 < a0 - 0.5 * dev.len() as f64, "{b11} vs {a0}");
        assert!(
            (b11 - b23).abs() < 0.01 * dev.len() as f64,
            "{b11} vs {b23}"
        );
    }
}
