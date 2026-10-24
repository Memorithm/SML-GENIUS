//! HLE-5 research measurements (Grok line, independent of HYB/H-1c):
//! three mechanisms meant to add *new information* to order-k counts rather
//! than re-deriving what the counts already know (the HLE-3/HLE-4 failure).
//!
//! 1. **Learned-gate slot memory** ([`SlotMemory`]): a bounded set-associative
//!    Boolean-addressed memory (`capacity` slots of `tag u32, byte u8,
//!    confidence u8, priority u8`), keyed by hashes of the last 4 and 8 bytes.
//!    State is discrete and bounded at inference; cost per byte is O(ways).
//!    Writes and evictions are decided by a logistic gate whose training path
//!    is explicit: on TRAIN only, each write candidate gets a causal
//!    *hindsight* label ("this key recurs within `horizon` bytes followed by
//!    the same byte"), and the gate is fitted by logistic regression on
//!    features known at write time. Controls with equal capacity:
//!    always-write/LRU and a write-rate-matched random gate.
//! 2. **Searched sparse bit-context heads** ([`search_groups`]): structure
//!    search over *which* past bits form a context (feature groups), seeded by
//!    a Walsh degree-1 screening of the residual of the order-3 base model, then
//!    improved by (1+1) mutations (bit replacement and sub-group reuse from
//!    already-selected heads), accepted only on a SELECT split disjoint from
//!    TRAIN and TEST. Controls: random groups, contiguous last-bytes groups.
//! 3. **Conditional mixer routing**: the logistic mixer weight set is selected
//!    by a small discrete route (bit depth x memory confidence x order-model
//!    confidence) instead of bit depth alone.
//!
//! Every predictor is bit-level (MSB-first binary decomposition per byte).
//! No attention, token softmax or KV cache. Measurements only; no claim
//! beyond the printed output of the ignored `hle5_table` test.

use crate::diff_logic::Adam;
use crate::hle3::{ctx_key, tree_node, BackoffBitCounts};
use crate::hle4::{fit_orders, stream_features, StreamFeatures};
use std::collections::HashMap;

fn bit(b: u8, j: usize) -> bool {
    (b >> (7 - j)) & 1 == 1
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

fn stretch(p: f64) -> f32 {
    let p = p.clamp(1e-4, 1.0 - 1e-4);
    (p / (1.0 - p)).ln() as f32
}

/// Small deterministic PRNG (splitmix64) so every arm is reproducible.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ---------------------------------------------------------------------------
// 1. Slot memory
// ---------------------------------------------------------------------------

/// Key lengths (bytes of context hashed into a memory address).
pub const MEM_KEYS: [usize; 2] = [8, 4];
/// Associativity.
pub const WAYS: usize = 4;
/// Bytes of state per slot: tag (4) + byte (1) + confidence (1) + priority (1).
pub const SLOT_BYTES: usize = 7;

/// Bounded set-associative memory. All state is fixed-size after `new`.
#[derive(Debug, Clone)]
pub struct SlotMemory {
    sets: usize,
    tag: Vec<u32>,
    byte: Vec<u8>,
    conf: Vec<u8>,
    prio: Vec<u8>,
    used: Vec<bool>,
    salt: u64,
}

/// One read result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MemRead {
    pub hit: bool,
    pub byte: u8,
    pub conf: u8,
}

impl SlotMemory {
    /// `capacity` is rounded down to a multiple of [`WAYS`] (minimum one set).
    pub fn new(capacity: usize, salt: u64) -> Self {
        let sets = (capacity / WAYS).max(1);
        let n = sets * WAYS;
        Self {
            sets,
            tag: vec![0; n],
            byte: vec![0; n],
            conf: vec![0; n],
            prio: vec![0; n],
            used: vec![false; n],
            salt,
        }
    }

    pub fn capacity(&self) -> usize {
        self.sets * WAYS
    }

    /// Bytes of model state.
    pub fn state_bytes(&self) -> usize {
        self.capacity() * SLOT_BYTES
    }

    fn addr(&self, key: u64) -> (usize, u32) {
        let h = ctx_key(self.salt, &[key]);
        ((h % self.sets as u64) as usize, (h >> 32) as u32)
    }

