#![forbid(unsafe_code)]

//! Deterministic reference primitives for the SML-GENIUS research programme.
//!
//! This crate is intentionally small and dependency-free. It is an executable
//! specification surface, not a performance implementation and not evidence of
//! model quality or architectural novelty.

use core::fmt;

pub mod ablation;
pub mod anf;
pub mod anf_budgeted_four_support;
pub mod anf_byte_backoff;
pub mod anf_char;
pub mod anf_constructive;
pub mod anf_credit;
pub mod anf_credit_cycles;
pub mod anf_credit_multihop;
pub mod anf_credit_topology;
pub mod anf_delay_line;
pub mod anf_dual_hidden_credit;
pub mod anf_e5_logloss_arbiter;
pub mod anf_e5_materialization;
pub mod anf_e5_spectral_beam;
pub mod anf_elastic_capacity;
pub mod anf_epistemic_intuition;
pub mod anf_intermediate_credit;
pub mod anf_joint_feature_selection;
pub mod anf_joint_nonlinear_system_id;
pub mod anf_large;
pub mod anf_large_bank;
pub mod anf_large_recurrent;
pub mod anf_local;
pub mod anf_local_history;
pub mod anf_local_incremental;
pub mod anf_local_learn;
pub mod anf_marginal_support;
pub mod anf_nonlinear_joint_credit;
pub mod anf_pair_support;
pub mod anf_passive_committee;
pub mod anf_phase_capacity;
pub mod anf_sbg_unified;
pub mod anf_scoped_risk;
pub mod anf_side;
pub mod anf_sparse_byte;
pub mod anf_sparse_readout;
pub mod anf_three_gate_diagnostic;
pub mod anf_triple_support;
pub mod anf_wide_active_learn;
pub mod anf_wide_overlap_learn;
pub mod anf_wide_sparse;
pub mod anf_wide_unanchored_learn;
pub mod atlas;
pub mod audit;
pub mod bit_mixer;
pub mod bool_coder;
pub mod boolean_associative_focus;
pub mod boolean_associative_joint;
pub mod boolean_associative_search;
pub mod boolean_associative_train;
pub mod boolean_fourier;
pub mod boolean_hierarchical;
pub mod boolean_integral;
pub mod boolean_learning;
pub mod boolean_overlapping;
pub mod boolean_page;
pub mod boolean_search_bench;
pub mod byte_factorization;
pub mod calibrated_delay_readout;
pub mod candidate;
pub mod char_baselines;
pub mod circuit;
pub mod context_ledger;
pub mod crypto_features;
pub mod crypto_strategy;
pub mod delta_kv;
pub mod delta_kv_adaptive;
pub mod delta_kv_controls;
pub mod delta_kv_diagnostics;
pub mod delta_kv_dynamic;
pub mod delta_kv_panel;
pub mod delta_kv_recurrent;
pub mod delta_kv_resolution;
pub mod delta_kv_semantic;
pub mod diff_logic;
pub mod elastic_activation;
pub mod elastic_ngram;
pub mod elastic_ngram_long;
pub mod elastic_ngram_packed;
pub mod elastic_ngram_paged;
pub mod elastic_ngram_paged_sha;
pub mod elastic_ngram_trie;
pub mod elastic_weights;
pub mod eng1c_dev;
pub mod forge_topology;
pub mod frozen_bit_markov_prior;
pub mod gb11_p1_stream;
pub mod gb11_relaxed_anf;
pub mod geometric_anf_circuit;
pub mod geometric_boolean;
pub mod geometric_fan;
pub mod geometric_fan_anf;
pub mod gf2;
pub mod gru_side;
pub mod hyb_alt_pairwise_residual;
pub mod hyb_alt_ternary_sidecar;
pub mod hyb_alt_train_only;
pub mod hyb_baf_neural_gate;
pub mod hyb_fused_head;
pub mod hyb_h1_residual;
pub mod hyb_h1b_calibrated;
pub mod hyb_h1c_stable;
pub mod hyb_h1d_bank_head;
pub mod hyb_h1d_copy;
pub mod hyb_h2_mor_experts;
pub mod hyb_h2c_copy;
pub mod hyb_h2t_templates;
pub mod hyb_h3_engine;
pub mod hybrid_logic_engine;
pub mod large_anf_delta_kv;
pub mod large_anf_routed;
pub mod large_anf_selected_pages;
pub mod learned_mor_anf_recurrent;
pub mod memory;
pub mod mor;
pub mod mor_affinity;
pub mod mor_affinity_train;
pub mod mor_boolean_learning;
pub mod mor_controller;
pub mod mor_proposal_train;
pub mod mor_registry;
pub mod mor_route_objective;
pub mod mor_runtime;
pub mod mor_runtime_sequence_objective;
pub mod mor_sequence_objective;
pub mod mor_sequence_train;
pub mod mor_specialization;
pub mod mor_temporal;
pub mod mor_temporal_search;
pub mod paired_panel;
pub mod pascal_gf2;
pub mod pascal_vector_projection;
pub mod pi_boolean;
pub mod pvp6_executor;
pub mod pvp6_recurrent;
pub mod pvp6_sbg;
pub mod pvp6_sbg_atlas;
pub mod pvp6_sbg_coupled;
pub mod pvp6_sbg_expert_loop;
pub mod pvp6_sbg_experts;
pub mod pvp6_sbg_kernel;
pub mod pvp6_sbg_routing;
pub mod pvp6_sbg_trace;
pub mod routing_train;
pub mod rtlr_boolean_hybrid;
pub mod rtlr_cost_gate;
pub mod rtlr_elastic;
pub mod rtrl3_elastic;
pub mod semantic;
pub mod sparse_gradient_atlas;
pub mod sparse_mor_anf_recurrent;
pub mod spectral_baseline_gate;
pub mod spectral_dual_prior_gate;
pub mod spectral_method_gate;
pub mod task_loss;
pub mod tasks;
pub mod ternary_bitplane;
pub mod toy_baselines;
pub mod toy_tasks;
pub mod trace;
pub mod train;
pub mod train_lag_discovery;
pub mod v888_handoff;
pub mod wide_anf_delta_kv;
pub mod wide_anf_routed;
pub mod wide_anf_selected_pages;

