//! A6-C9: terminal-loss credit for TWO unknown nonlinear ANF state gates.
//!
//! Fixed 128/256-bit recurrent delay ring and readout (as A6-C7/8).
//! Train-only starts: full truth-table one-gate A6-C7 candidate and
//! affine two-gate A6-C8 candidate. Enumerate an unknown second gate
//! conditioned on the first from the observable FINAL TASK error,
//! then make ONE strictly improving two-axis coordinate pass on TRAIN.
//! All candidate tables use the entire 16-function Boolean grammar.
//!
//! The scorer partitions the N-word circular state into three delay
//! segments: root->gate_a, gate_a->gate_b, gate_b->root. Each step
//! updates synchronously using only PRIOR state. This is bit-exact
//! with native LocalAnfNetwork on selected candidates. No hidden state
//! targets, future labels or held-out test data enter the fitting API.
//!
//! The algorithm is deliberately not exhaustive over every pair,
//! not guaranteed globally optimal, and not an arbitrary learned DAG.
//! All hypothesis counts and control budgets are reported.

use std::fmt;

use crate::{
    anf_dual_hidden_credit::{
        fit_dual_hidden_credit, make_multi_gate_native, native_errors as native_dual_errors,
        DualCreditError, DualCreditReport,
    },
    anf_intermediate_credit::{
        fit_intermediate_gate, IntermediateError, IntermediateReport, IntermediateRule,
    },
    anf_local::LocalAnfNetwork,
    anf_local_incremental::LaneData,
};

