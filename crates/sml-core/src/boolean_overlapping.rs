//! Deterministic overlapping Boolean probes for page-local SML credit assignment.
//!
//! Hierarchical contiguous probes can hide useful parameters when favorable and
//! unfavorable effects cancel inside the same range. This module adds a second,
//! sparse-identification stage for small hierarchical leaves.
//!
//! A probe is represented by a compact affine partition descriptor. It never
//! materializes a page-sized list of parameter indexes. For each round, an odd
//! affine permutation maps leaf-local indexes into a power-of-two address space;
//! complementary probes then select the zero or one side of one address bit.
//! Repeating this with different deterministic permutations gives every parameter
//! several overlapping signatures.
//!
//! Attribution is integer-only. For one complementary probe pair, each parameter
//! receives the contrast between the loss improvements of the side containing it
//! and the opposite side. Contrasts are accumulated across dimensions and rounds.
//! This is a sparse-search heuristic, not an exact causal decomposition. Candidate
//! mutations still require independent holdout verification before commit.

use core::fmt;

use crate::{
    boolean_hierarchical::BooleanPageGroup,
    boolean_integral::BOOLEAN_INTEGRAL_SCALE_PPM,
};

const MAX_PROBE_ROUNDS: u16 = 64;

/// Limits and selection thresholds for overlapping leaf probes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlappingProbePolicy {
    /// Number of independent deterministic affine permutations.
    pub rounds: u16,
    /// Hard safety bound on the hierarchical leaf width handled by this stage.
    pub max_group_width: u64,
    /// Minimum normalized positive contrast required for a candidate.
    pub min_confidence_ppm: u32,
    /// Minimum accumulated positive contrast mass in ppm units.
    pub min_net_score_ppm: u64,
    /// Maximum candidates retained from one leaf.
    pub max_candidates: usize,
    /// Deterministic schedule seed. This is not a random-number-generator state.
    pub seed: u64,
}

impl Default for OverlappingProbePolicy {
    fn default() -> Self {
        Self {
            rounds: 4,
            max_group_width: 256,
            min_confidence_ppm: 250_000,
            min_net_score_ppm: 1,
            max_candidates: 16,
            seed: 1,
        }
    }
}

impl OverlappingProbePolicy {
    pub fn validate(self) -> Result<Self, OverlappingProbeError> {
        if self.rounds == 0 || self.rounds > MAX_PROBE_ROUNDS {
            return Err(OverlappingProbeError::InvalidRounds(self.rounds));
        }
        if self.max_group_width < 2 {
            return Err(OverlappingProbeError::InvalidMaximumGroupWidth(
                self.max_group_width,
            ));
        }
        if self.min_confidence_ppm > BOOLEAN_INTEGRAL_SCALE_PPM {
            return Err(OverlappingProbeError::PpmOutOfRange(
                self.min_confidence_ppm,
            ));
        }
        if self.max_candidates == 0 {
            return Err(OverlappingProbeError::ZeroCandidateBudget);
        }
        Ok(self)
    }
}

/// Compact descriptor for one side of one deterministic overlapping partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlappingProbe {
    pub group: BooleanPageGroup,
    pub round: u16,
    pub dimension: u8,
    pub side: bool,
    multiplier: u64,
    offset: u64,
    address_mask: u64,
}

impl OverlappingProbe {
    /// Returns whether one absolute page-local parameter index belongs to this probe.
    pub fn contains_parameter(
        self,
        parameter_index: u64,
    ) -> Result<bool, OverlappingProbeError> {
        if parameter_index < self.group.start || parameter_index >= self.group.end_exclusive() {
            return Err(OverlappingProbeError::ParameterOutsideGroup {
                parameter_index,
                group: self.group,
            });
        }
        let local = parameter_index - self.group.start;
        Ok(self.contains_local(local))
    }

    #[must_use]
    fn contains_local(self, local_index: u64) -> bool {
        let mapped = local_index
            .wrapping_mul(self.multiplier)
            .wrapping_add(self.offset)
            & self.address_mask;
        (((mapped >> self.dimension) & 1) != 0) == self.side
    }
}

