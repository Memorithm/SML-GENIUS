//! Local fan-in ANF recurrent layer (A5 toy learning prototype).
//!
//! Each gate reads only `k <= 8` positions of the source vector
//! `[state_0, ..., state_{N-1}, x]` (the recurrent state plus the current input
//! bit) and computes the GF(2) polynomial
//! `f(u) = XOR_{m : c_m = 1} AND_{j in m} u_j` over its `2^k` monomials. The
//! `2^k` coefficient bits `c_m` are the learned parameters. Because a gate never
//! addresses the whole state, the state width `N` is not limited by the 31-bit
//! address of `anf_large`; only the per-gate fan-in is bounded.
//!
//! Evaluation is bit-sliced: one `u64` lane word carries the same state bit for
//! 64 independent sequences, and every monomial product is one `AND` over lanes.

use crate::semantic::{SemanticRewriteStep, SemanticRewriter, SemanticState};
use crate::toy_tasks::SplitMix64;

/// Maximum per-gate fan-in (2^8 = 256 coefficient bits per gate).
pub const LOCAL_ANF_MAX_FAN_IN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalAnfError {
    /// Fan-in is zero or above [`LOCAL_ANF_MAX_FAN_IN`].
    FanIn(usize),
    /// A gate input index is outside the source vector, or repeated.
    Input(usize),
    /// Coefficient storage does not match `2^k` bits.
    Coefficients,
    /// State width is zero, too small for the wiring, or does not match.
    Width(usize),
    /// Recurrent depth must advance by exactly one per cycle.
    Depth,
}

impl core::fmt::Display for LocalAnfError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FanIn(k) => write!(f, "local ANF fan-in {k} outside 1..={LOCAL_ANF_MAX_FAN_IN}"),
            Self::Input(i) => write!(f, "local ANF input {i} out of range or repeated"),
            Self::Coefficients => {
                write!(f, "local ANF coefficient storage does not match 2^k bits")
            }
            Self::Width(w) => write!(f, "local ANF width {w} is invalid"),
            Self::Depth => write!(f, "local ANF recurrent depth must advance by one"),
        }
    }
}

impl std::error::Error for LocalAnfError {}

/// One local ANF gate with frozen wiring and learnable coefficients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAnfGate {
    inputs: Vec<usize>,
    coefficients: Vec<u64>,
}

fn coefficient_words(fan_in: usize) -> usize {
    (1usize << fan_in).div_ceil(64)
}

