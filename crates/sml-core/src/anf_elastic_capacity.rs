//! A6-C6: elastic ANF recurrent state widths through 256 Boolean bits.
//!
//! Fixed delay-line scaffold with terminal-only learning of three decisions:
//! (1) which previous state bit feeds back into the root gate,
//! (2) which current raw input enters that gate, and
//! (3) all four entries of its binary Boolean truth table.
//!
//! Each width W has W native GF(2) ANF gates: root gate fan-in 2, plus
//! W-1 exact unary identity delay gates. This restricted prior prevents the
//! exponential 4^(W-1) search of all possible delay-gate truth tables.
//! No intermediate state targets, future observations, CUDA dependency,
//! fixed-size u256 integer, or test data is used during training.
//!
//! All candidate feedback taps for widths [4,8,...,256] share ONE train-only
//! sweep: 256 taps * 4 raw choices * 16 binary tables = 16,384 unique
//! scored candidates. Width-specific winners are cumulative train minima.
//! The smallest width with minimum validation loss is accepted if and only
//! if it strictly beats the frozen baseline y_hat(t)=x0(t).
//!
//! Training scorer uses a dynamic u64 circular buffer, reused without
//! allocation between candidates. Every selected width winner is also
//! simulated with the real LocalAnfNetwork as an independent ANF oracle.
//! Held-out experiments call a separate streaming native evaluator.

use std::fmt;

