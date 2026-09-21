//! Task-loss training for the first learnable SML task micro-operation.
//!
//! This module learns hard Boolean micro-operations from complete task examples.
//! It does not pass target truth-table identities to the optimizer. Event routing
//! and associative-memory semantics remain separate systems.

use core::fmt;

use crate::{
    boolean_integral::{
        train_boolean_gate_samples, BooleanGateTrainingConfig, BooleanGateTrainingReport,
        BooleanIntegralError,
    },
    candidate::{
        SmlTaskCandidate, TaskCandidateConfig, TaskCandidateError, TaskCandidateTrainingReport,
    },
    tasks::{suite_from_case_id, TaskAnswer, TaskError, TaskEvent, TaskInstance, TaskKind},
    train::{
        GateSample1, GateSample2, GateSampleTrainingReport, GateSampleTrainingReport1,
        GateTrainingConfig, LatentGate1, LatentGate2, TrainError, TruthTable1, TRUTH_TABLE_COUNT,
        UNARY_TRUTH_TABLE_COUNT,
    },
};

/// Configuration for task-loss training of the conjunction micro-operation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConjunctionTaskTrainingConfig {
    pub train_start_case_id: u64,
    pub train_cases: u32,
    pub gate_training: GateTrainingConfig,
    pub candidate: TaskCandidateConfig,
}

impl Default for ConjunctionTaskTrainingConfig {
    fn default() -> Self {
        Self {
            train_start_case_id: 0,
            train_cases: 64,
            gate_training: GateTrainingConfig {
                epochs: 600,
                learning_rate: 0.2,
                epsilon: 1.0e-6,
            },
            candidate: TaskCandidateConfig::default(),
        }
    }
}

/// Configuration for task-loss training of the overwrite state transition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StateUpdateTaskTrainingConfig {
    pub train_start_case_id: u64,
    pub train_cases: u32,
    pub gate_training: GateTrainingConfig,
    pub candidate: TaskCandidateConfig,
}

impl Default for StateUpdateTaskTrainingConfig {
    fn default() -> Self {
        Self {
            train_start_case_id: 0,
            train_cases: 64,
            gate_training: GateTrainingConfig {
                epochs: 600,
                learning_rate: 0.2,
                epsilon: 1.0e-6,
            },
            candidate: TaskCandidateConfig::default(),
        }
    }
}

/// Configuration for task-loss training of recall and payload-copy projections.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecallCopyTaskTrainingConfig {
    pub train_start_case_id: u64,
    pub train_cases: u32,
    pub gate_training: GateTrainingConfig,
    pub candidate: TaskCandidateConfig,
}

impl Default for RecallCopyTaskTrainingConfig {
    fn default() -> Self {
        Self {
            train_start_case_id: 0,
            train_cases: 64,
            gate_training: GateTrainingConfig {
                epochs: 600,
                learning_rate: 0.2,
                epsilon: 1.0e-6,
            },
            candidate: TaskCandidateConfig::default(),
        }
    }
}

/// Evidence retained after learning one task micro-operation from task answers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConjunctionTaskTrainingReport {
    pub task_samples: u64,
    pub input_pattern_mask: u8,
    pub learned_boolean_bits: u64,
    pub latent_training_scalars: u64,
    pub gate: GateSampleTrainingReport,
    pub candidate: TaskCandidateTrainingReport,
}

/// Evidence retained after learning the overwrite state update from task answers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StateUpdateTaskTrainingReport {
    pub task_samples: u64,
    pub input_pattern_mask: u8,
    pub learned_boolean_bits: u64,
    pub latent_training_scalars: u64,
    pub gate: GateSampleTrainingReport,
    pub candidate: TaskCandidateTrainingReport,
}

/// Evidence retained after learning unary recall and payload-copy semantics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecallCopyTaskTrainingReport {
    pub recall_samples: u64,
    pub copy_samples: u64,
    pub recall_input_pattern_mask: u8,
    pub copy_input_pattern_mask: u8,
    pub learned_boolean_bits: u64,
    pub latent_training_scalars: u64,
    pub recall_gate: GateSampleTrainingReport1,
    pub copy_gate: GateSampleTrainingReport1,
    pub candidate: TaskCandidateTrainingReport,
}

