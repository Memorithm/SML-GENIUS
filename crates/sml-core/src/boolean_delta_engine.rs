//! Experimental native Boolean delta engine (BDE-0).
//!
//! Evaluates a validated acyclic Boolean circuit once, then recomputes only
//! gates reachable through *changed values* when external inputs change.
//! A gate that recomputes to its previous value does not dirty its fan-out.
//!
//! This is an exact, deterministic execution candidate, not a learned model,
//! a novel Boolean algebra, or a demonstrated performance improvement.
//! The independent correctness oracle is `BooleanCircuit::evaluate`.
#![forbid(unsafe_code)]

use crate::{CircuitError, Gate, Wire};

/// Explicit structural resource limits. They are admission limits, not measured
/// host resources. Each update evaluates at most `max_gates` Boolean gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanDeltaBudget {
    pub max_inputs: usize,
    pub max_gates: usize,
    pub max_edges: usize,
    pub max_outputs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanDeltaError {
    BudgetExceeded,
    EdgeCountOverflow,
    InvalidCircuit(CircuitError),
    InputCount { expected: usize, actual: usize },
}

impl core::fmt::Display for BooleanDeltaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for BooleanDeltaError {}

/// Exact per-update logical work. Not elapsed time or speedup evidence.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BooleanDeltaWork {
    pub changed_inputs: usize,
    pub evaluated_gates: usize,
    pub changed_gates: usize,
}

/// Model-owned, CPU-only, incremental evaluator for the existing SML-0 gate IR.
/// No hidden history: cached values are functions only of current inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooleanDeltaEngine {
    inputs: Vec<bool>,
    gates: Vec<Gate>,
    outputs: Vec<Wire>,
    values: Vec<bool>,
    input_fanout: Vec<Vec<usize>>,
    gate_fanout: Vec<Vec<usize>>,
    dirty: Vec<bool>,
}

fn sources(gate: Gate) -> (Wire, Option<Wire>) {
    match gate {
        Gate::Not(a) => (a, None),
        Gate::And(a, b) | Gate::Or(a, b) | Gate::Xor(a, b) => (a, Some(b)),
    }
}

fn validate_wire(wire: Wire, input_count: usize, gate_limit: usize) -> Result<(), CircuitError> {
    match wire {
        Wire::Input(index) if (index as usize) >= input_count => {
            Err(CircuitError::InvalidInputWire(index))
        }
        Wire::Gate(index) if (index as usize) >= gate_limit => {
            Err(CircuitError::ForwardOrCyclicGateReference {
                referenced: index as usize,
                current: gate_limit,
            })
        }
        _ => Ok(()),
    }
}

fn read(wire: Wire, inputs: &[bool], values: &[bool]) -> bool {
    match wire {
        Wire::Input(index) => inputs[index as usize],
        Wire::Gate(index) => values[index as usize],
    }
}

fn evaluate(gate: Gate, inputs: &[bool], values: &[bool]) -> bool {
    match gate {
        Gate::Not(a) => !read(a, inputs, values),
        Gate::And(a, b) => read(a, inputs, values) & read(b, inputs, values),
        Gate::Or(a, b) => read(a, inputs, values) | read(b, inputs, values),
        Gate::Xor(a, b) => read(a, inputs, values) ^ read(b, inputs, values),
    }
}

