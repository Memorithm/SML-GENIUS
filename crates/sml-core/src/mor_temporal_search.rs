//! Deterministic search of temporal-denoising policy for native SML MOR.
//!
//! Candidate policies are evaluated against the full-runtime recurrent objective.
//! The current policy remains selected unless a candidate strictly improves the
//! lexicographic runtime objective, avoiding policy churn on tied evidence.

use core::fmt;

use crate::{
    mor_runtime::{NativeMorRuntime, NativeMorRuntimeError},
    mor_runtime_sequence_objective::{
        NativeMorRuntimeSequenceError, NativeMorRuntimeSequenceExample,
        NativeMorRuntimeSequenceObjective, NativeMorRuntimeSequenceObjectiveReport,
    },
    mor_sequence_objective::MorRouteSequencePolicy,
    mor_temporal::{MorTemporalError, MorTemporalPolicy},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorTemporalPolicySearchConfig {
    pub candidates: Vec<MorTemporalPolicy>,
}

impl MorTemporalPolicySearchConfig {
    pub fn validate(mut self) -> Result<Self, MorTemporalPolicySearchError> {
        if self.candidates.is_empty() {
            return Err(MorTemporalPolicySearchError::EmptyCandidateSet);
        }

        let mut unique = Vec::with_capacity(self.candidates.len());
        for candidate in self.candidates.drain(..) {
            candidate
                .validate()
                .map_err(MorTemporalPolicySearchError::InvalidTemporalPolicy)?;
            if !unique.contains(&candidate) {
                unique.push(candidate);
            }
        }
        self.candidates = unique;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorTemporalPolicySearchReport {
    pub baseline_policy: MorTemporalPolicy,
    pub selected_policy: MorTemporalPolicy,
    pub baseline: NativeMorRuntimeSequenceObjectiveReport,
    pub final_report: NativeMorRuntimeSequenceObjectiveReport,
    pub candidates_evaluated: u64,
    pub accepted_change: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorTemporalPolicySearchArtifact {
    pub runtime: NativeMorRuntime,
    pub report: MorTemporalPolicySearchReport,
}

pub struct MorTemporalPolicySearch;

impl MorTemporalPolicySearch {
    pub fn train(
        runtime: &NativeMorRuntime,
        sequences: &[NativeMorRuntimeSequenceExample],
        objective_policy: MorRouteSequencePolicy,
        config: MorTemporalPolicySearchConfig,
    ) -> Result<MorTemporalPolicySearchArtifact, MorTemporalPolicySearchError> {
        let config = config.validate()?;
        let baseline_policy = runtime.controller().denoiser().policy();
        let baseline =
            NativeMorRuntimeSequenceObjective::evaluate(runtime, sequences, objective_policy)
                .map_err(MorTemporalPolicySearchError::Objective)?;

        let mut selected_policy = baseline_policy;
        let mut selected_report = baseline.clone();
        let mut candidates_evaluated = 0_u64;

        for candidate in config.candidates {
            if candidate == baseline_policy {
                continue;
            }
            let candidate_runtime = runtime
                .with_temporal_policy(candidate)
                .map_err(MorTemporalPolicySearchError::Runtime)?;
            let candidate_report = NativeMorRuntimeSequenceObjective::evaluate(
                &candidate_runtime,
                sequences,
                objective_policy,
            )
            .map_err(MorTemporalPolicySearchError::Objective)?;
            candidates_evaluated = candidates_evaluated.saturating_add(1);

            if report_is_better(&candidate_report, &selected_report) {
                selected_policy = candidate;
                selected_report = candidate_report;
            }
        }

        let selected_runtime = runtime
            .with_temporal_policy(selected_policy)
            .map_err(MorTemporalPolicySearchError::Runtime)?;

        Ok(MorTemporalPolicySearchArtifact {
            runtime: selected_runtime,
            report: MorTemporalPolicySearchReport {
                baseline_policy,
                selected_policy,
                baseline,
                final_report: selected_report,
                candidates_evaluated,
                accepted_change: selected_policy != baseline_policy,
            },
        })
    }
}

fn report_is_better(
    candidate: &NativeMorRuntimeSequenceObjectiveReport,
    incumbent: &NativeMorRuntimeSequenceObjectiveReport,
) -> bool {
    candidate.total_regret_ppm < incumbent.total_regret_ppm
        || (candidate.total_regret_ppm == incumbent.total_regret_ppm
            && candidate.total_chosen_cost_ppm < incumbent.total_chosen_cost_ppm)
        || (candidate.total_regret_ppm == incumbent.total_regret_ppm
            && candidate.total_chosen_cost_ppm == incumbent.total_chosen_cost_ppm
            && candidate.max_sequence_regret_ppm < incumbent.max_sequence_regret_ppm)
        || (candidate.total_regret_ppm == incumbent.total_regret_ppm
            && candidate.total_chosen_cost_ppm == incumbent.total_chosen_cost_ppm
            && candidate.max_sequence_regret_ppm == incumbent.max_sequence_regret_ppm
            && candidate.committed_switches < incumbent.committed_switches)
        || (candidate.total_regret_ppm == incumbent.total_regret_ppm
            && candidate.total_chosen_cost_ppm == incumbent.total_chosen_cost_ppm
            && candidate.max_sequence_regret_ppm == incumbent.max_sequence_regret_ppm
            && candidate.committed_switches == incumbent.committed_switches
            && candidate.temporal_held_steps < incumbent.temporal_held_steps)
        || (candidate.total_regret_ppm == incumbent.total_regret_ppm
            && candidate.total_chosen_cost_ppm == incumbent.total_chosen_cost_ppm
            && candidate.max_sequence_regret_ppm == incumbent.max_sequence_regret_ppm
            && candidate.committed_switches == incumbent.committed_switches
            && candidate.temporal_held_steps == incumbent.temporal_held_steps
            && candidate.precision_escalation_requests < incumbent.precision_escalation_requests)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorTemporalPolicySearchError {
    EmptyCandidateSet,
    InvalidTemporalPolicy(MorTemporalError),
    Runtime(NativeMorRuntimeError),
    Objective(NativeMorRuntimeSequenceError),
}

impl fmt::Display for MorTemporalPolicySearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorTemporalPolicySearchError {}
