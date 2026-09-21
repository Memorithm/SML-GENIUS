//! End-to-end gradient-free Boolean learning transaction for one packed SML page.
//!
//! The transaction composes the native SML learning primitives:
//!
//! 1. coarse contiguous group interventions;
//! 2. bounded rescue of near-cancelled rejected groups;
//! 3. overlapping leaf probes and integer contrast attribution;
//! 4. ranked candidate flips;
//! 5. independent train/holdout evaluation;
//! 6. one atomic generation-checked hard-weight commit.
//!
//! Every search evaluation uses immutable XOR views. The packed page is mutated
//! only by the final verified commit.

use core::fmt;

use crate::{
    boolean_hierarchical::{
        BooleanCommitDecision, BooleanCommitEvidence, BooleanCommitPolicy, BooleanGroupObservation,
        BooleanPageGroup, HierarchicalBooleanError, HierarchicalBooleanPolicy,
        HierarchicalBooleanSearch,
    },
    boolean_integral::BOOLEAN_INTEGRAL_SCALE_PPM,
    boolean_overlapping::{
        OverlappingProbeError, OverlappingProbeObservation, OverlappingProbePlan,
        OverlappingProbePolicy,
    },
    boolean_page::{BooleanPageError, BooleanPageRead, PackedBooleanPage},
};

/// Search and commit limits for one page-local Boolean learning transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanLearningPolicy {
    pub hierarchical: HierarchicalBooleanPolicy,
    /// Number of rejected groups per hierarchy round that remain under exploration.
    pub rescue_groups_per_round: usize,
    pub overlapping: OverlappingProbePolicy,
    pub commit: BooleanCommitPolicy,
    /// Hard cap on hierarchy rounds even if unresolved groups remain.
    pub max_hierarchy_rounds: u16,
    /// Maximum number of ranked bits proposed in the final combined mutation.
    pub max_candidate_flips: usize,
}

impl Default for BooleanLearningPolicy {
    fn default() -> Self {
        Self {
            hierarchical: HierarchicalBooleanPolicy::default(),
            rescue_groups_per_round: 2,
            overlapping: OverlappingProbePolicy::default(),
            commit: BooleanCommitPolicy::default(),
            max_hierarchy_rounds: 24,
            max_candidate_flips: 8,
        }
    }
}

impl BooleanLearningPolicy {
    pub fn validate(self) -> Result<Self, BooleanLearningPolicyError> {
        self.hierarchical
            .validate()
            .map_err(BooleanLearningPolicyError::Hierarchical)?;
        self.overlapping
            .validate()
            .map_err(BooleanLearningPolicyError::Overlapping)?;
        if self.max_hierarchy_rounds == 0 {
            return Err(BooleanLearningPolicyError::ZeroHierarchyRounds);
        }
        if self.max_candidate_flips == 0 {
            return Err(BooleanLearningPolicyError::ZeroCandidateFlipBudget);
        }
        if self.hierarchical.leaf_width > self.overlapping.max_group_width {
            return Err(BooleanLearningPolicyError::LeafWiderThanProbeLimit {
                leaf_width: self.hierarchical.leaf_width,
                probe_limit: self.overlapping.max_group_width,
            });
        }
        Ok(self)
    }
}

/// One globally ranked page-local Boolean candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanLearningCandidate {
    pub parameter_index: u64,
    pub net_score_ppm: i64,
    pub confidence_ppm: i32,
}

/// Terminal state of one learning transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanLearningOutcome {
    /// Search did not produce any bit clearing the candidate thresholds.
    NoCandidate,
    /// A candidate set reached the commit gate; the contained decision states
    /// whether it was committed or rejected.
    CommitGate(BooleanCommitDecision),
}

/// Evidence and exact work counters emitted by one transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooleanLearningReport {
    pub generation_before: u64,
    pub generation_after: u64,
    pub train_baseline_loss_ppm: u32,
    pub holdout_baseline_loss_ppm: u32,
    pub train_candidate_loss_ppm: Option<u32>,
    pub holdout_candidate_loss_ppm: Option<u32>,
    pub hierarchy_rounds: u16,
    pub group_evaluations: u64,
    pub rescued_groups: u64,
    pub leaf_groups: u64,
    pub unresolved_groups: u64,
    pub probe_evaluations: u64,
    pub ranked_candidates: u64,
    pub proposed_flips: Vec<u64>,
    pub committed_flips: u64,
    pub outcome: BooleanLearningOutcome,
}

