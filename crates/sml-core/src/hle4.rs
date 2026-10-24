//! HLE-4 research measurements: does a routed Boolean circuit that sees
//! long context (16..1024 bytes, through fixed-size Boolean summaries) add
//! information on top of order-k counts, a byte cache and a match model?
//!
//! Every predictor is bit-level (MSB-first binary tree per byte) and every
//! long-range mechanism is Boolean or low-bit with bounded state:
//! - order-k interpolated backoff counts ([`crate::hle3::BackoffBitCounts`],
//!   leave-one-out on TRAIN so the mixer is not fitted on memorised targets);
//! - a sliding cache over the last `L` bytes (unigram and bigram bit counts);
//! - a Boolean-addressed match memory (hash of the last 4 / 8 bytes ->
//!   most recent position within `L`), predicting the byte that followed
//!   (copy / induction-like);
//! - a hashed-bigram occupancy sketch of the last `L` bytes (128 bits,
//!   bucket bit = above-average count), fed to the circuits.
//!
//! A per-decision logistic mixer (8 weight vectors, one per bit depth)
//! combines stretched probabilities, match signals and the circuits'
//! output bits. Circuits are trained through the mixer (relaxed), then the
//! mixer is refitted on the hard circuit bits that are used at evaluation.
//! No attention, softmax over tokens or KV cache is involved.
//!
//! Measurements only: proxy corpus (`docs/*.md`), no claim beyond output.

use crate::diff_logic::{Adam, DiffLogicNetwork, TrainMode};
use crate::hamming_index::{IndexStats, MultiIndexHamming};
use crate::hle2_byte_lm::{page_keys, ByteLmConfig, RouteMode};
use crate::hle3::{ctx_key, tree_node, BackoffBitCounts};
use std::collections::HashMap;

/// Sketch width in bits.
pub const SKETCH_BITS: usize = 128;
/// Local raw bytes always given to the circuit.
pub const LOCAL_BYTES: usize = 4;
/// Circuit input width.
pub const CIRCUIT_INPUTS: usize = 8 * LOCAL_BYTES + SKETCH_BITS + 8 + 1 + 8 + 8;

/// Per-stream precomputed features for one window length `L`.
#[derive(Debug, Clone)]
pub struct StreamFeatures {
    /// Stretched order-k probability per (pos, j), for each requested k.
    pub order_st: Vec<Vec<f32>>,
    /// Stretched unigram / bigram cache probabilities per (pos, j).
    pub cache_uni: Vec<f32>,
    pub cache_bi: Vec<f32>,
    /// Match prediction (byte, found4, found8) per position.
    pub matched: Vec<(u8, bool, bool)>,
    /// Sketch of the window ending before `pos`.
    pub sketch: Vec<u128>,
}

fn stretch(p: f64) -> f32 {
    let p = p.clamp(1e-4, 1.0 - 1e-4);
    (p / (1.0 - p)).ln() as f32
}

fn bit(b: u8, j: usize) -> bool {
    (b >> (7 - j)) & 1 == 1
}

fn order_keys(t: &[u8], pos: usize, j: usize, k: usize, out: &mut Vec<u64>) {
    out.clear();
    let mut f = vec![tree_node(t[pos], j)];
    out.push(ctx_key(0, &f));
    for o in 1..=k {
        f.push(if pos >= o { u64::from(t[pos - o]) } else { 256 });
        out.push(ctx_key(o as u64, &f));
    }
}

/// Order-k backoff count models fitted on `train`.
pub fn fit_orders(train: &[u8], orders: &[usize], beta: f64) -> Vec<BackoffBitCounts> {
    let mut keys = Vec::new();
    orders
        .iter()
        .map(|&k| {
            let mut m = BackoffBitCounts::new(k + 1, beta);
            for pos in 1..train.len() {
                for j in 0..8 {
                    order_keys(train, pos, j, k, &mut keys);
                    m.observe(&keys, bit(train[pos], j));
                }
            }
            m
        })
        .collect()
}

