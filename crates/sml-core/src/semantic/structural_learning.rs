//! Structural Boolean evidence learning for Semantic Boolean Geometry.
//!
//! Coordinate flips are handled by `evidence_learning`. This module covers
//! structural semantic mutations that cannot be represented by one coordinate:
//! BIND, SPLIT and MERGE. Candidate credit still uses the native
//! `BooleanIntegralAccumulator`, and hard mutation still requires the native
//! independent train/holdout commit gate.

use core::fmt;
use std::collections::BTreeMap;

use crate::{
    boolean_hierarchical::{
        BooleanCommitDecision, BooleanCommitEvidence, BooleanCommitPolicy, HierarchicalBooleanError,
    },
    boolean_integral::{
        BooleanCreditCell, BooleanIntegralAccumulator, BooleanIntegralDecision,
        BooleanIntegralError, BooleanIntegralPolicy, BooleanIntervention,
        BOOLEAN_INTEGRAL_SCALE_PPM,
    },
};

use super::{bind, bundle_majority, SemanticAlgebraError, SemanticBitField, SemanticState};

/// Small explicit set of competing semantic hypotheses.
///
/// This is a reference structure for structural learning experiments, not a
/// claim that production inference should materialize large hypothesis lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticHypothesisPool<const BITS: usize> {
    hypotheses: Vec<SemanticState<BITS>>,
}

impl<const BITS: usize> SemanticHypothesisPool<BITS> {
    pub fn new(hypotheses: Vec<SemanticState<BITS>>) -> Result<Self, SemanticStructuralError<()>> {
        if hypotheses.is_empty() {
            return Err(SemanticStructuralError::EmptyHypothesisPool);
        }
        Ok(Self { hypotheses })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.hypotheses.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.hypotheses.is_empty()
    }

    #[must_use]
    pub fn hypotheses(&self) -> &[SemanticState<BITS>] {
        &self.hypotheses
    }
}

/// Executable structural SBG mutation.
///
/// - BIND role-binds one hypothesis value field with the existing SBG algebra.
/// - SPLIT partitions one hypothesis context mask into two disjoint, exhaustive
///   child masks.
/// - MERGE uses an explicit third anchor for strict-majority bundling, avoiding
///   a hidden two-way tie rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticStructuralMutation<const BITS: usize> {
    Bind {
        hypothesis: usize,
        role: SemanticBitField<BITS>,
    },
    Split {
        hypothesis: usize,
        left_mask: SemanticBitField<BITS>,
        right_mask: SemanticBitField<BITS>,
    },
    Merge {
        left: usize,
        right: usize,
        anchor: SemanticState<BITS>,
    },
}

impl<const BITS: usize> SemanticStructuralMutation<BITS> {
    /// Applies the structural transformation to a clone of the hypothesis pool.
    pub fn preview(
        &self,
        pool: &SemanticHypothesisPool<BITS>,
    ) -> Result<SemanticHypothesisPool<BITS>, SemanticStructuralError<()>> {
        match self {
            Self::Bind { hypothesis, role } => preview_bind(pool, *hypothesis, role),
            Self::Split {
                hypothesis,
                left_mask,
                right_mask,
            } => preview_split(pool, *hypothesis, left_mask, right_mask),
            Self::Merge {
                left,
                right,
                anchor,
            } => preview_merge(pool, *left, *right, anchor),
        }
    }
}

/// Registered structural proposal with stable evidence identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticStructuralProposal<const BITS: usize> {
    pub proposal_id: u64,
    pub mutation: SemanticStructuralMutation<BITS>,
}

/// One measured intervention over one or more structural proposals.
///
/// Grouped proposal observations intentionally retain the Boolean-integral
/// shared-credit semantics. They are evidence, not individual causal proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticStructuralObservation {
    pub proposal_ids: Vec<u64>,
    pub baseline_loss_ppm: u32,
    pub perturbed_loss_ppm: u32,
}

/// Structural candidate whose integrated evidence clears the flip threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticStructuralCandidate<const BITS: usize> {
    pub proposal_id: u64,
    pub mutation: SemanticStructuralMutation<BITS>,
    pub observations: u32,
    pub confidence_ppm: i32,
}

/// Persistent Boolean evidence over a fixed structural proposal registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticStructuralEvidenceLedger<const BITS: usize> {
    proposals: Vec<SemanticStructuralProposal<BITS>>,
    proposal_index: BTreeMap<u64, usize>,
    accumulator: BooleanIntegralAccumulator,
    policy: BooleanIntegralPolicy,
}

impl<const BITS: usize> SemanticStructuralEvidenceLedger<BITS> {
    pub fn new(
        proposals: Vec<SemanticStructuralProposal<BITS>>,
        policy: BooleanIntegralPolicy,
    ) -> Result<Self, SemanticStructuralError<()>> {
        if proposals.is_empty() {
            return Err(SemanticStructuralError::EmptyProposalRegistry);
        }
        let policy = policy
            .validate()
            .map_err(SemanticStructuralError::Integral)?;
        let mut proposal_index = BTreeMap::new();
        for (index, proposal) in proposals.iter().enumerate() {
            if proposal_index.insert(proposal.proposal_id, index).is_some() {
                return Err(SemanticStructuralError::DuplicateProposalId(
                    proposal.proposal_id,
                ));
            }
        }

        let accumulator = BooleanIntegralAccumulator::new(proposals.len(), policy)
            .map_err(SemanticStructuralError::Integral)?;

        Ok(Self {
            proposals,
            proposal_index,
            accumulator,
            policy,
        })
    }

    #[must_use]
    pub fn proposal_count(&self) -> usize {
        self.proposals.len()
    }

    #[must_use]
    pub fn interventions(&self) -> u64 {
        self.accumulator.interventions()
    }

    pub fn observe(
        &mut self,
        observation: &SemanticStructuralObservation,
    ) -> Result<(), SemanticStructuralError<()>> {
        if observation.proposal_ids.is_empty() {
            return Err(SemanticStructuralError::EmptyProposalObservation);
        }

        let mut mask = Vec::with_capacity(observation.proposal_ids.len());
        for proposal_id in &observation.proposal_ids {
            let index = self
                .proposal_index
                .get(proposal_id)
                .copied()
                .ok_or(SemanticStructuralError::UnknownProposalId(*proposal_id))?;
            mask.push(
                u32::try_from(index)
                    .map_err(|_| SemanticStructuralError::ProposalIndexOutOfRange(index))?,
            );
        }

        self.accumulator
            .observe(&BooleanIntervention {
                mask,
                baseline_loss_ppm: observation.baseline_loss_ppm,
                perturbed_loss_ppm: observation.perturbed_loss_ppm,
            })
            .map_err(SemanticStructuralError::Integral)
    }

    pub fn cell(&self, proposal_id: u64) -> Result<BooleanCreditCell, SemanticStructuralError<()>> {
        let index = self
            .proposal_index
            .get(&proposal_id)
            .copied()
            .ok_or(SemanticStructuralError::UnknownProposalId(proposal_id))?;
        self.accumulator
            .cell(index)
            .map_err(SemanticStructuralError::Integral)
    }

    pub fn decision(
        &self,
        proposal_id: u64,
    ) -> Result<BooleanIntegralDecision, SemanticStructuralError<()>> {
        Ok(self.cell(proposal_id)?.decision(self.policy))
    }

    pub fn recommended_candidates(
        &self,
        max_candidates: usize,
    ) -> Result<Vec<SemanticStructuralCandidate<BITS>>, SemanticStructuralError<()>> {
        if max_candidates == 0 {
            return Err(SemanticStructuralError::ZeroCandidateBudget);
        }

        let mut candidates = Vec::new();
        for index in self.accumulator.recommended_flips() {
            let proposal = self
                .proposals
                .get(index)
                .ok_or(SemanticStructuralError::ProposalIndexOutOfRange(index))?;
            let cell = self
                .accumulator
                .cell(index)
                .map_err(SemanticStructuralError::Integral)?;
            candidates.push(SemanticStructuralCandidate {
                proposal_id: proposal.proposal_id,
                mutation: proposal.mutation.clone(),
                observations: cell.observations,
                confidence_ppm: cell.signed_confidence_ppm(),
            });
        }

        candidates.sort_by(|left, right| {
            right
                .confidence_ppm
                .cmp(&left.confidence_ppm)
                .then_with(|| right.observations.cmp(&left.observations))
                .then_with(|| left.proposal_id.cmp(&right.proposal_id))
        });
        candidates.truncate(max_candidates);
        Ok(candidates)
    }
}

