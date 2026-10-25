//! HYB H-2: MOR routing over H-1d bank-head experts.
//! Protocol: docs/HYB-H2-S5H-MOR-EXPERTS-PREREGISTRATION.md.
//!
//! H-1d put one ternary bank head on top of P1. H-2 replaces it by
//! `EXPERTS` heads of the same shape and a Boolean router: each byte goes
//! to exactly one expert, chosen before the byte is decoded, so the
//! per-byte cost stays one head plus the router.
//!
//! The router keeps one prototype of `ROUTE_WORDS` words per expert and
//! sends a byte to the prototype at the smallest Hamming distance from its
//! route bits (P1 context word and the depth-0 bank thermometers), ties to
//! the smallest index. Prototypes are learned by hard EM on CAL-FIT:
//!
//! 1. initial groups: quartiles of the depth-0 P1 |stretch|; prototypes
//!    are their bitwise majorities;
//! 2. each round routes every byte with the current prototypes and trains
//!    one averaged master per expert on its bytes (H-1d training, same
//!    epochs and averaging), so training always sees the inference routing;
//! 3. between rounds every byte is labelled with the expert whose master
//!    codes it best, and each prototype becomes the bitwise majority of
//!    its labelled bytes (an empty group keeps its prototype).
//!
//! The control router `hash_route` splits bytes by a fixed hash of the
//! context word: same experts, same training, no learned routing. Research
//! only, NOT_PROMOTED.

use crate::{
    hyb_h1_residual::{splitmix, DEPTHS},
    hyb_h1c_stable::{epoch_key, BURN_IN_EPOCHS, EPOCHS},
    hyb_h1d_bank_head::{bank_word, BankMaster, BankRecord, ROW},
};

/// Experts behind the router.
pub const EXPERTS: usize = 4;
/// Words of route bits per byte.
pub const ROUTE_WORDS: usize = 2;
/// Re-routing rounds after the initial partition (so `ROUNDS + 1`
/// trainings).
pub const ROUNDS: usize = 2;

pub type Prototype = [u64; ROUTE_WORDS];

/// Route bits of a byte: P1 context word, depth-0 bank thermometers. Both
/// are known before the first bit of the byte is decoded.
#[inline]
#[must_use]
pub fn route_bits(r: &BankRecord) -> Prototype {
    [r.record.context, bank_word(&r.banks.stretch[0])]
}

/// Learned router: nearest prototype in Hamming distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Router {
    pub prototypes: [Prototype; EXPERTS],
}

impl Router {
    #[inline]
    #[must_use]
    pub fn route(&self, r: &BankRecord) -> usize {
        self.route_bits(&route_bits(r))
    }

    #[inline]
    #[must_use]
    pub fn route_bits(&self, bits: &Prototype) -> usize {
        let mut best = (u32::MAX, 0);
        for (k, p) in self.prototypes.iter().enumerate() {
            let d: u32 = p.iter().zip(bits).map(|(a, b)| (a ^ b).count_ones()).sum();
            if d < best.0 {
                best = (d, k);
            }
        }
        best.1
    }

    /// Scalar oracle of `route`: bit by bit distances.
    #[must_use]
    pub fn oracle_route(&self, r: &BankRecord) -> usize {
        let bits = route_bits(r);
        let mut best = (usize::MAX, 0);
        for (k, p) in self.prototypes.iter().enumerate() {
            let mut d = 0;
            for w in 0..ROUTE_WORDS {
                for i in 0..64 {
                    d += usize::from((p[w] >> i) & 1 != (bits[w] >> i) & 1);
                }
            }
            if d < best.0 {
                best = (d, k);
            }
        }
        best.1
    }
}

/// Control router: fixed hash of the context word.
#[inline]
#[must_use]
pub fn hash_route(r: &BankRecord) -> usize {
    (splitmix(r.record.context ^ 0x4832_4d4f_525f_4354) % EXPERTS as u64) as usize
}

/// Bitwise majority vote of route bits, per group.
#[derive(Debug, Clone)]
pub struct Majority {
    ones: Vec<[u64; 64 * ROUTE_WORDS]>,
    count: [u64; EXPERTS],
}

impl Default for Majority {
    fn default() -> Self {
        Self {
            ones: vec![[0; 64 * ROUTE_WORDS]; EXPERTS],
            count: [0; EXPERTS],
        }
    }
}

impl Majority {
    pub fn add(&mut self, group: usize, bits: &Prototype) {
        self.count[group] += 1;
        for (w, &word) in bits.iter().enumerate() {
            let mut rest = word;
            while rest != 0 {
                self.ones[group][w * 64 + rest.trailing_zeros() as usize] += 1;
                rest &= rest - 1;
            }
        }
    }