/// Configuration for gradient-free Boolean-integral task learning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BooleanIntegralTaskTrainingConfig {
    pub train_start_case_id: u64,
    pub train_cases: u32,
    pub gate_training: BooleanGateTrainingConfig,
    pub candidate: TaskCandidateConfig,
}

impl Default for BooleanIntegralTaskTrainingConfig {
    fn default() -> Self {
        Self {
            train_start_case_id: 0,
            train_cases: 64,
            gate_training: BooleanGateTrainingConfig::default(),
            candidate: TaskCandidateConfig::default(),
        }
    }
}

/// Evidence retained after integer-only Boolean-integral task learning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BooleanIntegralTaskTrainingReport {
    pub task_samples: u64,
    pub input_pattern_mask: u8,
    pub learned_boolean_bits: u64,
    /// Four signed credit bits for each of the four active truth-table parameters.
    pub active_credit_bits: u64,
    pub gate: BooleanGateTrainingReport,
    pub candidate: TaskCandidateTrainingReport,
}

/// Aggregate held-out evaluation counters for one candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TaskPanelEvaluation {
    pub tasks: u64,
    pub exact_tasks: u64,
    pub conjunction_tasks: u64,
    pub exact_conjunction_tasks: u64,
    pub overwrite_tasks: u64,
    pub exact_overwrite_tasks: u64,
    pub delayed_recall_tasks: u64,
    pub exact_delayed_recall_tasks: u64,
    pub copy_tasks: u64,
    pub exact_copy_tasks: u64,
    pub trace_records: u64,
    pub route_lookups: u64,
    pub memory_reads: u64,
    pub memory_writes: u64,
}

/// Trains the conjunction micro-operation strictly from generated task examples.
pub fn train_conjunction_from_tasks(
    config: ConjunctionTaskTrainingConfig,
) -> Result<(SmlTaskCandidate, ConjunctionTaskTrainingReport), TaskLossError> {
    if config.train_cases == 0 {
        return Err(TaskLossError::ZeroTrainingCases);
    }

    let suite = suite_from_case_id(config.train_start_case_id, config.train_cases)?;
    let mut samples = Vec::with_capacity(config.train_cases as usize);
    let mut input_pattern_mask = 0_u8;

    for instance in &suite {
        if instance.kind != TaskKind::TwoFactConjunction {
            continue;
        }
        let sample = conjunction_sample(instance)?;
        let input_index = sample.left as u8 | ((sample.right as u8) << 1);
        input_pattern_mask |= 1_u8 << input_index;
        samples.push(sample);
    }

    if input_pattern_mask != 0b1111 {
        return Err(TaskLossError::IncompleteInputCoverage(input_pattern_mask));
    }

    let mut gate = LatentGate2::default();
    let gate_report = gate.train_samples(&samples, config.gate_training)?;
    if gate_report.hard_correct != samples.len() as u64 {
        return Err(TaskLossError::GateNotConverged {
            correct: gate_report.hard_correct,
            total: samples.len() as u64,
        });
    }

    let hard_table = gate.hard_table()?;
    let (candidate, candidate_report) =
        SmlTaskCandidate::train_with_conjunction(config.candidate, hard_table)?;

    Ok((
        candidate,
        ConjunctionTaskTrainingReport {
            task_samples: samples.len() as u64,
            input_pattern_mask,
            learned_boolean_bits: 4,
            latent_training_scalars: TRUTH_TABLE_COUNT as u64,
            gate: gate_report,
            candidate: candidate_report,
        },
    ))
}

