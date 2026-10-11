//! A6-C10: joint nonlinear ANF pair system identification from terminal labels.
//!
//! Given the FIXED cyclic N-bit identity-delay scaffold:
//!   root(t) = x1(t) XOR g(f(root(t-N), x_r(t-(N-j))),
//!                         x_q(t-(N-k))),
//! with hidden positions 1 <= j < k < N and binary Boolean tables f,g.
//!
//! The observed training label y(t) reveals the TERMINAL root output
//! root(t) = y(t) XOR x0(t), never either hidden gate output.
//! This gives a teacher-forced conditional three-input truth table on
//! train only; positions and raw channels are ranked JOINTLY by their
//! lower-bound three-variable conditional error (8 empirical addresses).
//! We factorize the best observed three-input functions into ALL 16x16
//! combinations of two native GF(2) ANF gates and finally rank the
//! shortlist using actual AUTONOMOUS recurrence on full TRAIN.
//!
//! Critically, using observed past output labels in train != test leakage.
//! Those labels are available only at earlier TRAIN timesteps; VALIDATION
//! and TEST run native recurrence without teacher forcing, and no TEST
//! data is accepted by the model search API.
//!
//! This is bounded structural identification (fixed root, readout, delay
//! scaffold, at most 2 mutations), NOT general free 256-gate learning.
//! The first probe uses 64 late TRAIN steps, retains 64 structures, and
//! tests 4 factorable functions per structure autonomously on TRAIN.
//! No claim of exhaustive global optimality is made.

use std::{collections::BinaryHeap, fmt};

use crate::{
    anf_dual_hidden_credit::{make_multi_gate_native, native_errors as native_dual_errors},
    anf_intermediate_credit::IntermediateRule,
    anf_local::LocalAnfNetwork,
    anf_local_incremental::{LaneBlock, LaneData},
    anf_nonlinear_joint_credit::{
        fit_nonlinear_credit, score_pair, NonlinearError, NonlinearReport,
    },
};