/// Inference RAM ceiling for the target SML-25B profile.
pub const TARGET_RAM_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Inference VRAM ceiling for the target SML-25B profile.
pub const TARGET_VRAM_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Nominal single-stream decode target.
pub const TARGET_TOKENS_PER_SECOND: u32 = 40;
/// Initial maximum active-parameter fraction per recurrent cycle, in parts per million.
pub const INITIAL_ACTIVE_PPM: u32 = 10_000;
/// Long-term active-parameter fraction target per recurrent cycle, in parts per million.
pub const STRETCH_ACTIVE_PPM: u32 = 2_500;

/// Exact physical accounting for the initial mixed-precision SML parameter atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParameterBudget {
    /// One physical bit per learned parameter.
    pub boolean: u64,
    /// Two physical bits per learned ternary parameter.
    pub ternary: u64,
    /// Four physical bits per learned residual parameter.
    pub residual4: u64,
}

impl ParameterBudget {
    /// Returns the exact number of learned scalar parameters represented by this budget.
    ///
    /// # Panics
    ///
    /// Panics, in every build profile, if the total does not fit in `u64`;
    /// use [`Self::checked_total_parameters`] for untrusted budgets.
    #[must_use]
    pub const fn total_parameters(self) -> u64 {
        match self.checked_total_parameters() {
            Some(total) => total,
            None => panic!("ParameterBudget total overflows u64"),
        }
    }

    /// Returns the total parameter count, or `None` if it overflows `u64`.
    #[must_use]
    pub const fn checked_total_parameters(self) -> Option<u64> {
        match self.boolean.checked_add(self.ternary) {
            Some(partial) => partial.checked_add(self.residual4),
            None => None,
        }
    }

    /// Returns exact packed storage in bits, excluding metadata and alignment.
    #[must_use]
    pub const fn storage_bits(self) -> u128 {
        self.boolean as u128 + 2 * self.ternary as u128 + 4 * self.residual4 as u128
    }