/// Trains overwrite-state semantics strictly from generated task answers.
pub fn train_state_update_from_tasks(
    config: StateUpdateTaskTrainingConfig,
) -> Result<(SmlTaskCandidate, StateUpdateTaskTrainingReport), TaskLossError> {
    if config.train_cases == 0 {
        return Err(TaskLossError::ZeroTrainingCases);
    }

    let suite = suite_from_case_id(config.train_start_case_id, config.train_cases)?;
    let mut samples = Vec::with_capacity(config.train_cases as usize);
    let mut input_pattern_mask = 0_u8;

    for instance in &suite {
        if instance.kind != TaskKind::OverwriteBitRecall {
            continue;
        }
        let sample = state_update_sample(instance)?;
        let input_index = sample.left as u8 | ((sample.right as u8) << 1);
        input_pattern_mask |= 1_u8 << input_index;
        samples.push(sample);
    }

    if input_pattern_mask != 0b1111 {
        return Err(TaskLossError::IncompleteInputCoverage(input_pattern_mask));
    }

    let mut gate = LatentGate2::default();
    let gate_report = gate.train_samples(&samples, config.gate_training)?;
    if gate_report.hard_correct != samples.len() as u64 {
        return Err(TaskLossError::GateNotConverged {
            correct: gate_report.hard_correct,
            total: samples.len() as u64,
        });
    }

    let hard_table = gate.hard_table()?;
    let (candidate, candidate_report) = SmlTaskCandidate::train_with_micro_ops(
        config.candidate,
        crate::train::TruthTable2::AND,
        hard_table,
    )?;

    Ok((
        candidate,
        StateUpdateTaskTrainingReport {
            task_samples: samples.len() as u64,
            input_pattern_mask,
            learned_boolean_bits: 4,
            latent_training_scalars: TRUTH_TABLE_COUNT as u64,
            gate: gate_report,
            candidate: candidate_report,
        },
    ))
}

/// Trains delayed-recall and payload-copy projections strictly from task answers.
///
/// Both learned operations are unary Boolean functions, so all two input rows
/// (false and true) must be observed independently for recall and copy before
/// either table can be committed.
pub fn train_recall_copy_from_tasks(
    config: RecallCopyTaskTrainingConfig,
) -> Result<(SmlTaskCandidate, RecallCopyTaskTrainingReport), TaskLossError> {
    if config.train_cases == 0 {
        return Err(TaskLossError::ZeroTrainingCases);
    }

    let suite = suite_from_case_id(config.train_start_case_id, config.train_cases)?;
    let mut recall_samples = Vec::with_capacity(config.train_cases as usize);
    let mut copy_samples = Vec::new();
    let mut recall_input_pattern_mask = 0_u8;
    let mut copy_input_pattern_mask = 0_u8;

    for instance in &suite {
        match instance.kind {
            TaskKind::DelayedBitRecall => {
                let sample = recall_sample(instance)?;
                recall_input_pattern_mask |= 1_u8 << sample.input as u8;
                recall_samples.push(sample);
            }
            TaskKind::CopyAfterMarker => {
                for sample in copy_samples_from_instance(instance)? {
                    copy_input_pattern_mask |= 1_u8 << sample.input as u8;
                    copy_samples.push(sample);
                }
            }
            _ => {}
        }
    }

    if recall_input_pattern_mask != 0b11 {
        return Err(TaskLossError::IncompleteUnaryInputCoverage(
            recall_input_pattern_mask,
        ));
    }
    if copy_input_pattern_mask != 0b11 {
        return Err(TaskLossError::IncompleteUnaryInputCoverage(
            copy_input_pattern_mask,
        ));
    }

    let mut recall_gate = LatentGate1::default();
    let recall_report = recall_gate.train_samples(&recall_samples, config.gate_training)?;
    if recall_report.hard_correct != recall_samples.len() as u64 {
        return Err(TaskLossError::GateNotConverged {
            correct: recall_report.hard_correct,
            total: recall_samples.len() as u64,
        });
    }

    let mut copy_gate = LatentGate1::default();
    let copy_report = copy_gate.train_samples(&copy_samples, config.gate_training)?;
    if copy_report.hard_correct != copy_samples.len() as u64 {
        return Err(TaskLossError::GateNotConverged {
            correct: copy_report.hard_correct,
            total: copy_samples.len() as u64,
        });
    }

    let (candidate, candidate_report) = SmlTaskCandidate::train_with_all_micro_ops(
        config.candidate,
        crate::train::TruthTable2::AND,
        crate::train::TruthTable2::RIGHT,
        recall_gate.hard_table()?,
        copy_gate.hard_table()?,
    )?;

    Ok((
        candidate,
        RecallCopyTaskTrainingReport {
            recall_samples: recall_samples.len() as u64,
            copy_samples: copy_samples.len() as u64,
            recall_input_pattern_mask,
            copy_input_pattern_mask,
            learned_boolean_bits: 4,
            latent_training_scalars: (UNARY_TRUTH_TABLE_COUNT as u64).saturating_mul(2),
            recall_gate: recall_report,
            copy_gate: copy_report,
            candidate: candidate_report,
        },
    ))
}