pub const A6C10_MAX_WIDTH: usize = 256;
pub const A6C10_RAW_POOL: [usize; 4] = [1, 2, 3, 4];
pub const A6C10_PROBE_STEPS: usize = 64;
pub const A6C10_STRUCTURAL_BEAM: usize = 64;
pub const A6C10_FACTORS_PER_STRUCTURE: usize = 4;
pub const A6C10_TRAIN_MAX_STEPS: usize = 768;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JointIdError {
    InvalidWidth,
    InvalidData,
    InvalidSpec,
    ScoreMismatch,
    Prior(NonlinearError),
}
impl fmt::Display for JointIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for JointIdError {}
impl From<NonlinearError> for JointIdError {
    fn from(err: NonlinearError) -> Self { Self::Prior(err) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JointOrigin {
    PriorA6C9,
    JointA6C10,
}
impl JointOrigin {
    pub fn name(self) -> &'static str {
        match self {
            Self::PriorA6C9 => "a6c9_previous",
            Self::JointA6C10 => "a6c10_joint",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JointIdReport {
    pub width: usize,
    pub previous: NonlinearReport,
    pub proposed: LocalAnfNetwork,
    pub proposed_rules: [IntermediateRule; 2],
    pub selected: LocalAnfNetwork,
    pub selected_rules: Vec<IntermediateRule>,
    pub origin: JointOrigin,
    pub train_errors: u64,
    pub validation_errors: u64,
    pub teacher_forced_errors: u64,
    pub selected_train_errors: u64,
    pub selected_validation_errors: u64,
    pub train_samples: u64,
    pub validation_samples: u64,
    pub probed_structures: usize,
    pub beam_count: usize,
    pub factorized_truth_masks: usize,
    pub native_train_rescores: usize,
    pub probe_step_count: usize,
    /// Dynamic N-word temporal scratch for 64 parallel Boolean sequences.
    pub ring_payload_bytes: usize,
    /// Counts only probe hypothesis heap and its 5 integers, not process RAM.
    pub beam_heap_payload_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Structure {
    // Sorting prefers smaller bound, then deterministic indices and channels.
    lower_bound: u64,
    first_gate: u16,
    second_gate: u16,
    first_raw: u8,
    second_raw: u8,
}

impl Structure {
    fn rules(self, f: u8, g: u8) -> [IntermediateRule; 2] {
        [
            IntermediateRule {
                gate: usize::from(self.first_gate),
                raw: usize::from(self.first_raw),
                truth: f,
            },
            IntermediateRule {
                gate: usize::from(self.second_gate),
                raw: usize::from(self.second_raw),
                truth: g,
            },
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Factorization {
    mask: u8,
    first_truth: u8,
    second_truth: u8,
    terms: usize,
}

#[derive(Clone, Debug)]
struct ObservedBlock<'a> {
    inputs: &'a [u64],
    steps: usize,
    valid: u64,
    root: Vec<u64>,
    target: Vec<u64>,
}
type Histograms = [[u64; 2]; 8];

fn validate(width: usize, data: &LaneData, train: bool) -> Result<u64, JointIdError> {
    if !(3..=A6C10_MAX_WIDTH).contains(&width) {
        return Err(JointIdError::InvalidWidth);
    }
    if data.input_bits != 5 || data.target_bits != 1 || data.blocks.is_empty() {
        return Err(JointIdError::InvalidData);
    }
    let mut samples = 0u64;
    let mut steps = 0usize;
    for block in &data.blocks {
        if block.steps == 0 || block.valid == 0
            || block.inputs.len() != 5 * block.steps
            || block.targets.len() != block.steps
        {
            return Err(JointIdError::InvalidData);
        }
        if train && block.steps < width + A6C10_PROBE_STEPS {
            return Err(JointIdError::InvalidData);
        }
        steps = steps.checked_add(block.steps).ok_or(JointIdError::InvalidData)?;
        samples += u64::from(block.valid.count_ones()) * block.steps as u64;
    }
    if train && steps > A6C10_TRAIN_MAX_STEPS {
        return Err(JointIdError::InvalidData);
    }
    Ok(samples)
}

fn observed<'a>(data: &'a LaneData) -> Vec<ObservedBlock<'a>> {
    data.blocks.iter().map(|block| {
        let mut root = Vec::with_capacity(block.steps);
        let mut target = Vec::with_capacity(block.steps);
        for t in 0..block.steps {
            let r = block.targets[t] ^ block.inputs[t * 5];
            root.push(r);
            target.push(r ^ block.inputs[t * 5 + 1]);
        }
        ObservedBlock {
            inputs: &block.inputs,
            steps: block.steps,
            valid: block.valid,
            root,
            target,
        }
    }).collect()
}

/// 3-input empirical counts on final output ONLY. Address bits:
/// bit0 = previously OBSERVED root(t-width), bit1 = delayed raw1,
/// bit2 = delayed raw2. No hidden/teacher gate bits enter this procedure.
///
/// The supervised output from t-width is strictly in the PAST at time t.
/// Use this only on TRAIN; native autonomous recurrence evaluates TEST.
fn histograms(
    width: usize,
    first_gate: usize,
    second_gate: usize,
    first_raw: usize,
    second_raw: usize,
    blocks: &[ObservedBlock<'_>],
    probe: bool,
) -> Histograms {
    let lag_a = width - first_gate;
    let lag_b = width - second_gate;
    let mut counts = [[0u64; 2]; 8];
    for block in blocks {
        let begin = if probe {
            block.steps - A6C10_PROBE_STEPS
        } else {
            width
        };
        for t in begin..block.steps {
            let root = block.root[t - width];
            let a = block.inputs[(t - lag_a) * 5 + first_raw];
            let b = block.inputs[(t - lag_b) * 5 + second_raw];
            let target = block.target[t];
            let roots = [!root, root];
            let left = [!a, a];
            let right = [!b, b];
            for address in 0..8 {
                let selected = roots[address & 1]
                    & left[(address >> 1) & 1]
                    & right[(address >> 2) & 1]
                    & block.valid;
                let one = u64::from((selected & target).count_ones());
                counts[address][0] += u64::from(selected.count_ones()) - one;
                counts[address][1] += one;
            }
        }
    }
    counts
}

fn unrestricted_lower_bound(counts: &Histograms) -> u64 {
    counts.iter().map(|row| row[0].min(row[1])).sum()
}

fn composed_mask(first: u8, second: u8) -> u8 {
    let mut mask = 0u8;
    for address in 0..8 {
        let r = address & 1;
        let a = (address >> 1) & 1;
        let b = (address >> 2) & 1;
        let hidden = (first >> (r | (a << 1))) & 1;
        let out = (second >> (usize::from(hidden) | (b << 1))) & 1;
        mask |= out << address;
    }
    mask
}

fn anf_terms(mask: u8) -> usize {
    let [a, b, c, d] = [0, 1, 2, 3].map(|i| mask & (1 << i) != 0);
    usize::from(a) + usize::from(b ^ a) +
        usize::from(c ^ a) + usize::from(d ^ c ^ b ^ a)
}

/// Canonical native factorization for each representable 3-input truth mask.
/// Prefer minimum ANF coefficients and deterministic function indices.
fn all_compositions() -> Vec<Factorization> {
    let mut by_mask: [Option<Factorization>; 256] = [None; 256];
    for first_truth in 0..16u8 {
        for second_truth in 0..16u8 {
            let mask = composed_mask(first_truth, second_truth);
            let factor = Factorization {
                mask,
                first_truth,
                second_truth,
                terms: anf_terms(first_truth) + anf_terms(second_truth),
            };
            let prior = by_mask[usize::from(mask)];
            if prior.is_none_or(|old| {
                (factor.terms, factor.first_truth, factor.second_truth)
                    < (old.terms, old.first_truth, old.second_truth)
            }) {
                by_mask[usize::from(mask)] = Some(factor);
            }
        }
    }
    by_mask.into_iter().flatten().collect()
}

fn mask_loss(mask: u8, counts: &Histograms) -> u64 {
    counts.iter().enumerate().map(|(i, c)| {
        if mask & (1 << i) == 0 { c[1] } else { c[0] }
    }).sum()
}

pub fn structure_count(width: usize) -> Option<usize> {
    (3..=A6C10_MAX_WIDTH).contains(&width)
        .then_some((width - 1) * (width - 2) / 2 * 16)
}

/// Probe ALL position/source pairs jointly, not one gate at a time.
/// One empirical lower bound for the unrestricted 3-input conditional
/// table ranks structures; retain the 64 lowest TRAIN-only results.
fn search_structures(
    width: usize,
    blocks: &[ObservedBlock<'_>],
) -> (Vec<Structure>, usize) {
    let mut queue: BinaryHeap<Structure> = BinaryHeap::with_capacity(A6C10_STRUCTURAL_BEAM + 1);
    let mut total = 0usize;
    for first_gate in 1..width {
        for second_gate in first_gate + 1..width {
            for first_raw in A6C10_RAW_POOL {
                for second_raw in A6C10_RAW_POOL {
                    let counts = histograms(
                        width, first_gate, second_gate,
                        first_raw, second_raw, blocks, true,
                    );
                    let candidate = Structure {
                        lower_bound: unrestricted_lower_bound(&counts),
                        first_gate: first_gate as u16,
                        second_gate: second_gate as u16,
                        first_raw: first_raw as u8,
                        second_raw: second_raw as u8,
                    };
                    total += 1;
                    if queue.len() < A6C10_STRUCTURAL_BEAM {
                        queue.push(candidate);
                    } else if queue.peek().is_some_and(|&worst| candidate < worst) {
                        queue.pop();
                        queue.push(candidate);
                    }
                }
            }
        }
    }
    (queue.into_sorted_vec(), total)
}

#[derive(Clone, Copy)]
struct Best {
    pair: [IntermediateRule; 2],
    full_train_errors: u64,
    teacher_forced_errors: u64,
    total_anf_terms: usize,
}

fn best_key(b: &Best) -> (u64, usize, usize, usize, usize, usize, u8, u8) {
    let [a, c] = b.pair;
    (
        b.full_train_errors,
        b.total_anf_terms,
        a.gate,
        c.gate,
        a.raw,
        c.raw,
        a.truth,
        c.truth,
    )
}

/// Train-only search; the final native score is independent of the
/// teacher-forced proxy and includes warm-up/initial-state effects.
/// This method never accepts a TEST or VALIDATION target for search.
pub fn fit_joint_system_id(
    train: &LaneData,
    validation: &LaneData,
    width: usize,
) -> Result<JointIdReport, JointIdError> {
    let train_samples = validate(width, train, true)?;
    let validation_samples = validate(width, validation, false)?;
    let previous = fit_nonlinear_credit(train, validation, width)?;
    let observed = observed(train);
    let (structures, probed_structures) = search_structures(width, &observed);
    if Some(probed_structures) != structure_count(width) {
        return Err(JointIdError::InvalidSpec);
    }
    let compositions = all_compositions();
    let mut ring = vec![0u64; width];
    let mut best: Option<Best> = None;
    let mut native_train_rescores = 0usize;

    for structure in &structures {
        let counts = histograms(
            width,
            usize::from(structure.first_gate),
            usize::from(structure.second_gate),
            usize::from(structure.first_raw),
            usize::from(structure.second_raw),
            &observed,
            false,
        );
        let mut factors = compositions.iter().copied().map(|f| {
            (mask_loss(f.mask, &counts), f.terms, f.first_truth, f.second_truth, f)
        }).collect::<Vec<_>>();
        factors.sort_unstable_by_key(|f| (f.0, f.1, f.2, f.3));
        for (teacher_loss, _, _, _, factor) in
            factors.into_iter().take(A6C10_FACTORS_PER_STRUCTURE)
        {
            let pair = structure.rules(factor.first_truth, factor.second_truth);
            let full_train_errors = score_pair(width, pair, train, &mut ring)?;
            native_train_rescores += 1;
            let candidate = Best {
                pair,
                full_train_errors,
                teacher_forced_errors: teacher_loss,
                total_anf_terms: factor.terms,
            };
            if best.as_ref().is_none_or(|old| best_key(&candidate) < best_key(old)) {
                best = Some(candidate);
            }
        }
    }

    let best = best.ok_or(JointIdError::InvalidSpec)?;
    let proposed = make_multi_gate_native(width, &best.pair)?;
    if native_dual_errors(&proposed, train)? != best.full_train_errors {
        return Err(JointIdError::ScoreMismatch);
    }
    let validation_errors = native_dual_errors(&proposed, validation)?;
    let use_new = validation_errors < previous.selected_validation_errors;
    let selected = if use_new { proposed.clone() } else { previous.selected.clone() };
    let selected_rules = if use_new {
        best.pair.to_vec()
    } else {
        previous.selected_rules.clone()
    };
    let selected_train_errors = if use_new {
        best.full_train_errors
    } else {
        previous.selected_train_errors
    };
    let selected_validation_errors = if use_new {
        validation_errors
    } else {
        previous.selected_validation_errors
    };
    if native_dual_errors(&selected, train)? != selected_train_errors
        || native_dual_errors(&selected, validation)? != selected_validation_errors
    {
        return Err(JointIdError::ScoreMismatch);
    }
    Ok(JointIdReport {
        width,
        previous,
        proposed,
        proposed_rules: best.pair,
        selected,
        selected_rules,
        origin: if use_new { JointOrigin::JointA6C10 } else { JointOrigin::PriorA6C9 },
        train_errors: best.full_train_errors,
        validation_errors,
        teacher_forced_errors: best.teacher_forced_errors,
        selected_train_errors,
        selected_validation_errors,
        train_samples,
        validation_samples,
        probed_structures,
        beam_count: structures.len(),
        factorized_truth_masks: compositions.len(),
        native_train_rescores,
        probe_step_count: A6C10_PROBE_STEPS,
        ring_payload_bytes: width * std::mem::size_of::<u64>(),
        beam_heap_payload_bytes: A6C10_STRUCTURAL_BEAM * std::mem::size_of::<Structure>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{anf_local_incremental::LaneBlock, toy_tasks::SplitMix64};

    fn fixture(seed: u64, width: usize, steps: usize, rules: &[IntermediateRule]) -> LaneData {
        let teacher = make_multi_gate_native(width, rules).unwrap();
        let mut rng = SplitMix64::new(seed);
        let mut inputs = vec![0u64; steps * 5];
        for word in &mut inputs { *word = rng.next_u64(); }
        let mut source = vec![0u64; teacher.source_width()];
        let mut next = vec![0u64; width];
        let mut scratch = vec![0u64; teacher.scratch_len()];
        let mut targets = vec![0u64; steps];
        for (t, label) in targets.iter_mut().enumerate() {
            let start = t * 5;
            source[width..].copy_from_slice(&inputs[start..start+5]);
            teacher.step_lanes(&mut source, &mut next, &mut scratch);
            *label = teacher.readout().evaluate_lanes(&source, &mut scratch);
        }
        LaneData {
            input_bits: 5, target_bits: 1,
            blocks: vec![LaneBlock { steps, inputs, targets, valid:u64::MAX }],
        }
    }

    #[test]
    fn composed_masks_are_exact_for_all_256_truth_pairs() {
        for f in 0..16u8 {
            for g in 0..16u8 {
                let mask = composed_mask(f, g);
                for r in 0..2usize {
                    for a in 0..2usize {
                        for b in 0..2usize {
                            let inner = (f >> (r + 2*a)) & 1;
                            let expected = (g >> (usize::from(inner) + 2*b)) & 1;
                            assert_eq!((mask >> (r + 2*a + 4*b)) & 1, expected);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn exact_two_gate_teacher_forced_and_and_has_zero_proxy_error() {
        let width = 16;
        let rules = [
            IntermediateRule { gate: 5, raw: 2, truth: 8 },
            IntermediateRule { gate: 11, raw: 3, truth: 8 },
        ];
        let data = fixture(10, width, 112, &rules);
        let obs = observed(&data);
        let counts = histograms(width, 5, 11, 2, 3, &obs, false);
        assert_eq!(mask_loss(composed_mask(8,8), &counts), 0);
        assert_eq!(unrestricted_lower_bound(&counts), 0);
        assert_eq!(structure_count(width), Some(1680));
    }

    #[test]
    fn joint_recovers_and_and_on_unseen_causal_rollout() {
        let width = 24;
        let rules = [
            IntermediateRule { gate: 7, raw: 2, truth: 8 },
            IntermediateRule { gate: 17, raw: 3, truth: 8 },
        ];
        let train = fixture(77, width, 160, &rules);
        let validation = fixture(78, width, 160, &rules);
        let fit = fit_joint_system_id(&train, &validation, width).unwrap();
        assert_eq!(fit.train_errors, 0);
        assert_eq!(fit.validation_errors, 0);
        assert_eq!(fit.selected_validation_errors, 0);
        let heldout = fixture(79, width, 512, &rules);
        assert_eq!(native_dual_errors(&fit.selected, &heldout).unwrap(), 0);
        assert_eq!(fit.native_train_rescores, 64 * 4);
    }

    #[test]
    fn validation_labels_cannot_change_train_proposed_hidden_functions() {
        let width = 16;
        let rules = [
            IntermediateRule { gate: 4, raw: 2, truth: 6 },
            IntermediateRule { gate: 12, raw: 3, truth: 8 },
        ];
        let train = fixture(81, width, 120, &rules);
        let val = fixture(82, width, 120, &rules);
        let x = fit_joint_system_id(&train, &val, width).unwrap();
        let mut flipped = val.clone();
        for b in &mut flipped.blocks {
            for target in &mut b.targets { *target = !*target; }
        }
        let y = fit_joint_system_id(&train, &flipped, width).unwrap();
        assert_eq!(x.proposed_rules, y.proposed_rules);
        assert_eq!(x.train_errors, y.train_errors);
        assert_eq!(x.teacher_forced_errors, y.teacher_forced_errors);
    }

    #[test]
    fn missing_probe_and_bad_width_are_rejected() {
        let small = fixture(91, 16, 20, &[]);
        let valid = fixture(92, 16, 120, &[]);
        assert_eq!(fit_joint_system_id(&small, &valid, 16), Err(JointIdError::InvalidData));
        assert_eq!(fit_joint_system_id(&valid, &valid, 2), Err(JointIdError::InvalidWidth));
        assert_eq!(structure_count(128), Some(128_016));
        assert_eq!(structure_count(256), Some(518_160));
    }
}