    #[must_use]
    pub fn counts(&self) -> [u64; EXPERTS] {
        self.count
    }

    /// Strict majority per bit; an empty group keeps `previous`.
    #[must_use]
    pub fn router(&self, previous: &Router) -> Router {
        let mut prototypes = previous.prototypes;
        for (k, p) in prototypes.iter_mut().enumerate() {
            if self.count[k] == 0 {
                continue;
            }
            *p = [0; ROUTE_WORDS];
            for (i, &n) in self.ones[k].iter().enumerate() {
                if 2 * n > self.count[k] {
                    p[i / 64] |= 1 << (i % 64);
                }
            }
        }
        Router { prototypes }
    }
}

/// Group of the initial partition: quartile of the depth-0 |stretch|,
/// with `edges` the three quartile boundaries.
#[must_use]
pub fn initial_group(r: &BankRecord, edges: &[u16; EXPERTS - 1]) -> usize {
    let m = r.record.logits[0].unsigned_abs();
    edges.iter().filter(|&&e| m >= e).count()
}

/// Quartile boundaries of a histogram of depth-0 |stretch| (index =
/// |stretch|).
#[must_use]
pub fn quartile_edges(histogram: &[u64]) -> [u16; EXPERTS - 1] {
    let total: u64 = histogram.iter().sum();
    let mut edges = [0u16; EXPERTS - 1];
    let mut seen = 0u64;
    let mut next = 0;
    for (m, &n) in histogram.iter().enumerate() {
        seen += n;
        while next < EXPERTS - 1 && seen * EXPERTS as u64 >= total * (next as u64 + 1) {
            edges[next] = (m + 1).min(usize::from(u16::MAX)) as u16;
            next += 1;
        }
    }
    edges
}

/// Trains one averaged master per expert, each on the bytes `route` sends
/// to it, over `docs` for `EPOCHS` permuted epochs (H-1c order keys, the
/// H-1d step), and returns the Polyak averages and the snapshot count.
/// Expert `k` starts from `BankMaster::new(seed ^ k)`.
pub fn train_routed<D: Copy, E>(
    seed: u64,
    docs: &[(u32, D)],
    route: impl Fn(&BankRecord) -> usize,
    mut replay: impl FnMut(D, &mut dyn FnMut(&BankRecord)) -> Result<(), E>,
) -> Result<Option<(Vec<BankMaster>, u64)>, E> {
    let mut masters: Vec<BankMaster> = (0..EXPERTS)
        .map(|k| BankMaster::new(seed ^ ((k as u64) << 40)))
        .collect();
    let mut sum_w = vec![vec![0.0f64; DEPTHS * ROW]; EXPERTS];
    let mut sum_b = vec![[0.0f64; DEPTHS]; EXPERTS];
    let mut samples = 0u64;
    let mut order = docs.to_vec();
    for epoch in 0..EPOCHS {
        order.sort_by_key(|&(doc, _)| (epoch_key(seed, epoch, doc), doc));
        for &(_, handle) in &order {
            replay(handle, &mut |r| masters[route(r)].train(r))?;
            if epoch >= BURN_IN_EPOCHS {
                for (k, m) in masters.iter().enumerate() {
                    for (s, &w) in sum_w[k].iter_mut().zip(m.weights()) {
                        *s += f64::from(w);
                    }
                    for (s, b) in sum_b[k].iter_mut().zip(m.bias()) {
                        *s += f64::from(b);
                    }
                }
                samples += 1;
            }
        }
    }
    if samples == 0 {
        return Ok(None);
    }
    let n = samples as f64;
    let averaged = (0..EXPERTS)
        .map(|k| {
            BankMaster::from_parts(
                sum_w[k].iter().map(|&s| (s / n) as f32).collect(),
                sum_b[k].map(|s| (s / n) as f32),
            )
        })
        .collect();
    Ok(Some((averaged, samples)))
}

/// The expert whose master codes `r` in the fewest bits (ties to the
/// smallest index).
#[must_use]
pub fn best_expert(masters: &[BankMaster], r: &BankRecord) -> usize {
    let mut best = (f64::INFINITY, 0);
    for (k, m) in masters.iter().enumerate() {
        let c = m.cost_bits(r);
        if c < best.0 {
            best = (c, k);
        }
    }
    best.1
}

