//! HLE-P1: a streaming, memory-bounded bit-level predictor for the S5h
//! protocol (`docs/HLE-P1-S5H-PREREGISTRATION.md`).
//!
//! Everything is prequential and one pass: each TRAIN document is scored and
//! then learned from, one byte at a time; DEV is scored with every parameter
//! frozen. Documents are fed one at a time, so memory is bounded by the
//! tables, never by the corpus. Context resets at each document and only the
//! bytes at offsets `2..len` are scored, as in GB-11 P1.
//!
//! Arms (see [`ArmSpec`]):
//! * hashed order-0..k backoff counts (Dirichlet interpolation, `beta = 2`);
//! * the same plus a sliding-window cache and an in-document match model,
//!   combined by a logistic mixer per tree depth;
//! * plus routed differentiable-logic circuits trained straight-through
//!   (hard forward) on the mixer's residual gradient;
//! * plus a recurrent Boolean workspace trained by truncated BPTT;
//! * controls: untrained circuits, context-free routing, random bits, and
//!   a workspace reset at every byte.
//!
//! Research only. Numbers it prints are measurements of this code, nothing
//! more.

use crate::diff_logic::{Adam, DiffLogicNetwork, ForwardTrace, TrainMode};

/// q4-equal witness state budget of GB-11 P1, in bytes.
pub const Q4_BUDGET_BYTES: usize = 25_000_448;
/// Dirichlet backoff strength.
pub const BETA: f64 = 2.0;
/// First scored offset in every document.
pub const FIRST_SCORED: usize = 2;
/// Sliding cache window, bytes.
pub const CACHE_WINDOW: usize = 1024;
/// Bytes hashed to find a match candidate.
pub const MATCH_MIN: usize = 6;
const MATCH_SLOTS_LOG2: u32 = 19;
/// Circuit pages (routed by the previous byte).
pub const PAGES: usize = 16;
/// Circuit outputs per tree depth.
pub const OUT_PER_DEPTH: usize = 4;
/// Recurrent workspace width, bits.
pub const STATE_BITS: usize = 32;
const CIRCUIT_BASE_INPUTS: usize = 32 + 8 + 1 + 64;
const CIRCUIT_WIDTHS: [usize; 3] = [256, 256, 8 * OUT_PER_DEPTH];
const RECURRENT_WIDTHS: [usize; 2] = [128, STATE_BITS];
const CIRCUIT_LR: f64 = 0.01;
const CIRCUIT_BATCH: usize = 16;
const MIXER_LR: f64 = 0.002;
const STRETCH_CLAMP: f64 = 8.0;

/// What the circuit part of an arm does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Trained circuits, context routing.
    Trained,
    /// Circuits stay at their seeded initialisation.
    Untrained,
    /// Trained, but routed by position, not by context.
    FrozenRouting,
    /// Circuit outputs replaced by seeded pseudo-random bits.
    RandomBits,
    /// Recurrent arm with its workspace cleared before every byte.
    StateReset,
}

/// Recurrent workspace settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recurrent {
    /// Truncated-BPTT horizon in bytes (`>= 1`).
    pub tbptt: usize,
}

/// Circuit settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CircuitSpec {
    pub control: Control,
    pub recurrent: Option<Recurrent>,
}

/// One preregistered arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmSpec {
    pub name: String,
    /// Highest context order (bytes) of the backoff counts.
    pub max_order: usize,
    /// Cache + match memory and the mixer.
    pub memory: bool,
    pub circuit: Option<CircuitSpec>,
    pub seed: u64,
    /// Total state budget in bytes; the count table gets what is left.
    pub budget_bytes: usize,
}

