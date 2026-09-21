//! Hierarchical Boolean-credit scheduling for page-local SML training.
//!
//! A 25B model cannot evaluate one XOR intervention per parameter. This module
//! therefore searches only inside active MOR pages and represents probes as
//! compact ranges. Promising ranges are recursively split until a bounded leaf
//! width is reached. The mechanism is a deterministic coarse-to-fine heuristic,
//! not a proof that every useful interacting bit will be discovered.
//!
//! Candidate changes are kept separate from commit evidence. A change is
//! committable only when it improves training evidence and satisfies an
//! independent holdout requirement.

use core::fmt;

use crate::{
    boolean_integral::BOOLEAN_INTEGRAL_SCALE_PPM,
    elastic_weights::{ElasticWeightError, ElasticWeightPlanner},
    PageId,
};

/// One compact contiguous parameter group inside a single circuit-atlas page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BooleanPageGroup {
    pub page_id: PageId,
    /// Page-local parameter offset.
    pub start: u64,
    pub len: u64,
    pub depth: u16,
}

impl BooleanPageGroup {
    pub fn new(
        page_id: PageId,
        start: u64,
        len: u64,
        depth: u16,
    ) -> Result<Self, HierarchicalBooleanError> {
        if len == 0 {
            return Err(HierarchicalBooleanError::ZeroGroupWidth);
        }
        start
            .checked_add(len)
            .ok_or(HierarchicalBooleanError::ArithmeticOverflow)?;
        Ok(Self {
            page_id,
            start,
            len,
            depth,
        })
    }

    #[must_use]
    pub fn end_exclusive(self) -> u64 {
        self.start + self.len
    }

    /// Splits the range deterministically into two non-empty children.
    pub fn split(self) -> Result<(Self, Self), HierarchicalBooleanError> {
        if self.len < 2 {
            return Err(HierarchicalBooleanError::UnsplittableGroup(self));
        }
        let left_len = self.len / 2;
        let right_len = self.len - left_len;
        let next_depth = self
            .depth
            .checked_add(1)
            .ok_or(HierarchicalBooleanError::DepthOverflow)?;
        Ok((
            Self::new(self.page_id, self.start, left_len, next_depth)?,
            Self::new(self.page_id, self.start + left_len, right_len, next_depth)?,
        ))
    }
}

/// Coarse-to-fine search limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HierarchicalBooleanPolicy {
    /// Width of the first compact probes emitted for an active page.
    pub root_group_width: u64,
    /// Maximum width at which a promising group becomes a leaf candidate.
    pub leaf_width: u64,
    /// Minimum strict loss reduction required to refine a group.
    pub min_improvement_ppm: u32,
    /// Hard bound on groups emitted by one refinement call.
    pub max_next_groups: usize,
    /// Hard recursion-depth guard.
    pub max_depth: u16,
}

impl Default for HierarchicalBooleanPolicy {
    fn default() -> Self {
        Self {
            root_group_width: 1_048_576,
            leaf_width: 256,
            min_improvement_ppm: 1,
            max_next_groups: 4_096,
            max_depth: 24,
        }
    }
}

impl HierarchicalBooleanPolicy {
    pub fn validate(self) -> Result<Self, HierarchicalBooleanError> {
        if self.root_group_width == 0 {
            return Err(HierarchicalBooleanError::ZeroRootGroupWidth);
        }
        if self.leaf_width == 0 {
            return Err(HierarchicalBooleanError::ZeroLeafWidth);
        }
        if self.leaf_width > self.root_group_width {
            return Err(HierarchicalBooleanError::LeafWiderThanRoot);
        }
        if self.min_improvement_ppm > BOOLEAN_INTEGRAL_SCALE_PPM {
            return Err(HierarchicalBooleanError::PpmOutOfRange(
                self.min_improvement_ppm,
            ));
        }
        if self.max_next_groups == 0 {
            return Err(HierarchicalBooleanError::ZeroNextGroupBudget);
        }
        if self.max_depth == 0 {
            return Err(HierarchicalBooleanError::ZeroMaximumDepth);
        }
        Ok(self)
    }
}

/// Loss evidence for toggling one compact page-local range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanGroupObservation {
    pub group: BooleanPageGroup,
    pub baseline_loss_ppm: u32,
    pub perturbed_loss_ppm: u32,
}

impl BooleanGroupObservation {
    pub fn validate(self) -> Result<Self, HierarchicalBooleanError> {
        validate_ppm(self.baseline_loss_ppm)?;
        validate_ppm(self.perturbed_loss_ppm)?;
        Ok(self)
    }

    /// Positive values mean the XOR range improved the objective.
    #[must_use]
    pub fn improvement_ppm(self) -> i64 {
        self.baseline_loss_ppm as i64 - self.perturbed_loss_ppm as i64
    }
}