pub const A6C9_MAX_WIDTH: usize = 256;
pub const A6C9_INPUT_BITS: usize = 5;
pub const A6C9_TRAIN_MAX_STEPS: usize = 768;
pub const A6C9_RAW_CHOICES: [usize; 4] = [1, 2, 3, 4];
pub const A6C9_TRUTH_TABLES: usize = 16;
pub const A6C9_COORDINATE_PASSES: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NonlinearError {
    InvalidWidth,
    InvalidData,
    InvalidRule,
    PriorSingle(IntermediateError),
    PriorAffine(DualCreditError),
    NativeOracle,
}
impl fmt::Display for NonlinearError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for NonlinearError {}
impl From<IntermediateError> for NonlinearError {
    fn from(e: IntermediateError) -> Self {
        Self::PriorSingle(e)
    }
}
impl From<DualCreditError> for NonlinearError {
    fn from(e: DualCreditError) -> Self {
        Self::PriorAffine(e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditOrigin {
    Identity,
    OneGate,
    AffinePair,
    NonlinearPair,
}
impl CreditOrigin {
    pub fn name(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::OneGate => "a6c7_one_gate",
            Self::AffinePair => "a6c8_affine_pair",
            Self::NonlinearPair => "a6c9_nonlinear_pair",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonlinearReport {
    pub width: usize,
    pub old_one: IntermediateReport,
    pub old_affine: DualCreditReport,
    pub conditional_rules: Option<[IntermediateRule; 2]>,
    pub conditional_proposed: Option<LocalAnfNetwork>,
    pub selected: LocalAnfNetwork,
    pub selected_rules: Vec<IntermediateRule>,
    pub selected_origin: CreditOrigin,
    pub selected_count: usize,
    pub baseline_train_errors: u64,
    pub baseline_validation_errors: u64,
    pub conditional_train_errors: Option<u64>,
    pub conditional_validation_errors: Option<u64>,
    pub selected_train_errors: u64,
    pub selected_validation_errors: u64,
    pub train_samples: u64,
    pub validation_samples: u64,
    pub conditional_evaluations: usize,
    pub coordinate_evaluations: usize,
    pub conditional_accepted_updates: usize,
    /// Only reused mutable circular-state payload, not total program memory.
    pub ring_payload_bytes: usize,
}

fn validate(width: usize, data: &LaneData, capped_train: bool) -> Result<u64, NonlinearError> {
    if !(3..=A6C9_MAX_WIDTH).contains(&width) {
        return Err(NonlinearError::InvalidWidth);
    }
    if data.input_bits != A6C9_INPUT_BITS || data.target_bits != 1 || data.blocks.is_empty() {
        return Err(NonlinearError::InvalidData);
    }
    let mut samples = 0u64;
    let mut total_steps = 0usize;
    for block in &data.blocks {
        if block.steps == 0
            || block.valid == 0
            || block.inputs.len() != block.steps * A6C9_INPUT_BITS
            || block.targets.len() != block.steps
        {
            return Err(NonlinearError::InvalidData);
        }
        total_steps = total_steps
            .checked_add(block.steps)
            .ok_or(NonlinearError::InvalidData)?;
        samples += u64::from(block.valid.count_ones()) * block.steps as u64;
    }
    if capped_train && total_steps > A6C9_TRAIN_MAX_STEPS {
        return Err(NonlinearError::InvalidData);
    }
    Ok(samples)
}

#[inline]
fn apply_truth(mask: u8, a: u64, b: u64) -> u64 {
    let mut v = 0u64;
    if mask & 1 != 0 {
        v |= !a & !b;
    }
    if mask & 2 != 0 {
        v |= a & !b;
    }
    if mask & 4 != 0 {
        v |= !a & b;
    }
    if mask & 8 != 0 {
        v |= a & b;
    }
    v
}

fn sorted_pair(
    width: usize,
    a: IntermediateRule,
    b: IntermediateRule,
) -> Result<[IntermediateRule; 2], NonlinearError> {
    if a.gate == b.gate
        || a.gate == 0
        || b.gate == 0
        || a.gate >= width
        || b.gate >= width
        || a.truth >= 16
        || b.truth >= 16
        || !A6C9_RAW_CHOICES.contains(&a.raw)
        || !A6C9_RAW_CHOICES.contains(&b.raw)
    {
        return Err(NonlinearError::InvalidRule);
    }
    Ok(if a.gate < b.gate { [a, b] } else { [b, a] })
}

/// O(T) exact paired recurrence oracle. Three disjoint circular regions of
/// the same width-word u64 buffer hold the delayed outputs of root, first
/// hidden gate and second hidden gate, respectively. Blocks reset to zero.
///
/// Hidden positions are sorted in ascending physical order. All three gate
/// updates for time t depend on their prior-state regions from t-1.
fn score_two(
    width: usize,
    pair: [IntermediateRule; 2],
    data: &LaneData,
    scratch: &mut [u64],
) -> u64 {
    debug_assert!(scratch.len() >= width);
    let [a, b] = pair;
    debug_assert!(a.gate > 0 && a.gate < b.gate && b.gate < width);
    let len0 = a.gate;
    let len1 = b.gate - a.gate;
    let len2 = width - b.gate;
    let mut errors = 0u64;
    for block in &data.blocks {
        scratch.fill(0);
        for t in 0..block.steps {
            let p0 = t % len0;
            let p1 = len0 + t % len1;
            let p2 = b.gate + t % len2;
            let root = scratch[p2] ^ block.inputs[t * A6C9_INPUT_BITS + 1];
            let next_a = apply_truth(
                a.truth,
                scratch[p0],
                block.inputs[t * A6C9_INPUT_BITS + a.raw],
            );
            let next_b = apply_truth(
                b.truth,
                scratch[p1],
                block.inputs[t * A6C9_INPUT_BITS + b.raw],
            );
            scratch[p0] = root;
            scratch[p1] = next_a;
            scratch[p2] = next_b;
            let actual = block.targets[t] ^ block.inputs[t * A6C9_INPUT_BITS];
            errors += u64::from(((actual ^ root) & block.valid).count_ones());
        }
    }
    errors
}

pub fn score_pair(
    width: usize,
    pair: [IntermediateRule; 2],
    data: &LaneData,
    scratch: &mut [u64],
) -> Result<u64, NonlinearError> {
    validate(width, data, false)?;
    if scratch.len() < width {
        return Err(NonlinearError::InvalidData);
    }
    let pair = sorted_pair(width, pair[0], pair[1])?;
    Ok(score_two(width, pair, data, scratch))
}

pub fn second_candidate_count(width: usize) -> Option<usize> {
    (3..=A6C9_MAX_WIDTH)
        .contains(&width)
        .then_some((width - 2) * A6C9_RAW_CHOICES.len() * A6C9_TRUTH_TABLES)
}

fn trained_better(
    trial: [IntermediateRule; 2],
    loss: u64,
    incumbent: Option<([IntermediateRule; 2], u64)>,
) -> bool {
    incumbent.is_none_or(|(old, old_loss)| {
        let fields = |pair: [IntermediateRule; 2]| {
            (
                pair[0].gate,
                pair[0].raw,
                pair[0].truth,
                pair[1].gate,
                pair[1].raw,
                pair[1].truth,
            )
        };
        (loss, fields(trial)) < (old_loss, fields(old))
    })
}

struct TrainContext<'a> {
    width: usize,
    data: &'a LaneData,
    scratch: &'a mut [u64],
    evaluations: &'a mut usize,
}
impl TrainContext<'_> {
    fn errors(&mut self, pair: [IntermediateRule; 2]) -> u64 {
        *self.evaluations += 1;
        score_two(self.width, pair, self.data, self.scratch)
    }
}

/// Given a provisional first gate (chosen TRAIN-only by A6-C7), inspect
/// every distinct second gate position, raw channel and full 4-bit table.
fn conditional(
    first: IntermediateRule,
    ctx: &mut TrainContext<'_>,
) -> Result<([IntermediateRule; 2], u64), NonlinearError> {
    let mut best: Option<([IntermediateRule; 2], u64)> = None;
    for gate in 1..ctx.width {
        if gate == first.gate {
            continue;
        }
        for raw in A6C9_RAW_CHOICES {
            for truth in 0..16u8 {
                let next = IntermediateRule { gate, raw, truth };
                let pair = sorted_pair(ctx.width, first, next)?;
                let loss = ctx.errors(pair);
                if trained_better(pair, loss, best) {
                    best = Some((pair, loss));
                }
            }
        }
    }
    best.ok_or(NonlinearError::InvalidData)
}

/// One preregistered complete coordinate pass (both hidden gate axes), with
/// only STRICT TRAIN error improvements accepted. No validation labels enter.
fn coordinate_pass(
    mut pair: [IntermediateRule; 2],
    mut current_loss: u64,
    ctx: &mut TrainContext<'_>,
) -> Result<([IntermediateRule; 2], u64, usize), NonlinearError> {
    let mut commits = 0usize;
    for axis in 0..2 {
        let fixed = pair[1 - axis];
        let mut best: Option<([IntermediateRule; 2], u64)> = None;
        for gate in 1..ctx.width {
            if gate == fixed.gate {
                continue;
            }
            for raw in A6C9_RAW_CHOICES {
                for truth in 0..16u8 {
                    let candidate = IntermediateRule { gate, raw, truth };
                    let other = sorted_pair(ctx.width, fixed, candidate)?;
                    let loss = ctx.errors(other);
                    if trained_better(other, loss, best) {
                        best = Some((other, loss));
                    }
                }
            }
        }
        if let Some((next, loss)) = best {
            if loss < current_loss {
                pair = next;
                current_loss = loss;
                commits += 1;
            }
        }
    }
    Ok((pair, current_loss, commits))
}

/// Experimental terminal-only nonlinear conditional learner. Both A6-C7 and
/// A6-C8 are fitted on exactly the same independent train/validation streams
/// as fair content controls (NOT equal budgets). Their TRAIN proposals are
/// used as anchors; validation does not tune gate parameters.
///
/// Candidate selection across 0,1,affine2,general2 networks happens only
/// after train search is frozen. No test dataset enters this API.
pub fn fit_nonlinear_credit(
    train: &LaneData,
    validation: &LaneData,
    width: usize,
) -> Result<NonlinearReport, NonlinearError> {
    let train_samples = validate(width, train, true)?;
    let validation_samples = validate(width, validation, false)?;
    let old_one = fit_intermediate_gate(train, validation, width)?;
    let old_affine = fit_dual_hidden_credit(train, validation, width)?;
    let baseline_train_errors = old_one.baseline_train_errors;
    let baseline_validation_errors = old_one.baseline_validation_errors;
    if baseline_train_errors != old_affine.baseline_train_errors
        || baseline_validation_errors != old_affine.baseline_validation_errors
    {
        return Err(NonlinearError::NativeOracle);
    }

    let mut scratch = vec![0u64; width];
    let mut conditional_evaluations = 0usize;
    let mut conditional_best: Option<([IntermediateRule; 2], u64)> = None;
    let mut ctx = TrainContext {
        width,
        data: train,
        scratch: &mut scratch,
        evaluations: &mut conditional_evaluations,
    };
    if let Some(anchor) = old_one.proposed_rule {
        let (pair, loss) = conditional(anchor, &mut ctx)?;
        conditional_best = Some((pair, loss));
    }
    // The A6-C8 joint TRAIN proposal handles 2 XOR/XNOR with no measurable
    // marginal single-gate credit. It is never learned from held-out data.
    let affine_pair = sorted_pair(width, old_affine.joint_rules[0], old_affine.joint_rules[1])?;
    let affine_score = ctx.errors(affine_pair);
    if affine_score != old_affine.joint_train_errors {
        return Err(NonlinearError::NativeOracle);
    }
    if trained_better(affine_pair, affine_score, conditional_best) {
        conditional_best = Some((affine_pair, affine_score));
    }
    let (initial_pair, initial_loss) = conditional_best.ok_or(NonlinearError::InvalidData)?;

    let before = *ctx.evaluations;
    let (pair, loss, committed_updates) = coordinate_pass(initial_pair, initial_loss, &mut ctx)?;
    let coordinate_evaluations = *ctx.evaluations - before;
    if committed_updates > A6C9_COORDINATE_PASSES * 2 {
        return Err(NonlinearError::NativeOracle);
    }
    let conditional_proposed = make_multi_gate_native(width, &pair)?;
    if score_two(width, pair, train, &mut scratch) != loss
        || native_dual_errors(&conditional_proposed, train)? != loss
    {
        return Err(NonlinearError::NativeOracle);
    }
    let conditional_validation_errors = native_dual_errors(&conditional_proposed, validation)?;
    let single_training = old_one.proposed_train_errors;
    let single_validation = old_one.proposed_validation_errors;
    let affine_training = old_affine.joint_train_errors;
    let affine_validation = old_affine.joint_validation_errors;

    // Every gate choice above has been locked from TRAIN. The held-out
    // validation compares just these four whole models; choose the lowest
    // validation loss, preferring fewer gates and then affine on pair ties.
    let mut selected_origin = CreditOrigin::Identity;
    let mut selected_count = 0usize;
    let mut selected_loss = baseline_validation_errors;
    let proposals = [
        (CreditOrigin::OneGate, 1usize, single_validation),
        (CreditOrigin::AffinePair, 2usize, affine_validation),
        (
            CreditOrigin::NonlinearPair,
            2usize,
            conditional_validation_errors,
        ),
    ];
    for (kind, count, val) in proposals {
        if (val, count) < (selected_loss, selected_count) {
            selected_origin = kind;
            selected_count = count;
            selected_loss = val;
        }
    }
    let selected_rules = match selected_origin {
        CreditOrigin::Identity => Vec::new(),
        CreditOrigin::OneGate => old_one.proposed_rule.into_iter().collect(),
        CreditOrigin::AffinePair => affine_pair.to_vec(),
        CreditOrigin::NonlinearPair => pair.to_vec(),
    };
    let selected = match selected_origin {
        CreditOrigin::Identity => old_one.reference.clone(),
        CreditOrigin::OneGate => old_one.proposed.clone(),
        CreditOrigin::AffinePair => old_affine.joint_proposed.clone(),
        CreditOrigin::NonlinearPair => conditional_proposed.clone(),
    };
    let selected_train_errors = match selected_origin {
        CreditOrigin::Identity => baseline_train_errors,
        CreditOrigin::OneGate => single_training,
        CreditOrigin::AffinePair => affine_training,
        CreditOrigin::NonlinearPair => loss,
    };
    if native_dual_errors(&selected, train)? != selected_train_errors
        || native_dual_errors(&selected, validation)? != selected_loss
    {
        return Err(NonlinearError::NativeOracle);
    }
    Ok(NonlinearReport {
        width,
        old_one,
        old_affine,
        conditional_rules: Some(pair),
        conditional_proposed: Some(conditional_proposed),
        selected,
        selected_rules,
        selected_origin,
        selected_count,
        baseline_train_errors,
        baseline_validation_errors,
        conditional_train_errors: Some(loss),
        conditional_validation_errors: Some(conditional_validation_errors),
        selected_train_errors,
        selected_validation_errors: selected_loss,
        train_samples,
        validation_samples,
        conditional_evaluations,
        coordinate_evaluations,
        conditional_accepted_updates: committed_updates,
        ring_payload_bytes: width * std::mem::size_of::<u64>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        anf_dual_hidden_credit::make_multi_gate_native, anf_local_incremental::LaneBlock,
        toy_tasks::SplitMix64,
    };

    fn dataset(seed: u64, width: usize, steps: usize, rules: &[IntermediateRule]) -> LaneData {
        let teacher = make_multi_gate_native(width, rules).unwrap();
        let mut rng = SplitMix64::new(seed);
        let mut inputs = vec![0u64; steps * A6C9_INPUT_BITS];
        for word in &mut inputs {
            *word = rng.next_u64();
        }
        let mut state = vec![0u64; teacher.source_width()];
        let mut next = vec![0u64; width];
        let mut scratch = vec![0u64; teacher.scratch_len()];
        let mut targets = vec![0u64; steps];
        for (t, target) in targets.iter_mut().enumerate() {
            let offset = t * A6C9_INPUT_BITS;
            state[width..].copy_from_slice(&inputs[offset..offset + A6C9_INPUT_BITS]);
            teacher.step_lanes(&mut state, &mut next, &mut scratch);
            *target = teacher.readout().evaluate_lanes(&state, &mut scratch);
        }
        LaneData {
            input_bits: A6C9_INPUT_BITS,
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
    fn segmented_circular_score_matches_native_every_binary_truth_pair() {
        let width = 16;
        let mut ring = vec![0u64; width];
        for truth_a in 0..16u8 {
            for truth_b in 0..16u8 {
                let a = IntermediateRule {
                    gate: 4,
                    raw: 2,
                    truth: truth_a,
                };
                let b = IntermediateRule {
                    gate: 11,
                    raw: 3,
                    truth: truth_b,
                };
                let sample = dataset(
                    777 + u64::from(truth_a) * 16 + u64::from(truth_b),
                    width,
                    80,
                    &[a, b],
                );
                let oracle = score_pair(width, [a, b], &sample, &mut ring).unwrap();
                let native =
                    native_dual_errors(&make_multi_gate_native(width, &[a, b]).unwrap(), &sample)
                        .unwrap();
                assert_eq!(oracle, native, "masks {truth_a},{truth_b}");
                assert_eq!(native, 0);
            }
        }
    }

    #[test]
    fn learn_xor_and_pair_from_terminal_labels_only() {
        let width = 32;
        let rules = [
            IntermediateRule {
                gate: 7,
                raw: 2,
                truth: 6,
            },
            IntermediateRule {
                gate: 23,
                raw: 3,
                truth: 8,
            },
        ];
        let train = dataset(101, width, 256, &rules);
        let validation = dataset(102, width, 256, &rules);
        let report = fit_nonlinear_credit(&train, &validation, width).unwrap();
        eprintln!(
            "A6C9_DIAG one_rule={:?} one_train={} one_val={} affine_pair={:?} affine_train={} affine_val={} nonlinear_pair={:?} nonlinear_train={:?} nonlinear_val={:?} selected={:?} selected_count={} selected_train={} selected_val={}",
            report.old_one.proposed_rule, report.old_one.proposed_train_errors,
            report.old_one.proposed_validation_errors, report.old_affine.joint_rules,
            report.old_affine.joint_train_errors, report.old_affine.joint_validation_errors,
            report.conditional_rules, report.conditional_train_errors,
            report.conditional_validation_errors, report.selected_rules,
            report.selected_count, report.selected_train_errors,
            report.selected_validation_errors
        );
        assert_eq!(report.selected_count, 2);
        assert_eq!(report.selected_train_errors, 0);
        assert_eq!(report.selected_validation_errors, 0);
        let test = dataset(103, width, 512, &rules);
        assert_eq!(native_dual_errors(&report.selected, &test).unwrap(), 0);
        assert_eq!(
            report.conditional_evaluations,
            second_candidate_count(width).unwrap() + 1 + 2 * second_candidate_count(width).unwrap()
        );
    }

    #[test]
    fn no_mutation_preserves_identity_without_hidden_supervision() {
        let width = 32;
        let train = dataset(104, width, 256, &[]);
        let validation = dataset(105, width, 256, &[]);
        let report = fit_nonlinear_credit(&train, &validation, width).unwrap();
        assert_eq!(report.selected_count, 0);
        assert_eq!(report.selected, report.old_one.reference);
        assert_eq!(report.selected_train_errors, 0);
    }

    #[test]
    fn validation_flip_does_not_rewrite_train_selected_gate_hypotheses() {
        let width = 24;
        let rules = [
            IntermediateRule {
                gate: 4,
                raw: 2,
                truth: 6,
            },
            IntermediateRule {
                gate: 16,
                raw: 3,
                truth: 8,
            },
        ];
        let train = dataset(107, width, 160, &rules);
        let val = dataset(108, width, 160, &rules);
        let a = fit_nonlinear_credit(&train, &val, width).unwrap();
        let mut flipped = val.clone();
        for block in &mut flipped.blocks {
            for label in &mut block.targets {
                *label = !*label;
            }
        }
        let b = fit_nonlinear_credit(&train, &flipped, width).unwrap();
        assert_eq!(a.conditional_rules, b.conditional_rules);
        assert_eq!(a.conditional_train_errors, b.conditional_train_errors);
        assert_eq!(a.old_one.proposed_rule, b.old_one.proposed_rule);
        assert_eq!(a.old_affine.joint_rules, b.old_affine.joint_rules);
    }

    #[test]
    fn rejects_malformed_split_and_duplicate_circuit_slots() {
        let width = 16;
        let data = dataset(109, width, 100, &[]);
        let same = IntermediateRule {
            gate: 5,
            raw: 2,
            truth: 6,
        };
        assert_eq!(
            sorted_pair(width, same, same),
            Err(NonlinearError::InvalidRule)
        );
        let mut malformed = data.clone();
        malformed.blocks[0].targets.clear();
        assert_eq!(
            fit_nonlinear_credit(&malformed, &data, width),
            Err(NonlinearError::InvalidData)
        );
    }
}