    fn find(&self, key: u64) -> Option<usize> {
        let (s, tag) = self.addr(key);
        (s * WAYS..(s + 1) * WAYS).find(|&i| self.used[i] && self.tag[i] == tag)
    }

    pub fn read(&self, key: u64) -> MemRead {
        self.find(key).map_or(MemRead::default(), |i| MemRead {
            hit: true,
            byte: self.byte[i],
            conf: self.conf[i],
        })
    }

    /// Reveal the byte that followed `key`. A hit updates confidence (and
    /// overwrites the stored byte on a miss-prediction only when `write`).
    /// A miss inserts only when `write`, evicting the lowest-priority way.
    /// `prio` is the insertion priority (larger = kept longer); `stamp` is
    /// folded in so ties evict the oldest.
    pub fn update(&mut self, key: u64, actual: u8, write: bool, prio: u8) {
        if let Some(i) = self.find(key) {
            if self.byte[i] == actual {
                self.conf[i] = (self.conf[i] + 1).min(15);
                self.prio[i] = self.prio[i].saturating_add(1);
            } else if write {
                self.byte[i] = actual;
                self.conf[i] = 0;
                self.prio[i] = prio;
            } else {
                self.conf[i] /= 2;
            }
            return;
        }
        if !write {
            return;
        }
        let (s, tag) = self.addr(key);
        let range = s * WAYS..(s + 1) * WAYS;
        let victim = range
            .clone()
            .find(|&i| !self.used[i])
            .unwrap_or_else(|| range.min_by_key(|&i| self.prio[i]).expect("ways > 0"));
        // Ageing: every insertion into a set decays the survivors so that
        // stale high-priority entries can eventually be evicted.
        for i in s * WAYS..(s + 1) * WAYS {
            self.prio[i] = self.prio[i].saturating_sub(1);
        }
        self.used[victim] = true;
        self.tag[victim] = tag;
        self.byte[victim] = actual;
        self.conf[victim] = 0;
        self.prio[victim] = prio;
    }
}

/// How the memory decides to write.
#[derive(Debug, Clone, PartialEq)]
pub enum WritePolicy {
    /// Write every candidate; constant priority (LRU-like via ageing).
    Always,
    /// Write with probability `rate`, independent of the input.
    Random { rate: f64, seed: u64 },
    /// Learned logistic gate over [`gate_features`].
    Learned(Vec<f64>),
}

/// Number of gate features.
pub const GATE_DIM: usize = 1 + 4 + 2 + 4 + 2;

fn byte_class(b: u8) -> usize {
    match b {
        b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' => 0,
        b' ' => 1,
        b'\n' | b'\r' | b'\t' => 2,
        _ => 3,
    }
}

fn surprise_bits(f: &StreamFeatures, t: &[u8], pos: usize) -> f64 {
    (0..8)
        .map(|j| {
            let p = sigmoid(f64::from(f.order_st[0][pos * 8 + j])).clamp(1e-6, 1.0 - 1e-6);
            if bit(t[pos], j) {
                -p.log2()
            } else {
                -(1.0 - p).log2()
            }
        })
        .sum()
}

/// Features of a write candidate known at write time (after `t[pos]` is
/// revealed): bias, order-model surprise bucket, key length, byte class,
/// whether the memory already predicted `t[pos]` for this key.
pub fn gate_features(surprise: f64, ki: usize, actual: u8, mem_ok: bool) -> [f64; GATE_DIM] {
    let mut x = [0.0; GATE_DIM];
    x[0] = 1.0;
    let sb = if surprise < 1.0 {
        0
    } else if surprise < 3.0 {
        1
    } else if surprise < 6.0 {
        2
    } else {
        3
    };
    x[1 + sb] = 1.0;
    x[5 + ki] = 1.0;
    x[7 + byte_class(actual)] = 1.0;
    x[11 + usize::from(mem_ok)] = 1.0;
    x
}

fn key_at(t: &[u8], pos: usize, k: usize) -> Option<u64> {
    (pos >= k).then(|| {
        ctx_key(
            0x5107 + k as u64,
            &t[pos - k..pos]
                .iter()
                .map(|&b| u64::from(b))
                .collect::<Vec<_>>(),
        )
    })
}