/// Held-out bits per byte of an order model alone.
pub fn order_bpc(model: &BackoffBitCounts, k: usize, dev: &[u8]) -> f64 {
    let mut keys = Vec::new();
    let mut bits = 0.0;
    for pos in 1..dev.len() {
        for j in 0..8 {
            order_keys(dev, pos, j, k, &mut keys);
            let p = model.p_one(&keys).clamp(1e-9, 1.0 - 1e-9);
            bits -= if bit(dev[pos], j) {
                p.log2()
            } else {
                (1.0 - p).log2()
            };
        }
    }
    bits / (dev.len() - 1) as f64
}

fn h(bytes: &[u8]) -> u64 {
    ctx_key(77, &bytes.iter().map(|&b| u64::from(b)).collect::<Vec<_>>())
}

/// Causal features of `t` with window `window`; order probabilities are
/// leave-one-out when `loo` (TRAIN) and plain otherwise (DEV).
pub fn stream_features(
    t: &[u8],
    window: usize,
    orders: &[(usize, &BackoffBitCounts)],
    loo: bool,
) -> StreamFeatures {
    let n = t.len();
    let mut keys = Vec::new();
    let order_st = orders
        .iter()
        .map(|&(k, m)| {
            let mut v = vec![0f32; n * 8];
            for pos in 1..n {
                for j in 0..8 {
                    order_keys(t, pos, j, k, &mut keys);
                    let b = bit(t[pos], j);
                    let p = if loo {
                        m.p_one_excluding(&keys, b)
                    } else {
                        m.p_one(&keys)
                    };
                    v[pos * 8 + j] = stretch(p);
                }
            }
            v
        })
        .collect();
    let mut cache_uni = vec![0f32; n * 8];
    let mut cache_bi = vec![0f32; n * 8];
    let mut uni = [[0u32; 2]; 256];
    let mut bi: HashMap<(u8, u64), [u32; 2]> = HashMap::new();
    let mut matched = vec![(0u8, false, false); n];
    let (mut m4, mut m8): (HashMap<u64, usize>, HashMap<u64, usize>) = Default::default();
    let mut sketch = vec![0u128; n];
    let mut buckets = vec![0u32; SKETCH_BITS];
    let bucket =
        |a: u8, b: u8| (ctx_key(5, &[u64::from(a), u64::from(b)]) % SKETCH_BITS as u64) as usize;
    for pos in 1..n {
        // Window: bytes t[pos-window .. pos).
        let mut s = 0u128;
        let thr = (window.min(pos) as u32).div_ceil(SKETCH_BITS as u32);
        for (i, &c) in buckets.iter().enumerate() {
            if c > 0 && c >= thr {
                s |= 1 << i;
            }
        }
        sketch[pos] = s;
        let prev = t[pos - 1];
        for j in 0..8 {
            let node = tree_node(t[pos], j);
            let [a0, a1] = uni[node as usize];
            cache_uni[pos * 8 + j] = stretch((f64::from(a1) + 0.5) / (f64::from(a0 + a1) + 1.0));
            let [b0, b1] = bi.get(&(prev, node)).copied().unwrap_or([0, 0]);
            cache_bi[pos * 8 + j] = stretch((f64::from(b1) + 0.5) / (f64::from(b0 + b1) + 1.0));
        }
        let lookup = |m: &HashMap<u64, usize>, l: usize| -> Option<u8> {
            if pos < l {
                return None;
            }
            m.get(&h(&t[pos - l..pos]))
                .filter(|&&q| pos - q <= window)
                .map(|&q| t[q])
        };
        let (p4, p8) = (lookup(&m4, 4), lookup(&m8, 8));
        matched[pos] = match (p8, p4) {
            (Some(b), _) => (b, true, true),
            (None, Some(b)) => (b, true, false),
            _ => (0, false, false),
        };
        // Update state with t[pos].
        if pos >= 4 {
            m4.insert(h(&t[pos - 4..pos]), pos);
        }
        if pos >= 8 {
            m8.insert(h(&t[pos - 8..pos]), pos);
        }
        let add = |c: &mut [u32; 2], b: bool, d: i32| {
            c[usize::from(b)] = (c[usize::from(b)] as i32 + d) as u32;
        };
        for j in 0..8 {
            let node = tree_node(t[pos], j);
            add(&mut uni[node as usize], bit(t[pos], j), 1);
            add(bi.entry((prev, node)).or_insert([0, 0]), bit(t[pos], j), 1);
        }
        buckets[bucket(prev, t[pos])] += 1;
        if pos >= window {
            let old = pos - window; // leaves the window [pos+1-window, pos+1)
            if old >= 1 {
                let op = t[old - 1];
                for j in 0..8 {
                    let node = tree_node(t[old], j);
                    add(&mut uni[node as usize], bit(t[old], j), -1);
                    add(
                        bi.get_mut(&(op, node)).expect("present"),
                        bit(t[old], j),
                        -1,
                    );
                }
                buckets[bucket(op, t[old])] -= 1;
            }
        }
    }
    StreamFeatures {
        order_st,
        cache_uni,
        cache_bi,
        matched,
        sketch,
    }
}