/// Trains conjunction semantics from task examples using Boolean-integral credit only.
pub fn train_conjunction_from_tasks_boolean_integral(
    config: BooleanIntegralTaskTrainingConfig,
) -> Result<(SmlTaskCandidate, BooleanIntegralTaskTrainingReport), TaskLossError> {
    if config.train_cases == 0 {
        return Err(TaskLossError::ZeroTrainingCases);
    }

    let suite = suite_from_case_id(config.train_start_case_id, config.train_cases)?;
    let mut samples = Vec::with_capacity(config.train_cases as usize);
    let mut input_pattern_mask = 0_u8;

    for instance in &suite {
        if instance.kind != TaskKind::TwoFactConjunction {
            continue;
        }
        let sample = conjunction_sample(instance)?;
        let input_index = sample.left as u8 | ((sample.right as u8) << 1);
        input_pattern_mask |= 1_u8 << input_index;
        samples.push(sample);
    }

    if input_pattern_mask != 0b1111 {
        return Err(TaskLossError::IncompleteInputCoverage(input_pattern_mask));
    }

    let gate_report = train_boolean_gate_samples(
        &samples,
        crate::train::TruthTable2::FALSE,
        config.gate_training,
    )?;
    if gate_report.hard_correct != samples.len() as u64 {
        return Err(TaskLossError::GateNotConverged {
            correct: gate_report.hard_correct,
            total: samples.len() as u64,
        });
    }

    let (candidate, candidate_report) =
        SmlTaskCandidate::train_with_conjunction(config.candidate, gate_report.final_table)?;

    Ok((
        candidate,
        BooleanIntegralTaskTrainingReport {
            task_samples: samples.len() as u64,
            input_pattern_mask,
            learned_boolean_bits: 4,
            active_credit_bits: 16,
            gate: gate_report,
            candidate: candidate_report,
        },
    ))
}

/// Trains overwrite-state semantics from task examples using Boolean-integral credit only.
pub fn train_state_update_from_tasks_boolean_integral(
    config: BooleanIntegralTaskTrainingConfig,
) -> Result<(SmlTaskCandidate, BooleanIntegralTaskTrainingReport), TaskLossError> {
    if config.train_cases == 0 {
        return Err(TaskLossError::ZeroTrainingCases);
    }

    let suite = suite_from_case_id(config.train_start_case_id, config.train_cases)?;
    let mut samples = Vec::with_capacity(config.train_cases as usize);
    let mut input_pattern_mask = 0_u8;

    for instance in &suite {
        if instance.kind != TaskKind::OverwriteBitRecall {
            continue;
        }
        let sample = state_update_sample(instance)?;
        let input_index = sample.left as u8 | ((sample.right as u8) << 1);
        input_pattern_mask |= 1_u8 << input_index;
        samples.push(sample);
    }

    if input_pattern_mask != 0b1111 {
        return Err(TaskLossError::IncompleteInputCoverage(input_pattern_mask));
    }

    let gate_report = train_boolean_gate_samples(
        &samples,
        crate::train::TruthTable2::FALSE,
        config.gate_training,
    )?;
    if gate_report.hard_correct != samples.len() as u64 {
        return Err(TaskLossError::GateNotConverged {
            correct: gate_report.hard_correct,
            total: samples.len() as u64,
        });
    }

    let (candidate, candidate_report) = SmlTaskCandidate::train_with_micro_ops(
        config.candidate,
        crate::train::TruthTable2::AND,
        gate_report.final_table,
    )?;

    Ok((
        candidate,
        BooleanIntegralTaskTrainingReport {
            task_samples: samples.len() as u64,
            input_pattern_mask,
            learned_boolean_bits: 4,
            active_credit_bits: 16,
            gate: gate_report,
            candidate: candidate_report,
        },
    ))
}