/// Hindsight label for TRAIN: does `(key_k(pos), t[pos])` recur within
/// `(pos, pos + horizon]`? Only ever computed on TRAIN.
pub fn hindsight_labels(t: &[u8], horizon: usize) -> Vec<[bool; 2]> {
    let mut out = vec![[false; 2]; t.len()];
    for (ki, &k) in MEM_KEYS.iter().enumerate() {
        let mut next: HashMap<(u64, u8), usize> = HashMap::new();
        for pos in (0..t.len()).rev() {
            if let Some(key) = key_at(t, pos, k) {
                if let Some(&q) = next.get(&(key, t[pos])) {
                    out[pos][ki] = q - pos <= horizon;
                }
                next.insert((key, t[pos]), pos);
            }
        }
    }
    out
}

/// Fit the write gate on TRAIN by logistic regression on hindsight labels.
/// Returns the weights and the gate's empirical write rate on TRAIN.
pub fn fit_write_gate(
    t: &[u8],
    f: &StreamFeatures,
    horizon: usize,
    epochs: usize,
) -> (Vec<f64>, f64) {
    let labels = hindsight_labels(t, horizon);
    // Class-balanced logistic loss: positives are rare, and an unweighted
    // fit collapses to "never write".
    let (mut pos_n, mut all_n) = (0usize, 0usize);
    for l in labels.iter().skip(1) {
        pos_n += usize::from(l[0]) + usize::from(l[1]);
        all_n += 2;
    }
    let pw = (all_n - pos_n) as f64 / pos_n.max(1) as f64;
    let mut w = vec![0.0; GATE_DIM];
    let mut opt = Adam::new(GATE_DIM, 0.02);
    let mut g = vec![0.0; GATE_DIM];
    // Gate features use a non-learned shadow memory for `mem_ok` so that the
    // feature distribution matches inference without circularity.
    for _ in 0..epochs {
        let mut shadow = SlotMemory::new(4096, 1);
        for pos in 1..t.len() {
            let s = surprise_bits(f, t, pos);
            for (ki, &k) in MEM_KEYS.iter().enumerate() {
                let Some(key) = key_at(t, pos, k) else {
                    continue;
                };
                let r = shadow.read(key);
                let x = gate_features(s, ki, t[pos], r.hit && r.byte == t[pos]);
                let z: f64 = x.iter().zip(&w).map(|(a, b)| a * b).sum();
                let y = labels[pos][ki];
                let d = (sigmoid(z) - f64::from(u8::from(y))) * if y { pw } else { 1.0 };
                g.iter_mut().zip(&x).for_each(|(gi, xi)| *gi += d * xi);
                shadow.update(key, t[pos], true, 128);
            }
            if pos % 256 == 0 {
                opt.step(&mut w, &g);
                g.iter_mut().for_each(|v| *v = 0.0);
            }
        }
    }
    let mut writes = (0usize, 0usize);
    let mut shadow = SlotMemory::new(4096, 1);
    for pos in 1..t.len() {
        let s = surprise_bits(f, t, pos);
        for (ki, &k) in MEM_KEYS.iter().enumerate() {
            if let Some(key) = key_at(t, pos, k) {
                let r = shadow.read(key);
                let x = gate_features(s, ki, t[pos], r.hit && r.byte == t[pos]);
                let z: f64 = x.iter().zip(&w).map(|(a, b)| a * b).sum();
                writes.0 += usize::from(z > 0.0);
                writes.1 += 1;
                shadow.update(key, t[pos], true, 128);
            }
        }
    }
    (w, writes.0 as f64 / writes.1.max(1) as f64)
}

/// Per-position memory reads (before `t[pos]` is revealed), one per key.
pub type MemTrace = Vec<[MemRead; 2]>;

