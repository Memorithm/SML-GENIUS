//! A6-C5: terminal-loss learning of Boolean recurrent state transitions with
//! self-feedback or mutual-feedback, not merely a preselected acyclic DAG.
//!
//! Every candidate has TWO synchronous two-input state gates:
//!   s0(t+1) = f(s0(t),s1(t),x(t))  (exactly two chosen sources)
//!   s1(t+1) = g(s0(t),s1(t),x(t))  (exactly two chosen sources)
//! and an immutable output y_hat(t) = x_baseline(t) XOR s1(t+1).
//! Both candidate functions are native GF(2) ANF truth tables. No hidden
//! state targets or future targets are provided to the search.
//!
//! Enumerates a bounded joint 240 x 240 table/source grammar (57,600 pairs)
//! using train-only bit-sliced u64 objective scores, verified on the selected
//! native LocalAnfNetwork before validation commit. Controls are a trained
//! direct/no-state model, a trained feed-forward DAG (no recurrent feedback
//! loops), strict single-gate coordinate descent from the zero initialization,
//! and randomized coordinate restarts with the SAME 57,600-score budget cap.
//!
//! The grammar has at most two state bits and fan-in two. It cannot represent
//! general recurrent programs, nor all delay/periodic-memory functions.

use std::fmt;

