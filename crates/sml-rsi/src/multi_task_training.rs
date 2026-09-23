use core::fmt;

use sml_core::{
    candidate::{SmlTaskCandidate, TaskCandidateConfig, TaskCandidateError},
    task_loss::{evaluate_task_panel, TaskLossError, TaskPanelEvaluation},
    tasks::{TaskAnswer, TaskEvent, TaskInstance, TaskKind},
    train::{
        GateSample1, GateSample2, GateSampleTrainingReport, GateSampleTrainingReport1,
        GateTrainingConfig, LatentGate1, LatentGate2, TrainError, TruthTable1, TruthTable2,
    },
};

use crate::curriculum::{CurriculumError, CurriculumPartition, CurriculumReport};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MultiTaskTrainingPolicy {
    pub gate_training: GateTrainingConfig,
    pub candidate: TaskCandidateConfig,
    pub min_task_instances_per_operation: usize,
}

impl Default for MultiTaskTrainingPolicy {
    fn default() -> Self {
        Self {
            gate_training: GateTrainingConfig {
                epochs: 600,
                learning_rate: 0.2,
                epsilon: 1.0e-6,
            },
            candidate: TaskCandidateConfig::default(),
            min_task_instances_per_operation: 16,
        }
    }
}

impl MultiTaskTrainingPolicy {
    pub fn validate(self) -> Result<Self, MultiTaskTrainingError> {
        self.gate_training
            .validate()
            .map_err(MultiTaskTrainingError::Train)?;
        if self.min_task_instances_per_operation < 4 {
            return Err(MultiTaskTrainingError::InvalidPolicy);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultiTaskCoverage {
    pub conjunction: u8,
    pub overwrite: u8,
    pub recall: u8,
    pub copy: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MultiTaskSampleCounts {
    pub conjunction_tasks: u64,
    pub overwrite_tasks: u64,
    pub recall_tasks: u64,
    pub copy_tasks: u64,
    pub copy_bit_samples: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiTaskTrainingReport {
    pub candidate: SmlTaskCandidate,
    pub conjunction_table: TruthTable2,
    pub overwrite_table: TruthTable2,
    pub recall_table: TruthTable1,
    pub copy_table: TruthTable1,
    pub conjunction_gate: GateSampleTrainingReport,
    pub overwrite_gate: GateSampleTrainingReport,
    pub recall_gate: GateSampleTrainingReport1,
    pub copy_gate: GateSampleTrainingReport1,
    pub coverage: MultiTaskCoverage,
    pub samples: MultiTaskSampleCounts,
    pub learned_boolean_bits: u64,
    pub latent_training_scalars: u64,
    pub training_evaluation: TaskPanelEvaluation,
    pub development_evaluation: TaskPanelEvaluation,
}

pub fn train_multitask_candidate(
    curriculum: &CurriculumReport,
    policy: MultiTaskTrainingPolicy,
) -> Result<MultiTaskTrainingReport, MultiTaskTrainingError> {
    let policy = policy.validate()?;
    curriculum
        .validate()
        .map_err(MultiTaskTrainingError::Curriculum)?;

    let training = curriculum
        .tasks
        .iter()
        .filter(|item| item.partition == CurriculumPartition::Training)
        .map(|item| item.task.clone())
        .collect::<Vec<_>>();
    let development = curriculum
        .tasks
        .iter()
        .filter(|item| item.partition == CurriculumPartition::Development)
        .map(|item| item.task.clone())
        .collect::<Vec<_>>();

    let extracted = extract_samples(&training, policy.min_task_instances_per_operation)?;

    let mut conjunction_gate = LatentGate2::default();
    let conjunction_report = conjunction_gate
        .train_samples(&extracted.conjunction, policy.gate_training)
        .map_err(MultiTaskTrainingError::Train)?;
    require_converged2(&conjunction_report, Operation::Conjunction)?;

    let mut overwrite_gate = LatentGate2::default();
    let overwrite_report = overwrite_gate
        .train_samples(&extracted.overwrite, policy.gate_training)
        .map_err(MultiTaskTrainingError::Train)?;
    require_converged2(&overwrite_report, Operation::Overwrite)?;

    let mut recall_gate = LatentGate1::default();
    let recall_report = recall_gate
        .train_samples(&extracted.recall, policy.gate_training)
        .map_err(MultiTaskTrainingError::Train)?;
    require_converged1(&recall_report, Operation::Recall)?;

    let mut copy_gate = LatentGate1::default();
    let copy_report = copy_gate
        .train_samples(&extracted.copy, policy.gate_training)
        .map_err(MultiTaskTrainingError::Train)?;
    require_converged1(&copy_report, Operation::Copy)?;

    let conjunction_table = conjunction_gate
        .hard_table()
        .map_err(MultiTaskTrainingError::Train)?;
    let overwrite_table = overwrite_gate
        .hard_table()
        .map_err(MultiTaskTrainingError::Train)?;
    let recall_table = recall_gate
        .hard_table()
        .map_err(MultiTaskTrainingError::Train)?;
    let copy_table = copy_gate
        .hard_table()
        .map_err(MultiTaskTrainingError::Train)?;

    let (candidate, _) = SmlTaskCandidate::train_with_all_micro_ops(
        policy.candidate,
        conjunction_table,
        overwrite_table,
        recall_table,
        copy_table,
    )
    .map_err(MultiTaskTrainingError::Candidate)?;

    let training_evaluation =
        evaluate_task_panel(&candidate, &training).map_err(MultiTaskTrainingError::TaskLoss)?;
    let development_evaluation =
        evaluate_task_panel(&candidate, &development).map_err(MultiTaskTrainingError::TaskLoss)?;

    Ok(MultiTaskTrainingReport {
        candidate,
        conjunction_table,
        overwrite_table,
        recall_table,
        copy_table,
        conjunction_gate: conjunction_report,
        overwrite_gate: overwrite_report,
        recall_gate: recall_report,
        copy_gate: copy_report,
        coverage: extracted.coverage,
        samples: extracted.counts,
        learned_boolean_bits: 12,
        latent_training_scalars: 40,
        training_evaluation,
        development_evaluation,
    })
}

#[derive(Debug)]
struct ExtractedSamples {
    conjunction: Vec<GateSample2>,
    overwrite: Vec<GateSample2>,
    recall: Vec<GateSample1>,
    copy: Vec<GateSample1>,
    coverage: MultiTaskCoverage,
    counts: MultiTaskSampleCounts,
}

fn extract_samples(
    tasks: &[TaskInstance],
    minimum: usize,
) -> Result<ExtractedSamples, MultiTaskTrainingError> {
    let mut conjunction = Vec::new();
    let mut overwrite = Vec::new();
    let mut recall = Vec::new();
    let mut copy = Vec::new();
    let mut coverage = MultiTaskCoverage {
        conjunction: 0,
        overwrite: 0,
        recall: 0,
        copy: 0,
    };
    let mut counts = MultiTaskSampleCounts {
        conjunction_tasks: 0,
        overwrite_tasks: 0,
        recall_tasks: 0,
        copy_tasks: 0,
        copy_bit_samples: 0,
    };

    for task in tasks {
        match task.kind {
            TaskKind::TwoFactConjunction => {
                let sample = conjunction_sample(task)?;
                coverage.conjunction |=
                    1_u8 << (u8::from(sample.left) | (u8::from(sample.right) << 1));
                conjunction.push(sample);
                counts.conjunction_tasks = counts.conjunction_tasks.saturating_add(1);
            }
            TaskKind::OverwriteBitRecall => {
                let sample = overwrite_sample(task)?;
                coverage.overwrite |=
                    1_u8 << (u8::from(sample.left) | (u8::from(sample.right) << 1));
                overwrite.push(sample);
                counts.overwrite_tasks = counts.overwrite_tasks.saturating_add(1);
            }
            TaskKind::DelayedBitRecall => {
                let sample = recall_sample(task)?;
                coverage.recall |= 1_u8 << u8::from(sample.input);
                recall.push(sample);
                counts.recall_tasks = counts.recall_tasks.saturating_add(1);
            }
            TaskKind::CopyAfterMarker => {
                let samples = copy_samples(task)?;
                for sample in samples {
                    coverage.copy |= 1_u8 << u8::from(sample.input);
                    copy.push(sample);
                }
                counts.copy_tasks = counts.copy_tasks.saturating_add(1);
            }
            TaskKind::KeyedFactRecall | TaskKind::DistractorRejection => {}
        }
    }
    counts.copy_bit_samples =
        u64::try_from(copy.len()).map_err(|_| MultiTaskTrainingError::Overflow)?;

    let copy_tasks =
        usize::try_from(counts.copy_tasks).map_err(|_| MultiTaskTrainingError::Overflow)?;
    for (operation, actual) in [
        (Operation::Conjunction, conjunction.len()),
        (Operation::Overwrite, overwrite.len()),
        (Operation::Recall, recall.len()),
        (Operation::Copy, copy_tasks),
    ] {
        if actual < minimum {
            return Err(MultiTaskTrainingError::InsufficientSamples {
                operation,
                actual,
                minimum,
            });
        }
    }

    if coverage.conjunction != 0b1111 {
        return Err(MultiTaskTrainingError::IncompleteCoverage {
            operation: Operation::Conjunction,
            mask: coverage.conjunction,
        });
    }
    if coverage.overwrite != 0b1111 {
        return Err(MultiTaskTrainingError::IncompleteCoverage {
            operation: Operation::Overwrite,
            mask: coverage.overwrite,
        });
    }
    if coverage.recall != 0b11 {
        return Err(MultiTaskTrainingError::IncompleteCoverage {
            operation: Operation::Recall,
            mask: coverage.recall,
        });
    }
    if coverage.copy != 0b11 {
        return Err(MultiTaskTrainingError::IncompleteCoverage {
            operation: Operation::Copy,
            mask: coverage.copy,
        });
    }

    Ok(ExtractedSamples {
        conjunction,
        overwrite,
        recall,
        copy,
        coverage,
        counts,
    })
}

fn conjunction_sample(task: &TaskInstance) -> Result<GateSample2, MultiTaskTrainingError> {
    let (left_key, right_key, query_index) = task
        .events
        .iter()
        .enumerate()
        .find_map(|(index, event)| match event {
            TaskEvent::QueryConjunction { left, right } => Some((*left, *right, index)),
            _ => None,
        })
        .ok_or(MultiTaskTrainingError::MalformedTask(task.case_id))?;
    let left = fact_before(task, left_key, query_index)?;
    let right = fact_before(task, right_key, query_index)?;
    let target = scalar_target(task)?;
    Ok(GateSample2 {
        left,
        right,
        target,
    })
}

fn overwrite_sample(task: &TaskInstance) -> Result<GateSample2, MultiTaskTrainingError> {
    let stores = task
        .events
        .iter()
        .filter_map(|event| match event {
            TaskEvent::StoreBit(value) => Some(*value),
            _ => None,
        })
        .collect::<Vec<_>>();
    if stores.len() != 2 {
        return Err(MultiTaskTrainingError::MalformedTask(task.case_id));
    }
    Ok(GateSample2 {
        left: stores[0],
        right: stores[1],
        target: scalar_target(task)?,
    })
}

fn recall_sample(task: &TaskInstance) -> Result<GateSample1, MultiTaskTrainingError> {
    let input = task
        .events
        .iter()
        .find_map(|event| match event {
            TaskEvent::StoreBit(value) => Some(*value),
            _ => None,
        })
        .ok_or(MultiTaskTrainingError::MalformedTask(task.case_id))?;
    Ok(GateSample1 {
        input,
        target: scalar_target(task)?,
    })
}

fn copy_samples(task: &TaskInstance) -> Result<Vec<GateSample1>, MultiTaskTrainingError> {
    let inputs = task
        .events
        .iter()
        .filter_map(|event| match event {
            TaskEvent::PayloadBit(value) => Some(*value),
            _ => None,
        })
        .collect::<Vec<_>>();
    let targets = match &task.expected {
        TaskAnswer::Bits(values) => values,
        TaskAnswer::Bit(_) => return Err(MultiTaskTrainingError::MalformedTask(task.case_id)),
    };
    if inputs.is_empty() || inputs.len() != targets.len() {
        return Err(MultiTaskTrainingError::MalformedTask(task.case_id));
    }
    Ok(inputs
        .into_iter()
        .zip(targets.iter().copied())
        .map(|(input, target)| GateSample1 { input, target })
        .collect())
}

fn scalar_target(task: &TaskInstance) -> Result<bool, MultiTaskTrainingError> {
    match &task.expected {
        TaskAnswer::Bit(value) => Ok(*value),
        TaskAnswer::Bits(_) => Err(MultiTaskTrainingError::MalformedTask(task.case_id)),
    }
}

fn fact_before(
    task: &TaskInstance,
    key: u16,
    before: usize,
) -> Result<bool, MultiTaskTrainingError> {
    task.events[..before]
        .iter()
        .rev()
        .find_map(|event| match event {
            TaskEvent::Fact {
                key: resident_key,
                value,
            } if *resident_key == key => Some(*value),
            _ => None,
        })
        .ok_or(MultiTaskTrainingError::MalformedTask(task.case_id))
}

fn require_converged2(
    report: &GateSampleTrainingReport,
    operation: Operation,
) -> Result<(), MultiTaskTrainingError> {
    if report.hard_correct != report.sample_count {
        return Err(MultiTaskTrainingError::GateNotConverged {
            operation,
            correct: report.hard_correct,
            total: report.sample_count,
        });
    }
    Ok(())
}

fn require_converged1(
    report: &GateSampleTrainingReport1,
    operation: Operation,
) -> Result<(), MultiTaskTrainingError> {
    if report.hard_correct != report.sample_count {
        return Err(MultiTaskTrainingError::GateNotConverged {
            operation,
            correct: report.hard_correct,
            total: report.sample_count,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Conjunction,
    Overwrite,
    Recall,
    Copy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MultiTaskTrainingError {
    InvalidPolicy,
    InsufficientSamples {
        operation: Operation,
        actual: usize,
        minimum: usize,
    },
    IncompleteCoverage {
        operation: Operation,
        mask: u8,
    },
    GateNotConverged {
        operation: Operation,
        correct: u64,
        total: u64,
    },
    MalformedTask(u64),
    Overflow,
    Curriculum(CurriculumError),
    Train(TrainError),
    Candidate(TaskCandidateError),
    TaskLoss(TaskLossError),
}

impl fmt::Display for MultiTaskTrainingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MultiTaskTrainingError {}