impl BooleanDeltaEngine {
    /// Validates every reference and structural budget before constructing any
    /// fan-out. Gate references must point strictly backward in the gate list.
    pub fn new(
        input_count: usize,
        gates: Vec<Gate>,
        outputs: Vec<Wire>,
        budget: BooleanDeltaBudget,
    ) -> Result<Self, BooleanDeltaError> {
        if input_count > budget.max_inputs
            || gates.len() > budget.max_gates
            || outputs.len() > budget.max_outputs
        {
            return Err(BooleanDeltaError::BudgetExceeded);
        }
        let mut edges = 0usize;
        for (index, &gate) in gates.iter().enumerate() {
            let (a, b) = sources(gate);
            validate_wire(a, input_count, index).map_err(BooleanDeltaError::InvalidCircuit)?;
            edges = edges
                .checked_add(1)
                .ok_or(BooleanDeltaError::EdgeCountOverflow)?;
            if let Some(b) = b {
                validate_wire(b, input_count, index).map_err(BooleanDeltaError::InvalidCircuit)?;
                edges = edges
                    .checked_add(1)
                    .ok_or(BooleanDeltaError::EdgeCountOverflow)?;
            }
            if edges > budget.max_edges {
                return Err(BooleanDeltaError::BudgetExceeded);
            }
        }
        for &output in &outputs {
            validate_wire(output, input_count, gates.len())
                .map_err(BooleanDeltaError::InvalidCircuit)?;
        }
        let mut input_fanout = vec![Vec::new(); input_count];
        let mut gate_fanout = vec![Vec::new(); gates.len()];
        for (index, &gate) in gates.iter().enumerate() {
            let (a, b) = sources(gate);
            for wire in [Some(a), b].into_iter().flatten() {
                match wire {
                    Wire::Input(source) => input_fanout[source as usize].push(index),
                    Wire::Gate(source) => gate_fanout[source as usize].push(index),
                }
            }
        }
        let inputs = vec![false; input_count];
        let mut values = Vec::with_capacity(gates.len());
        for &gate in &gates {
            values.push(evaluate(gate, &inputs, &values));
        }
        Ok(Self {
            inputs,
            dirty: vec![false; gates.len()],
            gates,
            outputs,
            values,
            input_fanout,
            gate_fanout,
        })
    }

    /// Apply an entire new input assignment. Wrong-sized input is rejected
    /// before any state mutation. Each gate is evaluated at most once.
    pub fn apply(&mut self, next_inputs: &[bool]) -> Result<BooleanDeltaWork, BooleanDeltaError> {
        if next_inputs.len() != self.inputs.len() {
            return Err(BooleanDeltaError::InputCount {
                expected: self.inputs.len(),
                actual: next_inputs.len(),
            });
        }
        self.dirty.fill(false);
        let mut work = BooleanDeltaWork::default();
        for (index, &next) in next_inputs.iter().enumerate() {
            if self.inputs[index] != next {
                self.inputs[index] = next;
                work.changed_inputs += 1;
                for &consumer in &self.input_fanout[index] {
                    self.dirty[consumer] = true;
                }
            }
        }
        // Topological order: a newly dirtied consumer always has a greater
        // index, including reconvergent paths and duplicate input edges.
        for index in 0..self.gates.len() {
            if !self.dirty[index] {
                continue;
            }
            work.evaluated_gates += 1;
            let next = evaluate(self.gates[index], &self.inputs, &self.values);
            if next != self.values[index] {
                self.values[index] = next;
                work.changed_gates += 1;
                for &consumer in &self.gate_fanout[index] {
                    self.dirty[consumer] = true;
                }
            }
        }
        Ok(work)
    }

    /// Reset the assignment without retaining state from previous queries.
    pub fn reset(&mut self, inputs: &[bool]) -> Result<BooleanDeltaWork, BooleanDeltaError> {
        self.apply(inputs)
    }

    #[must_use]
    pub fn outputs(&self) -> Vec<bool> {
        self.outputs
            .iter()
            .map(|&wire| read(wire, &self.inputs, &self.values))
            .collect()
    }

    #[must_use]
    pub fn inputs(&self) -> &[bool] {
        &self.inputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BooleanCircuit, ExecutionCounters};

    fn budget() -> BooleanDeltaBudget {
        BooleanDeltaBudget {
            max_inputs: 8,
            max_gates: 256,
            max_edges: 512,
            max_outputs: 16,
        }
    }

    fn circuit() -> (Vec<Gate>, Vec<Wire>) {
        let gates = vec![
            Gate::Xor(Wire::Input(0), Wire::Input(1)),
            Gate::And(Wire::Gate(0), Wire::Input(2)),
            Gate::Not(Wire::Gate(1)),
            Gate::Or(Wire::Gate(0), Wire::Gate(2)),
            Gate::Xor(Wire::Gate(3), Wire::Input(0)),
        ];
        let outputs = vec![Wire::Gate(4), Wire::Gate(2), Wire::Input(1)];
        (gates, outputs)
    }

