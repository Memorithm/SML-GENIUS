//! A6-C11D diagnostic: three hidden ANF gates, unlike two-gate A6-C10.
//! No hidden gate targets. Only past TRAIN outputs are used to construct
//! autoregressive Walsh features; all validation and held-out models are
//! executed freely, with actual native LocalAnfNetwork oracle verification.
//!
//! Shared 508-feature scan -> top24 lag/source pairs -> spectral triplets.
//! Equal feature-screen budgets, but 64 versus 640 complete TRAIN triplet
//! histogram evaluations (10x). The 16 best conditional candidates are
//! rescored in a genuinely causal four-segment u64 ring, then verified
//! against full-width native GF(2) ANF simulation.
//! 512 vs 2048 TRAIN data is compared independently from those budgets.
//! This is bounded three-gate identification, NOT arbitrary graph learning.

use crate::{
    anf_dual_hidden_credit::{make_multi_gate_native, native_errors, DualCreditError},
    anf_intermediate_credit::IntermediateRule,
    anf_local::LocalAnfNetwork,
    anf_local_incremental::LaneData,
};
use std::fmt;

pub const DIAG_INPUT_BITS: usize = 5;
pub const DIAG_MAX_WIDTH: usize = 256;
pub const DIAG_MAX_TRAIN_STEPS: usize = 2048;
pub const DIAG_RAW_POOL: [usize; 4] = [1, 2, 3, 4];
pub const DIAG_SCREEN_FEATURES: usize = 24;
pub const DIAG_NATIVE_RESCORING: usize = 16;
pub const DIAG_BUDGETS: [usize; 2] = [64, 640];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TripleDiagError {
    InvalidWidth,
    InvalidSplit,
    InvalidBudget,
    FactorizationEmpty,
    NativeOracle(DualCreditError),
    ScoreMismatch,
}
impl fmt::Display for TripleDiagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TripleDiagError {}
impl From<DualCreditError> for TripleDiagError {
    fn from(value: DualCreditError) -> Self {
        Self::NativeOracle(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Source {
    pub gate: usize,
    pub raw: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RankedFeature {
    pub source: Source,
    pub walsh_absolute: u64,
    pub raw_walsh_absolute: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TripleRules {
    pub rules: [IntermediateRule; 3],
}
impl TripleRules {
    pub fn sources(self) -> [Source; 3] {
        self.rules.map(|r| Source {
            gate: r.gate,
            raw: r.raw,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetFit {
    pub budget: usize,
    pub structures_full_train_scored: usize,
    pub native_train_rescores: usize,
    pub proposal_sources: [Source; 3],
    pub proposal_rules: TripleRules,
    pub proposal: LocalAnfNetwork,
    pub committed: LocalAnfNetwork,
    pub composed_truth: u16,
    pub teacher_forced_train_errors: u64,
    pub autonomous_train_errors: u64,
    pub autonomous_validation_errors: u64,
    pub baseline_validation_errors: u64,
    pub accepted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TripleDiagReport {
    pub width: usize,
    pub train_samples: u64,
    pub validation_samples: u64,
    pub baseline_train_errors: u64,
    pub baseline_validation_errors: u64,
    pub ranked_features: Vec<RankedFeature>,
    pub strongest_univariate: Source,
    pub all_lag_feature_count: usize,
    pub spectral_triplet_pool_count: usize,
    pub distinct_factorizable_masks: usize,
    pub screen_features: usize,
    pub budgets: Vec<BudgetFit>,
    pub scratch_ring_bytes: usize,
}
#[derive(Clone, Debug)]
struct Observed<'a> {
    inputs: &'a [u64],
    valid: u64,
    root: Vec<u64>,
    residual: Vec<u64>,
}
#[derive(Clone, Copy, Debug)]
struct Factor {
    mask: u16,
    truths: [u8; 3],
    anf_terms: usize,
}
#[derive(Clone, Copy, Debug)]
struct Candidate {
    sources: [Source; 3],
    spectral_sum: u64,
}
#[derive(Clone, Copy, Debug)]
struct Scored {
    candidate: Candidate,
    factor: Factor,
    teacher_loss: u64,
    native_loss: u64,
}
fn validate(width: usize, data: &LaneData, capped: bool) -> Result<u64, TripleDiagError> {
    if !(4..=DIAG_MAX_WIDTH).contains(&width) {
        return Err(TripleDiagError::InvalidWidth);
    }
    if data.input_bits != DIAG_INPUT_BITS || data.target_bits != 1 || data.blocks.is_empty() {
        return Err(TripleDiagError::InvalidSplit);
    }
    let mut steps = 0usize;
    let mut samples = 0u64;
    for block in &data.blocks {
        if block.steps <= width
            || block.valid == 0
            || block.inputs.len() != block.steps * data.input_bits
            || block.targets.len() != block.steps
        {
            return Err(TripleDiagError::InvalidSplit);
        }
        steps = steps
            .checked_add(block.steps)
            .ok_or(TripleDiagError::InvalidSplit)?;
        samples += u64::from(block.valid.count_ones()) * block.steps as u64;
    }
    if capped && steps > DIAG_MAX_TRAIN_STEPS {
        return Err(TripleDiagError::InvalidSplit);
    }
    Ok(samples)
}
#[inline]
fn bit_truth(mask: u8, a: u64, b: u64) -> u64 {
    let mut out = 0u64;
    if mask & 1 != 0 {
        out |= !a & !b
    }
    if mask & 2 != 0 {
        out |= a & !b
    }
    if mask & 4 != 0 {
        out |= !a & b
    }
    if mask & 8 != 0 {
        out |= a & b
    }
    out
}
fn sort_sources(mut sources: [Source; 3]) -> Option<[Source; 3]> {
    sources.sort_unstable();
    if sources[0].gate == sources[1].gate || sources[1].gate == sources[2].gate {
        None
    } else {
        Some(sources)
    }
}
fn validate_rules(width: usize, rule: TripleRules) -> Result<(), TripleDiagError> {
    if !(4..=DIAG_MAX_WIDTH).contains(&width) {
        return Err(TripleDiagError::InvalidWidth);
    }
    let [a, b, c] = rule.rules;
    if a.gate == 0
        || a.gate >= b.gate
        || b.gate >= c.gate
        || c.gate >= width
        || !DIAG_RAW_POOL.contains(&a.raw)
        || !DIAG_RAW_POOL.contains(&b.raw)
        || !DIAG_RAW_POOL.contains(&c.raw)
        || a.truth >= 16
        || b.truth >= 16
        || c.truth >= 16
    {
        return Err(TripleDiagError::InvalidSplit);
    }
    Ok(())
}
pub fn make_three_native(
    width: usize,
    rule: TripleRules,
) -> Result<LocalAnfNetwork, TripleDiagError> {
    validate_rules(width, rule)?;
    Ok(make_multi_gate_native(width, &rule.rules)?)
}

/// Causal exact four-delay-segment score, one recycled N-word ring.
pub fn score_three(
    width: usize,
    rule: TripleRules,
    data: &LaneData,
    ring: &mut [u64],
) -> Result<u64, TripleDiagError> {
    validate_rules(width, rule)?;
    if ring.len() < width {
        return Err(TripleDiagError::InvalidSplit);
    }
    let [a, b, c] = rule.rules;
    let (len0, len1, len2, len3) = (a.gate, b.gate - a.gate, c.gate - b.gate, width - c.gate);
    let mut errors = 0u64;
    for block in &data.blocks {
        ring.fill(0);
        for t in 0..block.steps {
            let (p0, p1, p2, p3) = (
                t % len0,
                a.gate + t % len1,
                b.gate + t % len2,
                c.gate + t % len3,
            );
            let root = ring[p3] ^ block.inputs[t * DIAG_INPUT_BITS + 1];
            let next_a = bit_truth(a.truth, ring[p0], block.inputs[t * DIAG_INPUT_BITS + a.raw]);
            let next_b = bit_truth(b.truth, ring[p1], block.inputs[t * DIAG_INPUT_BITS + b.raw]);
            let next_c = bit_truth(c.truth, ring[p2], block.inputs[t * DIAG_INPUT_BITS + c.raw]);
            ring[p0] = root;
            ring[p1] = next_a;
            ring[p2] = next_b;
            ring[p3] = next_c;
            let target = block.targets[t] ^ block.inputs[t * DIAG_INPUT_BITS];
            errors += u64::from(((target ^ root) & block.valid).count_ones());
        }
    }
    Ok(errors)
}
fn observed(data: &LaneData) -> Vec<Observed<'_>> {
    data.blocks
        .iter()
        .map(|block| {
            let mut root = Vec::with_capacity(block.steps);
            let mut residual = Vec::with_capacity(block.steps);
            for t in 0..block.steps {
                let r = block.targets[t] ^ block.inputs[t * DIAG_INPUT_BITS];
                root.push(r);
                residual.push(r ^ block.inputs[t * DIAG_INPUT_BITS + 1]);
            }
            Observed {
                inputs: &block.inputs,
                valid: block.valid,
                root,
                residual,
            }
        })
        .collect()
}
#[inline]
fn feature_at(width: usize, source: Source, block: &Observed<'_>, t: usize) -> u64 {
    block.inputs[(t - (width - source.gate)) * DIAG_INPUT_BITS + source.raw]
}
fn walsh_score(
    width: usize,
    source: Source,
    blocks: &[Observed<'_>],
    peel: Option<Source>,
) -> (u64, u64) {
    let (mut raw, mut root, mut peeled, mut peeled_root) = (0i64, 0i64, 0i64, 0i64);
    for block in blocks {
        let n = i64::from(block.valid.count_ones());
        for t in width..block.root.len() {
            let x = feature_at(width, source, block, t);
            let r = block.root[t - width];
            let z = block.residual[t];
            let p = peel.map_or(z, |candidate| z ^ feature_at(width, candidate, block, t));
            let corr = |v: u64| n - 2 * i64::from((v & block.valid).count_ones());
            raw += corr(z ^ x);
            root += corr(z ^ x ^ r);
            peeled += corr(p ^ x);
            peeled_root += corr(p ^ x ^ r);
        }
    }
    let first = raw.unsigned_abs();
    (
        first
            .max(root.unsigned_abs())
            .max(peeled.unsigned_abs())
            .max(peeled_root.unsigned_abs()),
        first,
    )
}
fn rank_features(width: usize, blocks: &[Observed<'_>]) -> (Vec<RankedFeature>, Source) {
    let mut sources = Vec::with_capacity((width - 1) * DIAG_RAW_POOL.len());
    for gate in 1..width {
        for raw in DIAG_RAW_POOL {
            sources.push(Source { gate, raw })
        }
    }
    let strongest = sources
        .iter()
        .copied()
        .map(|s| (walsh_score(width, s, blocks, None).1, s))
        .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)))
        .map(|v| v.1)
        .unwrap();
    let mut ranked = sources
        .into_iter()
        .map(|source| {
            let (full, first) = walsh_score(width, source, blocks, Some(strongest));
            RankedFeature {
                source,
                walsh_absolute: full,
                raw_walsh_absolute: first,
            }
        })
        .collect::<Vec<_>>();
    ranked.sort_unstable_by(|a, b| {
        b.walsh_absolute
            .cmp(&a.walsh_absolute)
            .then_with(|| a.source.cmp(&b.source))
    });
    (ranked, strongest)
}
fn spectral_triplets(features: &[RankedFeature]) -> Vec<Candidate> {
    let n = features.len().min(DIAG_SCREEN_FEATURES);
    let mut out = Vec::with_capacity(n * (n - 1) * (n - 2) / 6);
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                if let Some(sources) =
                    sort_sources([features[i].source, features[j].source, features[k].source])
                {
                    out.push(Candidate {
                        sources,
                        spectral_sum: features[i].walsh_absolute
                            + features[j].walsh_absolute
                            + features[k].walsh_absolute,
                    });
                }
            }
        }
    }
    out.sort_unstable_by(|a, b| {
        b.spectral_sum
            .cmp(&a.spectral_sum)
            .then_with(|| a.sources.cmp(&b.sources))
    });
    out
}
fn binary_terms(mask: u8) -> usize {
    let [a, b, c, d] = [0, 1, 2, 3].map(|i| mask & (1 << i) != 0);
    usize::from(a) + usize::from(b ^ a) + usize::from(c ^ a) + usize::from(d ^ a ^ b ^ c)
}
fn composed_mask(truths: [u8; 3]) -> u16 {
    let [f, g, h] = truths;
    let mut mask = 0u16;
    for address in 0..16 {
        let (r, a, b, c) = (
            address & 1,
            (address >> 1) & 1,
            (address >> 2) & 1,
            (address >> 3) & 1,
        );
        let va = (f >> (r + 2 * a)) & 1;
        let vb = (g >> (usize::from(va) + 2 * b)) & 1;
        let vc = (h >> (usize::from(vb) + 2 * c)) & 1;
        mask |= u16::from(vc) << address;
    }
    mask
}
fn factor_bank() -> Vec<Factor> {
    let mut bank = vec![None::<Factor>; 1 << 16];
    for f in 0..16u8 {
        for g in 0..16u8 {
            for h in 0..16u8 {
                let truths = [f, g, h];
                let mask = composed_mask(truths);
                let terms = binary_terms(f) + binary_terms(g) + binary_terms(h);
                let candidate = Factor {
                    mask,
                    truths,
                    anf_terms: terms,
                };
                let entry = &mut bank[usize::from(mask)];
                if entry.is_none_or(|old| (terms, truths) < (old.anf_terms, old.truths)) {
                    *entry = Some(candidate)
                }
            }
        }
    }
    bank.into_iter().flatten().collect()
}
type Counts = [[u64; 2]; 16];
fn histograms(width: usize, sources: [Source; 3], blocks: &[Observed<'_>]) -> Counts {
    let mut counts = [[0u64; 2]; 16];
    for block in blocks {
        for t in width..block.root.len() {
            let bits = [
                block.root[t - width],
                feature_at(width, sources[0], block, t),
                feature_at(width, sources[1], block, t),
                feature_at(width, sources[2], block, t),
            ];
            for (address, row) in counts.iter_mut().enumerate() {
                let mut sample = block.valid;
                for (bit, &v) in bits.iter().enumerate() {
                    sample &= if address & (1 << bit) != 0 { v } else { !v };
                }
                let n = u64::from((sample & block.residual[t]).count_ones());
                row[1] += n;
                row[0] += u64::from(sample.count_ones()) - n;
            }
        }
    }
    counts
}
fn mask_loss(mask: u16, counts: &Counts) -> u64 {
    counts
        .iter()
        .enumerate()
        .map(|(address, row)| row[if mask & (1 << address) == 0 { 1 } else { 0 }])
        .sum()
}
fn best_factor(counts: &Counts, bank: &[Factor]) -> (Factor, u64) {
    let mut best = (bank[0], u64::MAX);
    for &candidate in bank {
        let loss = mask_loss(candidate.mask, counts);
        if (loss, candidate.anf_terms, candidate.truths) < (best.1, best.0.anf_terms, best.0.truths)
        {
            best = (candidate, loss);
        }
    }
    best
}
fn to_rules(sources: [Source; 3], truths: [u8; 3]) -> TripleRules {
    TripleRules {
        rules: std::array::from_fn(|i| IntermediateRule {
            gate: sources[i].gate,
            raw: sources[i].raw,
            truth: truths[i],
        }),
    }
}
fn evaluate_budget(
    budget: usize,
    width: usize,
    structures: &[Candidate],
    obs: &[Observed<'_>],
    bank: &[Factor],
    train: &LaneData,
    validation: &LaneData,
    reference: &LocalAnfNetwork,
    baseline_validation_errors: u64,
    ring: &mut [u64],
) -> Result<BudgetFit, TripleDiagError> {
    let mut proxies = Vec::with_capacity(budget);
    for &structure in structures.iter().take(budget) {
        let counts = histograms(width, structure.sources, obs);
        let (factor, teacher_loss) = best_factor(&counts, bank);
        proxies.push((teacher_loss, factor.anf_terms, structure, factor));
    }
    proxies.sort_unstable_by(|a, b| {
        (a.0, a.1, a.2.sources, a.3.truths).cmp(&(b.0, b.1, b.2.sources, b.3.truths))
    });
    let rescores = proxies.len().min(DIAG_NATIVE_RESCORING);
    let mut best: Option<Scored> = None;
    for &(teacher_loss, _, structure, factor) in proxies.iter().take(rescores) {
        let rules = to_rules(structure.sources, factor.truths);
        let native_loss = score_three(width, rules, train, ring)?;
        let trial = Scored {
            candidate: structure,
            factor,
            teacher_loss,
            native_loss,
        };
        if best.as_ref().is_none_or(|old| {
            (
                trial.native_loss,
                trial.teacher_loss,
                trial.factor.anf_terms,
                trial.candidate.sources,
                trial.factor.truths,
            ) < (
                old.native_loss,
                old.teacher_loss,
                old.factor.anf_terms,
                old.candidate.sources,
                old.factor.truths,
            )
        }) {
            best = Some(trial)
        }
    }
    let best = best.ok_or(TripleDiagError::InvalidBudget)?;
    let rules = to_rules(best.candidate.sources, best.factor.truths);
    let proposal = make_three_native(width, rules)?;
    if native_errors(&proposal, train)? != best.native_loss {
        return Err(TripleDiagError::ScoreMismatch);
    }
    let val_error = native_errors(&proposal, validation)?;
    let accepted = val_error < baseline_validation_errors;
    Ok(BudgetFit {
        budget,
        structures_full_train_scored: budget,
        native_train_rescores: rescores,
        proposal_sources: best.candidate.sources,
        proposal_rules: rules,
        proposal: proposal.clone(),
        committed: if accepted {
            proposal
        } else {
            reference.clone()
        },
        composed_truth: best.factor.mask,
        teacher_forced_train_errors: best.teacher_loss,
        autonomous_train_errors: best.native_loss,
        autonomous_validation_errors: val_error,
        baseline_validation_errors,
        accepted,
    })
}
/// Learner API: no oracle hidden state parameters and no held-out tests.
pub fn fit_three_gate_diagnostic(
    train: &LaneData,
    validation: &LaneData,
    width: usize,
    budgets: &[usize],
) -> Result<TripleDiagReport, TripleDiagError> {
    let train_samples = validate(width, train, true)?;
    let validation_samples = validate(width, validation, false)?;
    if budgets.is_empty()
        || budgets.windows(2).any(|v| v[0] >= v[1])
        || budgets.iter().any(|&v| v == 0)
    {
        return Err(TripleDiagError::InvalidBudget);
    }
    let obs = observed(train);
    let (ranked, strongest) = rank_features(width, &obs);
    let structures = spectral_triplets(&ranked);
    if structures.len() < *budgets.last().ok_or(TripleDiagError::InvalidBudget)? {
        return Err(TripleDiagError::InvalidBudget);
    }
    let bank = factor_bank();
    if bank.is_empty() {
        return Err(TripleDiagError::FactorizationEmpty);
    }
    let reference = make_multi_gate_native(width, &[])?;
    let baseline_train_errors = native_errors(&reference, train)?;
    let baseline_validation_errors = native_errors(&reference, validation)?;
    let mut ring = vec![0u64; width];
    let mut fitted = Vec::with_capacity(budgets.len());
    for &budget in budgets {
        fitted.push(evaluate_budget(
            budget,
            width,
            &structures,
            &obs,
            &bank,
            train,
            validation,
            &reference,
            baseline_validation_errors,
            &mut ring,
        )?);
    }
    Ok(TripleDiagReport {
        width,
        train_samples,
        validation_samples,
        baseline_train_errors,
        baseline_validation_errors,
        ranked_features: ranked,
        strongest_univariate: strongest,
        all_lag_feature_count: (width - 1) * DIAG_RAW_POOL.len(),
        spectral_triplet_pool_count: structures.len(),
        distinct_factorizable_masks: bank.len(),
        screen_features: DIAG_SCREEN_FEATURES,
        budgets: fitted,
        scratch_ring_bytes: width * std::mem::size_of::<u64>(),
    })
}
/// Oracle-backed retrospective diagnostics only, NEVER inputs to fit.
pub fn retrospective_recall(
    teacher: TripleRules,
    report: &TripleDiagReport,
) -> (usize, usize, usize) {
    let expected = teacher.sources();
    let count = |n: usize| {
        expected
            .iter()
            .filter(|&&source| {
                report
                    .ranked_features
                    .iter()
                    .take(n)
                    .any(|candidate| candidate.source == source)
            })
            .count()
    };
    (count(8), count(DIAG_SCREEN_FEATURES), count(80))
}
/// Only post-hoc: was the true 3-source structure within each TRAIN budget?
pub fn retrospective_budget(
    teacher: TripleRules,
    train: &LaneData,
    width: usize,
    budgets: &[usize],
) -> Result<Vec<bool>, TripleDiagError> {
    validate(width, train, true)?;
    let obs = observed(train);
    let (ranked, _) = rank_features(width, &obs);
    let pool = spectral_triplets(&ranked);
    Ok(budgets
        .iter()
        .map(|&k| {
            pool.iter()
                .take(k)
                .any(|candidate| candidate.sources == teacher.sources())
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{anf_local_incremental::LaneBlock, toy_tasks::SplitMix64};

    fn rules() -> TripleRules {
        to_rules(
            [
                Source { gate: 7, raw: 2 },
                Source { gate: 19, raw: 3 },
                Source { gate: 27, raw: 4 },
            ],
            [6, 8, 6],
        )
    }
    fn data(seed: u64, width: usize, steps: usize, rules: TripleRules) -> LaneData {
        let net = make_three_native(width, rules).unwrap();
        let mut rng = SplitMix64::new(seed);
        let mut inputs = vec![0u64; steps * DIAG_INPUT_BITS];
        for word in &mut inputs {
            *word = rng.next_u64();
        }
        let mut source = vec![0u64; net.source_width()];
        let mut next = vec![0u64; width];
        let mut products = vec![0u64; net.scratch_len()];
        let mut targets = vec![0u64; steps];
        for (t, target) in targets.iter_mut().enumerate() {
            let from = t * DIAG_INPUT_BITS;
            source[width..].copy_from_slice(&inputs[from..from + DIAG_INPUT_BITS]);
            net.step_lanes(&mut source, &mut next, &mut products);
            *target = net.readout().evaluate_lanes(&source, &mut products);
        }
        LaneData {
            input_bits: DIAG_INPUT_BITS,
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
    fn four_thousand_ninety_six_factorizations_have_520_distinct_masks() {
        let bank = factor_bank();
        assert_eq!(bank.len(), 520);
        assert!(bank.iter().any(|f| f.mask == composed_mask([6, 8, 6])));
    }
    #[test]
    fn four_segment_scorer_matches_native_across_truth_compositions() {
        let width = 32;
        let mut ring = vec![0u64; width];
        for f in 0..16u8 {
            for g in [0u8, 2, 6, 8, 9, 14, 15] {
                for h in [0u8, 6, 8, 14] {
                    let rule = to_rules(rules().sources(), [f, g, h]);
                    let train = data(
                        123 + u64::from(f) * 256 + u64::from(g) * 16 + u64::from(h),
                        width,
                        96,
                        rule,
                    );
                    assert_eq!(score_three(width, rule, &train, &mut ring).unwrap(), 0);
                    assert_eq!(
                        native_errors(&make_three_native(width, rule).unwrap(), &train).unwrap(),
                        0
                    );
                }
            }
        }
    }
    #[test]
    fn registered_diagnostic_budgets_are_exercised_on_small_ring() {
        let train = data(45, 32, 512, rules());
        let val = data(46, 32, 128, rules());
        let result = fit_three_gate_diagnostic(&train, &val, 32, &DIAG_BUDGETS).unwrap();
        assert_eq!(result.all_lag_feature_count, 124);
        assert_eq!(result.distinct_factorizable_masks, 520);
        assert_eq!(result.budgets[0].structures_full_train_scored, 64);
        assert_eq!(result.budgets[1].structures_full_train_scored, 640);
        assert_eq!(result.train_samples, 512 * 64);
    }
    #[test]
    fn invalid_data_and_budget_are_rejected() {
        let train = data(47, 32, 128, rules());
        let val = data(48, 32, 128, rules());
        assert_eq!(
            fit_three_gate_diagnostic(&train, &val, 32, &[640, 64]),
            Err(TripleDiagError::InvalidBudget)
        );
        let mut broken = train.clone();
        broken.blocks[0].targets.clear();
        assert_eq!(
            fit_three_gate_diagnostic(&broken, &val, 32, &DIAG_BUDGETS),
            Err(TripleDiagError::InvalidSplit)
        );
    }
}
