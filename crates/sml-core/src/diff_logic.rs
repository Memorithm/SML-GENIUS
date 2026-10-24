//! Differentiable two-input Boolean logic-gate networks (DLGN-0).
//!
//! Each gate reads two wires and owns four latent table parameters
//! `theta[k]`, one per row of a two-input truth table (`k = 2a + b`). The
//! relaxed table bit is `p[k] = sigmoid(theta[k])`; no softmax is used.
//!
//! Relaxed forward (training) evaluates the multilinear extension
//!
//! ```text
//! y = (1-a)(1-b) p0 + (1-a) b p1 + a (1-b) p2 + a b p3
//! ```
//!
//! which equals the expected output of a gate whose four truth-table bits are
//! drawn independently with probabilities `p`. Every one of the 16 two-input
//! Boolean functions is a corner of this parameter space (e.g. AND is
//! `p = (0,0,0,1)` giving `ab`, OR gives `a + b - ab`, XOR gives
//! `a + b - 2ab`).
//!
//! Hard forward (inference) discretises each table bit as `theta[k] > 0` and
//! evaluates 64 samples at once with `u64` bit-sliced words.
//!
//! `TrainMode::StraightThrough` uses the hard table bits in the forward pass
//! and the sigmoid derivative in the backward pass (straight-through
//! estimator), so training can observe the discretised network directly.
//!
//! Forward and backward passes are hand-written; there are no dependencies.
//! This module makes no claim beyond what its tests assert.

use core::fmt;

/// Errors raised by network construction and evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLogicError {
    /// A layer width of zero or an empty network was requested.
    EmptyLayer,
    /// The input width was zero.
    NoInputs,
    /// Input length did not match the network input width.
    InputWidth { expected: usize, actual: usize },
    /// Target length did not match the network output width.
    TargetWidth { expected: usize, actual: usize },
    /// A training batch was empty or ragged.
    BadBatch,
}

impl fmt::Display for DiffLogicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyLayer => write!(f, "network layers must be non-empty"),
            Self::NoInputs => write!(f, "network needs at least one input"),
            Self::InputWidth { expected, actual } => {
                write!(f, "expected {expected} inputs, got {actual}")
            }
            Self::TargetWidth { expected, actual } => {
                write!(f, "expected {expected} targets, got {actual}")
            }
            Self::BadBatch => write!(f, "batch must be non-empty and rectangular"),
        }
    }
}

impl std::error::Error for DiffLogicError {}

/// Forward mode used during training.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainMode {
    /// Table bits are `sigmoid(theta)`.
    Relaxed,
    /// Table bits are `theta > 0` forward; sigmoid gradient backward.
    StraightThrough,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Gate {
    a: usize,
    b: usize,
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// Deterministic splitmix64 used for wiring and initialisation.
fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A layered differentiable logic-gate network with fixed random wiring.
///
/// Layer `l` gates read wires from layer `l-1` (layer 0 reads the inputs).
/// The final layer's gate outputs are the network outputs.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffLogicNetwork {
    input_count: usize,
    layers: Vec<Vec<Gate>>,
    /// Flat parameters: 4 per gate, in layer then gate order.
    theta: Vec<f64>,
}

/// Cached activations of one relaxed forward pass, needed by `backward`.
#[derive(Debug, Clone)]
pub struct ForwardTrace {
    /// `values[0]` = inputs, `values[l+1]` = outputs of layer `l`.
    values: Vec<Vec<f64>>,
}

impl ForwardTrace {
    /// Network outputs.
    pub fn outputs(&self) -> &[f64] {
        self.values.last().map(Vec::as_slice).unwrap_or(&[])
    }
}