/// Evaluates a frozen candidate and aggregates its held-out evidence counters.
pub fn evaluate_task_panel(
    candidate: &SmlTaskCandidate,
    panel: &[TaskInstance],
) -> Result<TaskPanelEvaluation, TaskLossError> {
    let mut report = TaskPanelEvaluation::default();

    for instance in panel {
        let evidence = candidate.execute(instance)?;
        report.tasks = report.tasks.saturating_add(1);
        report.trace_records = report
            .trace_records
            .saturating_add(evidence.trace.len() as u64);
        report.route_lookups = report
            .route_lookups
            .saturating_add(evidence.counters.route_lookups);
        report.memory_reads = report
            .memory_reads
            .saturating_add(evidence.counters.memory_reads);
        report.memory_writes = report
            .memory_writes
            .saturating_add(evidence.counters.memory_writes);

        if evidence.exact {
            report.exact_tasks = report.exact_tasks.saturating_add(1);
        }
        if instance.kind == TaskKind::TwoFactConjunction {
            report.conjunction_tasks = report.conjunction_tasks.saturating_add(1);
            if evidence.exact {
                report.exact_conjunction_tasks = report.exact_conjunction_tasks.saturating_add(1);
            }
        }
        if instance.kind == TaskKind::OverwriteBitRecall {
            report.overwrite_tasks = report.overwrite_tasks.saturating_add(1);
            if evidence.exact {
                report.exact_overwrite_tasks = report.exact_overwrite_tasks.saturating_add(1);
            }
        }
        if instance.kind == TaskKind::DelayedBitRecall {
            report.delayed_recall_tasks = report.delayed_recall_tasks.saturating_add(1);
            if evidence.exact {
                report.exact_delayed_recall_tasks =
                    report.exact_delayed_recall_tasks.saturating_add(1);
            }
        }
        if instance.kind == TaskKind::CopyAfterMarker {
            report.copy_tasks = report.copy_tasks.saturating_add(1);
            if evidence.exact {
                report.exact_copy_tasks = report.exact_copy_tasks.saturating_add(1);
            }
        }
    }

    Ok(report)
}

fn recall_sample(instance: &TaskInstance) -> Result<GateSample1, TaskLossError> {
    if instance.kind != TaskKind::DelayedBitRecall {
        return Err(TaskLossError::WrongTaskKind(instance.kind));
    }

    let input = instance
        .events
        .iter()
        .find_map(|event| match event {
            TaskEvent::StoreBit(value) => Some(*value),
            _ => None,
        })
        .ok_or(TaskLossError::MissingStoredBit)?;
    let target = match &instance.expected {
        TaskAnswer::Bit(value) => *value,
        TaskAnswer::Bits(_) => return Err(TaskLossError::NonScalarTarget),
    };

    Ok(GateSample1 { input, target })
}

fn copy_samples_from_instance(instance: &TaskInstance) -> Result<Vec<GateSample1>, TaskLossError> {
    if instance.kind != TaskKind::CopyAfterMarker {
        return Err(TaskLossError::WrongTaskKind(instance.kind));
    }

    let inputs: Vec<bool> = instance
        .events
        .iter()
        .filter_map(|event| match event {
            TaskEvent::PayloadBit(value) => Some(*value),
            _ => None,
        })
        .collect();
    let targets = match &instance.expected {
        TaskAnswer::Bits(values) => values,
        TaskAnswer::Bit(_) => return Err(TaskLossError::NonVectorTarget),
    };

    if inputs.len() != targets.len() {
        return Err(TaskLossError::CopyLengthMismatch {
            inputs: inputs.len(),
            targets: targets.len(),
        });
    }
    if inputs.is_empty() {
        return Err(TaskLossError::MissingPayload);
    }

    Ok(inputs
        .into_iter()
        .zip(targets.iter().copied())
        .map(|(input, target)| GateSample1 { input, target })
        .collect())
}