    /// Returns packed bytes rounded upward to the next byte.
    #[must_use]
    pub const fn storage_bytes_ceil(self) -> u128 {
        self.storage_bits().div_ceil(8)
    }

    /// Returns the Boolean share in parts per million, avoiding floating-point ambiguity.
    #[must_use]
    pub const fn boolean_fraction_ppm(self) -> u32 {
        let total = self.total_parameters();
        if total == 0 {
            return 0;
        }
        ((self.boolean as u128 * 1_000_000) / total as u128) as u32
    }
}

/// Canonical initial SML-25B research budget: 85% Boolean, 10% ternary, 5% 4-bit residual.
pub const SML_25B_INITIAL_BUDGET: ParameterBudget = ParameterBudget {
    boolean: 21_250_000_000,
    ternary: 2_500_000_000,
    residual4: 1_250_000_000,
};

/// Exact counters attached to a reference execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExecutionCounters {
    /// Primitive Boolean gate evaluations.
    pub boolean_ops: u64,
    /// Semantic GF(2) primitive evaluations.
    pub gf2_ops: u64,
    /// Algebraic-normal-form monomials evaluated.
    pub anf_monomials: u64,
    /// Direct route-table lookups.
    pub route_lookups: u64,
    /// Bounded associative-memory reads.
    pub memory_reads: u64,
    /// Bounded associative-memory writes.
    pub memory_writes: u64,
    /// Successful associative-memory reads.
    pub memory_hits: u64,
    /// Associative-memory misses.
    pub memory_misses: u64,
    /// Write-time address collisions.
    pub memory_collisions: u64,
    /// Deterministic collision replacements.
    pub memory_replacements: u64,
    /// Exact-key comparisons inside directly addressed memory slots.
    pub memory_key_comparisons: u64,
    /// Circuit-atlas page activations.
    pub atlas_page_activations: u64,
    /// Learned parameter touches induced by atlas-page activations.
    pub atlas_parameter_touches: u64,
    /// Semantic storage bits touched by atlas-page activations.
    pub atlas_storage_bits_touched: u128,
    /// Token-pair comparisons. This must remain zero for SML candidate paths.
    pub pairwise_comparisons: u64,
    /// Attention-score operations. This must remain zero for SML candidate paths.
    pub attention_score_ops: u64,
    /// Recurrent reasoning cycles completed.
    pub recurrent_cycles: u32,
    /// Learned parameters touched across recurrent cycles.
    pub active_parameters: u64,
    /// Semantic active-storage bits touched across recurrent cycles.
    pub active_storage_bits: u128,
}

/// A densely packed Boolean workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooleanWorkspace {
    words: Vec<u64>,
    len_bits: usize,
}

impl BooleanWorkspace {
    /// Builds a packed workspace from explicit bits.
    #[must_use]
    pub fn from_bits(bits: &[bool]) -> Self {
        let mut words = vec![0_u64; bits.len().div_ceil(64)];
        for (index, bit) in bits.iter().copied().enumerate() {
            if bit {
                words[index / 64] |= 1_u64 << (index % 64);
            }
        }
        Self {
            words,
            len_bits: bits.len(),
        }
    }

    /// Number of meaningful bits in the workspace.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len_bits
    }

    /// Returns true when the workspace contains no bits.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len_bits == 0
    }

    /// Reads one bit.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<bool> {
        if index >= self.len_bits {
            return None;
        }
        Some((self.words[index / 64] & (1_u64 << (index % 64))) != 0)
    }

    /// Returns the Hamming distance between equally-sized workspaces.
    pub fn hamming_distance(&self, other: &Self) -> Result<u64, WorkspaceError> {
        if self.len_bits != other.len_bits {
            return Err(WorkspaceError::LengthMismatch {
                left: self.len_bits,
                right: other.len_bits,
            });
        }
        Ok(self
            .words
            .iter()
            .zip(&other.words)
            .map(|(left, right)| (left ^ right).count_ones() as u64)
            .sum())
    }

    fn low_address(&self, width: u8) -> Result<usize, RouteError> {
        if width as usize > usize::BITS as usize {
            return Err(RouteError::AddressWidthTooLarge(width));
        }
        if width as usize > self.len_bits {
            return Err(RouteError::WorkspaceTooSmall {
                required: width as usize,
                actual: self.len_bits,
            });
        }

        let mut address = 0_usize;
        for bit in 0..width as usize {
            if self.get(bit).unwrap_or(false) {
                address |= 1_usize << bit;
            }
        }
        Ok(address)
    }
}

