//! A6-C5b: terminal-only training of parity-phase recurrent Boolean memory.
//!
//! Test whether increasing native ANF state width from 2 to 3 and 4 bits
//! resolves A6-C5's failed stride-3 parity negative control. Search remains
//! explicitly bounded to a single recurrent ring with trainable truth tables.
//!
//! For width N in 1..=4:
//!   s0(t+1) = f(s_feedback(t), x_raw(t))
//!   s_i(t+1) = u_i(s_(i-1)(t)), for each i in 1..N-1;
//!   predicted(t) = x_baseline(t) XOR s0(t+1).
//!
//! The feedback tap, raw input, complete 4-entry f truth table, and every
//! 2-entry unary delay-gate truth table are trained against TERMINAL OUTPUT
//! LABELS ONLY. There is no hidden-state supervision, no future input and no
//! access to test data in the fitting API. Native GF(2) ANF representations
//! are independently simulated as an oracle for the chosen circuits.
//!
//! Fixed ring scaffold means this is NOT unconstrained topology learning.
//! Validation selects the smallest state width achieving minimum val errors.

use std::fmt;
use crate::{
    anf_local::{LocalAnfError, LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::{LaneData, Trajectory},
};

pub const A6C5B_MAX_STATE_BITS: usize = 4;
pub const A6C5B_INPUT_BITS: usize = 5;
pub const A6C5B_RAW_CHOICES: usize = 4;
pub const A6C5B_MAX_TRAIN_STEPS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhaseError {
    Network(LocalAnfError),
    InvalidSpec,
    InvalidData,
    ScoreMismatch,
}
impl fmt::Display for PhaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PhaseError {}
impl From<LocalAnfError> for PhaseError {
    fn from(value: LocalAnfError) -> Self {
        Self::Network(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseSpec {
    /// Distinct raw input channels excluding baseline.
    pub raw_candidates: Vec<usize>,
    pub baseline_input: usize,
    pub target_bit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseRule {
    pub width: usize,
    pub feedback: usize,
    pub raw: usize,
    pub truth: u8,
    pub delays: [u8; A6C5B_MAX_STATE_BITS - 1],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidthFit {
    pub rule: PhaseRule,
    pub network: LocalAnfNetwork,
    pub train_errors: u64,
    pub validation_errors: u64,
    pub evaluations: usize,
    pub anf_nonzero_monomials: usize,
    pub effective_period: usize,
    pub accepted_against_zero: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseReport {
    pub widths: Vec<WidthFit>,
    pub selected: LocalAnfNetwork,
    pub selected_rule: PhaseRule,
    pub selected_width: usize,
    pub selected_effective_period: usize,
    pub accepted: bool,
    pub baseline_train_errors: u64,
    pub baseline_validation_errors: u64,
    pub selected_train_errors: u64,
    pub selected_validation_errors: u64,
    pub train_samples: u64,
    pub validation_samples: u64,
    pub objective_evaluations: usize,
}

fn validate(
    data: &LaneData, spec: &PhaseSpec, cap_train: bool,
) -> Result<u64, PhaseError> {
    if data.input_bits != A6C5B_INPUT_BITS
        || spec.target_bit >= data.target_bits
        || spec.baseline_input >= data.input_bits
        || spec.raw_candidates.len() != A6C5B_RAW_CHOICES
        || spec.raw_candidates.contains(&spec.baseline_input)
        || data.blocks.is_empty()
    {
        return Err(PhaseError::InvalidSpec);
    }
    for (i, &raw) in spec.raw_candidates.iter().enumerate() {
        if raw >= data.input_bits || spec.raw_candidates[..i].contains(&raw) {
            return Err(PhaseError::InvalidSpec);
        }
    }
    let mut samples = 0u64;
    let mut steps = 0usize;
    for block in &data.blocks {
        if block.steps == 0
            || block.valid == 0
            || block.inputs.len() != block.steps * data.input_bits
            || block.targets.len() != block.steps * data.target_bits
        {
            return Err(PhaseError::InvalidData);
        }
        steps = steps.checked_add(block.steps).ok_or(PhaseError::InvalidData)?;
        samples += u64::from(block.valid.count_ones()) * block.steps as u64;
    }
    if cap_train && steps > A6C5B_MAX_TRAIN_STEPS {
        return Err(PhaseError::InvalidData);
    }
    Ok(samples)
}

#[inline]
fn binary(truth: u8, a: u64, b: u64) -> u64 {
    let mut out = 0u64;
    if truth & 1 != 0 { out |= !a & !b; }
    if truth & 2 != 0 { out |= a & !b; }
    if truth & 4 != 0 { out |= !a & b; }
    if truth & 8 != 0 { out |= a & b; }
    out
}
#[inline]
fn unary(truth: u8, value: u64) -> u64 {
    match truth {
        0 => 0,
        1 => !value,
        2 => value,
        3 => u64::MAX,
        _ => unreachable!("unary table enumeration guarantees 0..4"),
    }
}
fn baseline_loss(data: &LaneData, spec: &PhaseSpec) -> u64 {
    data.blocks.iter().map(|block| {
        (0..block.steps).map(|t| {
            let residual = block.targets[t * data.target_bits + spec.target_bit]
                ^ block.inputs[t * data.input_bits + spec.baseline_input];
            u64::from((residual & block.valid).count_ones())
        }).sum::<u64>()
    }).sum()
}

fn train_loss(rule: PhaseRule, data: &LaneData, spec: &PhaseSpec) -> u64 {
    let mut errors = 0u64;
    for block in &data.blocks {
        let mut state = [0u64; A6C5B_MAX_STATE_BITS];
        for t in 0..block.steps {
            let root = binary(
                rule.truth,
                state[rule.feedback],
                block.inputs[t * data.input_bits + rule.raw],
            );
            let mut next = [0u64; A6C5B_MAX_STATE_BITS];
            next[0] = root;
            for i in 1..rule.width {
                next[i] = unary(rule.delays[i - 1], state[i - 1]);
            }
            let actual = block.targets[t * data.target_bits + spec.target_bit]
                ^ block.inputs[t * data.input_bits + spec.baseline_input];
            errors += u64::from(((root ^ actual) & block.valid).count_ones());
            state = next;
        }
    }
    errors
}
fn monomials_binary(mask: u8) -> usize {
    let t0 = mask & 1 != 0;
    let t1 = mask & 2 != 0;
    let t2 = mask & 4 != 0;
    let t3 = mask & 8 != 0;
    usize::from(t0) + usize::from(t1 ^ t0)
        + usize::from(t2 ^ t0) + usize::from(t3 ^ t2 ^ t1 ^ t0)
}
fn monomials_unary(mask: u8) -> usize {
    let t0 = mask & 1 != 0;
    let t1 = mask & 2 != 0;
    usize::from(t0) + usize::from(t1 ^ t0)
}
fn anf_terms(rule: PhaseRule) -> usize {
    monomials_binary(rule.truth)
        + rule.delays[..rule.width - 1].iter()
            .map(|&mask| monomials_unary(mask)).sum::<usize>()
}
fn root_uses_state(truth: u8) -> bool {
    (truth & 1 != 0) != (truth & 2 != 0)
        || (truth & 4 != 0) != (truth & 8 != 0)
}
fn effective_period(rule: PhaseRule) -> usize {
    if root_uses_state(rule.truth)
        && rule.delays[..rule.feedback].iter().all(|&mask| mask == 1 || mask == 2)
    {
        rule.feedback + 1
    } else {
        0
    }
}
fn better_train(rule: PhaseRule, errors: u64, prior: Option<(PhaseRule, u64)>) -> bool {
    prior.is_none_or(|(old, old_errors)| {
        (
            errors, anf_terms(rule), effective_period(rule),
            rule.feedback, rule.raw, rule.truth, rule.delays,
        ) < (
            old_errors, anf_terms(old), effective_period(old),
            old.feedback, old.raw, old.truth, old.delays,
        )
    })
}
fn delay_masks(width: usize, mut index: usize) -> [u8; A6C5B_MAX_STATE_BITS - 1] {
    let mut out = [0u8; A6C5B_MAX_STATE_BITS - 1];
    for mask in out.iter_mut().take(width - 1) {
        *mask = (index % 4) as u8;
        index /= 4;
    }
    out
}
pub fn candidate_count(width: usize) -> Option<usize> {
    if (1..=A6C5B_MAX_STATE_BITS).contains(&width) {
        Some(width * A6C5B_RAW_CHOICES * 16 * 4usize.pow((width - 1) as u32))
    } else {
        None
    }
}
fn make_model(rule: PhaseRule, spec: &PhaseSpec) -> Result<LocalAnfNetwork, PhaseError> {
    let source_width = rule.width + A6C5B_INPUT_BITS;
    let root: Vec<bool> = (0..4).map(|i| rule.truth & (1 << i) != 0).collect();
    let mut gates = Vec::with_capacity(rule.width);
    gates.push(LocalAnfGate::from_truth_table(
        vec![rule.feedback, rule.width + rule.raw], &root, source_width,
    )?);
    for i in 1..rule.width {
        let mask = rule.delays[i - 1];
        gates.push(LocalAnfGate::from_truth_table(
            vec![i - 1], &[mask & 1 != 0, mask & 2 != 0], source_width,
        )?);
    }
    let readout = LocalAnfGate::from_truth_table(
        vec![0, rule.width + spec.baseline_input],
        &[false, true, true, false], source_width,
    )?;
    Ok(LocalAnfNetwork::with_input_bits(
        rule.width, A6C5B_INPUT_BITS, gates, readout,
    )?)
}

/// Independently evaluate the actual native ANF network and frozen XOR
/// readout, not the internal bit-mask training-score approximation.
pub fn native_errors(
    model: &LocalAnfNetwork, data: &LaneData, spec: &PhaseSpec,
) -> Result<u64, PhaseError> {
    let samples = validate(data, spec, false)?;
    if model.input_bits() != A6C5B_INPUT_BITS {
        return Err(PhaseError::InvalidData);
    }
    let trajectory = Trajectory::simulate(model, data)?;
    let mut scratch = vec![0u64; model.readout().monomial_count()];
    let mut errors = 0u64;
    for (bi, block) in data.blocks.iter().enumerate() {
        for t in 0..block.steps {
            let prediction = model.readout().evaluate_lanes(
                trajectory.row(bi, t), &mut scratch,
            );
            let actual = block.targets[t * data.target_bits + spec.target_bit];
            errors += u64::from(((prediction ^ actual) & block.valid).count_ones());
        }
    }
    debug_assert!(errors <= samples);
    Ok(errors)
}
fn zero_model(spec: &PhaseSpec) -> Result<LocalAnfNetwork, PhaseError> {
    make_model(PhaseRule {
        width: 1,
        feedback: 0,
        raw: spec.raw_candidates[0],
        truth: 0,
        delays: [0; A6C5B_MAX_STATE_BITS - 1],
    }, spec)
}

/// Train each width using train ONLY. Width/commit decisions use val ONLY.
/// If validation has no strict improvement over frozen x0, return baseline.
/// There is deliberately no test parameter in this API.
pub fn fit_phase_memory(
    train: &LaneData,
    validation: &LaneData,
    spec: &PhaseSpec,
    max_width: usize,
) -> Result<PhaseReport, PhaseError> {
    if !(1..=A6C5B_MAX_STATE_BITS).contains(&max_width) {
        return Err(PhaseError::InvalidSpec);
    }
    let train_samples = validate(train, spec, true)?;
    let validation_samples = validate(validation, spec, false)?;
    let baseline_train_errors = baseline_loss(train, spec);
    let baseline_validation_errors = baseline_loss(validation, spec);
    let mut widths = Vec::with_capacity(max_width);
    let mut objective_evaluations = 0usize;

    for width in 1..=max_width {
        let mut best: Option<(PhaseRule, u64)> = None;
        let variants = 4usize.pow((width - 1) as u32);
        let mut evaluations = 0usize;
        for feedback in 0..width {
            for &raw in &spec.raw_candidates {
                for truth in 0..16u8 {
                    for delay in 0..variants {
                        let rule = PhaseRule {
                            width, feedback, raw, truth,
                            delays: delay_masks(width, delay),
                        };
                        let errors = train_loss(rule, train, spec);
                        evaluations += 1;
                        if better_train(rule, errors, best) {
                            best = Some((rule, errors));
                        }
                    }
                }
            }
        }
        if Some(evaluations) != candidate_count(width) {
            return Err(PhaseError::InvalidSpec);
        }
        let (rule, train_errors) = best.ok_or(PhaseError::InvalidSpec)?;
        let network = make_model(rule, spec)?;
        if native_errors(&network, train, spec)? != train_errors {
            return Err(PhaseError::ScoreMismatch);
        }
        let validation_errors = native_errors(&network, validation, spec)?;
        objective_evaluations += evaluations;
        widths.push(WidthFit {
            rule,
            network,
            train_errors,
            validation_errors,
            evaluations,
            anf_nonzero_monomials: anf_terms(rule),
            effective_period: effective_period(rule),
            accepted_against_zero: validation_errors < baseline_validation_errors,
        });
    }

    let mut chosen = 0usize;
    for index in 1..widths.len() {
        let old = &widths[chosen];
        let candidate = &widths[index];
        if (
            candidate.validation_errors,
            candidate.rule.width,
            candidate.anf_nonzero_monomials,
        ) < (
            old.validation_errors,
            old.rule.width,
            old.anf_nonzero_monomials,
        ) {
            chosen = index;
        }
    }
    let best = &widths[chosen];
    let accepted = best.validation_errors < baseline_validation_errors;
    Ok(PhaseReport {
        selected: if accepted { best.network.clone() } else { zero_model(spec)? },
        selected_rule: best.rule,
        selected_width: if accepted { best.rule.width } else { 0 },
        selected_effective_period: if accepted { best.effective_period } else { 0 },
        selected_train_errors: best.train_errors,
        selected_validation_errors: best.validation_errors,
        accepted,
        widths,
        baseline_train_errors,
        baseline_validation_errors,
        train_samples,
        validation_samples,
        objective_evaluations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{anf_local_incremental::LaneBlock, toy_tasks::SplitMix64};

    fn spec() -> PhaseSpec {
        PhaseSpec {
            raw_candidates: vec![1, 2, 3, 4],
            baseline_input: 0,
            target_bit: 0,
        }
    }
    fn dataset(seed: u64, steps: usize, stride: usize) -> LaneData {
        let mut rng = SplitMix64::new(seed);
        let mut inputs = vec![0u64; steps * A6C5B_INPUT_BITS];
        for input in &mut inputs {
            *input = rng.next_u64();
        }
        let mut phases = [0u64; 6];
        let mut targets = vec![0u64; steps];
        for (t, target) in targets.iter_mut().enumerate() {
            let signal = inputs[t * A6C5B_INPUT_BITS + 1];
            if stride == 0 {
                *target = inputs[t * A6C5B_INPUT_BITS] ^ signal;
            } else {
                phases[t % stride] ^= signal;
                *target = inputs[t * A6C5B_INPUT_BITS] ^ phases[t % stride];
            }
        }
        LaneData {
            input_bits: A6C5B_INPUT_BITS,
            target_bits: 1,
            blocks: vec![LaneBlock { steps, inputs, targets, valid: u64::MAX }],
        }
    }

    #[test]
    fn all_root_truth_tables_match_native_anf_semantics() {
        for truth in 0..16u8 {
            let rule = PhaseRule {
                width: 3, feedback: 2, raw: 1, truth, delays: [2, 2, 0],
            };
            let model = make_model(rule, &spec()).unwrap();
            let mut products = [0u64; 4];
            for a in [0, u64::MAX] {
                for b in [0, u64::MAX] {
                    let sources = [0, 0, a, 0, b, 0, 0, 0];
                    assert_eq!(
                        model.gates()[0].evaluate_lanes(&sources, &mut products),
                        binary(truth, a, b),
                    );
                }
            }
        }
        assert_eq!(candidate_count(1), Some(64));
        assert_eq!(candidate_count(2), Some(512));
        assert_eq!(candidate_count(3), Some(3_072));
        assert_eq!(candidate_count(4), Some(16_384));
    }

    #[test]
    fn width_three_trains_terminal_parity_without_intermediate_labels() {
        let fit = fit_phase_memory(
            &dataset(10, 64, 3), &dataset(11, 64, 3), &spec(), 4,
        ).unwrap();
        assert_eq!(fit.selected_width, 3);
        assert_eq!(fit.selected_effective_period, 3);
        assert_eq!(fit.selected_train_errors, 0);
        assert_eq!(fit.selected_validation_errors, 0);
        assert!(fit.widths[1].train_errors > 0);
        assert_eq!(fit.objective_evaluations, 20_032);
        assert_eq!(native_errors(&fit.selected, &dataset(12, 512, 3), &spec()).unwrap(), 0);
        assert_eq!(native_errors(&fit.selected, &dataset(13, 4096, 3), &spec()).unwrap(), 0);
    }

    #[test]
    fn width_four_recovers_stride_four_where_three_bits_fail() {
        let fit = fit_phase_memory(
            &dataset(20, 64, 4), &dataset(21, 64, 4), &spec(), 4,
        ).unwrap();
        assert_eq!(fit.selected_width, 4);
        assert_eq!(fit.selected_effective_period, 4);
        assert_eq!(fit.selected_train_errors, 0);
        assert!(fit.widths[2].train_errors > 0);
        assert_eq!(native_errors(&fit.selected, &dataset(22, 512, 4), &spec()).unwrap(), 0);
    }

    #[test]
    fn stride_five_remains_an_intentional_unrepresentable_control() {
        let fit = fit_phase_memory(
            &dataset(30, 64, 5), &dataset(31, 64, 5), &spec(), 4,
        ).unwrap();
        assert!(fit.widths.iter().all(|w| w.train_errors > 0));
        assert!(native_errors(&fit.selected, &dataset(32, 512, 5), &spec()).unwrap() > 0);
    }

    #[test]
    fn validation_label_changes_do_not_rewrite_train_fitted_gates() {
        let train = dataset(40, 64, 3);
        let val = dataset(41, 64, 3);
        let original = fit_phase_memory(&train, &val, &spec(), 4).unwrap();
        let mut inverted = val.clone();
        for block in &mut inverted.blocks {
            for label in &mut block.targets {
                *label = !*label;
            }
        }
        let changed = fit_phase_memory(&train, &inverted, &spec(), 4).unwrap();
        for (a, b) in original.widths.iter().zip(&changed.widths) {
            assert_eq!(a.rule, b.rule);
            assert_eq!(a.train_errors, b.train_errors);
            assert_eq!(a.network, b.network);
        }
    }

    #[test]
    fn incorrect_source_pool_or_malformed_data_are_rejected() {
        let bad = PhaseSpec {
            raw_candidates: vec![1, 1, 3, 4], ..spec()
        };
        let train = dataset(50, 16, 3);
        assert_eq!(
            fit_phase_memory(&train, &train, &bad, 4),
            Err(PhaseError::InvalidSpec),
        );
        assert_eq!(
            fit_phase_memory(&train, &train, &spec(), 0),
            Err(PhaseError::InvalidSpec),
        );
        let mut malformed = train.clone();
        malformed.blocks[0].targets.clear();
        assert_eq!(
            fit_phase_memory(&malformed, &train, &spec(), 4),
            Err(PhaseError::InvalidData),
        );
    }
}
