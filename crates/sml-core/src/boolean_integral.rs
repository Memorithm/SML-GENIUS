//! Gradient-free Boolean credit integration for native SML learning.
//!
//! The mechanism records the effect of explicit XOR interventions over a hard
//! Boolean parameter state. No derivative, floating-point latent weight, or
//! soft surrogate is required by this module.
//!
//! For one intervention mask M and integer loss L, the discrete evidence is:
//!
//! `delta(M) = L(base) - L(base XOR M)`
//!
//! Positive delta means the intervention improved the objective. Its magnitude
//! is divided across the bits in the mask before being accumulated. Repeated,
//! overlapping masks therefore build a discrete evidence integral for each bit.
//!
//! Multi-bit interventions are intentionally treated as shared evidence rather
//! than proof of an individual causal effect. Production training must use
//! repeated diverse masks and held-out verification before committing flips.

use core::fmt;

/// Canonical parts-per-million loss and confidence scale.
pub const BOOLEAN_INTEGRAL_SCALE_PPM: u32 = 1_000_000;

/// Controls bounded evidence accumulation and flip decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanIntegralPolicy {
    /// Minimum number of interventions touching a bit before any decision.
    pub min_observations: u32,
    /// Minimum absolute normalized evidence required for a decision.
    pub min_confidence_ppm: u32,
    /// Hard bound on the number of bits changed by one intervention.
    pub max_mask_width: u32,
}

impl Default for BooleanIntegralPolicy {
    fn default() -> Self {
        Self {
            min_observations: 3,
            min_confidence_ppm: 250_000,
            max_mask_width: 4_096,
        }
    }
}

impl BooleanIntegralPolicy {
    pub fn validate(self) -> Result<Self, BooleanIntegralError> {
        if self.min_observations == 0 {
            return Err(BooleanIntegralError::ZeroMinimumObservations);
        }
        if self.min_confidence_ppm > BOOLEAN_INTEGRAL_SCALE_PPM {
            return Err(BooleanIntegralError::ConfidenceOutOfRange(
                self.min_confidence_ppm,
            ));
        }
        if self.max_mask_width == 0 {
            return Err(BooleanIntegralError::ZeroMaximumMaskWidth);
        }
        Ok(self)
    }
}

/// One hard XOR intervention and its measured objective values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooleanIntervention {
    /// Parameter indexes toggled by XOR for the perturbed evaluation.
    pub mask: Vec<u32>,
    /// Objective before applying the mask.
    pub baseline_loss_ppm: u32,
    /// Objective after applying the mask.
    pub perturbed_loss_ppm: u32,
}

/// Exact accumulated evidence for one Boolean parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BooleanCreditCell {
    pub observations: u32,
    pub improving_observations: u32,
    pub worsening_observations: u32,
    pub neutral_observations: u32,
    /// Shared positive loss reduction mass after mask-width normalization.
    pub positive_mass: u64,
    /// Shared negative loss increase mass after mask-width normalization.
    pub negative_mass: u64,
}

impl BooleanCreditCell {
    /// Signed normalized evidence in [-1_000_000, +1_000_000].
    #[must_use]
    pub fn signed_confidence_ppm(self) -> i32 {
        let total = self.positive_mass as u128 + self.negative_mass as u128;
        if total == 0 {
            return 0;
        }
        let net = self.positive_mass as i128 - self.negative_mass as i128;
        let scaled = net * BOOLEAN_INTEGRAL_SCALE_PPM as i128 / total as i128;
        scaled.clamp(
            -(BOOLEAN_INTEGRAL_SCALE_PPM as i128),
            BOOLEAN_INTEGRAL_SCALE_PPM as i128,
        ) as i32
    }

    #[must_use]
    pub fn decision(self, policy: BooleanIntegralPolicy) -> BooleanIntegralDecision {
        if self.observations < policy.min_observations {
            return BooleanIntegralDecision::Undecided;
        }
        let confidence = self.signed_confidence_ppm();
        let threshold = policy.min_confidence_ppm as i32;
        if confidence >= threshold {
            BooleanIntegralDecision::Flip
        } else if confidence <= -threshold {
            BooleanIntegralDecision::Keep
        } else {
            BooleanIntegralDecision::Undecided
        }
    }
}

/// Decision produced from integrated evidence for the current Boolean state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanIntegralDecision {
    /// Evidence supports toggling the current bit.
    Flip,
    /// Evidence supports retaining the current bit.
    Keep,
    /// Evidence is insufficient or contradictory.
    Undecided,
}