    #[test]
    fn all_assignments_and_transitions_match_independent_full_oracle() {
        let (gates, outputs) = circuit();
        let oracle = BooleanCircuit::new(3, gates.clone(), outputs.clone());
        let mut delta = BooleanDeltaEngine::new(3, gates, outputs, budget()).unwrap();
        for first in 0..8 {
            for second in 0..8 {
                let a = [first & 1 != 0, first & 2 != 0, first & 4 != 0];
                let b = [second & 1 != 0, second & 2 != 0, second & 4 != 0];
                delta.reset(&a).unwrap();
                assert_eq!(
                    delta.outputs(),
                    oracle
                        .evaluate(&a, &mut ExecutionCounters::default())
                        .unwrap()
                );
                let work = delta.apply(&b).unwrap();
                assert!(work.evaluated_gates <= 5);
                assert!(work.changed_gates <= work.evaluated_gates);
                assert_eq!(
                    delta.outputs(),
                    oracle
                        .evaluate(&b, &mut ExecutionCounters::default())
                        .unwrap()
                );
            }
        }
    }

    #[test]
    fn stable_intermediate_stops_dirty_propagation_and_noop_does_no_work() {
        let gates = vec![
            Gate::And(Wire::Input(0), Wire::Input(1)),
            Gate::Not(Wire::Gate(0)),
            Gate::Xor(Wire::Gate(1), Wire::Input(2)),
        ];
        let mut engine = BooleanDeltaEngine::new(3, gates, vec![Wire::Gate(2)], budget()).unwrap();
        // Input 1 stays false, so changing input 0 cannot change the AND.
        assert_eq!(
            engine.apply(&[true, false, false]).unwrap(),
            BooleanDeltaWork {
                changed_inputs: 1,
                evaluated_gates: 1,
                changed_gates: 0,
            }
        );
        assert_eq!(engine.outputs(), vec![true]);
        assert_eq!(
            engine.apply(&[true, false, false]).unwrap(),
            BooleanDeltaWork::default()
        );
        assert_eq!(engine.apply(&[true, true, false]).unwrap().changed_gates, 3);
        assert_eq!(engine.outputs(), vec![false]);
    }

    #[test]
    fn invalid_circuits_and_budgets_fail_closed() {
        assert!(matches!(
            BooleanDeltaEngine::new(1, vec![Gate::Not(Wire::Gate(0))], vec![], budget()),
            Err(BooleanDeltaError::InvalidCircuit(
                CircuitError::ForwardOrCyclicGateReference { .. }
            ))
        ));
        assert!(matches!(
            BooleanDeltaEngine::new(1, vec![], vec![Wire::Input(1)], budget()),
            Err(BooleanDeltaError::InvalidCircuit(
                CircuitError::InvalidInputWire(1)
            ))
        ));
        assert_eq!(
            BooleanDeltaEngine::new(
                1,
                vec![Gate::Not(Wire::Input(0))],
                vec![],
                BooleanDeltaBudget {
                    max_edges: 0,
                    ..budget()
                }
            ),
            Err(BooleanDeltaError::BudgetExceeded)
        );
        let (gates, outputs) = circuit();
        let mut engine = BooleanDeltaEngine::new(3, gates, outputs, budget()).unwrap();
        let before = engine.clone();
        assert_eq!(
            engine.apply(&[true]),
            Err(BooleanDeltaError::InputCount {
                expected: 3,
                actual: 1
            })
        );
        assert_eq!(engine, before);
    }

    #[test]
    fn wide_chain_and_reconvergent_fanout_match_full_oracle() {
        let mut gates = vec![Gate::Xor(Wire::Input(0), Wire::Input(1))];
        for index in 1..129 {
            gates.push(Gate::Xor(
                Wire::Gate((index - 1) as u32),
                Wire::Input((index % 2) as u32),
            ));
        }
        let outputs = vec![Wire::Gate(128), Wire::Gate(63), Wire::Gate(64)];
        let oracle = BooleanCircuit::new(2, gates.clone(), outputs.clone());
        let mut delta = BooleanDeltaEngine::new(2, gates, outputs, budget()).unwrap();
        for bits in [
            [false, false],
            [true, false],
            [true, true],
            [false, true],
            [false, false],
        ] {
            delta.apply(&bits).unwrap();
            assert_eq!(
                delta.outputs(),
                oracle
                    .evaluate(&bits, &mut ExecutionCounters::default())
                    .unwrap()
            );
        }
    }
}

/// A bounded supervised Boolean readout trained from the delta engine's
/// Boolean output features. Each feature signature has two integer vote
/// counts; inference uses the majority label, with ties predicting false.
/// This is a trainable truth-table baseline, not evidence of generalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooleanDeltaLearner {
    engine: BooleanDeltaEngine,
    votes: Vec<(u64, u64)>,
    max_examples: u64,
    examples_seen: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanDeltaLearningError {
    Engine(BooleanDeltaError),
    FeatureBudgetExceeded,
    TrainingBudgetExceeded,
}