/// One measured probe outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlappingProbeObservation {
    pub probe: OverlappingProbe,
    pub baseline_loss_ppm: u32,
    pub perturbed_loss_ppm: u32,
}

impl OverlappingProbeObservation {
    #[must_use]
    pub fn improvement_ppm(self) -> i64 {
        self.baseline_loss_ppm as i64 - self.perturbed_loss_ppm as i64
    }
}

/// Integrated contrast evidence for one leaf-local Boolean parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OverlappingCreditCell {
    pub parameter_index: u64,
    pub net_score_ppm: i64,
    pub absolute_contrast_mass_ppm: u64,
    pub contrasts: u32,
}

impl OverlappingCreditCell {
    /// Signed normalized contrast in [-1_000_000, +1_000_000].
    #[must_use]
    pub fn confidence_ppm(self) -> i32 {
        if self.absolute_contrast_mass_ppm == 0 {
            return 0;
        }
        let scaled = self.net_score_ppm as i128 * BOOLEAN_INTEGRAL_SCALE_PPM as i128
            / self.absolute_contrast_mass_ppm as i128;
        scaled.clamp(
            -(BOOLEAN_INTEGRAL_SCALE_PPM as i128),
            BOOLEAN_INTEGRAL_SCALE_PPM as i128,
        ) as i32
    }
}

/// Ranked candidate emitted by overlapping probe attribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlappingBooleanCandidate {
    pub parameter_index: u64,
    pub net_score_ppm: i64,
    pub confidence_ppm: i32,
    pub contrasts: u32,
}

/// Complete attribution result for one hierarchical leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlappingCreditReport {
    pub group: BooleanPageGroup,
    pub probe_count: usize,
    pub cells: Vec<OverlappingCreditCell>,
    pub candidates: Vec<OverlappingBooleanCandidate>,
}

/// Compact deterministic probe schedule for one hierarchical leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlappingProbePlan {
    group: BooleanPageGroup,
    policy: OverlappingProbePolicy,
    dimensions: u8,
    address_mask: u64,
    probes: Vec<OverlappingProbe>,
}

impl OverlappingProbePlan {
    pub fn new(
        group: BooleanPageGroup,
        policy: OverlappingProbePolicy,
    ) -> Result<Self, OverlappingProbeError> {
        let policy = policy.validate()?;
        if group.len < 2 {
            return Err(OverlappingProbeError::GroupTooSmall(group));
        }
        if group.len > policy.max_group_width {
            return Err(OverlappingProbeError::GroupTooWide {
                actual: group.len,
                maximum: policy.max_group_width,
            });
        }

        let dimensions = ceil_log2(group.len);
        if dimensions == 0 {
            return Err(OverlappingProbeError::GroupTooSmall(group));
        }
        let address_mask = if dimensions == 64 {
            u64::MAX
        } else {
            (1_u64 << dimensions) - 1
        };
        let probe_count = usize::from(policy.rounds)
            .checked_mul(usize::from(dimensions))
            .and_then(|value| value.checked_mul(2))
            .ok_or(OverlappingProbeError::ArithmeticOverflow)?;

        let mut probes = Vec::with_capacity(probe_count);
        for round in 0..policy.rounds {
            let (multiplier, offset) =
                affine_parameters(policy.seed, round, address_mask);
            for dimension in 0..dimensions {
                probes.push(OverlappingProbe {
                    group,
                    round,
                    dimension,
                    side: false,
                    multiplier,
                    offset,
                    address_mask,
                });
                probes.push(OverlappingProbe {
                    group,
                    round,
                    dimension,
                    side: true,
                    multiplier,
                    offset,
                    address_mask,
                });
            }
        }

        Ok(Self {
            group,
            policy,
            dimensions,
            address_mask,
            probes,
        })
    }

    #[must_use]
    pub const fn group(&self) -> BooleanPageGroup {
        self.group
    }