use crate::{
    anf_constructive::{residual_output_errors, ConstructiveError},
    anf_local::{LocalAnfError, LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::LaneData,
    toy_tasks::SplitMix64,
};

pub const A6C5_STATE_BITS: usize = 2;
pub const A6C5_INPUT_BITS: usize = 5;
pub const A6C5_FAN_IN: usize = 2;
pub const A6C5_RULES: usize = 240;
pub const A6C5_PAIRS: usize = A6C5_RULES * A6C5_RULES;
const MAX_TRAIN_STEPS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackClass {
    Direct,
    Acyclic,
    SelfFeedback,
    MutualFeedback,
}
impl FeedbackClass {
    pub fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Acyclic => "acyclic_delay",
            Self::SelfFeedback => "self_feedback",
            Self::MutualFeedback => "mutual_feedback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycleError {
    Network(LocalAnfError),
    Constructive(ConstructiveError),
    Shape,
    Spec,
    NativeOracle,
}
impl fmt::Display for CycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CycleError {}
impl From<LocalAnfError> for CycleError {
    fn from(value: LocalAnfError) -> Self {
        Self::Network(value)
    }
}
impl From<ConstructiveError> for CycleError {
    fn from(value: ConstructiveError) -> Self {
        Self::Constructive(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleSpec {
    pub raw_candidates: Vec<usize>,
    pub baseline_input: usize,
    pub target_bit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleReport {
    pub proposed: LocalAnfNetwork,
    pub committed: LocalAnfNetwork,
    pub direct: LocalAnfNetwork,
    pub acyclic: LocalAnfNetwork,
    pub coordinate: LocalAnfNetwork,
    pub restarted: LocalAnfNetwork,
    pub proposed_class: FeedbackClass,
    pub selected_class: FeedbackClass,
    pub active_gate_count: usize,
    pub active_feedback_edges: usize,
    pub monomials: usize,
    pub accepted_joint: bool,
    pub accepted_direct: bool,
    pub accepted_acyclic: bool,
    pub accepted_coordinate: bool,
    pub accepted_restarted: bool,
    pub baseline_train_errors: u64,
    pub baseline_validation_errors: u64,
    pub joint_train_errors: u64,
    pub joint_validation_errors: u64,
    pub direct_train_errors: u64,
    pub direct_validation_errors: u64,
    pub acyclic_train_errors: u64,
    pub acyclic_validation_errors: u64,
    pub coordinate_train_errors: u64,
    pub coordinate_validation_errors: u64,
    pub restarted_train_errors: u64,
    pub restarted_validation_errors: u64,
    pub train_samples: u64,
    pub validation_samples: u64,
    pub joint_evaluations: usize,
    pub direct_evaluations: usize,
    pub acyclic_evaluations: usize,
    pub coordinate_evaluations: usize,
    pub restarted_evaluations: usize,
    pub restart_attempts: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rule {
    inputs: [usize; 2],
    truth: u8,
}
#[derive(Debug, Clone, Copy)]
struct Best {
    first: usize,
    second: usize,
    loss: u64,
    complexity: [usize; 3],
}

fn validate(
    network: &LocalAnfNetwork,
    data: &LaneData,
    spec: &CycleSpec,
    capped: bool,
) -> Result<u64, CycleError> {
    if network.state_bits() != A6C5_STATE_BITS
        || network.input_bits() != A6C5_INPUT_BITS
        || spec.raw_candidates.len() != A6C5_INPUT_BITS - 1
        || spec.baseline_input >= A6C5_INPUT_BITS
        || spec.raw_candidates.contains(&spec.baseline_input)
        || spec.target_bit >= data.target_bits
        || data.input_bits != A6C5_INPUT_BITS
        || data.blocks.is_empty()
        || network
            .gates()
            .iter()
            .any(|gate| gate.truth_table().iter().any(|&v| v))
    {
        return Err(CycleError::Spec);
    }
    for (i, &raw) in spec.raw_candidates.iter().enumerate() {
        if raw >= A6C5_INPUT_BITS || spec.raw_candidates[..i].contains(&raw) {
            return Err(CycleError::Spec);
        }
    }
    let mut steps = 0usize;
    let mut samples = 0u64;
    for block in &data.blocks {
        if block.steps == 0
            || block.valid == 0
            || block.inputs.len() != block.steps * data.input_bits
            || block.targets.len() != block.steps * data.target_bits
        {
            return Err(CycleError::Shape);
        }
        steps = steps.checked_add(block.steps).ok_or(CycleError::Shape)?;
        samples += u64::from(block.valid.count_ones()) * block.steps as u64;
    }
    if capped && steps > MAX_TRAIN_STEPS {
        return Err(CycleError::Shape);
    }
    Ok(samples)
}

fn rules(spec: &CycleSpec) -> Vec<Rule> {
    let sources: Vec<usize> = [0usize, 1]
        .into_iter()
        .chain(spec.raw_candidates.iter().map(|&raw| A6C5_STATE_BITS + raw))
        .collect();
    let mut all = Vec::with_capacity(A6C5_RULES);
    for (i, &a) in sources.iter().enumerate() {
        for &b in &sources[i + 1..] {
            for truth in 0..16u8 {
                all.push(Rule {
                    inputs: [a, b],
                    truth,
                });
            }
        }
    }
    all
}

#[inline]
fn apply_truth(truth: u8, a: u64, b: u64) -> u64 {
    let mut out = 0u64;
    if truth & 1 != 0 {
        out |= !a & !b;
    }
    if truth & 2 != 0 {
        out |= a & !b;
    }
    if truth & 4 != 0 {
        out |= !a & b;
    }
    if truth & 8 != 0 {
        out |= a & b;
    }
    out
}
#[inline]
fn source_word(source: usize, states: [u64; 2], inputs: &[u64], t: usize) -> u64 {
    match source {
        0 => states[0],
        1 => states[1],
        _ => inputs[t * A6C5_INPUT_BITS + source - A6C5_STATE_BITS],
    }
}
#[inline]
fn step(rule: &Rule, prior: [u64; 2], inputs: &[u64], t: usize) -> u64 {
    let a = source_word(rule.inputs[0], prior, inputs, t);
    let b = source_word(rule.inputs[1], prior, inputs, t);
    apply_truth(rule.truth, a, b)
}

/// Score exactly one candidate on training-only terminal y labels.
/// State is explicitly reset at the start of every independent block.
fn train_loss(first: &Rule, second: &Rule, data: &LaneData, spec: &CycleSpec) -> u64 {
    let mut errors = 0u64;
    for block in &data.blocks {
        let mut state = [0u64; 2];
        for t in 0..block.steps {
            let next = [
                step(first, state, &block.inputs, t),
                step(second, state, &block.inputs, t),
            ];
            let actual_residual = block.targets[t * data.target_bits + spec.target_bit]
                ^ block.inputs[t * data.input_bits + spec.baseline_input];
            errors += u64::from(((next[1] ^ actual_residual) & block.valid).count_ones());
            state = next;
        }
    }
    errors
}

fn input_matters(truth: u8, pos: usize) -> bool {
    (0..4).any(|address| {
        address & (1 << pos) == 0
            && ((truth >> address) & 1) != ((truth >> (address | (1 << pos))) & 1)
    })
}
fn depends_on(rule: &Rule, source: usize) -> bool {
    rule.inputs
        .iter()
        .enumerate()
        .any(|(j, &input)| input == source && input_matters(rule.truth, j))
}
fn monomials(truth: u8) -> usize {
    let mut coefficients: [bool; 4] = std::array::from_fn(|i| truth & (1 << i) != 0);
    for step in [1, 2] {
        for i in 0..4 {
            if i & step != 0 {
                coefficients[i] ^= coefficients[i ^ step];
            }
        }
    }
    coefficients.iter().filter(|&&v| v).count()
}
fn classify(first: &Rule, second: &Rule) -> FeedbackClass {
    let depends_first = depends_on(second, 0);
    if depends_first && depends_on(first, 1) {
        FeedbackClass::MutualFeedback
    } else if depends_on(second, 1) || (depends_first && depends_on(first, 0)) {
        FeedbackClass::SelfFeedback
    } else if depends_first {
        FeedbackClass::Acyclic
    } else {
        FeedbackClass::Direct
    }
}
fn complexity(first: &Rule, second: &Rule) -> [usize; 3] {
    let first_live = depends_on(second, 0);
    let active = usize::from(first_live) + 1;
    let feedback = usize::from(depends_on(second, 1))
        + usize::from(first_live && depends_on(first, 0))
        + usize::from(first_live && depends_on(first, 1));
    let terms = monomials(second.truth)
        + if first_live {
            monomials(first.truth)
        } else {
            0
        };
    [active, feedback, terms]
}

fn better(candidate: Best, selected: Option<Best>) -> bool {
    selected.is_none_or(|old| (candidate.loss, candidate.complexity) < (old.loss, old.complexity))
}
fn materialize(
    baseline: &LocalAnfNetwork,
    first: &Rule,
    second: &Rule,
) -> Result<LocalAnfNetwork, CycleError> {
    let mut result = baseline.clone();
    for (i, rule) in [first, second].iter().enumerate() {
        let table: Vec<bool> = (0..4).map(|j| rule.truth & (1 << j) != 0).collect();
        result.gates_mut()[i] =
            LocalAnfGate::from_truth_table(rule.inputs.to_vec(), &table, baseline.source_width())?;
    }
    Ok(result)
}
fn native_loss(
    network: &LocalAnfNetwork,
    data: &LaneData,
    spec: &CycleSpec,
) -> Result<u64, CycleError> {
    Ok(residual_output_errors(network, data, 1, spec.target_bit, spec.baseline_input)?.0)
}
fn validate_commit(
    initial: &LocalAnfNetwork,
    candidate: LocalAnfNetwork,
    validation: &LaneData,
    spec: &CycleSpec,
    baseline_errors: u64,
) -> Result<(LocalAnfNetwork, u64, bool), CycleError> {
    let loss = native_loss(&candidate, validation, spec)?;
    let accepted = loss < baseline_errors;
    Ok((
        if accepted { candidate } else { initial.clone() },
        loss,
        accepted,
    ))
}

struct Search<'a> {
    rules: &'a [Rule],
    training: &'a LaneData,
    spec: &'a CycleSpec,
}
impl Search<'_> {
    fn score(&self, a: usize, b: usize) -> u64 {
        train_loss(&self.rules[a], &self.rules[b], self.training, self.spec)
    }
}

/// Coordinate descent with strict improvement (no neutral credit).
/// Budget counts EVERY evaluated terminal loss, including starting points.
/// Individual coordinate moves have exactly the same 240-rule grammar.
fn descent(
    start: (usize, usize),
    search: &Search<'_>,
    budget: usize,
    evaluations: &mut usize,
) -> Best {
    debug_assert!(*evaluations < budget);
    let mut selected = start;
    let mut loss = search.score(selected.0, selected.1);
    *evaluations += 1;
    for _ in 0..4 {
        let mut improved = false;
        for axis in 0..2 {
            let mut best = selected;
            let mut best_loss = loss;
            for index in 0..search.rules.len() {
                if *evaluations >= budget {
                    let c = complexity(&search.rules[selected.0], &search.rules[selected.1]);
                    return Best {
                        first: selected.0,
                        second: selected.1,
                        loss,
                        complexity: c,
                    };
                }
                let candidate = if axis == 0 {
                    (index, selected.1)
                } else {
                    (selected.0, index)
                };
                let value = search.score(candidate.0, candidate.1);
                *evaluations += 1;
                if value < best_loss {
                    best_loss = value;
                    best = candidate;
                }
            }
            if best_loss < loss {
                loss = best_loss;
                selected = best;
                improved = true;
            }
        }
        if !improved || loss == 0 {
            break;
        }
    }
    Best {
        first: selected.0,
        second: selected.1,
        loss,
        complexity: complexity(&search.rules[selected.0], &search.rules[selected.1]),
    }
}

/// Fit models from training-only terminal labels, then use validation only to
/// commit/reject whole trained models. No test data enters this API.
pub fn fit_recurrent_cycles(
    initial: &LocalAnfNetwork,
    train: &LaneData,
    validation: &LaneData,
    spec: &CycleSpec,
    restart_seed: u64,
) -> Result<CycleReport, CycleError> {
    let train_samples = validate(initial, train, spec, true)?;
    let validation_samples = validate(initial, validation, spec, false)?;
    let bank = rules(spec);
    if bank.len() != A6C5_RULES {
        return Err(CycleError::Spec);
    }
    let mut best_joint: Option<Best> = None;
    let mut best_dag: Option<Best> = None;
    let mut best_direct: Option<Best> = None;
    let mut joint_evaluations = 0usize;
    let mut direct_evaluations = 0usize;
    let mut acyclic_evaluations = 0usize;

    // Full source/Boolean-table grammar includes every permitted self-loop,
    // both-edge mutual cycle, direct readout and acyclic delay.
    for (a, r0) in bank.iter().enumerate() {
        for (b, r1) in bank.iter().enumerate() {
            let loss = train_loss(r0, r1, train, spec);
            joint_evaluations += 1;
            let candidate = Best {
                first: a,
                second: b,
                loss,
                complexity: complexity(r0, r1),
            };
            if better(candidate, best_joint) {
                best_joint = Some(candidate);
            }
            // Acyclic control: gate0 raw-only, gate1 may use gate0 or raw,
            // but must not read itself. This eliminates ALL directed cycles.
            if r0.inputs.iter().all(|&s| s >= A6C5_STATE_BITS) && !r1.inputs.contains(&1) {
                acyclic_evaluations += 1;
                if better(candidate, best_dag) {
                    best_dag = Some(candidate);
                }
            }
            // The first gate is causally irrelevant in the raw-only control.
            // Count each distinct final gate once (fixed first-gate default).
            if a == 0 && r1.inputs.iter().all(|&s| s >= A6C5_STATE_BITS) {
                direct_evaluations += 1;
                if better(candidate, best_direct) {
                    best_direct = Some(candidate);
                }
            }
        }
    }
    if joint_evaluations != A6C5_PAIRS
        || direct_evaluations != 96
        || acyclic_evaluations != 96 * 160
    {
        return Err(CycleError::Spec);
    }
    let joint = best_joint.ok_or(CycleError::Spec)?;
    let direct = best_direct.ok_or(CycleError::Spec)?;
    let dag = best_dag.ok_or(CycleError::Spec)?;
    let proposed = materialize(initial, &bank[joint.first], &bank[joint.second])?;
    let direct_candidate = materialize(initial, &bank[direct.first], &bank[direct.second])?;
    let acyclic_candidate = materialize(initial, &bank[dag.first], &bank[dag.second])?;

    let baseline_train_errors = native_loss(initial, train, spec)?;
    let baseline_validation_errors = native_loss(initial, validation, spec)?;
    for (model, predicted) in [
        (&proposed, joint.loss),
        (&direct_candidate, direct.loss),
        (&acyclic_candidate, dag.loss),
    ] {
        if native_loss(model, train, spec)? != predicted {
            return Err(CycleError::NativeOracle);
        }
    }

    // Monotone coordinate control and randomized restarts share the identical
    // training data, objective, rules and maximum number of scored pairs.
    let search = Search {
        rules: &bank,
        training: train,
        spec,
    };
    let mut coordinate_evaluations = 0usize;
    let coordinate_best = descent((0, 0), &search, A6C5_PAIRS, &mut coordinate_evaluations);
    let mut restart_evaluations = 0usize;
    let mut restart_attempts = 0usize;
    let mut rng = SplitMix64::new(restart_seed ^ 0xa6c5_ca11_8ac0_d15c);
    let mut restart_best = Best {
        first: 0,
        second: 0,
        loss: baseline_train_errors,
        complexity: complexity(&bank[0], &bank[0]),
    };
    while restart_evaluations < A6C5_PAIRS && restart_best.loss != 0 {
        let initial_pair = (rng.below(bank.len()), rng.below(bank.len()));
        let candidate = descent(initial_pair, &search, A6C5_PAIRS, &mut restart_evaluations);
        restart_attempts += 1;
        if better(candidate, Some(restart_best)) {
            restart_best = candidate;
        }
    }
    let coordinate_candidate = materialize(
        initial,
        &bank[coordinate_best.first],
        &bank[coordinate_best.second],
    )?;
    let restarted_candidate = materialize(
        initial,
        &bank[restart_best.first],
        &bank[restart_best.second],
    )?;
    for (candidate, expected) in [
        (&coordinate_candidate, coordinate_best.loss),
        (&restarted_candidate, restart_best.loss),
    ] {
        if native_loss(candidate, train, spec)? != expected {
            return Err(CycleError::NativeOracle);
        }
    }
    let (committed, joint_validation_errors, accepted_joint) = validate_commit(
        initial,
        proposed.clone(),
        validation,
        spec,
        baseline_validation_errors,
    )?;
    let (direct, direct_validation_errors, accepted_direct) = validate_commit(
        initial,
        direct_candidate,
        validation,
        spec,
        baseline_validation_errors,
    )?;
    let (acyclic, acyclic_validation_errors, accepted_acyclic) = validate_commit(
        initial,
        acyclic_candidate,
        validation,
        spec,
        baseline_validation_errors,
    )?;
    let (coordinate, coordinate_validation_errors, accepted_coordinate) = validate_commit(
        initial,
        coordinate_candidate,
        validation,
        spec,
        baseline_validation_errors,
    )?;
    let (restarted, restarted_validation_errors, accepted_restarted) = validate_commit(
        initial,
        restarted_candidate,
        validation,
        spec,
        baseline_validation_errors,
    )?;
    let rule0 = &bank[joint.first];
    let rule1 = &bank[joint.second];
    Ok(CycleReport {
        proposed,
        committed,
        direct,
        acyclic,
        coordinate,
        restarted,
        proposed_class: classify(rule0, rule1),
        selected_class: if accepted_joint {
            classify(rule0, rule1)
        } else {
            FeedbackClass::Direct
        },
        active_gate_count: complexity(rule0, rule1)[0],
        active_feedback_edges: complexity(rule0, rule1)[1],
        monomials: complexity(rule0, rule1)[2],
        accepted_joint,
        accepted_direct,
        accepted_acyclic,
        accepted_coordinate,
        accepted_restarted,
        baseline_train_errors,
        baseline_validation_errors,
        joint_train_errors: joint.loss,
        joint_validation_errors,
        direct_train_errors: best_direct.unwrap().loss,
        direct_validation_errors,
        acyclic_train_errors: dag.loss,
        acyclic_validation_errors,
        coordinate_train_errors: coordinate_best.loss,
        coordinate_validation_errors,
        restarted_train_errors: restart_best.loss,
        restarted_validation_errors,
        train_samples,
        validation_samples,
        joint_evaluations,
        direct_evaluations,
        acyclic_evaluations,
        coordinate_evaluations,
        restarted_evaluations: restart_evaluations,
        restart_attempts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{anf_local_incremental::LaneBlock, toy_tasks::SplitMix64};

    fn baseline() -> LocalAnfNetwork {
        let n = A6C5_STATE_BITS;
        let w = n + A6C5_INPUT_BITS;
        let gates = vec![
            LocalAnfGate::zero(vec![n + 1, n + 2], w).unwrap(),
            LocalAnfGate::zero(vec![n + 1, n + 2], w).unwrap(),
        ];
        let readout = LocalAnfGate::zero(vec![n], w).unwrap();
        LocalAnfNetwork::with_input_bits(n, A6C5_INPUT_BITS, gates, readout).unwrap()
    }
    fn spec() -> CycleSpec {
        CycleSpec {
            raw_candidates: vec![1, 2, 3, 4],
            baseline_input: 0,
            target_bit: 0,
        }
    }
    #[derive(Clone, Copy)]
    enum Task {
        Direct,
        Delay,
        Running,
        Interleaved,
        Impossible,
    }
    fn fixture(seed: u64, steps: usize, task: Task) -> LaneData {
        let mut rng = SplitMix64::new(seed);
        let mut inputs = vec![0u64; steps * A6C5_INPUT_BITS];
        for word in &mut inputs {
            *word = rng.next_u64();
        }
        let mut targets = vec![0u64; steps];
        let mut period1 = 0u64;
        let mut period2 = [0u64; 2];
        let mut period3 = [0u64; 3];
        for (t, target) in targets.iter_mut().enumerate() {
            let signal = inputs[t * A6C5_INPUT_BITS + 1];
            period1 ^= signal;
            period2[t % 2] ^= signal;
            period3[t % 3] ^= signal;
            let residual = match task {
                Task::Direct => signal & inputs[t * A6C5_INPUT_BITS + 2],
                Task::Delay => {
                    if t == 0 {
                        0
                    } else {
                        inputs[(t - 1) * A6C5_INPUT_BITS + 1]
                    }
                }
                Task::Running => period1,
                Task::Interleaved => period2[t % 2],
                Task::Impossible => period3[t % 3],
            };
            *target = inputs[t * A6C5_INPUT_BITS] ^ residual;
        }
        LaneData {
            input_bits: A6C5_INPUT_BITS,
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
    fn all_16_two_input_truth_tables_match_native_anf() {
        for truth in 0..16u8 {
            let table: Vec<bool> = (0..4).map(|i| truth & (1 << i) != 0).collect();
            let gate = LocalAnfGate::from_truth_table(vec![2, 3], &table, 7).unwrap();
            let mut scratch = [0u64; 4];
            for a in [0u64, u64::MAX] {
                for b in [0u64, u64::MAX] {
                    assert_eq!(
                        apply_truth(truth, a, b),
                        gate.evaluate_gathered(&[a, b], &mut scratch),
                    );
                }
            }
        }
    }

    #[test]
    fn terminal_supervision_finds_direct_delay_and_cyclic_memory() {
        let cases = [
            (Task::Direct, FeedbackClass::Direct),
            (Task::Delay, FeedbackClass::Acyclic),
            (Task::Running, FeedbackClass::SelfFeedback),
            (Task::Interleaved, FeedbackClass::MutualFeedback),
        ];
        for (task, required) in cases {
            let reference = baseline();
            let training = fixture(20, 32, task);
            let validation = fixture(21, 32, task);
            let report =
                fit_recurrent_cycles(&reference, &training, &validation, &spec(), 44).unwrap();
            assert_eq!(report.joint_train_errors, 0);
            assert_eq!(report.joint_validation_errors, 0);
            assert_eq!(report.proposed_class, required);
            let test = fixture(22, 300, task);
            assert_eq!(native_loss(&report.committed, &test, &spec()).unwrap(), 0);
            assert_eq!(reference, baseline());
            assert!(report.restarted_evaluations <= report.joint_evaluations);
        }
    }

    #[test]
    fn unrepresentable_period_three_is_a_falsification_control() {
        let reference = baseline();
        let data = fixture(40, 32, Task::Impossible);
        let report = fit_recurrent_cycles(
            &reference,
            &data,
            &fixture(41, 20, Task::Impossible),
            &spec(),
            123,
        )
        .unwrap();
        assert!(report.joint_train_errors > 0);
    }

    #[test]
    fn proposal_does_not_consult_validation_targets_and_rejects_malformed_inputs() {
        let reference = baseline();
        let train = fixture(90, 32, Task::Running);
        let val = fixture(91, 24, Task::Running);
        let a = fit_recurrent_cycles(&reference, &train, &val, &spec(), 17).unwrap();
        let mut flipped = val.clone();
        for block in &mut flipped.blocks {
            for word in &mut block.targets {
                *word = !*word;
            }
        }
        let b = fit_recurrent_cycles(&reference, &train, &flipped, &spec(), 17).unwrap();
        assert_eq!(a.proposed, b.proposed);
        assert_eq!(a.joint_train_errors, b.joint_train_errors);
        assert_eq!(a.restarted_train_errors, b.restarted_train_errors);
        assert!(!b.accepted_joint);
        assert_eq!(reference, baseline());
        let mut wrong = spec();
        wrong.raw_candidates = vec![1, 1, 3, 4];
        assert_eq!(
            fit_recurrent_cycles(&reference, &train, &val, &wrong, 1),
            Err(CycleError::Spec)
        );
    }
}