/// SHA-256 input of a router: prototypes as little-endian words.
#[must_use]
pub fn router_bytes(router: &Router) -> Vec<u8> {
    router
        .prototypes
        .iter()
        .flat_map(|p| p.iter().flat_map(|w| w.to_le_bytes()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyb_h1d_bank_head::replay_bank_records;

    #[test]
    fn router_matches_its_oracle_and_breaks_ties_low() {
        let records = replay_bank_records(4096);
        let router = Router {
            prototypes: [
                route_bits(&records[0]),
                route_bits(&records[1]),
                [u64::MAX, 0],
                [0, u64::MAX],
            ],
        };
        let mut used = [0; EXPERTS];
        for r in &records {
            let k = router.route(r);
            assert_eq!(k, router.oracle_route(r));
            used[k] += 1;
        }
        assert!(used.iter().filter(|&&n| n > 0).count() >= 2);
        let same = Router {
            prototypes: [[7, 7]; EXPERTS],
        };
        assert_eq!(same.route(&records[3]), 0);
        assert_eq!(router.route(&records[0]), 0);
        assert_eq!(router.route(&records[1]), 1);
    }

    #[test]
    fn majority_is_strict_and_empty_groups_keep_their_prototype() {
        let previous = Router {
            prototypes: [[1, 1], [2, 2], [3, 3], [4, 4]],
        };
        let mut vote = Majority::default();
        vote.add(0, &[0b11, 1]);
        vote.add(0, &[0b01, 0]);
        vote.add(0, &[0b01, 1]);
        vote.add(1, &[0b10, 0]);
        vote.add(1, &[0b01, 0]);
        let router = vote.router(&previous);
        assert_eq!(router.prototypes[0], [0b01, 1]);
        // Two votes, one each: no strict majority.
        assert_eq!(router.prototypes[1], [0, 0]);
        assert_eq!(router.prototypes[2], [3, 3]);
        assert_eq!(vote.counts(), [3, 2, 0, 0]);
    }

    #[test]
    fn quartiles_split_a_histogram_in_four() {
        let mut histogram = vec![0u64; 2048];
        for m in 0..1000 {
            histogram[m] += 1;
        }
        let edges = quartile_edges(&histogram);
        assert_eq!(edges, [250, 500, 750]);
        let records = replay_bank_records(2048);
        let mut groups = [0usize; EXPERTS];
        for r in &records {
            groups[initial_group(r, &edges)] += 1;
        }
        assert_eq!(groups.iter().sum::<usize>(), records.len());
    }

    #[test]
    fn routed_training_specializes_and_hash_route_is_balanced() {
        let records = replay_bank_records(8192);
        let docs: Vec<(u32, usize)> = (0..8).map(|d| (d as u32, d)).collect();
        let chunk = records.len() / docs.len();
        let route = |r: &BankRecord| hash_route(r);
        let (masters, samples) = train_routed(5, &docs, route, |d: usize, train| {
            records[d * chunk..(d + 1) * chunk].iter().for_each(train);
            Ok::<(), ()>(())
        })
        .unwrap()
        .unwrap();
        assert_eq!(masters.len(), EXPERTS);
        assert_eq!(
            samples,
            u64::from(EPOCHS - BURN_IN_EPOCHS) * docs.len() as u64
        );
        // Each master only saw its own bytes: masters differ.
        assert_ne!(masters[0].weights(), masters[1].weights());
        let mut counts = [0usize; EXPERTS];
        for r in &records {
            counts[hash_route(r)] += 1;
            assert!(best_expert(&masters, r) < EXPERTS);
        }
        assert!(counts.iter().all(|&n| n > records.len() / 8));
    }

    #[test]
    fn master_cost_bits_matches_the_logistic_code_length() {
        let records = replay_bank_records(64);
        let master = BankMaster::new(3);
        for r in &records {
            let bits = master.cost_bits(r);
            assert!(bits.is_finite() && bits > 0.0);
        }
        // Zero weights: the cost is P1's own logistic code length.
        let zero = BankMaster::from_parts(vec![0.0; DEPTHS * ROW], [0.0; DEPTHS]);
        let r = &records[5];
        let mut want = 0.0;
        for d in 0..DEPTHS {
            let z = f64::from(r.record.logits[d]) / crate::gb11_p1_stream::STRETCH_SCALE;
            let p = 1.0 / (1.0 + (-z).exp());
            let bit = crate::hyb_h1_residual::target_bit(&r.record, d);
            want -= if bit { p.log2() } else { (1.0 - p).log2() };
        }
        assert!((zero.cost_bits(r) - want).abs() < 1e-9);
    }
}
