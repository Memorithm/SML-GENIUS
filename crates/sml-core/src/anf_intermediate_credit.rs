//! A6-C7: learn ONE previously unknown intermediate ANF state gate from the
//! final observed target in a 128/256-bit recurrent delay scaffold.
//!
//! Known, FROZEN backbone (N state bits, 5 raw input bits):
//!   s0(t+1) = s_(N-1)(t) XOR x1(t)
//!   si(t+1) = s_(i-1)(t)             for i != unknown j
//!   sj(t+1) = f(s_(j-1)(t),x_r(t))   for unknown j in 1..N-1
//!   y(t) = s0(t+1) XOR x0(t)
//!
//! The learner selects j, the raw input r and a FULL 2-input Boolean truth
//! table f from training-only FINAL y labels. No intermediate supervision,
//! future input, ground-truth gate label or test data is used by this API.
//! The identity backbone is preferred if no strict train reduction exists;
//! a trained mutation commits only after strict validation improvement.
//!
//! A segmented u64 ring is an EXACT O(T) evaluator for each candidate,
//! reusing N words across all N*4*16 hypotheses without per-candidate
//! allocation. The selected proposal is always checked against the actual
//! native GF(2) LocalAnfNetwork on both train and validation.
//!
//! NOTE: This trains ONE internal gate in a preconfigured delay scaffold.
//! It is not free graph discovery nor training of 256 independent ANF gates.

use std::fmt;

