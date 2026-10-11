//! A6-C8: train zero, one or TWO hidden affine ANF gates on the final loss.
//!
//! An N=128/256 recurrent delay ring has a fixed root XOR and native XOR
//! readout. Intermediate gates default to identity. Candidate mutations are
//! XOR(state_prev, raw) and XNOR(state_prev, raw) at unknown, distinct slots.
//! Both are real 2-input GF(2) ANF gates whose position, input channel and
//! truth table are learned from final labels; no hidden targets are supplied.
//!
//! Two independent XOR injections have ZERO single-gate marginal credit.
//! The observable terminal residual against the frozen identity backbone
//! satisfies: residual(t) = residual(t-N) XOR sum(injection at t-lag).
//! Innovation z(t)=residual(t) XOR residual(t-N) therefore separates recurrent
//! temporal transport from the joint unknown gate contributions. Even when
//! labels have independent bit flips, this remains a train-only score with
//! a noisy innovation target (two correlated noise terms).
//!
//! Pair discovery is a fixed-cost, two-stage HEURISTIC:
//! 1. ALL distinct-position pairs scored on the LAST 32 TRAIN timesteps;
//! 2. top 64 pairs rescored on COMPLETE TRAIN innovation, no validation/test.
//! An independent real LocalAnfNetwork simulation MUST be bit-exact with
//! additive innovation for selected pairs before their validation decision.
//! Finally only VALIDATION chooses between 0, 1 and 2 candidate gates,
//! preferring the smaller architecture on an equal validation score.
//!
//! This DOES NOT optimize arbitrary two-input truth tables jointly, general
//! non-affine AND interactions, cycles with unknown topology or 256 gates.

use std::{collections::BinaryHeap, fmt};