/// The preregistered arm list for one seed set.
pub fn preregistered_arms(seeds: &[u64], budget_bytes: usize) -> Vec<ArmSpec> {
    let mut arms = Vec::new();
    let base = |name: &str, max_order, memory, circuit, seed| ArmSpec {
        name: name.to_string(),
        max_order,
        memory,
        circuit,
        seed,
        budget_bytes,
    };
    for k in 2..=6 {
        arms.push(base(&format!("order{k}"), k, false, None, 0));
    }
    arms.push(base("order5_memory", 5, true, None, 0));
    let circuit = |control, recurrent| Some(CircuitSpec { control, recurrent });
    let rec = Some(Recurrent { tbptt: 4 });
    let circuit_arms = [
        ("residual_ste", circuit(Control::Trained, None)),
        ("residual_untrained", circuit(Control::Untrained, None)),
        (
            "residual_frozen_routing",
            circuit(Control::FrozenRouting, None),
        ),
        ("residual_random_bits", circuit(Control::RandomBits, None)),
        ("recurrent_tbptt4", circuit(Control::Trained, rec)),
        ("recurrent_state_reset", circuit(Control::StateReset, rec)),
    ];
    for &seed in seeds {
        for (name, c) in circuit_arms {
            arms.push(base(&format!("{name}_s{seed}"), 5, true, c, seed));
        }
    }
    arms
}

