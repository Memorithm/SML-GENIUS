//! A6-E5 reference: bounded partial Walsh projection and heldout-only
//! conjunction beam proposal. No 2^32 table, no TEST or oracle access.
//!
//! Residual r = onehot - p_NG is the NEGATIVE of the cross-entropy logit
//! gradient, and a positive likelihood improvement is log(p_new/p_base).
//! The structural code length below is a declared penalty, not an exact MDL
//! code. This reference is NOT a qualified Flaubert language benchmark.

use std::collections::BTreeSet;

use crate::anf_wide_sparse::{WideAnfError, WideAnfGate, WideAnfLimits};

pub const MAX_WALSH_MASKS: usize = 96;
pub const MAX_WALSH_ROWS: usize = 8192;
pub const MAX_CLASSES: usize = 256;
pub const MAX_BEAM_WIDTH: usize = 32;
pub const MAX_PARITY_TERMS: usize = 3;
pub const MAX_GATE_FAN_IN: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpectralError {
    InvalidConfiguration,
    InvalidMask,
    DuplicateMask,
    InvalidProbability,
    InvalidTarget,
    SampleBudget,
    EmptyData,
    InvalidFeatures,
    ResourceBudget,
    NativeGate(WideAnfError),
}

impl std::fmt::Display for SpectralError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SpectralError {}

