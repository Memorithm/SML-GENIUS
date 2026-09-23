//! SML-D1c bounded semantic feature-pair selection.
//!
//! Training alone selects one pair of already-declared SBG features and one
//! native two-input truth table. Validation can reject that proposal but cannot
//! choose a different pair or table. This is a finite feature-selection fixture,
//! not learned semantic encoding or arbitrary question understanding.

use core::{fmt, mem::size_of};
use std::collections::HashSet;

use crate::{
    semantic::{
        LabeledSemanticInput, ProjectionBudget, ProjectionCounters, SemanticDecisionError,
        SemanticFeature, SemanticProjection,
    },
    InputRecord, TruthTable2,
};

/// Declared fixed-coordinate control used for matched comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedFeatureControl {
    pub feature_slots: [usize; 2],
    pub table: TruthTable2,
}

/// Bounded feature-search policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureSearchPolicy {
    pub max_samples: usize,
    pub max_candidates: usize,
    pub max_candidate_pairs: usize,
    pub max_selection_evaluations: u64,
    pub min_train_error_reduction: u64,
    pub max_validation_errors: u64,
    pub max_validation_missing: u64,
}

impl Default for FeatureSearchPolicy {
    fn default() -> Self {
        Self {
            max_samples: 4096,
            max_candidates: 16,
            max_candidate_pairs: 120,
            max_selection_evaluations: 8_000_000,
            min_train_error_reduction: 1,
            max_validation_errors: 0,
            max_validation_missing: 0,
        }
    }
}

/// Frozen proposal returned by training-only selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureExpertProposal {
    pub left_slot: usize,
    pub right_slot: usize,
    pub left_feature: SemanticFeature,
    pub right_feature: SemanticFeature,
    pub table: TruthTable2,
}

impl FeatureExpertProposal {
    #[must_use]
    pub const fn feature_slots(self) -> [usize; 2] {
        [self.left_slot, self.right_slot]
    }

