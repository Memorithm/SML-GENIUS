//! Trainable acyclic Boolean circuits with fixed explicit wiring.
//!
//! Gate identities are learned through the SML-0.3 latent truth-table
//! relaxation. Wiring is fixed metadata in SML-0.4 and is not yet learned.

use core::fmt;

use crate::{
    train::{GateTrainingConfig, LatentGate2, TrainError, TruthTable2, TRUTH_TABLE_COUNT},
    ParameterBudget,
};

/// Reference to an external input or an earlier gate output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalRef {
    Input(u16),
    Gate(u16),
}

/// Fixed wiring for one trainable two-input gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CircuitNodeSpec {
    pub left: SignalRef,
    pub right: SignalRef,
}

/// One fully specified supervised Boolean sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CircuitSample {
    pub inputs: Vec<bool>,
    pub target: bool,
}

/// Exact representation accounting for a fixed-wiring hard circuit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CircuitAccounting {
    pub gate_count: u64,
    /// Four learned truth-table bits per hard two-input gate.
    pub learned_boolean_bits: u64,
    /// Fixed structural metadata, not learned parameters.
    pub wiring_metadata_bits: u64,
    /// Continuous training-only latent scalars.
    pub latent_training_scalars: u64,
}

impl CircuitAccounting {
    /// Final hard learned-parameter representation.
    #[must_use]
    pub const fn parameter_budget(self) -> ParameterBudget {
        ParameterBudget {
            boolean: self.learned_boolean_bits,
            ternary: 0,
            residual4: 0,
        }
    }
}

/// Deterministic training counters for a complete circuit run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CircuitTrainingCounters {
    pub epochs: u32,
    pub samples_seen: u64,
    pub soft_gate_evaluations: u64,
    pub latent_parameter_updates: u64,
}

/// Result of optimizing gate identities under fixed wiring.
#[derive(Debug, Clone, PartialEq)]
pub struct CircuitTrainingReport {
    pub initial_tables: Vec<TruthTable2>,
    pub final_tables: Vec<TruthTable2>,
    pub initial_loss: f64,
    pub final_loss: f64,
    pub hard_correct: u64,
    pub sample_count: u64,
    pub counters: CircuitTrainingCounters,
}

#[derive(Debug, Clone, PartialEq)]
struct CircuitNode {
    spec: CircuitNodeSpec,
    gate: LatentGate2,
}

/// Acyclic circuit whose wiring is fixed and whose gate identities are trainable.
#[derive(Debug, Clone, PartialEq)]
pub struct LatentBooleanCircuit {
    input_count: usize,
    nodes: Vec<CircuitNode>,
    output: SignalRef,
}

impl LatentBooleanCircuit {
    /// Constructs a circuit, validates all references and uses deterministic
    /// asymmetric latent initialization to avoid zero-gradient symmetry across
    /// stacked soft gates.
    pub fn new(
        input_count: usize,
        specs: Vec<CircuitNodeSpec>,
        output: SignalRef,
    ) -> Result<Self, CircuitError> {
        if input_count == 0 {
            return Err(CircuitError::ZeroInputs);
        }
        if input_count > u16::MAX as usize {
            return Err(CircuitError::TooManyInputs(input_count));
        }
        if specs.is_empty() {
            return Err(CircuitError::ZeroGates);
        }
        if specs.len() > u16::MAX as usize {
            return Err(CircuitError::TooManyGates(specs.len()));
        }

        for (index, spec) in specs.iter().copied().enumerate() {
            validate_source(spec.left, input_count, index)?;
            validate_source(spec.right, input_count, index)?;
        }
        validate_output(output, input_count, specs.len())?;

        let mut nodes = Vec::with_capacity(specs.len());
        for (index, spec) in specs.into_iter().enumerate() {
            nodes.push(CircuitNode {
                spec,
                gate: LatentGate2::from_logits(deterministic_logits(index))?,
            });
        }

        Ok(Self {
            input_count,
            nodes,
            output,
        })
    }

    /// Number of fixed external Boolean inputs.
    #[must_use]
    pub const fn input_count(&self) -> usize {
        self.input_count
    }

    /// Number of trainable gate identities.
    #[must_use]
    pub fn gate_count(&self) -> usize {
        self.nodes.len()
    }

    /// Exact hard-model versus training-only representation accounting.
    #[must_use]
    pub fn accounting(&self) -> CircuitAccounting {
        let signal_cardinality = self.input_count + self.nodes.len();
        let reference_bits = bits_required(signal_cardinality);
        let reference_count = self.nodes.len() as u64 * 2 + 1;

        CircuitAccounting {
            gate_count: self.nodes.len() as u64,
            learned_boolean_bits: self.nodes.len() as u64 * 4,
            wiring_metadata_bits: 64 + reference_count * reference_bits as u64,
            latent_training_scalars: self.nodes.len() as u64 * TRUTH_TABLE_COUNT as u64,
        }
    }