impl From<WideAnfError> for SpectralError {
    fn from(value: WideAnfError) -> Self {
        Self::NativeGate(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalshCandidate {
    pub mask: u32,
    pub residual_l2: f64,
}

/// Streaming 256-way class-conditioned Walsh residual projection.
/// Memory is O(number_of_masks * classes), not O(2^source_width).
#[derive(Clone, Debug)]
pub struct WalshResidualProbe {
    masks: Vec<u32>,
    classes: usize,
    row_limit: usize,
    rows: usize,
    moments: Vec<f64>,
}

impl WalshResidualProbe {
    pub fn new(masks: &[u32], classes: usize, row_limit: usize) -> Result<Self, SpectralError> {
        if masks.is_empty()
            || masks.len() > MAX_WALSH_MASKS
            || !(2..=MAX_CLASSES).contains(&classes)
            || row_limit == 0
            || row_limit > MAX_WALSH_ROWS
            || masks.contains(&0)
        {
            return Err(SpectralError::InvalidConfiguration);
        }
        let mut distinct = BTreeSet::new();
        for &mask in masks {
            if !distinct.insert(mask) {
                return Err(SpectralError::DuplicateMask);
            }
        }
        Ok(Self {
            masks: masks.to_vec(),
            classes,
            row_limit,
            rows: 0,
            moments: vec![0.0f64; masks.len() * classes],
        })
    }

    /// Requires OOF probabilities from an NG model which has never trained on
    /// this observation's label. Validate that promise at the CALL SITE.
    pub fn observe(
        &mut self,
        physical_bits: u32,
        truth: u8,
        baseline: &[f64],
    ) -> Result<(), SpectralError> {
        if self.rows >= self.row_limit {
            return Err(SpectralError::SampleBudget);
        }
        if usize::from(truth) >= self.classes {
            return Err(SpectralError::InvalidTarget);
        }
        if baseline.len() != self.classes
            || baseline
                .iter()
                .any(|&p| !p.is_finite() || !(0.0..=1.0).contains(&p))
            || (baseline.iter().sum::<f64>() - 1.0).abs() > 1.0e-9
        {
            return Err(SpectralError::InvalidProbability);
        }

        for (slot, &mask) in self.masks.iter().enumerate() {
            let sign = if (physical_bits & mask).count_ones() % 2 == 0 {
                1.0
            } else {
                -1.0
            };
            let moment = &mut self.moments[slot * self.classes..(slot + 1) * self.classes];
            for (class, (&p, sum)) in baseline.iter().zip(moment.iter_mut()).enumerate() {
                *sum += sign * (f64::from(u8::from(class == usize::from(truth))) - p);
            }
        }
        self.rows += 1;
        Ok(())
    }

    pub fn samples(&self) -> usize {
        self.rows
    }

    pub fn ranked(&self) -> Result<Vec<WalshCandidate>, SpectralError> {
        if self.rows == 0 {
            return Err(SpectralError::EmptyData);
        }
        let n = self.rows as f64;
        let mut ranked = self
            .masks
            .iter()
            .enumerate()
            .map(|(index, &mask)| {
                let rms = self.moments[index * self.classes..(index + 1) * self.classes]
                    .iter()
                    .map(|&v| (v / n).powi(2))
                    .sum::<f64>()
                    .sqrt();
                WalshCandidate {
                    mask,
                    residual_l2: rms,
                }
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|a, b| {
            b.residual_l2
                .total_cmp(&a.residual_l2)
                .then_with(|| a.mask.cmp(&b.mask))
        });
        Ok(ranked)
    }
}

pub fn parity(physical_bits: u32, mask: u32) -> bool {
    (physical_bits & mask).count_ones() % 2 == 1
}

/// AND of XOR parities. This is a Boolean function, NOT a direct monomial;
/// GF(2) Mobius compilation is used separately for a native ANF oracle.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConjoinedParities {
    masks: Vec<u32>,
}

impl ConjoinedParities {
    pub fn new(mut masks: Vec<u32>) -> Result<Self, SpectralError> {
        if masks.is_empty() || masks.len() > MAX_PARITY_TERMS || masks.contains(&0) {
            return Err(SpectralError::InvalidConfiguration);
        }
        masks.sort_unstable();
        if masks.windows(2).any(|p| p[0] == p[1]) {
            return Err(SpectralError::DuplicateMask);
        }
        Ok(Self { masks })
    }

    pub fn masks(&self) -> &[u32] {
        &self.masks
    }

    pub fn evaluate(&self, physical_bits: u32) -> bool {
        self.masks.iter().all(|&mask| parity(physical_bits, mask))
    }

    pub fn used_variables(&self) -> u32 {
        self.masks.iter().copied().fold(0u32, |a, b| a | b)
    }

    /// Declared *regularizer*, not a complete universal two-part MDL code.
    /// Bits: number of terms, seven-bit IDs per candidate from a 96-mask
    /// dictionary, active input coordinates, and coarse byte quantization
    /// for two 256-class conditional count sets.
    pub fn code_length_bits(&self, classes: usize) -> usize {
        1 + 7 * self.masks.len()
            + self.used_variables().count_ones() as usize
            + (2 * classes).div_ceil(8)
    }

    /// GF(2) Mobius conversion of the exact finite Boolean truth table.
    /// The eager oracle deliberately refuses >12 physical inputs: no
    /// exponential materialization at arbitrary width.
    pub fn compile_native(&self) -> Result<WideAnfGate, SpectralError> {
        let used = self.used_variables();
        let sources = (0..32)
            .filter(|&bit| used & (1u32 << bit) != 0)
            .collect::<Vec<_>>();
        if sources.is_empty() || sources.len() > MAX_GATE_FAN_IN {
            return Err(SpectralError::ResourceBudget);
        }
        let width = sources.len();
        let mut truth = Vec::with_capacity(1usize << width);
        for assignment in 0..(1usize << width) {
            let mut source = 0u32;
            for (local, &physical) in sources.iter().enumerate() {
                source |= u32::from(assignment & (1 << local) != 0) << physical;
            }
            truth.push(self.evaluate(source));
        }
        let mut coefficients = truth.clone();
        for bit in 0..width {
            for assignment in 0..(1usize << width) {
                if assignment & (1 << bit) != 0 {
                    let below = coefficients[assignment ^ (1 << bit)];
                    coefficients[assignment] ^= below;
                }
            }
        }
        let monomials = coefficients
            .iter()
            .enumerate()
            .filter_map(|(mask, &active)| {
                active.then(|| {
                    (0..width)
                        .filter(|&bit| mask & (1 << bit) != 0)
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let gate = WideAnfGate::from_monomials(
            sources.clone(),
            monomials,
            32,
            WideAnfLimits::new(12, 4096, 49_152, 4096, 49_152),
        )?;
        for (assignment, &expected) in truth.iter().enumerate() {
            let mut bits = [false; 32];
            for (local, &physical) in sources.iter().enumerate() {
                bits[physical] = assignment & (1 << local) != 0;
            }
            if gate.evaluate_scalar(&bits)? != expected {
                return Err(SpectralError::InvalidFeatures);
            }
        }
        Ok(gate)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GateRow {
    pub context: u32,
    pub truth: u8,
}

/// This must be an *inner-TRAIN* heldout row. The input only contains the
/// NG true-class probability, preventing this primitive from peeking at
/// any future/outer-TEST data. Callers must enforce chronological provenance.
#[derive(Clone, Copy, Debug)]
pub struct InnerValidationRow {
    pub context: u32,
    pub truth: u8,
    pub baseline_truth: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeamConfig {
    pub classes: usize,
    pub beam_width: usize,
    pub seed_pool: usize,
    pub max_depth: usize,
    pub alpha: f64,
    pub mixture_lambda: f64,
    pub gamma: f64,
    pub max_fit_rows: usize,
    pub max_inner_val_rows: usize,
    pub max_evaluations: usize,
}

impl Default for BeamConfig {
    fn default() -> Self {
        Self {
            classes: 256,
            beam_width: 16,
            seed_pool: 24,
            max_depth: 3,
            alpha: 4.0,
            mixture_lambda: 0.1,
            gamma: 1.0,
            max_fit_rows: 600_000,
            max_inner_val_rows: 150_000,
            max_evaluations: 1600,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CandidateScore {
    /// Natural log-likelihood improvement summed over inner TRAIN heldout.
    pub delta_log_nats: f64,
    pub penalty_nats: f64,
    pub adjusted_nats: f64,
}

fn score_gate(
    gate: &ConjoinedParities,
    fit: &[GateRow],
    validation: &[InnerValidationRow],
    cfg: BeamConfig,
) -> Result<CandidateScore, SpectralError> {
    let mut counts = vec![0u32; 2 * cfg.classes];
    let mut totals = [0u32; 2];
    for &row in fit {
        if usize::from(row.truth) >= cfg.classes {
            return Err(SpectralError::InvalidTarget);
        }
        let state = usize::from(gate.evaluate(row.context));
        totals[state] += 1;
        counts[state * cfg.classes + usize::from(row.truth)] += 1;
    }
    let mut improvement = 0.0;
    for &row in validation {
        if usize::from(row.truth) >= cfg.classes {
            return Err(SpectralError::InvalidTarget);
        }
        let baseline = row.baseline_truth;
        if !baseline.is_finite() || baseline <= 0.0 || baseline > 1.0 {
            return Err(SpectralError::InvalidProbability);
        }
        let state = usize::from(gate.evaluate(row.context));
        let denom = f64::from(totals[state]) + cfg.alpha * cfg.classes as f64;
        let local =
            (f64::from(counts[state * cfg.classes + usize::from(row.truth)]) + cfg.alpha) / denom;
        let hybrid = (1.0 - cfg.mixture_lambda) * baseline + cfg.mixture_lambda * local;
        improvement += (hybrid / baseline).ln();
    }
    let penalty = cfg.gamma * gate.code_length_bits(cfg.classes) as f64 * std::f64::consts::LN_2;
    Ok(CandidateScore {
        delta_log_nats: improvement,
        penalty_nats: penalty,
        adjusted_nats: improvement - penalty,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeamResult {
    /// None is a valid falsification: no gate cleared the declared MDL test.
    pub selected: Option<ConjoinedParities>,
    pub selected_score: Option<CandidateScore>,
    pub screened: usize,
    pub max_depth_visited: usize,
}

/// Bounded discovery: spectra propose *parity masks*, heldout log-loss and
/// MDL accept *ANDs of parities*. The frontier may retain negative-net
/// singletons to discover non-marginal two/three-way interactions.
pub fn discover_beam(
    fit: &[GateRow],
    inner_validation: &[InnerValidationRow],
    ranked_walsh: &[WalshCandidate],
    cfg: BeamConfig,
) -> Result<BeamResult, SpectralError> {
    if !(2..=MAX_CLASSES).contains(&cfg.classes)
        || cfg.beam_width == 0
        || cfg.beam_width > MAX_BEAM_WIDTH
        || cfg.seed_pool == 0
        || cfg.seed_pool > 24
        || cfg.max_depth == 0
        || cfg.max_depth > MAX_PARITY_TERMS
        || cfg.max_evaluations == 0
        || !(cfg.alpha > 0.0 && cfg.alpha.is_finite())
        || !(cfg.gamma >= 0.0 && cfg.gamma.is_finite())
        || !cfg.mixture_lambda.is_finite()
        || !(0.0..=1.0).contains(&cfg.mixture_lambda)
    {
        return Err(SpectralError::InvalidConfiguration);
    }
    if fit.is_empty() || inner_validation.is_empty() || ranked_walsh.is_empty() {
        return Err(SpectralError::EmptyData);
    }
    if fit.len() > cfg.max_fit_rows
        || inner_validation.len() > cfg.max_inner_val_rows
        || fit.len() > u32::MAX as usize
    {
        return Err(SpectralError::SampleBudget);
    }

    let mut ranked = ranked_walsh.to_vec();
    if ranked.len() > MAX_WALSH_MASKS
        || ranked
            .iter()
            .any(|r| r.mask == 0 || !r.residual_l2.is_finite() || r.residual_l2 < 0.0)
    {
        return Err(SpectralError::InvalidMask);
    }
    let mut used = BTreeSet::new();
    if ranked.iter().any(|r| !used.insert(r.mask)) {
        return Err(SpectralError::DuplicateMask);
    }
    ranked.sort_by(|a, b| {
        b.residual_l2
            .total_cmp(&a.residual_l2)
            .then_with(|| a.mask.cmp(&b.mask))
    });
    let seeds = ranked
        .iter()
        .take(cfg.seed_pool)
        .map(|r| r.mask)
        .collect::<Vec<_>>();
    let mut frontier = seeds
        .iter()
        .map(|&mask| ConjoinedParities::new(vec![mask]))
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen = BTreeSet::new();
    let mut winning: Option<(ConjoinedParities, CandidateScore)> = None;
    let mut evaluations = 0usize;
    let mut visited_depth = 0usize;

    for depth in 1..=cfg.max_depth {
        visited_depth = depth;
        let mut measured = Vec::new();
        for gate in frontier {
            if !seen.insert(gate.clone()) {
                continue;
            }
            if evaluations >= cfg.max_evaluations {
                return Err(SpectralError::ResourceBudget);
            }
            let score = score_gate(&gate, fit, inner_validation, cfg)?;
            evaluations += 1;
            if score.adjusted_nats > 0.0
                && winning.as_ref().is_none_or(|(best_gate, best)| {
                    score.adjusted_nats > best.adjusted_nats + 1.0e-12
                        || ((score.adjusted_nats - best.adjusted_nats).abs() <= 1.0e-12
                            && gate < *best_gate)
                })
            {
                winning = Some((gate.clone(), score));
            }
            measured.push((gate, score));
        }
        measured.sort_by(|left, right| {
            right
                .1
                .adjusted_nats
                .total_cmp(&left.1.adjusted_nats)
                .then_with(|| left.0.cmp(&right.0))
        });
        measured.truncate(cfg.beam_width);
        if depth >= cfg.max_depth {
            break;
        }
        let mut successors = BTreeSet::new();
        for (gate, _) in &measured {
            let last = *gate.masks().last().ok_or(SpectralError::EmptyData)?;
            for &mask in &seeds {
                if mask > last {
                    let mut next = gate.masks().to_vec();
                    next.push(mask);
                    successors.insert(ConjoinedParities::new(next)?);
                }
            }
        }
        if successors.is_empty() {
            break;
        }
        frontier = successors.into_iter().collect();
    }

    let (selected, selected_score) = winning.map_or((None, None), |(a, b)| (Some(a), Some(b)));
    Ok(BeamResult {
        selected,
        selected_score,
        screened: evaluations,
        max_depth_visited: visited_depth,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correct_logit_residual_recovers_hidden_xor_not_individual_bits() {
        let mut probe = WalshResidualProbe::new(&[1, 2, 3], 2, 128).unwrap();
        for _ in 0..16 {
            for x in 0..4u32 {
                let truth = u8::from(parity(x, 3));
                probe.observe(x, truth, &[0.5, 0.5]).unwrap();
            }
        }
        assert_eq!(probe.samples(), 64);
        let ranked = probe.ranked().unwrap();
        assert_eq!(ranked[0].mask, 3);
        assert!(ranked[0].residual_l2 > 0.70);
        assert!(ranked[1].residual_l2.abs() < 1.0e-12);
    }

    #[test]
    fn rejects_bad_probabilities_and_sample_overflow_without_modification() {
        let mut probe = WalshResidualProbe::new(&[1, 3], 2, 1).unwrap();
        assert_eq!(
            probe.observe(1, 1, &[0.2, 0.2]),
            Err(SpectralError::InvalidProbability)
        );
        assert_eq!(probe.samples(), 0);
        probe.observe(1, 1, &[0.4, 0.6]).unwrap();
        assert_eq!(
            probe.observe(0, 0, &[0.5, 0.5]),
            Err(SpectralError::SampleBudget)
        );
    }

    #[test]
    fn conjoined_xors_are_exactly_compiled_through_mobius_into_native_gate() {
        let gates = [
            ConjoinedParities::new(vec![1, 2]).unwrap(),
            ConjoinedParities::new(vec![3, 7]).unwrap(),
            ConjoinedParities::new(vec![0x101, 0x202, 0x404]).unwrap(),
        ];
        for expr in gates {
            let native = expr.compile_native().unwrap();
            for x in [0u32, 1, 2, 3, 7, 0x101, 0x707, u32::MAX] {
                let bits = std::array::from_fn::<_, 32, _>(|b| x & (1 << b) != 0);
                assert_eq!(native.evaluate_scalar(&bits).unwrap(), expr.evaluate(x));
            }
        }
    }

    #[test]
    fn beam_keeps_weak_singles_then_finds_joint_and_on_inner_train_holdout() {
        let mut fit = Vec::new();
        let mut val = Vec::new();
        for _ in 0..64 {
            for x in 0..4u32 {
                let truth = u8::from((x & 3) == 3);
                fit.push(GateRow { context: x, truth });
                val.push(InnerValidationRow {
                    context: x,
                    truth,
                    baseline_truth: 0.5,
                });
            }
        }
        let masks = [
            WalshCandidate {
                mask: 1,
                residual_l2: 0.3,
            },
            WalshCandidate {
                mask: 2,
                residual_l2: 0.3,
            },
        ];
        let cfg = BeamConfig {
            classes: 2,
            beam_width: 2,
            seed_pool: 2,
            max_depth: 2,
            alpha: 0.5,
            mixture_lambda: 1.0,
            gamma: 1.0,
            ..BeamConfig::default()
        };
        let result = discover_beam(&fit, &val, &masks, cfg).unwrap();
        assert_eq!(result.screened, 3);
        assert_eq!(result.selected.unwrap().masks(), &[1, 2]);
        assert!(result.selected_score.unwrap().adjusted_nats > 0.0);
    }

    #[test]
    fn null_conditional_data_must_not_manufacture_mdl_gain() {
        let mut fit = Vec::new();
        let mut val = Vec::new();
        for _ in 0..48 {
            for x in 0..4u32 {
                for truth in [0u8, 1u8] {
                    fit.push(GateRow { context: x, truth });
                    val.push(InnerValidationRow {
                        context: x,
                        truth,
                        baseline_truth: 0.5,
                    });
                }
            }
        }
        let masks = [
            WalshCandidate {
                mask: 1,
                residual_l2: 0.0,
            },
            WalshCandidate {
                mask: 2,
                residual_l2: 0.0,
            },
        ];
        let cfg = BeamConfig {
            classes: 2,
            beam_width: 2,
            seed_pool: 2,
            max_depth: 2,
            alpha: 0.5,
            mixture_lambda: 1.0,
            gamma: 1.0,
            ..BeamConfig::default()
        };
        let result = discover_beam(&fit, &val, &masks, cfg).unwrap();
        assert!(result.selected.is_none(), "a null feature should fail MDL");
    }

    #[test]
    fn rejects_invalid_beam_configuration_and_unseen_labels() {
        let cfg = BeamConfig {
            classes: 2,
            seed_pool: 0,
            ..BeamConfig::default()
        };
        let gate = [WalshCandidate {
            mask: 1,
            residual_l2: 0.1,
        }];
        assert_eq!(
            discover_beam(
                &[GateRow {
                    context: 0,
                    truth: 0
                }],
                &[InnerValidationRow {
                    context: 0,
                    truth: 0,
                    baseline_truth: 0.5
                }],
                &gate,
                cfg
            ),
            Err(SpectralError::InvalidConfiguration)
        );
    }
}