fn mix64(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn stretch(p: f64) -> f64 {
    let p = p.clamp(1e-6, 1.0 - 1e-6);
    (p / (1.0 - p)).ln().clamp(-STRETCH_CLAMP, STRETCH_CLAMP)
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// Hashed `[n0, n1]` counters packed in `u32` (16 bits each, halved on
/// overflow). Collisions are not detected: that is the memory budget.
struct CountTable {
    slots: Vec<u32>,
    modulus: u64,
}

impl CountTable {
    fn with_bytes(bytes: usize) -> Self {
        let n = (bytes / 4).max(1024);
        Self {
            slots: vec![0; n],
            modulus: n as u64,
        }
    }
    fn bytes(&self) -> usize {
        self.slots.len() * 4
    }
    fn get(&self, key: u64) -> (f64, f64) {
        let v = self.slots[(key % self.modulus) as usize];
        (f64::from(v & 0xffff), f64::from(v >> 16))
    }
    fn add(&mut self, key: u64, bit: bool) {
        let s = &mut self.slots[(key % self.modulus) as usize];
        let (mut n0, mut n1) = (*s & 0xffff, *s >> 16);
        if bit {
            n1 += 1;
        } else {
            n0 += 1;
        }
        if n0 == 0xffff || n1 == 0xffff {
            n0 >>= 1;
            n1 >>= 1;
        }
        *s = n0 | (n1 << 16);
    }
}

/// Byte-tree counts over a sliding window (`node` = prefix with leading 1).
struct Cache {
    node: [u32; 512],
    ring: Vec<u8>,
}

impl Cache {
    fn new() -> Self {
        Self {
            node: [0; 512],
            ring: Vec::with_capacity(CACHE_WINDOW),
        }
    }
    fn reset(&mut self) {
        self.node = [0; 512];
        self.ring.clear();
    }
    fn walk(&mut self, byte: u8, add: bool) {
        let mut n = 1usize;
        for j in 0..8 {
            n = 2 * n + usize::from((byte >> (7 - j)) & 1);
            if add {
                self.node[n] += 1;
            } else {
                self.node[n] -= 1;
            }
        }
    }
    fn push(&mut self, pos: usize, byte: u8) {
        if self.ring.len() == CACHE_WINDOW {
            let old = self.ring[pos % CACHE_WINDOW];
            self.walk(old, false);
            self.ring[pos % CACHE_WINDOW] = byte;
        } else {
            self.ring.push(byte);
        }
        self.walk(byte, true);
    }
    /// P(next bit = 1 | prefix node) from the window.
    fn p_one(&self, node: usize) -> f64 {
        let c0 = f64::from(self.node[2 * node]);
        let c1 = f64::from(self.node[2 * node + 1]);
        (c1 + 0.4) / (c0 + c1 + 0.8)
    }
}

/// In-document match model: last position of each `MATCH_MIN`-byte hash.
struct Matcher {
    table: Vec<u32>,
    generation: Vec<u32>,
    gen: u32,
    ptr: usize,
    len: usize,
}

impl Matcher {
    fn new() -> Self {
        let n = 1usize << MATCH_SLOTS_LOG2;
        Self {
            table: vec![0; n],
            generation: vec![0; n],
            gen: 0,
            ptr: 0,
            len: 0,
        }
    }
    fn bytes(&self) -> usize {
        self.table.len() * 8
    }
    fn reset(&mut self) {
        self.gen = self.gen.wrapping_add(1);
        self.ptr = 0;
        self.len = 0;
    }
    fn key(doc: &[u8], end: usize) -> usize {
        let mut h = 0u64;
        for &b in &doc[end - MATCH_MIN..end] {
            h = mix64(h ^ u64::from(b));
        }
        (h >> (64 - MATCH_SLOTS_LOG2)) as usize
    }
    /// Predicted next byte and match length before byte `pos`.
    fn predicted(&self, doc: &[u8]) -> Option<(u8, usize)> {
        (self.len > 0).then(|| (doc[self.ptr], self.len))
    }
    /// Call after byte `pos` is known (`doc[..=pos]`).
    fn update(&mut self, doc: &[u8], pos: usize) {
        if self.len > 0 && doc[self.ptr] == doc[pos] {
            self.len += 1;
            self.ptr += 1;
        } else {
            self.len = 0;
        }
        let end = pos + 1;
        if end >= MATCH_MIN {
            let k = Self::key(doc, end);
            if self.len == 0 && self.generation[k] == self.gen {
                let cand = self.table[k] as usize;
                if cand < end && doc[cand - MATCH_MIN..cand] == doc[end - MATCH_MIN..end] {
                    self.ptr = cand;
                    self.len = 1;
                }
            }
            self.table[k] = end as u32;
            self.generation[k] = self.gen;
        }
        if self.ptr >= end {
            self.len = 0;
        }
    }
}

struct Circuits {
    nets: Vec<DiffLogicNetwork>,
    adams: Vec<Adam>,
    grads: Vec<Vec<f64>>,
    pending: Vec<usize>,
    rnn: Option<(DiffLogicNetwork, Adam, Vec<f64>, usize)>,
}

/// Per-byte memory of one forward pass, for the backward pass.
struct ByteTrace {
    page: usize,
    trace: ForwardTrace,
    rnn_traces: Vec<ForwardTrace>,
}

/// A streaming model for one arm.
pub struct Model {
    spec: ArmSpec,
    counts: CountTable,
    cache: Cache,
    matcher: Matcher,
    circuits: Option<Circuits>,
    mixer: Vec<f64>,
    inputs_per_depth: usize,
    state: Vec<f64>,
    rnn_hist: Vec<ForwardTrace>,
    rng: u64,
    pub frozen: bool,
}

/// Running totals for one split.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tally {
    pub files: u64,
    pub bytes: u64,
    pub events: u64,
    pub bits: f64,
}

impl Tally {
    pub fn bpc(&self) -> f64 {
        if self.events == 0 {
            f64::NAN
        } else {
            self.bits / self.events as f64
        }
    }
}

impl Model {
    pub fn new(spec: ArmSpec) -> Self {
        let levels = spec.max_order + 1;
        let circuits = spec.circuit.map(|c| {
            let inputs = CIRCUIT_BASE_INPUTS + c.recurrent.map_or(0, |_| STATE_BITS);
            let nets: Vec<_> = (0..PAGES)
                .map(|p| {
                    DiffLogicNetwork::new(inputs, &CIRCUIT_WIDTHS, spec.seed * 1000 + p as u64, 0.5)
                        .expect("valid circuit widths")
                })
                .collect();
            let n = nets[0].parameter_count();
            let rnn = c.recurrent.map(|r| {
                let net = DiffLogicNetwork::new(
                    STATE_BITS + 16,
                    &RECURRENT_WIDTHS,
                    spec.seed * 1000 + 999,
                    0.5,
                )
                .expect("valid recurrent widths");
                let m = net.parameter_count();
                (net, Adam::new(m, CIRCUIT_LR), vec![0.0; m], r.tbptt.max(1))
            });
            Circuits {
                adams: (0..PAGES).map(|_| Adam::new(n, CIRCUIT_LR)).collect(),
                grads: vec![vec![0.0; n]; PAGES],
                pending: vec![0; PAGES],
                nets,
                rnn,
            }
        });
        let mut used = 0;
        if spec.memory {
            used += Matcher::new().bytes() + CACHE_WINDOW + 512 * 4;
        }
        if let Some(c) = &circuits {
            // Deployed circuits: one hard truth table (4 bits) per gate.
            let gates: usize = c
                .nets
                .iter()
                .map(|n| n.parameter_count() / 4)
                .sum::<usize>()
                + c.rnn.as_ref().map_or(0, |r| r.0.parameter_count() / 4);
            used += gates.div_ceil(2);
        }
        let inputs_per_depth = levels
            + if spec.memory { 3 } else { 0 }
            + if circuits.is_some() { OUT_PER_DEPTH } else { 0 }
            + 1;
        let mixer_bytes = 8 * inputs_per_depth * 4;
        used += mixer_bytes;
        let counts = CountTable::with_bytes(spec.budget_bytes.saturating_sub(used));
        let mut mixer = vec![0.0; 8 * inputs_per_depth];
        // Start from "trust the finest count estimate".
        for j in 0..8 {
            mixer[j * inputs_per_depth + levels - 1] = 1.0;
        }
        Self {
            rng: mix64(spec.seed ^ 0x5eed),
            spec,
            counts,
            cache: Cache::new(),
            matcher: Matcher::new(),
            circuits,
            mixer,
            inputs_per_depth,
            state: vec![0.0; STATE_BITS],
            rnn_hist: Vec::new(),
            frozen: false,
        }
    }

    pub fn spec(&self) -> &ArmSpec {
        &self.spec
    }

    /// Bytes of state this arm deploys (counts + memory + hard circuits + mixer).
    pub fn state_bytes(&self) -> usize {
        let mut b = self.counts.bytes() + 8 * self.inputs_per_depth * 4;
        if self.spec.memory {
            b += self.matcher.bytes() + CACHE_WINDOW + 512 * 4;
        }
        if let Some(c) = &self.circuits {
            let gates: usize = c
                .nets
                .iter()
                .map(|n| n.parameter_count() / 4)
                .sum::<usize>()
                + c.rnn.as_ref().map_or(0, |r| r.0.parameter_count() / 4);
            b += gates.div_ceil(2);
        }
        b
    }

    fn level_key(level: usize, doc: &[u8], pos: usize, node: usize) -> u64 {
        let mut h = mix64((level as u64) << 56 | node as u64);
        for &b in &doc[pos - level..pos] {
            h = mix64(h ^ u64::from(b) ^ 0x100);
        }
        h
    }

    fn circuit_inputs(&self, doc: &[u8], pos: usize, matched: Option<(u8, usize)>) -> Vec<f64> {
        let mut x = Vec::with_capacity(CIRCUIT_BASE_INPUTS + STATE_BITS);
        for back in 1..=4 {
            let b = if pos >= back { doc[pos - back] } else { 0 };
            for i in 0..8 {
                x.push(f64::from((b >> i) & 1));
            }
        }
        let (mb, active) = matched.map_or((0, 0.0), |(b, _)| (b, 1.0));
        for i in 0..8 {
            x.push(f64::from((mb >> i) & 1));
        }
        x.push(active);
        // 64-bit hashed bigram sketch of the cache window.
        let mut sketch = 0u64;
        let lo = pos.saturating_sub(CACHE_WINDOW);
        let hi = pos;
        if hi >= lo + 2 {
            // Sampled every 4 bytes: bounded cost per byte.
            let mut i = lo + 1;
            while i < hi {
                let h = mix64(u64::from(doc[i - 1]) << 8 | u64::from(doc[i]));
                sketch |= 1 << (h & 63);
                i += 4;
            }
        }
        for i in 0..64 {
            x.push(((sketch >> i) & 1) as f64);
        }
        if self.circuits.as_ref().is_some_and(|c| c.rnn.is_some()) {
            x.extend_from_slice(&self.state);
        }
        x
    }

    fn page(&self, doc: &[u8], pos: usize) -> usize {
        let control = self.spec.circuit.map(|c| c.control);
        if control == Some(Control::FrozenRouting) {
            pos % PAGES
        } else {
            (mix64(u64::from(doc[pos - 1])) as usize) % PAGES
        }
    }

    /// Reset per-document context.
    pub fn start_document(&mut self) {
        self.cache.reset();
        self.matcher.reset();
        self.state.iter_mut().for_each(|s| *s = 0.0);
        self.rnn_hist.clear();
    }

    /// Code (and, unless frozen, learn) byte `pos` of `doc`; returns bits.
    pub fn code_byte(&mut self, doc: &[u8], pos: usize) -> f64 {
        let target = doc[pos];
        let levels = self.spec.max_order + 1;
        let usable = levels.min(pos + 1);
        let matched = if self.spec.memory {
            self.matcher.predicted(doc)
        } else {
            None
        };
        let control = self.spec.circuit.map(|c| c.control);
        // Circuit forward, once per byte.
        let mut circuit_out = Vec::new();
        let mut byte_trace = None;
        if self.circuits.is_some() {
            if control == Some(Control::StateReset) {
                self.state.iter_mut().for_each(|s| *s = 0.0);
            }
            if control == Some(Control::RandomBits) {
                for _ in 0..8 * OUT_PER_DEPTH {
                    self.rng = mix64(self.rng);
                    circuit_out.push((self.rng & 1) as f64);
                }
            } else {
                let x = self.circuit_inputs(doc, pos, matched);
                let page = self.page(doc, pos);
                let c = self.circuits.as_ref().expect("circuits");
                let trace = c.nets[page]
                    .forward(&x, TrainMode::StraightThrough)
                    .expect("circuit inputs");
                circuit_out = trace.outputs().to_vec();
                byte_trace = Some(ByteTrace {
                    page,
                    trace,
                    rnn_traces: Vec::new(),
                });
            }
        }
        let ipd = self.inputs_per_depth;
        let mut d_out = vec![0.0; circuit_out.len()];
        let mut bits = 0.0;
        let mut node = 1usize;
        let mut keys = [0u64; 8];
        let mut inputs = vec![0.0; ipd];
        for j in 0..8 {
            let bit = (target >> (7 - j)) & 1 == 1;
            let mut p = 0.5;
            let mut idx = 0;
            for (l, key) in keys.iter_mut().enumerate().take(levels) {
                if l < usable {
                    *key = Self::level_key(l, doc, pos, node);
                    let (n0, n1) = self.counts.get(*key);
                    p = (n1 + BETA * p) / (n0 + n1 + BETA);
                }
                inputs[idx] = stretch(p);
                idx += 1;
            }
            if self.spec.memory {
                inputs[idx] = stretch(self.cache.p_one(node));
                let (mbit, conf) = match matched {
                    Some((b, len)) if (usize::from(b) | 0x100) >> (8 - j) == node => {
                        let e = if (b >> (7 - j)) & 1 == 1 { 1.0 } else { -1.0 };
                        (e, e * ((len as f64) + 1.0).ln())
                    }
                    _ => (0.0, 0.0),
                };
                inputs[idx + 1] = mbit;
                inputs[idx + 2] = conf;
                idx += 3;
            }
            if !circuit_out.is_empty() {
                for k in 0..OUT_PER_DEPTH {
                    inputs[idx + k] = 2.0 * circuit_out[j * OUT_PER_DEPTH + k] - 1.0;
                }
                idx += OUT_PER_DEPTH;
            }
            inputs[idx] = 1.0;
            let w = &mut self.mixer[j * ipd..(j + 1) * ipd];
            let dot: f64 = w.iter().zip(&inputs).map(|(a, b)| a * b).sum();
            let pm = sigmoid(dot.clamp(-30.0, 30.0)).clamp(1.0 / 65536.0, 1.0 - 1.0 / 65536.0);
            bits -= if bit { pm.log2() } else { (1.0 - pm).log2() };
            if !self.frozen {
                let err = pm - f64::from(u8::from(bit));
                if !circuit_out.is_empty() {
                    let off = idx - OUT_PER_DEPTH;
                    for k in 0..OUT_PER_DEPTH {
                        d_out[j * OUT_PER_DEPTH + k] = err * w[off + k] * 2.0;
                    }
                }
                if self.spec.memory || self.circuits.is_some() {
                    for (wi, xi) in w.iter_mut().zip(&inputs) {
                        *wi -= MIXER_LR * err * xi;
                    }
                }
                for &key in keys.iter().take(usable) {
                    self.counts.add(key, bit);
                }
            }
            node = 2 * node + usize::from(bit);
        }
        if !self.frozen {
            self.learn_circuits(byte_trace, &d_out, control);
        }
        self.advance(doc, pos);
        bits
    }

    fn learn_circuits(&mut self, bt: Option<ByteTrace>, d_out: &[f64], control: Option<Control>) {
        let Some(mut bt) = bt else { return };
        if control == Some(Control::Untrained) {
            return;
        }
        let state_lo = CIRCUIT_BASE_INPUTS;
        let c = self.circuits.as_mut().expect("circuits");
        let dx = c.nets[bt.page].backward_with_input_grad(
            &bt.trace,
            d_out,
            TrainMode::StraightThrough,
            &mut c.grads[bt.page],
        );
        c.pending[bt.page] += 1;
        if c.pending[bt.page] == CIRCUIT_BATCH {
            let g = &mut c.grads[bt.page];
            g.iter_mut().for_each(|v| *v /= CIRCUIT_BATCH as f64);
            c.adams[bt.page].step(c.nets[bt.page].parameters_mut(), g);
            g.iter_mut().for_each(|v| *v = 0.0);
            c.pending[bt.page] = 0;
        }
        if let Some((net, adam, grad, k)) = c.rnn.as_mut() {
            if control != Some(Control::StateReset) {
                // Truncated BPTT through the last `k` workspace updates.
                let mut d_state = dx[state_lo..state_lo + STATE_BITS].to_vec();
                for tr in self.rnn_hist.iter().rev().take(*k) {
                    let din = net.backward_with_input_grad(
                        tr,
                        &d_state,
                        TrainMode::StraightThrough,
                        grad,
                    );
                    d_state = din[..STATE_BITS].to_vec();
                }
                adam.step(net.parameters_mut(), grad);
                grad.iter_mut().for_each(|v| *v = 0.0);
            }
        }
        bt.rnn_traces.clear();
    }

    fn advance(&mut self, doc: &[u8], pos: usize) {
        if self.spec.memory {
            self.cache.push(pos, doc[pos]);
            self.matcher.update(doc, pos);
        }
        if let Some(c) = self.circuits.as_ref() {
            if let Some((net, _, _, k)) = c.rnn.as_ref() {
                let mut x = self.state.clone();
                let prev = if pos >= 1 { doc[pos - 1] } else { 0 };
                for b in [doc[pos], prev] {
                    for i in 0..8 {
                        x.push(f64::from((b >> i) & 1));
                    }
                }
                let tr = net
                    .forward(&x, TrainMode::StraightThrough)
                    .expect("recurrent inputs");
                self.state = tr.outputs().to_vec();
                self.rnn_hist.push(tr);
                if self.rnn_hist.len() > *k {
                    self.rnn_hist.remove(0);
                }
            }
        }
    }

    /// Run one document; updates `tally` with the scored bytes.
    pub fn code_document(&mut self, doc: &[u8], tally: &mut Tally) {
        self.start_document();
        for pos in 0..doc.len() {
            if pos >= FIRST_SCORED {
                tally.bits += self.code_byte(doc, pos);
                tally.events += 1;
            } else {
                self.advance(doc, pos);
            }
        }
        tally.files += 1;
        tally.bytes += doc.len() as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(n: usize, seed: u64) -> Vec<u8> {
        let words = ["the ", "cat ", "sat ", "on ", "a ", "mat ", "and ", "dog\n"];
        let mut s = seed;
        let mut out = Vec::new();
        while out.len() < n {
            s = mix64(s);
            out.extend_from_slice(words[(s % 8) as usize].as_bytes());
        }
        out.truncate(n);
        out
    }

    fn run(spec: ArmSpec) -> (Tally, Tally, usize) {
        let mut m = Model::new(spec);
        let mut train = Tally::default();
        for d in 0..6 {
            m.code_document(&text(3000, d), &mut train);
        }
        m.frozen = true;
        let mut dev = Tally::default();
        m.code_document(&text(3000, 99), &mut dev);
        (train, dev, m.state_bytes())
    }

    #[test]
    fn arms_learn_within_budget_and_dev_is_frozen() {
        let budget = 8 << 20;
        let arms = preregistered_arms(&[1], budget);
        assert_eq!(arms.len(), 12);
        for spec in arms {
            let name = spec.name.clone();
            let (train, dev, bytes) = run(spec);
            assert!(bytes <= budget, "{name} uses {bytes}");
            assert_eq!(dev.events, 2998);
            assert!(dev.bpc() < 4.0, "{name}: {}", dev.bpc());
            assert!(train.bpc().is_finite());
        }
    }

    #[test]
    fn frozen_dev_is_deterministic() {
        let spec = preregistered_arms(&[3], 8 << 20)
            .into_iter()
            .find(|a| a.name.starts_with("recurrent_tbptt4"))
            .unwrap();
        assert_eq!(run(spec.clone()).1, run(spec).1);
    }
}