/// Executes one complete page-local Boolean learning transaction.
pub struct BooleanLearningTransaction;

impl BooleanLearningTransaction {
    pub fn train_page<E, TrainObjective, HoldoutObjective>(
        page: &mut PackedBooleanPage,
        policy: BooleanLearningPolicy,
        mut train_objective: TrainObjective,
        mut holdout_objective: HoldoutObjective,
    ) -> Result<BooleanLearningReport, BooleanLearningError<E>>
    where
        TrainObjective: FnMut(&dyn BooleanPageRead) -> Result<u32, E>,
        HoldoutObjective: FnMut(&dyn BooleanPageRead) -> Result<u32, E>,
    {
        let policy = policy
            .validate()
            .map_err(BooleanLearningError::Policy)?;
        let generation_before = page.generation();
        let train_baseline_loss_ppm = evaluate_objective(page, &mut train_objective)?;
        let holdout_baseline_loss_ppm = evaluate_objective(page, &mut holdout_objective)?;

        let mut groups = HierarchicalBooleanSearch::initial_groups(
            page.page_id(),
            page.parameter_count(),
            policy.hierarchical,
        )
        .map_err(BooleanLearningError::Hierarchical)?;

        let mut leaf_groups: Vec<BooleanPageGroup> = Vec::new();
        let mut hierarchy_rounds = 0_u16;
        let mut group_evaluations = 0_u64;
        let mut rescued_groups = 0_u64;

        while !groups.is_empty() && hierarchy_rounds < policy.max_hierarchy_rounds {
            let mut observations = Vec::with_capacity(groups.len());
            for group in groups.iter().copied() {
                let view = page
                    .group_view(group)
                    .map_err(BooleanLearningError::Page)?;
                let loss = evaluate_objective(&view, &mut train_objective)?;
                observations.push(BooleanGroupObservation {
                    group,
                    baseline_loss_ppm: train_baseline_loss_ppm,
                    perturbed_loss_ppm: loss,
                });
                group_evaluations = group_evaluations.saturating_add(1);
            }

            let round = HierarchicalBooleanSearch::refine_with_rescue(
                &observations,
                policy.hierarchical,
                policy.rescue_groups_per_round,
            )
            .map_err(BooleanLearningError::Hierarchical)?;
            rescued_groups = rescued_groups.saturating_add(round.rescued_groups);
            leaf_groups.extend(round.leaf_candidates);
            groups = round.next_groups;
            hierarchy_rounds = hierarchy_rounds.saturating_add(1);
        }

        leaf_groups.sort_by_key(|group| (group.page_id.0, group.start, group.len, group.depth));
        leaf_groups.dedup();

        let unresolved_groups = groups.len() as u64;
        let leaf_group_count = leaf_groups.len() as u64;
        let mut candidates: Vec<BooleanLearningCandidate> = Vec::new();
        let mut probe_evaluations = 0_u64;

        for leaf in leaf_groups {
            if leaf.len == 1 {
                let view = page
                    .group_view(leaf)
                    .map_err(BooleanLearningError::Page)?;
                let loss = evaluate_objective(&view, &mut train_objective)?;
                group_evaluations = group_evaluations.saturating_add(1);
                let improvement = train_baseline_loss_ppm as i64 - loss as i64;
                if improvement >= policy.hierarchical.min_improvement_ppm as i64 {
                    candidates.push(BooleanLearningCandidate {
                        parameter_index: leaf.start,
                        net_score_ppm: improvement,
                        confidence_ppm: BOOLEAN_INTEGRAL_SCALE_PPM as i32,
                    });
                }
                continue;
            }

            let plan = OverlappingProbePlan::new(leaf, policy.overlapping)
                .map_err(BooleanLearningError::Overlapping)?;
            let mut observations = Vec::with_capacity(plan.probe_count());
            for probe in plan.probes().iter().copied() {
                let view = page
                    .probe_view(probe)
                    .map_err(BooleanLearningError::Page)?;
                let loss = evaluate_objective(&view, &mut train_objective)?;
                observations.push(OverlappingProbeObservation {
                    probe,
                    baseline_loss_ppm: train_baseline_loss_ppm,
                    perturbed_loss_ppm: loss,
                });
                probe_evaluations = probe_evaluations.saturating_add(1);
            }

            let report = plan
                .score(&observations)
                .map_err(BooleanLearningError::Overlapping)?;
            candidates.extend(report.candidates.into_iter().map(|candidate| {
                BooleanLearningCandidate {
                    parameter_index: candidate.parameter_index,
                    net_score_ppm: candidate.net_score_ppm,
                    confidence_ppm: candidate.confidence_ppm,
                }
            }));
        }

        candidates.sort_by(|left, right| {
            right
                .net_score_ppm
                .cmp(&left.net_score_ppm)
                .then_with(|| right.confidence_ppm.cmp(&left.confidence_ppm))
                .then_with(|| left.parameter_index.cmp(&right.parameter_index))
        });

        let mut unique_candidates = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if unique_candidates
                .iter()
                .any(|existing: &BooleanLearningCandidate| {
                    existing.parameter_index == candidate.parameter_index
                })
            {
                continue;
            }
            unique_candidates.push(candidate);
        }
        let ranked_candidates = unique_candidates.len() as u64;
        unique_candidates.truncate(policy.max_candidate_flips);