impl DiffLogicNetwork {
    /// Build a network with `widths[l]` gates in layer `l`.
    ///
    /// Each gate draws two distinct source wires from the previous layer
    /// (when it has at least two wires); parameters start at small
    /// deterministic values in `[-init_scale, init_scale]`.
    pub fn new(
        input_count: usize,
        widths: &[usize],
        seed: u64,
        init_scale: f64,
    ) -> Result<Self, DiffLogicError> {
        if input_count == 0 {
            return Err(DiffLogicError::NoInputs);
        }
        if widths.is_empty() || widths.contains(&0) {
            return Err(DiffLogicError::EmptyLayer);
        }
        let mut rng = seed;
        let mut layers = Vec::with_capacity(widths.len());
        let mut prev = input_count;
        for &w in widths {
            let mut layer = Vec::with_capacity(w);
            for g in 0..w {
                // Cover every source wire at least once before random picks.
                let a = if g < prev {
                    g
                } else {
                    (splitmix(&mut rng) % prev as u64) as usize
                };
                let mut b = (splitmix(&mut rng) % prev as u64) as usize;
                if prev > 1 && b == a {
                    b = (a + 1 + (splitmix(&mut rng) % (prev as u64 - 1)) as usize) % prev;
                }
                layer.push(Gate { a, b });
            }
            layers.push(layer);
            prev = w;
        }
        let gates: usize = widths.iter().sum();
        let theta = (0..gates * 4)
            .map(|_| {
                let u = (splitmix(&mut rng) >> 11) as f64 / (1u64 << 53) as f64;
                (2.0 * u - 1.0) * init_scale
            })
            .collect();
        Ok(Self {
            input_count,
            layers,
            theta,
        })
    }

    pub fn input_count(&self) -> usize {
        self.input_count
    }

    pub fn output_count(&self) -> usize {
        self.layers.last().map_or(0, Vec::len)
    }

    pub fn parameter_count(&self) -> usize {
        self.theta.len()
    }

    pub fn parameters(&self) -> &[f64] {
        &self.theta
    }

    pub fn parameters_mut(&mut self) -> &mut [f64] {
        &mut self.theta
    }

    fn table(&self, offset: usize, mode: TrainMode) -> [f64; 4] {
        let mut p = [0.0; 4];
        for (k, slot) in p.iter_mut().enumerate() {
            let t = self.theta[offset + k];
            *slot = match mode {
                TrainMode::Relaxed => sigmoid(t),
                TrainMode::StraightThrough => {
                    if t > 0.0 {
                        1.0
                    } else {
                        0.0
                    }
                }
            };
        }
        p
    }

    /// Relaxed or straight-through forward pass for one sample in `[0,1]^n`.
    pub fn forward(&self, inputs: &[f64], mode: TrainMode) -> Result<ForwardTrace, DiffLogicError> {
        if inputs.len() != self.input_count {
            return Err(DiffLogicError::InputWidth {
                expected: self.input_count,
                actual: inputs.len(),
            });
        }
        let mut values = Vec::with_capacity(self.layers.len() + 1);
        values.push(inputs.to_vec());
        let mut offset = 0;
        for layer in &self.layers {
            let prev = values.last().expect("non-empty");
            let mut out = Vec::with_capacity(layer.len());
            for gate in layer {
                let (a, b) = (prev[gate.a], prev[gate.b]);
                let p = self.table(offset, mode);
                out.push(
                    (1.0 - a) * (1.0 - b) * p[0]
                        + (1.0 - a) * b * p[1]
                        + a * (1.0 - b) * p[2]
                        + a * b * p[3],
                );
                offset += 4;
            }
            values.push(out);
        }
        Ok(ForwardTrace { values })
    }

    /// Accumulate `d loss / d theta` into `grad` given `d loss / d output`.
    ///
    /// In both modes the parameter gradient uses `sigmoid'(theta)`; in
    /// straight-through mode this is the estimator, not the true derivative.
    pub fn backward(&self, trace: &ForwardTrace, d_out: &[f64], mode: TrainMode, grad: &mut [f64]) {
        let _ = self.backward_with_input_grad(trace, d_out, mode, grad);
    }