/// Which mixer inputs and circuit variant an arm uses.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmConfig {
    /// Index into `StreamFeatures::order_st`.
    pub order: usize,
    pub cache: bool,
    pub matcher: bool,
    /// Circuit outputs fed to the mixer (None = no circuit).
    pub circuit: Option<ByteLmConfig>,
    /// Circuit sees only the local bytes (sketch / match inputs zeroed).
    pub local_only: bool,
    /// Replace circuit bits by input-independent pseudo-random bits.
    pub random_bits: bool,
    pub mix_lr: f64,
}

fn match_signal(f: &StreamFeatures, t: &[u8], pos: usize, j: usize) -> (f32, f32) {
    let (b, f4, f8) = f.matched[pos];
    // Usable only while the predicted byte agrees with the known prefix.
    if !f4 || (j > 0 && (b >> (8 - j)) != (t[pos] >> (8 - j))) {
        return (0.0, 0.0);
    }
    let s = if bit(b, j) { 1.0 } else { -1.0 };
    (s, if f8 { s } else { 0.0 })
}

fn circuit_input(
    f: &StreamFeatures,
    t: &[u8],
    pos: usize,
    j: usize,
    local: bool,
    x: &mut Vec<f64>,
) {
    x.clear();
    for i in 1..=LOCAL_BYTES {
        let b = if pos >= i { t[pos - i] } else { 0 };
        (0..8).for_each(|k| x.push(f64::from((b >> k) & 1)));
    }
    let s = if local { 0 } else { f.sketch[pos] };
    (0..SKETCH_BITS).for_each(|k| x.push(((s >> k) & 1) as f64));
    let (b, f4, _) = if local {
        (0, false, false)
    } else {
        f.matched[pos]
    };
    (0..8).for_each(|k| x.push(f64::from((b >> k) & 1)));
    x.push(f64::from(f4));
    (0..8).for_each(|k| {
        x.push(if k < j {
            f64::from((t[pos] >> (7 - k)) & 1)
        } else {
            0.0
        })
    });
    (0..8).for_each(|k| x.push(f64::from(k == j)));
}