/// Exact reference ledger for discrete Boolean intervention evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooleanIntegralAccumulator {
    cells: Vec<BooleanCreditCell>,
    policy: BooleanIntegralPolicy,
    interventions: u64,
}

impl BooleanIntegralAccumulator {
    pub fn new(
        parameter_count: usize,
        policy: BooleanIntegralPolicy,
    ) -> Result<Self, BooleanIntegralError> {
        if parameter_count == 0 {
            return Err(BooleanIntegralError::ZeroParameters);
        }
        Ok(Self {
            cells: vec![BooleanCreditCell::default(); parameter_count],
            policy: policy.validate()?,
            interventions: 0,
        })
    }

    #[must_use]
    pub fn parameter_count(&self) -> usize {
        self.cells.len()
    }

    #[must_use]
    pub const fn interventions(&self) -> u64 {
        self.interventions
    }

    pub fn cell(&self, index: usize) -> Result<BooleanCreditCell, BooleanIntegralError> {
        self.cells
            .get(index)
            .copied()
            .ok_or(BooleanIntegralError::ParameterOutOfRange(index))
    }

    /// Adds one intervention to the discrete integral.
    ///
    /// Evidence magnitude is divided by mask width so a wider intervention does
    /// not automatically dominate a narrow one. This is shared evidence: it is
    /// not claimed to identify an individual effect from one grouped experiment.
    pub fn observe(
        &mut self,
        intervention: &BooleanIntervention,
    ) -> Result<(), BooleanIntegralError> {
        validate_loss(intervention.baseline_loss_ppm)?;
        validate_loss(intervention.perturbed_loss_ppm)?;

        if intervention.mask.is_empty() {
            return Err(BooleanIntegralError::EmptyMask);
        }
        if intervention.mask.len() > self.policy.max_mask_width as usize {
            return Err(BooleanIntegralError::MaskTooWide {
                actual: intervention.mask.len(),
                maximum: self.policy.max_mask_width as usize,
            });
        }

        let mut indexes: Vec<usize> = intervention
            .mask
            .iter()
            .map(|index| *index as usize)
            .collect();
        indexes.sort_unstable();

        for pair in indexes.windows(2) {
            if pair[0] == pair[1] {
                return Err(BooleanIntegralError::DuplicateMaskIndex(pair[0]));
            }
        }
        for index in &indexes {
            if *index >= self.cells.len() {
                return Err(BooleanIntegralError::ParameterOutOfRange(*index));
            }
        }

        let baseline = intervention.baseline_loss_ppm as i64;
        let perturbed = intervention.perturbed_loss_ppm as i64;
        let delta = baseline - perturbed;
        let width = indexes.len() as u64;
        let magnitude = delta.unsigned_abs();
        let shared_mass = if magnitude == 0 {
            0
        } else {
            magnitude.div_ceil(width)
        };

        for index in indexes {
            let cell = &mut self.cells[index];
            cell.observations = cell.observations.saturating_add(1);
            if delta > 0 {
                cell.improving_observations = cell.improving_observations.saturating_add(1);
                cell.positive_mass = cell.positive_mass.saturating_add(shared_mass);
            } else if delta < 0 {
                cell.worsening_observations = cell.worsening_observations.saturating_add(1);
                cell.negative_mass = cell.negative_mass.saturating_add(shared_mass);
            } else {
                cell.neutral_observations = cell.neutral_observations.saturating_add(1);
            }
        }

        self.interventions = self.interventions.saturating_add(1);
        Ok(())
    }

    /// Returns only indexes whose integrated evidence clears the flip policy.
    #[must_use]
    pub fn recommended_flips(&self) -> Vec<usize> {
        self.cells
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, cell)| {
                (cell.decision(self.policy) == BooleanIntegralDecision::Flip).then_some(index)
            })
            .collect()
    }
}

/// Four-bit saturating runtime credit ledger.
///
/// Two signed scores are stored per byte. Score 0 is encoded as nibble 7;
/// representable scores are -7..=+7. This is intended for active routed weights,
/// not as a mandatory 25B-resident global structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedBooleanCreditLedger {
    bytes: Vec<u8>,
    len: usize,
}