        if unique_candidates.is_empty() {
            return Ok(BooleanLearningReport {
                generation_before,
                generation_after: page.generation(),
                train_baseline_loss_ppm,
                holdout_baseline_loss_ppm,
                train_candidate_loss_ppm: None,
                holdout_candidate_loss_ppm: None,
                hierarchy_rounds,
                group_evaluations,
                rescued_groups,
                leaf_groups: leaf_group_count,
                unresolved_groups,
                probe_evaluations,
                ranked_candidates,
                proposed_flips: Vec::new(),
                committed_flips: 0,
                outcome: BooleanLearningOutcome::NoCandidate,
            });
        }

        let proposed_flips: Vec<u64> = unique_candidates
            .iter()
            .map(|candidate| candidate.parameter_index)
            .collect();

        let (train_candidate_loss_ppm, holdout_candidate_loss_ppm) = {
            let candidate_view = page
                .index_flip_view(&proposed_flips)
                .map_err(BooleanLearningError::Page)?;
            (
                evaluate_objective(&candidate_view, &mut train_objective)?,
                evaluate_objective(&candidate_view, &mut holdout_objective)?,
            )
        };

        let evidence = BooleanCommitEvidence {
            train_baseline_loss_ppm,
            train_candidate_loss_ppm,
            holdout_baseline_loss_ppm,
            holdout_candidate_loss_ppm,
        };
        let commit = page
            .commit_verified_flips(
                &proposed_flips,
                generation_before,
                evidence,
                policy.commit,
            )
            .map_err(BooleanLearningError::Page)?;

        Ok(BooleanLearningReport {
            generation_before,
            generation_after: page.generation(),
            train_baseline_loss_ppm,
            holdout_baseline_loss_ppm,
            train_candidate_loss_ppm: Some(train_candidate_loss_ppm),
            holdout_candidate_loss_ppm: Some(holdout_candidate_loss_ppm),
            hierarchy_rounds,
            group_evaluations,
            rescued_groups,
            leaf_groups: leaf_group_count,
            unresolved_groups,
            probe_evaluations,
            ranked_candidates,
            proposed_flips,
            committed_flips: commit.committed_flips,
            outcome: BooleanLearningOutcome::CommitGate(commit.decision),
        })
    }
}

fn evaluate_objective<E, Objective>(
    read: &dyn BooleanPageRead,
    objective: &mut Objective,
) -> Result<u32, BooleanLearningError<E>>
where
    Objective: FnMut(&dyn BooleanPageRead) -> Result<u32, E>,
{
    let loss = objective(read).map_err(BooleanLearningError::Objective)?;
    if loss > BOOLEAN_INTEGRAL_SCALE_PPM {
        return Err(BooleanLearningError::LossOutOfRange(loss));
    }
    Ok(loss)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanLearningPolicyError {
    ZeroHierarchyRounds,
    ZeroCandidateFlipBudget,
    LeafWiderThanProbeLimit {
        leaf_width: u64,
        probe_limit: u64,
    },
    Hierarchical(HierarchicalBooleanError),
    Overlapping(OverlappingProbeError),
}

impl fmt::Display for BooleanLearningPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for BooleanLearningPolicyError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BooleanLearningError<E> {
    Policy(BooleanLearningPolicyError),
    Page(BooleanPageError),
    Hierarchical(HierarchicalBooleanError),
    Overlapping(OverlappingProbeError),
    Objective(E),
    LossOutOfRange(u32),
}

impl<E: fmt::Debug> fmt::Display for BooleanLearningError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<E: fmt::Debug> std::error::Error for BooleanLearningError<E> {}
