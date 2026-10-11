//! A6-C3 terminal-only credit assignment to THREE causal recurrent ANF gates.
//!
//! Stage 0 reads two raw input channels. Stage 1 reads prior stage-0 state
//! and one current raw input. Stage 2 reads prior stage-1 state and one
//! current raw input; output is XOR(baseline input, stage-2 current state).
//! No intermediate labels or future inputs enter the train objective.
//!
//! Bounded source/function grammar: exhaustive joint three-gate intervention,
//! strict one-gate coordinate descent, and randomized coordinate restarts with
//! the same MAXIMUM number of scored hypotheses as the joint search.
//! Binary truth tables are converted to native ANF and crosschecked by
//! full LocalAnfNetwork simulation before validation commitment.
//!
//! This is a small reference learning experiment, not a scalable optimizer
//! or evidence of long-horizon language-model generalization.

use std::fmt;

use crate::{
    anf_constructive::{residual_output_errors, ConstructiveError},
    anf_local::{LocalAnfError, LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::LaneData,
    toy_tasks::SplitMix64,
};

pub const A6C3_GATE_COUNT: usize = 3;
pub const A6C3_MAX_TRAIN_STEPS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultiHopError {
    Network(LocalAnfError),
    Constructive(ConstructiveError),
    Shape,
    Spec,
    ScoreMismatch,
}
impl fmt::Display for MultiHopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MultiHopError {}
impl From<LocalAnfError> for MultiHopError {
    fn from(value: LocalAnfError) -> Self {
        Self::Network(value)
    }
}
impl From<ConstructiveError> for MultiHopError {
    fn from(value: ConstructiveError) -> Self {
        Self::Constructive(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiHopSpec {
    /// Exactly four distinct non-baseline raw channels; identities unknown.
    pub raw_candidates: Vec<usize>,
    pub baseline_input: usize,
    pub target_bit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiHopReport {
    pub proposed: LocalAnfNetwork,
    pub committed: LocalAnfNetwork,
    pub coordinate: LocalAnfNetwork,
    pub restarted: LocalAnfNetwork,
    pub joint_accepted: bool,
    pub coordinate_accepted: bool,
    pub restarted_accepted: bool,
    pub baseline_train_errors: u64,
    pub baseline_validation_errors: u64,
    pub joint_train_errors: u64,
    pub joint_validation_errors: u64,
    pub coordinate_train_errors: u64,
    pub coordinate_validation_errors: u64,
    pub restarted_train_errors: u64,
    pub restarted_validation_errors: u64,
    pub train_samples: u64,
    pub validation_samples: u64,
    pub joint_evaluations: usize,
    pub coordinate_evaluations: usize,
    pub restart_evaluations: usize,
    pub restart_attempts: usize,
    /// Exact stored u64 payload in train-only stage-1 cache; excludes metadata.
    pub cached_word_bytes: usize,
}

#[derive(Debug, Clone)]
struct GateRule {
    inputs: [usize; 2],
    truth: u8,
}
#[derive(Debug, Clone, Copy)]
struct Triple {
    first: usize,
    second: usize,
    third: usize,
}
type Series = Vec<Vec<u64>>;
type TrainCache = Vec<Vec<Series>>;

fn check_spec(network: &LocalAnfNetwork, spec: &MultiHopSpec) -> Result<(), MultiHopError> {
    if network.state_bits() != A6C3_GATE_COUNT
        || network.input_bits() != 5
        || spec.raw_candidates.len() != 4
        || spec.baseline_input >= network.input_bits()
        || spec.raw_candidates.contains(&spec.baseline_input)
    {
        return Err(MultiHopError::Spec);
    }
    for (j, &source) in spec.raw_candidates.iter().enumerate() {
        if source >= network.input_bits() || spec.raw_candidates[..j].contains(&source) {
            return Err(MultiHopError::Spec);
        }
    }
    if network.gates().iter().any(|g| g.truth_table().iter().any(|&v| v)) {
        return Err(MultiHopError::Spec);
    }
    Ok(())
}

fn check_data(
    network: &LocalAnfNetwork,
    data: &LaneData,
    spec: &MultiHopSpec,
    capped: bool,
) -> Result<u64, MultiHopError> {
    if data.input_bits != network.input_bits()
        || spec.target_bit >= data.target_bits
        || data.blocks.is_empty()
    {
        return Err(MultiHopError::Shape);
    }
    let mut steps = 0usize;
    let mut samples = 0u64;
    for block in &data.blocks {
        if block.steps == 0
            || block.valid == 0
            || block.inputs.len() != block.steps * data.input_bits
            || block.targets.len() != block.steps * data.target_bits
        {
            return Err(MultiHopError::Shape);
        }
        steps = steps.checked_add(block.steps).ok_or(MultiHopError::Shape)?;
        samples += u64::from(block.valid.count_ones()) * block.steps as u64;
    }
    if capped && steps > A6C3_MAX_TRAIN_STEPS {
        return Err(MultiHopError::Shape);
    }
    Ok(samples)
}

/// Address 0=(0,0), 1=(1,0), 2=(0,1), 3=(1,1).
/// Exact lane-parallel oracle for LocalAnfGate::from_truth_table.
#[inline]
fn apply_truth(truth: u8, a: u64, b: u64) -> u64 {
    let mut out = 0u64;
    if truth & 1 != 0 { out |= !a & !b; }
    if truth & 2 != 0 { out |= a & !b; }
    if truth & 4 != 0 { out |= !a & b; }
    if truth & 8 != 0 { out |= a & b; }
    out
}

fn grammar(
    network: &LocalAnfNetwork,
    spec: &MultiHopSpec,
) -> (Vec<GateRule>, Vec<GateRule>, Vec<GateRule>) {
    let n = network.state_bits();
    let mut first = Vec::new();
    let mut second = Vec::new();
    let mut third = Vec::new();
    for (i, &a) in spec.raw_candidates.iter().enumerate() {
        for &b in &spec.raw_candidates[i + 1..] {
            for truth in 0..16u8 {
                first.push(GateRule {
                    inputs: [n + a, n + b],
                    truth,
                });
            }
        }
    }
    for &raw in &spec.raw_candidates {
        for truth in 0..16u8 {
            second.push(GateRule {
                inputs: [0, n + raw],
                truth,
            });
            third.push(GateRule {
                inputs: [1, n + raw],
                truth,
            });
        }
    }
    (first, second, third)
}

fn first_outputs(rule: &GateRule, data: &LaneData, n: usize) -> Series {
    let a = rule.inputs[0] - n;
    let b = rule.inputs[1] - n;
    data.blocks.iter().map(|block| {
        (0..block.steps).map(|t| {
            apply_truth(
                rule.truth,
                block.inputs[t * data.input_bits + a],
                block.inputs[t * data.input_bits + b],
            )
        }).collect()
    }).collect()
}
fn second_outputs(rule: &GateRule, upstream: &Series, data: &LaneData, n: usize) -> Series {
    let raw = rule.inputs[1] - n;
    data.blocks.iter().enumerate().map(|(bi, block)| {
        (0..block.steps).map(|t| {
            let prior = if t == 0 { 0 } else { upstream[bi][t - 1] };
            apply_truth(rule.truth, prior, block.inputs[t * data.input_bits + raw])
        }).collect()
    }).collect()
}
fn terminal_errors(rule: &GateRule, prior: &Series, data: &LaneData, spec: &MultiHopSpec, n: usize) -> u64 {
    let raw = rule.inputs[1] - n;
    let mut errors = 0u64;
    for (bi, block) in data.blocks.iter().enumerate() {
        for t in 0..block.steps {
            let state = if t == 0 { 0 } else { prior[bi][t - 1] };
            let estimate = apply_truth(rule.truth, state, block.inputs[t * data.input_bits + raw]);
            let residual = block.targets[t * data.target_bits + spec.target_bit]
                ^ block.inputs[t * data.input_bits + spec.baseline_input];
            errors += u64::from(((estimate ^ residual) & block.valid).count_ones());
        }
    }
    errors
}

fn score(
    selected: Triple, cache: &TrainCache, third: &[GateRule],
    data: &LaneData, spec: &MultiHopSpec, n: usize,
) -> u64 {
    terminal_errors(&third[selected.third], &cache[selected.first][selected.second], data, spec, n)
}

fn build_cache(
    network: &LocalAnfNetwork, data: &LaneData,
    first: &[GateRule], second: &[GateRule],
) -> (TrainCache, usize) {
    let n = network.state_bits();
    let mut all = Vec::with_capacity(first.len());
    let mut words = 0usize;
    for upstream in first {
        let s0 = first_outputs(upstream, data, n);
        let mut next = Vec::with_capacity(second.len());
        for middle in second {
            let s1 = second_outputs(middle, &s0, data, n);
            words += s1.iter().map(Vec::len).sum::<usize>();
            next.push(s1);
        }
        all.push(next);
    }
    (all, words * std::mem::size_of::<u64>())
}

fn as_anf(rule: &GateRule, width: usize) -> Result<LocalAnfGate, MultiHopError> {
    let table: Vec<bool> = (0..4).map(|i| (rule.truth >> i) & 1 != 0).collect();
    Ok(LocalAnfGate::from_truth_table(rule.inputs.to_vec(), &table, width)?)
}
fn build_model(
    reference: &LocalAnfNetwork, selected: Triple,
    first: &[GateRule], second: &[GateRule], third: &[GateRule],
) -> Result<LocalAnfNetwork, MultiHopError> {
    let mut model = reference.clone();
    model.gates_mut()[0] = as_anf(&first[selected.first], reference.source_width())?;
    model.gates_mut()[1] = as_anf(&second[selected.second], reference.source_width())?;
    model.gates_mut()[2] = as_anf(&third[selected.third], reference.source_width())?;
    Ok(model)
}
fn actual_errors(
    network: &LocalAnfNetwork, data: &LaneData, spec: &MultiHopSpec,
) -> Result<u64, MultiHopError> {
    Ok(residual_output_errors(
        network, data, 2, spec.target_bit, spec.baseline_input,
    )?.0)
}

/// One-gate-at-a-time strictly improving moves; no neutral temporary credit.
/// Restarted control uses different initial triples but exactly this method.
fn descent(
    start: Triple, budget: usize, evaluations: &mut usize,
    cache: &TrainCache, first: &[GateRule], second: &[GateRule], third: &[GateRule],
    data: &LaneData, spec: &MultiHopSpec, n: usize,
) -> (Triple, u64) {
    debug_assert!(*evaluations < budget);
    let mut selected = start;
    let mut current = score(selected, cache, third, data, spec, n);
    *evaluations += 1;
    for _ in 0..3 {
        let mut improved = false;
        for axis in 0..3 {
            let choices = match axis {
                0 => first.len(),
                1 => second.len(),
                _ => third.len(),
            };
            let mut best = selected;
            let mut best_loss = current;
            for position in 0..choices {
                if *evaluations >= budget {
                    return (selected, current);
                }
                let mut candidate = selected;
                match axis {
                    0 => candidate.first = position,
                    1 => candidate.second = position,
                    _ => candidate.third = position,
                }
                let trial = score(candidate, cache, third, data, spec, n);
                *evaluations += 1;
                if trial < best_loss {
                    best_loss = trial;
                    best = candidate;
                }
            }
            if best_loss < current {
                selected = best;
                current = best_loss;
                improved = true;
            }
        }
        if !improved || current == 0 {
            break;
        }
    }
    (selected, current)
}

fn validate_commit(
    reference: &LocalAnfNetwork, candidate: LocalAnfNetwork,
    validation: &LaneData, spec: &MultiHopSpec, baseline: u64,
) -> Result<(LocalAnfNetwork, bool, u64), MultiHopError> {
    let loss = actual_errors(&candidate, validation, spec)?;
    let accepted = loss < baseline;
    Ok((if accepted { candidate } else { reference.clone() }, accepted, loss))
}

/// Exhaustive three-gate fit versus two independent end-task-only controls.
/// Candidates and restart choices use TRAIN data exclusively; VALIDATION only
/// decides whether to commit each complete network. No test split parameter.
pub fn fit_three_gate_credit(
    network: &LocalAnfNetwork,
    train: &LaneData,
    validation: &LaneData,
    spec: &MultiHopSpec,
    restart_seed: u64,
) -> Result<MultiHopReport, MultiHopError> {
    check_spec(network, spec)?;
    let train_samples = check_data(network, train, spec, true)?;
    let validation_samples = check_data(network, validation, spec, false)?;
    let (first, second, third) = grammar(network, spec);
    let (cache, cached_word_bytes) = build_cache(network, train, &first, &second);
    let n = network.state_bits();
    let baseline_train_errors = actual_errors(network, train, spec)?;
    let baseline_validation_errors = actual_errors(network, validation, spec)?;
    let initial = Triple { first: 0, second: 0, third: 0 };
    let mut best = initial;
    let mut joint_train_errors = u64::MAX;
    let mut joint_evaluations = 0usize;
    for a in 0..first.len() {
        for b in 0..second.len() {
            for c in 0..third.len() {
                let triple = Triple { first: a, second: b, third: c };
                let loss = score(triple, &cache, &third, train, spec, n);
                joint_evaluations += 1;
                if loss < joint_train_errors {
                    best = triple;
                    joint_train_errors = loss;
                }
            }
        }
    }
    let proposed = build_model(network, best, &first, &second, &third)?;
    if actual_errors(&proposed, train, spec)? != joint_train_errors {
        return Err(MultiHopError::ScoreMismatch);
    }
    let (committed, joint_accepted, joint_validation_errors) =
        validate_commit(network, proposed.clone(), validation, spec, baseline_validation_errors)?;

    let mut coordinate_evaluations = 0usize;
    let (coordinate_choice, coordinate_train_errors) = descent(
        initial, joint_evaluations, &mut coordinate_evaluations,
        &cache, &first, &second, &third, train, spec, n,
    );
    let coordinate_candidate =
        build_model(network, coordinate_choice, &first, &second, &third)?;
    if actual_errors(&coordinate_candidate, train, spec)? != coordinate_train_errors {
        return Err(MultiHopError::ScoreMismatch);
    }
    let (coordinate, coordinate_accepted, coordinate_validation_errors) =
        validate_commit(network, coordinate_candidate, validation, spec, baseline_validation_errors)?;

    // Same objective-evaluation CAP as exhaustive joint search. Random
    // restarts can stop sooner if they discover a zero-training-error chain.
    let mut rng = SplitMix64::new(restart_seed ^ 0xa6c3_a6c3_1eed_9001);
    let mut restart_evaluations = 0usize;
    let mut restart_attempts = 0usize;
    let mut restarted_choice = initial;
    let mut restarted_train_errors = baseline_train_errors;
    while restart_evaluations < joint_evaluations && restarted_train_errors > 0 {
        restart_attempts += 1;
        let start = Triple {
            first: rng.below(first.len()),
            second: rng.below(second.len()),
            third: rng.below(third.len()),
        };
        let (choice, loss) = descent(
            start, joint_evaluations, &mut restart_evaluations,
            &cache, &first, &second, &third, train, spec, n,
        );
        if loss < restarted_train_errors {
            restarted_choice = choice;
            restarted_train_errors = loss;
        }
    }
    let restarted_candidate =
        build_model(network, restarted_choice, &first, &second, &third)?;
    if actual_errors(&restarted_candidate, train, spec)? != restarted_train_errors {
        return Err(MultiHopError::ScoreMismatch);
    }
    let (restarted, restarted_accepted, restarted_validation_errors) =
        validate_commit(network, restarted_candidate, validation, spec, baseline_validation_errors)?;

    Ok(MultiHopReport {
        proposed,
        committed,
        coordinate,
        restarted,
        joint_accepted,
        coordinate_accepted,
        restarted_accepted,
        baseline_train_errors,
        baseline_validation_errors,
        joint_train_errors,
        joint_validation_errors,
        coordinate_train_errors,
        coordinate_validation_errors,
        restarted_train_errors,
        restarted_validation_errors,
        train_samples,
        validation_samples,
        joint_evaluations,
        coordinate_evaluations,
        restart_evaluations,
        restart_attempts,
        cached_word_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{anf_local_incremental::LaneBlock, toy_tasks::SplitMix64};

    fn baseline() -> LocalAnfNetwork {
        let n = 3;
        let width = n + 5;
        let gates = vec![
            LocalAnfGate::zero(vec![n + 1, n + 2], width).unwrap(),
            LocalAnfGate::zero(vec![0, n + 1], width).unwrap(),
            LocalAnfGate::zero(vec![1, n + 1], width).unwrap(),
        ];
        let readout = LocalAnfGate::zero(vec![n], width).unwrap();
        LocalAnfNetwork::with_input_bits(n, 5, gates, readout).unwrap()
    }
    fn spec() -> MultiHopSpec {
        MultiHopSpec { raw_candidates: vec![1, 2, 3, 4], baseline_input: 0, target_bit: 0 }
    }
    fn fixture(seed: u64, steps: usize) -> LaneData {
        let mut rng = SplitMix64::new(seed);
        let mut inputs = vec![0u64; 5 * steps];
        for word in &mut inputs {
            *word = rng.next_u64();
        }
        let mut targets = vec![0u64; steps];
        for t in 0..steps {
            let parity = if t >= 2 {
                inputs[5 * (t - 2) + 1] ^ inputs[5 * (t - 2) + 2]
            } else { 0 };
            let middle = if t >= 1 {
                inputs[5 * (t - 1) + 3] & parity
            } else { 0 };
            targets[t] = inputs[5 * t] ^ inputs[5 * t + 4] ^ middle;
        }
        LaneData {
            input_bits: 5,
            target_bits: 1,
            blocks: vec![LaneBlock { steps, inputs, targets, valid: u64::MAX }],
        }
    }

    #[test]
    fn bit_sliced_truth_masks_match_existing_anf_semantics() {
        for mask in 0..16u8 {
            let rule = GateRule { inputs: [3, 4], truth: mask };
            let gate = as_anf(&rule, 8).unwrap();
            let mut products = [0u64; 4];
            for a in [0, u64::MAX] {
                for b in [0, u64::MAX] {
                    assert_eq!(
                        apply_truth(mask, a, b),
                        gate.evaluate_gathered(&[a, b], &mut products),
                    );
                }
            }
        }
    }

    #[test]
    fn learns_hidden_three_gate_two_hop_chain_from_terminal_labels() {
        let network = baseline();
        let train = fixture(100, 48);
        let val = fixture(200, 24);
        let report = fit_three_gate_credit(&network, &train, &val, &spec(), 123).unwrap();
        assert_eq!(report.joint_evaluations, 6 * 16 * 4 * 16 * 4 * 16);
        assert_eq!(report.joint_train_errors, 0);
        assert_eq!(report.joint_validation_errors, 0);
        assert!(report.joint_accepted);
        assert_eq!(report.proposed.gates()[0].inputs(), &[4, 5]);
        assert_eq!(report.proposed.gates()[1].inputs(), &[0, 6]);
        assert_eq!(report.proposed.gates()[2].inputs(), &[1, 7]);
        let test = fixture(300, 24);
        assert_eq!(actual_errors(&report.committed, &test, &spec()).unwrap(), 0);
        assert!(report.restart_evaluations <= report.joint_evaluations);
        let mut damaged = report.committed.clone();
        damaged.gates_mut()[0] = network.gates()[0].clone();
        assert!(actual_errors(&damaged, &test, &spec()).unwrap() > 0);
    }

    #[test]
    fn no_intermediate_or_validation_target_leakage() {
        let network = baseline();
        let train = fixture(333, 32);
        let val = fixture(444, 20);
        let a = fit_three_gate_credit(&network, &train, &val, &spec(), 7).unwrap();
        let mut inverted = val.clone();
        for block in &mut inverted.blocks {
            for word in &mut block.targets { *word = !*word; }
        }
        let b = fit_three_gate_credit(&network, &train, &inverted, &spec(), 7).unwrap();
        assert_eq!(a.proposed, b.proposed);
        assert_eq!(a.joint_train_errors, b.joint_train_errors);
        assert_eq!(a.coordinate_train_errors, b.coordinate_train_errors);
        assert_eq!(a.restarted_train_errors, b.restarted_train_errors);
        assert_eq!(network, baseline());
        assert!(!b.joint_accepted);
    }

    #[test]
    fn invalid_candidate_list_or_data_shape_is_rejected() {
        let network = baseline();
        let train = fixture(99, 16);
        let bad_spec = MultiHopSpec {
            raw_candidates: vec![1, 2, 2, 4], ..spec()
        };
        assert_eq!(
            fit_three_gate_credit(&network, &train, &train, &bad_spec, 1),
            Err(MultiHopError::Spec)
        );
        let mut bad_data = train.clone();
        bad_data.blocks[0].targets.clear();
        assert_eq!(
            fit_three_gate_credit(&network, &bad_data, &train, &spec(), 1),
            Err(MultiHopError::Shape)
        );
    }
}
