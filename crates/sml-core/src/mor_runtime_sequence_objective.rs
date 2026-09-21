//! Sequence objective over the fully composed native SML MOR runtime.
//!
//! Unlike the raw sequence objective, this evaluator runs the actual native
//! runtime state machine: affinity, specialization validation, load balancing,
//! recurrent history and temporal denoising. Regret is measured on the route
//! committed by temporal denoising, while observed pre-denoise decisions remain
//! available as diagnostic evidence.

use core::fmt;

use crate::{
    mor::ExpertRuntimeHint,
    mor_runtime::{NativeMorRuntime, NativeMorRuntimeError},
    mor_sequence_objective::{
        evaluate_sequence_from_chosen_pages, MorRouteSequenceError, MorRouteSequenceEvidence,
        MorRouteSequenceExample, MorRouteSequencePolicy, MorRouteSequenceStep,
    },
    mor_temporal::MorTemporalTransition,
    BooleanWorkspace, PageId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorRuntimeSequenceStep {
    pub state: BooleanWorkspace,
    pub runtime_hints: Vec<ExpertRuntimeHint>,
    pub outcomes: Vec<crate::mor_route_objective::MorExpertOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorRuntimeSequenceExample {
    pub steps: Vec<NativeMorRuntimeSequenceStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMorRuntimeStepEvidence {
    pub step_index: usize,
    pub observed_primary: PageId,
    pub committed_primary: PageId,
    pub temporal_transition: MorTemporalTransition,
    pub temporal_held: bool,
    pub active_routers: usize,
    pub selected_experts: usize,
    pub noise_ppm: u32,
    pub structural_disagreement_ppm: u32,
    pub precision_escalation_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorRuntimeSequenceEvidence {
    pub sequence_index: usize,
    pub objective: MorRouteSequenceEvidence,
    pub runtime_steps: Vec<NativeMorRuntimeStepEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorRuntimeSequenceObjectiveReport {
    pub sequences: usize,
    pub total_steps: usize,
    pub total_chosen_cost_ppm: u64,
    pub total_oracle_cost_ppm: u64,
    pub total_regret_ppm: u64,
    pub mean_regret_per_sequence_ppm: u64,
    pub max_sequence_regret_ppm: u64,
    pub zero_regret_sequences: usize,
    pub committed_switches: u64,
    pub temporal_held_steps: u64,
    pub temporal_confirmed_steps: u64,
    pub temporal_urgent_steps: u64,
    pub precision_escalation_requests: u64,
    pub evidence: Vec<NativeMorRuntimeSequenceEvidence>,
}

pub struct NativeMorRuntimeSequenceObjective;

impl NativeMorRuntimeSequenceObjective {
    pub fn evaluate(
        runtime: &NativeMorRuntime,
        sequences: &[NativeMorRuntimeSequenceExample],
        policy: MorRouteSequencePolicy,
    ) -> Result<NativeMorRuntimeSequenceObjectiveReport, NativeMorRuntimeSequenceError> {
        policy
            .validate()
            .map_err(NativeMorRuntimeSequenceError::SequenceObjective)?;
        if sequences.is_empty() {
            return Err(NativeMorRuntimeSequenceError::EmptySequenceBatch);
        }

        let mut evidence = Vec::with_capacity(sequences.len());
        let mut total_steps = 0_usize;
        let mut total_chosen = 0_u64;
        let mut total_oracle = 0_u64;
        let mut total_regret = 0_u64;
        let mut max_regret = 0_u64;
        let mut zero_regret_sequences = 0_usize;
        let mut committed_switches = 0_u64;
        let mut temporal_held_steps = 0_u64;
        let mut temporal_confirmed_steps = 0_u64;
        let mut temporal_urgent_steps = 0_u64;
        let mut precision_escalation_requests = 0_u64;

        for (sequence_index, sequence) in sequences.iter().enumerate() {
            if sequence.steps.is_empty() {
                return Err(NativeMorRuntimeSequenceError::EmptySequence(
                    sequence_index,
                ));
            }

            let mut sequence_runtime = runtime.clone();
            sequence_runtime.reset();

            let objective_sequence = MorRouteSequenceExample {
                steps: sequence
                    .steps
                    .iter()
                    .map(|step| MorRouteSequenceStep {
                        state: step.state.clone(),
                        outcomes: step.outcomes.clone(),
                    })
                    .collect(),
            };

            let mut committed_pages = Vec::with_capacity(sequence.steps.len());
            let mut runtime_steps = Vec::with_capacity(sequence.steps.len());

            for (step_index, step) in sequence.steps.iter().enumerate() {
                let report = sequence_runtime
                    .route_state(&step.state, &step.runtime_hints)
                    .map_err(NativeMorRuntimeSequenceError::Runtime)?;
                let observed_primary = report
                    .cycle
                    .cycle
                    .decision
                    .ranked_experts
                    .first()
                    .map(|expert| expert.page_id)
                    .ok_or(NativeMorRuntimeSequenceError::EmptyObservedDecision {
                        sequence_index,
                        step_index,
                    })?;
                let committed_primary = report
                    .committed_route
                    .primary
                    .ok_or(NativeMorRuntimeSequenceError::MissingCommittedRoute {
                        sequence_index,
                        step_index,
                    })?;

                committed_pages.push(committed_primary);
                let transition = report.cycle.temporal.transition;
                match transition {
                    MorTemporalTransition::Held => {
                        temporal_held_steps = temporal_held_steps.saturating_add(1);
                    }
                    MorTemporalTransition::Confirmed => {
                        temporal_confirmed_steps = temporal_confirmed_steps.saturating_add(1);
                    }
                    MorTemporalTransition::Urgent => {
                        temporal_urgent_steps = temporal_urgent_steps.saturating_add(1);
                    }
                    MorTemporalTransition::Initialized | MorTemporalTransition::Stable => {}
                }
                if report
                    .cycle
                    .cycle
                    .metrics
                    .precision_escalation_requested
                {
                    precision_escalation_requests =
                        precision_escalation_requests.saturating_add(1);
                }

                runtime_steps.push(NativeMorRuntimeStepEvidence {
                    step_index,
                    observed_primary,
                    committed_primary,
                    temporal_transition: transition,
                    temporal_held: transition == MorTemporalTransition::Held,
                    active_routers: report.cycle.cycle.metrics.active_routers,
                    selected_experts: report.cycle.cycle.metrics.selected_experts,
                    noise_ppm: report.cycle.cycle.metrics.noise_ppm,
                    structural_disagreement_ppm: report
                        .cycle
                        .cycle
                        .metrics
                        .structural_disagreement_ppm,
                    precision_escalation_requested: report
                        .cycle
                        .cycle
                        .metrics
                        .precision_escalation_requested,
                });
            }

            let objective = evaluate_sequence_from_chosen_pages(
                &objective_sequence,
                sequence_index,
                policy,
                &committed_pages,
            )
            .map_err(NativeMorRuntimeSequenceError::SequenceObjective)?;

            total_steps = total_steps
                .checked_add(objective.steps.len())
                .ok_or(NativeMorRuntimeSequenceError::ArithmeticOverflow)?;
            total_chosen = total_chosen
                .checked_add(objective.chosen_total_cost_ppm)
                .ok_or(NativeMorRuntimeSequenceError::ArithmeticOverflow)?;
            total_oracle = total_oracle
                .checked_add(objective.oracle_total_cost_ppm)
                .ok_or(NativeMorRuntimeSequenceError::ArithmeticOverflow)?;
            total_regret = total_regret
                .checked_add(objective.regret_ppm)
                .ok_or(NativeMorRuntimeSequenceError::ArithmeticOverflow)?;
            max_regret = max_regret.max(objective.regret_ppm);
            committed_switches = committed_switches
                .checked_add(objective.chosen_switches as u64)
                .ok_or(NativeMorRuntimeSequenceError::ArithmeticOverflow)?;
            if objective.regret_ppm == 0 {
                zero_regret_sequences = zero_regret_sequences.saturating_add(1);
            }

            evidence.push(NativeMorRuntimeSequenceEvidence {
                sequence_index,
                objective,
                runtime_steps,
            });
        }

        Ok(NativeMorRuntimeSequenceObjectiveReport {
            sequences: sequences.len(),
            total_steps,
            total_chosen_cost_ppm: total_chosen,
            total_oracle_cost_ppm: total_oracle,
            total_regret_ppm: total_regret,
            mean_regret_per_sequence_ppm: total_regret / sequences.len() as u64,
            max_sequence_regret_ppm: max_regret,
            zero_regret_sequences,
            committed_switches,
            temporal_held_steps,
            temporal_confirmed_steps,
            temporal_urgent_steps,
            precision_escalation_requests,
            evidence,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMorRuntimeSequenceError {
    EmptySequenceBatch,
    EmptySequence(usize),
    EmptyObservedDecision {
        sequence_index: usize,
        step_index: usize,
    },
    MissingCommittedRoute {
        sequence_index: usize,
        step_index: usize,
    },
    ArithmeticOverflow,
    Runtime(NativeMorRuntimeError),
    SequenceObjective(MorRouteSequenceError),
}

impl fmt::Display for NativeMorRuntimeSequenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for NativeMorRuntimeSequenceError {}