impl LocalAnfGate {
    /// `inputs[j]` is the source position read as local variable `j`;
    /// bit `m` of `coefficients` is the coefficient of monomial `m`.
    pub fn new(
        inputs: Vec<usize>,
        coefficients: Vec<u64>,
        source_width: usize,
    ) -> Result<Self, LocalAnfError> {
        let k = inputs.len();
        if k == 0 || k > LOCAL_ANF_MAX_FAN_IN {
            return Err(LocalAnfError::FanIn(k));
        }
        for (j, &input) in inputs.iter().enumerate() {
            if input >= source_width || inputs[..j].contains(&input) {
                return Err(LocalAnfError::Input(input));
            }
        }
        if coefficients.len() != coefficient_words(k) {
            return Err(LocalAnfError::Coefficients);
        }
        if k < 6 && coefficients[0] >> (1u32 << k) != 0 {
            return Err(LocalAnfError::Coefficients);
        }
        Ok(Self {
            inputs,
            coefficients,
        })
    }
    pub fn zero(inputs: Vec<usize>, source_width: usize) -> Result<Self, LocalAnfError> {
        let words = coefficient_words(inputs.len().clamp(1, LOCAL_ANF_MAX_FAN_IN));
        Self::new(inputs, vec![0; words], source_width)
    }
    /// Builds the gate whose truth table is `table` (`table[u]` for local address `u`).
    pub fn from_truth_table(
        inputs: Vec<usize>,
        table: &[bool],
        source_width: usize,
    ) -> Result<Self, LocalAnfError> {
        let mut gate = Self::zero(inputs, source_width)?;
        if table.len() != gate.monomial_count() {
            return Err(LocalAnfError::Coefficients);
        }
        for (m, value) in mobius(table).into_iter().enumerate() {
            if value {
                gate.flip(m);
            }
        }
        Ok(gate)
    }
    pub fn fan_in(&self) -> usize {
        self.inputs.len()
    }
    pub fn inputs(&self) -> &[usize] {
        &self.inputs
    }
    pub fn monomial_count(&self) -> usize {
        1 << self.inputs.len()
    }
    pub fn coefficient(&self, monomial: usize) -> bool {
        (self.coefficients[monomial / 64] >> (monomial % 64)) & 1 == 1
    }
    /// XOR one coefficient (panics if `monomial >= 2^k`).
    pub fn flip(&mut self, monomial: usize) {
        assert!(monomial < self.monomial_count());
        self.coefficients[monomial / 64] ^= 1u64 << (monomial % 64);
    }
    pub fn nonzero_monomials(&self) -> usize {
        self.coefficients
            .iter()
            .map(|w| w.count_ones() as usize)
            .sum()
    }
    /// Literal ANF evaluation on one source vector (reference semantics).
    pub fn evaluate(&self, source: &[bool]) -> bool {
        let address = self
            .inputs
            .iter()
            .enumerate()
            .fold(0usize, |a, (j, &i)| a | (usize::from(source[i]) << j));
        (0..self.monomial_count())
            .filter(|&m| self.coefficient(m) && m & address == m)
            .count()
            % 2
            == 1
    }
    /// Bit-sliced evaluation over 64 lanes; `products` is caller scratch of
    /// at least `2^k` words.
    pub fn evaluate_lanes(&self, source: &[u64], products: &mut [u64]) -> u64 {
        let count = self.monomial_count();
        products[0] = u64::MAX;
        let mut acc = if self.coefficient(0) { u64::MAX } else { 0 };
        for m in 1..count {
            let low = m.trailing_zeros() as usize;
            products[m] = products[m & (m - 1)] & source[self.inputs[low]];
            if self.coefficient(m) {
                acc ^= products[m];
            }
        }
        acc
    }
    /// Bit-sliced evaluation where `values[j]` is the lane word of local input
    /// `j` (already gathered by the caller); same result as [`Self::evaluate_lanes`].
    pub fn evaluate_gathered(&self, values: &[u64], products: &mut [u64]) -> u64 {
        let count = self.monomial_count();
        products[0] = u64::MAX;
        let mut acc = if self.coefficient(0) { u64::MAX } else { 0 };
        for m in 1..count {
            let low = m.trailing_zeros() as usize;
            products[m] = products[m & (m - 1)] & values[low];
            if self.coefficient(m) {
                acc ^= products[m];
            }
        }
        acc
    }
    /// Truth table `table[u]` for every local address `u` (Möbius transform).
    pub fn truth_table(&self) -> Vec<bool> {
        let coefficients: Vec<bool> = (0..self.monomial_count())
            .map(|m| self.coefficient(m))
            .collect();
        mobius(&coefficients)
    }
}

/// GF(2) subset (Möbius/zeta) transform; it is its own inverse.
fn mobius(values: &[bool]) -> Vec<bool> {
    let mut out = values.to_vec();
    let mut step = 1;
    while step < out.len() {
        for m in 0..out.len() {
            if m & step != 0 {
                out[m] ^= out[m ^ step];
            }
        }
        step <<= 1;
    }
    out
}

/// Wiring and size parameters of a [`LocalAnfNetwork`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalAnfShape {
    /// Recurrent state bits `N`.
    pub state_bits: usize,
    /// Fan-in `k` of every state gate (>= 2: input bit + own previous bit + k-2 random).
    pub fan_in: usize,
    /// Fan-in of the readout gate (input bit + the first `readout_fan_in - 1` state bits).
    pub readout_fan_in: usize,
    /// Frozen wiring prior of the state gates.
    pub wiring: LocalAnfWiring,
}

/// How the `k - 2` (or `k - 3`) free inputs of each state gate are chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalAnfWiring {
    /// Gate `g` reads `x`, `s_g` and `k-2` distinct uniformly random state bits.
    Random,
    /// Gate `g` reads `x`, `s_g`, its neighbour `s_{g-1}` (for `g > 0`) and
    /// random state bits for the remaining inputs (1-D locality prior).
    Local,
}

