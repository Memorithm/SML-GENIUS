//! A6-C4: learn both Boolean gates AND their causal recurrent topology from
//! final task labels only. No task-specific intermediate labels or gate-wiring
//! blueprint is supplied to the learner.
//!
//! A bounded 3-slot acyclic search grammar is used, with synchronous update:
//!  s0(t+1) <- two CURRENT raw input bits;
//!  s1(t+1) <- two inputs chosen among prior s0(t) and current raw bits;
//!  s2(t+1) <- two inputs chosen among prior s0(t), prior s1(t), current raw;
//!  y_hat(t) = x_baseline(t) XOR s2(t+1).
//!
//! This graph permits direct (0-hop), skip (1-hop), serial (2-hop) and parallel
//! (two 1-hop branches) topologies. All input-source edges and two-input GF(2)
//! gate truth tables are selected on TRAIN ONLY. The final gate is fitted
//! exactly by conditional majority, converted to ANF via the existing Mobius
//! transform; earlier gate functions are enumerated. The best full circuit is
//! accepted on VALIDATION only if it strictly improves the zero-gate baseline.
//! No test data is accepted by this API.
//!
//! Factorized enumeration avoids repeating structurally identical hypotheses:
//!  6 direct readouts + 96*4 skip readouts + 96*160*5 stateful readouts
//!  = 77,190 train-task fits, not millions of identical direct candidates.
//! This is an explicitly bounded proof-of-mechanism reference, NOT general
//! recurrent topology discovery or scalable automated architecture search.

use std::fmt;

