//! HLE-3 research measurements.
//!
//! 1. A count-table output head for the HLE-2 byte predictor: smoothed
//!    next-bit counts addressed by the circuit's hard output bits, fitted
//!    after the circuits are trained (one alternation), optionally mixed
//!    with byte-context counts to see what the circuit bits add.
//! 2. Learned (clustered) routing keys via Hamming k-majority over text
//!    context signatures, routed through `HybridLogicEngine` with and
//!    without the exact multi-index Hamming index.
//!
//! All numbers are measurements on the repository's own `docs/*.md`
//! (a proxy corpus), not on S5h, and make no claim beyond what is printed.

use std::collections::HashMap;

/// Interpolated additive (Dirichlet) backoff over a chain of contexts,
/// coarse to fine: `p_l = (n1_l + beta * p_{l-1}) / (n_l + beta)`, with
/// `p_{-1} = 1/2`. This is not Kneser-Ney.
#[derive(Debug, Clone, Default)]
pub struct BackoffBitCounts {
    levels: Vec<HashMap<u64, [u32; 2]>>,
    beta: f64,
}

impl BackoffBitCounts {
    pub fn new(levels: usize, beta: f64) -> Self {
        Self {
            levels: vec![HashMap::new(); levels],
            beta,
        }
    }

    /// Record bit `bit` under the given per-level context keys.
    pub fn observe(&mut self, keys: &[u64], bit: bool) {
        for (table, &k) in self.levels.iter_mut().zip(keys) {
            table.entry(k).or_insert([0; 2])[usize::from(bit)] += 1;
        }
    }

    /// Probability of a 1.
    pub fn p_one(&self, keys: &[u64]) -> f64 {
        let mut p = 0.5;
        for (table, &k) in self.levels.iter().zip(keys) {
            let [n0, n1] = table.get(&k).copied().unwrap_or([0, 0]);
            p = (f64::from(n1) + self.beta * p) / (f64::from(n0 + n1) + self.beta);
        }
        p
    }
}

/// Mix level identity and fields into one 64-bit context key.
pub fn ctx_key(level: u64, fields: &[u64]) -> u64 {
    let mut h = 0xCBF2_9CE4_8422_2325u64 ^ level;
    for &f in fields {
        h ^= f;
        h = h.wrapping_mul(0x0000_0100_0000_01B3).rotate_left(23) ^ (h >> 29);
    }
    h
}

/// Binary-tree node of decision `j` given the target's known high bits
/// (1..=255, unique per prefix).
pub fn tree_node(target: u8, j: usize) -> u64 {
    (1u64 << j) | (u64::from(target) >> (8 - j))
}