/// Recurrent layer `s_{t+1} = F(s_t, x_t)` plus readout `y_t = R(s_{t+1}, x_t)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAnfNetwork {
    state_bits: usize,
    /// Input bits per step, at source positions `N..N + input_bits` (1 for the toy tasks).
    input_bits: usize,
    gates: Vec<LocalAnfGate>,
    readout: LocalAnfGate,
}

impl LocalAnfNetwork {
    pub fn new(
        state_bits: usize,
        gates: Vec<LocalAnfGate>,
        readout: LocalAnfGate,
    ) -> Result<Self, LocalAnfError> {
        Self::with_input_bits(state_bits, 1, gates, readout)
    }

    /// Network whose source vector is `[s_0..s_{N-1}, x_0..x_{W-1}]` with
    /// `W = input_bits` input bits per step (e.g. 8 bit-planes for bytes).
    pub fn with_input_bits(
        state_bits: usize,
        input_bits: usize,
        gates: Vec<LocalAnfGate>,
        readout: LocalAnfGate,
    ) -> Result<Self, LocalAnfError> {
        if state_bits == 0 || gates.len() != state_bits || input_bits == 0 {
            return Err(LocalAnfError::Width(state_bits));
        }
        let width = state_bits + input_bits;
        for gate in gates.iter().chain(core::iter::once(&readout)) {
            if let Some(&bad) = gate.inputs.iter().find(|&&i| i >= width) {
                return Err(LocalAnfError::Input(bad));
            }
        }
        Ok(Self {
            state_bits,
            input_bits,
            gates,
            readout,
        })
    }

    /// Seeded frozen wiring. Gate `g` reads `x` (local variable 0), its own
    /// previous bit `s_g` (variable 1) and other state bits per [`LocalAnfWiring`].
    /// Initial coefficients: the identity monomial `{s_g}` plus each other
    /// monomial with probability `2 / 2^k` (about two random terms per gate).
    pub fn random(shape: LocalAnfShape, seed: u64) -> Result<Self, LocalAnfError> {
        let n = shape.state_bits;
        let k = shape.fan_in;
        if !(2..=LOCAL_ANF_MAX_FAN_IN).contains(&k) {
            return Err(LocalAnfError::FanIn(k));
        }
        if n < k - 1 || shape.readout_fan_in == 0 || shape.readout_fan_in > n + 1 {
            return Err(LocalAnfError::Width(n));
        }
        let mut rng = SplitMix64::new(seed);
        let mut gates = Vec::with_capacity(n);
        for g in 0..n {
            let mut inputs = vec![n, g];
            if shape.wiring == LocalAnfWiring::Local && g > 0 && k > 2 {
                inputs.push(g - 1);
            }
            while inputs.len() < k {
                let candidate = rng.below(n);
                if !inputs.contains(&candidate) {
                    inputs.push(candidate);
                }
            }
            let mut gate = LocalAnfGate::zero(inputs, n + 1)?;
            gate.flip(0b10);
            for m in 0..gate.monomial_count() {
                if rng.below(gate.monomial_count()) < 2 {
                    gate.flip(m);
                }
            }
            gates.push(gate);
        }
        let mut readout_inputs = vec![n];
        readout_inputs.extend(0..shape.readout_fan_in - 1);
        let readout = LocalAnfGate::zero(readout_inputs, n + 1)?;
        Self::new(n, gates, readout)
    }