/// Run the memory causally over `t` (fresh state) and return the reads.
pub fn memory_trace(
    t: &[u8],
    f: &StreamFeatures,
    capacity: usize,
    salt: u64,
    policy: &WritePolicy,
) -> MemTrace {
    let mut mem = SlotMemory::new(capacity, salt);
    let mut rng = match policy {
        WritePolicy::Random { seed, .. } => Rng::new(*seed),
        _ => Rng::new(0),
    };
    let mut out = vec![[MemRead::default(); 2]; t.len()];
    for pos in 1..t.len() {
        let keys: Vec<Option<u64>> = MEM_KEYS.iter().map(|&k| key_at(t, pos, k)).collect();
        for (ki, key) in keys.iter().enumerate() {
            if let Some(key) = key {
                out[pos][ki] = mem.read(*key);
            }
        }
        let s = match policy {
            WritePolicy::Learned(_) => surprise_bits(f, t, pos),
            _ => 0.0,
        };
        for (ki, key) in keys.iter().enumerate() {
            let Some(key) = *key else { continue };
            let r = out[pos][ki];
            let (write, prio) = match policy {
                WritePolicy::Always => (true, 128),
                WritePolicy::Random { rate, .. } => (rng.unit() < *rate, 128),
                WritePolicy::Learned(w) => {
                    let x = gate_features(s, ki, t[pos], r.hit && r.byte == t[pos]);
                    let p = sigmoid(x.iter().zip(w).map(|(a, b)| a * b).sum());
                    (p > 0.5, (p * 255.0) as u8)
                }
            };
            mem.update(key, t[pos], write, prio);
        }
    }
    out
}

fn conf_bucket(c: u8) -> usize {
    match c {
        0 => 0,
        1..=2 => 1,
        3..=6 => 2,
        _ => 3,
    }
}

/// Memory mixer inputs: for each key, a signed vote placed in the slot of
/// its confidence bucket (so the mixer learns how far to trust each level).
fn mem_inputs(tr: &MemTrace, t: &[u8], pos: usize, j: usize, out: &mut Vec<f64>) {
    for r in &tr[pos] {
        let mut v = [0.0; 4];
        if r.hit && (j == 0 || (r.byte >> (8 - j)) == (t[pos] >> (8 - j))) {
            v[conf_bucket(r.conf)] = if bit(r.byte, j) { 1.0 } else { -1.0 };
        }
        out.extend_from_slice(&v);
    }
}

// ---------------------------------------------------------------------------
// 2. Searched sparse bit-context heads
// ---------------------------------------------------------------------------

/// Past bytes visible to bit-context heads.
pub const POOL_BYTES: usize = 12;
/// Candidate pool size (bit `b` of byte `t[pos-1-d]` is index `8d + b`).
pub const POOL: usize = 8 * POOL_BYTES;

fn pool_bit(t: &[u8], pos: usize, idx: usize) -> u64 {
    let d = idx / 8 + 1;
    if pos < d {
        0
    } else {
        u64::from((t[pos - d] >> (idx % 8)) & 1)
    }
}

fn group_value(t: &[u8], pos: usize, g: &[u8]) -> u64 {
    g.iter()
        .fold(0u64, |a, &i| (a << 1) | pool_bit(t, pos, usize::from(i)))
}

fn head_keys(t: &[u8], pos: usize, j: usize, gid: u64, g: &[u8], out: &mut Vec<u64>) {
    out.clear();
    let node = tree_node(t[pos], j);
    out.push(ctx_key(0x4E00 + gid, &[node]));
    out.push(ctx_key(0x4E80 + gid, &[node, group_value(t, pos, g)]));
}

/// Fit a bit-context head (counts keyed on `(node, group value)`).
pub fn fit_head(train: &[u8], gid: u64, g: &[u8]) -> BackoffBitCounts {
    let mut m = BackoffBitCounts::new(2, 2.0);
    let mut k = Vec::new();
    for pos in 1..train.len() {
        for j in 0..8 {
            head_keys(train, pos, j, gid, g, &mut k);
            m.observe(&k, bit(train[pos], j));
        }
    }
    m
}

/// Stretched head probabilities per (pos, j); leave-one-out when `loo`.
pub fn head_stream(t: &[u8], m: &BackoffBitCounts, gid: u64, g: &[u8], loo: bool) -> Vec<f32> {
    let mut v = vec![0f32; t.len() * 8];
    let mut k = Vec::new();
    for pos in 1..t.len() {
        for j in 0..8 {
            head_keys(t, pos, j, gid, g, &mut k);
            let b = bit(t[pos], j);
            v[pos * 8 + j] = stretch(if loo {
                m.p_one_excluding(&k, b)
            } else {
                m.p_one(&k)
            });
        }
    }
    v
}