/// Result of one structural train/holdout commit gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticStructuralLearningReport<const BITS: usize> {
    pub proposal_id: u64,
    pub mutation: SemanticStructuralMutation<BITS>,
    pub train_baseline_loss_ppm: u32,
    pub train_candidate_loss_ppm: u32,
    pub holdout_baseline_loss_ppm: u32,
    pub holdout_candidate_loss_ppm: u32,
    pub decision: BooleanCommitDecision,
    pub committed: bool,
}

/// Evaluates and atomically commits one evidence-qualified structural candidate.
pub fn commit_structural_candidate<const BITS: usize, E, TrainObjective, HoldoutObjective>(
    pool: &mut SemanticHypothesisPool<BITS>,
    candidate: &SemanticStructuralCandidate<BITS>,
    commit_policy: BooleanCommitPolicy,
    mut train_objective: TrainObjective,
    mut holdout_objective: HoldoutObjective,
) -> Result<SemanticStructuralLearningReport<BITS>, SemanticStructuralError<E>>
where
    TrainObjective: FnMut(&SemanticHypothesisPool<BITS>) -> Result<u32, E>,
    HoldoutObjective: FnMut(&SemanticHypothesisPool<BITS>) -> Result<u32, E>,
{
    let train_baseline_loss_ppm = evaluate_objective(pool, &mut train_objective)?;
    let holdout_baseline_loss_ppm = evaluate_objective(pool, &mut holdout_objective)?;

    let candidate_pool = candidate
        .mutation
        .preview(pool)
        .map_err(convert_unit_error)?;
    let train_candidate_loss_ppm = evaluate_objective(&candidate_pool, &mut train_objective)?;
    let holdout_candidate_loss_ppm = evaluate_objective(&candidate_pool, &mut holdout_objective)?;

    let decision = BooleanCommitEvidence {
        train_baseline_loss_ppm,
        train_candidate_loss_ppm,
        holdout_baseline_loss_ppm,
        holdout_candidate_loss_ppm,
    }
    .decide(commit_policy)
    .map_err(SemanticStructuralError::Commit)?;

    let committed = decision == BooleanCommitDecision::Commit;
    if committed {
        *pool = candidate_pool;
    }

    Ok(SemanticStructuralLearningReport {
        proposal_id: candidate.proposal_id,
        mutation: candidate.mutation.clone(),
        train_baseline_loss_ppm,
        train_candidate_loss_ppm,
        holdout_baseline_loss_ppm,
        holdout_candidate_loss_ppm,
        decision,
        committed,
    })
}

fn preview_bind<const BITS: usize>(
    pool: &SemanticHypothesisPool<BITS>,
    hypothesis: usize,
    role: &SemanticBitField<BITS>,
) -> Result<SemanticHypothesisPool<BITS>, SemanticStructuralError<()>> {
    if role.count_ones() == 0 {
        return Err(SemanticStructuralError::EmptyRole);
    }
    let mut hypotheses = pool.hypotheses.clone();
    let hypothesis_count = hypotheses.len();
    let state =
        hypotheses
            .get_mut(hypothesis)
            .ok_or(SemanticStructuralError::HypothesisOutOfRange {
                index: hypothesis,
                len: hypothesis_count,
            })?;
    state.value = bind(role, &state.value).map_err(SemanticStructuralError::Algebra)?;
    Ok(SemanticHypothesisPool { hypotheses })
}

fn preview_split<const BITS: usize>(
    pool: &SemanticHypothesisPool<BITS>,
    hypothesis: usize,
    left_mask: &SemanticBitField<BITS>,
    right_mask: &SemanticBitField<BITS>,
) -> Result<SemanticHypothesisPool<BITS>, SemanticStructuralError<()>> {
    if left_mask.count_ones() == 0 || right_mask.count_ones() == 0 {
        return Err(SemanticStructuralError::EmptySplitMask);
    }

    let parent =
        pool.hypotheses
            .get(hypothesis)
            .ok_or(SemanticStructuralError::HypothesisOutOfRange {
                index: hypothesis,
                len: pool.hypotheses.len(),
            })?;

    let mut overlap = left_mask.clone();
    overlap.and_assign(right_mask);
    if overlap.count_ones() != 0 {
        return Err(SemanticStructuralError::OverlappingSplitMasks);
    }

    let mut union = left_mask.clone();
    union.or_assign(right_mask);
    if union != parent.meaning_mask {
        return Err(SemanticStructuralError::SplitDoesNotPartitionMeaningMask);
    }

    let mut left = parent.clone();
    left.meaning_mask = left_mask.clone();
    left.ambiguity.and_assign(left_mask);

    let mut right = parent.clone();
    right.meaning_mask = right_mask.clone();
    right.ambiguity.and_assign(right_mask);

    let mut hypotheses = pool.hypotheses.clone();
    hypotheses[hypothesis] = left;
    hypotheses.insert(hypothesis + 1, right);
    Ok(SemanticHypothesisPool { hypotheses })
}