    /// Like [`Self::backward`], and also returns `d loss / d inputs`, which
    /// lets callers chain the network inside a larger computation graph.
    pub fn backward_with_input_grad(
        &self,
        trace: &ForwardTrace,
        d_out: &[f64],
        mode: TrainMode,
        grad: &mut [f64],
    ) -> Vec<f64> {
        debug_assert_eq!(grad.len(), self.theta.len());
        let mut delta = d_out.to_vec();
        let mut offset_end = self.theta.len();
        for (l, layer) in self.layers.iter().enumerate().rev() {
            let offset_start = offset_end - 4 * layer.len();
            let prev = &trace.values[l];
            let mut d_prev = vec![0.0; prev.len()];
            for (g, gate) in layer.iter().enumerate() {
                let off = offset_start + 4 * g;
                let (a, b) = (prev[gate.a], prev[gate.b]);
                let p = self.table(off, mode);
                let d = delta[g];
                let coef = [(1.0 - a) * (1.0 - b), (1.0 - a) * b, a * (1.0 - b), a * b];
                for k in 0..4 {
                    let s = sigmoid(self.theta[off + k]);
                    grad[off + k] += d * coef[k] * s * (1.0 - s);
                }
                d_prev[gate.a] += d * ((1.0 - b) * (p[2] - p[0]) + b * (p[3] - p[1]));
                d_prev[gate.b] += d * ((1.0 - a) * (p[1] - p[0]) + a * (p[3] - p[2]));
            }
            delta = d_prev;
            offset_end = offset_start;
        }
        delta
    }

    /// Mean binary cross-entropy over a batch; writes the mean gradient.
    pub fn loss_and_grad(
        &self,
        inputs: &[Vec<f64>],
        targets: &[Vec<f64>],
        mode: TrainMode,
        grad: &mut [f64],
    ) -> Result<f64, DiffLogicError> {
        if inputs.is_empty() || inputs.len() != targets.len() {
            return Err(DiffLogicError::BadBatch);
        }
        grad.iter_mut().for_each(|g| *g = 0.0);
        let n = inputs.len() as f64;
        let mut loss = 0.0;
        const EPS: f64 = 1e-7;
        for (x, t) in inputs.iter().zip(targets) {
            if t.len() != self.output_count() {
                return Err(DiffLogicError::TargetWidth {
                    expected: self.output_count(),
                    actual: t.len(),
                });
            }
            let trace = self.forward(x, mode)?;
            let y = trace.outputs();
            let mut d_out = vec![0.0; y.len()];
            for ((yi, ti), di) in y.iter().zip(t).zip(d_out.iter_mut()) {
                let yc = yi.clamp(EPS, 1.0 - EPS);
                loss -= ti * yc.ln() + (1.0 - ti) * (1.0 - yc).ln();
                *di = (yc - ti) / (yc * (1.0 - yc)) / n;
            }
            self.backward(&trace, &d_out, mode, grad);
        }
        Ok(loss / n)
    }

    /// Freeze the discretised network (`theta > 0` per table bit).
    pub fn discretize(&self) -> HardLogicNetwork {
        let mut offset = 0;
        let layers = self
            .layers
            .iter()
            .map(|layer| {
                layer
                    .iter()
                    .map(|gate| {
                        let mut table = 0u8;
                        for k in 0..4 {
                            if self.theta[offset + k] > 0.0 {
                                table |= 1 << k;
                            }
                        }
                        offset += 4;
                        HardGate {
                            a: gate.a,
                            b: gate.b,
                            table,
                        }
                    })
                    .collect()
            })
            .collect();
        HardLogicNetwork {
            input_count: self.input_count,
            layers,
        }
    }
}

/// One discretised gate: truth-table bit `k = 2a + b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardGate {
    pub a: usize,
    pub b: usize,
    /// Low four bits hold the truth table.
    pub table: u8,
}

