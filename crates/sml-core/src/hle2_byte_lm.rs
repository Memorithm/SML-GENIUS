//! HLE-2 partial harness: byte-level next-byte prediction with routed
//! differentiable logic circuits, measured in bits per character (BPC).
//!
//! Each target byte is predicted MSB-first as 8 binary decisions. The input
//! of decision `j` is the bit vector of the previous `context_bytes` bytes,
//! the already-known high bits of the target and a one-hot of `j`. A page is
//! selected by exact Hamming top-1 over the previous byte (8-bit key) using
//! [`crate::hamming_index::MultiIndexHamming`]; the page's circuit emits
//! `out_gates` bits and the probability of a 1 is their (smoothed) mean.
//!
//! This is a measurement harness only. It is not the GB-11 P1 protocol and
//! its numbers are not comparable with the S5h witness unless run on the
//! same corpus and split.

use crate::diff_logic::{Adam, DiffLogicNetwork, TrainMode};
use crate::hamming_index::{IndexStats, MultiIndexHamming};
use crate::PageId;

/// How page keys are chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMode {
    /// Keys are the `pages` most frequent previous bytes of TRAIN.
    Learned,
    /// Keys are fixed pseudo-random distinct bytes (routing frozen).
    FrozenRandom,
    /// One page (no routing).
    Single,
}

/// Harness configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct ByteLmConfig {
    pub context_bytes: usize,
    pub pages: usize,
    pub widths: Vec<usize>,
    pub epochs: usize,
    pub learning_rate: f64,
    pub batch_bytes: usize,
    pub seed: u64,
    pub route: RouteMode,
    /// When false the gates stay at their random initialisation
    /// (wiring and tables frozen control).
    pub train_gates: bool,
}

/// Held-out measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ByteLmReport {
    pub relaxed_bpc: f64,
    pub hard_bpc: f64,
    pub parameters: usize,
    pub train_bytes: usize,
    pub dev_bytes: usize,
}

const EPS: f64 = 1e-3;

fn features(ctx: &[u8], prefix: u8, j: usize, c: usize, out: &mut Vec<f64>) {
    out.clear();
    for i in 0..c {
        let b = if ctx.len() > i {
            ctx[ctx.len() - 1 - i]
        } else {
            0
        };
        for k in 0..8 {
            out.push(f64::from((b >> k) & 1));
        }
    }
    for k in 0..8 {
        // Known high bits of the target (MSB first); zero when unknown.
        let known = k < j;
        out.push(if known {
            f64::from((prefix >> (7 - k)) & 1)
        } else {
            0.0
        });
    }
    for k in 0..8 {
        out.push(f64::from(k == j));
    }
}

fn page_keys(train: &[u8], cfg: &ByteLmConfig) -> Vec<(PageId, u64)> {
    let p = match cfg.route {
        RouteMode::Single => 1,
        _ => cfg.pages.clamp(1, 256),
    };
    let keys: Vec<u8> = match cfg.route {
        RouteMode::Single => vec![0],
        RouteMode::Learned => {
            let mut counts = [0u64; 256];
            train.iter().for_each(|&b| counts[b as usize] += 1);
            let mut order: Vec<u8> = (0..=255).collect();
            order.sort_by_key(|&b| (std::cmp::Reverse(counts[b as usize]), b));
            order.truncate(p);
            order
        }
        RouteMode::FrozenRandom => {
            let mut order: Vec<u8> = (0..=255).collect();
            let mut s = cfg.seed ^ 0xA5A5;
            for i in (1..256).rev() {
                s = s
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                order.swap(i, (s >> 33) as usize % (i + 1));
            }
            order.truncate(p);
            order
        }
    };
    keys.into_iter()
        .enumerate()
        .map(|(i, k)| (PageId(i as u32), u64::from(k)))
        .collect()
}