    #[must_use]
    pub const fn dimensions(&self) -> u8 {
        self.dimensions
    }

    #[must_use]
    pub fn probes(&self) -> &[OverlappingProbe] {
        &self.probes
    }

    #[must_use]
    pub fn probe_count(&self) -> usize {
        self.probes.len()
    }

    /// Integrates a complete set of complementary probe measurements.
    ///
    /// Observations may arrive in any order but every planned probe must appear
    /// exactly once. The two sides of a complementary pair must share the same
    /// baseline objective so their contrast has a defined meaning.
    pub fn score(
        &self,
        observations: &[OverlappingProbeObservation],
    ) -> Result<OverlappingCreditReport, OverlappingProbeError> {
        if observations.len() != self.probes.len() {
            return Err(OverlappingProbeError::ObservationCount {
                expected: self.probes.len(),
                actual: observations.len(),
            });
        }

        let mut slots: Vec<Option<OverlappingProbeObservation>> =
            vec![None; self.probes.len()];

        for observation in observations.iter().copied() {
            validate_ppm(observation.baseline_loss_ppm)?;
            validate_ppm(observation.perturbed_loss_ppm)?;
            let slot = self.slot_for_probe(observation.probe)?;
            if slots[slot].is_some() {
                return Err(OverlappingProbeError::DuplicateObservation {
                    round: observation.probe.round,
                    dimension: observation.probe.dimension,
                    side: observation.probe.side,
                });
            }
            slots[slot] = Some(observation);
        }

        if slots.iter().any(Option::is_none) {
            return Err(OverlappingProbeError::MissingObservation);
        }

        for round in 0..self.policy.rounds {
            for dimension in 0..self.dimensions {
                let left = slots[self.slot(round, dimension, false)]
                    .ok_or(OverlappingProbeError::MissingObservation)?;
                let right = slots[self.slot(round, dimension, true)]
                    .ok_or(OverlappingProbeError::MissingObservation)?;
                if left.baseline_loss_ppm != right.baseline_loss_ppm {
                    return Err(OverlappingProbeError::BaselineMismatch {
                        round,
                        dimension,
                        left: left.baseline_loss_ppm,
                        right: right.baseline_loss_ppm,
                    });
                }
            }
        }

        let width = usize::try_from(self.group.len)
            .map_err(|_| OverlappingProbeError::ArithmeticOverflow)?;
        let mut cells = Vec::with_capacity(width);

        for local in 0..self.group.len {
            let parameter_index = self
                .group
                .start
                .checked_add(local)
                .ok_or(OverlappingProbeError::ArithmeticOverflow)?;
            let mut net_score_ppm = 0_i64;
            let mut absolute_contrast_mass_ppm = 0_u64;
            let mut contrasts = 0_u32;

            for round in 0..self.policy.rounds {
                for dimension in 0..self.dimensions {
                    let containing_side = self.membership(local, round, dimension);
                    let selected = slots[self.slot(round, dimension, containing_side)]
                        .ok_or(OverlappingProbeError::MissingObservation)?;
                    let opposite = slots[self.slot(round, dimension, !containing_side)]
                        .ok_or(OverlappingProbeError::MissingObservation)?;
                    let contrast =
                        selected.improvement_ppm() - opposite.improvement_ppm();

                    net_score_ppm = net_score_ppm
                        .checked_add(contrast)
                        .ok_or(OverlappingProbeError::ArithmeticOverflow)?;
                    absolute_contrast_mass_ppm = absolute_contrast_mass_ppm
                        .checked_add(contrast.unsigned_abs())
                        .ok_or(OverlappingProbeError::ArithmeticOverflow)?;
                    contrasts = contrasts
                        .checked_add(1)
                        .ok_or(OverlappingProbeError::ArithmeticOverflow)?;
                }
            }

            cells.push(OverlappingCreditCell {
                parameter_index,
                net_score_ppm,
                absolute_contrast_mass_ppm,
                contrasts,
            });
        }

        let mut candidates: Vec<OverlappingBooleanCandidate> = cells
            .iter()
            .copied()
            .filter_map(|cell| {
                let confidence = cell.confidence_ppm();
                (cell.net_score_ppm >= self.policy.min_net_score_ppm as i64
                    && confidence >= self.policy.min_confidence_ppm as i32)
                    .then_some(OverlappingBooleanCandidate {
                        parameter_index: cell.parameter_index,
                        net_score_ppm: cell.net_score_ppm,
                        confidence_ppm: confidence,
                        contrasts: cell.contrasts,
                    })
            })
            .collect();

        candidates.sort_by(|left, right| {
            right
                .net_score_ppm
                .cmp(&left.net_score_ppm)
                .then_with(|| right.confidence_ppm.cmp(&left.confidence_ppm))
                .then_with(|| left.parameter_index.cmp(&right.parameter_index))
        });
        candidates.truncate(self.policy.max_candidates);

        Ok(OverlappingCreditReport {
            group: self.group,
            probe_count: self.probes.len(),
            cells,
            candidates,
        })
    }