/// Result of refining one batch of group observations.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HierarchicalBooleanRound {
    /// Groups that should be evaluated in the next round.
    pub next_groups: Vec<BooleanPageGroup>,
    /// Promising or deliberately rescued groups ready for leaf-level credit assignment.
    pub leaf_candidates: Vec<BooleanPageGroup>,
    pub rejected_groups: u64,
    pub favorable_groups: u64,
    /// Rejected groups deliberately explored because their net evidence may hide cancellation.
    pub rescued_groups: u64,
}

/// Deterministic scheduler for active-page Boolean credit search.
pub struct HierarchicalBooleanSearch;

impl HierarchicalBooleanSearch {
    /// Partitions one active page into compact initial probes without materializing
    /// one parameter index per bit.
    pub fn initial_groups(
        page_id: PageId,
        parameter_count: u64,
        policy: HierarchicalBooleanPolicy,
    ) -> Result<Vec<BooleanPageGroup>, HierarchicalBooleanError> {
        let policy = policy.validate()?;
        if parameter_count == 0 {
            return Err(HierarchicalBooleanError::ZeroParameters(page_id));
        }

        let count = parameter_count.div_ceil(policy.root_group_width);
        if count > policy.max_next_groups as u64 {
            return Err(HierarchicalBooleanError::GroupBudgetExceeded {
                required: usize::try_from(count).unwrap_or(usize::MAX),
                maximum: policy.max_next_groups,
            });
        }

        let mut groups = Vec::with_capacity(count as usize);
        let mut start = 0_u64;
        while start < parameter_count {
            let remaining = parameter_count - start;
            let len = remaining.min(policy.root_group_width);
            groups.push(BooleanPageGroup::new(page_id, start, len, 0)?);
            start = start
                .checked_add(len)
                .ok_or(HierarchicalBooleanError::ArithmeticOverflow)?;
        }
        Ok(groups)
    }

    /// Keeps only strictly useful groups, then recursively schedules their
    /// children. Equal or worse evidence is discarded.
    pub fn refine(
        observations: &[BooleanGroupObservation],
        policy: HierarchicalBooleanPolicy,
    ) -> Result<HierarchicalBooleanRound, HierarchicalBooleanError> {
        Self::refine_with_rescue(observations, policy, 0)
    }

    /// Refines favorable groups and deterministically rescues a bounded number
    /// of rejected groups with the smallest absolute net effect.
    ///
    /// Near-zero group evidence can hide cancellation between useful and harmful
    /// parameters. Rescue preserves bounded exploration without turning the
    /// hierarchy into an exhaustive search.
    pub fn refine_with_rescue(
        observations: &[BooleanGroupObservation],
        policy: HierarchicalBooleanPolicy,
        rescue_budget: usize,
    ) -> Result<HierarchicalBooleanRound, HierarchicalBooleanError> {
        let policy = policy.validate()?;
        if observations.is_empty() {
            return Err(HierarchicalBooleanError::EmptyObservations);
        }

        let mut seen = Vec::with_capacity(observations.len());
        let mut round = HierarchicalBooleanRound::default();
        let mut rescue_candidates: Vec<(u64, BooleanGroupObservation)> = Vec::new();

        for observation in observations.iter().copied() {
            let observation = observation.validate()?;
            if seen.contains(&observation.group) {
                return Err(HierarchicalBooleanError::DuplicateGroup(observation.group));
            }
            seen.push(observation.group);

            let improvement = observation.improvement_ppm();
            if improvement < policy.min_improvement_ppm as i64 {
                round.rejected_groups = round.rejected_groups.saturating_add(1);
                rescue_candidates.push((improvement.unsigned_abs(), observation));
                continue;
            }

            round.favorable_groups = round.favorable_groups.saturating_add(1);
            schedule_group(observation.group, &mut round, policy)?;
        }

        rescue_candidates.sort_by_key(|(magnitude, observation)| {
            (
                *magnitude,
                observation.group.page_id.0,
                observation.group.start,
                observation.group.len,
                observation.group.depth,
            )
        });

        for (_, observation) in rescue_candidates.into_iter().take(rescue_budget) {
            round.rescued_groups = round.rescued_groups.saturating_add(1);
            schedule_group(observation.group, &mut round, policy)?;
        }

        round
            .next_groups
            .sort_by_key(|group| (group.page_id.0, group.start, group.len, group.depth));
        round
            .leaf_candidates
            .sort_by_key(|group| (group.page_id.0, group.start, group.len, group.depth));
        Ok(round)
    }
}

fn schedule_group(
    group: BooleanPageGroup,
    round: &mut HierarchicalBooleanRound,
    policy: HierarchicalBooleanPolicy,
) -> Result<(), HierarchicalBooleanError> {
    if group.len <= policy.leaf_width || group.depth >= policy.max_depth {
        round.leaf_candidates.push(group);
        return Ok(());
    }

    let (left, right) = group.split()?;
    if round.next_groups.len().saturating_add(2) > policy.max_next_groups {
        return Err(HierarchicalBooleanError::GroupBudgetExceeded {
            required: round.next_groups.len().saturating_add(2),
            maximum: policy.max_next_groups,
        });
    }
    round.next_groups.push(left);
    round.next_groups.push(right);
    Ok(())
}