    #[must_use]
    pub const fn features(self) -> [SemanticFeature; 2] {
        [self.left_feature, self.right_feature]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureSearchOutcome {
    Accepted,
    Rejected,
}

/// Evidence from bounded feature-pair search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeatureSearchReport {
    pub outcome: FeatureSearchOutcome,
    pub selected: FeatureExpertProposal,
    pub training_samples: u64,
    pub validation_samples: u64,
    pub candidate_features: u64,
    pub candidate_pairs: u64,
    pub training_eligible_pairs: u64,
    pub training_ineligible_pairs: u64,
    pub selection_evaluations_bound: u64,
    pub selection_evaluations_executed: u64,
    pub baseline_training_errors: u64,
    pub selected_training_errors: u64,
    pub baseline_validation_errors: u64,
    pub selected_validation_errors: u64,
    pub selected_validation_missing: u64,
    pub selected_validation_coverage: u8,
    pub projection: ProjectionCounters,
}

/// Selects one semantic feature pair and one truth table using training data only.
///
/// Candidate pairs must be fully known and cover all four Boolean input patterns
/// on training data. Validation is evaluated only after the pair/table is frozen.
/// Missing selected features on validation count as failures and are reported.
pub fn search_feature_pair_expert<const BITS: usize>(
    projection: &SemanticProjection<BITS>,
    training: &[LabeledSemanticInput<'_, BITS>],
    validation: &[LabeledSemanticInput<'_, BITS>],
    baseline: FixedFeatureControl,
    policy: FeatureSearchPolicy,
    projection_budget: ProjectionBudget,
) -> Result<FeatureSearchReport, FeatureSearchError> {
    if training.is_empty() || validation.is_empty() {
        return Err(FeatureSearchError::EmptySplit);
    }

    let candidate_count = projection.features().len();
    if candidate_count < 2 || candidate_count > policy.max_candidates {
        return Err(FeatureSearchError::CandidateBudget);
    }

    validate_pair(baseline.feature_slots, candidate_count)
        .map_err(|_| FeatureSearchError::InvalidBaselinePair)?;

    let total = training
        .len()
        .checked_add(validation.len())
        .ok_or(FeatureSearchError::Overflow)?;
    if total > policy.max_samples {
        return Err(FeatureSearchError::SampleBudget);
    }

    let pair_count = candidate_count
        .checked_mul(candidate_count - 1)
        .and_then(|value| value.checked_div(2))
        .ok_or(FeatureSearchError::Overflow)?;
    if pair_count > policy.max_candidate_pairs {
        return Err(FeatureSearchError::PairBudget);
    }

    let train_count = u64::try_from(training.len()).map_err(|_| FeatureSearchError::Overflow)?;
    let pair_count_u64 = u64::try_from(pair_count).map_err(|_| FeatureSearchError::Overflow)?;
    let selection_evaluations_bound = train_count
        .checked_mul(pair_count_u64)
        .and_then(|value| value.checked_mul(16))
        .ok_or(FeatureSearchError::Overflow)?;
    if selection_evaluations_bound > policy.max_selection_evaluations {
        return Err(FeatureSearchError::EvaluationBudget);
    }

    let projected_payload = total
        .checked_mul(size_of::<ProjectedLabel>())
        .ok_or(FeatureSearchError::Overflow)?;
    if projected_payload > projection_budget.max_buffer_bytes {
        return Err(FeatureSearchError::Projection(
            SemanticDecisionError::ProjectionBudget,
        ));
    }

    projection
        .plan(total, projection_budget)
        .map_err(FeatureSearchError::Projection)?;

    let mut ids = HashSet::new();
    ids.try_reserve(total)
        .map_err(|_| FeatureSearchError::Allocation)?;
    for sample in training.iter().chain(validation) {
        if !ids.insert(sample.input.record_id) {
            return Err(FeatureSearchError::DuplicateCaseId);
        }
    }

    // Training is projected and searched before validation is read.
    let mut train_rows = Vec::new();
    train_rows
        .try_reserve_exact(training.len())
        .map_err(|_| FeatureSearchError::Allocation)?;
    let mut projection_counters = ProjectionCounters::default();
    for sample in training {
        let (record, work) = projection
            .project(sample.input)
            .map_err(FeatureSearchError::Projection)?;
        add_projection_counters(&mut projection_counters, work);
        train_rows.push(ProjectedLabel {
            record,
            target: sample.target,
        });
    }

    let baseline_training_errors =
        pair_errors(&train_rows, baseline.feature_slots, baseline.table).0;

    let mut selected = None;
    let mut selected_errors = u64::MAX;
    let mut training_eligible_pairs = 0_u64;
    let mut training_ineligible_pairs = 0_u64;
    let mut selection_evaluations_executed = 0_u64;

    for left in 0..candidate_count {
        for right in (left + 1)..candidate_count {
            let pair = [left, right];
            let Some(coverage) = pair_training_coverage(&train_rows, pair) else {
                training_ineligible_pairs = training_ineligible_pairs.saturating_add(1);
                continue;
            };
            if coverage != 0b1111 {
                training_ineligible_pairs = training_ineligible_pairs.saturating_add(1);
                continue;
            }
            training_eligible_pairs = training_eligible_pairs.saturating_add(1);

            for code in 0..16 {
                let table =
                    TruthTable2::new(code).map_err(|_| FeatureSearchError::InvalidTable)?;
                let errors = pair_errors_known(&train_rows, pair, table);
                selection_evaluations_executed = selection_evaluations_executed
                    .checked_add(train_count)
                    .ok_or(FeatureSearchError::Overflow)?;
                if errors < selected_errors {
                    selected_errors = errors;
                    selected = Some(FeatureExpertProposal {
                        left_slot: left,
                        right_slot: right,
                        left_feature: projection.features()[left],
                        right_feature: projection.features()[right],
                        table,
                    });
                }
            }
        }
    }

    let selected = selected.ok_or(FeatureSearchError::NoEligiblePair)?;

    // Validation is not touched until the selected pair/table is frozen.
    let mut validation_rows = Vec::new();
    validation_rows
        .try_reserve_exact(validation.len())
        .map_err(|_| FeatureSearchError::Allocation)?;
    for sample in validation {
        let (record, work) = projection
            .project(sample.input)
            .map_err(FeatureSearchError::Projection)?;
        add_projection_counters(&mut projection_counters, work);
        validation_rows.push(ProjectedLabel {
            record,
            target: sample.target,
        });
    }

    let (baseline_validation_errors, _) =
        pair_errors(&validation_rows, baseline.feature_slots, baseline.table);
    let (selected_validation_errors, selected_validation_missing) = pair_errors(
        &validation_rows,
        selected.feature_slots(),
        selected.table,
    );
    let selected_validation_coverage =
        pair_coverage_allow_missing(&validation_rows, selected.feature_slots());

    let train_ok = selected_errors
        .checked_add(policy.min_train_error_reduction)
        .is_some_and(|value| value <= baseline_training_errors);
    let validation_ok = selected_validation_errors <= baseline_validation_errors
        && selected_validation_errors <= policy.max_validation_errors
        && selected_validation_missing <= policy.max_validation_missing
        && selected_validation_coverage == 0b1111;

    Ok(FeatureSearchReport {
        outcome: if train_ok && validation_ok {
            FeatureSearchOutcome::Accepted
        } else {
            FeatureSearchOutcome::Rejected
        },
        selected,
        training_samples: train_count,
        validation_samples: u64::try_from(validation.len())
            .map_err(|_| FeatureSearchError::Overflow)?,
        candidate_features: u64::try_from(candidate_count)
            .map_err(|_| FeatureSearchError::Overflow)?,
        candidate_pairs: pair_count_u64,
        training_eligible_pairs,
        training_ineligible_pairs,
        selection_evaluations_bound,
        selection_evaluations_executed,
        baseline_training_errors,
        selected_training_errors: selected_errors,
        baseline_validation_errors,
        selected_validation_errors,
        selected_validation_missing,
        selected_validation_coverage,
        projection: projection_counters,
    })
}

#[derive(Debug, Clone, Copy)]
struct ProjectedLabel {
    record: InputRecord,
    target: bool,
}

fn validate_pair(pair: [usize; 2], candidate_count: usize) -> Result<(), ()> {
    if pair[0] == pair[1] || pair.iter().any(|slot| *slot >= candidate_count) {
        Err(())
    } else {
        Ok(())
    }
}

fn pair_training_coverage(rows: &[ProjectedLabel], pair: [usize; 2]) -> Option<u8> {
    let required = (1_u64 << pair[0]) | (1_u64 << pair[1]);
    let mut coverage = 0_u8;
    for row in rows {
        if row.record.known & required != required {
            return None;
        }
        let index = u8::from(row.record.values & (1_u64 << pair[0]) != 0)
            | (u8::from(row.record.values & (1_u64 << pair[1]) != 0) << 1);
        coverage |= 1_u8 << index;
    }
    Some(coverage)
}

fn pair_coverage_allow_missing(rows: &[ProjectedLabel], pair: [usize; 2]) -> u8 {
    let required = (1_u64 << pair[0]) | (1_u64 << pair[1]);
    rows.iter().fold(0_u8, |coverage, row| {
        if row.record.known & required != required {
            coverage
        } else {
            let index = u8::from(row.record.values & (1_u64 << pair[0]) != 0)
                | (u8::from(row.record.values & (1_u64 << pair[1]) != 0) << 1);
            coverage | (1_u8 << index)
        }
    })
}

fn pair_errors_known(rows: &[ProjectedLabel], pair: [usize; 2], table: TruthTable2) -> u64 {
    rows.iter()
        .filter(|row| {
            let left = row.record.values & (1_u64 << pair[0]) != 0;
            let right = row.record.values & (1_u64 << pair[1]) != 0;
            table.evaluate(left, right) != row.target
        })
        .count() as u64
}

fn pair_errors(
    rows: &[ProjectedLabel],
    pair: [usize; 2],
    table: TruthTable2,
) -> (u64, u64) {
    let required = (1_u64 << pair[0]) | (1_u64 << pair[1]);
    let mut errors = 0_u64;
    let mut missing = 0_u64;
    for row in rows {
        if row.record.known & required != required {
            errors = errors.saturating_add(1);
            missing = missing.saturating_add(1);
            continue;
        }
        let left = row.record.values & (1_u64 << pair[0]) != 0;
        let right = row.record.values & (1_u64 << pair[1]) != 0;
        if table.evaluate(left, right) != row.target {
            errors = errors.saturating_add(1);
        }
    }
    (errors, missing)
}

fn add_projection_counters(total: &mut ProjectionCounters, item: ProjectionCounters) {
    total.records = total.records.saturating_add(item.records);
    total.feature_queries = total.feature_queries.saturating_add(item.feature_queries);
    total.observation_mask_reads = total
        .observation_mask_reads
        .saturating_add(item.observation_mask_reads);
    total.semantic_bit_reads = total
        .semantic_bit_reads
        .saturating_add(item.semantic_bit_reads);
    total.known_features = total.known_features.saturating_add(item.known_features);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureSearchError {
    EmptySplit,
    Overflow,
    SampleBudget,
    CandidateBudget,
    PairBudget,
    EvaluationBudget,
    InvalidBaselinePair,
    DuplicateCaseId,
    NoEligiblePair,
    InvalidTable,
    Allocation,
    Projection(SemanticDecisionError),
}

impl From<SemanticDecisionError> for FeatureSearchError {
    fn from(error: SemanticDecisionError) -> Self {
        Self::Projection(error)
    }
}

impl fmt::Display for FeatureSearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for FeatureSearchError {}