    /// Returns the current hard table selected by each gate.
    pub fn hard_tables(&self) -> Result<Vec<TruthTable2>, CircuitError> {
        self.nodes
            .iter()
            .map(|node| node.gate.hard_table().map_err(CircuitError::from))
            .collect()
    }

    /// Executes only the hard selected Boolean tables.
    pub fn hard_forward(&self, inputs: &[bool]) -> Result<bool, CircuitError> {
        self.validate_inputs(inputs)?;
        let mut gate_values = Vec::with_capacity(self.nodes.len());

        for node in &self.nodes {
            let left = resolve_hard(node.spec.left, inputs, &gate_values)?;
            let right = resolve_hard(node.spec.right, inputs, &gate_values)?;
            gate_values.push(node.gate.hard_forward(left, right)?);
        }

        resolve_hard(self.output, inputs, &gate_values)
    }

    /// Hard Boolean accuracy on a declared sample panel.
    pub fn hard_accuracy(&self, samples: &[CircuitSample]) -> Result<u64, CircuitError> {
        validate_samples(samples, self.input_count)?;
        let mut correct = 0_u64;
        for sample in samples {
            if self.hard_forward(&sample.inputs)? == sample.target {
                correct = correct.saturating_add(1);
            }
        }
        Ok(correct)
    }

    /// Full-batch deterministic optimization of gate identities.
    pub fn train(
        &mut self,
        samples: &[CircuitSample],
        config: GateTrainingConfig,
    ) -> Result<CircuitTrainingReport, CircuitError> {
        let config = config.validate()?;
        validate_samples(samples, self.input_count)?;

        let output_gate = match self.output {
            SignalRef::Gate(index) => index as usize,
            SignalRef::Input(_) => return Err(CircuitError::UntrainableOutput),
        };

        let initial_tables = self.hard_tables()?;
        let initial_loss = self.soft_loss(samples, config.epsilon)?;
        let mut counters = CircuitTrainingCounters::default();

        for _ in 0..config.epochs {
            let mut gradients = vec![[0.0_f64; TRUTH_TABLE_COUNT]; self.nodes.len()];

            for sample in samples {
                let forward = self.forward_soft(&sample.inputs)?;
                let probability = forward.gate_outputs[output_gate]
                    .clamp(config.epsilon, 1.0 - config.epsilon);
                let target = bool_as_f64(sample.target);
                let output_gradient =
                    (probability - target) / (probability * (1.0 - probability));

                let mut upstream = vec![0.0_f64; self.nodes.len()];
                upstream[output_gate] = output_gradient;

                for gate_index in (0..self.nodes.len()).rev() {
                    let gate_upstream = upstream[gate_index];
                    let cache = &forward.caches[gate_index];

                    for code in 0..TRUTH_TABLE_COUNT {
                        gradients[gate_index][code] += gate_upstream
                            * cache.weights[code]
                            * (cache.candidate_outputs[code] - cache.output);
                    }

                    let left_derivative: f64 = (0..TRUTH_TABLE_COUNT)
                        .map(|code| cache.weights[code] * cache.left_derivatives[code])
                        .sum();
                    let right_derivative: f64 = (0..TRUTH_TABLE_COUNT)
                        .map(|code| cache.weights[code] * cache.right_derivatives[code])
                        .sum();

                    propagate_to_gate(
                        self.nodes[gate_index].spec.left,
                        gate_upstream * left_derivative,
                        &mut upstream,
                    );
                    propagate_to_gate(
                        self.nodes[gate_index].spec.right,
                        gate_upstream * right_derivative,
                        &mut upstream,
                    );
                }
            }

            let normalization = samples.len() as f64;
            for (node, gradient) in self.nodes.iter_mut().zip(gradients) {
                let mut logits = *node.gate.logits();
                for (logit, grad) in logits.iter_mut().zip(gradient) {
                    *logit -= config.learning_rate * grad / normalization;
                }
                node.gate = LatentGate2::from_logits(logits)?;
            }

            counters.epochs = counters.epochs.saturating_add(1);
            counters.samples_seen = counters.samples_seen.saturating_add(samples.len() as u64);
            counters.soft_gate_evaluations = counters.soft_gate_evaluations.saturating_add(
                samples.len() as u64 * self.nodes.len() as u64,
            );
            counters.latent_parameter_updates = counters.latent_parameter_updates.saturating_add(
                self.nodes.len() as u64 * TRUTH_TABLE_COUNT as u64,
            );
        }

        Ok(CircuitTrainingReport {
            initial_tables,
            final_tables: self.hard_tables()?,
            initial_loss,
            final_loss: self.soft_loss(samples, config.epsilon)?,
            hard_correct: self.hard_accuracy(samples)?,
            sample_count: samples.len() as u64,
            counters,
        })
    }