/// Independent train/holdout evidence required before mutating hard weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanCommitEvidence {
    pub train_baseline_loss_ppm: u32,
    pub train_candidate_loss_ppm: u32,
    pub holdout_baseline_loss_ppm: u32,
    pub holdout_candidate_loss_ppm: u32,
}

/// Commit requirements for a proposed Boolean mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanCommitPolicy {
    pub min_train_improvement_ppm: u32,
    /// Zero means holdout must not regress. A positive value requires that much
    /// holdout improvement.
    pub min_holdout_improvement_ppm: u32,
}

impl Default for BooleanCommitPolicy {
    fn default() -> Self {
        Self {
            min_train_improvement_ppm: 1,
            min_holdout_improvement_ppm: 0,
        }
    }
}

/// Why a proposed Boolean mutation was accepted or rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanCommitDecision {
    Commit,
    RejectTrain,
    RejectHoldout,
}

impl BooleanCommitEvidence {
    pub fn decide(
        self,
        policy: BooleanCommitPolicy,
    ) -> Result<BooleanCommitDecision, HierarchicalBooleanError> {
        for value in [
            self.train_baseline_loss_ppm,
            self.train_candidate_loss_ppm,
            self.holdout_baseline_loss_ppm,
            self.holdout_candidate_loss_ppm,
            policy.min_train_improvement_ppm,
            policy.min_holdout_improvement_ppm,
        ] {
            validate_ppm(value)?;
        }

        let train_improvement =
            self.train_baseline_loss_ppm as i64 - self.train_candidate_loss_ppm as i64;
        if train_improvement < policy.min_train_improvement_ppm as i64 {
            return Ok(BooleanCommitDecision::RejectTrain);
        }

        let holdout_improvement =
            self.holdout_baseline_loss_ppm as i64 - self.holdout_candidate_loss_ppm as i64;
        if holdout_improvement < policy.min_holdout_improvement_ppm as i64 {
            return Ok(BooleanCommitDecision::RejectHoldout);
        }

        Ok(BooleanCommitDecision::Commit)
    }
}

/// Aggregate page-level evidence used to prioritize Elastic Weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BooleanPageCreditSummary {
    pub positive_mass: u64,
    pub negative_mass: u64,
    pub favorable_groups: u64,
    pub unfavorable_groups: u64,
}

impl BooleanPageCreditSummary {
    pub fn observe(
        &mut self,
        observation: BooleanGroupObservation,
    ) -> Result<(), HierarchicalBooleanError> {
        let observation = observation.validate()?;
        let delta = observation.improvement_ppm();
        if delta > 0 {
            self.positive_mass = self.positive_mass.saturating_add(delta as u64);
            self.favorable_groups = self.favorable_groups.saturating_add(1);
        } else if delta < 0 {
            self.negative_mass = self.negative_mass.saturating_add(delta.unsigned_abs());
            self.unfavorable_groups = self.unfavorable_groups.saturating_add(1);
        }
        Ok(())
    }

    /// Positive-credit ratio in ppm. Neutral-only evidence maps to zero.
    #[must_use]
    pub fn importance_ppm(self) -> u32 {
        let total = self.positive_mass as u128 + self.negative_mass as u128;
        if total == 0 {
            return 0;
        }
        ((self.positive_mass as u128 * BOOLEAN_INTEGRAL_SCALE_PPM as u128) / total) as u32
    }

    /// Feeds page-level learning evidence into SML's internal Elastic Weight
    /// planner without changing parameter values or residency directly.
    pub fn record_elastic_importance(
        self,
        planner: &mut ElasticWeightPlanner,
        page_id: PageId,
    ) -> Result<(), HierarchicalBooleanError> {
        planner
            .record_importance(page_id, self.importance_ppm())
            .map_err(HierarchicalBooleanError::Elastic)
    }
}

fn validate_ppm(value: u32) -> Result<(), HierarchicalBooleanError> {
    if value > BOOLEAN_INTEGRAL_SCALE_PPM {
        return Err(HierarchicalBooleanError::PpmOutOfRange(value));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchicalBooleanError {
    ZeroParameters(PageId),
    ZeroGroupWidth,
    ZeroRootGroupWidth,
    ZeroLeafWidth,
    LeafWiderThanRoot,
    ZeroNextGroupBudget,
    ZeroMaximumDepth,
    EmptyObservations,
    PpmOutOfRange(u32),
    ArithmeticOverflow,
    DepthOverflow,
    UnsplittableGroup(BooleanPageGroup),
    DuplicateGroup(BooleanPageGroup),
    GroupBudgetExceeded { required: usize, maximum: usize },
    Elastic(ElasticWeightError),
}

impl fmt::Display for HierarchicalBooleanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for HierarchicalBooleanError {}