/// Train on `train` and report held-out BPC on `dev`.
pub fn train_and_evaluate(train: &[u8], dev: &[u8], cfg: &ByteLmConfig) -> ByteLmReport {
    let c = cfg.context_bytes;
    let n_in = 8 * c + 16;
    let index = MultiIndexHamming::new(8, page_keys(train, cfg)).expect("validated 8-bit keys");
    let mut nets: Vec<DiffLogicNetwork> = (0..index.len())
        .map(|i| {
            DiffLogicNetwork::new(n_in, &cfg.widths, cfg.seed + i as u64, 0.5).expect("widths")
        })
        .collect();
    let mut opts: Vec<Adam> = nets
        .iter()
        .map(|n| Adam::new(n.parameter_count(), cfg.learning_rate))
        .collect();
    let route = |prev: u8| -> usize {
        let mut st = IndexStats::default();
        index.top_k(u64::from(prev), 1, &mut st).expect("in range")[0]
            .1
             .0 as usize
    };
    let mut x = Vec::with_capacity(n_in);
    if cfg.train_gates {
        let mut grads: Vec<Vec<f64>> = nets
            .iter()
            .map(|n| vec![0.0; n.parameter_count()])
            .collect();
        let mut touched = vec![false; nets.len()];
        for _ in 0..cfg.epochs {
            for (pos, &target) in train.iter().enumerate().skip(1) {
                let ctx = &train[pos.saturating_sub(c)..pos];
                let page = route(train[pos - 1]);
                let net = &nets[page];
                let g_out = net.output_count() as f64;
                for j in 0..8 {
                    features(ctx, target, j, c, &mut x);
                    let tr = net.forward(&x, TrainMode::Relaxed).expect("width");
                    let mean = tr.outputs().iter().sum::<f64>() / g_out;
                    let p = EPS + (1.0 - 2.0 * EPS) * mean;
                    let t = f64::from((target >> (7 - j)) & 1);
                    let dp = (p - t) / (p * (1.0 - p));
                    let d = vec![dp * (1.0 - 2.0 * EPS) / g_out; tr.outputs().len()];
                    net.backward(&tr, &d, TrainMode::Relaxed, &mut grads[page]);
                }
                touched[page] = true;
                if pos % cfg.batch_bytes == 0 || pos + 1 == train.len() {
                    for i in 0..nets.len() {
                        if touched[i] {
                            opts[i].step(nets[i].parameters_mut(), &grads[i]);
                            grads[i].iter_mut().for_each(|g| *g = 0.0);
                            touched[i] = false;
                        }
                    }
                }
            }
        }
    }
    let hard: Vec<_> = nets.iter().map(DiffLogicNetwork::discretize).collect();
    let (mut soft_bits, mut hard_bits) = (0.0, 0.0);
    for pos in 1..dev.len() {
        let ctx = &dev[pos.saturating_sub(c)..pos];
        let target = dev[pos];
        let page = route(dev[pos - 1]);
        for j in 0..8 {
            features(ctx, target, j, c, &mut x);
            let t = (target >> (7 - j)) & 1 == 1;
            let tr = nets[page].forward(&x, TrainMode::Relaxed).expect("width");
            let g = tr.outputs().len() as f64;
            let p = EPS + (1.0 - 2.0 * EPS) * tr.outputs().iter().sum::<f64>() / g;
            soft_bits -= if t { p.log2() } else { (1.0 - p).log2() };
            let bits: Vec<bool> = x.iter().map(|&v| v > 0.5).collect();
            let out = hard[page].eval_bits(&bits).expect("width");
            let ones = out.iter().filter(|&&b| b).count() as f64;
            let ph = (ones + 0.5) / (g + 1.0);
            hard_bits -= if t { ph.log2() } else { (1.0 - ph).log2() };
        }
    }
    let n = (dev.len() - 1) as f64;
    ByteLmReport {
        relaxed_bpc: soft_bits / n,
        hard_bpc: hard_bits / n,
        parameters: nets.iter().map(DiffLogicNetwork::parameter_count).sum(),
        train_bytes: train.len(),
        dev_bytes: dev.len(),
    }
}