impl PackedBooleanCreditLedger {
    pub fn new(parameter_count: usize) -> Result<Self, BooleanIntegralError> {
        if parameter_count == 0 {
            return Err(BooleanIntegralError::ZeroParameters);
        }
        Ok(Self {
            bytes: vec![0x77; parameter_count.div_ceil(2)],
            len: parameter_count,
        })
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Exact semantic storage for a four-bit credit score per active parameter.
    #[must_use]
    pub const fn storage_bits_for_parameters(parameter_count: u64) -> u128 {
        parameter_count as u128 * 4
    }

    /// Packed byte requirement, rounded upward.
    #[must_use]
    pub const fn storage_bytes_for_parameters(parameter_count: u64) -> u128 {
        Self::storage_bits_for_parameters(parameter_count).div_ceil(8)
    }

    pub fn score(&self, index: usize) -> Result<i8, BooleanIntegralError> {
        if index >= self.len {
            return Err(BooleanIntegralError::ParameterOutOfRange(index));
        }
        let byte = self.bytes[index / 2];
        let nibble = if index % 2 == 0 {
            byte & 0x0f
        } else {
            byte >> 4
        };
        if nibble > 14 {
            return Err(BooleanIntegralError::InvalidPackedCredit(nibble));
        }
        Ok(nibble as i8 - 7)
    }

    /// Adds one favorable or unfavorable unit vote with saturation.
    pub fn vote(&mut self, index: usize, favorable: bool) -> Result<i8, BooleanIntegralError> {
        let current = self.score(index)?;
        let next = if favorable {
            current.saturating_add(1).min(7)
        } else {
            current.saturating_sub(1).max(-7)
        };
        self.set_score(index, next)?;
        Ok(next)
    }

    pub fn candidate_flips(&self, minimum_score: i8) -> Result<Vec<usize>, BooleanIntegralError> {
        if !(1..=7).contains(&minimum_score) {
            return Err(BooleanIntegralError::InvalidPackedThreshold(minimum_score));
        }
        let mut out = Vec::new();
        for index in 0..self.len {
            if self.score(index)? >= minimum_score {
                out.push(index);
            }
        }
        Ok(out)
    }

    fn set_score(&mut self, index: usize, score: i8) -> Result<(), BooleanIntegralError> {
        if index >= self.len {
            return Err(BooleanIntegralError::ParameterOutOfRange(index));
        }
        if !(-7..=7).contains(&score) {
            return Err(BooleanIntegralError::InvalidPackedScore(score));
        }
        let encoded = (score + 7) as u8;
        let byte = &mut self.bytes[index / 2];
        if index % 2 == 0 {
            *byte = (*byte & 0xf0) | encoded;
        } else {
            *byte = (*byte & 0x0f) | (encoded << 4);
        }
        Ok(())
    }
}


/// Gradient-free training settings for one hard two-input Boolean gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanGateTrainingConfig {
    /// Maximum accepted coordinate passes over the four truth-table bits.
    pub max_passes: u32,
    /// Evidence confidence required before a bit is eligible to flip.
    pub decision_confidence_ppm: u32,
}

impl Default for BooleanGateTrainingConfig {
    fn default() -> Self {
        Self {
            max_passes: 4,
            decision_confidence_ppm: 500_000,
        }
    }
}

impl BooleanGateTrainingConfig {
    pub fn validate(self) -> Result<Self, BooleanIntegralError> {
        if self.max_passes == 0 {
            return Err(BooleanIntegralError::ZeroTrainingPasses);
        }
        if self.decision_confidence_ppm > BOOLEAN_INTEGRAL_SCALE_PPM {
            return Err(BooleanIntegralError::ConfidenceOutOfRange(
                self.decision_confidence_ppm,
            ));
        }
        Ok(self)
    }
}

/// Integer-only evidence emitted by Boolean-integral gate training.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanGateTrainingReport {
    pub initial_table: crate::train::TruthTable2,
    pub final_table: crate::train::TruthTable2,
    pub initial_loss_ppm: u32,
    pub final_loss_ppm: u32,
    pub hard_correct: u64,
    pub sample_count: u64,
    pub passes_completed: u32,
    pub interventions: u64,
    pub accepted_flips: u64,
}

