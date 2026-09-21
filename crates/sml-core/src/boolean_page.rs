//! Packed Boolean parameter pages and non-mutating probe execution.
//!
//! This module provides the physical reference surface needed to execute SML's
//! Boolean-integral learning probes against hard packed weights. Parameters are
//! stored one bit each in machine words. Overlapping probes are exposed as
//! ephemeral XOR views: evaluating a probe does not copy or mutate the page.
//!
//! Hard mutations are committed separately and atomically. A commit requires
//! both an expected page generation and the existing train/holdout commit gate,
//! so stale or holdout-regressing proposals fail closed.

use core::fmt;

use crate::{
    boolean_hierarchical::{
        BooleanCommitDecision, BooleanCommitEvidence, BooleanCommitPolicy, BooleanPageGroup,
        HierarchicalBooleanError,
    },
    boolean_integral::BOOLEAN_INTEGRAL_SCALE_PPM,
    boolean_overlapping::{
        OverlappingProbe, OverlappingProbeError, OverlappingProbeObservation, OverlappingProbePlan,
    },
    PageId,
};

/// Read-only Boolean page contract shared by base pages and ephemeral probe views.
pub trait BooleanPageRead {
    fn page_id(&self) -> PageId;
    fn parameter_count(&self) -> u64;
    fn bit(&self, parameter_index: u64) -> Result<bool, BooleanPageError>;
}

/// One hard Boolean parameter page packed into machine words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedBooleanPage {
    page_id: PageId,
    parameter_count: u64,
    words: Vec<u64>,
    generation: u64,
}

impl PackedBooleanPage {
    pub fn zeros(page_id: PageId, parameter_count: u64) -> Result<Self, BooleanPageError> {
        if parameter_count == 0 {
            return Err(BooleanPageError::ZeroParameters(page_id));
        }
        let word_count_u64 = parameter_count.div_ceil(64);
        let word_count = usize::try_from(word_count_u64)
            .map_err(|_| BooleanPageError::ParameterCountTooLarge(parameter_count))?;
        Ok(Self {
            page_id,
            parameter_count,
            words: vec![0; word_count],
            generation: 0,
        })
    }

    pub fn from_bits(page_id: PageId, bits: &[bool]) -> Result<Self, BooleanPageError> {
        if bits.is_empty() {
            return Err(BooleanPageError::ZeroParameters(page_id));
        }
        let parameter_count = bits.len() as u64;
        let mut page = Self::zeros(page_id, parameter_count)?;
        for (index, value) in bits.iter().copied().enumerate() {
            if value {
                let word = index / 64;
                let shift = index % 64;
                page.words[word] |= 1_u64 << shift;
            }
        }
        Ok(page)
    }

    #[must_use]
    pub const fn page_id(&self) -> PageId {
        self.page_id
    }

    #[must_use]
    pub const fn parameter_count(&self) -> u64 {
        self.parameter_count
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Exact logical packed payload, excluding allocator and page metadata.
    #[must_use]
    pub const fn payload_bits(&self) -> u128 {
        self.parameter_count as u128
    }

    /// Exact logical packed bytes, rounded upward to the next byte.
    #[must_use]
    pub const fn payload_bytes_ceil(&self) -> u128 {
        Self::packed_bytes_for_parameters(self.parameter_count)
    }

    #[must_use]
    pub const fn packed_bytes_for_parameters(parameter_count: u64) -> u128 {
        (parameter_count as u128).div_ceil(8)
    }

    pub fn bit(&self, parameter_index: u64) -> Result<bool, BooleanPageError> {
        if parameter_index >= self.parameter_count {
            return Err(BooleanPageError::ParameterOutOfRange {
                parameter_index,
                parameter_count: self.parameter_count,
            });
        }
        let word_index = usize::try_from(parameter_index / 64)
            .map_err(|_| BooleanPageError::ParameterCountTooLarge(self.parameter_count))?;
        let shift = parameter_index % 64;
        Ok((self.words[word_index] & (1_u64 << shift)) != 0)
    }

    /// Creates a read-only XOR overlay for one compact overlapping probe.
    pub fn probe_view(
        &self,
        probe: OverlappingProbe,
    ) -> Result<BooleanProbeView<'_>, BooleanPageError> {
        self.validate_group(probe.group)?;
        Ok(BooleanProbeView { page: self, probe })
    }