impl HardGate {
    /// Bit-sliced evaluation of 64 lanes.
    #[inline]
    pub fn eval_word(self, a: u64, b: u64) -> u64 {
        let mut out = 0;
        if self.table & 1 != 0 {
            out |= !a & !b;
        }
        if self.table & 2 != 0 {
            out |= !a & b;
        }
        if self.table & 4 != 0 {
            out |= a & !b;
        }
        if self.table & 8 != 0 {
            out |= a & b;
        }
        out
    }
}

/// Inference-time hard Boolean network; no floating point is executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardLogicNetwork {
    input_count: usize,
    layers: Vec<Vec<HardGate>>,
}

impl HardLogicNetwork {
    pub fn input_count(&self) -> usize {
        self.input_count
    }

    pub fn output_count(&self) -> usize {
        self.layers.last().map_or(0, Vec::len)
    }

    pub fn gate_count(&self) -> usize {
        self.layers.iter().map(Vec::len).sum()
    }

    /// Evaluate 64 samples at once: `inputs[i]` holds bit `i` of every lane.
    pub fn eval_packed(&self, inputs: &[u64]) -> Result<Vec<u64>, DiffLogicError> {
        if inputs.len() != self.input_count {
            return Err(DiffLogicError::InputWidth {
                expected: self.input_count,
                actual: inputs.len(),
            });
        }
        let mut cur = inputs.to_vec();
        for layer in &self.layers {
            cur = layer
                .iter()
                .map(|g| g.eval_word(cur[g.a], cur[g.b]))
                .collect();
        }
        Ok(cur)
    }

    /// Evaluate a single sample.
    pub fn eval_bits(&self, inputs: &[bool]) -> Result<Vec<bool>, DiffLogicError> {
        let words: Vec<u64> = inputs.iter().map(|&b| u64::from(b)).collect();
        Ok(self
            .eval_packed(&words)?
            .into_iter()
            .map(|w| w & 1 == 1)
            .collect())
    }
}

/// Plain SGD.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sgd {
    pub learning_rate: f64,
}

impl Sgd {
    pub fn step(&self, params: &mut [f64], grad: &[f64]) {
        for (p, g) in params.iter_mut().zip(grad) {
            *p -= self.learning_rate * g;
        }
    }
}

/// Adam with bias correction.
#[derive(Debug, Clone, PartialEq)]
pub struct Adam {
    pub learning_rate: f64,
    pub beta1: f64,
    pub beta2: f64,
    pub epsilon: f64,
    m: Vec<f64>,
    v: Vec<f64>,
    t: i32,
}

impl Adam {
    pub fn new(parameter_count: usize, learning_rate: f64) -> Self {
        Self {
            learning_rate,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
            m: vec![0.0; parameter_count],
            v: vec![0.0; parameter_count],
            t: 0,
        }
    }

    pub fn step(&mut self, params: &mut [f64], grad: &[f64]) {
        self.t += 1;
        let c1 = 1.0 - self.beta1.powi(self.t);
        let c2 = 1.0 - self.beta2.powi(self.t);
        for i in 0..params.len() {
            self.m[i] = self.beta1 * self.m[i] + (1.0 - self.beta1) * grad[i];
            self.v[i] = self.beta2 * self.v[i] + (1.0 - self.beta2) * grad[i] * grad[i];
            let mh = self.m[i] / c1;
            let vh = self.v[i] / c2;
            params[i] -= self.learning_rate * mh / (vh.sqrt() + self.epsilon);
        }
    }
}

/// Straight-through bit weights: latent reals, hard `{0,1}` forward.
///
/// Backward passes the upstream gradient through unchanged while
/// `|w| <= clip`, and zeroes it outside (the clipped STE).
#[derive(Debug, Clone, PartialEq)]
pub struct SteBits {
    pub latent: Vec<f64>,
    pub clip: f64,
}