use crate::{
    anf_elastic_capacity::{native_errors as native_anf_errors, ElasticPhaseError, ElasticPhaseSpec},
    anf_local::{LocalAnfError, LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::LaneData,
};

pub const A6C7_MAX_WIDTH: usize = 256;
pub const A6C7_INPUT_BITS: usize = 5;
pub const A6C7_TRAIN_MAX_STEPS: usize = 768;
pub const A6C7_ROOT_INPUT: usize = 1;
pub const A6C7_BASELINE_INPUT: usize = 0;
pub const A6C7_RAW_CANDIDATES: [usize; 4] = [1, 2, 3, 4];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntermediateError {
    Network(LocalAnfError),
    Oracle(ElasticPhaseError),
    InvalidWidth,
    InvalidSpec,
    InvalidData,
    ScoreMismatch,
}

impl fmt::Display for IntermediateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for IntermediateError {}
impl From<LocalAnfError> for IntermediateError {
    fn from(err: LocalAnfError) -> Self { Self::Network(err) }
}
impl From<ElasticPhaseError> for IntermediateError {
    fn from(err: ElasticPhaseError) -> Self { Self::Oracle(err) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntermediateRule {
    /// Unknown hidden gate position; the root (0) is NOT trainable here.
    pub gate: usize,
    /// Selected current raw channel.
    pub raw: usize,
    /// Binary truth mask indexed by previous_state + 2*raw.
    pub truth: u8,
}
impl IntermediateRule {
    pub fn truth_is_identity(self) -> bool { self.truth == 10 }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntermediateReport {
    pub width: usize,
    /// Train-selected candidate, even when validation rejects its adoption.
    pub proposed_rule: Option<IntermediateRule>,
    pub proposed: LocalAnfNetwork,
    /// Validation-accepted circuit, otherwise immutable identity reference.
    pub committed: LocalAnfNetwork,
    pub reference: LocalAnfNetwork,
    pub accepted: bool,
    pub baseline_train_errors: u64,
    pub baseline_validation_errors: u64,
    pub proposed_train_errors: u64,
    pub proposed_validation_errors: u64,
    pub train_samples: u64,
    pub validation_samples: u64,
    pub candidate_evaluations: usize,
    /// Reused bit-sliced state ring payload, excluding all Vec metadata.
    pub ring_payload_bytes: usize,
}

fn to_spec() -> ElasticPhaseSpec {
    ElasticPhaseSpec {
        raw_candidates: A6C7_RAW_CANDIDATES.to_vec(),
        baseline_input: A6C7_BASELINE_INPUT,
        target_bit: 0,
    }
}

fn validate_data(data: &LaneData, capped: bool) -> Result<u64, IntermediateError> {
    if data.input_bits != A6C7_INPUT_BITS || data.target_bits != 1
        || data.blocks.is_empty()
    {
        return Err(IntermediateError::InvalidData);
    }
    let mut steps = 0usize;
    let mut samples = 0u64;
    for block in &data.blocks {
        if block.steps == 0 || block.valid == 0
            || block.inputs.len() != block.steps * data.input_bits
            || block.targets.len() != block.steps * data.target_bits
        {
            return Err(IntermediateError::InvalidData);
        }
        steps = steps.checked_add(block.steps).ok_or(IntermediateError::InvalidData)?;
        samples += u64::from(block.valid.count_ones()) * block.steps as u64;
    }
    if capped && steps > A6C7_TRAIN_MAX_STEPS {
        return Err(IntermediateError::InvalidData);
    }
    Ok(samples)
}

pub fn candidate_count(width: usize) -> Option<usize> {
    if (2..=A6C7_MAX_WIDTH).contains(&width) {
        Some((width - 1) * A6C7_RAW_CANDIDATES.len() * 16)
    } else {
        None
    }
}

fn validate_rule(width: usize, rule: IntermediateRule) -> Result<(), IntermediateError> {
    if width < 2 || width > A6C7_MAX_WIDTH {
        return Err(IntermediateError::InvalidWidth);
    }
    if rule.gate == 0 || rule.gate >= width
        || !A6C7_RAW_CANDIDATES.contains(&rule.raw)
        || rule.truth >= 16
    {
        return Err(IntermediateError::InvalidSpec);
    }
    Ok(())
}

#[inline]
fn apply_truth(mask: u8, state: u64, raw: u64) -> u64 {
    let mut out = 0u64;
    if mask & 1 != 0 { out |= !state & !raw; }
    if mask & 2 != 0 { out |= state & !raw; }
    if mask & 4 != 0 { out |= !state & raw; }
    if mask & 8 != 0 { out |= state & raw; }
    out
}

/// Bit-exact delayed ring. Prefix length=gate, suffix length=width-gate.
/// Prefix queues root outputs until the free gate receives them.
/// Suffix queues free-gate outputs until the root receives them.
fn score_candidate(
    width: usize, rule: IntermediateRule,
    data: &LaneData, ring: &mut [u64],
) -> u64 {
    debug_assert!(rule.gate > 0 && rule.gate < width && ring.len() >= width);
    let prefix_len = rule.gate;
    let suffix_len = width - rule.gate;
    let mut errors = 0u64;
    for block in &data.blocks {
        ring.fill(0);
        for t in 0..block.steps {
            let a = t % prefix_len;
            let b = prefix_len + t % suffix_len;
            let root = ring[b] ^ block.inputs[t * A6C7_INPUT_BITS + A6C7_ROOT_INPUT];
            let hidden = apply_truth(
                rule.truth, ring[a],
                block.inputs[t * A6C7_INPUT_BITS + rule.raw],
            );
            ring[a] = root;
            ring[b] = hidden;
            let residual = block.targets[t] ^ block.inputs[t * A6C7_INPUT_BITS];
            errors += u64::from(((residual ^ root) & block.valid).count_ones());
        }
    }
    errors
}

fn score_identity(width: usize, data: &LaneData, ring: &mut [u64]) -> u64 {
    debug_assert!(ring.len() >= width);
    let mut errors = 0u64;
    for block in &data.blocks {
        ring.fill(0);
        for t in 0..block.steps {
            let address = t % width;
            let root = ring[address]
                ^ block.inputs[t * A6C7_INPUT_BITS + A6C7_ROOT_INPUT];
            ring[address] = root;
            let residual = block.targets[t] ^ block.inputs[t * A6C7_INPUT_BITS];
            errors += u64::from(((residual ^ root) & block.valid).count_ones());
        }
    }
    errors
}

/// Native ANF network at the ACTUAL width. For an intermediate mutation,
/// exactly one of the width-1 delay gates changes its own source and complete
/// Boolean table. No fake u256 integer, no reduced-width proxy.
pub fn native_model(
    width: usize, rule: Option<IntermediateRule>,
) -> Result<LocalAnfNetwork, IntermediateError> {
    if width < 2 || width > A6C7_MAX_WIDTH {
        return Err(IntermediateError::InvalidWidth);
    }
    if let Some(candidate) = rule {
        validate_rule(width, candidate)?;
    }
    let source_width = width + A6C7_INPUT_BITS;
    let mut gates = Vec::with_capacity(width);
    gates.push(LocalAnfGate::from_truth_table(
        vec![width - 1, width + A6C7_ROOT_INPUT],
        &[false, true, true, false], source_width,
    )?);
    for i in 1..width {
        let gate = if let Some(hidden) = rule.filter(|hidden| hidden.gate == i) {
            let table: [bool; 4] = std::array::from_fn(|j| hidden.truth & (1 << j) != 0);
            LocalAnfGate::from_truth_table(
                vec![i - 1, width + hidden.raw], &table, source_width,
            )?
        } else {
            LocalAnfGate::from_truth_table(
                vec![i - 1], &[false, true], source_width,
            )?
        };
        gates.push(gate);
    }
    let readout = LocalAnfGate::from_truth_table(
        vec![0, width + A6C7_BASELINE_INPUT],
        &[false, true, true, false], source_width,
    )?;
    Ok(LocalAnfNetwork::with_input_bits(
        width, A6C7_INPUT_BITS, gates, readout,
    )?)
}

/// Reference native streaming evaluator for chosen model.
pub fn native_errors(
    model: &LocalAnfNetwork, data: &LaneData,
) -> Result<u64, IntermediateError> {
    validate_data(data, false)?;
    Ok(native_anf_errors(model, data, &to_spec())?)
}

pub fn score_rule(
    width: usize, candidate: Option<IntermediateRule>,
    data: &LaneData, ring: &mut [u64],
) -> Result<u64, IntermediateError> {
    validate_data(data, false)?;
    if width < 2 || width > A6C7_MAX_WIDTH || ring.len() < width {
        return Err(IntermediateError::InvalidWidth);
    }
    Ok(if let Some(rule) = candidate {
        validate_rule(width, rule)?;
        score_candidate(width, rule, data, ring)
    } else {
        score_identity(width, data, ring)
    })
}

/// Train only the hidden-gate choice and Boolean function; validate once.
/// The identity baseline wins all train ties by construction.
pub fn fit_intermediate_gate(
    train: &LaneData, validation: &LaneData, width: usize,
) -> Result<IntermediateReport, IntermediateError> {
    if width < 2 || width > A6C7_MAX_WIDTH {
        return Err(IntermediateError::InvalidWidth);
    }
    let train_samples = validate_data(train, true)?;
    let validation_samples = validate_data(validation, false)?;
    let mut ring = vec![0u64; width];
    let baseline_train_errors = score_identity(width, train, &mut ring);
    let reference = native_model(width, None)?;
    if native_errors(&reference, train)? != baseline_train_errors {
        return Err(IntermediateError::ScoreMismatch);
    }
    let baseline_validation_errors = native_errors(&reference, validation)?;
    let mut best: Option<(IntermediateRule, u64)> = None;
    let mut candidate_evaluations = 0usize;
    for gate in 1..width {
        for &raw in &A6C7_RAW_CANDIDATES {
            for truth in 0..16u8 {
                let rule = IntermediateRule { gate, raw, truth };
                let loss = score_candidate(width, rule, train, &mut ring);
                candidate_evaluations += 1;
                let superior = best.as_ref().is_none_or(|(prior, old_loss)| {
                    (loss, gate, raw, truth)
                        < (*old_loss, prior.gate, prior.raw, prior.truth)
                });
                if superior {
                    best = Some((rule, loss));
                }
            }
        }
    }
    if Some(candidate_evaluations) != candidate_count(width) {
        return Err(IntermediateError::InvalidSpec);
    }

    // Never add a hidden gate on a training tie or an improvement absent.
    let (proposed_rule, proposed_train_errors) =
        if let Some((rule, errors)) = best.filter(|(_, e)| *e < baseline_train_errors) {
            (Some(rule), errors)
        } else {
            (None, baseline_train_errors)
        };
    let proposed = native_model(width, proposed_rule)?;
    if native_errors(&proposed, train)? != proposed_train_errors {
        return Err(IntermediateError::ScoreMismatch);
    }
    let proposed_validation_errors = native_errors(&proposed, validation)?;
    let accepted = proposed_validation_errors < baseline_validation_errors;
    let committed = if accepted {
        proposed.clone()
    } else {
        reference.clone()
    };
    Ok(IntermediateReport {
        width,
        proposed_rule,
        proposed,
        committed,
        reference,
        accepted,
        baseline_train_errors,
        baseline_validation_errors,
        proposed_train_errors,
        proposed_validation_errors,
        train_samples,
        validation_samples,
        candidate_evaluations,
        ring_payload_bytes: width * std::mem::size_of::<u64>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{anf_local_incremental::LaneBlock, toy_tasks::SplitMix64};

    fn dataset(seed: u64, steps: usize, width: usize, hidden: Option<IntermediateRule>) -> LaneData {
        let model = native_model(width, hidden).unwrap();
        let mut rng = SplitMix64::new(seed);
        let mut inputs = vec![0u64; steps * A6C7_INPUT_BITS];
        for word in &mut inputs {
            *word = rng.next_u64();
        }
        let mut source = vec![0u64; model.source_width()];
        let mut next = vec![0u64; width];
        let mut scratch = vec![0u64; model.scratch_len()];
        let mut targets = vec![0u64; steps];
        for t in 0..steps {
            source[width..].copy_from_slice(
                &inputs[t * A6C7_INPUT_BITS..(t + 1) * A6C7_INPUT_BITS],
            );
            model.step_lanes(&mut source, &mut next, &mut scratch);
            targets[t] = model.readout().evaluate_lanes(&source, &mut scratch);
        }
        LaneData {
            input_bits: A6C7_INPUT_BITS,
            target_bits: 1,
            blocks: vec![LaneBlock { steps, inputs, targets, valid: u64::MAX }],
        }
    }

    #[test]
    fn segmented_ring_matches_native_all_truth_tables_at_multiple_positions() {
        for width in [8, 16] {
            for gate in [1usize, width / 2, width - 1] {
                for truth in 0..16u8 {
                    let rule = IntermediateRule { gate, raw: 2, truth };
                    let data = dataset(19 + truth as u64, 44, width, Some(rule));
                    let mut ring = vec![0u64; width];
                    let specialized = score_candidate(width, rule, &data, &mut ring);
                    let native = native_errors(&native_model(width, Some(rule)).unwrap(), &data).unwrap();
                    assert_eq!(specialized, native);
                    assert_eq!(native, 0);
                }
            }
        }
    }

    #[test]
    fn identity_wins_train_tie_and_does_not_mutate_recurrent_network() {
        let width = 24;
        let train = dataset(50, 128, width, None);
        let val = dataset(51, 64, width, None);
        let report = fit_intermediate_gate(&train, &val, width).unwrap();
        assert!(report.proposed_rule.is_none());
        assert!(!report.accepted);
        assert_eq!(report.proposed, report.reference);
        assert_eq!(report.proposed_train_errors, 0);
        assert_eq!(report.proposed_validation_errors, 0);
    }

    #[test]
    fn learns_midstream_and_deep_xor_sources_without_hidden_labels() {
        for (width, gate) in [(32, 7), (128, 45), (256, 1)] {
            let hidden = IntermediateRule { gate, raw: 2, truth: 6 };
            let train = dataset(81, 512, width, Some(hidden));
            let val = dataset(82, 512, width, Some(hidden));
            let report = fit_intermediate_gate(&train, &val, width).unwrap();
            assert_eq!(report.proposed_rule, Some(hidden));
            assert_eq!(report.proposed_train_errors, 0);
            assert_eq!(report.proposed_validation_errors, 0);
            assert!(report.accepted);
            let test = dataset(83, 1024, width, Some(hidden));
            assert_eq!(native_errors(&report.committed, &test).unwrap(), 0);
        }
    }

    #[test]
    fn rejects_invalid_position_and_malformed_data() {
        let mut ring = [0u64; 8];
        let data = dataset(5, 24, 8, None);
        assert_eq!(candidate_count(128), Some(8_128));
        assert_eq!(candidate_count(256), Some(16_320));
        assert_eq!(
            score_rule(8, Some(IntermediateRule { gate: 8, raw: 2, truth: 6 }), &data, &mut ring),
            Err(IntermediateError::InvalidSpec),
        );
        let mut bad = data.clone();
        bad.blocks[0].targets.clear();
        assert_eq!(
            fit_intermediate_gate(&bad, &data, 8),
            Err(IntermediateError::InvalidData),
        );
    }
}