/// Equal-weight logit average of `streams` (plus the base order stream) scored
/// in bits per byte on `t` — the cheap selection objective.
fn ensemble_bpc(t: &[u8], streams: &[&[f32]]) -> f64 {
    let mut bits = 0.0;
    for pos in 1..t.len() {
        for j in 0..8 {
            let z: f64 = streams
                .iter()
                .map(|s| f64::from(s[pos * 8 + j]))
                .sum::<f64>()
                / streams.len() as f64;
            let p = sigmoid(z).clamp(1e-9, 1.0 - 1e-9);
            bits -= if bit(t[pos], j) {
                p.log2()
            } else {
                (1.0 - p).log2()
            };
        }
    }
    bits / (t.len() - 1) as f64
}

/// Walsh degree-1 screening: |E[(2y-1)(2x_i-1) * w]| per pool bit, where
/// the target is the residual sign of the order-2 model (y = bit, weight =
/// |y - p|). These are the degree-1 Walsh coefficients of the residual.
pub fn walsh_screen(t: &[u8], f: &StreamFeatures, stride: usize) -> Vec<f64> {
    let mut c = vec![0.0; POOL];
    let mut n = 0.0;
    for pos in (1..t.len()).step_by(stride.max(1)) {
        for j in 0..8 {
            let p = sigmoid(f64::from(f.order_st[0][pos * 8 + j]));
            let y = f64::from(u8::from(bit(t[pos], j)));
            let r = y - p;
            for (i, ci) in c.iter_mut().enumerate() {
                let x = 2.0 * pool_bit(t, pos, i) as f64 - 1.0;
                *ci += r * x;
            }
            n += 1.0;
        }
    }
    c.iter().map(|v| (v / n).abs()).collect()
}

/// How groups are chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMode {
    /// Contiguous high bits of the last bytes (an n-gram-like control).
    Contiguous,
    /// Uniformly random distinct pool bits (control).
    Random,
    /// Walsh-screened seed + (1+1) mutation search on SELECT.
    Searched,
}

/// Search statistics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchStats {
    pub evaluations: usize,
    pub accepted: usize,
    pub select_bpc: f64,
}

