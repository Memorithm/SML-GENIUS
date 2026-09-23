use core::fmt;
use std::collections::BTreeSet;

use sml_core::{
    candidate::{SmlTaskCandidate, TaskCandidateConfig, TaskCandidateError},
    task_loss::{evaluate_task_panel, TaskLossError, TaskPanelEvaluation},
    tasks::{two_fact_conjunction, TaskAnswer, TaskError, TaskEvent, TaskInstance, TaskKind},
    train::{GateSample2, GateSampleTrainingReport, GateTrainingConfig, LatentGate2, TrainError, TruthTable2},
};

use crate::curriculum::{CurriculumPartition, CurriculumReport};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchedTrainingPolicy {
    pub gate_training: GateTrainingConfig,
    pub candidate: TaskCandidateConfig,
    pub fixed_start_case_id: u64,
    pub fixed_difficulty: u16,
    pub holdout_start_case_id: u64,
    pub holdout_cases: u32,
    pub holdout_difficulty: u16,
    pub min_training_tasks: usize,
}

impl Default for MatchedTrainingPolicy {
    fn default() -> Self {
        Self {
            gate_training: GateTrainingConfig {
                epochs: 600,
                learning_rate: 0.2,
                epsilon: 1.0e-6,
            },
            candidate: TaskCandidateConfig::default(),
            fixed_start_case_id: 2_000_000,
            fixed_difficulty: 7,
            holdout_start_case_id: 3_000_000,
            holdout_cases: 64,
            holdout_difficulty: 11,
            min_training_tasks: 16,
        }
    }
}