fn conjunction_sample(instance: &TaskInstance) -> Result<GateSample2, TaskLossError> {
    if instance.kind != TaskKind::TwoFactConjunction {
        return Err(TaskLossError::WrongTaskKind(instance.kind));
    }

    let mut query = None;
    for (index, event) in instance.events.iter().copied().enumerate() {
        if let TaskEvent::QueryConjunction { left, right } = event {
            query = Some((left, right, index));
            break;
        }
    }
    let (left_key, right_key, query_index) = query.ok_or(TaskLossError::MissingQuery)?;
    let left = fact_before(instance, left_key, query_index)?;
    let right = fact_before(instance, right_key, query_index)?;
    let target = match &instance.expected {
        TaskAnswer::Bit(value) => *value,
        TaskAnswer::Bits(_) => return Err(TaskLossError::NonScalarTarget),
    };

    Ok(GateSample2 {
        left,
        right,
        target,
    })
}

fn state_update_sample(instance: &TaskInstance) -> Result<GateSample2, TaskLossError> {
    if instance.kind != TaskKind::OverwriteBitRecall {
        return Err(TaskLossError::WrongTaskKind(instance.kind));
    }

    let stores: Vec<bool> = instance
        .events
        .iter()
        .filter_map(|event| match event {
            TaskEvent::StoreBit(value) => Some(*value),
            _ => None,
        })
        .collect();
    if stores.len() != 2 {
        return Err(TaskLossError::MissingStateUpdatePair);
    }
    let target = match &instance.expected {
        TaskAnswer::Bit(value) => *value,
        TaskAnswer::Bits(_) => return Err(TaskLossError::NonScalarTarget),
    };

    Ok(GateSample2 {
        left: stores[0],
        right: stores[1],
        target,
    })
}

fn fact_before(instance: &TaskInstance, key: u16, before: usize) -> Result<bool, TaskLossError> {
    instance.events[..before]
        .iter()
        .rev()
        .find_map(|event| match event {
            TaskEvent::Fact {
                key: resident_key,
                value,
            } if *resident_key == key => Some(*value),
            _ => None,
        })
        .ok_or(TaskLossError::MissingFact(key))
}

/// Task-data extraction, optimization, candidate construction, or evaluation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLossError {
    ZeroTrainingCases,
    IncompleteInputCoverage(u8),
    IncompleteUnaryInputCoverage(u8),
    GateNotConverged { correct: u64, total: u64 },
    WrongTaskKind(TaskKind),
    MissingQuery,
    MissingStateUpdatePair,
    MissingStoredBit,
    MissingPayload,
    MissingFact(u16),
    CopyLengthMismatch { inputs: usize, targets: usize },
    NonScalarTarget,
    NonVectorTarget,
    Task(TaskError),
    Train(TrainError),
    BooleanIntegral(BooleanIntegralError),
    Candidate(TaskCandidateError),
}

impl fmt::Display for TaskLossError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for TaskLossError {}

impl From<TaskError> for TaskLossError {
    fn from(value: TaskError) -> Self {
        Self::Task(value)
    }
}

impl From<TrainError> for TaskLossError {
    fn from(value: TrainError) -> Self {
        Self::Train(value)
    }
}

impl From<BooleanIntegralError> for TaskLossError {
    fn from(value: BooleanIntegralError) -> Self {
        Self::BooleanIntegral(value)
    }
}

impl From<TaskCandidateError> for TaskLossError {
    fn from(value: TaskCandidateError) -> Self {
        Self::Candidate(value)
    }
}