/// Fit on `train` and return held-out bits per byte on `dev` for a model
/// whose per-level keys are produced by `keys(text, pos, j, out)`.
pub fn backoff_bpc(
    train: &[u8],
    dev: &[u8],
    levels: usize,
    beta: f64,
    keys_train: &mut dyn FnMut(usize, usize, &mut Vec<u64>),
    keys_dev: &mut dyn FnMut(usize, usize, &mut Vec<u64>),
) -> f64 {
    let mut model = BackoffBitCounts::new(levels, beta);
    let mut k = Vec::with_capacity(levels);
    for (pos, &byte) in train.iter().enumerate().skip(1) {
        for j in 0..8 {
            keys_train(pos, j, &mut k);
            model.observe(&k, (byte >> (7 - j)) & 1 == 1);
        }
    }
    let mut bits = 0.0;
    for (pos, &byte) in dev.iter().enumerate().skip(1) {
        for j in 0..8 {
            keys_dev(pos, j, &mut k);
            let p = model.p_one(&k).clamp(1e-9, 1.0 - 1e-9);
            bits -= if (byte >> (7 - j)) & 1 == 1 {
                p.log2()
            } else {
                (1.0 - p).log2()
            };
        }
    }
    bits / (dev.len() - 1) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hle2_byte_lm::{features, train_circuits, ByteLmConfig, RouteMode, TrainedCircuits};

    #[test]
    fn backoff_counts_learn_and_stay_normalised() {
        let mut m = BackoffBitCounts::new(2, 2.0);
        for _ in 0..50 {
            m.observe(&[1, 10], true);
            m.observe(&[1, 11], false);
        }
        assert!(m.p_one(&[1, 10]) > 0.9);
        assert!(m.p_one(&[1, 11]) < 0.1);
        assert!((m.p_one(&[2, 99]) - 0.5).abs() < 1e-12);
        let text = b"abcabcabcabc".repeat(20);
        let mut kt = |pos: usize, j: usize, out: &mut Vec<u64>| {
            out.clear();
            out.push(ctx_key(0, &[tree_node(text[pos], j)]));
            out.push(ctx_key(
                1,
                &[u64::from(text[pos - 1]), tree_node(text[pos], j)],
            ));
        };
        let text2 = text.clone();
        let mut kd = |pos: usize, j: usize, out: &mut Vec<u64>| {
            out.clear();
            out.push(ctx_key(0, &[tree_node(text2[pos], j)]));
            out.push(ctx_key(
                1,
                &[u64::from(text2[pos - 1]), tree_node(text2[pos], j)],
            ));
        };
        let bpc = backoff_bpc(&text, &text, 2, 2.0, &mut kt, &mut kd);
        assert!(bpc < 0.5, "{bpc}");
        assert_eq!(tree_node(0b1010_0000, 0), 1);
        assert_eq!(tree_node(0b1010_0000, 3), 0b1101);
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

    /// Hard circuit codes (first 12 output bits) per (pos, j), plus page.
    fn codes(tc: &TrainedCircuits, text: &[u8], c: usize) -> Vec<(u16, u16)> {
        let hard: Vec<_> = tc.nets.iter().map(|n| n.discretize()).collect();
        let mut x = Vec::new();
        let mut out = vec![(0, 0); text.len() * 8];
        for pos in 1..text.len() {
            let page = tc.route(text[pos - 1]);
            let ctx = &text[pos.saturating_sub(c)..pos];
            for j in 0..8 {
                features(ctx, text[pos], j, c, &mut x);
                let bits: Vec<bool> = x.iter().map(|&v| v > 0.5).collect();
                let o = hard[page].eval_bits(&bits).unwrap();
                let code = o
                    .iter()
                    .take(12)
                    .enumerate()
                    .fold(0u16, |a, (i, &b)| a | (u16::from(b) << i));
                out[pos * 8 + j] = (page as u16, code);
            }
        }
        out
    }

    /// HLE-3 output-head table. Run with
    /// `cargo test --release -p sml-core --lib hle3_head_table -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn hle3_head_table() {
        let (ta, da) = corpus();
        let train = &ta[..ta.len().min(400_000)];
        let dev = &da[..da.len().min(60_000)];
        println!("HLE3_HEAD,corpus=repo-docs-md,split=file_index%10==0->dev,train_bytes={},dev_bytes={},beta=2", train.len(), dev.len());
        let beta = 2.0;
        let b = |t: &[u8], pos: usize, back: usize| -> u64 {
            if pos >= back {
                u64::from(t[pos - back])
            } else {
                256
            }
        };
        // Byte-context baselines at bit level (same backoff).
        for order in 0..=3usize {
            let mk = |t: &[u8]| {
                let t = t.to_vec();
                move |pos: usize, j: usize, out: &mut Vec<u64>| {
                    out.clear();
                    let mut f = vec![tree_node(t[pos], j)];
                    out.push(ctx_key(0, &f));
                    for o in 1..=order {
                        f.push(b(&t, pos, o));
                        out.push(ctx_key(o as u64, &f));
                    }
                }
            };
            let r = backoff_bpc(train, dev, order + 1, beta, &mut mk(train), &mut mk(dev));
            println!("HLE3_HEAD,bytectx_order{order},dev_bpc={r:.3}");
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
            ("trained_learned_route", base.clone()),
            (
                "trained_frozen_random_route",
                ByteLmConfig {
                    route: RouteMode::FrozenRandom,
                    ..base.clone()
                },
            ),
            (
                "trained_single_page",
                ByteLmConfig {
                    route: RouteMode::Single,
                    ..base.clone()
                },
            ),
            (
                "untrained_gates_random_bits",
                ByteLmConfig {
                    train_gates: false,
                    ..base.clone()
                },
            ),
        ];
        println!("HLE3_HEAD,arm,head,dev_bpc");
        for (name, cfg) in arms {
            let tc = train_circuits(train, &cfg);
            let (ct, cd) = (codes(&tc, train, 2), codes(&tc, dev, 2));
            // Head variants: (label, code bits, byte-context order).
            for &(label, cb, order) in &[
                ("count12", 12u32, 0usize),
                ("count8", 8, 0),
                ("order1+code6", 6, 1),
                ("order2+code6", 6, 2),
            ] {
                let mk = |t: &[u8], codes: &[(u16, u16)]| {
                    let t = t.to_vec();
                    let codes = codes.to_vec();
                    move |pos: usize, j: usize, out: &mut Vec<u64>| {
                        out.clear();
                        let node = tree_node(t[pos], j);
                        let (page, code) = codes[pos * 8 + j];
                        let code = u64::from(code) & ((1u64 << cb) - 1);
                        let mut f = vec![node];
                        out.push(ctx_key(0, &f));
                        if order == 0 {
                            f.push(u64::from(page));
                            out.push(ctx_key(1, &f));
                        }
                        for o in 1..=order {
                            f.push(b(&t, pos, o));
                            out.push(ctx_key(1 + o as u64, &f));
                        }
                        f.push(code);
                        out.push(ctx_key(9, &f));
                    }
                };
                let levels = if order == 0 { 3 } else { order + 2 };
                let r = backoff_bpc(
                    train,
                    dev,
                    levels,
                    beta,
                    &mut mk(train, &ct),
                    &mut mk(dev, &cd),
                );
                println!("HLE3_HEAD,{name},{label},{r:.3}");
            }
        }
    }

    /// HLE-3 learned-key routing bench through `HybridLogicEngine`. Run with
    /// `cargo test --release -p sml-core --lib hle3_route_bench -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn hle3_route_bench() {
        use crate::diff_logic::DiffLogicNetwork;
        use crate::hamming_index::hamming_k_majority;
        use crate::hybrid_logic_engine::{HybridConfig, HybridLogicEngine, HybridPage};
        use crate::{ExecutionCounters, PageId};
        use std::time::Instant;
        let (ta, da) = corpus();
        let sig = |t: &[u8], pos: usize| -> u64 {
            let mut v = [0u8; 8];
            v.copy_from_slice(&t[pos - 8..pos]);
            u64::from_le_bytes(v)
        };
        let train_sigs: Vec<u64> = (8..ta.len().min(400_000))
            .step_by(3)
            .map(|p| sig(&ta, p))
            .collect();
        let queries: Vec<u64> = (8..da.len().min(60_000))
            .step_by(29)
            .take(2000)
            .map(|p| sig(&da, p))
            .collect();
        let hard = DiffLogicNetwork::new(65, &[64], 1, 0.1)
            .unwrap()
            .discretize();
        println!("HLE3_ROUTE,keys,pages,k,scan_ns_per_q,index_ns_per_q,scan_lookups_per_q,index_candidates_per_q,equal");
        for &p in &[4096usize, 65536] {
            let mut s = 99u64;
            let learned = hamming_k_majority(64, &train_sigs, p, 2);
            let random: Vec<u64> = (0..p)
                .map(|_| {
                    s = s
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    s ^ (s >> 29)
                })
                .collect();
            for (label, keys) in [("kmajority_text", learned), ("uniform_random", random)] {
                let pages = keys
                    .iter()
                    .enumerate()
                    .map(|(i, &key)| HybridPage {
                        id: PageId(i as u32),
                        key,
                        network: hard.clone(),
                    })
                    .collect();
                let cfg = HybridConfig {
                    input_bits: 1,
                    workspace_bits: 64,
                    pages_per_cycle: 4,
                    min_cycles: 1,
                    max_cycles: 1,
                };
                let scan = HybridLogicEngine::new(cfg, pages).unwrap();
                let indexed = scan.clone().with_route_index(true);
                let (mut ca, mut cb) = (ExecutionCounters::default(), ExecutionCounters::default());
                let t = Instant::now();
                let ra: Vec<_> = queries.iter().map(|&q| scan.route(q, &mut ca)).collect();
                let ns_scan = t.elapsed().as_nanos() as f64 / queries.len() as f64;
                let t = Instant::now();
                let rb: Vec<_> = queries.iter().map(|&q| indexed.route(q, &mut cb)).collect();
                let ns_idx = t.elapsed().as_nanos() as f64 / queries.len() as f64;
                let n = queries.len() as f64;
                println!(
                    "HLE3_ROUTE,{label},{p},4,{ns_scan:.0},{ns_idx:.0},{:.0},{:.1},{}",
                    ca.route_lookups as f64 / n,
                    cb.route_lookups as f64 / n,
                    ra == rb
                );
                assert_eq!(ra, rb);
            }
        }
    }
}