    fn slot_for_probe(&self, probe: OverlappingProbe) -> Result<usize, OverlappingProbeError> {
        if probe.group != self.group
            || probe.round >= self.policy.rounds
            || probe.dimension >= self.dimensions
        {
            return Err(OverlappingProbeError::ForeignProbe);
        }
        let slot = self.slot(probe.round, probe.dimension, probe.side);
        if self.probes[slot] != probe {
            return Err(OverlappingProbeError::ForeignProbe);
        }
        Ok(slot)
    }

    #[must_use]
    fn slot(&self, round: u16, dimension: u8, side: bool) -> usize {
        ((usize::from(round) * usize::from(self.dimensions) + usize::from(dimension)) * 2)
            + usize::from(side)
    }

    #[must_use]
    fn membership(&self, local_index: u64, round: u16, dimension: u8) -> bool {
        let (multiplier, offset) =
            affine_parameters(self.policy.seed, round, self.address_mask);
        let mapped = local_index
            .wrapping_mul(multiplier)
            .wrapping_add(offset)
            & self.address_mask;
        ((mapped >> dimension) & 1) != 0
    }
}

fn ceil_log2(value: u64) -> u8 {
    if value <= 1 {
        0
    } else {
        (u64::BITS - (value - 1).leading_zeros()) as u8
    }
}

fn affine_parameters(seed: u64, round: u16, address_mask: u64) -> (u64, u64) {
    let round_value = u64::from(round);
    let raw_multiplier =
        mix64(seed ^ round_value.wrapping_mul(0x9e37_79b9_7f4a_7c15)) | 1;
    let mut multiplier = raw_multiplier & address_mask;
    multiplier |= 1;
    let offset =
        mix64(seed ^ round_value.wrapping_mul(0xbf58_476d_1ce4_e5b9) ^ 0x1234)
            & address_mask;
    (multiplier, offset)
}

/// SplitMix64 finalizer used only to derive a deterministic probe schedule.
fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn validate_ppm(value: u32) -> Result<(), OverlappingProbeError> {
    if value > BOOLEAN_INTEGRAL_SCALE_PPM {
        return Err(OverlappingProbeError::PpmOutOfRange(value));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlappingProbeError {
    InvalidRounds(u16),
    InvalidMaximumGroupWidth(u64),
    ZeroCandidateBudget,
    GroupTooSmall(BooleanPageGroup),
    GroupTooWide { actual: u64, maximum: u64 },
    ParameterOutsideGroup {
        parameter_index: u64,
        group: BooleanPageGroup,
    },
    PpmOutOfRange(u32),
    ArithmeticOverflow,
    ObservationCount { expected: usize, actual: usize },
    DuplicateObservation {
        round: u16,
        dimension: u8,
        side: bool,
    },
    MissingObservation,
    BaselineMismatch {
        round: u16,
        dimension: u8,
        left: u32,
        right: u32,
    },
    ForeignProbe,
}

impl fmt::Display for OverlappingProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for OverlappingProbeError {}
