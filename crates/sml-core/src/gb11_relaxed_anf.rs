//! GB-11: relaxed training of routed, shared-input sparse ANF banks.
//!
//! Training state is continuous; the deployed model is a hard Boolean circuit.
//! Each bank reads one learned `ADDRESS_BITS`-bit address (shared by all of
//! its gates), each gate is an ANF over that address, and a small Boolean
//! router bank picks one bank per example. The ANF arm relaxes a gate as the
//! exact probability that a XOR of independent Bernoulli(p_m * v_m) terms is
//! one, so the expected monomial count is regularizable in the same basis that
//! the bit-sliced `Pvp6Bank` kernel pays for. The LUT arm is the control:
//! a learned truth table, binarized, then converted to ANF by GF(2) Möbius.
//!
//! Synthetic research pilot only (docs/GB11-RELAXED-SPARSE-ANF-LEARNING-
//! PREREGISTRATION.md). No language-model, timing or energy claim.

use crate::{
    pascal_vector_projection::{Pvp6PopulationProfile, Pvp6Workload},
    pvp6_executor::{Pvp6Bank, Pvp6ExecutionError, Pvp6ExecutionPath, Pvp6Gate},
};

/// Width of one example.
pub const INPUT_BITS: usize = 32;
/// Address bits read by one bank; the bank domain is `2^ADDRESS_BITS` = 64.
pub const ADDRESS_BITS: usize = 6;
/// ANF monomials (and truth-table rows) of one gate.
pub const MONOMIALS: usize = 1 << ADDRESS_BITS;
/// Expert banks.
pub const BANKS: usize = 4;
/// Gates per bank, i.e. output bits per example.
pub const GATES: usize = 8;
/// Router gates; `2^ROUTER_GATES == BANKS`.
pub const ROUTER_GATES: usize = 2;

const UNITS: usize = BANKS + 1;
const ROUTER_UNIT: usize = BANKS;
const GATE_COUNT: usize = BANKS * GATES + ROUTER_GATES;
const SELECTOR_LEN: usize = UNITS * ADDRESS_BITS * INPUT_BITS;
const PARAM_LEN: usize = SELECTOR_LEN + GATE_COUNT * MONOMIALS;
const PROBABILITY_EPSILON: f64 = 1.0e-7;

/// Deterministic SplitMix64 stream.
#[derive(Debug, Clone)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64
    }

    pub fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }
}

/// Hard ANF value of `monomials` at a packed address.
#[must_use]
pub fn anf_eval(monomials: &[u16], address: usize) -> bool {
    monomials.iter().fold(false, |acc, &m| {
        acc ^ (address & usize::from(m) == usize::from(m))
    })
}

/// Packs the selected input bits of `x`; slot `j` becomes address bit `j`.
#[must_use]
pub fn address_of(x: u32, selection: &[usize; ADDRESS_BITS]) -> usize {
    selection
        .iter()
        .enumerate()
        .fold(0, |acc, (j, &i)| acc | ((((x >> i) & 1) as usize) << j))
}

/// GF(2) Möbius transform: truth table to ANF coefficients, in place.
pub fn gf2_mobius(table: &mut [bool; MONOMIALS]) {
    for bit in 0..ADDRESS_BITS {
        for m in 0..MONOMIALS {
            if m & (1 << bit) != 0 {
                table[m] ^= table[m ^ (1 << bit)];
            }
        }
    }
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

/// Teacher function family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeacherFamily {
    /// 1..=6 monomials of degree 1..=3 per gate.
    Sparse,
    /// 24..=40 uniform monomials per gate.
    Dense,
}

/// Frozen routed teacher: router bank plus `BANKS` banks of `GATES` ANF gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Teacher {
    pub router_address: [usize; ADDRESS_BITS],
    pub router_gates: [Vec<u16>; ROUTER_GATES],
    pub bank_addresses: [[usize; ADDRESS_BITS]; BANKS],
    pub bank_gates: Vec<Vec<Vec<u16>>>,
}

fn distinct_selection(rng: &mut SplitMix64) -> [usize; ADDRESS_BITS] {
    let mut selection = [0; ADDRESS_BITS];
    let mut used = 0_u64;
    for slot in &mut selection {
        loop {
            let i = rng.below(INPUT_BITS);
            if used & (1 << i) == 0 {
                used |= 1 << i;
                *slot = i;
                break;
            }
        }
    }
    selection
}

fn teacher_gate(rng: &mut SplitMix64, family: TeacherFamily) -> Vec<u16> {
    let k = match family {
        TeacherFamily::Sparse => 1 + rng.below(6),
        TeacherFamily::Dense => 24 + rng.below(17),
    };
    let mut used = 0_u64;
    let mut gate = Vec::with_capacity(k);
    while gate.len() < k {
        let mask = match family {
            TeacherFamily::Sparse => {
                let degree = 1 + rng.below(3);
                let mut mask = 0_usize;
                while mask.count_ones() as usize != degree {
                    mask |= 1 << rng.below(ADDRESS_BITS);
                }
                mask
            }
            TeacherFamily::Dense => rng.below(MONOMIALS),
        };
        if used & (1 << mask) == 0 {
            used |= 1 << mask;
            gate.push(mask as u16);
        }
    }
    gate.sort_unstable();
    gate
}

impl Teacher {
    /// Deterministic teacher; the router is `x_a XOR x_b x_c` per router gate.
    #[must_use]
    pub fn generate(family: TeacherFamily, seed: u64) -> Self {
        let mut rng = SplitMix64::new(seed);
        let router_address = distinct_selection(&mut rng);
        let router_gates = [vec![0b00_0001, 0b00_0110], vec![0b00_1000, 0b11_0000]];
        let mut bank_addresses = [[0; ADDRESS_BITS]; BANKS];
        let mut bank_gates = Vec::with_capacity(BANKS);
        for address in &mut bank_addresses {
            *address = distinct_selection(&mut rng);
            bank_gates.push((0..GATES).map(|_| teacher_gate(&mut rng, family)).collect());
        }
        Self {
            router_address,
            router_gates,
            bank_addresses,
            bank_gates,
        }
    }

    #[must_use]
    pub fn route(&self, x: u32) -> usize {
        let address = address_of(x, &self.router_address);
        self.router_gates.iter().enumerate().fold(0, |acc, (j, g)| {
            acc | (usize::from(anf_eval(g, address)) << j)
        })
    }

    #[must_use]
    pub fn label(&self, x: u32) -> u8 {
        let bank = self.route(x);
        let address = address_of(x, &self.bank_addresses[bank]);
        self.bank_gates[bank]
            .iter()
            .enumerate()
            .fold(0, |acc, (g, gate)| {
                acc | (u8::from(anf_eval(gate, address)) << g)
            })
    }
}

/// One split of examples, labels and teacher bank identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dataset {
    pub inputs: Vec<u32>,
    pub labels: Vec<u8>,
    pub banks: Vec<u8>,
}

impl Dataset {
    /// `noise` flips each label bit independently with this probability.
    #[must_use]
    pub fn sample(teacher: &Teacher, count: usize, seed: u64, noise: f64) -> Self {
        let mut rng = SplitMix64::new(seed);
        let mut data = Self {
            inputs: Vec::with_capacity(count),
            labels: Vec::with_capacity(count),
            banks: Vec::with_capacity(count),
        };
        for _ in 0..count {
            let x = rng.next_u64() as u32;
            let mut label = teacher.label(x);
            for g in 0..GATES {
                if rng.unit() < noise {
                    label ^= 1 << g;
                }
            }
            data.inputs.push(x);
            data.labels.push(label);
            data.banks.push(teacher.route(x) as u8);
        }
        data
    }
}

/// Expert parameterization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpertKind {
    /// Logit per truth-table row, multilinear extension, STE, then Möbius.
    Lut,
    /// Logit per ANF monomial, probabilistic-XOR relaxation.
    Anf,
}

/// One preregistered arm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arm {
    pub name: &'static str,
    pub kind: ExpertKind,
    /// Weight of the expected cost `sum_m p_m (1 + |m|)`, averaged over gates.
    pub lambda: f64,
    /// Logistic (binary Concrete) noise before the STE phase.
    pub concrete: bool,
}

pub const ARMS: [Arm; 6] = [
    Arm {
        name: "lut_ste",
        kind: ExpertKind::Lut,
        lambda: 0.0,
        concrete: false,
    },
    Arm {
        name: "anf_xor",
        kind: ExpertKind::Anf,
        lambda: 0.0,
        concrete: false,
    },
    Arm {
        name: "anf_cost_s",
        kind: ExpertKind::Anf,
        lambda: 3.0e-4,
        concrete: false,
    },
    Arm {
        name: "anf_cost_m",
        kind: ExpertKind::Anf,
        lambda: 1.0e-3,
        concrete: false,
    },
    Arm {
        name: "anf_cost_l",
        kind: ExpertKind::Anf,
        lambda: 3.0e-3,
        concrete: false,
    },
    Arm {
        name: "anf_concrete",
        kind: ExpertKind::Anf,
        lambda: 1.0e-3,
        concrete: true,
    },
];

/// Optimization settings; `preregistered` is the frozen P0 configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrainConfig {
    pub epochs: usize,
    pub batch: usize,
    pub learning_rate: f64,
    pub balance: f64,
    pub tau_start: f64,
    pub tau_end: f64,
    pub concrete_start: f64,
    pub concrete_end: f64,
    /// Fraction of epochs, at the end, trained with straight-through binarization.
    pub ste_fraction: f64,
    /// Penalty on overlap `sum_{j<l} <w_j, w_l>` of one unit's slot selectors.
    pub diversity: f64,
    /// Half-width of the uniform address-selector logit initialization.
    pub selector_init: f64,
    /// Learning-rate multiplier for address-selector logits.
    pub selector_lr_scale: f64,
    /// Scale of Gumbel noise added to selector logits (one draw per batch).
    pub selector_gumbel: f64,
    pub seed: u64,
}

impl TrainConfig {
    #[must_use]
    pub const fn preregistered(seed: u64) -> Self {
        Self {
            epochs: 40,
            batch: 256,
            learning_rate: 0.05,
            balance: 0.1,
            tau_start: 1.0,
            tau_end: 0.05,
            concrete_start: 1.0,
            concrete_end: 0.3,
            ste_fraction: 0.25,
            diversity: 0.0,
            selector_init: 0.1,
            selector_lr_scale: 1.0,
            selector_gumbel: 0.0,
            seed,
        }
    }

    fn ste_start(&self) -> usize {
        self.epochs - (self.epochs as f64 * self.ste_fraction).round() as usize
    }

    fn schedule(&self, epoch: usize, concrete: bool) -> Schedule {
        let progress = if self.epochs > 1 {
            epoch as f64 / (self.epochs - 1) as f64
        } else {
            1.0
        };
        let ste = epoch >= self.ste_start();
        let concrete_temperature = (concrete && !ste).then(|| {
            let span = self.ste_start().saturating_sub(1).max(1) as f64;
            let t = (epoch as f64 / span).min(1.0);
            self.concrete_start * (self.concrete_end / self.concrete_start).powf(t)
        });
        Schedule {
            tau: self.tau_start * (self.tau_end / self.tau_start).powf(progress),
            ste,
            concrete_temperature,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Schedule {
    tau: f64,
    ste: bool,
    concrete_temperature: Option<f64>,
}

/// Trainable state: address-selector logits then per-gate logits.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub kind: ExpertKind,
    params: Vec<f64>,
}

fn selector_index(unit: usize, slot: usize, input: usize) -> usize {
    (unit * ADDRESS_BITS + slot) * INPUT_BITS + input
}

fn gate_index(gate: usize, monomial: usize) -> usize {
    SELECTOR_LEN + gate * MONOMIALS + monomial
}

fn unit_gates(unit: usize) -> core::ops::Range<usize> {
    if unit == ROUTER_UNIT {
        BANKS * GATES..GATE_COUNT
    } else {
        unit * GATES..(unit + 1) * GATES
    }
}

/// Monomial values `v_m = prod_{j in m} a_j`.
fn monomial_values(address: &[f64; ADDRESS_BITS]) -> [f64; MONOMIALS] {
    let mut v = [1.0; MONOMIALS];
    for m in 1..MONOMIALS {
        let low = m.trailing_zeros() as usize;
        v[m] = v[m & (m - 1)] * address[low];
    }
    v
}

/// Per-batch gate tables: forward coefficients and their derivative scale.
struct GateTables {
    /// ANF: p_m used forward. LUT: real Möbius coefficients alpha_m of s.
    forward: Vec<[f64; MONOMIALS]>,
    /// ANF: dp/dtheta. LUT: ds/dphi.
    scale: Vec<[f64; MONOMIALS]>,
}

/// Hard Boolean model compiled from a trained [`Model`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardModel {
    pub addresses: [[usize; ADDRESS_BITS]; UNITS],
    /// Sorted monomials of each gate; banks first, router gates last.
    pub gates: Vec<Vec<u16>>,
}

impl HardModel {
    #[must_use]
    pub fn route(&self, x: u32) -> usize {
        let address = address_of(x, &self.addresses[ROUTER_UNIT]);
        unit_gates(ROUTER_UNIT).enumerate().fold(0, |acc, (j, g)| {
            acc | (usize::from(anf_eval(&self.gates[g], address)) << j)
        })
    }

    #[must_use]
    pub fn predict(&self, x: u32) -> (usize, u8) {
        let bank = self.route(x);
        let address = address_of(x, &self.addresses[bank]);
        let byte = unit_gates(bank).enumerate().fold(0, |acc, (g, gi)| {
            acc | (u8::from(anf_eval(&self.gates[gi], address)) << g)
        });
        (bank, byte)
    }
}

impl Model {
    #[must_use]
    pub fn init(kind: ExpertKind, seed: u64) -> Self {
        Self::init_with(kind, seed, 0.1)
    }

    #[must_use]
    pub fn init_with(kind: ExpertKind, seed: u64, selector_init: f64) -> Self {
        let mut rng = SplitMix64::new(seed ^ 0x6B11_1417_A11C_E5ED);
        let mut params = Vec::with_capacity(PARAM_LEN);
        for _ in 0..SELECTOR_LEN {
            params.push(selector_init * (2.0 * rng.unit() - 1.0));
        }
        let base = match kind {
            ExpertKind::Anf => -3.0,
            ExpertKind::Lut => 0.0,
        };
        for _ in SELECTOR_LEN..PARAM_LEN {
            params.push(base + 0.2 * rng.unit() - 0.1);
        }
        Self { kind, params }
    }

    #[must_use]
    pub fn params(&self) -> &[f64] {
        &self.params
    }

    fn selector_weights(&self, tau: f64) -> Vec<[f64; INPUT_BITS]> {
        self.selector_weights_noisy(tau, &[])
    }

    /// `noise`, when non-empty, holds one additive term per selector logit.
    fn selector_weights_noisy(&self, tau: f64, noise: &[f64]) -> Vec<[f64; INPUT_BITS]> {
        let mut out = Vec::with_capacity(UNITS * ADDRESS_BITS);
        for unit in 0..UNITS {
            for slot in 0..ADDRESS_BITS {
                let base = selector_index(unit, slot, 0);
                let logits: [f64; INPUT_BITS] = core::array::from_fn(|i| {
                    self.params[base + i] + noise.get(base + i).copied().unwrap_or(0.0)
                });
                let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let mut w = [0.0; INPUT_BITS];
                let mut sum = 0.0;
                for (wi, &l) in w.iter_mut().zip(&logits) {
                    *wi = ((l - max) / tau).exp();
                    sum += *wi;
                }
                for wi in &mut w {
                    *wi /= sum;
                }
                out.push(w);
            }
        }
        out
    }

    fn gate_tables(&self, sched: &Schedule, rng: Option<&mut SplitMix64>) -> GateTables {
        let mut rng = rng;
        let mut forward = Vec::with_capacity(GATE_COUNT);
        let mut scale = Vec::with_capacity(GATE_COUNT);
        for gate in 0..GATE_COUNT {
            let mut f = [0.0; MONOMIALS];
            let mut s = [0.0; MONOMIALS];
            for m in 0..MONOMIALS {
                let theta = self.params[gate_index(gate, m)];
                let p = sigmoid(theta);
                match (sched.ste, sched.concrete_temperature, rng.as_deref_mut()) {
                    (true, _, _) => {
                        f[m] = f64::from(u8::from(theta >= 0.0));
                        s[m] = p * (1.0 - p);
                    }
                    (false, Some(t), Some(r)) if self.kind == ExpertKind::Anf => {
                        let u = r.unit().clamp(1.0e-9, 1.0 - 1.0e-9);
                        let noisy = sigmoid((theta + (u / (1.0 - u)).ln()) / t);
                        f[m] = noisy;
                        s[m] = noisy * (1.0 - noisy) / t;
                    }
                    _ => {
                        f[m] = p;
                        s[m] = p * (1.0 - p);
                    }
                }
            }
            if self.kind == ExpertKind::Lut {
                // alpha_m = sum_{u subset m} (-1)^{|m|-|u|} s_u.
                for bit in 0..ADDRESS_BITS {
                    for m in 0..MONOMIALS {
                        if m & (1 << bit) != 0 {
                            f[m] -= f[m ^ (1 << bit)];
                        }
                    }
                }
            }
            forward.push(f);
            scale.push(s);
        }
        GateTables { forward, scale }
    }

    fn soft_address(weights: &[[f64; INPUT_BITS]], unit: usize, x: u32) -> [f64; ADDRESS_BITS] {
        let mut a = [0.0; ADDRESS_BITS];
        for (slot, aj) in a.iter_mut().enumerate() {
            let w = &weights[unit * ADDRESS_BITS + slot];
            *aj = (0..INPUT_BITS)
                .filter(|&i| (x >> i) & 1 == 1)
                .map(|i| w[i])
                .sum();
        }
        a
    }

    fn gate_forward(&self, table: &[f64; MONOMIALS], v: &[f64; MONOMIALS]) -> f64 {
        match self.kind {
            ExpertKind::Lut => table.iter().zip(v).map(|(a, b)| a * b).sum::<f64>(),
            ExpertKind::Anf => {
                let q: f64 = table
                    .iter()
                    .zip(v)
                    .map(|(p, vm)| 1.0 - 2.0 * p * vm)
                    .product();
                0.5 * (1.0 - q)
            }
        }
    }

    /// Accumulates d/dtable into `g_table` and d/dv into `g_v`.
    fn gate_backward(
        &self,
        table: &[f64; MONOMIALS],
        v: &[f64; MONOMIALS],
        upstream: f64,
        g_table: &mut [f64; MONOMIALS],
        g_v: &mut [f64; MONOMIALS],
    ) {
        match self.kind {
            ExpertKind::Lut => {
                for m in 0..MONOMIALS {
                    g_table[m] += upstream * v[m];
                    g_v[m] += upstream * table[m];
                }
            }
            ExpertKind::Anf => {
                let mut prefix = [1.0; MONOMIALS + 1];
                for m in 0..MONOMIALS {
                    prefix[m + 1] = prefix[m] * (1.0 - 2.0 * table[m] * v[m]);
                }
                let mut suffix = 1.0;
                for m in (0..MONOMIALS).rev() {
                    let exclusive = prefix[m] * suffix;
                    g_table[m] += upstream * v[m] * exclusive;
                    g_v[m] += upstream * table[m] * exclusive;
                    suffix *= 1.0 - 2.0 * table[m] * v[m];
                }
            }
        }
    }

    fn router_probabilities(r: &[f64; ROUTER_GATES]) -> [f64; BANKS] {
        let mut pi = [1.0; BANKS];
        for (b, pb) in pi.iter_mut().enumerate() {
            for (j, &rj) in r.iter().enumerate() {
                *pb *= if (b >> j) & 1 == 1 { rj } else { 1.0 - rj };
            }
        }
        pi
    }

    /// Mean loss and gradient of one mini-batch.
    fn batch_loss_grad(
        &self,
        inputs: &[u32],
        labels: &[u8],
        sched: &Schedule,
        balance: f64,
        lambda: f64,
        rng: Option<&mut SplitMix64>,
        selector_noise: &[f64],
    ) -> (f64, Vec<f64>) {
        let weights = self.selector_weights_noisy(sched.tau, selector_noise);
        let tables = self.gate_tables(sched, rng);
        let n = inputs.len() as f64;

        // Pass 1: routing distribution for the balance term.
        let mut routes = Vec::with_capacity(inputs.len());
        let mut mean_pi = [0.0; BANKS];
        for &x in inputs {
            let a = Self::soft_address(&weights, ROUTER_UNIT, x);
            let v = monomial_values(&a);
            let mut r = [0.0; ROUTER_GATES];
            for (j, g) in unit_gates(ROUTER_UNIT).enumerate() {
                r[j] = self
                    .gate_forward(&tables.forward[g], &v)
                    .clamp(PROBABILITY_EPSILON, 1.0 - PROBABILITY_EPSILON);
            }
            let pi = Self::router_probabilities(&r);
            for b in 0..BANKS {
                mean_pi[b] += pi[b] / n;
            }
            routes.push((a, v, r, pi));
        }
        let mut loss = balance
            * mean_pi
                .iter()
                .map(|&p| p * p.max(PROBABILITY_EPSILON).ln())
                .sum::<f64>();
        let balance_grad: [f64; BANKS] =
            core::array::from_fn(|b| balance * (mean_pi[b].max(PROBABILITY_EPSILON).ln() + 1.0));

        let mut grad = vec![0.0; PARAM_LEN];
        let mut g_tables = vec![[0.0; MONOMIALS]; GATE_COUNT];

        for (sample, (&x, &y)) in inputs.iter().zip(labels).enumerate() {
            let (ra, rv, r, pi) = &routes[sample];
            let mut addresses = [[0.0; ADDRESS_BITS]; BANKS];
            let mut values = [[0.0; MONOMIALS]; BANKS];
            let mut bank_out = [[0.0; GATES]; BANKS];
            for b in 0..BANKS {
                addresses[b] = Self::soft_address(&weights, b, x);
                values[b] = monomial_values(&addresses[b]);
                for (g, gi) in unit_gates(b).enumerate() {
                    bank_out[b][g] = self.gate_forward(&tables.forward[gi], &values[b]);
                }
            }
            let mut g_pi = balance_grad;
            let mut g_out = [0.0; GATES];
            for g in 0..GATES {
                let p = (0..BANKS)
                    .map(|b| pi[b] * bank_out[b][g])
                    .sum::<f64>()
                    .clamp(PROBABILITY_EPSILON, 1.0 - PROBABILITY_EPSILON);
                let target = f64::from((y >> g) & 1);
                loss -= (target * p.ln() + (1.0 - target) * (1.0 - p).ln()) / (GATES as f64 * n);
                g_out[g] = (p - target) / (p * (1.0 - p) * GATES as f64);
                for b in 0..BANKS {
                    g_pi[b] += g_out[g] * bank_out[b][g];
                }
            }
            // Expert banks.
            for b in 0..BANKS {
                let mut g_v = [0.0; MONOMIALS];
                for (g, gi) in unit_gates(b).enumerate() {
                    self.gate_backward(
                        &tables.forward[gi],
                        &values[b],
                        g_out[g] * pi[b],
                        &mut g_tables[gi],
                        &mut g_v,
                    );
                }
                accumulate_selector(
                    &mut grad,
                    &weights,
                    b,
                    x,
                    &addresses[b],
                    &values[b],
                    &g_v,
                    sched.tau,
                );
            }
            // Router bank.
            let mut g_r = [0.0; ROUTER_GATES];
            for b in 0..BANKS {
                for (j, gr) in g_r.iter_mut().enumerate() {
                    let mut other = 1.0;
                    for (l, &rl) in r.iter().enumerate() {
                        if l != j {
                            other *= if (b >> l) & 1 == 1 { rl } else { 1.0 - rl };
                        }
                    }
                    let sign = if (b >> j) & 1 == 1 { 1.0 } else { -1.0 };
                    *gr += g_pi[b] * sign * other;
                }
            }
            let mut g_v = [0.0; MONOMIALS];
            for (j, gi) in unit_gates(ROUTER_UNIT).enumerate() {
                self.gate_backward(&tables.forward[gi], rv, g_r[j], &mut g_tables[gi], &mut g_v);
            }
            accumulate_selector(&mut grad, &weights, ROUTER_UNIT, x, ra, rv, &g_v, sched.tau);
        }

        for (gate, g_table) in g_tables.iter_mut().enumerate() {
            if self.kind == ExpertKind::Lut {
                // Transpose of the real Möbius transform.
                for bit in 0..ADDRESS_BITS {
                    for m in 0..MONOMIALS {
                        if m & (1 << bit) == 0 {
                            g_table[m] -= g_table[m | (1 << bit)];
                        }
                    }
                }
            }
            for m in 0..MONOMIALS {
                grad[gate_index(gate, m)] += g_table[m] * tables.scale[gate][m];
            }
        }
        for g in &mut grad {
            *g /= n;
        }
        if lambda > 0.0 {
            for gate in 0..GATE_COUNT {
                for m in 0..MONOMIALS {
                    let p = sigmoid(self.params[gate_index(gate, m)]);
                    let cost = 1.0 + f64::from(m.count_ones());
                    loss += lambda * p * cost / GATE_COUNT as f64;
                    grad[gate_index(gate, m)] += lambda * cost * p * (1.0 - p) / GATE_COUNT as f64;
                }
            }
        }
        (loss, grad)
    }