impl SteBits {
    pub fn hard(&self) -> Vec<bool> {
        self.latent.iter().map(|&w| w > 0.0).collect()
    }

    /// Pack the hard bits into `u64` words (bit `i % 64` of word `i / 64`).
    pub fn packed(&self) -> Vec<u64> {
        let mut words = vec![0u64; self.latent.len().div_ceil(64)];
        for (i, &w) in self.latent.iter().enumerate() {
            if w > 0.0 {
                words[i / 64] |= 1 << (i % 64);
            }
        }
        words
    }

    /// Map `d loss / d hard_bit` to `d loss / d latent`.
    pub fn backward(&self, d_hard: &[f64]) -> Vec<f64> {
        self.latent
            .iter()
            .zip(d_hard)
            .map(|(&w, &d)| if w.abs() <= self.clip { d } else { 0.0 })
            .collect()
    }
}

/// Exhaustive truth-table dataset for an `n`-input Boolean function.
pub fn truth_table_dataset(
    n: usize,
    f: impl Fn(u32) -> Vec<bool>,
) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for row in 0..(1u32 << n) {
        xs.push((0..n).map(|i| f64::from((row >> i) & 1)).collect());
        ys.push(f(row).into_iter().map(|b| f64::from(u8::from(b))).collect());
    }
    (xs, ys)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn train(
        net: &mut DiffLogicNetwork,
        xs: &[Vec<f64>],
        ys: &[Vec<f64>],
        steps: usize,
        lr: f64,
        mode: TrainMode,
    ) -> (f64, f64) {
        let mut opt = Adam::new(net.parameter_count(), lr);
        let mut grad = vec![0.0; net.parameter_count()];
        let first = net.loss_and_grad(xs, ys, mode, &mut grad).unwrap();
        let mut last = first;
        for _ in 0..steps {
            last = net.loss_and_grad(xs, ys, mode, &mut grad).unwrap();
            let mut p = net.parameters().to_vec();
            opt.step(&mut p, &grad);
            net.parameters_mut().copy_from_slice(&p);
        }
        (first, last)
    }

    fn hard_matches_target(hard: &HardLogicNetwork, n: usize, ys: &[Vec<f64>]) -> bool {
        // Pack every row of the truth table into one 64-lane word batch.
        let inputs: Vec<u64> = (0..n)
            .map(|i| {
                (0..(1u64 << n))
                    .filter(|r| (r >> i) & 1 == 1)
                    .fold(0, |w, r| w | (1 << r))
            })
            .collect();
        let out = hard.eval_packed(&inputs).unwrap();
        ys.iter().enumerate().all(|(r, y)| {
            y.iter()
                .zip(&out)
                .all(|(t, w)| ((w >> r) & 1 == 1) == (*t > 0.5))
        })
    }

    #[test]
    fn relaxed_gate_corners_are_the_16_boolean_functions() {
        let mut net = DiffLogicNetwork::new(2, &[1], 0, 0.0).unwrap();
        // Force wiring a=input0, b=input1 regardless of seed.
        net.layers[0][0] = Gate { a: 0, b: 1 };
        for table in 0u8..16 {
            for k in 0..4 {
                net.theta[k] = if table >> k & 1 == 1 { 40.0 } else { -40.0 };
            }
            let hard = net.discretize();
            for row in 0..4u32 {
                let (a, b) = ((row >> 1) & 1, row & 1);
                let expect = (table >> row) & 1 == 1;
                let y = net
                    .forward(&[f64::from(a), f64::from(b)], TrainMode::Relaxed)
                    .unwrap()
                    .outputs()[0];
                assert!((y - f64::from(u8::from(expect))).abs() < 1e-9);
                assert_eq!(hard.eval_bits(&[a == 1, b == 1]).unwrap(), vec![expect]);
            }
        }
    }

    #[test]
    fn relaxed_and_or_xor_match_probabilistic_formulas() {
        let mut net = DiffLogicNetwork::new(2, &[1], 0, 0.0).unwrap();
        net.layers[0][0] = Gate { a: 0, b: 1 };
        let (a, b) = (0.3, 0.8);
        let set = |net: &mut DiffLogicNetwork, t: u8| {
            for k in 0..4 {
                net.theta[k] = if t >> k & 1 == 1 { 60.0 } else { -60.0 };
            }
        };
        let eval =
            |net: &DiffLogicNetwork| net.forward(&[a, b], TrainMode::Relaxed).unwrap().outputs()[0];
        set(&mut net, 0b1000);
        assert!((eval(&net) - a * b).abs() < 1e-12);
        set(&mut net, 0b1110);
        assert!((eval(&net) - (a + b - a * b)).abs() < 1e-12);
        set(&mut net, 0b0110);
        assert!((eval(&net) - (a + b - 2.0 * a * b)).abs() < 1e-12);
    }

    #[test]
    fn backward_matches_finite_differences() {
        let net = DiffLogicNetwork::new(3, &[4, 3, 2], 7, 1.5).unwrap();
        let xs = vec![vec![0.2, 0.9, 0.4], vec![1.0, 0.0, 1.0]];
        let ys = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let mut grad = vec![0.0; net.parameter_count()];
        net.loss_and_grad(&xs, &ys, TrainMode::Relaxed, &mut grad)
            .unwrap();
        let mut scratch = vec![0.0; net.parameter_count()];
        let h = 1e-6;
        for (i, &gi) in grad.iter().enumerate() {
            let mut plus = net.clone();
            plus.theta[i] += h;
            let mut minus = net.clone();
            minus.theta[i] -= h;
            let lp = plus
                .loss_and_grad(&xs, &ys, TrainMode::Relaxed, &mut scratch)
                .unwrap();
            let lm = minus
                .loss_and_grad(&xs, &ys, TrainMode::Relaxed, &mut scratch)
                .unwrap();
            let fd = (lp - lm) / (2.0 * h);
            assert!((fd - gi).abs() < 1e-6, "param {i}: fd {fd} vs {gi}");
        }
    }

    #[test]
    fn learns_xor_and_hard_mode_matches_after_discretization() {
        let (xs, ys) = truth_table_dataset(2, |r| vec![(r & 1) ^ (r >> 1 & 1) == 1]);
        let mut net = DiffLogicNetwork::new(2, &[1], 1, 0.1).unwrap();
        let (first, last) = train(&mut net, &xs, &ys, 300, 0.1, TrainMode::Relaxed);
        assert!(last < first * 0.05, "loss {first} -> {last}");
        let hard = net.discretize();
        assert!(hard_matches_target(&hard, 2, &ys));
        // Rounded relaxed outputs agree with the hard network on every row.
        for x in &xs {
            let soft = net.forward(x, TrainMode::Relaxed).unwrap().outputs()[0] > 0.5;
            let bits: Vec<bool> = x.iter().map(|v| *v > 0.5).collect();
            assert_eq!(hard.eval_bits(&bits).unwrap()[0], soft);
        }
    }

    #[test]
    fn learns_three_bit_parity_with_a_two_layer_network() {
        let (xs, ys) = truth_table_dataset(3, |r| vec![r.count_ones() % 2 == 1]);
        // Search a few deterministic wiring seeds; the claim is only that
        // at least one of them trains to an exact hard parity circuit.
        let mut solved = false;
        for seed in 0..16 {
            let mut net = DiffLogicNetwork::new(3, &[6, 3, 1], seed, 0.5).unwrap();
            let (first, last) = train(&mut net, &xs, &ys, 1500, 0.05, TrainMode::Relaxed);
            assert!(last < first, "seed {seed}: loss did not decrease");
            let hard = net.discretize();
            if hard_matches_target(&hard, 3, &ys) {
                for x in &xs {
                    let soft = net.forward(x, TrainMode::Relaxed).unwrap().outputs()[0] > 0.5;
                    let bits: Vec<bool> = x.iter().map(|v| *v > 0.5).collect();
                    assert_eq!(hard.eval_bits(&bits).unwrap()[0], soft);
                }
                solved = true;
                break;
            }
        }
        assert!(solved, "no seed produced an exact hard parity network");
    }

    #[test]
    fn straight_through_training_reaches_exact_hard_function() {
        // Two-output target: (a AND b, a OR c).
        let (xs, ys) = truth_table_dataset(3, |r| {
            let (a, b, c) = (r & 1 == 1, r >> 1 & 1 == 1, r >> 2 & 1 == 1);
            vec![a && b, a || c]
        });
        // Search deterministic wiring seeds; the claim is only that some
        // seed reaches an exact hard circuit and that the straight-through
        // fine-tune does not increase the hard-forward loss for it.
        let mut solved = false;
        for seed in 0..16 {
            let mut net = DiffLogicNetwork::new(3, &[6, 2], seed, 0.5).unwrap();
            train(&mut net, &xs, &ys, 800, 0.05, TrainMode::Relaxed);
            let mut grad = vec![0.0; net.parameter_count()];
            let before = net
                .loss_and_grad(&xs, &ys, TrainMode::StraightThrough, &mut grad)
                .unwrap();
            let (_, _) = train(&mut net, &xs, &ys, 200, 0.05, TrainMode::StraightThrough);
            let after = net
                .loss_and_grad(&xs, &ys, TrainMode::StraightThrough, &mut grad)
                .unwrap();
            if after <= before && hard_matches_target(&net.discretize(), 3, &ys) {
                solved = true;
                break;
            }
        }
        assert!(solved, "no seed reached an exact hard circuit");
    }

    #[test]
    fn sgd_decreases_loss() {
        let (xs, ys) = truth_table_dataset(2, |r| vec![r == 3]);
        let mut net = DiffLogicNetwork::new(2, &[1], 2, 0.1).unwrap();
        let mut grad = vec![0.0; net.parameter_count()];
        let first = net
            .loss_and_grad(&xs, &ys, TrainMode::Relaxed, &mut grad)
            .unwrap();
        let sgd = Sgd { learning_rate: 2.0 };
        for _ in 0..200 {
            net.loss_and_grad(&xs, &ys, TrainMode::Relaxed, &mut grad)
                .unwrap();
            sgd.step(net.parameters_mut(), &grad);
        }
        let last = net
            .loss_and_grad(&xs, &ys, TrainMode::Relaxed, &mut grad)
            .unwrap();
        assert!(last < first * 0.2);
    }

    #[test]
    fn ste_bits_pack_and_clip() {
        let bits = SteBits {
            latent: vec![0.5, -0.2, 3.0, 0.0],
            clip: 1.0,
        };
        assert_eq!(bits.hard(), vec![true, false, true, false]);
        assert_eq!(bits.packed(), vec![0b0101]);
        assert_eq!(
            bits.backward(&[1.0, 2.0, 3.0, 4.0]),
            vec![1.0, 2.0, 0.0, 4.0]
        );
    }

    #[test]
    fn shape_errors_fail_closed() {
        assert_eq!(
            DiffLogicNetwork::new(0, &[1], 0, 0.1),
            Err(DiffLogicError::NoInputs)
        );
        assert_eq!(
            DiffLogicNetwork::new(2, &[], 0, 0.1),
            Err(DiffLogicError::EmptyLayer)
        );
        let net = DiffLogicNetwork::new(2, &[1], 0, 0.1).unwrap();
        assert!(net.forward(&[0.0], TrainMode::Relaxed).is_err());
        assert!(net.discretize().eval_packed(&[0]).is_err());
    }
}
