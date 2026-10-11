//! A6-C2: end-task-only credit assignment across two recurrent ANF gates.
//!
//! Gate 0 constructs an intermediate feature from two CURRENT raw input bits.
//! Gate 1 reads the PREVIOUS state of gate 0 plus one current raw input.
//! Both gates are optimized jointly against only the observable final target;
//! neither receives a hidden label or a per-gate target.
//!
//! Search enumerates a bounded 2x2 Boolean truth-table grammar; it is NOT
//! unconstrained program synthesis, implicit differentiation or a 25B result.

use std::fmt;

use crate::{
    anf_constructive::residual_output_errors,
    anf_local::{LocalAnfError, LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::LaneData,
};

pub const MAX_CREDIT_INPUT_BITS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreditError {
    Network(LocalAnfError),
    InvalidShape,
    InvalidSpec,
    ScoreMismatch,
}
impl fmt::Display for CreditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CreditError {}
impl From<LocalAnfError> for CreditError {
    fn from(value: LocalAnfError) -> Self {
        Self::Network(value)
    }
}

/// Restricts an interpretable pair search to two state gates and raw inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoupledCreditSpec {
    pub intermediate_gate: usize,
    pub downstream_gate: usize,
    pub raw_inputs: Vec<usize>,
    pub target_bit: usize,
    pub baseline_input: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoupledCreditReport {
    pub proposed: LocalAnfNetwork,
    pub committed: LocalAnfNetwork,
    pub coordinate: LocalAnfNetwork,
    pub baseline_train_errors: u64,
    pub baseline_validation_errors: u64,
    pub coordinate_train_errors: u64,
    pub coordinate_validation_errors: u64,
    pub joint_train_errors: u64,
    pub joint_validation_errors: u64,
    pub samples_train: u64,
    pub samples_validation: u64,
    pub searched_pairs: usize,
    pub coordinate_proposals: usize,
    pub committed_joint: bool,
    pub committed_coordinate: bool,
}

fn check_data(network: &LocalAnfNetwork, data: &LaneData, spec: &CoupledCreditSpec) -> Result<u64, CreditError> {
    if data.input_bits != network.input_bits()
        || spec.target_bit >= data.target_bits
        || data.blocks.is_empty()
    {
        return Err(CreditError::InvalidShape);
    }
    let mut samples = 0u64;
    for block in &data.blocks {
        if block.steps == 0 || block.valid == 0
            || block.inputs.len() != block.steps * data.input_bits
            || block.targets.len() != block.steps * data.target_bits
        {
            return Err(CreditError::InvalidShape);
        }
        samples += u64::from(block.valid.count_ones()) * block.steps as u64;
    }
    Ok(samples)
}

fn check_spec(network: &LocalAnfNetwork, spec: &CoupledCreditSpec) -> Result<(), CreditError> {
    let n = network.state_bits();
    let w = network.input_bits();
    if n != 2 || spec.intermediate_gate >= n || spec.downstream_gate >= n
        || spec.intermediate_gate == spec.downstream_gate
        || spec.baseline_input >= w || w > MAX_CREDIT_INPUT_BITS
        || spec.raw_inputs.len() < 2 || spec.raw_inputs.len() > w
    {
        return Err(CreditError::InvalidSpec);
    }
    for (i, &source) in spec.raw_inputs.iter().enumerate() {
        if source >= w || spec.raw_inputs[..i].contains(&source) {
            return Err(CreditError::InvalidSpec);
        }
    }
    if network.gates()[spec.intermediate_gate].truth_table().iter().any(|&v| v)
        || network.gates()[spec.downstream_gate].truth_table().iter().any(|&v| v)
    {
        return Err(CreditError::InvalidSpec);
    }
    Ok(())
}

fn gate_from_mask(inputs: Vec<usize>, mask: u8, width: usize) -> Result<LocalAnfGate, CreditError> {
    let table: Vec<bool> = (0..4).map(|a| mask & (1 << a) != 0).collect();
    Ok(LocalAnfGate::from_truth_table(inputs, &table, width)?)
}

fn grammar(network: &LocalAnfNetwork, spec: &CoupledCreditSpec) -> Result<(Vec<LocalAnfGate>, Vec<LocalAnfGate>), CreditError> {
    let n = network.state_bits();
    let mut upstream = Vec::new();
    let mut downstream = Vec::new();
    for (i, &a) in spec.raw_inputs.iter().enumerate() {
        for &b in &spec.raw_inputs[i + 1..] {
            for mask in 0..16u8 {
                upstream.push(gate_from_mask(vec![n + a, n + b], mask, network.source_width())?);
            }
        }
    }
    for &raw in &spec.raw_inputs {
        for mask in 0..16u8 {
            downstream.push(gate_from_mask(
                vec![spec.intermediate_gate, n + raw],
                mask,
                network.source_width(),
            )?);
        }
    }
    Ok((upstream, downstream))
}

/// Gate-0 outputs at each step, representing s_(t+1), across 64 SIMD lanes.
fn intermediate_words(upstream: &LocalAnfGate, data: &LaneData, n: usize) -> Vec<Vec<u64>> {
    let mut all = Vec::with_capacity(data.blocks.len());
    let mut products = [0u64; 4];
    let mut values = [0u64; 2];
    for block in &data.blocks {
        let mut words = Vec::with_capacity(block.steps);
        for t in 0..block.steps {
            for (slot, &source) in values.iter_mut().zip(upstream.inputs()) {
                *slot = block.inputs[t * data.input_bits + source - n];
            }
            words.push(upstream.evaluate_gathered(&values, &mut products));
        }
        all.push(words);
    }
    all
}

/// Causal end-task score: downstream reads upstream at t-1, never at t.
/// A target label is used only to score the observable final-output residual.
fn end_task_errors(
    previous_gate_words: &[Vec<u64>],
    downstream: &LocalAnfGate,
    data: &LaneData,
    spec: &CoupledCreditSpec,
    n: usize,
) -> u64 {
    let raw = downstream.inputs()[1] - n;
    let mut errors = 0u64;
    let mut products = [0u64; 4];
    let mut values = [0u64; 2];
    for (bi, block) in data.blocks.iter().enumerate() {
        for t in 0..block.steps {
            values[0] = if t == 0 { 0 } else { previous_gate_words[bi][t - 1] };
            values[1] = block.inputs[t * data.input_bits + raw];
            let predicted_residual = downstream.evaluate_gathered(&values, &mut products);
            let actual_residual = block.targets[t * data.target_bits + spec.target_bit]
                ^ block.inputs[t * data.input_bits + spec.baseline_input];
            errors += u64::from(((predicted_residual ^ actual_residual) & block.valid).count_ones());
        }
    }
    errors
}

/// Independent oracle using the actual shared native recurrent execution.
fn actual_errors(network: &LocalAnfNetwork, data: &LaneData, spec: &CoupledCreditSpec) -> Result<u64, CreditError> {
    Ok(residual_output_errors(
        network,
        data,
        spec.downstream_gate,
        spec.target_bit,
        spec.baseline_input,
    )?.0)
}

fn with_pair(network: &LocalAnfNetwork, spec: &CoupledCreditSpec, first: &LocalAnfGate, second: &LocalAnfGate) -> LocalAnfNetwork {
    let mut updated = network.clone();
    updated.gates_mut()[spec.intermediate_gate] = first.clone();
    updated.gates_mut()[spec.downstream_gate] = second.clone();
    updated
}

/// Fit the best paired intervention against the final observable target only.
/// Compare to strict-improvement coordinate descent over individual gate edits.
///
/// All 6,144 pair candidates (4 inputs: 6 upstream pairs * 16 functions *
/// 4 downstream raw choices * 16 functions) are ranked by TRAIN error.
/// Val is used only to commit (or reject) the whole proposed paired model.
/// Test data is neither passed nor consulted.
pub fn fit_coupled_credit(
    network: &LocalAnfNetwork,
    train: &LaneData,
    validation: &LaneData,
    spec: &CoupledCreditSpec,
) -> Result<CoupledCreditReport, CreditError> {
    check_spec(network, spec)?;
    let samples_train = check_data(network, train, spec)?;
    let samples_validation = check_data(network, validation, spec)?;
    let (first_bank, second_bank) = grammar(network, spec)?;
    let n = network.state_bits();
    let mut tables = Vec::with_capacity(first_bank.len());
    for gate in &first_bank {
        tables.push(intermediate_words(gate, train, n));
    }
    let baseline_train_errors = actual_errors(network, train, spec)?;
    let baseline_validation_errors = actual_errors(network, validation, spec)?;
    let mut best_error = u64::MAX;
    let mut best_pair = (0usize, 0usize);
    let mut searched_pairs = 0usize;
    for (a, words) in tables.iter().enumerate() {
        for (b, gate) in second_bank.iter().enumerate() {
            searched_pairs += 1;
            let score = end_task_errors(words, gate, train, spec, n);
            if score < best_error {
                best_error = score;
                best_pair = (a, b);
            }
        }
    }
    let proposed = with_pair(network, spec, &first_bank[best_pair.0], &second_bank[best_pair.1]);
    if actual_errors(&proposed, train, spec)? != best_error {
        return Err(CreditError::ScoreMismatch);
    }
    let joint_validation_errors = actual_errors(&proposed, validation, spec)?;
    let committed_joint = joint_validation_errors < baseline_validation_errors;
    let committed = if committed_joint { proposed.clone() } else { network.clone() };

    // Coordinate baseline starts from exactly the same zero-gate model.
    // Strict objective reductions only; neutral changes do not earn credit.
    let mut coordinate = network.clone();
    let mut coordinate_train_errors = baseline_train_errors;
    let mut coordinate_proposals = 0usize;
    for _ in 0..3 {
        let prior_loss = coordinate_train_errors;
        let second = coordinate.gates()[spec.downstream_gate].clone();
        let mut best_first = None;
        for (a, words) in tables.iter().enumerate() {
            coordinate_proposals += 1;
            let score = end_task_errors(words, &second, train, spec, n);
            if score < coordinate_train_errors {
                coordinate_train_errors = score;
                best_first = Some(a);
            }
        }
        if let Some(a) = best_first {
            coordinate.gates_mut()[spec.intermediate_gate] = first_bank[a].clone();
        }
        let current_words = intermediate_words(&coordinate.gates()[spec.intermediate_gate], train, n);
        let mut best_second = None;
        for (b, gate) in second_bank.iter().enumerate() {
            coordinate_proposals += 1;
            let score = end_task_errors(&current_words, gate, train, spec, n);
            if score < coordinate_train_errors {
                coordinate_train_errors = score;
                best_second = Some(b);
            }
        }
        if let Some(b) = best_second {
            coordinate.gates_mut()[spec.downstream_gate] = second_bank[b].clone();
        }
        if coordinate_train_errors == prior_loss {
            break;
        }
    }
    if actual_errors(&coordinate, train, spec)? != coordinate_train_errors {
        return Err(CreditError::ScoreMismatch);
    }
    let coordinate_validation_errors = actual_errors(&coordinate, validation, spec)?;
    let committed_coordinate = coordinate_validation_errors < baseline_validation_errors;
    if !committed_coordinate {
        coordinate = network.clone();
    }
    Ok(CoupledCreditReport {
        proposed,
        committed,
        coordinate,
        baseline_train_errors,
        baseline_validation_errors,
        coordinate_train_errors,
        coordinate_validation_errors,
        joint_train_errors: best_error,
        joint_validation_errors,
        samples_train,
        samples_validation,
        searched_pairs,
        coordinate_proposals,
        committed_joint,
        committed_coordinate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{anf_local_incremental::LaneBlock, toy_tasks::SplitMix64};

    fn base() -> LocalAnfNetwork {
        let n = 2;
        let width = n + 4;
        let gates = vec![
            LocalAnfGate::zero(vec![n], width).unwrap(),
            LocalAnfGate::zero(vec![0, n + 1], width).unwrap(),
        ];
        let readout = LocalAnfGate::zero(vec![n], width).unwrap();
        LocalAnfNetwork::with_input_bits(n, 4, gates, readout).unwrap()
    }

    fn spec() -> CoupledCreditSpec {
        CoupledCreditSpec {
            intermediate_gate: 0,
            downstream_gate: 1,
            raw_inputs: vec![0, 1, 2, 3],
            target_bit: 0,
            baseline_input: 0,
        }
    }

    fn fixture(seed: u64, steps: usize) -> LaneData {
        let mut rng = SplitMix64::new(seed);
        let mut inputs = vec![0u64; steps * 4];
        for x in &mut inputs {
            *x = rng.next_u64();
        }
        let mut targets = vec![0u64; steps];
        for t in 0..steps {
            let x0 = inputs[4 * t];
            let x1 = inputs[4 * t + 1];
            let prior = if t == 0 {
                0
            } else {
                inputs[4 * (t - 1) + 2] ^ inputs[4 * (t - 1) + 3]
            };
            targets[t] = x0 ^ (x1 & prior);
        }
        LaneData {
            input_bits: 4,
            target_bits: 1,
            blocks: vec![LaneBlock {
                steps,
                inputs,
                targets,
                valid: u64::MAX,
            }],
        }
    }

    #[test]
    fn joint_credit_learns_hidden_delayed_parity_from_only_output_labels() {
        let model = base();
        let report = fit_coupled_credit(&model, &fixture(1, 64), &fixture(2, 32), &spec()).unwrap();
        assert_eq!(report.joint_train_errors, 0);
        assert_eq!(report.joint_validation_errors, 0);
        assert!(report.committed_joint);
        assert_eq!(report.searched_pairs, 6 * 16 * 4 * 16);
        assert_eq!(report.proposed.gates()[0].inputs(), &[4, 5]);
        assert_eq!(report.proposed.gates()[1].inputs(), &[0, 3]);
        assert_eq!(actual_errors(&report.committed, &fixture(3, 32), &spec()).unwrap(), 0);
    }

    #[test]
    fn fit_does_not_depend_on_validation_labels_and_rolls_back_when_harmful() {
        let model = base();
        let train = fixture(10, 40);
        let val = fixture(20, 30);
        let before = model.clone();
        let a = fit_coupled_credit(&model, &train, &val, &spec()).unwrap();
        let b = fit_coupled_credit(&model, &train, &val, &spec()).unwrap();
        assert_eq!(a, b);
        assert_eq!(model, before);
        let mut corrupt = val;
        for block in &mut corrupt.blocks {
            for word in &mut block.targets {
                *word = !*word;
            }
        }
        let c = fit_coupled_credit(&model, &train, &corrupt, &spec()).unwrap();
        assert_eq!(a.proposed, c.proposed);
        assert_eq!(a.joint_train_errors, c.joint_train_errors);
        assert!(!c.committed_joint);
        assert_eq!(c.committed, model);
    }

    #[test]
    fn rejects_duplicate_candidates_and_invalid_data() {
        let model = base();
        let data = fixture(3, 8);
        let mut invalid = spec();
        invalid.raw_inputs = vec![0, 1, 1, 3];
        assert_eq!(fit_coupled_credit(&model, &data, &data, &invalid), Err(CreditError::InvalidSpec));
        let mut malformed = data.clone();
        malformed.blocks[0].targets.clear();
        assert_eq!(fit_coupled_credit(&model, &malformed, &data, &spec()), Err(CreditError::InvalidShape));
    }
}