    /// Adds `mu * sum_{j<l} <w_j, w_l>` per unit and its logit gradient.
    fn add_diversity(&self, loss: f64, mut grad: Vec<f64>, tau: f64, mu: f64) -> (f64, Vec<f64>) {
        let weights = self.selector_weights(tau);
        let mut loss = loss;
        for unit in 0..UNITS {
            let slots = &weights[unit * ADDRESS_BITS..(unit + 1) * ADDRESS_BITS];
            let mut total = [0.0; INPUT_BITS];
            for w in slots {
                for (t, wi) in total.iter_mut().zip(w) {
                    *t += wi;
                }
            }
            for (slot, w) in slots.iter().enumerate() {
                // d/dw_ji = mu * sum_{l != j} w_li; overlap counted once per pair.
                let g: [f64; INPUT_BITS] = core::array::from_fn(|i| mu * (total[i] - w[i]));
                let dot: f64 = w.iter().zip(&g).map(|(a, b)| a * b).sum();
                loss += 0.5 * dot;
                for i in 0..INPUT_BITS {
                    grad[selector_index(unit, slot, i)] += w[i] * (g[i] - dot) / tau;
                }
            }
        }
        (loss, grad)
    }

    /// Hard Boolean form: argmax addresses and binarized gates as ANF.
    #[must_use]
    pub fn harden(&self) -> HardModel {
        let mut addresses = [[0; ADDRESS_BITS]; UNITS];
        for (unit, address) in addresses.iter_mut().enumerate() {
            for (slot, a) in address.iter_mut().enumerate() {
                let base = selector_index(unit, slot, 0);
                let logits = &self.params[base..base + INPUT_BITS];
                *a = (0..INPUT_BITS)
                    .fold(0, |best, i| if logits[i] > logits[best] { i } else { best });
            }
        }
        let gates = (0..GATE_COUNT)
            .map(|gate| {
                let mut bits = [false; MONOMIALS];
                for (m, bit) in bits.iter_mut().enumerate() {
                    *bit = self.params[gate_index(gate, m)] >= 0.0;
                }
                if self.kind == ExpertKind::Lut {
                    gf2_mobius(&mut bits);
                }
                (0..MONOMIALS)
                    .filter(|&m| bits[m])
                    .map(|m| m as u16)
                    .collect()
            })
            .collect();
        HardModel { addresses, gates }
    }

