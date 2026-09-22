//! SML-D1a bounded, integer-only reference fitting of one two-input expert.
//!
//! Select among the 16 existing SML truth tables using training data ONLY.
//! A separate validation split can reject, never select, the candidate. This
//! finite function-identification experiment is not language generalization or
//! calibrated uncertainty. Keep a third, untouched test panel for final reports.

use std::collections::HashSet;

use crate::TruthTable2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LabeledDecision {
    pub case_id: u64,
    pub left: bool,
    pub right: bool,
    pub target: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FitPolicy {
    pub max_samples: usize,
    pub min_train_error_reduction: u64,
    pub max_validation_errors: u64,
}

impl Default for FitPolicy {
    fn default() -> Self {
        Self {
            max_samples: 4096,
            min_train_error_reduction: 1,
            max_validation_errors: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitOutcome {
    Accepted(TruthTable2),
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FitReport {
    pub outcome: FitOutcome,
    pub selected: TruthTable2,
    pub training_samples: u64,
    pub validation_samples: u64,
    pub baseline_training_errors: u64,
    pub candidate_training_errors: u64,
    pub baseline_validation_errors: u64,
    pub candidate_validation_errors: u64,
    pub selection_gate_evaluations: u64,
    pub total_gate_evaluations: u64,
    /// Logical table bits in an accepted proposal; not physical storage or a live commit.
    pub accepted_logical_table_bits: u64,
}

/// Exact finite empirical-risk minimizer with fail-closed split/coverage checks.
/// Validation is a development gate, NOT an untouched final test set. Disjoint
/// caller case IDs do not prove that source data are independent or deduplicated.
pub fn fit_boolean_expert(
    training: &[LabeledDecision],
    validation: &[LabeledDecision],
    incumbent: TruthTable2,
    policy: FitPolicy,
) -> Result<FitReport, FitError> {
    if training.is_empty() || validation.is_empty() {
        return Err(FitError::EmptySplit);
    }
    let total = training
        .len()
        .checked_add(validation.len())
        .ok_or(FitError::Overflow)?;
    if total > policy.max_samples {
        return Err(FitError::SampleBudget);
    }
    let train_count = u64::try_from(training.len()).map_err(|_| FitError::Overflow)?;
    let validation_count = u64::try_from(validation.len()).map_err(|_| FitError::Overflow)?;
    let selection_gate_evaluations = train_count.checked_mul(16).ok_or(FitError::Overflow)?;
    let total_gate_evaluations = train_count
        .checked_mul(17)
        .and_then(|n| {
            validation_count
                .checked_mul(2)
                .and_then(|v| n.checked_add(v))
        })
        .ok_or(FitError::Overflow)?;
    let mut ids = HashSet::new();
    ids.try_reserve(total).map_err(|_| FitError::Allocation)?;
    for sample in training.iter().chain(validation) {
        if !ids.insert(sample.case_id) {
            return Err(FitError::DuplicateCaseId);
        }
    }
    if coverage(training) != 0b1111 || coverage(validation) != 0b1111 {
        return Err(FitError::IncompleteInputCoverage);
    }
    let baseline_training_errors = errors(incumbent, training);
    let mut selected = TruthTable2::FALSE;
    let mut candidate_training_errors = u64::MAX;
    for code in 0..16 {
        let table = TruthTable2::new(code).map_err(|_| FitError::InvalidTable)?;
        let loss = errors(table, training);
        // Explicit lowest-code tie break; validation cannot influence this loop.
        if loss < candidate_training_errors {
            selected = table;
            candidate_training_errors = loss;
        }
    }
    let baseline_validation_errors = errors(incumbent, validation);
    let candidate_validation_errors = errors(selected, validation);
    let train_ok = candidate_training_errors
        .checked_add(policy.min_train_error_reduction)
        .is_some_and(|n| n <= baseline_training_errors);
    let accepted = train_ok
        && candidate_validation_errors <= baseline_validation_errors
        && candidate_validation_errors <= policy.max_validation_errors;
    Ok(FitReport {
        outcome: if accepted {
            FitOutcome::Accepted(selected)
        } else {
            FitOutcome::Rejected
        },
        selected,
        training_samples: train_count,
        validation_samples: validation_count,
        baseline_training_errors,
        candidate_training_errors,
        baseline_validation_errors,
        candidate_validation_errors,
        selection_gate_evaluations,
        total_gate_evaluations,
        accepted_logical_table_bits: if accepted { 4 } else { 0 },
    })
}

fn coverage(samples: &[LabeledDecision]) -> u8 {
    samples.iter().fold(0, |mask, sample| {
        mask | (1_u8 << (u8::from(sample.left) | (u8::from(sample.right) << 1)))
    })
}

fn errors(table: TruthTable2, samples: &[LabeledDecision]) -> u64 {
    samples
        .iter()
        .filter(|s| table.evaluate(s.left, s.right) != s.target)
        .count() as u64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitError {
    EmptySplit,
    Overflow,
    SampleBudget,
    DuplicateCaseId,
    IncompleteInputCoverage,
    InvalidTable,
    Allocation,
}

impl core::fmt::Display for FitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FitError {}