/// Errors produced by workspace operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceError {
    /// Hamming distance requires equal logical widths.
    LengthMismatch { left: usize, right: usize },
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthMismatch { left, right } => {
                write!(f, "workspace length mismatch: {left} != {right}")
            }
        }
    }
}

impl std::error::Error for WorkspaceError {}

/// A reference to a Boolean circuit signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    /// External circuit input.
    Input(u32),
    /// Output of a previously evaluated gate.
    Gate(u32),
}

/// Boolean primitives permitted by the SML-0 reference circuit oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Not(Wire),
    And(Wire, Wire),
    Or(Wire, Wire),
    Xor(Wire, Wire),
}

/// A topologically ordered deterministic Boolean circuit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooleanCircuit {
    input_count: usize,
    gates: Vec<Gate>,
    outputs: Vec<Wire>,
}

impl BooleanCircuit {
    /// Creates a circuit. Structural validity is checked during evaluation.
    #[must_use]
    pub fn new(input_count: usize, gates: Vec<Gate>, outputs: Vec<Wire>) -> Self {
        Self {
            input_count,
            gates,
            outputs,
        }
    }

    /// Evaluates the circuit and increments exact primitive-operation counters.
    pub fn evaluate(
        &self,
        inputs: &[bool],
        counters: &mut ExecutionCounters,
    ) -> Result<Vec<bool>, CircuitError> {
        if inputs.len() != self.input_count {
            return Err(CircuitError::InputCount {
                expected: self.input_count,
                actual: inputs.len(),
            });
        }

        let mut gate_values = Vec::with_capacity(self.gates.len());
        for (gate_index, gate) in self.gates.iter().copied().enumerate() {
            let resolve = |wire: Wire| self.resolve(wire, inputs, &gate_values, gate_index);
            let value = match gate {
                Gate::Not(a) => !resolve(a)?,
                Gate::And(a, b) => resolve(a)? & resolve(b)?,
                Gate::Or(a, b) => resolve(a)? | resolve(b)?,
                Gate::Xor(a, b) => resolve(a)? ^ resolve(b)?,
            };
            counters.boolean_ops = counters.boolean_ops.saturating_add(1);
            gate_values.push(value);
        }

        self.outputs
            .iter()
            .copied()
            .map(|wire| self.resolve(wire, inputs, &gate_values, self.gates.len()))
            .collect()
    }

    fn resolve(
        &self,
        wire: Wire,
        inputs: &[bool],
        gate_values: &[bool],
        current_gate: usize,
    ) -> Result<bool, CircuitError> {
        match wire {
            Wire::Input(index) => inputs
                .get(index as usize)
                .copied()
                .ok_or(CircuitError::InvalidInputWire(index)),
            Wire::Gate(index) => {
                let index = index as usize;
                if index >= current_gate {
                    return Err(CircuitError::ForwardOrCyclicGateReference {
                        referenced: index,
                        current: current_gate,
                    });
                }
                gate_values
                    .get(index)
                    .copied()
                    .ok_or(CircuitError::InvalidGateWire(index))
            }
        }
    }
}

/// Structural or input errors from the Boolean circuit oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitError {
    InputCount { expected: usize, actual: usize },
    InvalidInputWire(u32),
    InvalidGateWire(usize),
    ForwardOrCyclicGateReference { referenced: usize, current: usize },
}

impl fmt::Display for CircuitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CircuitError {}

/// Stable page identifier in the future hierarchical circuit atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PageId(pub u32);

/// Direct-address reference router. Routing does not scan history or rank token pairs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectRouteTable {
    address_width: u8,
    entries: Vec<PageId>,
}

