use core::fmt;
use std::collections::BTreeSet;

use sml_core::{
    task_loss::{evaluate_task_panel, TaskLossError, TaskPanelEvaluation},
    tasks::{suite_from_case_id, TaskError, TaskKind},
};

use crate::{
    curriculum::{CurriculumPartition, CurriculumReport},
    multi_task_training::{
        train_multitask_candidate, MultiTaskTrainingError, MultiTaskTrainingPolicy,
        MultiTaskTrainingReport,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurriculumSource {
    FixedControl,
    ModelProposed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurriculumCandidate {
    pub candidate_id: String,
    pub source: CurriculumSource,
    pub curriculum: CurriculumReport,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortfolioPolicy {
    pub training: MultiTaskTrainingPolicy,
    pub max_candidates: usize,
    pub selection_start_case_id: u64,
    pub selection_cases_per_kind: u32,
}

impl Default for PortfolioPolicy {
    fn default() -> Self {
        Self {
            training: MultiTaskTrainingPolicy::default(),
            max_candidates: 8,
            selection_start_case_id: 4_000_000,
            selection_cases_per_kind: 32,
        }
    }
}

impl PortfolioPolicy {
    pub fn validate(self) -> Result<Self, PortfolioError> {
        self.training
            .validate()
            .map_err(PortfolioError::Training)?;
        if self.max_candidates < 2 || self.selection_cases_per_kind == 0 {
            return Err(PortfolioError::InvalidPolicy);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainingShape {
    pub delayed_recall: u64,
    pub overwrite: u64,
    pub keyed_recall: u64,
    pub copy: u64,
    pub conjunction: u64,
    pub distractor: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PortfolioEntry {
    pub candidate_id: String,
    pub source: CurriculumSource,
    pub training_shape: TrainingShape,
    pub training_events: u64,
    pub training: MultiTaskTrainingReport,
    pub selection: TaskPanelEvaluation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CurriculumPortfolioReport {
    pub entries: Vec<PortfolioEntry>,
    pub selected_index: usize,
    pub selected_id: String,
    pub selection_tasks: u64,
}

impl CurriculumPortfolioReport {
    #[must_use]
    pub fn selected(&self) -> &PortfolioEntry {
        &self.entries[self.selected_index]
    }
}

pub fn run_curriculum_portfolio(
    candidates: &[CurriculumCandidate],
    policy: PortfolioPolicy,
) -> Result<CurriculumPortfolioReport, PortfolioError> {
    let policy = policy.validate()?;
    if candidates.len() < 2 || candidates.len() > policy.max_candidates {
        return Err(PortfolioError::CandidateBudget);
    }

    let mut ids = BTreeSet::new();
    let mut has_fixed = false;
    let mut has_proposed = false;
    let mut reference_shape = None;

    for candidate in candidates {
        validate_id(&candidate.candidate_id)?;
        if !ids.insert(candidate.candidate_id.as_str()) {
            return Err(PortfolioError::DuplicateCandidateId);
        }
        candidate
            .curriculum
            .validate()
            .map_err(PortfolioError::Curriculum)?;
        match candidate.source {
            CurriculumSource::FixedControl => has_fixed = true,
            CurriculumSource::ModelProposed => has_proposed = true,
        }
        let shape = training_shape(&candidate.curriculum);
        if let Some(reference) = reference_shape {
            if shape != reference {
                return Err(PortfolioError::UnmatchedTrainingShape);
            }
        } else {
            reference_shape = Some(shape);
        }
    }
    if !has_fixed || !has_proposed {
        return Err(PortfolioError::MissingSourceClass);
    }

    let selection_panel = suite_from_case_id(
        policy.selection_start_case_id,
        policy.selection_cases_per_kind,
    )
    .map_err(PortfolioError::Task)?;
    for task in &selection_panel {
        let identity = (task_kind_tag(task.kind), task.case_id);
        if candidates.iter().any(|candidate| {
            candidate
                .curriculum
                .tasks
                .iter()
                .any(|item| (task_kind_tag(item.task.kind), item.task.case_id) == identity)
        }) {
            return Err(PortfolioError::SelectionOverlap(identity.1));
        }
    }

    let mut entries = Vec::new();
    entries
        .try_reserve_exact(candidates.len())
        .map_err(|_| PortfolioError::Allocation)?;

    for candidate in candidates {
        let training =
            train_multitask_candidate(&candidate.curriculum, policy.training)
                .map_err(PortfolioError::Training)?;
        let selection = evaluate_task_panel(&training.candidate, &selection_panel)
            .map_err(PortfolioError::TaskLoss)?;
        entries.push(PortfolioEntry {
            candidate_id: candidate.candidate_id.clone(),
            source: candidate.source,
            training_shape: training_shape(&candidate.curriculum),
            training_events: training_events(&candidate.curriculum)?,
            training,
            selection,
        });
    }

    let mut selected_index = 0_usize;
    for index in 1..entries.len() {
        if entry_better(&entries[index], &entries[selected_index]) {
            selected_index = index;
        }
    }

    Ok(CurriculumPortfolioReport {
        selected_id: entries[selected_index].candidate_id.clone(),
        selected_index,
        selection_tasks: u64::try_from(selection_panel.len()).map_err(|_| PortfolioError::Overflow)?,
        entries,
    })
}

fn entry_better(left: &PortfolioEntry, right: &PortfolioEntry) -> bool {
    left.selection
        .exact_tasks
        .cmp(&right.selection.exact_tasks)
        .then_with(|| right.training_events.cmp(&left.training_events))
        .then_with(|| right.candidate_id.cmp(&left.candidate_id))
        .is_gt()
}

fn training_shape(curriculum: &CurriculumReport) -> TrainingShape {
    let mut shape = TrainingShape {
        delayed_recall: 0,
        overwrite: 0,
        keyed_recall: 0,
        copy: 0,
        conjunction: 0,
        distractor: 0,
    };
    for item in curriculum
        .tasks
        .iter()
        .filter(|item| item.partition == CurriculumPartition::Training)
    {
        match item.task.kind {
            TaskKind::DelayedBitRecall => shape.delayed_recall = shape.delayed_recall.saturating_add(1),
            TaskKind::OverwriteBitRecall => shape.overwrite = shape.overwrite.saturating_add(1),
            TaskKind::KeyedFactRecall => shape.keyed_recall = shape.keyed_recall.saturating_add(1),
            TaskKind::CopyAfterMarker => shape.copy = shape.copy.saturating_add(1),
            TaskKind::TwoFactConjunction => shape.conjunction = shape.conjunction.saturating_add(1),
            TaskKind::DistractorRejection => shape.distractor = shape.distractor.saturating_add(1),
        }
    }
    shape
}

fn training_events(curriculum: &CurriculumReport) -> Result<u64, PortfolioError> {
    curriculum
        .tasks
        .iter()
        .filter(|item| item.partition == CurriculumPartition::Training)
        .try_fold(0_u64, |total, item| {
            total
                .checked_add(
                    u64::try_from(item.task.events.len()).map_err(|_| PortfolioError::Overflow)?,
                )
                .ok_or(PortfolioError::Overflow)
        })
}

fn validate_id(value: &str) -> Result<(), PortfolioError> {
    if value.trim().is_empty()
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(PortfolioError::InvalidCandidateId);
    }
    Ok(())
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
pub enum PortfolioError {
    InvalidPolicy,
    CandidateBudget,
    InvalidCandidateId,
    DuplicateCandidateId,
    MissingSourceClass,
    UnmatchedTrainingShape,
    SelectionOverlap(u64),
    Allocation,
    Overflow,
    Curriculum(crate::curriculum::CurriculumError),
    Training(MultiTaskTrainingError),
    Task(TaskError),
    TaskLoss(TaskLossError),
}

impl fmt::Display for PortfolioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for PortfolioError {}