use crate::{
    anf_local::{LocalAnfError, LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::LaneData,
};

pub const ELASTIC_MAX_WIDTH: usize = 256;
pub const ELASTIC_INPUT_BITS: usize = 5;
pub const ELASTIC_RAW_CHOICES: usize = 4;
pub const ELASTIC_MAX_TRAIN_STEPS: usize = 768;
pub const ELASTIC_REGISTERED_WIDTHS: [usize; 7] = [4, 8, 16, 32, 64, 128, 256];
pub const ELASTIC_UNIQUE_CANDIDATES: usize = ELASTIC_MAX_WIDTH * ELASTIC_RAW_CHOICES * 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElasticPhaseError {
    Network(LocalAnfError),
    InvalidSpec,
    InvalidData,
    InvalidWidths,
    NativeOracle,
}

impl fmt::Display for ElasticPhaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ElasticPhaseError {}
impl From<LocalAnfError> for ElasticPhaseError {
    fn from(value: LocalAnfError) -> Self {
        Self::Network(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElasticPhaseSpec {
    pub raw_candidates: Vec<usize>,
    pub baseline_input: usize,
    pub target_bit: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElasticPhaseRule {
    /// Number of logically allocated state bits (actual native gate count).
    pub width: usize,
    /// Index of previous state word read by the root ANF gate.
    pub tap: usize,
    /// Physical current input channel, excluding baseline.
    pub raw: usize,
    /// Low four truth-table bits indexed by address = state + 2*input.
    pub truth: u8,
}

impl ElasticPhaseRule {
    pub fn active_memory_bits(self) -> usize {
        if truth_uses_previous_state(self.truth) {
            self.tap + 1
        } else {
            0
        }
    }
    pub fn root_monomials(self) -> usize {
        let [a, b, c, d] = [0, 1, 2, 3].map(|i| self.truth & (1 << i) != 0);
        usize::from(a) + usize::from(a ^ b) + usize::from(a ^ c) + usize::from(a ^ b ^ c ^ d)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElasticWidthFit {
    pub rule: ElasticPhaseRule,
    pub network: LocalAnfNetwork,
    pub train_errors: u64,
    pub validation_errors: u64,
    pub admissible_candidates: usize,
    pub logical_state_bytes_64_lanes: usize,
    pub native_anf_coefficient_bits: usize,
    pub native_bits_read_per_step: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElasticPhaseReport {
    pub families: Vec<ElasticWidthFit>,
    pub selected: LocalAnfNetwork,
    pub selected_rule: ElasticPhaseRule,
    /// Zero indicates no width accepted over the reference prediction x0.
    pub selected_width: usize,
    pub selected_active_memory_bits: usize,
    pub accepted: bool,
    pub baseline_train_errors: u64,
    pub baseline_validation_errors: u64,
    pub selected_train_errors: u64,
    pub selected_validation_errors: u64,
    pub train_samples: u64,
    pub validation_samples: u64,
    /// The sweep counts every unique (tap,raw,truth) exactly once.
    pub unique_train_evaluations: usize,
    /// Allocated and reused u64 state payload during scoring, not total RAM.
    pub search_ring_payload_bytes: usize,
}

fn validate(
    data: &LaneData,
    spec: &ElasticPhaseSpec,
    capped_train: bool,
) -> Result<u64, ElasticPhaseError> {
    if data.input_bits != ELASTIC_INPUT_BITS
        || spec.target_bit >= data.target_bits
        || spec.baseline_input >= data.input_bits
        || spec.raw_candidates.len() != ELASTIC_RAW_CHOICES
        || spec.raw_candidates.contains(&spec.baseline_input)
        || data.blocks.is_empty()
    {
        return Err(ElasticPhaseError::InvalidSpec);
    }
    for (j, &raw) in spec.raw_candidates.iter().enumerate() {
        if raw >= ELASTIC_INPUT_BITS || spec.raw_candidates[..j].contains(&raw) {
            return Err(ElasticPhaseError::InvalidSpec);
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
            return Err(ElasticPhaseError::InvalidData);
        }
        steps = steps
            .checked_add(block.steps)
            .ok_or(ElasticPhaseError::InvalidData)?;
        samples += u64::from(block.valid.count_ones()) * block.steps as u64;
    }
    if capped_train && steps > ELASTIC_MAX_TRAIN_STEPS {
        return Err(ElasticPhaseError::InvalidData);
    }
    Ok(samples)
}

fn check_widths(widths: &[usize]) -> Result<(), ElasticPhaseError> {
    if widths.is_empty()
        || widths.len() > ELASTIC_MAX_WIDTH
        || widths.iter().any(|&w| w == 0 || w > ELASTIC_MAX_WIDTH)
        || widths.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(ElasticPhaseError::InvalidWidths);
    }
    Ok(())
}

#[inline]
fn apply_truth(mask: u8, state: u64, raw: u64) -> u64 {
    let mut out = 0u64;
    if mask & 1 != 0 {
        out |= !state & !raw;
    }
    if mask & 2 != 0 {
        out |= state & !raw;
    }
    if mask & 4 != 0 {
        out |= !state & raw;
    }
    if mask & 8 != 0 {
        out |= state & raw;
    }
    out
}
/// Independent scored bit-sliced reference used for post-freeze evaluation.
pub fn score_rule(
    rule: ElasticPhaseRule,
    data: &LaneData,
    spec: &ElasticPhaseSpec,
    ring: &mut [u64],
) -> Result<u64, ElasticPhaseError> {
    validate(data, spec, false)?;
    if rule.width == 0
        || rule.width > ELASTIC_MAX_WIDTH
        || rule.tap >= rule.width
        || rule.truth >= 16
        || ring.len() < rule.width
        || !spec.raw_candidates.contains(&rule.raw)
    {
        return Err(ElasticPhaseError::InvalidSpec);
    }
    Ok(score_ring(rule, data, spec, ring))
}

fn truth_uses_previous_state(mask: u8) -> bool {
    (mask & 1 != 0) != (mask & 2 != 0) || (mask & 4 != 0) != (mask & 8 != 0)
}

/// Exact O(T) candidate score: root reads its (tap+1)-step delayed output.
/// The source buffer holds a whole maximum-width elastic ring; each block is
/// reset to zeros, preventing sequence cross-contamination.
fn score_ring(
    rule: ElasticPhaseRule,
    data: &LaneData,
    spec: &ElasticPhaseSpec,
    ring: &mut [u64],
) -> u64 {
    debug_assert!(ring.len() >= rule.width);
    let lag = rule.tap + 1;
    let mut errors = 0u64;
    for block in &data.blocks {
        ring.fill(0);
        for t in 0..block.steps {
            let slot = t % lag;
            let old = ring[slot];
            let next = apply_truth(
                rule.truth,
                old,
                block.inputs[t * data.input_bits + rule.raw],
            );
            ring[slot] = next;
            let residual = block.targets[t * data.target_bits + spec.target_bit]
                ^ block.inputs[t * data.input_bits + spec.baseline_input];
            errors += u64::from(((residual ^ next) & block.valid).count_ones());
        }
    }
    errors
}

/// Tie-breaking is determined exclusively by training errors, then minimum
/// active memory and ANF terms, followed by deterministic source/truth order.
fn train_better(rule: ElasticPhaseRule, loss: u64, old: Option<(ElasticPhaseRule, u64)>) -> bool {
    old.is_none_or(|(prior, old_loss)| {
        (
            loss,
            rule.active_memory_bits(),
            rule.root_monomials(),
            rule.tap,
            rule.raw,
            rule.truth,
        ) < (
            old_loss,
            prior.active_memory_bits(),
            prior.root_monomials(),
            prior.tap,
            prior.raw,
            prior.truth,
        )
    })
}

pub fn candidates_for_width(width: usize) -> Option<usize> {
    if (1..=ELASTIC_MAX_WIDTH).contains(&width) {
        Some(width * ELASTIC_RAW_CHOICES * 16)
    } else {
        None
    }
}

pub fn make_native(
    rule: ElasticPhaseRule,
    spec: &ElasticPhaseSpec,
) -> Result<LocalAnfNetwork, ElasticPhaseError> {
    if rule.width == 0
        || rule.width > ELASTIC_MAX_WIDTH
        || rule.tap >= rule.width
        || rule.raw >= ELASTIC_INPUT_BITS
        || rule.truth >= 16
        || !spec.raw_candidates.contains(&rule.raw)
    {
        return Err(ElasticPhaseError::InvalidSpec);
    }
    let source_width = rule.width + ELASTIC_INPUT_BITS;
    let root: [bool; 4] = std::array::from_fn(|i| rule.truth & (1 << i) != 0);
    let mut gates = Vec::with_capacity(rule.width);
    gates.push(LocalAnfGate::from_truth_table(
        vec![rule.tap, rule.width + rule.raw],
        &root,
        source_width,
    )?);
    for i in 1..rule.width {
        gates.push(LocalAnfGate::from_truth_table(
            vec![i - 1],
            &[false, true],
            source_width,
        )?);
    }
    let readout = LocalAnfGate::from_truth_table(
        vec![0, rule.width + spec.baseline_input],
        &[false, true, true, false],
        source_width,
    )?;
    Ok(LocalAnfNetwork::with_input_bits(
        rule.width,
        ELASTIC_INPUT_BITS,
        gates,
        readout,
    )?)
}

/// Streaming exact native execution: O(width) state plus O(width) scratch,
/// without materializing a width-by-time tensor. Reports bit-lane errors.
pub fn native_errors(
    model: &LocalAnfNetwork,
    data: &LaneData,
    spec: &ElasticPhaseSpec,
) -> Result<u64, ElasticPhaseError> {
    let total = validate(data, spec, false)?;
    if model.input_bits() != ELASTIC_INPUT_BITS
        || model.state_bits() == 0
        || model.state_bits() > ELASTIC_MAX_WIDTH
    {
        return Err(ElasticPhaseError::InvalidSpec);
    }
    let n = model.state_bits();
    let mut source = vec![0u64; model.source_width()];
    let mut next = vec![0u64; n];
    let mut products = vec![0u64; model.scratch_len()];
    let mut errors = 0u64;
    for block in &data.blocks {
        source.fill(0);
        for t in 0..block.steps {
            let begin = t * ELASTIC_INPUT_BITS;
            source[n..].copy_from_slice(&block.inputs[begin..begin + ELASTIC_INPUT_BITS]);
            model.step_lanes(&mut source, &mut next, &mut products);
            let predicted = model.readout().evaluate_lanes(&source, &mut products);
            let observed = block.targets[t * data.target_bits + spec.target_bit];
            errors += u64::from(((predicted ^ observed) & block.valid).count_ones());
        }
    }
    debug_assert!(errors <= total);
    Ok(errors)
}

pub fn reference_errors(data: &LaneData, spec: &ElasticPhaseSpec) -> u64 {
    data.blocks
        .iter()
        .map(|block| {
            (0..block.steps)
                .map(|t| {
                    let observed_residual = block.targets[t * data.target_bits + spec.target_bit]
                        ^ block.inputs[t * data.input_bits + spec.baseline_input];
                    u64::from((observed_residual & block.valid).count_ones())
                })
                .sum::<u64>()
        })
        .sum()
}

fn zero_model(spec: &ElasticPhaseSpec, width: usize) -> Result<LocalAnfNetwork, ElasticPhaseError> {
    make_native(
        ElasticPhaseRule {
            width,
            tap: 0,
            raw: spec.raw_candidates[0],
            truth: 0,
        },
        spec,
    )
}

/// One train-only prefix sweep serves every width in the caller-supplied
/// ascending family. Validation may select a family but NEVER adjust a gate.
/// There is intentionally no test parameter in this training function.
pub fn fit_elastic_phase(
    train: &LaneData,
    validation: &LaneData,
    spec: &ElasticPhaseSpec,
    widths: &[usize],
) -> Result<ElasticPhaseReport, ElasticPhaseError> {
    check_widths(widths)?;
    let train_samples = validate(train, spec, true)?;
    let validation_samples = validate(validation, spec, false)?;
    let baseline_train_errors = reference_errors(train, spec);
    let baseline_validation_errors = reference_errors(validation, spec);

    let max_width = *widths.last().ok_or(ElasticPhaseError::InvalidWidths)?;
    let mut ring = vec![0u64; max_width];
    let mut minima: Vec<Option<(ElasticPhaseRule, u64)>> = vec![None; widths.len()];
    let mut evaluations = 0usize;

    // Each candidate is evaluated exactly ONCE and offered to every width
    // that can physically realize its feedback tap.
    for tap in 0..max_width {
        for &raw in &spec.raw_candidates {
            for truth in 0..16u8 {
                let candidate = ElasticPhaseRule {
                    width: max_width,
                    tap,
                    raw,
                    truth,
                };
                let loss = score_ring(candidate, train, spec, &mut ring);
                evaluations += 1;
                for (i, &width) in widths.iter().enumerate() {
                    if tap < width {
                        let candidate_for_width = ElasticPhaseRule { width, ..candidate };
                        if train_better(candidate_for_width, loss, minima[i]) {
                            minima[i] = Some((candidate_for_width, loss));
                        }
                    }
                }
            }
        }
    }
    if Some(evaluations) != candidates_for_width(max_width) {
        return Err(ElasticPhaseError::InvalidWidths);
    }

    let mut families = Vec::with_capacity(widths.len());
    for (i, &width) in widths.iter().enumerate() {
        let (rule, train_errors) = minima[i].ok_or(ElasticPhaseError::InvalidWidths)?;
        let network = make_native(rule, spec)?;
        if native_errors(&network, train, spec)? != train_errors {
            return Err(ElasticPhaseError::NativeOracle);
        }
        let validation_errors = native_errors(&network, validation, spec)?;
        families.push(ElasticWidthFit {
            rule,
            native_anf_coefficient_bits: network.learned_bits(),
            native_bits_read_per_step: network.bits_read_per_token(),
            logical_state_bytes_64_lanes: width * std::mem::size_of::<u64>(),
            network,
            train_errors,
            validation_errors,
            admissible_candidates: candidates_for_width(width)
                .ok_or(ElasticPhaseError::InvalidWidths)?,
        });
    }

    let mut selected_index = 0usize;
    for index in 1..families.len() {
        let prior = &families[selected_index];
        let candidate = &families[index];
        if (candidate.validation_errors, candidate.rule.width)
            < (prior.validation_errors, prior.rule.width)
        {
            selected_index = index;
        }
    }
    let chosen = &families[selected_index];
    let accepted = chosen.validation_errors < baseline_validation_errors;
    Ok(ElasticPhaseReport {
        selected: if accepted {
            chosen.network.clone()
        } else {
            zero_model(spec, widths[0])?
        },
        selected_rule: chosen.rule,
        selected_width: if accepted { chosen.rule.width } else { 0 },
        selected_active_memory_bits: if accepted {
            chosen.rule.active_memory_bits()
        } else {
            0
        },
        accepted,
        baseline_train_errors,
        baseline_validation_errors,
        selected_train_errors: chosen.train_errors,
        selected_validation_errors: chosen.validation_errors,
        train_samples,
        validation_samples,
        unique_train_evaluations: evaluations,
        search_ring_payload_bytes: max_width * std::mem::size_of::<u64>(),
        families,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        anf_local_incremental::LaneBlock, semantic::bitfield::SemanticBitField,
        toy_tasks::SplitMix64,
    };

    fn spec() -> ElasticPhaseSpec {
        ElasticPhaseSpec {
            raw_candidates: vec![1, 2, 3, 4],
            baseline_input: 0,
            target_bit: 0,
        }
    }
    fn dataset(seed: u64, steps: usize, stride: usize) -> LaneData {
        let mut rng = SplitMix64::new(seed);
        let mut inputs = vec![0u64; steps * ELASTIC_INPUT_BITS];
        for word in &mut inputs {
            *word = rng.next_u64();
        }
        let mut phases = vec![0u64; stride.max(1)];
        let mut targets = vec![0u64; steps];
        for (t, target) in targets.iter_mut().enumerate() {
            let raw = inputs[t * ELASTIC_INPUT_BITS + 1];
            let residual = if stride == 0 {
                raw
            } else {
                let position = t % stride;
                phases[position] ^= raw;
                phases[position]
            };
            *target = inputs[t * ELASTIC_INPUT_BITS] ^ residual;
        }
        LaneData {
            input_bits: ELASTIC_INPUT_BITS,
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
    fn packed_semantic_bitfield_covers_128_and_256_boundaries() {
        let mut field128 = SemanticBitField::<128>::zeroed().unwrap();
        for bit in [0, 63, 64, 127] {
            field128.set(bit, true).unwrap();
        }
        assert_eq!(field128.count_ones(), 4);
        assert_eq!(field128.words().len(), 2);
        let mut field256 = SemanticBitField::<256>::zeroed().unwrap();
        for bit in [0, 63, 64, 127, 128, 255] {
            field256.set(bit, true).unwrap();
            assert!(field256.get(bit).unwrap());
        }
        assert_eq!(field256.words().len(), 4);
        assert_eq!(field256.count_ones(), 6);
    }

    #[test]
    fn truth_table_and_mobius_anf_agree_across_16_masks() {
        for truth in 0..16u8 {
            let rule = ElasticPhaseRule {
                width: 4,
                tap: 3,
                raw: 1,
                truth,
            };
            let model = make_native(rule, &spec()).unwrap();
            let mut products = [0u64; 4];
            for state in [0u64, u64::MAX] {
                for raw in [0u64, u64::MAX] {
                    let values = [state, raw];
                    let observed = model.gates()[0].evaluate_gathered(&values, &mut products);
                    assert_eq!(apply_truth(truth, state, raw), observed);
                }
            }
        }
    }

    #[test]
    fn training_recovers_sixteen_and_thirty_two_bit_parities() {
        let widths = [4, 8, 16, 32];
        for (stride, expected) in [(16, 16), (32, 32)] {
            let training = dataset(70 + stride as u64, 128, stride);
            let validation = dataset(80 + stride as u64, 128, stride);
            let fitted = fit_elastic_phase(&training, &validation, &spec(), &widths).unwrap();
            assert!(fitted.accepted);
            assert_eq!(fitted.selected_width, expected);
            assert_eq!(fitted.selected_active_memory_bits, expected);
            assert_eq!(fitted.selected_rule.truth, 6);
            assert_eq!(fitted.selected_rule.raw, 1);
            assert_eq!(fitted.selected_train_errors, 0);
            assert_eq!(fitted.selected_validation_errors, 0);
            assert_eq!(
                native_errors(&fitted.selected, &dataset(99, 2048, stride), &spec()).unwrap(),
                0
            );
        }
    }

    #[test]
    fn full_width_256_elastic_state_is_bit_exact_and_reused() {
        let rule = ElasticPhaseRule {
            width: 256,
            tap: 255,
            raw: 1,
            truth: 6,
        };
        let model = make_native(rule, &spec()).unwrap();
        assert_eq!(model.state_bits(), 256);
        assert_eq!(model.gates().len(), 256);
        let test = dataset(178, 520, 256);
        let mut scratch_ring = vec![0u64; 256];
        assert_eq!(score_ring(rule, &test, &spec(), &mut scratch_ring), 0);
        assert_eq!(native_errors(&model, &test, &spec()).unwrap(), 0);
        assert_eq!(model.readout().fan_in(), 2);
        assert_eq!(model.gates()[255].fan_in(), 1);
    }

    #[test]
    fn unsupported_width_families_or_malformed_splits_are_rejected() {
        let train = dataset(15, 32, 4);
        let val = dataset(16, 32, 4);
        for widths in [
            &[][..],
            &[0, 4][..],
            &[16, 8][..],
            &[8, 8][..],
            &[256, 257][..],
        ] {
            assert_eq!(
                fit_elastic_phase(&train, &val, &spec(), widths),
                Err(ElasticPhaseError::InvalidWidths),
            );
        }
        let mut malformed = train.clone();
        malformed.blocks[0].targets.clear();
        assert_eq!(
            fit_elastic_phase(&malformed, &val, &spec(), &[4]),
            Err(ElasticPhaseError::InvalidData),
        );
    }
}