impl From<BooleanDeltaError> for BooleanDeltaLearningError {
    fn from(error: BooleanDeltaError) -> Self {
        Self::Engine(error)
    }
}

/// Exact logical work and pre-update mistake for one supervised example.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanDeltaLearningStep {
    pub work: BooleanDeltaWork,
    pub mistake_before_update: bool,
    pub examples_seen: u64,
}

impl BooleanDeltaLearner {
    /// Limit the readout to at most 12 Boolean features (4096 vote buckets).
    /// `max_examples` is an explicit upper bound on all learning updates.
    pub fn new(
        engine: BooleanDeltaEngine,
        max_feature_bits: usize,
        max_examples: u64,
    ) -> Result<Self, BooleanDeltaLearningError> {
        let features = engine.outputs.len();
        if features > max_feature_bits || features > 12 {
            return Err(BooleanDeltaLearningError::FeatureBudgetExceeded);
        }
        let buckets = 1usize << features;
        Ok(Self {
            engine,
            votes: vec![(0, 0); buckets],
            max_examples,
            examples_seen: 0,
        })
    }

    fn signature(&self) -> usize {
        self.engine
            .outputs
            .iter()
            .enumerate()
            .fold(0usize, |mask, (index, &wire)| {
                mask | (usize::from(read(wire, &self.engine.inputs, &self.engine.values)) << index)
            })
    }

    fn prediction(&self) -> bool {
        let (negative, positive) = self.votes[self.signature()];
        positive > negative
    }

    /// Predict without updating learned vote counts. The engine's execution
    /// cache may change, but its learned state remains untouched.
    pub fn predict(
        &mut self,
        inputs: &[bool],
    ) -> Result<(bool, BooleanDeltaWork), BooleanDeltaLearningError> {
        let work = self.engine.apply(inputs)?;
        Ok((self.prediction(), work))
    }

    /// Online supervised learning with deterministic majority-vote updates.
    /// Rejected input lengths or exhausted training budgets do not mutate state.
    pub fn train(
        &mut self,
        inputs: &[bool],
        label: bool,
    ) -> Result<BooleanDeltaLearningStep, BooleanDeltaLearningError> {
        if inputs.len() != self.engine.inputs.len() {
            return Err(BooleanDeltaLearningError::Engine(
                BooleanDeltaError::InputCount {
                    expected: self.engine.inputs.len(),
                    actual: inputs.len(),
                },
            ));
        }
        if self.examples_seen >= self.max_examples {
            return Err(BooleanDeltaLearningError::TrainingBudgetExceeded);
        }
        let work = self.engine.apply(inputs)?;
        let signature = self.signature();
        let mistake_before_update = self.prediction() != label;
        let (negative, positive) = &mut self.votes[signature];
        if label {
            *positive += 1;
        } else {
            *negative += 1;
        }
        self.examples_seen += 1;
        Ok(BooleanDeltaLearningStep {
            work,
            mistake_before_update,
            examples_seen: self.examples_seen,
        })
    }

    #[must_use]
    pub fn examples_seen(&self) -> u64 {
        self.examples_seen
    }

    #[must_use]
    pub fn learned_table_bytes(&self) -> usize {
        self.votes.len() * core::mem::size_of::<(u64, u64)>()
    }
}

#[cfg(test)]
mod learning_tests {
    use super::*;
    use crate::{BooleanCircuit, ExecutionCounters};
    use std::hint::black_box;
    use std::time::Instant;

    fn learner(max_examples: u64) -> BooleanDeltaLearner {
        let gates = vec![Gate::Xor(Wire::Input(0), Wire::Input(1))];
        let outputs = vec![Wire::Gate(0), Wire::Input(2)];
        let engine = BooleanDeltaEngine::new(
            4,
            gates,
            outputs,
            BooleanDeltaBudget {
                max_inputs: 4,
                max_gates: 1,
                max_edges: 2,
                max_outputs: 2,
            },
        )
        .unwrap();
        BooleanDeltaLearner::new(engine, 2, max_examples).unwrap()
    }

    fn sample(bits: usize, nuisance: bool) -> ([bool; 4], bool) {
        let a = bits & 1 != 0;
        let b = bits & 2 != 0;
        let c = bits & 4 != 0;
        ([a, b, c, nuisance], (a ^ b) & c)
    }