    fn validate_inputs(&self, inputs: &[bool]) -> Result<(), CircuitError> {
        if inputs.len() != self.input_count {
            return Err(CircuitError::InputCount {
                expected: self.input_count,
                actual: inputs.len(),
            });
        }
        Ok(())
    }

    fn forward_soft(&self, inputs: &[bool]) -> Result<SoftForward, CircuitError> {
        self.validate_inputs(inputs)?;
        let mut gate_outputs = Vec::with_capacity(self.nodes.len());
        let mut caches = Vec::with_capacity(self.nodes.len());

        for node in &self.nodes {
            let left = resolve_soft(node.spec.left, inputs, &gate_outputs)?;
            let right = resolve_soft(node.spec.right, inputs, &gate_outputs)?;
            let weights = softmax(node.gate.logits())?;

            let mut candidate_outputs = [0.0_f64; TRUTH_TABLE_COUNT];
            let mut left_derivatives = [0.0_f64; TRUTH_TABLE_COUNT];
            let mut right_derivatives = [0.0_f64; TRUTH_TABLE_COUNT];

            for code in 0..TRUTH_TABLE_COUNT {
                let table = TruthTable2::new(code as u8)?;
                let (output, left_derivative, right_derivative) =
                    multilinear_extension(table, left, right);
                candidate_outputs[code] = output;
                left_derivatives[code] = left_derivative;
                right_derivatives[code] = right_derivative;
            }

            let output: f64 = (0..TRUTH_TABLE_COUNT)
                .map(|code| weights[code] * candidate_outputs[code])
                .sum();
            gate_outputs.push(output);
            caches.push(GateCache {
                weights,
                candidate_outputs,
                left_derivatives,
                right_derivatives,
                output,
            });
        }

        Ok(SoftForward {
            gate_outputs,
            caches,
        })
    }

    fn soft_loss(&self, samples: &[CircuitSample], epsilon: f64) -> Result<f64, CircuitError> {
        let output_gate = match self.output {
            SignalRef::Gate(index) => index as usize,
            SignalRef::Input(_) => return Err(CircuitError::UntrainableOutput),
        };
        let mut loss = 0.0_f64;

        for sample in samples {
            let forward = self.forward_soft(&sample.inputs)?;
            let probability =
                forward.gate_outputs[output_gate].clamp(epsilon, 1.0 - epsilon);
            let target = bool_as_f64(sample.target);
            loss -= target * probability.ln() + (1.0 - target) * (1.0 - probability).ln();
        }

        Ok(loss / samples.len() as f64)
    }
}

#[derive(Debug)]
struct SoftForward {
    gate_outputs: Vec<f64>,
    caches: Vec<GateCache>,
}

#[derive(Debug)]
struct GateCache {
    weights: [f64; TRUTH_TABLE_COUNT],
    candidate_outputs: [f64; TRUTH_TABLE_COUNT],
    left_derivatives: [f64; TRUTH_TABLE_COUNT],
    right_derivatives: [f64; TRUTH_TABLE_COUNT],
    output: f64,
}

fn validate_source(
    signal: SignalRef,
    input_count: usize,
    current_gate: usize,
) -> Result<(), CircuitError> {
    match signal {
        SignalRef::Input(index) if index as usize >= input_count => {
            Err(CircuitError::InvalidInputRef(index))
        }
        SignalRef::Gate(index) if index as usize >= current_gate => {
            Err(CircuitError::ForwardOrCyclicGateRef {
                referenced: index,
                current: current_gate,
            })
        }
        _ => Ok(()),
    }
}

fn validate_output(
    signal: SignalRef,
    input_count: usize,
    gate_count: usize,
) -> Result<(), CircuitError> {
    match signal {
        SignalRef::Input(index) if index as usize >= input_count => {
            Err(CircuitError::InvalidInputRef(index))
        }
        SignalRef::Gate(index) if index as usize >= gate_count => {
            Err(CircuitError::InvalidOutputGate(index))
        }
        _ => Ok(()),
    }
}