fn preview_merge<const BITS: usize>(
    pool: &SemanticHypothesisPool<BITS>,
    left: usize,
    right: usize,
    anchor: &SemanticState<BITS>,
) -> Result<SemanticHypothesisPool<BITS>, SemanticStructuralError<()>> {
    if left == right {
        return Err(SemanticStructuralError::MergeSameHypothesis(left));
    }
    let left_state =
        pool.hypotheses
            .get(left)
            .ok_or(SemanticStructuralError::HypothesisOutOfRange {
                index: left,
                len: pool.hypotheses.len(),
            })?;
    let right_state =
        pool.hypotheses
            .get(right)
            .ok_or(SemanticStructuralError::HypothesisOutOfRange {
                index: right,
                len: pool.hypotheses.len(),
            })?;

    let merged = SemanticState {
        value: bundle_majority(&[&left_state.value, &right_state.value, &anchor.value])
            .map_err(SemanticStructuralError::Algebra)?,
        meaning_mask: bundle_majority(&[
            &left_state.meaning_mask,
            &right_state.meaning_mask,
            &anchor.meaning_mask,
        ])
        .map_err(SemanticStructuralError::Algebra)?,
        ambiguity: bundle_majority(&[
            &left_state.ambiguity,
            &right_state.ambiguity,
            &anchor.ambiguity,
        ])
        .map_err(SemanticStructuralError::Algebra)?,
        routing: bundle_majority(&[&left_state.routing, &right_state.routing, &anchor.routing])
            .map_err(SemanticStructuralError::Algebra)?,
    };

    let low = left.min(right);
    let high = left.max(right);
    let mut hypotheses = pool.hypotheses.clone();
    hypotheses[low] = merged;
    hypotheses.remove(high);
    Ok(SemanticHypothesisPool { hypotheses })
}

fn evaluate_objective<const BITS: usize, E, Objective>(
    pool: &SemanticHypothesisPool<BITS>,
    objective: &mut Objective,
) -> Result<u32, SemanticStructuralError<E>>
where
    Objective: FnMut(&SemanticHypothesisPool<BITS>) -> Result<u32, E>,
{
    let loss = objective(pool).map_err(SemanticStructuralError::Objective)?;
    if loss > BOOLEAN_INTEGRAL_SCALE_PPM {
        return Err(SemanticStructuralError::LossOutOfRange(loss));
    }
    Ok(loss)
}

fn convert_unit_error<E>(error: SemanticStructuralError<()>) -> SemanticStructuralError<E> {
    match error {
        SemanticStructuralError::EmptyHypothesisPool => {
            SemanticStructuralError::EmptyHypothesisPool
        }
        SemanticStructuralError::HypothesisOutOfRange { index, len } => {
            SemanticStructuralError::HypothesisOutOfRange { index, len }
        }
        SemanticStructuralError::EmptyRole => SemanticStructuralError::EmptyRole,
        SemanticStructuralError::EmptySplitMask => SemanticStructuralError::EmptySplitMask,
        SemanticStructuralError::OverlappingSplitMasks => {
            SemanticStructuralError::OverlappingSplitMasks
        }
        SemanticStructuralError::SplitDoesNotPartitionMeaningMask => {
            SemanticStructuralError::SplitDoesNotPartitionMeaningMask
        }
        SemanticStructuralError::MergeSameHypothesis(index) => {
            SemanticStructuralError::MergeSameHypothesis(index)
        }
        SemanticStructuralError::EmptyProposalRegistry => {
            SemanticStructuralError::EmptyProposalRegistry
        }
        SemanticStructuralError::DuplicateProposalId(id) => {
            SemanticStructuralError::DuplicateProposalId(id)
        }
        SemanticStructuralError::UnknownProposalId(id) => {
            SemanticStructuralError::UnknownProposalId(id)
        }
        SemanticStructuralError::ProposalIndexOutOfRange(index) => {
            SemanticStructuralError::ProposalIndexOutOfRange(index)
        }
        SemanticStructuralError::EmptyProposalObservation => {
            SemanticStructuralError::EmptyProposalObservation
        }
        SemanticStructuralError::ZeroCandidateBudget => {
            SemanticStructuralError::ZeroCandidateBudget
        }
        SemanticStructuralError::LossOutOfRange(loss) => {
            SemanticStructuralError::LossOutOfRange(loss)
        }
        SemanticStructuralError::Integral(error) => SemanticStructuralError::Integral(error),
        SemanticStructuralError::Algebra(error) => SemanticStructuralError::Algebra(error),
        SemanticStructuralError::Commit(error) => SemanticStructuralError::Commit(error),
        SemanticStructuralError::Objective(()) => {
            unreachable!("unit objective error is never converted")
        }
    }
}