    fn heldout_errors(model: &mut BooleanDeltaLearner) -> usize {
        (0..8)
            .filter(|&bits| {
                let (inputs, label) = sample(bits, true);
                model.predict(&inputs).unwrap().0 != label
            })
            .count()
    }

    #[test]
    fn supervised_updates_reduce_errors_on_distinct_heldout_inputs() {
        let mut model = learner(8);
        // Train and test use distinct full assignments (nuisance bit differs),
        // but intentionally share readout feature signatures. This establishes
        // learnability of the readout, not out-of-distribution generalization.
        assert_eq!(heldout_errors(&mut model), 2);
        let mut prequential_mistakes = 0;
        for bits in 0..8 {
            let (inputs, label) = sample(bits, false);
            let step = model.train(&inputs, label).unwrap();
            prequential_mistakes += usize::from(step.mistake_before_update);
        }
        assert_eq!(prequential_mistakes, 1);
        assert_eq!(model.examples_seen(), 8);
        assert_eq!(model.learned_table_bytes(), 64);
        assert_eq!(heldout_errors(&mut model), 0);
    }

    #[test]
    fn training_budget_and_invalid_input_fail_without_mutation() {
        let mut model = learner(1);
        let before = model.clone();
        assert_eq!(
            model.train(&[false], true),
            Err(BooleanDeltaLearningError::Engine(
                BooleanDeltaError::InputCount {
                    expected: 4,
                    actual: 1,
                }
            ))
        );
        assert_eq!(model, before);
        model.train(&[true, false, true, false], true).unwrap();
        let after = model.clone();
        assert_eq!(
            model.train(&[false, false, false, false], false),
            Err(BooleanDeltaLearningError::TrainingBudgetExceeded)
        );
        assert_eq!(model, after);
        assert_eq!(
            BooleanDeltaLearner::new(model.engine.clone(), 1, 10),
            Err(BooleanDeltaLearningError::FeatureBudgetExceeded)
        );
    }

    #[test]
    fn measured_quality_work_and_cpu_time_against_full_circuit() {
        // Synthetic, fixed-order CPU microbenchmark; time is observational.
        // Do not interpret one CI runner's timings as a speedup or model claim.
        let mut model = learner(8);
        let before_errors = heldout_errors(&mut model);
        let training_start = Instant::now();
        for bits in 0..8 {
            let (inputs, label) = sample(bits, false);
            model.train(&inputs, label).unwrap();
        }
        let train_ns = training_start.elapsed().as_nanos();
        let after_errors = heldout_errors(&mut model);
        assert_eq!((before_errors, after_errors), (2, 0));

        let oracle = BooleanCircuit::new(
            4,
            vec![Gate::Xor(Wire::Input(0), Wire::Input(1))],
            vec![Wire::Gate(0), Wire::Input(2)],
        );
        let iterations = 20_000usize;
        let mut evaluated_gates = 0usize;
        let mut errors = 0usize;
        let start = Instant::now();
        for index in 0..iterations {
            let (inputs, label) = sample(index % 8, index & 8 != 0);
            let (prediction, work) = model.predict(&inputs).unwrap();
            evaluated_gates += work.evaluated_gates;
            errors += usize::from(black_box(prediction) != label);
        }
        let incremental_ns = start.elapsed().as_nanos();
        assert_eq!(errors, 0);

        let mut oracle_errors = 0usize;
        let oracle_start = Instant::now();
        for index in 0..iterations {
            let (inputs, label) = sample(index % 8, index & 8 != 0);
            let outputs = oracle
                .evaluate(&inputs, &mut ExecutionCounters::default())
                .unwrap();
            oracle_errors += usize::from(black_box(outputs[0] & outputs[1]) != label);
        }
        let oracle_ns = oracle_start.elapsed().as_nanos();
        assert_eq!(oracle_errors, 0);
        println!(
            "BDE0_LEARNING_MEASURE_V1 train_samples=8 heldout_samples=8 before_errors={before_errors} after_errors={after_errors} train_ns={train_ns} infer_samples={iterations} infer_errors={errors} incremental_gate_evaluations={evaluated_gates} incremental_ns={incremental_ns} oracle_ns={oracle_ns} learned_table_bytes={}",
            model.learned_table_bytes()
        );
    }
}