impl MatchedTrainingPolicy {
    pub fn validate(self) -> Result<Self, MatchedTrainingError> {
        self.gate_training
            .validate()
            .map_err(MatchedTrainingError::Train)?;
        if self.fixed_difficulty == 0
            || self.fixed_difficulty > 16_383
            || self.holdout_difficulty == 0
            || self.holdout_difficulty > 16_383
            || self.holdout_cases == 0
            || self.min_training_tasks < 4
        {
            return Err(MatchedTrainingError::InvalidPolicy);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedTrainingOutcome {
    ProposedBetter,
    Tie,
    FixedBetter,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrainedCurriculumCandidate {
    pub candidate: SmlTaskCandidate,
    pub table: TruthTable2,
    pub gate: GateSampleTrainingReport,
    pub input_coverage: u8,
    pub training_tasks: u64,
    pub development: TaskPanelEvaluation,
    pub holdout: TaskPanelEvaluation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchedTrainingReport {
    pub proposed: TrainedCurriculumCandidate,
    pub fixed: TrainedCurriculumCandidate,
    pub outcome: MatchedTrainingOutcome,
    pub matched_training_tasks: u64,
    pub holdout_tasks: u64,
    pub exact_holdout_delta: i64,
}

pub fn run_matched_conjunction_training(
    curriculum: &CurriculumReport,
    policy: MatchedTrainingPolicy,
) -> Result<MatchedTrainingReport, MatchedTrainingError> {
    let policy = policy.validate()?;
    validate_curriculum_report(curriculum)?;

    let proposed_training = curriculum
        .tasks
        .iter()
        .filter(|item| {
            item.partition == CurriculumPartition::Training
                && item.task.kind == TaskKind::TwoFactConjunction
        })
        .map(|item| item.task.clone())
        .collect::<Vec<_>>();
    let proposed_development = curriculum
        .tasks
        .iter()
        .filter(|item| {
            item.partition == CurriculumPartition::Development
                && item.task.kind == TaskKind::TwoFactConjunction
        })
        .map(|item| item.task.clone())
        .collect::<Vec<_>>();

    if proposed_training.len() < policy.min_training_tasks {
        return Err(MatchedTrainingError::InsufficientProposedTraining {
            actual: proposed_training.len(),
            minimum: policy.min_training_tasks,
        });
    }

    let matched_count =
        u32::try_from(proposed_training.len()).map_err(|_| MatchedTrainingError::Overflow)?;
    let fixed_training = build_conjunction_range(
        policy.fixed_start_case_id,
        matched_count,
        policy.fixed_difficulty,
    )?;
    let holdout = build_conjunction_range(
        policy.holdout_start_case_id,
        policy.holdout_cases,
        policy.holdout_difficulty,
    )?;

    enforce_disjoint_case_ids(
        &proposed_training,
        &proposed_development,
        &fixed_training,
        &holdout,
    )?;

    let proposed = train_candidate(
        &proposed_training,
        &proposed_development,
        &holdout,
        policy.gate_training,
        policy.candidate,
    )?;
    let fixed = train_candidate(
        &fixed_training,
        &[],
        &holdout,
        policy.gate_training,
        policy.candidate,
    )?;

    let proposed_exact = proposed.holdout.exact_conjunction_tasks;
    let fixed_exact = fixed.holdout.exact_conjunction_tasks;
    let exact_holdout_delta = i64::try_from(proposed_exact)
        .and_then(|left| i64::try_from(fixed_exact).map(|right| left - right))
        .map_err(|_| MatchedTrainingError::Overflow)?;

    let outcome = match proposed_exact.cmp(&fixed_exact) {
        core::cmp::Ordering::Greater => MatchedTrainingOutcome::ProposedBetter,
        core::cmp::Ordering::Equal => MatchedTrainingOutcome::Tie,
        core::cmp::Ordering::Less => MatchedTrainingOutcome::FixedBetter,
    };

    Ok(MatchedTrainingReport {
        proposed,
        fixed,
        outcome,
        matched_training_tasks: u64::from(matched_count),
        holdout_tasks: u64::from(policy.holdout_cases),
        exact_holdout_delta,
    })
}

fn train_candidate(
    training: &[TaskInstance],
    development: &[TaskInstance],
    holdout: &[TaskInstance],
    gate_training: GateTrainingConfig,
    candidate_config: TaskCandidateConfig,
) -> Result<TrainedCurriculumCandidate, MatchedTrainingError> {
    let (samples, input_coverage) = conjunction_samples(training)?;
    if input_coverage != 0b1111 {
        return Err(MatchedTrainingError::IncompleteInputCoverage(input_coverage));
    }

    let mut gate = LatentGate2::default();
    let gate_report = gate
        .train_samples(&samples, gate_training)
        .map_err(MatchedTrainingError::Train)?;
    if gate_report.hard_correct != gate_report.sample_count {
        return Err(MatchedTrainingError::GateNotConverged {
            correct: gate_report.hard_correct,
            total: gate_report.sample_count,
        });
    }
    let table = gate.hard_table().map_err(MatchedTrainingError::Train)?;
    let (candidate, _) = SmlTaskCandidate::train_with_conjunction(candidate_config, table)
        .map_err(MatchedTrainingError::Candidate)?;
    let development =
        evaluate_task_panel(&candidate, development).map_err(MatchedTrainingError::TaskLoss)?;
    let holdout = evaluate_task_panel(&candidate, holdout).map_err(MatchedTrainingError::TaskLoss)?;

    Ok(TrainedCurriculumCandidate {
        candidate,
        table,
        gate: gate_report,
        input_coverage,
        training_tasks: u64::try_from(training.len()).map_err(|_| MatchedTrainingError::Overflow)?,
        development,
        holdout,
    })
}

fn conjunction_samples(
    tasks: &[TaskInstance],
) -> Result<(Vec<GateSample2>, u8), MatchedTrainingError> {
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(tasks.len())
        .map_err(|_| MatchedTrainingError::Allocation)?;
    let mut coverage = 0_u8;

    for task in tasks {
        if task.kind != TaskKind::TwoFactConjunction {
            return Err(MatchedTrainingError::WrongTaskKind(task.kind));
        }
        let query_index = task
            .events
            .iter()
            .position(|event| matches!(event, TaskEvent::QueryConjunction { .. }))
            .ok_or(MatchedTrainingError::MalformedTask(task.case_id))?;
        let (left_key, right_key) = match task.events[query_index] {
            TaskEvent::QueryConjunction { left, right } => (left, right),
            _ => return Err(MatchedTrainingError::MalformedTask(task.case_id)),
        };
        let left = last_fact(&task.events[..query_index], left_key)
            .ok_or(MatchedTrainingError::MalformedTask(task.case_id))?;
        let right = last_fact(&task.events[..query_index], right_key)
            .ok_or(MatchedTrainingError::MalformedTask(task.case_id))?;
        let target = match task.expected {
            TaskAnswer::Bit(value) => value,
            TaskAnswer::Bits(_) => return Err(MatchedTrainingError::MalformedTask(task.case_id)),
        };

        if task.solve().map_err(MatchedTrainingError::Task)? != task.expected {
            return Err(MatchedTrainingError::OracleMismatch(task.case_id));
        }

        coverage |= 1_u8 << (u8::from(left) | (u8::from(right) << 1));
        samples.push(GateSample2 {
            left,
            right,
            target,
        });
    }

    Ok((samples, coverage))
}

fn last_fact(events: &[TaskEvent], key: u16) -> Option<bool> {
    events.iter().rev().find_map(|event| match event {
        TaskEvent::Fact {
            key: resident_key,
            value,
        } if *resident_key == key => Some(*value),
        _ => None,
    })
}

fn build_conjunction_range(
    start_case_id: u64,
    count: u32,
    difficulty: u16,
) -> Result<Vec<TaskInstance>, MatchedTrainingError> {
    let end = start_case_id
        .checked_add(u64::from(count))
        .ok_or(MatchedTrainingError::Overflow)?;
    let mut tasks = Vec::new();
    tasks
        .try_reserve_exact(usize::try_from(count).map_err(|_| MatchedTrainingError::Overflow)?)
        .map_err(|_| MatchedTrainingError::Allocation)?;
    for case_id in start_case_id..end {
        tasks.push(
            two_fact_conjunction(case_id, usize::from(difficulty))
                .map_err(MatchedTrainingError::Task)?,
        );
    }
    Ok(tasks)
}

fn validate_curriculum_report(curriculum: &CurriculumReport) -> Result<(), MatchedTrainingError> {
    let expected = curriculum
        .training_tasks
        .checked_add(curriculum.development_tasks)
        .ok_or(MatchedTrainingError::Overflow)?;
    if curriculum.verified_tasks != expected
        || curriculum.verified_tasks
            != u64::try_from(curriculum.tasks.len()).map_err(|_| MatchedTrainingError::Overflow)?
    {
        return Err(MatchedTrainingError::InvalidCurriculumReport);
    }
    let mut events = 0_u64;
    for item in &curriculum.tasks {
        if !item
            .task
            .oracle_matches()
            .map_err(MatchedTrainingError::Task)?
        {
            return Err(MatchedTrainingError::OracleMismatch(item.task.case_id));
        }
        events = events
            .checked_add(
                u64::try_from(item.task.events.len()).map_err(|_| MatchedTrainingError::Overflow)?,
            )
            .ok_or(MatchedTrainingError::Overflow)?;
    }
    if events != curriculum.total_events {
        return Err(MatchedTrainingError::InvalidCurriculumReport);
    }
    Ok(())
}

fn enforce_disjoint_case_ids(
    proposed_training: &[TaskInstance],
    proposed_development: &[TaskInstance],
    fixed: &[TaskInstance],
    holdout: &[TaskInstance],
) -> Result<(), MatchedTrainingError> {
    let mut seen = BTreeSet::new();
    for task in proposed_training
        .iter()
        .chain(proposed_development)
        .chain(fixed)
        .chain(holdout)
    {
        if !seen.insert(task.case_id) {
            return Err(MatchedTrainingError::OverlappingCaseId(task.case_id));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchedTrainingError {
    InvalidPolicy,
    InvalidCurriculumReport,
    InsufficientProposedTraining { actual: usize, minimum: usize },
    WrongTaskKind(TaskKind),
    MalformedTask(u64),
    OracleMismatch(u64),
    IncompleteInputCoverage(u8),
    GateNotConverged { correct: u64, total: u64 },
    OverlappingCaseId(u64),
    Overflow,
    Allocation,
    Train(TrainError),
    Task(TaskError),
    Candidate(TaskCandidateError),
    TaskLoss(TaskLossError),
}

impl fmt::Display for MatchedTrainingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MatchedTrainingError {}