/// Invalid structural semantic mutation, evidence or objective state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticStructuralError<E> {
    EmptyHypothesisPool,
    HypothesisOutOfRange { index: usize, len: usize },
    EmptyRole,
    EmptySplitMask,
    OverlappingSplitMasks,
    SplitDoesNotPartitionMeaningMask,
    MergeSameHypothesis(usize),
    EmptyProposalRegistry,
    DuplicateProposalId(u64),
    UnknownProposalId(u64),
    ProposalIndexOutOfRange(usize),
    EmptyProposalObservation,
    ZeroCandidateBudget,
    LossOutOfRange(u32),
    Integral(BooleanIntegralError),
    Algebra(SemanticAlgebraError),
    Commit(HierarchicalBooleanError),
    Objective(E),
}

impl<E: fmt::Debug> fmt::Display for SemanticStructuralError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<E: fmt::Debug> std::error::Error for SemanticStructuralError<E> {}

#[cfg(test)]
mod tests {
    use super::{
        commit_structural_candidate, SemanticHypothesisPool, SemanticStructuralCandidate,
        SemanticStructuralEvidenceLedger, SemanticStructuralMutation,
        SemanticStructuralObservation, SemanticStructuralProposal,
    };
    use crate::{
        boolean_hierarchical::{BooleanCommitDecision, BooleanCommitPolicy},
        boolean_integral::BooleanIntegralPolicy,
        semantic::{bind, bundle_majority, SemanticBitField, SemanticState},
    };

    fn state(
        value: &[usize],
        meaning_mask: &[usize],
        ambiguity: &[usize],
        routing: &[usize],
    ) -> SemanticState<8> {
        SemanticState {
            value: SemanticBitField::from_set_indices(value).expect("value"),
            meaning_mask: SemanticBitField::from_set_indices(meaning_mask).expect("mask"),
            ambiguity: SemanticBitField::from_set_indices(ambiguity).expect("ambiguity"),
            routing: SemanticBitField::from_set_indices(routing).expect("routing"),
        }
    }

    #[test]
    fn bind_is_real_sbg_algebra_transformation() {
        let original = state(&[0, 2, 4], &[0, 1, 2, 3], &[1], &[6]);
        let role = SemanticBitField::<8>::from_set_indices(&[1, 5]).expect("role");
        let pool = SemanticHypothesisPool::new(vec![original.clone()]).expect("pool");
        let mutation = SemanticStructuralMutation::Bind {
            hypothesis: 0,
            role: role.clone(),
        };

        let preview = mutation.preview(&pool).expect("preview");
        assert_eq!(
            preview.hypotheses()[0].value,
            bind(&role, &original.value).expect("bound")
        );
        assert_eq!(preview.hypotheses()[0].meaning_mask, original.meaning_mask);
    }

    #[test]
    fn split_creates_two_context_partition_hypotheses() {
        let original = state(&[0, 2, 4, 6], &[0, 1, 4, 5], &[0, 4, 7], &[2, 3]);
        let pool = SemanticHypothesisPool::new(vec![original]).expect("pool");
        let left = SemanticBitField::<8>::from_set_indices(&[0, 1]).expect("left");
        let right = SemanticBitField::<8>::from_set_indices(&[4, 5]).expect("right");

        let preview = SemanticStructuralMutation::Split {
            hypothesis: 0,
            left_mask: left.clone(),
            right_mask: right.clone(),
        }
        .preview(&pool)
        .expect("split");

        assert_eq!(preview.len(), 2);
        assert_eq!(preview.hypotheses()[0].meaning_mask, left);
        assert_eq!(preview.hypotheses()[1].meaning_mask, right);
        assert_eq!(preview.hypotheses()[0].ambiguity.count_ones(), 1);
        assert_eq!(preview.hypotheses()[1].ambiguity.count_ones(), 1);
    }