    /// Relaxed prediction at temperature `tau`, deterministic gate probabilities.
    #[must_use]
    pub fn soft_predict(&self, x: u32, tau: f64) -> u8 {
        let sched = Schedule {
            tau,
            ste: false,
            concrete_temperature: None,
        };
        let weights = self.selector_weights(tau);
        let tables = self.gate_tables(&sched, None);
        let rv = monomial_values(&Self::soft_address(&weights, ROUTER_UNIT, x));
        let mut r = [0.0; ROUTER_GATES];
        for (j, g) in unit_gates(ROUTER_UNIT).enumerate() {
            r[j] = self.gate_forward(&tables.forward[g], &rv);
        }
        let pi = Self::router_probabilities(&r);
        let mut byte = 0;
        let mut out = [0.0; GATES];
        for (b, &pb) in pi.iter().enumerate() {
            let v = monomial_values(&Self::soft_address(&weights, b, x));
            for (g, gi) in unit_gates(b).enumerate() {
                out[g] += pb * self.gate_forward(&tables.forward[gi], &v);
            }
        }
        for (g, &p) in out.iter().enumerate() {
            if p > 0.5 {
                byte |= 1 << g;
            }
        }
        byte
    }
}

#[allow(clippy::too_many_arguments)]
fn accumulate_selector(
    grad: &mut [f64],
    weights: &[[f64; INPUT_BITS]],
    unit: usize,
    x: u32,
    a: &[f64; ADDRESS_BITS],
    v: &[f64; MONOMIALS],
    g_v: &[f64; MONOMIALS],
    tau: f64,
) {
    for slot in 0..ADDRESS_BITS {
        let bit = 1 << slot;
        let g_a: f64 = (0..MONOMIALS)
            .filter(|m| m & bit != 0)
            .map(|m| g_v[m] * v[m ^ bit])
            .sum();
        let w = &weights[unit * ADDRESS_BITS + slot];
        for (i, &wi) in w.iter().enumerate() {
            let xi = f64::from((x >> i) & 1);
            grad[selector_index(unit, slot, i)] += g_a * wi * (xi - a[slot]) / tau;
        }
    }
}

/// Final metrics of one training run.
#[derive(Debug, Clone, PartialEq)]
pub struct RunReport {
    pub final_loss: f64,
    pub val_bit_accuracy: f64,
    pub test_bit_accuracy: f64,
    pub test_byte_accuracy: f64,
    pub soft_test_bit_accuracy: f64,
    /// Monomial count of each of the `BANKS * GATES` expert gates.
    pub expert_k: Vec<usize>,
    pub router_k: Vec<usize>,
    pub mean_degree: f64,
    pub zero_gates: usize,
    pub distinct_address_bits: Vec<usize>,
    /// Uniform-density sub-banks (one transposition each) per bank.
    pub sub_banks: Vec<usize>,
    pub routing_purity: f64,
    pub min_bank_load: f64,
    /// Analytic bit-slice cost per 64-query block, summed over banks.
    pub and_ops: usize,
    pub xor_ops: usize,
    pub parity_checks: u64,
    pub parity_mismatches: u64,
}

fn bit_accuracy(predict: impl Fn(u32) -> u8, data: &Dataset) -> (f64, f64) {
    let mut bits = 0_u64;
    let mut bytes = 0_u64;
    for (&x, &y) in data.inputs.iter().zip(&data.labels) {
        let diff = predict(x) ^ y;
        bits += u64::from(GATES as u32 - diff.count_ones());
        bytes += u64::from(diff == 0);
    }
    let n = data.inputs.len() as f64;
    (bits as f64 / (n * GATES as f64), bytes as f64 / n)
}

/// Compiles each unit's non-zero gates into uniform-density `Pvp6Bank<64>`
/// groups and checks `Selected` and `Bitsliced` against the hard model.
pub fn pvp6_parity(hard: &HardModel, data: &Dataset) -> Result<(u64, u64), Pvp6ExecutionError> {
    let mut checks = 0_u64;
    let mut mismatches = 0_u64;
    for unit in 0..UNITS {
        let queries: Vec<usize> = data
            .inputs
            .iter()
            .filter(|&&x| unit == ROUTER_UNIT || hard.route(x) == unit)
            .map(|&x| address_of(x, &hard.addresses[unit]))
            .collect();
        let gate_ids: Vec<usize> = unit_gates(unit).collect();
        let mut densities: Vec<usize> = gate_ids
            .iter()
            .map(|&g| hard.gates[g].len())
            .filter(|&k| k > 0)
            .collect();
        densities.sort_unstable();
        densities.dedup();
        for k in densities {
            let members: Vec<usize> = gate_ids
                .iter()
                .copied()
                .filter(|&g| hard.gates[g].len() == k)
                .collect();
            let bank = Pvp6Bank::<MONOMIALS>::new(
                members
                    .iter()
                    .map(|&g| Pvp6Gate::from_monomials(&hard.gates[g]))
                    .collect::<Result<_, _>>()?,
            )?;
            for chunk in queries.chunks(64) {
                let workload = Pvp6Workload::new(
                    k as u32,
                    chunk.len() as u32,
                    1,
                    Pvp6PopulationProfile::AffineUniform,
                );
                let selected = bank.evaluate_on(Pvp6ExecutionPath::Selected, workload, chunk)?;
                let sliced = bank.evaluate_on(Pvp6ExecutionPath::Bitsliced, workload, chunk)?;
                for (gi, &g) in members.iter().enumerate() {
                    for (qi, &q) in chunk.iter().enumerate() {
                        let expected = anf_eval(&hard.gates[g], q);
                        checks += 1;
                        if selected.value(gi, qi) != Some(expected)
                            || sliced.value(gi, qi) != Some(expected)
                        {
                            mismatches += 1;
                        }
                    }
                }
            }
        }
    }
    Ok((checks, mismatches))
}

/// Trains one arm on `train`, then reports VAL/TEST, parsimony and parity.
pub fn train_and_report(
    arm: &Arm,
    config: &TrainConfig,
    train: &Dataset,
    val: &Dataset,
    test: &Dataset,
) -> Result<(Model, RunReport), Pvp6ExecutionError> {
    let mut model = Model::init_with(arm.kind, config.seed, config.selector_init);
    let mut order: Vec<usize> = (0..train.inputs.len()).collect();
    let mut rng = SplitMix64::new(config.seed.wrapping_mul(0x2545_F491_4F6C_DD1D));
    let mut noise_rng = SplitMix64::new(config.seed ^ 0xC0C0_C0C0_C0C0_C0C0);
    let mut m1 = vec![0.0; PARAM_LEN];
    let mut m2 = vec![0.0; PARAM_LEN];
    let mut step = 0_i32;
    let mut final_loss = 0.0;
    for epoch in 0..config.epochs {
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i + 1));
        }
        let sched = config.schedule(epoch, arm.concrete);
        let mut epoch_loss = 0.0;
        let mut batches = 0;
        for chunk in order.chunks(config.batch) {
            let xs: Vec<u32> = chunk.iter().map(|&i| train.inputs[i]).collect();
            let ys: Vec<u8> = chunk.iter().map(|&i| train.labels[i]).collect();
            let selector_noise: Vec<f64> = if config.selector_gumbel > 0.0 {
                (0..SELECTOR_LEN)
                    .map(|_| {
                        let u = noise_rng.unit().clamp(1.0e-12, 1.0 - 1.0e-12);
                        -config.selector_gumbel * (-u.ln()).ln()
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let (loss, grad) = model.batch_loss_grad(
                &xs,
                &ys,
                &sched,
                config.balance,
                arm.lambda,
                Some(&mut noise_rng),
                &selector_noise,
            );
            let (loss, grad) = if config.diversity > 0.0 {
                model.add_diversity(loss, grad, sched.tau, config.diversity)
            } else {
                (loss, grad)
            };
            step += 1;
            let c1 = 1.0 - 0.9_f64.powi(step);
            let c2 = 1.0 - 0.999_f64.powi(step);
            for (k, g) in grad.iter().enumerate() {
                m1[k] = 0.9 * m1[k] + 0.1 * g;
                m2[k] = 0.999 * m2[k] + 0.001 * g * g;
                let lr = if k < SELECTOR_LEN {
                    config.learning_rate * config.selector_lr_scale
                } else {
                    config.learning_rate
                };
                model.params[k] -= lr * (m1[k] / c1) / ((m2[k] / c2).sqrt() + 1.0e-8);
            }
            epoch_loss += loss;
            batches += 1;
        }
        final_loss = epoch_loss / f64::from(batches);
        if std::env::var_os("GB11_TRACE").is_some() {
            let hard = model.harden();
            let (h, _) = bit_accuracy(|x| hard.predict(x).1, val);
            let (sacc, _) = bit_accuracy(|x| model.soft_predict(x, sched.tau), val);
            let k: usize = hard.gates.iter().map(Vec::len).sum();
            eprintln!("TRACE {} e{epoch} tau {:.3} loss {final_loss:.4} soft {sacc:.4} hard {h:.4} k {k} addr {:?}", arm.name, sched.tau, hard.addresses);
        }
    }

    let hard = model.harden();
    let (val_bit_accuracy, _) = bit_accuracy(|x| hard.predict(x).1, val);
    let (test_bit_accuracy, test_byte_accuracy) = bit_accuracy(|x| hard.predict(x).1, test);
    let final_tau = config.schedule(config.epochs - 1, false).tau;
    let (soft_test_bit_accuracy, _) = bit_accuracy(|x| model.soft_predict(x, final_tau), test);

    let expert_k: Vec<usize> = hard.gates[..BANKS * GATES].iter().map(Vec::len).collect();
    let router_k = hard.gates[BANKS * GATES..].iter().map(Vec::len).collect();
    let degrees: Vec<u32> = hard.gates[..BANKS * GATES]
        .iter()
        .flatten()
        .map(|m| m.count_ones())
        .collect();
    let mean_degree = if degrees.is_empty() {
        0.0
    } else {
        f64::from(degrees.iter().sum::<u32>()) / degrees.len() as f64
    };
    let zero_gates = expert_k.iter().filter(|&&k| k == 0).count();
    let mut distinct_address_bits = Vec::with_capacity(BANKS);
    let mut sub_banks = Vec::with_capacity(BANKS);
    for b in 0..BANKS {
        let mut bits = hard.addresses[b].to_vec();
        bits.sort_unstable();
        bits.dedup();
        distinct_address_bits.push(bits.len());
        let mut ks: Vec<usize> = expert_k[b * GATES..(b + 1) * GATES]
            .iter()
            .copied()
            .filter(|&k| k > 0)
            .collect();
        ks.sort_unstable();
        ks.dedup();
        sub_banks.push(ks.len());
    }
    let and_ops = hard.gates[..BANKS * GATES]
        .iter()
        .flatten()
        .map(|m| (m.count_ones() as usize).saturating_sub(1))
        .sum();
    let xor_ops = expert_k.iter().map(|k| k.saturating_sub(1)).sum();

    let mut confusion = [[0_u64; BANKS]; BANKS];
    for (&x, &t) in test.inputs.iter().zip(&test.banks) {
        confusion[hard.route(x)][usize::from(t)] += 1;
    }
    let n = test.inputs.len() as f64;
    let routing_purity = confusion
        .iter()
        .map(|row| *row.iter().max().unwrap_or(&0))
        .sum::<u64>() as f64
        / n;
    let min_bank_load = confusion
        .iter()
        .map(|row| row.iter().sum::<u64>() as f64 / n)
        .fold(f64::INFINITY, f64::min);
    let (parity_checks, parity_mismatches) = pvp6_parity(&hard, test)?;

    Ok((
        model,
        RunReport {
            final_loss,
            val_bit_accuracy,
            test_bit_accuracy,
            test_byte_accuracy,
            soft_test_bit_accuracy,
            expert_k,
            router_k,
            mean_degree,
            zero_gates,
            distinct_address_bits,
            sub_banks,
            routing_purity,
            min_bank_load,
            and_ops,
            xor_ops,
            parity_checks,
            parity_mismatches,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soft_sched() -> Schedule {
        Schedule {
            tau: 0.7,
            ste: false,
            concrete_temperature: None,
        }
    }

    fn perturbed(kind: ExpertKind, seed: u64) -> Model {
        let mut model = Model::init(kind, seed);
        let mut rng = SplitMix64::new(seed + 99);
        for p in &mut model.params {
            *p += rng.unit() * 2.0 - 1.0;
        }
        model
    }

    #[test]
    fn teacher_is_deterministic_and_respects_family_bounds() {
        let a = Teacher::generate(TeacherFamily::Sparse, 7);
        assert_eq!(a, Teacher::generate(TeacherFamily::Sparse, 7));
        for gate in a.bank_gates.iter().flatten() {
            assert!((1..=6).contains(&gate.len()));
            assert!(gate.iter().all(|m| (1..=3).contains(&m.count_ones())));
        }
        let d = Teacher::generate(TeacherFamily::Dense, 7);
        assert!(d
            .bank_gates
            .iter()
            .flatten()
            .all(|g| (24..=40).contains(&g.len())));
        let data = Dataset::sample(&a, 4096, 1, 0.0);
        for b in 0..BANKS {
            let load = data.banks.iter().filter(|&&t| usize::from(t) == b).count();
            assert!(load > 800, "teacher bank {b} load {load}");
        }
    }

    #[test]
    fn anf_relaxation_is_exact_on_binary_points() {
        let model = Model::init(ExpertKind::Anf, 1);
        let mut rng = SplitMix64::new(3);
        for _ in 0..200 {
            let mut table = [0.0; MONOMIALS];
            let mut monomials = Vec::new();
            for (m, t) in table.iter_mut().enumerate() {
                if rng.below(5) == 0 {
                    *t = 1.0;
                    monomials.push(m as u16);
                }
            }
            let address = rng.below(MONOMIALS);
            let a: [f64; ADDRESS_BITS] =
                core::array::from_fn(|j| f64::from(((address >> j) & 1) as u8));
            let p = model.gate_forward(&table, &monomial_values(&a));
            assert_eq!(p, f64::from(u8::from(anf_eval(&monomials, address))));
        }
    }

    #[test]
    fn lut_multilinear_matches_table_and_mobius_on_binary_points() {
        let model = Model::init(ExpertKind::Lut, 1);
        let mut rng = SplitMix64::new(4);
        let mut table = [false; MONOMIALS];
        for t in &mut table {
            *t = rng.below(2) == 1;
        }
        let mut alpha: [f64; MONOMIALS] = core::array::from_fn(|u| f64::from(u8::from(table[u])));
        for bit in 0..ADDRESS_BITS {
            for m in 0..MONOMIALS {
                if m & (1 << bit) != 0 {
                    alpha[m] -= alpha[m ^ (1 << bit)];
                }
            }
        }
        let mut anf = table;
        gf2_mobius(&mut anf);
        let monomials: Vec<u16> = (0..MONOMIALS)
            .filter(|&m| anf[m])
            .map(|m| m as u16)
            .collect();
        for (address, &expected) in table.iter().enumerate() {
            let a: [f64; ADDRESS_BITS] =
                core::array::from_fn(|j| f64::from(((address >> j) & 1) as u8));
            let p = model.gate_forward(&alpha, &monomial_values(&a));
            assert!((p - f64::from(u8::from(expected))).abs() < 1.0e-12);
            assert_eq!(anf_eval(&monomials, address), expected);
        }
    }

    fn check_gradient(kind: ExpertKind, lambda: f64) {
        let teacher = Teacher::generate(TeacherFamily::Sparse, 11);
        let data = Dataset::sample(&teacher, 6, 12, 0.1);
        let mut model = perturbed(kind, 5);
        let sched = soft_sched();
        let (_, grad) =
            model.batch_loss_grad(&data.inputs, &data.labels, &sched, 0.1, lambda, None, &[]);
        let mut rng = SplitMix64::new(77);
        let probes: Vec<usize> = (0..40)
            .map(|i| {
                if i % 2 == 0 {
                    rng.below(SELECTOR_LEN)
                } else {
                    SELECTOR_LEN + rng.below(PARAM_LEN - SELECTOR_LEN)
                }
            })
            .collect();
        for k in probes {
            let h = 1.0e-5;
            let original = model.params[k];
            model.params[k] = original + h;
            let (up, _) =
                model.batch_loss_grad(&data.inputs, &data.labels, &sched, 0.1, lambda, None, &[]);
            model.params[k] = original - h;
            let (down, _) =
                model.batch_loss_grad(&data.inputs, &data.labels, &sched, 0.1, lambda, None, &[]);
            model.params[k] = original;
            let numeric = (up - down) / (2.0 * h);
            let tolerance = 1.0e-6 + 1.0e-4 * numeric.abs().max(grad[k].abs());
            assert!(
                (numeric - grad[k]).abs() < tolerance,
                "{kind:?} param {k}: analytic {} numeric {numeric}",
                grad[k]
            );
        }
    }

    #[test]
    fn anf_gradient_matches_finite_differences() {
        check_gradient(ExpertKind::Anf, 1.0e-3);
    }

    #[test]
    fn lut_gradient_matches_finite_differences() {
        check_gradient(ExpertKind::Lut, 0.0);
    }

    #[test]
    fn short_training_compiles_to_exact_pvp6_banks() {
        let teacher = Teacher::generate(TeacherFamily::Sparse, 21);
        let train = Dataset::sample(&teacher, 512, 22, 0.05);
        let val = Dataset::sample(&teacher, 128, 23, 0.05);
        let test = Dataset::sample(&teacher, 256, 24, 0.0);
        let config = TrainConfig {
            epochs: 8,
            batch: 128,
            ..TrainConfig::preregistered(1)
        };
        for arm in [ARMS[0], ARMS[3], ARMS[5]] {
            let (_, report) = train_and_report(&arm, &config, &train, &val, &test).unwrap();
            let live = report
                .expert_k
                .iter()
                .chain(&report.router_k)
                .any(|&k| k > 0);
            assert_eq!(report.parity_checks > 0, live, "{}", arm.name);
            assert_eq!(report.parity_mismatches, 0, "{}", arm.name);
            assert!(report.final_loss.is_finite());
        }
    }
}

#[cfg(test)]
mod diag {
    use super::*;

    /// Localization diagnostic: freeze some selectors at the teacher bits.
    #[test]
    #[ignore]
    fn diag_oracle_addresses() {
        let teacher = Teacher::generate(TeacherFamily::Sparse, 0x6B99);
        let train = Dataset::sample(&teacher, 16384, 1, 0.05);
        let val = Dataset::sample(&teacher, 4096, 2, 0.05);
        let freeze_router = std::env::var_os("FREEZE_ROUTER").is_some();
        let freeze_banks = std::env::var_os("FREEZE_BANKS").is_some();
        let epochs: usize = std::env::var("EPOCHS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30);
        for kind in [ExpertKind::Anf, ExpertKind::Lut] {
            let mut model = Model::init(kind, 1);
            let mut frozen = vec![false; PARAM_LEN];
            for unit in 0..UNITS {
                let is_router = unit == ROUTER_UNIT;
                if (is_router && !freeze_router) || (!is_router && !freeze_banks) {
                    continue;
                }
                let addr = if is_router {
                    teacher.router_address
                } else {
                    teacher.bank_addresses[unit]
                };
                for slot in 0..ADDRESS_BITS {
                    for i in 0..INPUT_BITS {
                        let k = selector_index(unit, slot, i);
                        model.params[k] = if i == addr[slot] { 30.0 } else { 0.0 };
                        frozen[k] = true;
                    }
                }
            }
            let config = TrainConfig {
                diversity: 0.5,
                ..TrainConfig::preregistered(1)
            };
            let mut m1 = vec![0.0; PARAM_LEN];
            let mut m2 = vec![0.0; PARAM_LEN];
            let mut step = 0;
            for epoch in 0..epochs {
                let progress = epoch as f64 / (epochs - 1) as f64;
                let sched = Schedule {
                    tau: 0.05_f64.powf(progress),
                    ste: false,
                    concrete_temperature: None,
                };
                for chunk in (0..train.inputs.len()).collect::<Vec<_>>().chunks(256) {
                    let xs: Vec<u32> = chunk.iter().map(|&i| train.inputs[i]).collect();
                    let ys: Vec<u8> = chunk.iter().map(|&i| train.labels[i]).collect();
                    let (l, grad) = model.batch_loss_grad(&xs, &ys, &sched, 0.1, 0.0, None, &[]);
                    let (_, grad) = model.add_diversity(l, grad, sched.tau, config.diversity);
                    step += 1;
                    let c1 = 1.0 - 0.9_f64.powi(step);
                    let c2 = 1.0 - 0.999_f64.powi(step);
                    for k in 0..PARAM_LEN {
                        if frozen[k] {
                            continue;
                        }
                        let g = grad[k];
                        m1[k] = 0.9 * m1[k] + 0.1 * g;
                        m2[k] = 0.999 * m2[k] + 0.001 * g * g;
                        model.params[k] -= 0.05 * (m1[k] / c1) / ((m2[k] / c2).sqrt() + 1.0e-8);
                    }
                }
                if epoch % 5 == 4 {
                    let hard = model.harden();
                    let (h, _) = bit_accuracy(|x| hard.predict(x).1, &val);
                    let (s, _) = bit_accuracy(|x| model.soft_predict(x, sched.tau), &val);
                    let k: usize = hard.gates.iter().map(Vec::len).sum();
                    eprintln!(
                        "{kind:?} e{epoch} soft {s:.4} hard {h:.4} k {k} addr {:?}",
                        hard.addresses
                    );
                }
            }
        }
        eprintln!(
            "teacher router {:?} banks {:?}",
            teacher.router_address, teacher.bank_addresses
        );
    }
}