fn validate_samples(samples: &[CircuitSample], input_count: usize) -> Result<(), CircuitError> {
    if samples.is_empty() {
        return Err(CircuitError::EmptySamples);
    }
    for sample in samples {
        if sample.inputs.len() != input_count {
            return Err(CircuitError::InputCount {
                expected: input_count,
                actual: sample.inputs.len(),
            });
        }
    }
    Ok(())
}

fn resolve_hard(
    signal: SignalRef,
    inputs: &[bool],
    gates: &[bool],
) -> Result<bool, CircuitError> {
    match signal {
        SignalRef::Input(index) => inputs
            .get(index as usize)
            .copied()
            .ok_or(CircuitError::InvalidInputRef(index)),
        SignalRef::Gate(index) => gates
            .get(index as usize)
            .copied()
            .ok_or(CircuitError::InvalidOutputGate(index)),
    }
}

fn resolve_soft(
    signal: SignalRef,
    inputs: &[bool],
    gates: &[f64],
) -> Result<f64, CircuitError> {
    match signal {
        SignalRef::Input(index) => inputs
            .get(index as usize)
            .copied()
            .map(bool_as_f64)
            .ok_or(CircuitError::InvalidInputRef(index)),
        SignalRef::Gate(index) => gates
            .get(index as usize)
            .copied()
            .ok_or(CircuitError::InvalidOutputGate(index)),
    }
}

fn propagate_to_gate(signal: SignalRef, gradient: f64, upstream: &mut [f64]) {
    if let SignalRef::Gate(index) = signal {
        if let Some(value) = upstream.get_mut(index as usize) {
            *value += gradient;
        }
    }
}

fn softmax(logits: &[f64; TRUTH_TABLE_COUNT]) -> Result<[f64; TRUTH_TABLE_COUNT], CircuitError> {
    if logits.iter().any(|value| !value.is_finite()) {
        return Err(CircuitError::Train(TrainError::NonFiniteLatent));
    }

    let maximum = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut weights = [0.0_f64; TRUTH_TABLE_COUNT];
    let mut denominator = 0.0_f64;
    for (weight, logit) in weights.iter_mut().zip(logits.iter().copied()) {
        *weight = (logit - maximum).exp();
        denominator += *weight;
    }
    if !denominator.is_finite() || denominator <= 0.0 {
        return Err(CircuitError::Train(TrainError::InvalidSoftmax));
    }
    for weight in &mut weights {
        *weight /= denominator;
    }
    Ok(weights)
}

fn multilinear_extension(table: TruthTable2, left: f64, right: f64) -> (f64, f64, f64) {
    let y00 = bool_as_f64(table.evaluate(false, false));
    let y10 = bool_as_f64(table.evaluate(true, false));
    let y01 = bool_as_f64(table.evaluate(false, true));
    let y11 = bool_as_f64(table.evaluate(true, true));

    let output = y00 * (1.0 - left) * (1.0 - right)
        + y10 * left * (1.0 - right)
        + y01 * (1.0 - left) * right
        + y11 * left * right;
    let left_derivative =
        (y10 - y00) * (1.0 - right) + (y11 - y01) * right;
    let right_derivative =
        (y01 - y00) * (1.0 - left) + (y11 - y10) * left;
    (output, left_derivative, right_derivative)
}

fn deterministic_logits(node_index: usize) -> [f64; TRUTH_TABLE_COUNT] {
    let mut logits = [0.0_f64; TRUTH_TABLE_COUNT];
    for (code, logit) in logits.iter_mut().enumerate() {
        let seed = (node_index as u64 + 1)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ (code as u64 + 1).wrapping_mul(0xd1b5_4a32_d192_ed03);
        let mixed = mix64(seed);
        let unit = (mixed >> 11) as f64 / (1_u64 << 53) as f64;
        *logit = (unit - 0.5) * 0.02;
    }
    logits
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn bits_required(cardinality: usize) -> u32 {
    if cardinality <= 1 {
        1
    } else {
        usize::BITS - (cardinality - 1).leading_zeros()
    }
}

const fn bool_as_f64(value: bool) -> f64 {
    if value {
        1.0
    } else {
        0.0
    }
}

/// Invalid fixed wiring, samples, or underlying latent training state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitError {
    ZeroInputs,
    TooManyInputs(usize),
    ZeroGates,
    TooManyGates(usize),
    InvalidInputRef(u16),
    ForwardOrCyclicGateRef { referenced: u16, current: usize },
    InvalidOutputGate(u16),
    InputCount { expected: usize, actual: usize },
    EmptySamples,
    UntrainableOutput,
    Train(TrainError),
}

impl From<TrainError> for CircuitError {
    fn from(value: TrainError) -> Self {
        Self::Train(value)
    }
}

impl fmt::Display for CircuitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CircuitError {}