/// Learns a hard two-input Boolean gate without a soft surrogate or gradient.
///
/// Each pass measures the four possible one-bit XOR interventions, integrates
/// their integer loss evidence, then commits only the best strictly improving
/// flip. Recomputing evidence after each commit avoids carrying stale flip
/// direction across a changed Boolean state.
pub fn train_boolean_gate_samples(
    samples: &[crate::train::GateSample2],
    initial_table: crate::train::TruthTable2,
    config: BooleanGateTrainingConfig,
) -> Result<BooleanGateTrainingReport, BooleanIntegralError> {
    let config = config.validate()?;
    if samples.is_empty() {
        return Err(BooleanIntegralError::EmptySamples);
    }

    let initial_loss_ppm = hard_gate_loss_ppm(initial_table, samples)?;
    let mut current = initial_table;
    let mut current_loss_ppm = initial_loss_ppm;
    let mut passes_completed = 0_u32;
    let mut interventions = 0_u64;
    let mut accepted_flips = 0_u64;

    for _ in 0..config.max_passes {
        if current_loss_ppm == 0 {
            break;
        }

        let policy = BooleanIntegralPolicy {
            min_observations: 1,
            min_confidence_ppm: config.decision_confidence_ppm,
            max_mask_width: 1,
        };
        let mut integral = BooleanIntegralAccumulator::new(4, policy)?;
        let mut candidate_losses = [0_u32; 4];

        for bit in 0..4_u32 {
            let candidate = crate::train::TruthTable2::new(current.code() ^ (1_u8 << bit))
                .map_err(BooleanIntegralError::Train)?;
            let loss = hard_gate_loss_ppm(candidate, samples)?;
            candidate_losses[bit as usize] = loss;
            integral.observe(&BooleanIntervention {
                mask: vec![bit],
                baseline_loss_ppm: current_loss_ppm,
                perturbed_loss_ppm: loss,
            })?;
            interventions = interventions.saturating_add(1);
        }

        let mut selected: Option<(usize, u32)> = None;
        for bit in integral.recommended_flips() {
            let loss = candidate_losses[bit];
            if loss >= current_loss_ppm {
                continue;
            }
            match selected {
                None => selected = Some((bit, loss)),
                Some((best_bit, best_loss))
                    if loss < best_loss || (loss == best_loss && bit < best_bit) =>
                {
                    selected = Some((bit, loss));
                }
                _ => {}
            }
        }

        passes_completed = passes_completed.saturating_add(1);
        let Some((bit, next_loss)) = selected else {
            break;
        };

        current = crate::train::TruthTable2::new(current.code() ^ (1_u8 << bit))
            .map_err(BooleanIntegralError::Train)?;
        current_loss_ppm = next_loss;
        accepted_flips = accepted_flips.saturating_add(1);
    }

    let hard_correct = samples
        .iter()
        .filter(|sample| current.evaluate(sample.left, sample.right) == sample.target)
        .count() as u64;

    Ok(BooleanGateTrainingReport {
        initial_table,
        final_table: current,
        initial_loss_ppm,
        final_loss_ppm: current_loss_ppm,
        hard_correct,
        sample_count: samples.len() as u64,
        passes_completed,
        interventions,
        accepted_flips,
    })
}

fn hard_gate_loss_ppm(
    table: crate::train::TruthTable2,
    samples: &[crate::train::GateSample2],
) -> Result<u32, BooleanIntegralError> {
    if samples.is_empty() {
        return Err(BooleanIntegralError::EmptySamples);
    }
    let wrong = samples
        .iter()
        .filter(|sample| table.evaluate(sample.left, sample.right) != sample.target)
        .count() as u128;
    Ok(((wrong * BOOLEAN_INTEGRAL_SCALE_PPM as u128) / samples.len() as u128) as u32)
}

fn validate_loss(loss_ppm: u32) -> Result<(), BooleanIntegralError> {
    if loss_ppm > BOOLEAN_INTEGRAL_SCALE_PPM {
        return Err(BooleanIntegralError::LossOutOfRange(loss_ppm));
    }
    Ok(())
}

/// Invalid intervention, policy, or packed credit state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanIntegralError {
    ZeroParameters,
    EmptySamples,
    ZeroTrainingPasses,
    ZeroMinimumObservations,
    ZeroMaximumMaskWidth,
    ConfidenceOutOfRange(u32),
    LossOutOfRange(u32),
    EmptyMask,
    MaskTooWide { actual: usize, maximum: usize },
    DuplicateMaskIndex(usize),
    ParameterOutOfRange(usize),
    InvalidPackedCredit(u8),
    InvalidPackedScore(i8),
    InvalidPackedThreshold(i8),
    Train(crate::train::TrainError),
}

impl fmt::Display for BooleanIntegralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for BooleanIntegralError {}