    #[test]
    fn merge_uses_explicit_anchor_for_strict_majority() {
        let left = state(&[0, 1], &[0, 1], &[0], &[6]);
        let right = state(&[0, 2], &[0, 2], &[2], &[6]);
        let anchor = state(&[0, 3], &[0, 3], &[3], &[6]);
        let pool = SemanticHypothesisPool::new(vec![left.clone(), right.clone()]).expect("pool");

        let preview = SemanticStructuralMutation::Merge {
            left: 0,
            right: 1,
            anchor: anchor.clone(),
        }
        .preview(&pool)
        .expect("merge");

        assert_eq!(preview.len(), 1);
        assert_eq!(
            preview.hypotheses()[0].value,
            bundle_majority(&[&left.value, &right.value, &anchor.value]).expect("bundle")
        );
    }

    #[test]
    fn boolean_integral_evidence_recommends_structural_proposal() {
        let role = SemanticBitField::<8>::from_set_indices(&[1, 5]).expect("role");
        let proposal = SemanticStructuralProposal {
            proposal_id: 17,
            mutation: SemanticStructuralMutation::Bind {
                hypothesis: 0,
                role,
            },
        };
        let mut ledger = SemanticStructuralEvidenceLedger::new(
            vec![proposal],
            BooleanIntegralPolicy {
                min_observations: 2,
                min_confidence_ppm: 500_000,
                max_mask_width: 2,
            },
        )
        .expect("ledger");
        let observation = SemanticStructuralObservation {
            proposal_ids: vec![17],
            baseline_loss_ppm: 900_000,
            perturbed_loss_ppm: 200_000,
        };

        ledger.observe(&observation).expect("first");
        assert!(ledger
            .recommended_candidates(1)
            .expect("candidates")
            .is_empty());

        ledger.observe(&observation).expect("second");
        let candidates = ledger.recommended_candidates(1).expect("candidates");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].proposal_id, 17);
    }

    #[test]
    fn split_can_commit_through_train_and_holdout_gate() {
        let original = state(&[0, 2], &[0, 1, 4, 5], &[0, 4], &[6]);
        let mut pool = SemanticHypothesisPool::new(vec![original]).expect("pool");
        let candidate = SemanticStructuralCandidate {
            proposal_id: 9,
            mutation: SemanticStructuralMutation::Split {
                hypothesis: 0,
                left_mask: SemanticBitField::from_set_indices(&[0, 1]).expect("left"),
                right_mask: SemanticBitField::from_set_indices(&[4, 5]).expect("right"),
            },
            observations: 3,
            confidence_ppm: 1_000_000,
        };

        let report = commit_structural_candidate(
            &mut pool,
            &candidate,
            BooleanCommitPolicy::default(),
            |candidate_pool| {
                Ok::<u32, ()>(if candidate_pool.len() == 2 {
                    100_000
                } else {
                    800_000
                })
            },
            |candidate_pool| {
                Ok::<u32, ()>(if candidate_pool.len() == 2 {
                    200_000
                } else {
                    700_000
                })
            },
        )
        .expect("commit");

        assert_eq!(report.decision, BooleanCommitDecision::Commit);
        assert!(report.committed);
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn holdout_rejection_preserves_original_hypothesis_pool() {
        let original = state(&[0, 2], &[0, 1, 4, 5], &[0, 4], &[6]);
        let mut pool = SemanticHypothesisPool::new(vec![original]).expect("pool");
        let before = pool.clone();
        let candidate = SemanticStructuralCandidate {
            proposal_id: 9,
            mutation: SemanticStructuralMutation::Split {
                hypothesis: 0,
                left_mask: SemanticBitField::from_set_indices(&[0, 1]).expect("left"),
                right_mask: SemanticBitField::from_set_indices(&[4, 5]).expect("right"),
            },
            observations: 3,
            confidence_ppm: 1_000_000,
        };

        let report = commit_structural_candidate(
            &mut pool,
            &candidate,
            BooleanCommitPolicy::default(),
            |candidate_pool| {
                Ok::<u32, ()>(if candidate_pool.len() == 2 {
                    100_000
                } else {
                    800_000
                })
            },
            |candidate_pool| {
                Ok::<u32, ()>(if candidate_pool.len() == 2 {
                    900_000
                } else {
                    200_000
                })
            },
        )
        .expect("gate");

        assert_eq!(report.decision, BooleanCommitDecision::RejectHoldout);
        assert!(!report.committed);
        assert_eq!(pool, before);
    }
}
