//! Controlled held-out ablations for the integrated SML-1 task candidate.
//!
//! Ablations retain the same task panel and execution surface. Execution failures
//! are counted rather than discarded so capacity-induced collisions remain part
//! of the evidence.

use crate::{
    candidate::{AssociativeMemoryMode, SmlTaskCandidate, TaskCandidateConfig, TaskCandidateError},
    tasks::TaskInstance,
};

/// Aggregate evidence for one candidate configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TaskAblationEvaluation {
    pub tasks: u64,
    pub successful_executions: u64,
    pub exact_tasks: u64,
    pub execution_failures: u64,
    pub memory_collisions: u64,
    pub route_lookups: u64,
    pub memory_reads: u64,
    pub memory_writes: u64,
    pub learned_route_bits: u64,
}

/// Side-by-side SML-1 routing/memory ablation evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sml1AblationReport {
    pub learned_routing: TaskAblationEvaluation,
    pub fixed_routing: TaskAblationEvaluation,
    pub no_associative_memory: TaskAblationEvaluation,
    pub constrained_memory: TaskAblationEvaluation,
}

/// Runs learned-route, fixed-route, no-memory and constrained-memory candidates
/// against the exact same held-out panel.
pub fn run_sml1_ablations(
    panel: &[TaskInstance],
    constrained_memory_capacity: usize,
) -> Result<Sml1AblationReport, TaskCandidateError> {
    let default = TaskCandidateConfig::default();
    let (learned, _) = SmlTaskCandidate::train(default)?;
    let fixed = SmlTaskCandidate::fixed_route(default)?;

    let no_memory_config = TaskCandidateConfig {
        memory_capacity: 0,
        memory_mode: AssociativeMemoryMode::DisabledZeroRead,
        ..default
    };
    let (no_memory, _) = SmlTaskCandidate::train(no_memory_config)?;

    let constrained_config = TaskCandidateConfig {
        memory_capacity: constrained_memory_capacity,
        memory_mode: AssociativeMemoryMode::Direct,
        ..default
    };
    let (constrained, _) = SmlTaskCandidate::train(constrained_config)?;

    Ok(Sml1AblationReport {
        learned_routing: evaluate_ablation(&learned, panel),
        fixed_routing: evaluate_ablation(&fixed, panel),
        no_associative_memory: evaluate_ablation(&no_memory, panel),
        constrained_memory: evaluate_ablation(&constrained, panel),
    })
}

/// Evaluates a candidate while retaining fail-closed execution failures as evidence.
#[must_use]
pub fn evaluate_ablation(
    candidate: &SmlTaskCandidate,
    panel: &[TaskInstance],
) -> TaskAblationEvaluation {
    let mut report = TaskAblationEvaluation {
        tasks: panel.len() as u64,
        learned_route_bits: candidate.route_accounting().learned_boolean_bits,
        ..TaskAblationEvaluation::default()
    };

    for instance in panel {
        match candidate.execute(instance) {
            Ok(evidence) => {
                report.successful_executions = report.successful_executions.saturating_add(1);
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
            }
            Err(TaskCandidateError::MemoryCollision { .. }) => {
                report.execution_failures = report.execution_failures.saturating_add(1);
                report.memory_collisions = report.memory_collisions.saturating_add(1);
            }
            Err(_) => {
                report.execution_failures = report.execution_failures.saturating_add(1);
            }
        }
    }

    report
}