    /// Creates a read-only view that toggles every parameter in one contiguous group.
    pub fn group_view(
        &self,
        group: BooleanPageGroup,
    ) -> Result<BooleanGroupView<'_>, BooleanPageError> {
        self.validate_group(group)?;
        Ok(BooleanGroupView { page: self, group })
    }

    /// Creates a read-only view that toggles an explicit, validated set of indexes.
    pub fn index_flip_view(
        &self,
        parameter_indexes: &[u64],
    ) -> Result<BooleanIndexFlipView<'_>, BooleanPageError> {
        if parameter_indexes.is_empty() {
            return Err(BooleanPageError::EmptyCommit);
        }
        let mut indexes = parameter_indexes.to_vec();
        indexes.sort_unstable();
        self.validate_indexes(&indexes)?;
        Ok(BooleanIndexFlipView {
            page: self,
            indexes,
        })
    }

    /// Atomically commits verified hard flips.
    ///
    /// All indexes, generation state, and train/holdout evidence are validated
    /// before any bit is modified.
    pub fn commit_verified_flips(
        &mut self,
        parameter_indexes: &[u64],
        expected_generation: u64,
        evidence: BooleanCommitEvidence,
        policy: BooleanCommitPolicy,
    ) -> Result<BooleanVerifiedCommitReport, BooleanPageError> {
        if expected_generation != self.generation {
            return Err(BooleanPageError::StaleGeneration {
                expected: self.generation,
                actual: expected_generation,
            });
        }
        if parameter_indexes.is_empty() {
            return Err(BooleanPageError::EmptyCommit);
        }

        let mut indexes = parameter_indexes.to_vec();
        indexes.sort_unstable();
        self.validate_indexes(&indexes)?;

        let decision = evidence
            .decide(policy)
            .map_err(BooleanPageError::CommitEvidence)?;
        let generation_before = self.generation;

        if decision != BooleanCommitDecision::Commit {
            return Ok(BooleanVerifiedCommitReport {
                decision,
                generation_before,
                generation_after: generation_before,
                requested_flips: indexes.len() as u64,
                committed_flips: 0,
            });
        }

        let next_generation = self
            .generation
            .checked_add(1)
            .ok_or(BooleanPageError::GenerationOverflow)?;

        for index in indexes.iter().copied() {
            self.toggle_unchecked(index)?;
        }
        self.generation = next_generation;

        Ok(BooleanVerifiedCommitReport {
            decision,
            generation_before,
            generation_after: self.generation,
            requested_flips: indexes.len() as u64,
            committed_flips: indexes.len() as u64,
        })
    }

    fn validate_group(&self, group: BooleanPageGroup) -> Result<(), BooleanPageError> {
        if group.page_id != self.page_id {
            return Err(BooleanPageError::PageMismatch {
                expected: self.page_id,
                actual: group.page_id,
            });
        }
        if group.end_exclusive() > self.parameter_count {
            return Err(BooleanPageError::GroupOutsidePage {
                group,
                parameter_count: self.parameter_count,
            });
        }
        Ok(())
    }

    fn validate_indexes(&self, indexes: &[u64]) -> Result<(), BooleanPageError> {
        for index in indexes.iter().copied() {
            if index >= self.parameter_count {
                return Err(BooleanPageError::ParameterOutOfRange {
                    parameter_index: index,
                    parameter_count: self.parameter_count,
                });
            }
        }
        for pair in indexes.windows(2) {
            if pair[0] == pair[1] {
                return Err(BooleanPageError::DuplicateCommitIndex(pair[0]));
            }
        }
        Ok(())
    }

    fn toggle_unchecked(&mut self, parameter_index: u64) -> Result<(), BooleanPageError> {
        let word_index = usize::try_from(parameter_index / 64)
            .map_err(|_| BooleanPageError::ParameterCountTooLarge(self.parameter_count))?;
        let shift = parameter_index % 64;
        self.words[word_index] ^= 1_u64 << shift;
        Ok(())
    }
}

impl BooleanPageRead for PackedBooleanPage {
    fn page_id(&self) -> PageId {
        self.page_id
    }

    fn parameter_count(&self) -> u64 {
        self.parameter_count
    }

    fn bit(&self, parameter_index: u64) -> Result<bool, BooleanPageError> {
        PackedBooleanPage::bit(self, parameter_index)
    }
}

/// Ephemeral read-only XOR overlay over a packed Boolean page.
#[derive(Debug, Clone, Copy)]
pub struct BooleanProbeView<'a> {
    page: &'a PackedBooleanPage,
    probe: OverlappingProbe,
}

impl BooleanProbeView<'_> {
    #[must_use]
    pub const fn probe(&self) -> OverlappingProbe {
        self.probe
    }
}

impl BooleanPageRead for BooleanProbeView<'_> {
    fn page_id(&self) -> PageId {
        self.page.page_id
    }

    fn parameter_count(&self) -> u64 {
        self.page.parameter_count
    }

    fn bit(&self, parameter_index: u64) -> Result<bool, BooleanPageError> {
        let base = self.page.bit(parameter_index)?;
        let group = self.probe.group;
        if parameter_index < group.start || parameter_index >= group.end_exclusive() {
            return Ok(base);
        }
        let toggled = self
            .probe
            .contains_parameter(parameter_index)
            .map_err(BooleanPageError::Probe)?;
        Ok(base ^ toggled)
    }
}

