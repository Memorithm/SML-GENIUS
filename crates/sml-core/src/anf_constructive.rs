//! A6-C1 constructive, residual-supervised learning of local recurrent ANF gates.
//!
//! The output target is trained as the XOR residual of a frozen direct-input
//! baseline. The newly fitted gate is applied to the current input and prior
//! recurrent state, producing the next state BEFORE the output is evaluated.
//! No label or future input is a gate source. This is a small-fan-in, bounded
//! exhaustive synergy search: unlike one-variable forward selection it can
//! recover a pure parity interaction with zero marginal correlation.
//!
//! Gate truth tables are fitted by train-only majority and compiled into GF(2)
//! ANF coefficients through the existing Mobius transform. A separate validation
//! comparison controls whether each gate is actually committed. Test data must
//! only be scored by the caller after all gate choices have been frozen.
//!
//! The construction search is intentionally limited to <=16 candidate sources
//! and <=4 selected inputs. These bounds are NOT a large-bank scaling claim.

use std::fmt;

use crate::anf_local::{LocalAnfError, LocalAnfGate, LocalAnfNetwork, LOCAL_ANF_MAX_FAN_IN};
use crate::anf_local_incremental::{LaneData, Trajectory};

pub const MAX_CONSTRUCTIVE_CANDIDATES: usize = 16;
pub const MAX_CONSTRUCTIVE_FAN_IN: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructiveError {
    Network(LocalAnfError),
    Shape,
    Candidate(usize),
    FanIn(usize),
    Empty,
}

impl fmt::Display for ConstructiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ConstructiveError {}