use crate::{
    anf_constructive::{residual_output_errors, ConstructiveError},
    anf_local::{LocalAnfError, LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::LaneData,
};

const STATES: usize = 3;
const INPUT_BITS: usize = 5;
const MAX_TOTAL_TRAIN_STEPS: usize = 256;
const RAW_POOL: usize = 4;
const EXPECTED_EVALUATIONS: usize = 6 + 96 * 4 + 96 * 160 * 5;
const EXPECTED_CHAIN_FITS: usize = 96 * 64 * 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveredTopology {
    Direct,
    Skip,
    ViaMiddle,
    Chain,
    Parallel,
    BranchChain,
}

impl DiscoveredTopology {
    pub fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct_0_hops",
            Self::Skip => "skip_1_hop",
            Self::ViaMiddle => "middle_1_hop",
            Self::Chain => "chain_2_hops",
            Self::Parallel => "parallel_1_hop",
            Self::BranchChain => "branch_2_hops",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopologyError {
    Network(LocalAnfError),
    Constructive(ConstructiveError),
    Shape,
    Spec,
    NativeOracle,
}
impl fmt::Display for TopologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TopologyError {}
impl From<LocalAnfError> for TopologyError {
    fn from(value: LocalAnfError) -> Self { Self::Network(value) }
}
impl From<ConstructiveError> for TopologyError {
    fn from(value: ConstructiveError) -> Self { Self::Constructive(value) }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologySpec {
    /// Four unique candidate channels different from baseline_input.
    pub raw_candidates: Vec<usize>,
    pub baseline_input: usize,
    pub target_bit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologyReport {
    /// Validation-accepted model; otherwise unchanged reference network.
    pub selected: LocalAnfNetwork,
    /// Exact train winner, even if validation rejected it.
    pub proposed: LocalAnfNetwork,
    pub direct_control: LocalAnfNetwork,
    pub chain_control: LocalAnfNetwork,
    pub accepted: bool,
    pub direct_accepted: bool,
    pub chain_accepted: bool,
    pub topology: DiscoveredTopology,
    pub depth: usize,
    pub live_state_gates: usize,
    pub learned_monomials: usize,
    pub train_errors: u64,
    pub baseline_train_errors: u64,
    pub baseline_validation_errors: u64,
    pub validation_errors: u64,
    pub direct_train_errors: u64,
    pub direct_validation_errors: u64,
    pub chain_train_errors: u64,
    pub chain_validation_errors: u64,
    pub train_samples: u64,
    pub validation_samples: u64,
    pub train_evaluations: usize,
    pub chain_evaluations: usize,
    /// The streamed search retains two multi-lane series, not the 4.5MiB
    /// A6-C3 intermediate-state tensor. This counts their u64 payload.
    pub peak_series_payload_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GateRule {
    inputs: [usize; 2],
    /// Truth-table bits indexed by local address: index = bit0 + 2*bit1.
    truth: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Structure {
    topology: DiscoveredTopology,
    depth: usize,
    live_gates: usize,
    monomials: usize,
    active: [bool; 3],
}

#[derive(Debug, Clone)]
struct Trained {
    gates: [GateRule; 3],
    errors: u64,
    structure: Structure,
}

type Series = Vec<Vec<u64>>; // one u64 per time step and 64 lanes

fn validate_setup(
    network: &LocalAnfNetwork,
    data: &LaneData,
    spec: &TopologySpec,
    limit_train: bool,
) -> Result<u64, TopologyError> {
    if network.state_bits() != STATES
        || network.input_bits() != INPUT_BITS
        || spec.raw_candidates.len() != RAW_POOL
        || spec.baseline_input >= INPUT_BITS
        || spec.raw_candidates.contains(&spec.baseline_input)
        || spec.target_bit >= data.target_bits
        || data.input_bits != INPUT_BITS
        || data.blocks.is_empty()
    {
        return Err(TopologyError::Spec);
    }
    for (i, &raw) in spec.raw_candidates.iter().enumerate() {
        if raw >= INPUT_BITS || spec.raw_candidates[..i].contains(&raw) {
            return Err(TopologyError::Spec);
        }
    }
    if network
        .gates()
        .iter()
        .any(|gate| gate.truth_table().iter().any(|&bit| bit))
    {
        return Err(TopologyError::Spec);
    }
    let mut steps = 0usize;
    let mut samples = 0u64;
    for block in &data.blocks {
        if block.steps == 0
            || block.valid == 0
            || block.inputs.len() != block.steps * data.input_bits
            || block.targets.len() != block.steps * data.target_bits
        {
            return Err(TopologyError::Shape);
        }
        steps = steps.checked_add(block.steps).ok_or(TopologyError::Shape)?;
        samples += u64::from(block.valid.count_ones()) * block.steps as u64;
    }
    if limit_train && steps > MAX_TOTAL_TRAIN_STEPS {
        return Err(TopologyError::Shape);
    }
    Ok(samples)
}

#[inline]
fn apply_truth(truth: u8, a: u64, b: u64) -> u64 {
    let mut out = 0u64;
    if truth & 1 != 0 { out |= !a & !b; }
    if truth & 2 != 0 { out |= a & !b; }
    if truth & 4 != 0 { out |= !a & b; }
    if truth & 8 != 0 { out |= a & b; }
    out
}

fn first_rules(spec: &TopologySpec) -> Vec<GateRule> {
    let mut out = Vec::with_capacity(96);
    for (i, &a) in spec.raw_candidates.iter().enumerate() {
        for &b in &spec.raw_candidates[i + 1..] {
            for truth in 0..16u8 {
                out.push(GateRule { inputs: [STATES + a, STATES + b], truth });
            }
        }
    }
    out
}

fn middle_rules(spec: &TopologySpec) -> Vec<GateRule> {
    let inputs: Vec<usize> = std::iter::once(0)
        .chain(spec.raw_candidates.iter().map(|&raw| STATES + raw))
        .collect();
    let mut out = Vec::with_capacity(160);
    for (i, &a) in inputs.iter().enumerate() {
        for &b in &inputs[i + 1..] {
            for truth in 0..16u8 {
                out.push(GateRule { inputs: [a, b], truth });
            }
        }
    }
    out
}

fn output_pairs(spec: &TopologySpec) -> Vec<[usize; 2]> {
    let options: Vec<usize> = [0, 1].into_iter()
        .chain(spec.raw_candidates.iter().map(|&raw| STATES + raw))
        .collect();
    let mut pairs = Vec::with_capacity(15);
    for (i, &a) in options.iter().enumerate() {
        for &b in &options[i + 1..] { pairs.push([a, b]); }
    }
    pairs
}

fn first_series(rule: &GateRule, data: &LaneData) -> Series {
    let a = rule.inputs[0] - STATES;
    let b = rule.inputs[1] - STATES;
    data.blocks.iter().map(|block| {
        (0..block.steps).map(|t| {
            apply_truth(
                rule.truth,
                block.inputs[t * INPUT_BITS + a],
                block.inputs[t * INPUT_BITS + b],
            )
        }).collect()
    }).collect()
}

fn middle_series(rule: &GateRule, first: &Series, data: &LaneData) -> Series {
    data.blocks.iter().enumerate().map(|(bi, block)| {
        (0..block.steps).map(|t| {
            let word = |source: usize| {
                if source == 0 {
                    if t == 0 { 0 } else { first[bi][t - 1] }
                } else {
                    block.inputs[t * INPUT_BITS + source - STATES]
                }
            };
            apply_truth(rule.truth, word(rule.inputs[0]), word(rule.inputs[1]))
        }).collect()
    }).collect()
}

/// Derive the exact minimum TRAIN errors of a two-input Boolean final gate.
/// No intermediate labels, no target access during gate-0/gate-1 updates.
fn fit_output(
    inputs: [usize; 2],
    first: Option<&Series>,
    middle: Option<&Series>,
    data: &LaneData,
    spec: &TopologySpec,
) -> (u8, u64) {
    let mut counts = [[0u64; 2]; 4];
    for (bi, block) in data.blocks.iter().enumerate() {
        for t in 0..block.steps {
            let source_word = |source: usize| match source {
                0 => first.map_or(0, |s| if t == 0 { 0 } else { s[bi][t - 1] }),
                1 => middle.map_or(0, |s| if t == 0 { 0 } else { s[bi][t - 1] }),
                _ => block.inputs[t * INPUT_BITS + source - STATES],
            };
            let a = source_word(inputs[0]);
            let b = source_word(inputs[1]);
            let labels = block.targets[t * data.target_bits + spec.target_bit]
                ^ block.inputs[t * INPUT_BITS + spec.baseline_input];
            let masks = [!a & !b, a & !b, !a & b, a & b];
            for (address, &mask) in masks.iter().enumerate() {
                let seen = mask & block.valid;
                let positive = u64::from((seen & labels).count_ones());
                counts[address][1] += positive;
                counts[address][0] += u64::from(seen.count_ones()) - positive;
            }
        }
    }
    let mut truth = 0u8;
    let mut loss = 0u64;
    for (i, c) in counts.iter().enumerate() {
        if c[1] > c[0] { truth |= 1 << i; }
        loss += c[0].min(c[1]);
    }
    (truth, loss)
}

fn input_matters(truth: u8, pos: usize) -> bool {
    (0..4).any(|address| address & (1 << pos) == 0
        && ((truth >> address) & 1) != ((truth >> (address | (1 << pos))) & 1))
}
fn matters(rule: &GateRule, source: usize) -> bool {
    rule.inputs.iter().enumerate().any(|(i, &s)| s == source && input_matters(rule.truth, i))
}
fn monomials(truth: u8) -> usize {
    let mut a: [bool; 4] = std::array::from_fn(|i| truth & (1 << i) != 0);
    for step in [1, 2] {
        for i in 0..4 { if i & step != 0 { a[i] ^= a[i ^ step]; } }
    }
    a.iter().filter(|&&v| v).count()
}

fn structure(gates: &[GateRule; 3]) -> Structure {
    let output_from_first = matters(&gates[2], 0);
    let output_from_middle = matters(&gates[2], 1);
    let middle_from_first = matters(&gates[1], 0);
    let active_first = output_from_first || (output_from_middle && middle_from_first);
    let active_middle = output_from_middle;
    let depth = if output_from_middle && middle_from_first {
        2
    } else if output_from_first || output_from_middle {
        1
    } else { 0 };
    let topology = if output_from_first && output_from_middle {
        if middle_from_first { DiscoveredTopology::BranchChain }
        else { DiscoveredTopology::Parallel }
    } else if output_from_middle {
        if middle_from_first { DiscoveredTopology::Chain }
        else { DiscoveredTopology::ViaMiddle }
    } else if output_from_first {
        DiscoveredTopology::Skip
    } else { DiscoveredTopology::Direct };
    let active = [active_first, active_middle, true];
    Structure {
        topology,
        depth,
        live_gates: active.iter().filter(|&&live| live).count(),
        monomials: gates.iter().zip(active).filter(|(_, live)| *live)
            .map(|(gate, _)| monomials(gate.truth)).sum(),
        active,
    }
}

fn consider(best: &mut Option<Trained>, gates: [GateRule; 3], errors: u64) {
    let candidate = Trained { structure: structure(&gates), gates, errors };
    let better = best.as_ref().is_none_or(|prior| {
        let a = (
            candidate.errors,
            candidate.structure.live_gates,
            candidate.structure.depth,
            candidate.structure.monomials,
        );
        let b = (
            prior.errors,
            prior.structure.live_gates,
            prior.structure.depth,
            prior.structure.monomials,
        );
        a < b
    });
    if better { *best = Some(candidate); }
}

fn materialize(
    reference: &LocalAnfNetwork,
    trained: &Trained,
) -> Result<LocalAnfNetwork, TopologyError> {
    let mut model = reference.clone();
    for (i, rule) in trained.gates.iter().enumerate() {
        if trained.structure.active[i] {
            let table: Vec<bool> = (0..4).map(|a| rule.truth & (1 << a) != 0).collect();
            model.gates_mut()[i] =
                LocalAnfGate::from_truth_table(rule.inputs.to_vec(), &table, reference.source_width())?;
        }
    }
    Ok(model)
}

fn native_errors(
    network: &LocalAnfNetwork, data: &LaneData, spec: &TopologySpec,
) -> Result<u64, TopologyError> {
    Ok(residual_output_errors(
        network, data, 2, spec.target_bit, spec.baseline_input,
    )?.0)
}

fn validate_commit(
    original: &LocalAnfNetwork, model: LocalAnfNetwork,
    validation: &LaneData, spec: &TopologySpec, baseline_errors: u64,
) -> Result<(LocalAnfNetwork, u64, bool), TopologyError> {
    let val = native_errors(&model, validation, spec)?;
    let accepted = val < baseline_errors;
    Ok((if accepted { model } else { original.clone() }, val, accepted))
}

/// Learn sources, topology and 3 Boolean functions solely from terminal
/// target errors, with a 0-hop and a fixed A6-C3 2-hop chain control.
///
/// Complexity tie-breaking prefers fewer effective state gates and shorter
/// causal depth. The fixed-chain model is selected independently on TRAIN.
/// Validation checks whether each selected whole circuit is worth committing.
/// Neither model search nor selection receives held-out test samples.
pub fn fit_adaptive_topology(
    reference: &LocalAnfNetwork,
    train: &LaneData,
    validation: &LaneData,
    spec: &TopologySpec,
) -> Result<TopologyReport, TopologyError> {
    let train_samples = validate_setup(reference, train, spec, true)?;
    let validation_samples = validate_setup(reference, validation, spec, false)?;
    let first = first_rules(spec);
    let middle = middle_rules(spec);
    let pairs = output_pairs(spec);
    if first.len() != 96 || middle.len() != 160 || pairs.len() != 15 {
        return Err(TopologyError::Spec);
    }
    let empty_first = GateRule { inputs: first[0].inputs, truth: 0 };
    let empty_middle = GateRule { inputs: middle[0].inputs, truth: 0 };
    let mut adaptive: Option<Trained> = None;
    let mut direct: Option<Trained> = None;
    let mut chain: Option<Trained> = None;
    let mut evals = 0usize;
    let mut chain_evals = 0usize;

    // Direct, structurally independent of all state-gate parameter choices.
    for &pair in pairs.iter().filter(|p| p[0] >= STATES) {
        let (truth, loss) = fit_output(pair, None, None, train, spec);
        let gates = [empty_first, empty_middle, GateRule { inputs: pair, truth }];
        consider(&mut direct, gates, loss);
        consider(&mut adaptive, gates, loss);
        evals += 1;
    }

    for &rule0 in &first {
        let s0 = first_series(&rule0, train);

        // One-hop skip topology: output sees previous gate 0, no gate 1.
        for &pair in pairs.iter().filter(|p| p[0] == 0 && p[1] >= STATES) {
            let (truth, loss) = fit_output(pair, Some(&s0), None, train, spec);
            consider(&mut adaptive, [
                rule0, empty_middle, GateRule { inputs: pair, truth },
            ], loss);
            evals += 1;
        }

        // Enumerate all permissible stage-1 sources and two-input functions.
        for &rule1 in &middle {
            let s1 = middle_series(&rule1, &s0, train);
            for &pair in pairs.iter().filter(|p| p[1] == 1 || p[0] == 1) {
                // Cases: [old_s0, old_s1], [old_s1, current_raw].
                let (truth, loss) = fit_output(pair, Some(&s0), Some(&s1), train, spec);
                let gates = [rule0, rule1, GateRule { inputs: pair, truth }];
                consider(&mut adaptive, gates, loss);
                evals += 1;
                // Strict C3 reference topology: 0 -> 1 -> 2, one hop per
                // upstream state, plus one raw input at each stage.
                if rule1.inputs[0] == 0 && pair[0] == 1 && pair[1] >= STATES {
                    consider(&mut chain, gates, loss);
                    chain_evals += 1;
                }
            }
        }
    }
    if evals != EXPECTED_EVALUATIONS || chain_evals != EXPECTED_CHAIN_FITS {
        return Err(TopologyError::Spec);
    }
    let adaptive = adaptive.ok_or(TopologyError::Spec)?;
    let direct = direct.ok_or(TopologyError::Spec)?;
    let chain = chain.ok_or(TopologyError::Spec)?;
    let proposed = materialize(reference, &adaptive)?;
    let direct_candidate = materialize(reference, &direct)?;
    let chain_candidate = materialize(reference, &chain)?;
    for (candidate, expected_loss) in [
        (&proposed, adaptive.errors), (&direct_candidate, direct.errors),
        (&chain_candidate, chain.errors),
    ] {
        if native_errors(candidate, train, spec)? != expected_loss {
            return Err(TopologyError::NativeOracle);
        }
    }
    let baseline_train_errors = native_errors(reference, train, spec)?;
    let baseline_validation_errors = native_errors(reference, validation, spec)?;
    let (selected, validation_errors, accepted) = validate_commit(
        reference, proposed.clone(), validation, spec, baseline_validation_errors,
    )?;
    let (direct_control, direct_validation_errors, direct_accepted) = validate_commit(
        reference, direct_candidate, validation, spec, baseline_validation_errors,
    )?;
    let (chain_control, chain_validation_errors, chain_accepted) = validate_commit(
        reference, chain_candidate, validation, spec, baseline_validation_errors,
    )?;
    let train_words = train.blocks.iter().map(|b| b.steps).sum::<usize>();
    Ok(TopologyReport {
        selected,
        proposed,
        direct_control,
        chain_control,
        accepted,
        direct_accepted,
        chain_accepted,
        topology: adaptive.structure.topology,
        depth: adaptive.structure.depth,
        live_state_gates: adaptive.structure.live_gates,
        learned_monomials: adaptive.structure.monomials,
        train_errors: adaptive.errors,
        baseline_train_errors,
        baseline_validation_errors,
        validation_errors,
        direct_train_errors: direct.errors,
        direct_validation_errors,
        chain_train_errors: chain.errors,
        chain_validation_errors,
        train_samples,
        validation_samples,
        train_evaluations: evals,
        chain_evaluations: chain_evals,
        peak_series_payload_bytes: 2 * train_words * std::mem::size_of::<u64>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{anf_local_incremental::LaneBlock, toy_tasks::SplitMix64};

    fn zero() -> LocalAnfNetwork {
        let n = 3;
        let width = n + INPUT_BITS;
        let gates = vec![
            LocalAnfGate::zero(vec![n + 1, n + 2], width).unwrap(),
            LocalAnfGate::zero(vec![0, n + 1], width).unwrap(),
            LocalAnfGate::zero(vec![1, n + 1], width).unwrap(),
        ];
        let readout = LocalAnfGate::zero(vec![n], width).unwrap();
        LocalAnfNetwork::with_input_bits(n, INPUT_BITS, gates, readout).unwrap()
    }
    fn spec() -> TopologySpec {
        TopologySpec { raw_candidates: vec![1, 2, 3, 4], baseline_input: 0, target_bit: 0 }
    }
    #[derive(Clone, Copy)]
    enum Task { Direct, Skip, Serial, Parallel }
    fn fixture(seed: u64, steps: usize, task: Task) -> LaneData {
        let mut rng = SplitMix64::new(seed);
        let mut inputs = vec![0u64; steps * INPUT_BITS];
        for word in &mut inputs { *word = rng.next_u64(); }
        let mut targets = vec![0u64; steps];
        let x = |t: usize, p: usize| inputs[t * INPUT_BITS + p];
        for t in 0..steps {
            let residual = match task {
                Task::Direct => x(t, 1) & x(t, 2),
                Task::Skip => {
                    if t == 0 { 0 } else {
                        (x(t - 1, 1) ^ x(t - 1, 2)) & x(t, 3)
                    }
                }
                Task::Serial => {
                    x(t, 4) ^ if t < 2 { 0 } else {
                        x(t - 1, 3) & (x(t - 2, 1) ^ x(t - 2, 2))
                    }
                }
                Task::Parallel => {
                    if t == 0 { 0 } else {
                        (x(t - 1, 1) & x(t - 1, 2))
                            ^ (x(t - 1, 3) & x(t - 1, 4))
                    }
                }
            };
            targets[t] = x(t, 0) ^ residual;
        }
        LaneData {
            input_bits: INPUT_BITS,
            target_bits: 1,
            blocks: vec![LaneBlock { steps, inputs, targets, valid: u64::MAX }],
        }
    }

    #[test]
    fn native_truth_table_matches_packed_mask_for_all_boolean_functions() {
        for mask in 0..16u8 {
            let table: Vec<bool> = (0..4).map(|i| mask & (1 << i) != 0).collect();
            let gate = LocalAnfGate::from_truth_table(vec![3, 4], &table, 8).unwrap();
            let mut products = [0u64; 4];
            for a in [0, u64::MAX] {
                for b in [0, u64::MAX] {
                    assert_eq!(gate.evaluate_gathered(&[a, b], &mut products), apply_truth(mask, a, b));
                }
            }
        }
    }

    #[test]
    fn finds_multiple_topologies_using_only_output_labels() {
        for (task, depth) in [
            (Task::Direct, 0), (Task::Skip, 1), (Task::Serial, 2),
            (Task::Parallel, 1),
        ] {
            let train = fixture(21, 48, task);
            let validation = fixture(22, 24, task);
            let reference = zero();
            let report = fit_adaptive_topology(&reference, &train, &validation, &spec()).unwrap();
            assert_eq!(report.train_errors, 0);
            assert_eq!(report.validation_errors, 0);
            assert_eq!(report.depth, depth, "task depth mismatch");
            assert!(report.accepted);
            let test = fixture(23, 24, task);
            assert_eq!(native_errors(&report.selected, &test, &spec()).unwrap(), 0);
            assert_eq!(report.train_evaluations, EXPECTED_EVALUATIONS);
            assert_eq!(report.chain_evaluations, EXPECTED_CHAIN_FITS);
            assert_eq!(reference, zero());
        }
    }

    #[test]
    fn output_only_training_does_not_see_validation_or_future_targets() {
        let data = fixture(54, 48, Task::Serial);
        let val = fixture(55, 24, Task::Serial);
        let original = zero();
        let a = fit_adaptive_topology(&original, &data, &val, &spec()).unwrap();
        let mut inverted = val.clone();
        for block in &mut inverted.blocks {
            for target in &mut block.targets { *target = !*target; }
        }
        let b = fit_adaptive_topology(&original, &data, &inverted, &spec()).unwrap();
        assert_eq!(a.proposed, b.proposed);
        assert_eq!(a.train_errors, b.train_errors);
        assert_eq!(a.topology, b.topology);
        assert!(!b.accepted);
        assert_eq!(original, zero());
    }

    #[test]
    fn rejects_malformed_source_pool_and_training_split() {
        let mut bad_spec = spec();
        bad_spec.raw_candidates = vec![1, 2, 2, 4];
        let train = fixture(3, 8, Task::Direct);
        assert_eq!(
            fit_adaptive_topology(&zero(), &train, &train, &bad_spec),
            Err(TopologyError::Spec)
        );
        let mut malformed = train.clone();
        malformed.blocks[0].targets.clear();
        assert_eq!(
            fit_adaptive_topology(&zero(), &malformed, &train, &spec()),
            Err(TopologyError::Shape)
        );
    }
}