/// Choose `heads` groups of `width` pool bits. Only `fit` (a TRAIN prefix)
/// and `select` (disjoint from TRAIN and TEST) are read.
#[allow(clippy::too_many_arguments)]
pub fn search_groups(
    mode: GroupMode,
    fit: &[u8],
    select: &[u8],
    select_base: &[f32],
    screen: &[f64],
    heads: usize,
    width: usize,
    iters: usize,
    seed: u64,
) -> (Vec<Vec<u8>>, SearchStats) {
    let mut rng = Rng::new(seed);
    let mut stats = SearchStats::default();
    let mut groups: Vec<Vec<u8>> = Vec::new();
    let mut chosen_streams: Vec<Vec<f32>> = Vec::new();
    let eval = |g: &[u8], gid: u64| -> Vec<f32> {
        let m = fit_head(fit, gid, g);
        head_stream(select, &m, gid, g, false)
    };
    for h in 0..heads {
        let gid = h as u64;
        let mut g: Vec<u8> = match mode {
            GroupMode::Contiguous => (0..width)
                .map(|i| {
                    // byte i/8 back, high bits first within the byte
                    let d = (i / 8) as u8;
                    8 * (d + h as u8) + (7 - (i % 8) as u8)
                })
                .collect(),
            GroupMode::Random | GroupMode::Searched => {
                let mut g = Vec::new();
                while g.len() < width {
                    let i = if mode == GroupMode::Searched && h == 0 && g.len() < width / 2 {
                        // Seed from the Walsh screen (top bits, not yet used).
                        let mut order: Vec<usize> = (0..POOL).collect();
                        order.sort_by(|&a, &b| screen[b].total_cmp(&screen[a]));
                        order[g.len()] as u8
                    } else {
                        rng.below(POOL) as u8
                    };
                    if !g.contains(&i) {
                        g.push(i);
                    }
                }
                g
            }
        };
        if mode == GroupMode::Searched {
            let score = |s: &[f32], chosen: &[Vec<f32>]| {
                let mut all: Vec<&[f32]> = vec![select_base];
                all.extend(chosen.iter().map(|c| c.as_slice()));
                all.push(s);
                ensemble_bpc(select, &all)
            };
            let mut cur = eval(&g, gid);
            let mut best = score(&cur, &chosen_streams);
            stats.evaluations += 1;
            for _ in 0..iters {
                let mut cand = g.clone();
                let r = rng.unit();
                if r < 0.25 && !groups.is_empty() {
                    // Sub-expression reuse: copy a pair of bits from an
                    // already-selected head.
                    let src = &groups[rng.below(groups.len())];
                    for _ in 0..2 {
                        let b = src[rng.below(src.len())];
                        if !cand.contains(&b) {
                            let slot = rng.below(cand.len());
                            cand[slot] = b;
                        }
                    }
                } else {
                    let flips = if r < 0.7 { 1 } else { 2 };
                    for _ in 0..flips {
                        let b = rng.below(POOL) as u8;
                        if !cand.contains(&b) {
                            let slot = rng.below(cand.len());
                            cand[slot] = b;
                        }
                    }
                }
                if cand == g {
                    continue;
                }
                let s = eval(&cand, gid);
                let sc = score(&s, &chosen_streams);
                stats.evaluations += 1;
                if sc < best {
                    best = sc;
                    g = cand;
                    cur = s;
                    stats.accepted += 1;
                }
            }
            stats.select_bpc = best;
            chosen_streams.push(cur);
        }
        groups.push(g);
    }
    (groups, stats)
}

// ---------------------------------------------------------------------------
// 3. Mixer with conditional routing
// ---------------------------------------------------------------------------

/// One arm of the HLE-5 table.
#[derive(Debug, Clone, PartialEq)]
pub struct Arm5 {
    pub cache: bool,
    pub matcher: bool,
    pub memory: bool,
    pub heads: usize,
    pub routed: bool,
    pub mix_lr: f64,
}

/// Inputs for one stream (TRAIN with LOO/causal features, or TEST).
pub struct Stream5<'a> {
    pub t: &'a [u8],
    pub f: &'a StreamFeatures,
    pub mem: Option<&'a MemTrace>,
    pub heads: Vec<&'a [f32]>,
}

/// Number of mixer routes when routed.
pub const ROUTES: usize = 8 * 5 * 4;

fn route(arm: &Arm5, s: &Stream5, pos: usize, j: usize) -> usize {
    if !arm.routed {
        return j;
    }
    let mc = s.mem.map_or(0, |m| {
        m[pos]
            .iter()
            .find(|r| r.hit)
            .map_or(0, |r| 1 + conf_bucket(r.conf))
    });
    let a = f64::from(s.f.order_st[0][pos * 8 + j]).abs();
    let oc = if a < 1.0 {
        0
    } else if a < 2.5 {
        1
    } else if a < 5.0 {
        2
    } else {
        3
    };
    (j * 5 + mc) * 4 + oc
}

fn inputs(arm: &Arm5, s: &Stream5, pos: usize, j: usize, out: &mut Vec<f64>) {
    out.clear();
    out.push(1.0);
    out.push(f64::from(s.f.order_st[0][pos * 8 + j]));
    out.push(f64::from(s.f.order_st[1][pos * 8 + j]));
    if arm.cache {
        out.push(f64::from(s.f.cache_uni[pos * 8 + j]));
        out.push(f64::from(s.f.cache_bi[pos * 8 + j]));
    }
    if arm.matcher {
        let (b, f4, f8) = s.f.matched[pos];
        let ok = f4 && (j == 0 || (b >> (8 - j)) == (s.t[pos] >> (8 - j)));
        let v = if bit(b, j) { 1.0 } else { -1.0 };
        out.push(if ok { v } else { 0.0 });
        out.push(if ok && f8 { v } else { 0.0 });
    }
    if arm.memory {
        if let Some(m) = s.mem {
            mem_inputs(m, s.t, pos, j, out);
        }
    }
    for h in s.heads.iter().take(arm.heads) {
        out.push(f64::from(h[pos * 8 + j]));
    }
}