    pub fn state_bits(&self) -> usize {
        self.state_bits
    }
    /// Input bits per step (`W`); source positions `N..N + W`.
    pub fn input_bits(&self) -> usize {
        self.input_bits
    }
    /// Source vector width `N + W`.
    pub fn source_width(&self) -> usize {
        self.state_bits + self.input_bits
    }
    pub fn gates(&self) -> &[LocalAnfGate] {
        &self.gates
    }
    pub fn gates_mut(&mut self) -> &mut [LocalAnfGate] {
        &mut self.gates
    }
    pub fn readout(&self) -> &LocalAnfGate {
        &self.readout
    }
    pub fn set_readout(&mut self, readout: LocalAnfGate) -> Result<(), LocalAnfError> {
        let width = self.source_width();
        if let Some(&bad) = readout.inputs.iter().find(|&&i| i >= width) {
            return Err(LocalAnfError::Input(bad));
        }
        self.readout = readout;
        Ok(())
    }
    /// Learned coefficient bits (state gates + readout).
    pub fn learned_bits(&self) -> usize {
        self.gates
            .iter()
            .chain(core::iter::once(&self.readout))
            .map(LocalAnfGate::monomial_count)
            .sum()
    }
    /// State/input bits read per token (sum of fan-ins, gates + readout).
    pub fn bits_read_per_token(&self) -> usize {
        self.gates
            .iter()
            .chain(core::iter::once(&self.readout))
            .map(LocalAnfGate::fan_in)
            .sum()
    }
    /// Largest scratch needed by [`LocalAnfGate::evaluate_lanes`].
    pub fn scratch_len(&self) -> usize {
        self.gates
            .iter()
            .chain(core::iter::once(&self.readout))
            .map(LocalAnfGate::monomial_count)
            .max()
            .unwrap_or(1)
    }

    /// Scalar reference step: returns `(s_{t+1}, y_t)`.
    pub fn step(&self, state: &[bool], x: bool) -> (Vec<bool>, bool) {
        assert_eq!(state.len(), self.state_bits);
        assert_eq!(self.input_bits, 1, "scalar reference step is single-input");
        let mut source = state.to_vec();
        source.push(x);
        let mut next: Vec<bool> = self.gates.iter().map(|g| g.evaluate(&source)).collect();
        next.push(x);
        let y = self.readout.evaluate(&next);
        next.pop();
        (next, y)
    }
    /// Runs one sequence from the all-zero state; returns `y_t` for every step.
    pub fn run(&self, inputs: &[bool]) -> Vec<bool> {
        let mut state = vec![false; self.state_bits];
        inputs
            .iter()
            .map(|&x| {
                let (next, y) = self.step(&state, x);
                state = next;
                y
            })
            .collect()
    }
    /// Bit-sliced step: `source` holds `N` state words followed by the `W`
    /// input words; it is overwritten with `s_{t+1}` (input words kept).
    pub fn step_lanes(&self, source: &mut [u64], next: &mut [u64], products: &mut [u64]) {
        for (slot, gate) in next.iter_mut().zip(&self.gates) {
            *slot = gate.evaluate_lanes(source, products);
        }
        source[..self.state_bits].copy_from_slice(&next[..self.state_bits]);
    }
}

/// Plugs a [`LocalAnfNetwork`] into the SBG recurrent-loop interface.
///
/// `value[0..N]` holds the recurrent state and `value[N]` the current input bit
/// (written by the caller before each cycle). One rewrite replaces `value[0..N]`
/// with the next state; the input slot and the other SBG fields are preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalAnfRewriter<const S: usize> {
    network: LocalAnfNetwork,
    cycles: u32,
}

impl<const S: usize> LocalAnfRewriter<S> {
    pub fn new(network: LocalAnfNetwork) -> Result<Self, LocalAnfError> {
        if network.state_bits() >= S {
            return Err(LocalAnfError::Width(S));
        }
        Ok(Self { network, cycles: 0 })
    }
    pub fn input_slot(&self) -> usize {
        self.network.state_bits()
    }
    /// Readout `y_t` on a state produced by [`SemanticRewriter::rewrite`].
    pub fn output(&self, state: &SemanticState<S>) -> Result<bool, LocalAnfError> {
        let source = self.source(state)?;
        Ok(self.network.readout.evaluate(&source))
    }
    fn source(&self, state: &SemanticState<S>) -> Result<Vec<bool>, LocalAnfError> {
        (0..=self.network.state_bits())
            .map(|i| state.value.get(i).map_err(|_| LocalAnfError::Width(S)))
            .collect()
    }
}

