use core::fmt;
use std::collections::BTreeSet;

use sml_core::{
    task_loss::{evaluate_task_panel, TaskLossError, TaskPanelEvaluation},
    tasks::{suite_from_case_id, TaskError, TaskInstance, TaskKind},
};

use crate::portfolio::{
    entry_better, CurriculumCandidate, CurriculumPortfolioReport, CurriculumSource,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PostSelectionPolicy {
    pub min_holdout_tasks: usize,
    pub require_all_task_families: bool,
}

impl Default for PostSelectionPolicy {
    fn default() -> Self {
        Self {
            min_holdout_tasks: 32,
            require_all_task_families: true,
        }
    }
}

impl PostSelectionPolicy {
    pub fn validate(self) -> Result<Self, PostSelectionError> {
        if self.min_holdout_tasks == 0 {
            return Err(PostSelectionError::InvalidPolicy);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostSelectionOutcome {
    SelectedBetter,
    Tie,
    SelectedWorse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostSelectionReport {
    pub selected_id: String,
    pub fixed_control_id: String,
    pub selected_holdout: TaskPanelEvaluation,
    pub fixed_holdout: TaskPanelEvaluation,
    pub holdout_tasks: u64,
    pub exact_task_delta: i64,
    pub selected_training_events: u64,
    pub fixed_training_events: u64,
    pub outcome: PostSelectionOutcome,
}

pub fn evaluate_post_selection_holdout(
    portfolio: &CurriculumPortfolioReport,
    candidates: &[CurriculumCandidate],
    holdout: &[TaskInstance],
    policy: PostSelectionPolicy,
) -> Result<PostSelectionReport, PostSelectionError> {
    let policy = policy.validate()?;
    validate_portfolio_binding(portfolio, candidates)?;
    validate_holdout(portfolio, candidates, holdout, policy)?;

    let selected = portfolio
        .entries
        .get(portfolio.selected_index)
        .ok_or(PostSelectionError::InvalidPortfolio)?;
    if selected.candidate_id != portfolio.selected_id {
        return Err(PostSelectionError::InvalidPortfolio);
    }

    let fixed_index = best_fixed_index(portfolio)?;
    let fixed = &portfolio.entries[fixed_index];

    let selected_holdout = evaluate_task_panel(&selected.training.candidate, holdout)
        .map_err(PostSelectionError::TaskLoss)?;
    let fixed_holdout = evaluate_task_panel(&fixed.training.candidate, holdout)
        .map_err(PostSelectionError::TaskLoss)?;

    let exact_task_delta = i64::try_from(selected_holdout.exact_tasks)
        .and_then(|left| i64::try_from(fixed_holdout.exact_tasks).map(|right| left - right))
        .map_err(|_| PostSelectionError::Overflow)?;
    let outcome = match selected_holdout.exact_tasks.cmp(&fixed_holdout.exact_tasks) {
        core::cmp::Ordering::Greater => PostSelectionOutcome::SelectedBetter,
        core::cmp::Ordering::Equal => PostSelectionOutcome::Tie,
        core::cmp::Ordering::Less => PostSelectionOutcome::SelectedWorse,
    };

    Ok(PostSelectionReport {
        selected_id: selected.candidate_id.clone(),
        fixed_control_id: fixed.candidate_id.clone(),
        selected_holdout,
        fixed_holdout,
        holdout_tasks: u64::try_from(holdout.len()).map_err(|_| PostSelectionError::Overflow)?,
        exact_task_delta,
        selected_training_events: selected.training_events,
        fixed_training_events: fixed.training_events,
        outcome,
    })
}

fn validate_portfolio_binding(
    portfolio: &CurriculumPortfolioReport,
    candidates: &[CurriculumCandidate],
) -> Result<(), PostSelectionError> {
    if portfolio.entries.is_empty()
        || portfolio.selected_index >= portfolio.entries.len()
        || portfolio.entries[portfolio.selected_index].candidate_id != portfolio.selected_id
    {
        return Err(PostSelectionError::InvalidPortfolio);
    }
    if portfolio.entries.len() != candidates.len() {
        return Err(PostSelectionError::CandidateBindingMismatch);
    }

    let portfolio_ids = portfolio
        .entries
        .iter()
        .map(|entry| entry.candidate_id.as_str())
        .collect::<BTreeSet<_>>();
    let candidate_ids = candidates
        .iter()
        .map(|candidate| candidate.candidate_id.as_str())
        .collect::<BTreeSet<_>>();
    if portfolio_ids.len() != portfolio.entries.len()
        || candidate_ids.len() != candidates.len()
        || portfolio_ids != candidate_ids
    {
        return Err(PostSelectionError::CandidateBindingMismatch);
    }

    for candidate in candidates {
        candidate
            .curriculum
            .validate()
            .map_err(PostSelectionError::Curriculum)?;
    }
    Ok(())
}

fn validate_holdout(
    portfolio: &CurriculumPortfolioReport,
    candidates: &[CurriculumCandidate],
    holdout: &[TaskInstance],
    policy: PostSelectionPolicy,
) -> Result<(), PostSelectionError> {
    if holdout.len() < policy.min_holdout_tasks {
        return Err(PostSelectionError::InsufficientHoldout {
            actual: holdout.len(),
            minimum: policy.min_holdout_tasks,
        });
    }

    let selection_panel = suite_from_case_id(
        portfolio.selection_start_case_id,
        portfolio.selection_cases_per_kind,
    )
    .map_err(PostSelectionError::Task)?;
    let selection_ids = selection_panel
        .iter()
        .map(|task| task_identity(task.kind, task.case_id))
        .collect::<BTreeSet<_>>();
    let curriculum_ids = candidates
        .iter()
        .flat_map(|candidate| candidate.curriculum.tasks.iter())
        .map(|item| task_identity(item.task.kind, item.task.case_id))
        .collect::<BTreeSet<_>>();

    let mut holdout_ids = BTreeSet::new();
    let mut family_mask = 0_u8;
    for task in holdout {
        if !task.oracle_matches().map_err(PostSelectionError::Task)? {
            return Err(PostSelectionError::OracleMismatch(task.case_id));
        }
        let identity = task_identity(task.kind, task.case_id);
        if !holdout_ids.insert(identity) {
            return Err(PostSelectionError::DuplicateHoldoutTask(task.case_id));
        }
        if selection_ids.contains(&identity) {
            return Err(PostSelectionError::HoldoutSelectionOverlap(task.case_id));
        }
        if curriculum_ids.contains(&identity) {
            return Err(PostSelectionError::HoldoutCurriculumOverlap(task.case_id));
        }
        family_mask |= 1_u8 << task_kind_tag(task.kind);
    }

    if policy.require_all_task_families && family_mask != 0b11_1111 {
        return Err(PostSelectionError::IncompleteFamilyCoverage(family_mask));
    }
    Ok(())
}

fn best_fixed_index(portfolio: &CurriculumPortfolioReport) -> Result<usize, PostSelectionError> {
    let mut best = None;
    for (index, entry) in portfolio.entries.iter().enumerate() {
        if entry.source != CurriculumSource::FixedControl {
            continue;
        }
        best = match best {
            None => Some(index),
            Some(current) if entry_better(entry, &portfolio.entries[current]) => Some(index),
            Some(current) => Some(current),
        };
    }
    best.ok_or(PostSelectionError::MissingFixedControl)
}

const fn task_identity(kind: TaskKind, case_id: u64) -> (u8, u64) {
    (task_kind_tag(kind), case_id)
}

const fn task_kind_tag(kind: TaskKind) -> u8 {
    match kind {
        TaskKind::DelayedBitRecall => 0,
        TaskKind::OverwriteBitRecall => 1,
        TaskKind::KeyedFactRecall => 2,
        TaskKind::CopyAfterMarker => 3,
        TaskKind::TwoFactConjunction => 4,
        TaskKind::DistractorRejection => 5,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PostSelectionError {
    InvalidPolicy,
    InvalidPortfolio,
    CandidateBindingMismatch,
    MissingFixedControl,
    InsufficientHoldout { actual: usize, minimum: usize },
    DuplicateHoldoutTask(u64),
    HoldoutSelectionOverlap(u64),
    HoldoutCurriculumOverlap(u64),
    IncompleteFamilyCoverage(u8),
    OracleMismatch(u64),
    Overflow,
    Curriculum(crate::curriculum::CurriculumError),
    Task(TaskError),
    TaskLoss(TaskLossError),
}

impl fmt::Display for PostSelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for PostSelectionError {}