/// Result of one arm.
#[derive(Debug, Clone, PartialEq)]
pub struct ArmResult {
    pub test_bpc: f64,
    pub mixer_params: usize,
    pub train_s: f64,
    pub eval_ns_per_byte: f64,
}

/// Train the mixer on `train` (one pass, Adam) and score `test`.
pub fn run_arm5(arm: &Arm5, train: &Stream5, test: &Stream5) -> ArmResult {
    let t0 = std::time::Instant::now();
    let mut x = Vec::new();
    inputs(arm, train, 1, 0, &mut x);
    let dim = x.len();
    let routes = if arm.routed { ROUTES } else { 8 };
    let mut w = vec![0.0; routes * dim];
    (0..routes).for_each(|r| w[r * dim + 1] = 1.0);
    let mut opt = Adam::new(w.len(), arm.mix_lr);
    let mut g = vec![0.0; w.len()];
    for pos in 1..train.t.len() {
        for j in 0..8 {
            inputs(arm, train, pos, j, &mut x);
            let r = route(arm, train, pos, j);
            let wr = &w[r * dim..(r + 1) * dim];
            let z: f64 = x.iter().zip(wr).map(|(a, b)| a * b).sum();
            let d = sigmoid(z) - f64::from(u8::from(bit(train.t[pos], j)));
            x.iter()
                .enumerate()
                .for_each(|(i, a)| g[r * dim + i] += d * a);
        }
        if pos % 64 == 0 {
            opt.step(&mut w, &g);
            g.iter_mut().for_each(|v| *v = 0.0);
        }
    }
    let train_s = t0.elapsed().as_secs_f64();
    let t1 = std::time::Instant::now();
    let mut bits = 0.0;
    for pos in 1..test.t.len() {
        for j in 0..8 {
            inputs(arm, test, pos, j, &mut x);
            let r = route(arm, test, pos, j);
            let z: f64 = x
                .iter()
                .zip(&w[r * dim..(r + 1) * dim])
                .map(|(a, b)| a * b)
                .sum();
            let p = sigmoid(z).clamp(1e-9, 1.0 - 1e-9);
            bits -= if bit(test.t[pos], j) {
                p.log2()
            } else {
                (1.0 - p).log2()
            };
        }
    }
    ArmResult {
        test_bpc: bits / (test.t.len() - 1) as f64,
        mixer_params: w.len(),
        train_s,
        eval_ns_per_byte: t1.elapsed().as_nanos() as f64 / test.t.len() as f64,
    }
}

/// Split documents into TRAIN / SELECT / TEST by a hash of the file name
/// (independent of the HLE-4 `i % 10` split and of DEV S5h).
pub fn split_docs(docs: &[(String, Vec<u8>)]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let (mut a, mut b, mut c) = (Vec::new(), Vec::new(), Vec::new());
    for (name, bytes) in docs {
        let h = ctx_key(0x4C35, &name.bytes().map(u64::from).collect::<Vec<_>>()) % 10;
        match h {
            0 => c.extend_from_slice(bytes),
            1 => b.extend_from_slice(bytes),
            _ => a.extend_from_slice(bytes),
        }
    }
    (a, b, c)
}

/// Convenience: order-3/4 base features for a stream.
pub fn base_features(
    t: &[u8],
    models: &[BackoffBitCounts],
    window: usize,
    loo: bool,
) -> StreamFeatures {
    stream_features(
        t,
        window,
        &[(BASE_ORDERS[0], &models[0]), (BASE_ORDERS[1], &models[1])],
        loo,
    )
}

/// Orders of the two count models fed to every mixer arm.
pub const BASE_ORDERS: [usize; 2] = [3, 4];

/// Fit the order-3 and order-4 models used by every arm.
pub fn fit_base(train: &[u8]) -> Vec<BackoffBitCounts> {
    fit_orders(train, &BASE_ORDERS, 2.0)
}

#[cfg(test)]
mod tests;