/// Order-`n` byte count model with add-`alpha` smoothing (reference only).
pub fn ngram_bpc(train: &[u8], dev: &[u8], order: usize, alpha: f64) -> f64 {
    use std::collections::HashMap;
    let mut table: HashMap<&[u8], [u32; 256]> = HashMap::new();
    for pos in order..train.len() {
        table.entry(&train[pos - order..pos]).or_insert([0; 256])[train[pos] as usize] += 1;
    }
    let mut bits = 0.0;
    for pos in 1..dev.len() {
        let lo = pos.saturating_sub(order);
        let counts = if pos >= order {
            table.get(&dev[lo..pos])
        } else {
            None
        };
        let (num, den) = match counts {
            Some(cnt) => (
                f64::from(cnt[dev[pos] as usize]) + alpha,
                f64::from(cnt.iter().sum::<u32>()) + 256.0 * alpha,
            ),
            None => (1.0, 256.0),
        };
        bits -= (num / den).log2();
    }
    bits / (dev.len() - 1) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(route: RouteMode, train_gates: bool) -> ByteLmConfig {
        ByteLmConfig {
            context_bytes: 1,
            pages: 4,
            widths: vec![32, 16],
            epochs: 3,
            learning_rate: 0.05,
            batch_bytes: 16,
            seed: 3,
            route,
            train_gates,
        }
    }

    #[test]
    fn learns_a_periodic_sequence_and_beats_frozen_control() {
        let text: Vec<u8> = b"abcabcabcabd".repeat(40);
        let (train, dev) = text.split_at(360);
        let trained = train_and_evaluate(train, dev, &cfg(RouteMode::Learned, true));
        let frozen = train_and_evaluate(train, dev, &cfg(RouteMode::Learned, false));
        assert!(trained.relaxed_bpc.is_finite() && trained.hard_bpc.is_finite());
        assert!(
            trained.relaxed_bpc < frozen.relaxed_bpc,
            "{trained:?} vs {frozen:?}"
        );
        assert!(trained.relaxed_bpc < 8.0);
        let single = train_and_evaluate(train, dev, &cfg(RouteMode::Single, true));
        assert_eq!(single.parameters * 4, trained.parameters);
    }

    #[test]
    fn ngram_reference_is_sane() {
        let text: Vec<u8> = b"abab".repeat(50);
        assert!(ngram_bpc(&text, &text, 1, 0.01) < 0.5);
        assert!((ngram_bpc(&text, &text, 0, 1.0) - 8.0).abs() < 7.5);
    }

    fn corpus() -> (Vec<u8>, Vec<u8>) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        files.sort();
        let (mut train, mut dev) = (Vec::new(), Vec::new());
        for (i, f) in files.iter().enumerate() {
            let bytes = std::fs::read(f).unwrap();
            if i % 10 == 0 {
                dev.extend(bytes)
            } else {
                train.extend(bytes)
            }
        }
        (train, dev)
    }

    /// HLE-2 linguistic proxy table on the repo's own `docs/*.md` (NOT the
    /// S5h corpus). Run with
    /// `cargo test --release -p sml-core --lib hle2_byte_lm_table -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn hle2_byte_lm_table() {
        let (train_all, dev_all) = corpus();
        let train = &train_all[..train_all.len().min(400_000)];
        let dev = &dev_all[..dev_all.len().min(60_000)];
        println!(
            "HLE2_LM,corpus=repo-docs-md,split=file_index%10==0->dev,train_bytes={},dev_bytes={}",
            train.len(),
            dev.len()
        );
        for order in 0..=3 {
            println!(
                "HLE2_LM,ngram_order{order},params=counts,dev_bpc={:.3}",
                ngram_bpc(train, dev, order, 0.05)
            );
        }
        let base = ByteLmConfig {
            context_bytes: 2,
            pages: 16,
            widths: vec![256, 256, 64],
            epochs: 1,
            learning_rate: 0.02,
            batch_bytes: 64,
            seed: 11,
            route: RouteMode::Learned,
            train_gates: true,
        };
        let arms = [
            ("hle_learned_route", base.clone()),
            (
                "hle_frozen_random_route",
                ByteLmConfig {
                    route: RouteMode::FrozenRandom,
                    ..base.clone()
                },
            ),
            (
                "hle_single_page",
                ByteLmConfig {
                    route: RouteMode::Single,
                    ..base.clone()
                },
            ),
            (
                "hle_frozen_gates",
                ByteLmConfig {
                    train_gates: false,
                    ..base.clone()
                },
            ),
        ];
        println!("HLE2_LM,arm,params,relaxed_dev_bpc,hard_dev_bpc");
        for (name, c) in arms {
            let r = train_and_evaluate(train, dev, &c);
            println!(
                "HLE2_LM,{name},{},{:.3},{:.3}",
                r.parameters, r.relaxed_bpc, r.hard_bpc
            );
        }
    }
}