use crate::{
    anf_intermediate_credit::{
        native_model as identity_native_model,
        IntermediateRule, A6C7_INPUT_BITS, A6C7_MAX_WIDTH,
    },
    anf_local::{LocalAnfError, LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::LaneData,
};

pub const A6C8_MAX_WIDTH: usize = A6C7_MAX_WIDTH;
pub const A6C8_INPUT_BITS: usize = A6C7_INPUT_BITS;
pub const A6C8_PROBE_STEPS: usize = 32;
pub const A6C8_BEAM_WIDTH: usize = 64;
pub const A6C8_MAX_TRAIN_STEPS: usize = 768;
pub const A6C8_AFFINE_TRUTHS: [u8; 2] = [6, 9]; // XOR, XNOR
pub const A6C8_RAW_CHOICES: [usize; 4] = [1, 2, 3, 4];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DualCreditError {
    Network(LocalAnfError),
    InvalidWidth,
    InvalidData,
    InvalidSpec,
    NativeOracle,
}
impl fmt::Display for DualCreditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DualCreditError {}
impl From<LocalAnfError> for DualCreditError {
    fn from(e: LocalAnfError) -> Self {
        Self::Network(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DualCreditReport {
    pub width: usize,
    pub reference: LocalAnfNetwork,
    /// Best train-innovation fit among individual affine mutations.
    pub single_proposed: LocalAnfNetwork,
    pub single_rule: IntermediateRule,
    /// Best complete-train innovation fit among 64 beam survivors.
    pub joint_proposed: LocalAnfNetwork,
    pub joint_rules: [IntermediateRule; 2],
    /// Zero, one or two mutations selected only on validation.
    pub selected: LocalAnfNetwork,
    pub selected_rules: Vec<IntermediateRule>,
    pub selected_count: usize,
    pub baseline_train_errors: u64,
    pub baseline_validation_errors: u64,
    pub single_train_errors: u64,
    pub single_validation_errors: u64,
    pub joint_train_errors: u64,
    pub joint_validation_errors: u64,
    pub selected_train_errors: u64,
    pub selected_validation_errors: u64,
    pub train_samples: u64,
    pub validation_samples: u64,
    pub one_gate_candidates: usize,
    pub two_gate_pairs_probed: usize,
    pub pairs_full_train_rescored: usize,
    pub probe_steps: usize,
    pub beam_width: usize,
    pub feature_probe_payload_bytes: usize,
}

#[inline]
fn bool_truth(truth: u8) -> [bool; 4] {
    std::array::from_fn(|i| truth & (1 << i) != 0)
}

/// Native generator and scorer utility. This supports 0..N-1 gate
/// modifications, including non-affine AND only for negative fixtures.
/// The learner itself can propose <=2 affine modifications (truth 6/9).
pub fn make_multi_gate_native(
    width: usize,
    mutations: &[IntermediateRule],
) -> Result<LocalAnfNetwork, DualCreditError> {
    if !(2..=A6C8_MAX_WIDTH).contains(&width) {
        return Err(DualCreditError::InvalidWidth);
    }
    let mut seen = vec![false; width];
    let mut net = identity_native_model(width, None).map_err(|e| match e {
        crate::anf_intermediate_credit::IntermediateError::Network(inner) => {
            DualCreditError::Network(inner)
        }
        _ => DualCreditError::InvalidSpec,
    })?;
    let source_width = net.source_width();
    for rule in mutations {
        if rule.gate == 0
            || rule.gate >= width
            || !A6C8_RAW_CHOICES.contains(&rule.raw)
            || rule.truth >= 16
            || seen[rule.gate]
        {
            return Err(DualCreditError::InvalidSpec);
        }
        seen[rule.gate] = true;
        net.gates_mut()[rule.gate] = LocalAnfGate::from_truth_table(
            vec![rule.gate - 1, width + rule.raw],
            &bool_truth(rule.truth),
            source_width,
        )?;
    }
    Ok(net)
}

/// The native evaluator is independent of the innovation scorer.
pub fn native_errors(model: &LocalAnfNetwork, data: &LaneData) -> Result<u64, DualCreditError> {
    crate::anf_intermediate_credit::native_errors(model, data).map_err(|e| match e {
        crate::anf_intermediate_credit::IntermediateError::Network(inner) => {
            DualCreditError::Network(inner)
        }
        _ => DualCreditError::InvalidData,
    })
}

fn validate_data(
    width: usize,
    data: &LaneData,
    capped_train: bool,
) -> Result<u64, DualCreditError> {
    if !(2..=A6C8_MAX_WIDTH).contains(&width) {
        return Err(DualCreditError::InvalidWidth);
    }
    if data.input_bits != A6C8_INPUT_BITS || data.target_bits != 1 || data.blocks.is_empty() {
        return Err(DualCreditError::InvalidData);
    }
    let mut total_steps = 0usize;
    let mut samples = 0u64;
    for block in &data.blocks {
        if block.steps == 0
            || block.valid == 0
            || block.inputs.len() != block.steps * data.input_bits
            || block.targets.len() != block.steps * data.target_bits
        {
            return Err(DualCreditError::InvalidData);
        }
        total_steps = total_steps
            .checked_add(block.steps)
            .ok_or(DualCreditError::InvalidData)?;
        samples += u64::from(block.valid.count_ones()) * block.steps as u64;
    }
    if capped_train
        && (total_steps > A6C8_MAX_TRAIN_STEPS
            || data
                .blocks
                .last()
                .is_none_or(|block| block.steps < width + A6C8_PROBE_STEPS))
    {
        return Err(DualCreditError::InvalidData);
    }
    Ok(samples)
}

pub fn candidate_count(width: usize) -> Option<usize> {
    (2..=A6C8_MAX_WIDTH)
        .contains(&width)
        .then_some((width - 1) * A6C8_RAW_CHOICES.len() * A6C8_AFFINE_TRUTHS.len())
}
pub fn pair_count(width: usize) -> Option<usize> {
    (2..=A6C8_MAX_WIDTH).contains(&width).then_some(
        ((width - 1) * (width - 2) / 2)
            * (A6C8_RAW_CHOICES.len() * A6C8_AFFINE_TRUTHS.len()).pow(2),
    )
}
fn candidates(width: usize) -> Vec<IntermediateRule> {
    let mut out = Vec::with_capacity(candidate_count(width).unwrap_or(0));
    for gate in 1..width {
        for raw in A6C8_RAW_CHOICES {
            for truth in A6C8_AFFINE_TRUTHS {
                out.push(IntermediateRule { gate, raw, truth });
            }
        }
    }
    out
}

fn innovation_from_labels(width: usize, data: &LaneData) -> Vec<Vec<u64>> {
    data.blocks
        .iter()
        .map(|block| {
            let mut ring = vec![0u64; width];
            let mut residual = vec![0u64; block.steps];
            for (t, target) in block.targets.iter().enumerate() {
                let position = t % width;
                let current_root = ring[position] ^ block.inputs[t * A6C8_INPUT_BITS + 1];
                ring[position] = current_root;
                residual[t] = *target ^ block.inputs[t * A6C8_INPUT_BITS] ^ current_root;
            }
            let mut innovation = vec![0u64; block.steps];
            for t in 0..block.steps {
                innovation[t] = residual[t] ^ if t >= width { residual[t - width] } else { 0 };
            }
            innovation
        })
        .collect()
}

#[inline]
fn feature(width: usize, rule: IntermediateRule, block_inputs: &[u64], t: usize) -> u64 {
    let lag = width - rule.gate;
    if t < lag {
        return 0;
    }
    let raw = block_inputs[(t - lag) * A6C8_INPUT_BITS + rule.raw];
    if rule.truth == 9 {
        !raw
    } else {
        raw
    }
}

fn train_innovation_errors(
    width: usize,
    data: &LaneData,
    innovations: &[Vec<u64>],
    rules: &[IntermediateRule],
) -> u64 {
    let mut errors = 0u64;
    for (bi, block) in data.blocks.iter().enumerate() {
        for t in 0..block.steps {
            let mut predicted = 0u64;
            for &rule in rules {
                predicted ^= feature(width, rule, &block.inputs, t);
            }
            errors += u64::from(((innovations[bi][t] ^ predicted) & block.valid).count_ones());
        }
    }
    errors
}

/// Simulate a native recurrent network and extract ONLY its predicted final
/// output. Rebuild innovation against the same frozen reference as an
/// independent oracle; it must exactly equal the affine feature sum.
/// This oracle never uses the labels of the original training dataset.
fn oracle_affine_native(
    width: usize,
    data: &LaneData,
    rules: &[IntermediateRule],
) -> Result<(), DualCreditError> {
    let net = make_multi_gate_native(width, rules)?;
    let mut prediction_data = data.clone();
    let mut source = vec![0u64; net.source_width()];
    let mut next = vec![0u64; width];
    let mut scratch = vec![0u64; net.scratch_len()];
    for block in &mut prediction_data.blocks {
        source.fill(0);
        for t in 0..block.steps {
            let begin = t * A6C8_INPUT_BITS;
            source[width..].copy_from_slice(&block.inputs[begin..begin + A6C8_INPUT_BITS]);
            net.step_lanes(&mut source, &mut next, &mut scratch);
            block.targets[t] = net.readout().evaluate_lanes(&source, &mut scratch);
        }
    }
    let innovations = innovation_from_labels(width, &prediction_data);
    if train_innovation_errors(width, &prediction_data, &innovations, rules) != 0 {
        return Err(DualCreditError::NativeOracle);
    }
    Ok(())
}

/// Jointly discover two zero-marginal XOR/XNOR hidden gates.
/// Probe is the LAST 32 TRAIN timesteps of the last training block.
/// The top 64 joint hypotheses then receive a complete train scan.
/// This beam is a heuristic and is NOT a globally exact full-train search.
pub fn fit_dual_hidden_credit(
    train: &LaneData,
    validation: &LaneData,
    width: usize,
) -> Result<DualCreditReport, DualCreditError> {
    let train_samples = validate_data(width, train, true)?;
    let validation_samples = validate_data(width, validation, false)?;
    let innovation = innovation_from_labels(width, train);
    let bank = candidates(width);
    let baseline = make_multi_gate_native(width, &[])?;
    let baseline_train_errors = native_errors(&baseline, train)?;
    let baseline_validation_errors = native_errors(&baseline, validation)?;

    // Individual-gate train control. No other gate labels available.
    let mut best_one: Option<(u64, usize)> = None;
    for (idx, &rule) in bank.iter().enumerate() {
        let loss = train_innovation_errors(width, train, &innovation, &[rule]);
        if best_one.is_none_or(|(old, prior)| (loss, idx) < (old, prior)) {
            best_one = Some((loss, idx));
        }
    }
    let (_, one_index) = best_one.ok_or(DualCreditError::InvalidSpec)?;
    let single_rule = bank[one_index];

    // Contiguous late-time probe arrays: 32 * 64 binary observations.
    // No future labels or validation data enter the candidate ranking.
    let probe_block = train.blocks.last().ok_or(DualCreditError::InvalidData)?;
    let probe_z = innovation.last().ok_or(DualCreditError::InvalidData)?;
    let start = probe_block.steps - A6C8_PROBE_STEPS;
    let mut target = [0u64; A6C8_PROBE_STEPS];
    target.copy_from_slice(&probe_z[start..]);
    let mut features = Vec::with_capacity(bank.len());
    for &rule in &bank {
        let mut values = [0u64; A6C8_PROBE_STEPS];
        for (i, word) in values.iter_mut().enumerate() {
            *word = feature(width, rule, &probe_block.inputs, start + i);
        }
        features.push(values);
    }

    // BinaryHeap keeps only the 64 smallest (probe error, first, second).
    // Pairs from the SAME physical gate position are forbidden.
    let mut best_probe = BinaryHeap::with_capacity(A6C8_BEAM_WIDTH + 1);
    let mut pair_evaluations = 0usize;
    for a in 0..bank.len() {
        for b in a + 1..bank.len() {
            if bank[a].gate == bank[b].gate {
                continue;
            }
            let mut error = 0u32;
            for t in 0..A6C8_PROBE_STEPS {
                error += ((target[t] ^ features[a][t] ^ features[b][t]) & probe_block.valid)
                    .count_ones();
            }
            pair_evaluations += 1;
            let item = (error, a, b);
            if best_probe.len() < A6C8_BEAM_WIDTH {
                best_probe.push(item);
            } else if best_probe.peek().is_some_and(|&worst| item < worst) {
                best_probe.pop();
                best_probe.push(item);
            }
        }
    }
    if Some(pair_evaluations) != pair_count(width) {
        return Err(DualCreditError::InvalidSpec);
    }
    let mut shortlist = best_probe.into_sorted_vec();
    shortlist.sort_unstable();
    let full_pairs = shortlist.len();
    let mut best_two: Option<(u64, usize, usize)> = None;
    for (_, a, b) in shortlist {
        let loss = train_innovation_errors(width, train, &innovation, &[bank[a], bank[b]]);
        if best_two.is_none_or(|old| (loss, a, b) < old) {
            best_two = Some((loss, a, b));
        }
    }
    let (_, a, b) = best_two.ok_or(DualCreditError::InvalidSpec)?;
    let joint_rules = [bank[a], bank[b]];

    // Separate model-based terminal scores must reflect the *true* native
    // recurrent execution. The affine innovation oracle validates geometry.
    oracle_affine_native(width, train, &[single_rule])?;
    oracle_affine_native(width, train, &joint_rules)?;
    let single_proposed = make_multi_gate_native(width, &[single_rule])?;
    let joint_proposed = make_multi_gate_native(width, &joint_rules)?;
    let single_train_errors = native_errors(&single_proposed, train)?;
    let single_validation_errors = native_errors(&single_proposed, validation)?;
    let joint_train_errors = native_errors(&joint_proposed, train)?;
    let joint_validation_errors = native_errors(&joint_proposed, validation)?;

    // Model-count selection is controlled ONLY by validation labels.
    // Identity is the default on exact ties; never tune gates on test.
    let mut selected_count = 0usize;
    let mut selected_validation_errors = baseline_validation_errors;
    for (count, loss) in [(1, single_validation_errors), (2, joint_validation_errors)] {
        if (loss, count) < (selected_validation_errors, selected_count) {
            selected_count = count;
            selected_validation_errors = loss;
        }
    }
    let selected_rules = match selected_count {
        0 => Vec::new(),
        1 => vec![single_rule],
        2 => joint_rules.to_vec(),
        _ => unreachable!("at most two candidate mutations"),
    };
    let selected = match selected_count {
        0 => baseline.clone(),
        1 => single_proposed.clone(),
        2 => joint_proposed.clone(),
        _ => unreachable!("at most two candidate mutations"),
    };
    let selected_train_errors = match selected_count {
        0 => baseline_train_errors,
        1 => single_train_errors,
        2 => joint_train_errors,
        _ => unreachable!("at most two candidate mutations"),
    };
    Ok(DualCreditReport {
        width,
        reference: baseline,
        single_proposed,
        single_rule,
        joint_proposed,
        joint_rules,
        selected,
        selected_rules,
        selected_count,
        baseline_train_errors,
        baseline_validation_errors,
        single_train_errors,
        single_validation_errors,
        joint_train_errors,
        joint_validation_errors,
        selected_train_errors,
        selected_validation_errors,
        train_samples,
        validation_samples,
        one_gate_candidates: bank.len(),
        two_gate_pairs_probed: pair_evaluations,
        pairs_full_train_rescored: full_pairs,
        probe_steps: A6C8_PROBE_STEPS,
        beam_width: A6C8_BEAM_WIDTH,
        feature_probe_payload_bytes: features.len() * A6C8_PROBE_STEPS * std::mem::size_of::<u64>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{anf_local_incremental::LaneBlock, toy_tasks::SplitMix64};

    fn dataset(seed: u64, width: usize, steps: usize, rules: &[IntermediateRule]) -> LaneData {
        let model = make_multi_gate_native(width, rules).unwrap();
        let mut rng = SplitMix64::new(seed);
        let mut inputs = vec![0u64; steps * A6C8_INPUT_BITS];
        for word in &mut inputs {
            *word = rng.next_u64();
        }
        let mut state = vec![0u64; model.source_width()];
        let mut next = vec![0u64; width];
        let mut scratch = vec![0u64; model.scratch_len()];
        let mut targets = vec![0u64; steps];
        for (t, target) in targets.iter_mut().enumerate() {
            let start = t * A6C8_INPUT_BITS;
            state[width..].copy_from_slice(&inputs[start..start + A6C8_INPUT_BITS]);
            model.step_lanes(&mut state, &mut next, &mut scratch);
            *target = model.readout().evaluate_lanes(&state, &mut scratch);
        }
        LaneData {
            input_bits: A6C8_INPUT_BITS,
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
    fn pair_counts_and_xnor_native_innovation_are_exact() {
        assert_eq!(candidate_count(128), Some(1016));
        assert_eq!(candidate_count(256), Some(2040));
        assert_eq!(pair_count(128), Some(512_064));
        assert_eq!(pair_count(256), Some(2_072_640));
        for width in [16, 32] {
            let rules = [
                IntermediateRule {
                    gate: 3,
                    raw: 2,
                    truth: 6,
                },
                IntermediateRule {
                    gate: width - 2,
                    raw: 3,
                    truth: 9,
                },
            ];
            let test = dataset(19, width, 128, &rules);
            oracle_affine_native(width, &test, &rules).unwrap();
            let z = innovation_from_labels(width, &test);
            assert_eq!(train_innovation_errors(width, &test, &z, &rules), 0);
        }
    }

    #[test]
    fn jointly_fits_two_zero_marginal_hidden_gates_without_auxiliary_labels() {
        let width = 32;
        let rules = [
            IntermediateRule {
                gate: 7,
                raw: 2,
                truth: 6,
            },
            IntermediateRule {
                gate: 21,
                raw: 3,
                truth: 6,
            },
        ];
        let train = dataset(100, width, 160, &rules);
        let validation = dataset(200, width, 160, &rules);
        let fit = fit_dual_hidden_credit(&train, &validation, width).unwrap();
        assert_eq!(fit.selected_count, 2);
        assert_eq!(fit.selected_rules, rules);
        assert_eq!(fit.joint_train_errors, 0);
        assert_eq!(fit.joint_validation_errors, 0);
        assert!(fit.single_train_errors > 0);
        let test = dataset(300, width, 512, &rules);
        assert_eq!(native_errors(&fit.selected, &test).unwrap(), 0);
    }

    #[test]
    fn identity_is_stable_without_fictitious_hidden_mutation() {
        let width = 32;
        let train = dataset(101, width, 160, &[]);
        let val = dataset(102, width, 160, &[]);
        let report = fit_dual_hidden_credit(&train, &val, width).unwrap();
        assert_eq!(report.selected_count, 0);
        assert_eq!(report.selected, report.reference);
        assert_eq!(report.baseline_train_errors, 0);
    }

    #[test]
    fn triple_mutation_is_not_misrepresented_as_exactly_two() {
        let width = 32;
        let rules = [
            IntermediateRule {
                gate: 4,
                raw: 2,
                truth: 6,
            },
            IntermediateRule {
                gate: 13,
                raw: 3,
                truth: 6,
            },
            IntermediateRule {
                gate: 25,
                raw: 4,
                truth: 6,
            },
        ];
        let train = dataset(103, width, 160, &rules);
        let val = dataset(104, width, 160, &rules);
        let report = fit_dual_hidden_credit(&train, &val, width).unwrap();
        assert!(report.joint_train_errors > 0);
        let test = dataset(105, width, 512, &rules);
        assert!(native_errors(&report.selected, &test).unwrap() > 0);
    }

    #[test]
    fn duplicate_physical_gates_and_too_short_training_are_rejected() {
        assert_eq!(
            make_multi_gate_native(
                32,
                &[
                    IntermediateRule {
                        gate: 3,
                        raw: 2,
                        truth: 6
                    },
                    IntermediateRule {
                        gate: 3,
                        raw: 3,
                        truth: 9
                    },
                ]
            ),
            Err(DualCreditError::InvalidSpec),
        );
        let short = dataset(106, 32, 16, &[]);
        let valid = dataset(107, 32, 160, &[]);
        assert_eq!(
            fit_dual_hidden_credit(&short, &valid, 32),
            Err(DualCreditError::InvalidData),
        );
    }
}