impl From<LocalAnfError> for ConstructiveError {
    fn from(value: LocalAnfError) -> Self {
        Self::Network(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstructedGate {
    pub gate: LocalAnfGate,
    /// Input positions in [previous state, current inputs].
    pub inputs: Vec<usize>,
    pub train_errors: u64,
    pub train_samples: u64,
    pub evaluated_subsets: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstructionDecision {
    pub committed: bool,
    pub baseline_validation_errors: u64,
    pub candidate_validation_errors: u64,
    pub validation_samples: u64,
}

fn validate_data(
    network: &LocalAnfNetwork,
    data: &LaneData,
    target_bit: usize,
    baseline_input: usize,
) -> Result<u64, ConstructiveError> {
    if data.input_bits != network.input_bits()
        || target_bit >= data.target_bits
        || baseline_input >= data.input_bits
    {
        return Err(ConstructiveError::Shape);
    }
    let mut samples = 0u64;
    for block in &data.blocks {
        if block.steps == 0
            || block.valid == 0
            || block.inputs.len() != block.steps * data.input_bits
            || block.targets.len() != block.steps * data.target_bits
        {
            return Err(ConstructiveError::Shape);
        }
        samples += u64::from(block.valid.count_ones()) * block.steps as u64;
    }
    if samples == 0 {
        return Err(ConstructiveError::Empty);
    }
    Ok(samples)
}

/// True when a frozen candidate state may depend (directly or transitively)
/// on the gate we intend to replace. Such candidates cannot be fitted from a
/// cached pre-construction trajectory without causing stale-feature leakage.
fn depends_on_gate(network: &LocalAnfNetwork, candidate: usize, gate_index: usize) -> bool {
    let n = network.state_bits();
    if candidate >= n {
        return false;
    }
    let mut seen = vec![false; n];
    let mut stack = vec![candidate];
    while let Some(g) = stack.pop() {
        if g == gate_index {
            return true;
        }
        if seen[g] {
            continue;
        }
        seen[g] = true;
        stack.extend(
            network.gates()[g]
                .inputs()
                .iter()
                .copied()
                .filter(|&source| source < n),
        );
    }
    false
}

/// Iterate fixed-size source subsets in deterministic candidate order.
fn visit_subsets<F: FnMut(&[usize])>(
    candidates: &[usize],
    k: usize,
    start: usize,
    selected: &mut Vec<usize>,
    visit: &mut F,
) {
    if selected.len() == k {
        visit(selected);
        return;
    }
    let remaining = k - selected.len();
    for i in start..=candidates.len() - remaining {
        selected.push(candidates[i]);
        visit_subsets(candidates, k, i + 1, selected, visit);
        selected.pop();
    }
}

fn residual_counts(
    network: &LocalAnfNetwork,
    data: &LaneData,
    trajectory: &Trajectory,
    inputs: &[usize],
    target_bit: usize,
    baseline_input: usize,
) -> Vec<[u64; 2]> {
    let n = network.state_bits();
    let mut counts = vec![[0u64; 2]; 1 << inputs.len()];
    for (bi, block) in data.blocks.iter().enumerate() {
        for t in 0..block.steps {
            // Trajectory row(t-1) is s_t (post-update of step t-1).
            // The candidate gate itself is evaluated at step t from (s_t, x_t).
            let previous = (t > 0).then(|| trajectory.row(bi, t - 1));
            let mut words = [0u64; LOCAL_ANF_MAX_FAN_IN];
            for (j, &source) in inputs.iter().enumerate() {
                words[j] = if source >= n {
                    block.inputs[t * data.input_bits + source - n]
                } else {
                    previous.map_or(0, |row| row[source])
                };
            }
            let label = block.targets[t * data.target_bits + target_bit]
                ^ block.inputs[t * data.input_bits + baseline_input];
            for (address, counts_for_address) in counts.iter_mut().enumerate() {
                let mut mask = block.valid;
                for (j, &word) in words.iter().take(inputs.len()).enumerate() {
                    mask &= if (address >> j) & 1 != 0 { word } else { !word };
                }
                counts_for_address[1] += u64::from((mask & label).count_ones());
                counts_for_address[0] += u64::from((mask & !label).count_ones());
            }
        }
    }
    counts
}

/// Fit ONE previously unused state gate on the XOR residual of a direct input.
///
/// All coefficients, input-source choices and ANF monomials use training data
/// only; zero-marginal XOR interactions require a complete joint source subset,
/// so search enumerates all subsets from sizes 1..=max_fan_in, not greedy
/// singleton improvements. Equal training errors retain the smaller fan-in,
/// then the earliest subset in deterministic candidate order.
///
/// Candidates cannot include the target gate or any other gate downstream of
/// it (even transitively), so cached source trajectories stay valid.
pub fn fit_residual_gate(
    network: &LocalAnfNetwork,
    train: &LaneData,
    gate_index: usize,
    target_bit: usize,
    baseline_input: usize,
    candidates: &[usize],
    max_fan_in: usize,
) -> Result<ConstructedGate, ConstructiveError> {
    let samples = validate_data(network, train, target_bit, baseline_input)?;
    if gate_index >= network.state_bits() {
        return Err(ConstructiveError::Candidate(gate_index));
    }
    if candidates.is_empty() || candidates.len() > MAX_CONSTRUCTIVE_CANDIDATES {
        return Err(ConstructiveError::Shape);
    }
    if max_fan_in == 0 || max_fan_in > MAX_CONSTRUCTIVE_FAN_IN || max_fan_in > candidates.len() {
        return Err(ConstructiveError::FanIn(max_fan_in));
    }
    for (j, &source) in candidates.iter().enumerate() {
        if source >= network.source_width()
            || candidates[..j].contains(&source)
            || depends_on_gate(network, source, gate_index)
        {
            return Err(ConstructiveError::Candidate(source));
        }
    }
    let trajectory = Trajectory::simulate(network, train)?;
    let mut best_errors = u64::MAX;
    let mut best_counts = Vec::new();
    let mut best_inputs = Vec::new();
    let mut examined = 0usize;
    for k in 1..=max_fan_in {
        visit_subsets(candidates, k, 0, &mut Vec::new(), &mut |subset| {
            examined += 1;
            let counts = residual_counts(
                network,
                train,
                &trajectory,
                subset,
                target_bit,
                baseline_input,
            );
            let errors = counts.iter().map(|c| c[0].min(c[1])).sum();
            if errors < best_errors {
                best_errors = errors;
                best_counts = counts;
                best_inputs = subset.to_vec();
            }
        });
    }
    let truth_table: Vec<bool> = best_counts.iter().map(|c| c[1] > c[0]).collect();
    let gate =
        LocalAnfGate::from_truth_table(best_inputs.clone(), &truth_table, network.source_width())?;
    Ok(ConstructedGate {
        gate,
        inputs: best_inputs,
        train_errors: best_errors,
        train_samples: samples,
        evaluated_subsets: examined,
    })
}

/// Score XOR(baseline current input, hidden state gate) with the existing
/// native two-input ANF readout; no teacher forcing or future targets enter
/// the model. Only the actual label is used to count errors.
pub fn residual_output_errors(
    network: &LocalAnfNetwork,
    data: &LaneData,
    gate_index: usize,
    target_bit: usize,
    baseline_input: usize,
) -> Result<(u64, u64), ConstructiveError> {
    let samples = validate_data(network, data, target_bit, baseline_input)?;
    if gate_index >= network.state_bits() {
        return Err(ConstructiveError::Candidate(gate_index));
    }
    let readout = LocalAnfGate::from_truth_table(
        vec![network.state_bits() + baseline_input, gate_index],
        &[false, true, true, false],
        network.source_width(),
    )?;
    let traj = Trajectory::simulate(network, data)?;
    let mut scratch = [0u64; 4];
    let mut errors = 0u64;
    for (bi, block) in data.blocks.iter().enumerate() {
        for t in 0..block.steps {
            let predicted = readout.evaluate_lanes(traj.row(bi, t), &mut scratch);
            let actual = block.targets[t * data.target_bits + target_bit];
            errors += u64::from(((predicted ^ actual) & block.valid).count_ones());
        }
    }
    Ok((errors, samples))
}

/// Validate a train-fitted candidate WITHOUT mutating the network if it fails.
/// Strictly lower validation errors are required to commit the new state gate.
pub fn validate_and_commit(
    network: &mut LocalAnfNetwork,
    validation: &LaneData,
    gate_index: usize,
    target_bit: usize,
    baseline_input: usize,
    fitted: &ConstructedGate,
) -> Result<ConstructionDecision, ConstructiveError> {
    if gate_index >= network.state_bits() {
        return Err(ConstructiveError::Candidate(gate_index));
    }
    let (baseline_errors, samples) =
        residual_output_errors(network, validation, gate_index, target_bit, baseline_input)?;
    let mut candidate = network.clone();
    candidate.gates_mut()[gate_index] = fitted.gate.clone();
    let (candidate_errors, candidate_samples) = residual_output_errors(
        &candidate,
        validation,
        gate_index,
        target_bit,
        baseline_input,
    )?;
    if samples != candidate_samples {
        return Err(ConstructiveError::Shape);
    }
    let committed = candidate_errors < baseline_errors;
    if committed {
        network.gates_mut()[gate_index] = fitted.gate.clone();
    }
    Ok(ConstructionDecision {
        committed,
        baseline_validation_errors: baseline_errors,
        candidate_validation_errors: candidate_errors,
        validation_samples: samples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anf_local_incremental::LaneBlock;
    use crate::toy_tasks::SplitMix64;

    fn fixture_network() -> LocalAnfNetwork {
        // One constructible gate (0) and two history lags for two input bits.
        let n = 5;
        let width = n + 2;
        let mut gates = vec![LocalAnfGate::zero(vec![n], width).unwrap()];
        for p in 0..2 {
            let mut g = LocalAnfGate::zero(vec![n + p], width).unwrap();
            g.flip(1);
            gates.push(g);
        }
        for p in 0..2 {
            let mut g = LocalAnfGate::zero(vec![1 + p], width).unwrap();
            g.flip(1);
            gates.push(g);
        }
        let readout = LocalAnfGate::zero(vec![n], width).unwrap();
        LocalAnfNetwork::with_input_bits(n, 2, gates, readout).unwrap()
    }

    fn xor_fixture(seed: u64) -> LaneData {
        let steps = 48;
        let mut rng = SplitMix64::new(seed);
        let mut inputs = vec![0u64; steps * 2];
        for value in &mut inputs {
            *value = rng.next_u64();
        }
        let mut targets = vec![0u64; steps];
        for t in 0..steps {
            let a = inputs[t * 2];
            let b = if t > 0 { inputs[(t - 1) * 2 + 1] } else { 0 };
            let c = if t > 1 { inputs[(t - 2) * 2] } else { 0 };
            // Residual is a three-way parity with zero singleton credit.
            targets[t] = inputs[t * 2 + 1] ^ a ^ b ^ c;
        }
        LaneData {
            input_bits: 2,
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
    fn joint_search_finds_zero_marginal_three_way_parity() {
        let network = fixture_network();
        let training = xor_fixture(7);
        let validation = xor_fixture(17);
        let test = xor_fixture(41);
        let sources = [1, 2, 3, 4, 5, 6];
        let frozen = network.clone();
        let one = fit_residual_gate(&network, &training, 0, 0, 1, &sources, 1).unwrap();
        let pair = fit_residual_gate(&network, &training, 0, 0, 1, &sources, 2).unwrap();
        let triple = fit_residual_gate(&network, &training, 0, 0, 1, &sources, 3).unwrap();
        assert_eq!(triple.train_errors, 0);
        assert_eq!(triple.inputs, vec![2, 3, 5]);
        assert_eq!(triple.gate.nonzero_monomials(), 3);
        assert!(one.train_errors > 0);
        assert!(pair.train_errors > 0);
        assert_eq!(network, frozen);

        let mut trained = network.clone();
        let decision = validate_and_commit(&mut trained, &validation, 0, 0, 1, &triple).unwrap();
        assert!(decision.committed);
        assert_eq!(decision.candidate_validation_errors, 0);
        let (errors, samples) = residual_output_errors(&trained, &test, 0, 0, 1).unwrap();
        assert_eq!(errors, 0);
        assert_eq!(samples, 48 * 64);
        assert_eq!(trained.gates()[0], triple.gate);
        assert_eq!(&trained.gates()[1..], &network.gates()[1..]);
    }

    #[test]
    fn candidate_dependency_is_rejected_even_indirectly() {
        let mut network = fixture_network();
        let training = xor_fixture(3);
        assert_eq!(
            fit_residual_gate(&network, &training, 0, 0, 1, &[0, 5], 2),
            Err(ConstructiveError::Candidate(0))
        );
        let mut downstream = LocalAnfGate::zero(vec![0], network.source_width()).unwrap();
        downstream.flip(1);
        network.gates_mut()[1] = downstream;
        assert_eq!(
            fit_residual_gate(&network, &training, 0, 0, 1, &[1, 5], 2),
            Err(ConstructiveError::Candidate(1))
        );
        assert_eq!(
            fit_residual_gate(&network, &training, 0, 0, 1, &[5, 5], 2),
            Err(ConstructiveError::Candidate(5))
        );
    }

    #[test]
    fn fitting_never_reads_validation_or_mutates_original_network() {
        let network = fixture_network();
        let original = network.clone();
        let train = xor_fixture(123);
        let mut val = xor_fixture(321);
        let sources = [1, 2, 3, 4, 5, 6];
        let first = fit_residual_gate(&network, &train, 0, 0, 1, &sources, 3).unwrap();
        for word in &mut val.blocks[0].targets {
            *word = !*word;
        }
        let again = fit_residual_gate(&network, &train, 0, 0, 1, &sources, 3).unwrap();
        assert_eq!(first, again);
        assert_eq!(network, original);
    }
}
