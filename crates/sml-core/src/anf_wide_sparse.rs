//! Elastic native GF(2) ANF gate with a dense k<=8 fast path and a wide,
//! sparse, bit-sliced execution path. No 2^k table exists for sparse gates.
//!
//! A sparse monomial is stored as ceil(k/64) packed mask words. A compiled
//! prefix DAG shares intermediate ANDs between monomials. Its caller-owned
//! scratch is reused across 64-lane evaluations and recurrent state steps.
//! This is an execution primitive, not a fitted wide-gate learner.

use std::collections::BTreeMap;
use std::mem::size_of;

use crate::anf_large::LargeAnfGate;
use crate::anf_local::{LocalAnfError, LocalAnfGate, LocalAnfNetwork};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideAnfLimits {
    /// Explicit caller-controlled safety ceiling, not an algebraic width cap.
    pub max_fan_in: usize,
    pub max_monomials: usize,
    pub max_term_input_refs: usize,
    pub max_stored_mask_words: usize,
    pub max_prefix_nodes: usize,
}

impl WideAnfLimits {
    pub const fn new(
        max_fan_in: usize,
        max_monomials: usize,
        max_term_input_refs: usize,
        max_stored_mask_words: usize,
        max_prefix_nodes: usize,
    ) -> Self {
        Self {
            max_fan_in,
            max_monomials,
            max_term_input_refs,
            max_stored_mask_words,
            max_prefix_nodes,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WideAnfError {
    EmptyFanIn,
    BudgetExceeded,
    Allocation,
    InputOutOfRange(usize),
    DuplicateInput(usize),
    MonomialOutOfRange(usize),
    DuplicateMonomialInput(usize),
    DuplicateMonomial,
    SourceTooShort,
    ScratchTooShort,
    InvalidNetwork,
    Local(LocalAnfError),
}

impl From<LocalAnfError> for WideAnfError {
    fn from(e: LocalAnfError) -> Self {
        Self::Local(e)
    }
}

impl std::fmt::Display for WideAnfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WideAnfError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WideAnfBackend {
    Dense,
    Sparse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PrefixNode {
    /// Node 0 is the implicit constant 1; stored nodes are numbered 1..=len.
    parent: usize,
    /// Local variable number (index into the gate's dynamic inputs).
    variable: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SparseBody {
    /// Canonical wide masks: no dense 2^k coefficients ever allocated.
    masks: Vec<Vec<u64>>,
    /// Share the AND of sorted variable prefixes between active monomials.
    nodes: Vec<PrefixNode>,
    /// Terminal node for every active monomial, including node 0 = constant.
    terminals: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Execution {
    Dense(LocalAnfGate),
    Sparse(SparseBody),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WideAnfGate {
    inputs: Vec<usize>,
    /// Cached exact global source bound; no O(k) validation in the hot loop.
    required_source_len: usize,
    active_monomials: usize,
    execution: Execution,
}

impl WideAnfGate {
    /// Monomials are lists of gate-local variables; [] is constant one and
    /// an empty list of monomials means the zero function. Inputs address
    /// the caller's global source vector. All bounds are checked up front.
    pub fn from_monomials(
        inputs: Vec<usize>,
        monomials: Vec<Vec<usize>>,
        source_width: usize,
        limits: WideAnfLimits,
    ) -> Result<Self, WideAnfError> {
        let k = inputs.len();
        if k == 0 {
            return Err(WideAnfError::EmptyFanIn);
        }
        if k > limits.max_fan_in || monomials.len() > limits.max_monomials {
            return Err(WideAnfError::BudgetExceeded);
        }
        let mut unique_inputs = inputs.clone();
        unique_inputs.sort_unstable();
        for &index in &unique_inputs {
            if index >= source_width {
                return Err(WideAnfError::InputOutOfRange(index));
            }
        }
        if let Some(pair) = unique_inputs.windows(2).find(|w| w[0] == w[1]) {
            return Err(WideAnfError::DuplicateInput(pair[0]));
        }
        // Every index is < source_width, hence +1 is safe; exactly the
        // same precondition as checking all input indices on every call.
        let required_source_len = unique_inputs[k - 1] + 1;
        let mask_words_per_term = k.div_ceil(64);
        let mask_words_total = mask_words_per_term
            .checked_mul(monomials.len())
            .ok_or(WideAnfError::BudgetExceeded)?;
        if mask_words_total > limits.max_stored_mask_words {
            return Err(WideAnfError::BudgetExceeded);
        }
        let mut canonical = Vec::new();
        canonical
            .try_reserve_exact(monomials.len())
            .map_err(|_| WideAnfError::Allocation)?;
        let mut references = 0usize;
        for mut term in monomials {
            term.sort_unstable();
            for &index in &term {
                if index >= k {
                    return Err(WideAnfError::MonomialOutOfRange(index));
                }
            }
            if let Some(pair) = term.windows(2).find(|w| w[0] == w[1]) {
                return Err(WideAnfError::DuplicateMonomialInput(pair[0]));
            }
            references = references
                .checked_add(term.len())
                .ok_or(WideAnfError::BudgetExceeded)?;
            if references > limits.max_term_input_refs {
                return Err(WideAnfError::BudgetExceeded);
            }
            canonical.push(term);
        }
        canonical.sort_unstable();
        if canonical.windows(2).any(|w| w[0] == w[1]) {
            return Err(WideAnfError::DuplicateMonomial);
        }

        let term_count = canonical.len();
        // Fixed density policy: dense only at k<=8 and >=25% occupancy.
        // Otherwise avoid exponential coefficient storage and evaluation.
        let use_dense =
            k <= crate::anf_local::LOCAL_ANF_MAX_FAN_IN && term_count >= (1usize << k).div_ceil(4);

        let execution = if use_dense {
            let bit_count = 1usize << k;
            let mut coefficient_words = vec![0u64; bit_count.div_ceil(64)];
            for term in &canonical {
                let index = term.iter().fold(0usize, |acc, &bit| acc | (1usize << bit));
                coefficient_words[index / 64] |= 1u64 << (index % 64);
            }
            Execution::Dense(LocalAnfGate::new(
                inputs.clone(),
                coefficient_words,
                source_width,
            )?)
        } else {
            let mut masks = Vec::new();
            let mut nodes = Vec::new();
            let mut terminals = Vec::new();
            masks
                .try_reserve_exact(term_count)
                .map_err(|_| WideAnfError::Allocation)?;
            nodes
                .try_reserve_exact(references.min(limits.max_prefix_nodes))
                .map_err(|_| WideAnfError::Allocation)?;
            terminals
                .try_reserve_exact(term_count)
                .map_err(|_| WideAnfError::Allocation)?;
            let mut transitions = BTreeMap::<(usize, usize), usize>::new();
            for term in &canonical {
                let mut mask = Vec::new();
                mask.try_reserve_exact(mask_words_per_term)
                    .map_err(|_| WideAnfError::Allocation)?;
                mask.resize(mask_words_per_term, 0u64);
                let mut parent = 0usize;
                for &variable in term {
                    mask[variable / 64] |= 1u64 << (variable % 64);
                    let key = (parent, variable);
                    parent = if let Some(&existing) = transitions.get(&key) {
                        existing
                    } else {
                        if nodes.len() >= limits.max_prefix_nodes {
                            return Err(WideAnfError::BudgetExceeded);
                        }
                        nodes.push(PrefixNode { parent, variable });
                        let inserted = nodes.len();
                        transitions.insert(key, inserted);
                        inserted
                    };
                }
                masks.push(mask);
                terminals.push(parent);
            }
            Execution::Sparse(SparseBody {
                masks,
                nodes,
                terminals,
            })
        };
        Ok(Self {
            inputs,
            required_source_len,
            active_monomials: term_count,
            execution,
        })
    }

    /// Compatibility bridge from the existing low-fan-in LocalAnfGate.
    pub fn from_local(
        gate: &LocalAnfGate,
        source_width: usize,
        limits: WideAnfLimits,
    ) -> Result<Self, WideAnfError> {
        let k = gate.fan_in();
        let mut monomials = Vec::new();
        for mask in 0..gate.monomial_count() {
            if gate.coefficient(mask) {
                let local = (0..k)
                    .filter(|&bit| mask & (1usize << bit) != 0)
                    .collect::<Vec<_>>();
                monomials.push(local);
            }
        }
        Self::from_monomials(gate.inputs().to_vec(), monomials, source_width, limits)
    }

    /// Bridge the existing sparse u32 address domain (1..31 bits) to
    /// an arbitrary global wide source vector, without changing its ANF.
    pub fn from_large(
        gate: &LargeAnfGate,
        source_inputs: Vec<usize>,
        source_width: usize,
        limits: WideAnfLimits,
    ) -> Result<Self, WideAnfError> {
        if source_inputs.len() != gate.address_bits() as usize {
            return Err(WideAnfError::InvalidNetwork);
        }
        let k = gate.address_bits() as usize;
        let monomials = gate
            .monomials()
            .iter()
            .map(|&mask| {
                (0..k)
                    .filter(|&bit| mask & (1u32 << bit) != 0)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        Self::from_monomials(source_inputs, monomials, source_width, limits)
    }

    #[must_use]
    pub fn backend(&self) -> WideAnfBackend {
        match self.execution {
            Execution::Dense(_) => WideAnfBackend::Dense,
            Execution::Sparse(_) => WideAnfBackend::Sparse,
        }
    }

    #[must_use]
    pub fn input_positions(&self) -> &[usize] {
        &self.inputs
    }

    #[must_use]
    pub fn fan_in(&self) -> usize {
        self.inputs.len()
    }

    #[must_use]
    pub fn active_monomials(&self) -> usize {
        self.active_monomials
    }

    #[must_use]
    pub fn prefix_nodes(&self) -> usize {
        match &self.execution {
            Execution::Dense(_) => 0,
            Execution::Sparse(body) => body.nodes.len(),
        }
    }

    #[must_use]
    pub fn stored_mask_words(&self) -> usize {
        match &self.execution {
            Execution::Dense(_) => 0,
            Execution::Sparse(body) => body.masks.iter().map(Vec::len).sum(),
        }
    }

    #[must_use]
    pub fn scratch_words(&self) -> usize {
        match &self.execution {
            Execution::Dense(gate) => gate.monomial_count(),
            Execution::Sparse(body) => body.nodes.len() + 1,
        }
    }

    /// Logical payload bytes only (packed coefficients, index/prefix arrays);
    /// excludes allocator headers/capacity and resident memory overhead.
    #[must_use]
    pub fn payload_bytes(&self) -> usize {
        let input_bytes = self.inputs.len() * size_of::<usize>();
        match &self.execution {
            Execution::Dense(gate) => {
                input_bytes + gate.monomial_count().div_ceil(64) * size_of::<u64>()
            }
            Execution::Sparse(body) => {
                input_bytes
                    + self.stored_mask_words() * size_of::<u64>()
                    + body.nodes.len() * size_of::<PrefixNode>()
                    + body.terminals.len() * size_of::<usize>()
            }
        }
    }

    /// Independent scalar reference: the sparse path reads packed monomial
    /// masks, NOT the cached-prefix nodes used by the u64 batch evaluator.
    pub fn evaluate_scalar(&self, source: &[bool]) -> Result<bool, WideAnfError> {
        if source.len() < self.required_source_len {
            return Err(WideAnfError::SourceTooShort);
        }
        match &self.execution {
            Execution::Dense(gate) => Ok(gate.evaluate(source)),
            Execution::Sparse(body) => {
                let mut result = false;
                for monomial in &body.masks {
                    let mut active = true;
                    for (word_index, &mask) in monomial.iter().enumerate() {
                        let mut set_bits = mask;
                        while set_bits != 0 {
                            let bit = set_bits.trailing_zeros() as usize;
                            let local = word_index * 64 + bit;
                            if !source[self.inputs[local]] {
                                active = false;
                                break;
                            }
                            set_bits &= set_bits - 1;
                        }
                        if !active {
                            break;
                        }
                    }
                    result ^= active;
                }
                Ok(result)
            }
        }
    }

    /// Exact 64-lane evaluation. Caller owns scratch to avoid per-step
    /// allocations. All bounds are checked before writing scratch.
    pub fn evaluate_lanes(&self, source: &[u64], scratch: &mut [u64]) -> Result<u64, WideAnfError> {
        if source.len() < self.required_source_len {
            return Err(WideAnfError::SourceTooShort);
        }
        if scratch.len() < self.scratch_words() {
            return Err(WideAnfError::ScratchTooShort);
        }
        match &self.execution {
            Execution::Dense(gate) => Ok(gate.evaluate_lanes(source, scratch)),
            Execution::Sparse(body) => {
                scratch[0] = u64::MAX;
                for (index, node) in body.nodes.iter().enumerate() {
                    scratch[index + 1] = scratch[node.parent] & source[self.inputs[node.variable]];
                }
                Ok(body
                    .terminals
                    .iter()
                    .fold(0u64, |out, &terminal| out ^ scratch[terminal]))
            }
        }
    }
}

/// Runtime-sized recurrent network running the same native state/input/readout
/// contract as LocalAnfNetwork, with wide sparse gates allowed at each slot.
/// This adapter does not modify existing SBG/MOR/Delta-KV integrations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WideAnfNetwork {
    state_bits: usize,
    input_bits: usize,
    gates: Vec<WideAnfGate>,
    readout: WideAnfGate,
}

impl WideAnfNetwork {
    pub fn new(
        state_bits: usize,
        input_bits: usize,
        gates: Vec<WideAnfGate>,
        readout: WideAnfGate,
    ) -> Result<Self, WideAnfError> {
        let source_width = state_bits
            .checked_add(input_bits)
            .ok_or(WideAnfError::InvalidNetwork)?;
        if state_bits == 0 || input_bits == 0 || gates.len() != state_bits {
            return Err(WideAnfError::InvalidNetwork);
        }
        if gates
            .iter()
            .chain(std::iter::once(&readout))
            .any(|gate| gate.inputs.iter().any(|&index| index >= source_width))
        {
            return Err(WideAnfError::InvalidNetwork);
        }
        Ok(Self {
            state_bits,
            input_bits,
            gates,
            readout,
        })
    }

    /// Native compatibility bridge; does not alter LocalAnfNetwork.
    pub fn from_local(
        network: &LocalAnfNetwork,
        limits: WideAnfLimits,
    ) -> Result<Self, WideAnfError> {
        let source_width = network.source_width();
        let mut gates = Vec::new();
        gates
            .try_reserve_exact(network.state_bits())
            .map_err(|_| WideAnfError::Allocation)?;
        for gate in network.gates() {
            gates.push(WideAnfGate::from_local(gate, source_width, limits)?);
        }
        let readout = WideAnfGate::from_local(network.readout(), source_width, limits)?;
        Self::new(network.state_bits(), network.input_bits(), gates, readout)
    }

    #[must_use]
    pub fn source_width(&self) -> usize {
        self.state_bits + self.input_bits
    }

    #[must_use]
    pub fn scratch_words(&self) -> usize {
        self.gates
            .iter()
            .chain(std::iter::once(&self.readout))
            .map(WideAnfGate::scratch_words)
            .max()
            .unwrap_or(0)
    }

    pub fn step_lanes(
        &self,
        source: &mut [u64],
        next: &mut [u64],
        scratch: &mut [u64],
    ) -> Result<(), WideAnfError> {
        if source.len() < self.source_width() {
            return Err(WideAnfError::SourceTooShort);
        }
        if next.len() < self.state_bits || scratch.len() < self.scratch_words() {
            return Err(WideAnfError::ScratchTooShort);
        }
        for (slot, gate) in next.iter_mut().take(self.state_bits).zip(&self.gates) {
            *slot = gate.evaluate_lanes(source, scratch)?;
        }
        source[..self.state_bits].copy_from_slice(&next[..self.state_bits]);
        Ok(())
    }

    pub fn readout_lanes(&self, source: &[u64], scratch: &mut [u64]) -> Result<u64, WideAnfError> {
        self.readout.evaluate_lanes(source, scratch)
    }

    /// All-zero initial state, one caller-owned scratch reused across time.
    /// `inputs` is step-major with input_bits lane words per step.
    pub fn run_lanes(&self, inputs: &[u64]) -> Result<Vec<u64>, WideAnfError> {
        if inputs.len() % self.input_bits != 0 {
            return Err(WideAnfError::InvalidNetwork);
        }
        let steps = inputs.len() / self.input_bits;
        let mut out = Vec::new();
        out.try_reserve_exact(steps)
            .map_err(|_| WideAnfError::Allocation)?;
        let mut source = vec![0u64; self.source_width()];
        let mut next = vec![0u64; self.state_bits];
        let mut scratch = vec![0u64; self.scratch_words()];
        for chunk in inputs.chunks_exact(self.input_bits) {
            source[self.state_bits..].copy_from_slice(chunk);
            self.step_lanes(&mut source, &mut next, &mut scratch)?;
            out.push(self.readout_lanes(&source, &mut scratch)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anf_local::{LocalAnfShape, LocalAnfWiring};
    use crate::toy_tasks::SplitMix64;

    fn limits() -> WideAnfLimits {
        WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
    }

    fn literal(term: &[usize], lane: usize, input: &[u64]) -> bool {
        term.iter().all(|&index| (input[index] >> lane) & 1 == 1)
    }

    #[test]
    fn all_wide_widths_equal_independent_literal_oracle_over_64_lanes() {
        for k in [8, 16, 32, 64, 128, 256, 1024] {
            let mut rng = SplitMix64::new(k as u64 * 31);
            let mut inputs = (0..k).map(|_| rng.next_u64()).collect::<Vec<_>>();
            let monomials = vec![
                vec![],
                vec![0],
                vec![k - 1],
                vec![0, k / 2, k - 1],
                (0..k).collect::<Vec<_>>(),
            ];
            let gate =
                WideAnfGate::from_monomials((0..k).collect(), monomials.clone(), k, limits())
                    .unwrap();
            for mode in 0..3 {
                if mode == 1 {
                    inputs.fill(0);
                }
                if mode == 2 {
                    inputs.fill(u64::MAX);
                }
                let mut scratch = vec![0; gate.scratch_words()];
                let actual = gate.evaluate_lanes(&inputs, &mut scratch).unwrap();
                for lane in 0..64 {
                    let expected = monomials
                        .iter()
                        .fold(false, |parity, m| parity ^ literal(m, lane, &inputs));
                    assert_eq!((actual >> lane) & 1 != 0, expected, "k={k}, lane={lane}");
                    let scalar = inputs
                        .iter()
                        .map(|w| (w >> lane) & 1 != 0)
                        .collect::<Vec<_>>();
                    assert_eq!(gate.evaluate_scalar(&scalar).unwrap(), expected);
                }
            }
            if k > 8 {
                assert_eq!(gate.backend(), WideAnfBackend::Sparse);
                assert_eq!(gate.stored_mask_words(), monomials.len() * k.div_ceil(64));
            }
        }
    }

    #[test]
    fn backend_selects_density_and_constant_and_zero_are_exact() {
        let k = 8;
        let monomials = (0..128)
            .map(|mask| (0..k).filter(|&i| mask & (1 << i) != 0).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let dense = WideAnfGate::from_monomials((0..k).collect(), monomials, k, limits()).unwrap();
        assert_eq!(dense.backend(), WideAnfBackend::Dense);
        assert_eq!(dense.scratch_words(), 256);
        let sparse =
            WideAnfGate::from_monomials((0..k).collect(), vec![vec![0]], k, limits()).unwrap();
        assert_eq!(sparse.backend(), WideAnfBackend::Sparse);
        assert!(sparse.scratch_words() < dense.scratch_words());
        let zero = WideAnfGate::from_monomials(vec![0], vec![], 1, limits()).unwrap();
        let one = WideAnfGate::from_monomials(vec![0], vec![vec![]], 1, limits()).unwrap();
        let mut zero_scratch = vec![0u64; zero.scratch_words()];
        let mut one_scratch = vec![0u64; one.scratch_words()];
        assert_eq!(zero.evaluate_lanes(&[u64::MAX], &mut zero_scratch), Ok(0));
        assert_eq!(one.evaluate_lanes(&[0], &mut one_scratch), Ok(u64::MAX));
    }

    #[test]
    fn all_invalid_inputs_and_budgets_reject_without_modifying_caller_buffers() {
        let lim = limits();
        assert_eq!(
            WideAnfGate::from_monomials(vec![], vec![], 0, lim),
            Err(WideAnfError::EmptyFanIn)
        );
        assert_eq!(
            WideAnfGate::from_monomials(vec![1], vec![], 1, lim),
            Err(WideAnfError::InputOutOfRange(1))
        );
        assert_eq!(
            WideAnfGate::from_monomials(vec![0, 0], vec![], 2, lim),
            Err(WideAnfError::DuplicateInput(0))
        );
        assert_eq!(
            WideAnfGate::from_monomials(vec![0], vec![vec![1]], 1, lim),
            Err(WideAnfError::MonomialOutOfRange(1))
        );
        assert_eq!(
            WideAnfGate::from_monomials(vec![0, 1], vec![vec![0, 0]], 2, lim),
            Err(WideAnfError::DuplicateMonomialInput(0))
        );
        assert_eq!(
            WideAnfGate::from_monomials(vec![0], vec![vec![0], vec![0]], 1, lim),
            Err(WideAnfError::DuplicateMonomial)
        );
        assert_eq!(
            WideAnfGate::from_monomials(
                vec![0, 1, 2],
                vec![vec![0, 1, 2]],
                3,
                WideAnfLimits::new(2, 1, 10, 10, 10),
            ),
            Err(WideAnfError::BudgetExceeded)
        );
        assert_eq!(
            WideAnfGate::from_monomials(
                (0..1024).collect(),
                vec![(0..1024).collect()],
                1024,
                WideAnfLimits::new(2048, 256, 8192, 8192, 30),
            ),
            Err(WideAnfError::BudgetExceeded)
        );
        let gate = WideAnfGate::from_monomials(vec![0, 1], vec![vec![0], vec![1]], 2, lim).unwrap();
        let mut scratch = vec![0x5555u64; gate.scratch_words() - 1];
        assert_eq!(
            gate.evaluate_lanes(&[0, 0], &mut scratch),
            Err(WideAnfError::ScratchTooShort)
        );
        assert!(scratch.iter().all(|w| *w == 0x5555));
        assert_eq!(
            gate.evaluate_lanes(&[0], &mut vec![0; gate.scratch_words()]),
            Err(WideAnfError::SourceTooShort)
        );
    }

    #[test]
    fn cached_global_bound_preserves_source_validation_for_noncontiguous_inputs() {
        let gate = WideAnfGate::from_monomials(
            vec![2, 99],
            vec![vec![0], vec![1]],
            100,
            limits(),
        )
        .unwrap();
        let mut scratch = vec![0u64; gate.scratch_words()];
        let short = vec![u64::MAX; 99];
        assert_eq!(
            gate.evaluate_lanes(&short, &mut scratch),
            Err(WideAnfError::SourceTooShort)
        );
        assert_eq!(
            gate.evaluate_scalar(&vec![true; 99]),
            Err(WideAnfError::SourceTooShort)
        );
        let mut exact = vec![0u64; 100];
        exact[2] = u64::MAX;
        exact[99] = 0;
        assert_eq!(gate.evaluate_lanes(&exact, &mut scratch), Ok(u64::MAX));
    }

    #[test]
    fn native_recurrent_network_bridge_is_bit_exact_at_k8_n128() {
        for seed in [11, 29] {
            let local = LocalAnfNetwork::random(
                LocalAnfShape {
                    state_bits: 128,
                    fan_in: 8,
                    readout_fan_in: 8,
                    wiring: LocalAnfWiring::Local,
                },
                seed,
            )
            .unwrap();
            let wide = WideAnfNetwork::from_local(&local, limits()).unwrap();
            let mut rng = SplitMix64::new(seed ^ 0xf1f1_f2f2_f3f3_f4f4);
            let mut local_state = vec![0u64; local.source_width()];
            let mut local_next = vec![0u64; local.state_bits()];
            let mut local_scratch = vec![0u64; local.scratch_len()];
            let input_words = (0..256).map(|_| rng.next_u64()).collect::<Vec<_>>();
            let wide_output = wide.run_lanes(&input_words).unwrap();
            for (t, &word) in input_words.iter().enumerate() {
                local_state[local.state_bits()] = word;
                local.step_lanes(&mut local_state, &mut local_next, &mut local_scratch);
                let native_y = local
                    .readout()
                    .evaluate_lanes(&local_state, &mut local_scratch);
                assert_eq!(native_y, wide_output[t], "seed={seed}, timestep={t}");
            }
        }
    }

    #[test]
    fn million_address_existing_large_gate_bridges_exactly_at_k31() {
        let terms = vec![0u32, 1, 3, 1 << 20, 1 << 30, (1 << 30) | 1];
        let large = LargeAnfGate::new(31, terms).unwrap();
        let bridge = WideAnfGate::from_large(&large, (0..31).collect(), 31, limits()).unwrap();
        assert_eq!(bridge.backend(), WideAnfBackend::Sparse);
        let mut rng = SplitMix64::new(18);
        let input = (0..31).map(|_| rng.next_u64()).collect::<Vec<_>>();
        let mut scratch = vec![0u64; bridge.scratch_words()];
        let got = bridge.evaluate_lanes(&input, &mut scratch).unwrap();
        for lane in 0..64 {
            let address = input
                .iter()
                .enumerate()
                .fold(0u32, |a, (i, &word)| a | (((word >> lane) as u32 & 1) << i));
            assert_eq!(
                (got >> lane) & 1 != 0,
                large.evaluate(address).unwrap(),
                "lane={lane}"
            );
        }
    }

    #[test]
    fn wide_network_can_really_step_with_gate_fan_in_above_eight() {
        let lim = limits();
        let state_bits = 1024;
        let inputs = (0..state_bits).collect::<Vec<_>>();
        let wide_and = WideAnfGate::from_monomials(
            inputs,
            vec![(0..state_bits).collect()],
            state_bits + 1,
            lim,
        )
        .unwrap();
        assert_eq!(wide_and.scratch_words(), state_bits + 1);
        // The source is fully configurable: a 1024-input AND in one
        // recurrent slot next to ordinary copy/zero slots is admissible.
        let copy =
            WideAnfGate::from_monomials(vec![state_bits], vec![vec![0]], state_bits + 1, lim)
                .unwrap();
        let mut gates = vec![copy.clone(); state_bits];
        gates[0] = wide_and;
        let net = WideAnfNetwork::new(state_bits, 1, gates, copy).unwrap();
        let mut source = vec![u64::MAX; net.source_width()];
        let mut next = vec![0u64; state_bits];
        let mut scratch = vec![0u64; net.scratch_words()];
        net.step_lanes(&mut source, &mut next, &mut scratch)
            .unwrap();
        assert_eq!(source[0], u64::MAX);
        assert_eq!(net.readout_lanes(&source, &mut scratch), Ok(u64::MAX));
    }
}