fn mixer_inputs(
    arm: &ArmConfig,
    f: &StreamFeatures,
    t: &[u8],
    pos: usize,
    j: usize,
    out: &mut Vec<f64>,
) {
    out.clear();
    out.push(1.0);
    out.push(f64::from(f.order_st[arm.order][pos * 8 + j]));
    if arm.cache {
        out.push(f64::from(f.cache_uni[pos * 8 + j]));
        out.push(f64::from(f.cache_bi[pos * 8 + j]));
    }
    if arm.matcher {
        let (a, b) = match_signal(f, t, pos, j);
        out.push(f64::from(a));
        out.push(f64::from(b));
    }
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

/// Train the arm on `train` and return held-out bits per byte on `dev`.
pub fn run_arm(
    arm: &ArmConfig,
    train: &[u8],
    ft: &StreamFeatures,
    dev: &[u8],
    fd: &StreamFeatures,
) -> f64 {
    let mut base = Vec::new();
    mixer_inputs(arm, ft, train, 1, 0, &mut base);
    let nb = base.len();
    let (index, mut nets, g) = match &arm.circuit {
        Some(c) => {
            let index = MultiIndexHamming::new(8, page_keys(train, c)).expect("8-bit keys");
            let nets: Vec<DiffLogicNetwork> = (0..index.len())
                .map(|i| {
                    DiffLogicNetwork::new(CIRCUIT_INPUTS, &c.widths, c.seed + i as u64, 0.5)
                        .expect("widths")
                })
                .collect();
            let g = nets[0].output_count();
            (Some(index), nets, g)
        }
        None => (None, Vec::new(), 0),
    };
    let route = |prev: u8| -> usize {
        index.as_ref().map_or(0, |ix| {
            ix.top_k(u64::from(prev), 1, &mut IndexStats::default())
                .expect("in range")[0]
                .1
                 .0 as usize
        })
    };
    let dim = nb + g;
    // Mixer starts as "trust the order model".
    let mut w = vec![0.0; 8 * dim];
    (0..8).for_each(|j| w[j * dim + 1] = 1.0);
    let mut wopt = Adam::new(w.len(), arm.mix_lr);
    let mut wgrad = vec![0.0; w.len()];
    let seed = arm.circuit.as_ref().map_or(0, |c| c.seed);
    let rnd = |pos: usize, j: usize, i: usize| {
        ctx_key(seed ^ 0xBEEF, &[pos as u64, j as u64, i as u64]) & 1 == 1
    };
    let (mut x, mut m) = (Vec::new(), Vec::new());
    let cfg = arm.circuit.clone();
    let batch = cfg.as_ref().map_or(64, |c| c.batch_bytes);
    // Phase 1: joint relaxed training of circuits through the mixer.
    if let Some(c) = cfg.as_ref().filter(|c| c.train_gates && !arm.random_bits) {
        let mut opts: Vec<Adam> = nets
            .iter()
            .map(|n| Adam::new(n.parameter_count(), c.learning_rate))
            .collect();
        let mut grads: Vec<Vec<f64>> = nets
            .iter()
            .map(|n| vec![0.0; n.parameter_count()])
            .collect();
        let mut touched = vec![false; nets.len()];
        for _ in 0..c.epochs {
            for pos in 1..train.len() {
                let page = route(train[pos - 1]);
                for j in 0..8 {
                    mixer_inputs(arm, ft, train, pos, j, &mut m);
                    circuit_input(ft, train, pos, j, arm.local_only, &mut x);
                    let tr = nets[page].forward(&x, TrainMode::Relaxed).expect("width");
                    let o = tr.outputs();
                    let wj = &w[j * dim..(j + 1) * dim];
                    let mut z: f64 = m.iter().zip(wj).map(|(a, b)| a * b).sum();
                    z += o
                        .iter()
                        .zip(&wj[nb..])
                        .map(|(a, b)| (2.0 * a - 1.0) * b)
                        .sum::<f64>();
                    let dz = sigmoid(z) - f64::from(u8::from(bit(train[pos], j)));
                    let gj = &mut wgrad[j * dim..(j + 1) * dim];
                    m.iter().enumerate().for_each(|(i, a)| gj[i] += dz * a);
                    o.iter()
                        .enumerate()
                        .for_each(|(i, a)| gj[nb + i] += dz * (2.0 * a - 1.0));
                    let d: Vec<f64> = wj[nb..].iter().map(|b| dz * 2.0 * b).collect();
                    nets[page].backward(&tr, &d, TrainMode::Relaxed, &mut grads[page]);
                }
                touched[page] = true;
                if pos % batch == 0 {
                    wopt.step(&mut w, &wgrad);
                    wgrad.iter_mut().for_each(|v| *v = 0.0);
                    for i in 0..nets.len() {
                        if touched[i] {
                            opts[i].step(nets[i].parameters_mut(), &grads[i]);
                            grads[i].iter_mut().for_each(|v| *v = 0.0);
                            touched[i] = false;
                        }
                    }
                }
            }
        }
    }
    // Phase 2: refit the mixer on the hard bits used at evaluation.
    let hard: Vec<_> = nets.iter().map(DiffLogicNetwork::discretize).collect();
    let hard_bits = |f: &StreamFeatures,
                     t: &[u8],
                     pos: usize,
                     j: usize,
                     x: &mut Vec<f64>,
                     out: &mut Vec<f64>| {
        if arm.circuit.is_none() {
            return;
        }
        if arm.random_bits {
            (0..g).for_each(|i| out.push(if rnd(pos, j, i) { 1.0 } else { -1.0 }));
            return;
        }
        circuit_input(f, t, pos, j, arm.local_only, x);
        let bits: Vec<bool> = x.iter().map(|&v| v > 0.5).collect();
        let page = route(t[pos - 1]);
        hard[page]
            .eval_bits(&bits)
            .expect("width")
            .iter()
            .for_each(|&b| out.push(if b { 1.0 } else { -1.0 }));
    };
    let mut wopt = Adam::new(w.len(), arm.mix_lr);
    for pos in 1..train.len() {
        for j in 0..8 {
            mixer_inputs(arm, ft, train, pos, j, &mut m);
            hard_bits(ft, train, pos, j, &mut x, &mut m);
            let wj = &w[j * dim..(j + 1) * dim];
            let z: f64 = m.iter().zip(wj).map(|(a, b)| a * b).sum();
            let dz = sigmoid(z) - f64::from(u8::from(bit(train[pos], j)));
            m.iter()
                .enumerate()
                .for_each(|(i, a)| wgrad[j * dim + i] += dz * a);
        }
        if pos % 64 == 0 {
            wopt.step(&mut w, &wgrad);
            wgrad.iter_mut().for_each(|v| *v = 0.0);
        }
    }
    let mut bits = 0.0;
    for pos in 1..dev.len() {
        for j in 0..8 {
            mixer_inputs(arm, fd, dev, pos, j, &mut m);
            hard_bits(fd, dev, pos, j, &mut x, &mut m);
            let z: f64 = m
                .iter()
                .zip(&w[j * dim..(j + 1) * dim])
                .map(|(a, b)| a * b)
                .sum();
            let p = sigmoid(z).clamp(1e-9, 1.0 - 1e-9);
            bits -= if bit(dev[pos], j) {
                p.log2()
            } else {
                (1.0 - p).log2()
            };
        }
    }
    bits / (dev.len() - 1) as f64
}

/// Default circuit config used by the HLE-4 bench.
pub fn circuit_config(seed: u64, route: RouteMode, train_gates: bool) -> ByteLmConfig {
    ByteLmConfig {
        context_bytes: LOCAL_BYTES,
        pages: 16,
        widths: vec![128, 128, 16],
        epochs: 1,
        learning_rate: 0.02,
        batch_bytes: 64,
        seed,
        route,
        train_gates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn features_are_causal_and_match_model_copies() {
        let t = b"the cat sat. the cat sat. the cat sat.".to_vec();
        let ord = fit_orders(&t, &[2], 2.0);
        let f = stream_features(&t, 64, &[(2, &ord[0])], false);
        // After one repetition the 8-byte match predicts the next byte.
        let pos = 13 + 8;
        assert_eq!(f.matched[pos], (t[pos], true, true));
        // Changing the future does not change features at `pos`.
        let mut t2 = t.clone();
        t2[pos] = b'#';
        let f2 = stream_features(&t2, 64, &[(2, &ord[0])], false);
        assert_eq!(f.matched[pos], f2.matched[pos]);
        assert_eq!(f.sketch[pos], f2.sketch[pos]);
        // Window bound: with window 4 the repetition is out of reach.
        let f3 = stream_features(&t, 4, &[(2, &ord[0])], false);
        assert!(!f3.matched[pos].1);
    }

    #[test]
    fn mixer_with_match_beats_order_model_on_repetitive_text() {
        let t: Vec<u8> = b"alpha beta gamma delta epsilon zeta eta theta. ".repeat(30);
        let (train, dev) = t.split_at(t.len() * 3 / 4);
        let ord = fit_orders(train, &[1], 2.0);
        let ft = stream_features(train, 256, &[(1, &ord[0])], true);
        let fd = stream_features(dev, 256, &[(1, &ord[0])], false);
        let arm = |matcher| ArmConfig {
            order: 0,
            cache: false,
            matcher,
            circuit: None,
            local_only: false,
            random_bits: false,
            mix_lr: 0.02,
        };
        let a = run_arm(&arm(false), train, &ft, dev, &fd);
        let b = run_arm(&arm(true), train, &ft, dev, &fd);
        assert!(b < a - 0.3, "{a} {b}");
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

    /// HLE-4 long-context table. Run with
    /// `cargo test --release -p sml-core --lib hle4_long_context_table -- --ignored --nocapture`.
    /// Env: `HLE4_TRAIN` (bytes, default 400000), `HLE4_SEEDS` (default 3).
    #[test]
    #[ignore]
    fn hle4_long_context_table() {
        let env = |k: &str, d: usize| {
            std::env::var(k)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(d)
        };
        let (ta, da) = corpus();
        let train = &ta[..ta.len().min(env("HLE4_TRAIN", 400_000))];
        let dev = &da[..da.len().min(60_000)];
        let seeds: Vec<u64> = (0..env("HLE4_SEEDS", 3) as u64)
            .map(|s| 11 + 100 * s)
            .collect();
        println!(
            "HLE4,corpus=repo-docs-md,train_bytes={},dev_bytes={},beta=2,seeds={seeds:?}",
            train.len(),
            dev.len()
        );
        let t0 = std::time::Instant::now();
        let orders = [2usize, 3, 4, 5];
        let models = fit_orders(train, &orders, 2.0);
        for (k, m) in orders.iter().zip(&models) {
            println!(
                "HLE4,baseline,order{k},dev_bpc={:.4}",
                order_bpc(m, *k, dev)
            );
        }
        let om: Vec<(usize, &BackoffBitCounts)> = vec![(2, &models[0]), (3, &models[1])];
        let plain = |order, cache, matcher| ArmConfig {
            order,
            cache,
            matcher,
            circuit: None,
            local_only: false,
            random_bits: false,
            mix_lr: 0.01,
        };
        println!("HLE4,window,arm,order,dev_bpc_mean,spread(max-min),n,elapsed_s");
        for &win in &[16usize, 64, 256, 1024] {
            let ft = stream_features(train, win, &om, true);
            let fd = stream_features(dev, win, &om, false);
            let mut arms: Vec<(String, Vec<ArmConfig>)> = Vec::new();
            for (oi, _) in om.iter().enumerate() {
                arms.push(("mix_order_only".into(), vec![plain(oi, false, false)]));
                arms.push(("mix_order+cache".into(), vec![plain(oi, true, false)]));
                arms.push(("mix_order+cache+match".into(), vec![plain(oi, true, true)]));
            }
            let circ = |oi: usize, route, train_gates, local_only, random_bits| -> Vec<ArmConfig> {
                seeds
                    .iter()
                    .map(|&s| ArmConfig {
                        circuit: Some(circuit_config(s, route, train_gates)),
                        local_only,
                        random_bits,
                        ..plain(oi, true, true)
                    })
                    .collect()
            };
            arms.push((
                "+circuit_trained".into(),
                circ(0, RouteMode::Learned, true, false, false),
            ));
            arms.push((
                "+circuit_local_only(reset)".into(),
                circ(0, RouteMode::Learned, true, true, false),
            ));
            if win == 1024 || win == 16 {
                arms.push((
                    "+circuit_untrained".into(),
                    circ(0, RouteMode::Learned, false, false, false),
                ));
                arms.push((
                    "+circuit_frozen_route".into(),
                    circ(0, RouteMode::FrozenRandom, true, false, false),
                ));
                arms.push((
                    "+random_bits".into(),
                    circ(0, RouteMode::Learned, false, false, true),
                ));
            }
            if win == 1024 {
                arms.push((
                    "+circuit_trained".into(),
                    circ(1, RouteMode::Learned, true, false, false),
                ));
            }
            for (name, cfgs) in arms {
                let r: Vec<f64> = std::thread::scope(|s| {
                    let hs: Vec<_> = cfgs
                        .iter()
                        .map(|a| s.spawn(|| run_arm(a, train, &ft, dev, &fd)))
                        .collect();
                    hs.into_iter().map(|h| h.join().unwrap()).collect()
                });
                let mean = r.iter().sum::<f64>() / r.len() as f64;
                let spread = r.iter().cloned().fold(f64::MIN, f64::max)
                    - r.iter().cloned().fold(f64::MAX, f64::min);
                println!(
                    "HLE4,{win},{name},order{},{mean:.4},{spread:.4},{},{:.0}",
                    om[cfgs[0].order].0,
                    r.len(),
                    t0.elapsed().as_secs_f64()
                );
            }
        }
    }
}