impl DirectRouteTable {
    /// Builds a complete direct-address table with exactly two-to-the-width entries.
    pub fn new(address_width: u8, entries: Vec<PageId>) -> Result<Self, RouteError> {
        if address_width as u32 >= usize::BITS {
            return Err(RouteError::AddressWidthTooLarge(address_width));
        }
        let expected = 1_usize << address_width;
        if entries.len() != expected {
            return Err(RouteError::EntryCount {
                expected,
                actual: entries.len(),
            });
        }
        Ok(Self {
            address_width,
            entries,
        })
    }

    /// Resolves a page with one direct table lookup after extracting address bits.
    pub fn route(
        &self,
        workspace: &BooleanWorkspace,
        counters: &mut ExecutionCounters,
    ) -> Result<PageId, RouteError> {
        let address = workspace.low_address(self.address_width)?;
        counters.route_lookups = counters.route_lookups.saturating_add(1);
        Ok(self.entries[address])
    }
}

/// Errors from deterministic direct routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteError {
    AddressWidthTooLarge(u8),
    EntryCount { expected: usize, actual: usize },
    WorkspaceTooSmall { required: usize, actual: usize },
}

impl fmt::Display for RouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RouteError {}

/// Result of one internal reasoning cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasoningStep {
    pub workspace: BooleanWorkspace,
    /// A true value prevents stability-based early halting for this step.
    pub contradiction: bool,
    /// Caller-measured active learned parameters touched in this cycle.
    pub active_parameters: u64,
    /// Caller-measured semantic active-storage bits touched in this cycle.
    pub active_storage_bits: u128,
    /// Circuit-atlas pages selected during this cycle, in execution order.
    pub selected_pages: Vec<PageId>,
    /// Token-pair comparisons performed by this cycle.
    pub pairwise_comparisons: u64,
    /// Attention-score operations performed by this cycle.
    pub attention_score_ops: u64,
}

/// Deterministic recurrent reasoning kernel contract.
pub trait ReasoningKernel {
    type Error;

    fn step(
        &mut self,
        workspace: &BooleanWorkspace,
        depth: u32,
    ) -> Result<ReasoningStep, Self::Error>;
}

/// Adaptive recurrent-depth policy used by the SML-0 reference runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HaltingPolicy {
    pub min_depth: u32,
    pub max_depth: u32,
    pub stable_hamming_threshold: u64,
}

impl HaltingPolicy {
    /// Rejects impossible or empty depth ranges.
    pub fn validate(self) -> Result<Self, RecurrentError> {
        if self.min_depth == 0 || self.max_depth == 0 || self.min_depth > self.max_depth {
            return Err(RecurrentError::InvalidDepthRange {
                min: self.min_depth,
                max: self.max_depth,
            });
        }
        Ok(self)
    }
}

/// Summary emitted by a completed recurrent reasoning run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurrentOutcome {
    pub workspace: BooleanWorkspace,
    pub depth: u32,
    pub counters: ExecutionCounters,
}

/// Errors from the recurrent reference runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurrentError {
    InvalidDepthRange { min: u32, max: u32 },
    Workspace(WorkspaceError),
}

impl fmt::Display for RecurrentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RecurrentError {}

impl From<WorkspaceError> for RecurrentError {
    fn from(value: WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

/// Executes repeated internal reasoning cycles until stability or the hard depth cap.
///
/// Kernel errors are returned separately from runner errors so callers do not need
/// to erase their domain-specific failure type.
pub fn run_recurrent<K: ReasoningKernel>(
    initial: BooleanWorkspace,
    kernel: &mut K,
    policy: HaltingPolicy,
) -> Result<RecurrentOutcome, RunError<K::Error>> {
    trace::run_recurrent_traced(initial, kernel, policy, 0).map(|traced| traced.outcome)
}

/// Composite error from the recurrent runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError<E> {
    Runner(RecurrentError),
    Kernel(E),
}

/// Returns whether an active parameter count stays inside a declared ppm budget.
#[must_use]
pub fn active_fraction_within_ppm(active: u64, total: u64, limit_ppm: u32) -> bool {
    if total == 0 {
        return active == 0;
    }
    active as u128 * 1_000_000 <= total as u128 * limit_ppm as u128
}