impl<const S: usize> SemanticRewriter<S> for LocalAnfRewriter<S> {
    type Error = LocalAnfError;
    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        if Some(depth) != self.cycles.checked_add(1) {
            return Err(LocalAnfError::Depth);
        }
        let source = self.source(state)?;
        let mut next = state.clone();
        for (i, gate) in self.network.gates.iter().enumerate() {
            next.value
                .set(i, gate.evaluate(&source))
                .map_err(|_| LocalAnfError::Width(S))?;
        }
        self.cycles = depth;
        Ok(SemanticRewriteStep {
            state: next,
            stability_blocked: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape() -> LocalAnfShape {
        LocalAnfShape {
            state_bits: 24,
            fan_in: 5,
            readout_fan_in: 4,
            wiring: LocalAnfWiring::Random,
        }
    }

    #[test]
    fn fan_in_is_respected_and_validated() {
        let net = LocalAnfNetwork::random(shape(), 3).unwrap();
        for (g, gate) in net.gates().iter().enumerate() {
            assert_eq!(gate.fan_in(), 5);
            assert_eq!(&gate.inputs()[..2], &[24, g]);
            let mut sorted = gate.inputs().to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), 5);
        }
        assert_eq!(net.learned_bits(), 24 * 32 + 16);
        assert_eq!(net.bits_read_per_token(), 24 * 5 + 4);
        assert_eq!(
            LocalAnfGate::zero(vec![0; 9], 10),
            Err(LocalAnfError::FanIn(9))
        );
        assert_eq!(
            LocalAnfGate::zero(vec![1, 1], 10),
            Err(LocalAnfError::Input(1))
        );
        assert_eq!(
            LocalAnfGate::zero(vec![10], 10),
            Err(LocalAnfError::Input(10))
        );
    }

    #[test]
    fn gate_output_depends_only_on_wired_inputs() {
        let net = LocalAnfNetwork::random(shape(), 11).unwrap();
        let mut rng = SplitMix64::new(5);
        for gate in net.gates() {
            for _ in 0..32 {
                let a: Vec<bool> = (0..25).map(|_| rng.next_bool()).collect();
                let mut b: Vec<bool> = (0..25).map(|_| rng.next_bool()).collect();
                for &i in gate.inputs() {
                    b[i] = a[i];
                }
                assert_eq!(gate.evaluate(&a), gate.evaluate(&b));
            }
        }
    }

    #[test]
    fn truth_table_round_trip_and_lane_evaluation_match_literal_anf() {
        let mut rng = SplitMix64::new(9);
        for k in 1..=LOCAL_ANF_MAX_FAN_IN {
            let table: Vec<bool> = (0..1usize << k).map(|_| rng.next_bool()).collect();
            let inputs: Vec<usize> = (0..k).map(|j| (j * 3) % 11).collect();
            let gate = LocalAnfGate::from_truth_table(inputs.clone(), &table, 11).unwrap();
            assert_eq!(gate.truth_table(), table);
            let lanes: Vec<u64> = (0..11).map(|_| rng.next_u64()).collect();
            let mut products = vec![0; 1 << k];
            let packed = gate.evaluate_lanes(&lanes, &mut products);
            for lane in 0..64 {
                let source: Vec<bool> = lanes.iter().map(|w| (w >> lane) & 1 == 1).collect();
                let address = inputs
                    .iter()
                    .enumerate()
                    .fold(0, |a, (j, &i)| a | (usize::from(source[i]) << j));
                assert_eq!((packed >> lane) & 1 == 1, table[address]);
                assert_eq!(gate.evaluate(&source), table[address]);
            }
        }
    }

    #[test]
    fn rewriter_in_recurrent_interface_matches_reference_run() {
        let net = LocalAnfNetwork::random(shape(), 4).unwrap();
        let mut rewriter = LocalAnfRewriter::<32>::new(net.clone()).unwrap();
        let inputs: Vec<bool> = (0..40).map(|t| (t * 7 + t / 3) % 5 < 2).collect();
        let expected = net.run(&inputs);
        let mut state = SemanticState::<32>::zeroed().unwrap();
        state.routing.set(31, true).unwrap();
        for (t, &x) in inputs.iter().enumerate() {
            state.value.set(rewriter.input_slot(), x).unwrap();
            state = rewriter.rewrite(&state, t as u32 + 1).unwrap().state;
            assert_eq!(rewriter.output(&state).unwrap(), expected[t]);
            assert!(state.routing.get(31).unwrap());
        }
        assert_eq!(rewriter.rewrite(&state, 1), Err(LocalAnfError::Depth));
        assert_eq!(
            LocalAnfRewriter::<24>::new(net).map(|_| ()),
            Err(LocalAnfError::Width(24))
        );
    }
}