/// Ephemeral view that toggles one complete contiguous page-local group.
#[derive(Debug, Clone, Copy)]
pub struct BooleanGroupView<'a> {
    page: &'a PackedBooleanPage,
    group: BooleanPageGroup,
}

impl BooleanPageRead for BooleanGroupView<'_> {
    fn page_id(&self) -> PageId {
        self.page.page_id
    }

    fn parameter_count(&self) -> u64 {
        self.page.parameter_count
    }

    fn bit(&self, parameter_index: u64) -> Result<bool, BooleanPageError> {
        let base = self.page.bit(parameter_index)?;
        Ok(base
            ^ (parameter_index >= self.group.start
                && parameter_index < self.group.end_exclusive()))
    }
}

/// Ephemeral view that toggles a validated sorted set of explicit indexes.
#[derive(Debug, Clone)]
pub struct BooleanIndexFlipView<'a> {
    page: &'a PackedBooleanPage,
    indexes: Vec<u64>,
}

impl BooleanPageRead for BooleanIndexFlipView<'_> {
    fn page_id(&self) -> PageId {
        self.page.page_id
    }

    fn parameter_count(&self) -> u64 {
        self.page.parameter_count
    }

    fn bit(&self, parameter_index: u64) -> Result<bool, BooleanPageError> {
        let base = self.page.bit(parameter_index)?;
        Ok(base ^ self.indexes.binary_search(&parameter_index).is_ok())
    }
}

/// Measurements produced by executing one complete overlapping probe plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooleanProbeExecutionReport {
    pub baseline_loss_ppm: u32,
    pub observations: Vec<OverlappingProbeObservation>,
}

/// Executes a complete overlapping plan against ephemeral XOR views.
///
/// The objective receives the immutable base page once and then one read-only
/// view per probe. The base page generation and packed payload remain unchanged.
pub fn evaluate_overlapping_plan<E, F>(
    page: &PackedBooleanPage,
    plan: &OverlappingProbePlan,
    mut objective: F,
) -> Result<BooleanProbeExecutionReport, BooleanProbeExecutionError<E>>
where
    F: FnMut(&dyn BooleanPageRead) -> Result<u32, E>,
{
    page.validate_group(plan.group())
        .map_err(BooleanProbeExecutionError::Page)?;

    let baseline_loss_ppm = objective(page).map_err(BooleanProbeExecutionError::Objective)?;
    validate_loss(baseline_loss_ppm).map_err(BooleanProbeExecutionError::Page)?;

    let mut observations = Vec::with_capacity(plan.probe_count());
    for probe in plan.probes().iter().copied() {
        let view = page
            .probe_view(probe)
            .map_err(BooleanProbeExecutionError::Page)?;
        let perturbed_loss_ppm = objective(&view).map_err(BooleanProbeExecutionError::Objective)?;
        validate_loss(perturbed_loss_ppm).map_err(BooleanProbeExecutionError::Page)?;
        observations.push(OverlappingProbeObservation {
            probe,
            baseline_loss_ppm,
            perturbed_loss_ppm,
        });
    }

    Ok(BooleanProbeExecutionReport {
        baseline_loss_ppm,
        observations,
    })
}

/// Outcome of one verified hard-weight commit attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanVerifiedCommitReport {
    pub decision: BooleanCommitDecision,
    pub generation_before: u64,
    pub generation_after: u64,
    pub requested_flips: u64,
    pub committed_flips: u64,
}

fn validate_loss(loss_ppm: u32) -> Result<(), BooleanPageError> {
    if loss_ppm > BOOLEAN_INTEGRAL_SCALE_PPM {
        return Err(BooleanPageError::LossOutOfRange(loss_ppm));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanPageError {
    ZeroParameters(PageId),
    ParameterCountTooLarge(u64),
    ParameterOutOfRange {
        parameter_index: u64,
        parameter_count: u64,
    },
    PageMismatch {
        expected: PageId,
        actual: PageId,
    },
    GroupOutsidePage {
        group: BooleanPageGroup,
        parameter_count: u64,
    },
    EmptyCommit,
    DuplicateCommitIndex(u64),
    StaleGeneration {
        expected: u64,
        actual: u64,
    },
    GenerationOverflow,
    LossOutOfRange(u32),
    Probe(OverlappingProbeError),
    CommitEvidence(HierarchicalBooleanError),
}

impl fmt::Display for BooleanPageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for BooleanPageError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BooleanProbeExecutionError<E> {
    Page(BooleanPageError),
    Objective(E),
}

impl<E: fmt::Debug> fmt::Display for BooleanProbeExecutionError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<E: fmt::Debug> std::error::Error for BooleanProbeExecutionError<E> {}
