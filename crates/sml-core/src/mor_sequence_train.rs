//! Sequence-aware deterministic training for native SML MOR.
//!
//! This optimizer minimizes recurrent path regret rather than isolated-step
//! regret. It alternates coordinate search over router-signature weights and
//! router-to-expert proposal scores while preserving the sparse hard structure.

use core::fmt;

use crate::{
    mor::{MixtureOfRouters, MOR_SCALE_PPM},
    mor_affinity::{MorAffinityError, NativeAffinityBank, NativeRouterSpec, SmlFingerprintPlan},
    mor_sequence_objective::{
        MorRouteSequenceError, MorRouteSequenceExample, MorRouteSequenceObjective,
        MorRouteSequenceObjectiveReport, MorRouteSequencePolicy,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorSequenceSearchConfig {
    pub passes: u32,
    pub candidate_signature_weights_ppm: Vec<u32>,
    pub candidate_proposal_scores_ppm: Vec<u32>,
}

impl Default for MorSequenceSearchConfig {
    fn default() -> Self {
        Self {
            passes: 4,
            candidate_signature_weights_ppm: vec![
                100_000, 250_000, 500_000, 750_000, 1_000_000,
            ],
            candidate_proposal_scores_ppm: vec![
                100_000, 250_000, 500_000, 750_000, 1_000_000,
            ],
        }
    }
}

impl MorSequenceSearchConfig {
    pub fn validate(mut self) -> Result<Self, MorSequenceTrainingError> {
        if self.passes == 0 {
            return Err(MorSequenceTrainingError::ZeroSearchPasses);
        }
        validate_grid(
            &mut self.candidate_signature_weights_ppm,
            MorSequenceTrainingError::EmptySignatureWeightGrid,
            MorSequenceTrainingError::InvalidSignatureWeight,
        )?;
        validate_grid(
            &mut self.candidate_proposal_scores_ppm,
            MorSequenceTrainingError::EmptyProposalScoreGrid,
            MorSequenceTrainingError::InvalidProposalScore,
        )?;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorSequenceSearchReport {
    pub baseline: MorRouteSequenceObjectiveReport,
    pub final_report: MorRouteSequenceObjectiveReport,
    pub passes_completed: u32,
    pub candidate_evaluations: u64,
    pub signature_weight_updates: u64,
    pub proposal_score_updates: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorSequenceSearchArtifact {
    pub bank: NativeAffinityBank,
    pub report: MorSequenceSearchReport,
}

pub struct MorSequenceSearch;

impl MorSequenceSearch {
    pub fn train(
        bank: NativeAffinityBank,
        engine: &MixtureOfRouters,
        sequences: &[MorRouteSequenceExample],
        objective_policy: MorRouteSequencePolicy,
        config: MorSequenceSearchConfig,
    ) -> Result<MorSequenceSearchArtifact, MorSequenceTrainingError> {
        let config = config.validate()?;
        let baseline = MorRouteSequenceObjective::evaluate(
            &bank,
            engine,
            sequences,
            objective_policy,
        )
        .map_err(MorSequenceTrainingError::SequenceObjective)?;

        let mut current_bank = bank;
        let mut current_report = baseline.clone();
        let mut passes_completed = 0_u32;
        let mut candidate_evaluations = 0_u64;
        let mut signature_weight_updates = 0_u64;
        let mut proposal_score_updates = 0_u64;

        for _ in 0..config.passes {
            let mut changed = false;

            for router_index in 0..current_bank.routers().len() {
                let atom_count = current_bank.routers()[router_index].atoms.len();
                for atom_index in 0..atom_count {
                    let current =
                        current_bank.routers()[router_index].atoms[atom_index].weight_ppm;
                    let mut best_value = current;
                    let mut best_report = current_report.clone();

                    for candidate in &config.candidate_signature_weights_ppm {
                        if *candidate == current {
                            continue;
                        }
                        let candidate_bank = rebuild_atom_weight(
                            &current_bank,
                            router_index,
                            atom_index,
                            *candidate,
                        )?;
                        let candidate_report = evaluate(
                            &candidate_bank,
                            engine,
                            sequences,
                            objective_policy,
                        )?;
                        candidate_evaluations = candidate_evaluations.saturating_add(1);

                        if report_is_better(&candidate_report, &best_report)
                            || (reports_equal_rank(&candidate_report, &best_report)
                                && *candidate < best_value)
                        {
                            best_value = *candidate;
                            best_report = candidate_report;
                        }
                    }

                    if best_value != current {
                        current_bank = rebuild_atom_weight(
                            &current_bank,
                            router_index,
                            atom_index,
                            best_value,
                        )?;
                        current_report = best_report;
                        signature_weight_updates = signature_weight_updates.saturating_add(1);
                        changed = true;
                    }
                }
            }

            for router_index in 0..current_bank.routers().len() {
                let proposal_count = current_bank.routers()[router_index].proposals.len();
                for proposal_index in 0..proposal_count {
                    let current =
                        current_bank.routers()[router_index].proposals[proposal_index].score_ppm;
                    let mut best_value = current;
                    let mut best_report = current_report.clone();

                    for candidate in &config.candidate_proposal_scores_ppm {
                        if *candidate == current {
                            continue;
                        }
                        let candidate_bank = rebuild_proposal_score(
                            &current_bank,
                            router_index,
                            proposal_index,
                            *candidate,
                        )?;
                        let candidate_report = evaluate(
                            &candidate_bank,
                            engine,
                            sequences,
                            objective_policy,
                        )?;
                        candidate_evaluations = candidate_evaluations.saturating_add(1);

                        if report_is_better(&candidate_report, &best_report)
                            || (reports_equal_rank(&candidate_report, &best_report)
                                && *candidate < best_value)
                        {
                            best_value = *candidate;
                            best_report = candidate_report;
                        }
                    }

                    if best_value != current {
                        current_bank = rebuild_proposal_score(
                            &current_bank,
                            router_index,
                            proposal_index,
                            best_value,
                        )?;
                        current_report = best_report;
                        proposal_score_updates = proposal_score_updates.saturating_add(1);
                        changed = true;
                    }
                }
            }

            passes_completed = passes_completed.saturating_add(1);
            if !changed {
                break;
            }
        }

        Ok(MorSequenceSearchArtifact {
            bank: current_bank,
            report: MorSequenceSearchReport {
                baseline,
                final_report: current_report,
                passes_completed,
                candidate_evaluations,
                signature_weight_updates,
                proposal_score_updates,
            },
        })
    }
}

fn evaluate(
    bank: &NativeAffinityBank,
    engine: &MixtureOfRouters,
    sequences: &[MorRouteSequenceExample],
    policy: MorRouteSequencePolicy,
) -> Result<MorRouteSequenceObjectiveReport, MorSequenceTrainingError> {
    MorRouteSequenceObjective::evaluate(bank, engine, sequences, policy)
        .map_err(MorSequenceTrainingError::SequenceObjective)
}

fn rebuild_atom_weight(
    bank: &NativeAffinityBank,
    router_index: usize,
    atom_index: usize,
    weight_ppm: u32,
) -> Result<NativeAffinityBank, MorSequenceTrainingError> {
    let mut routers: Vec<NativeRouterSpec> = bank.routers().to_vec();
    let router = routers
        .get_mut(router_index)
        .ok_or(MorSequenceTrainingError::InternalInvariant)?;
    let atom = router
        .atoms
        .get_mut(atom_index)
        .ok_or(MorSequenceTrainingError::InternalInvariant)?;
    atom.weight_ppm = weight_ppm;
    rebuild_bank(bank, routers)
}

fn rebuild_proposal_score(
    bank: &NativeAffinityBank,
    router_index: usize,
    proposal_index: usize,
    score_ppm: u32,
) -> Result<NativeAffinityBank, MorSequenceTrainingError> {
    let mut routers: Vec<NativeRouterSpec> = bank.routers().to_vec();
    let router = routers
        .get_mut(router_index)
        .ok_or(MorSequenceTrainingError::InternalInvariant)?;
    let proposal = router
        .proposals
        .get_mut(proposal_index)
        .ok_or(MorSequenceTrainingError::InternalInvariant)?;
    proposal.score_ppm = score_ppm;
    rebuild_bank(bank, routers)
}

fn rebuild_bank(
    bank: &NativeAffinityBank,
    routers: Vec<NativeRouterSpec>,
) -> Result<NativeAffinityBank, MorSequenceTrainingError> {
    let plan = SmlFingerprintPlan::new(
        bank.fingerprint_plan().positions().to_vec(),
        bank.policy(),
    )
    .map_err(MorSequenceTrainingError::Affinity)?;
    NativeAffinityBank::new(plan, routers, bank.policy()).map_err(MorSequenceTrainingError::Affinity)
}

fn report_is_better(
    candidate: &MorRouteSequenceObjectiveReport,
    incumbent: &MorRouteSequenceObjectiveReport,
) -> bool {
    candidate.total_regret_ppm < incumbent.total_regret_ppm
        || (candidate.total_regret_ppm == incumbent.total_regret_ppm
            && candidate.total_chosen_cost_ppm < incumbent.total_chosen_cost_ppm)
        || (candidate.total_regret_ppm == incumbent.total_regret_ppm
            && candidate.total_chosen_cost_ppm == incumbent.total_chosen_cost_ppm
            && candidate.max_sequence_regret_ppm < incumbent.max_sequence_regret_ppm)
}

fn reports_equal_rank(
    left: &MorRouteSequenceObjectiveReport,
    right: &MorRouteSequenceObjectiveReport,
) -> bool {
    left.total_regret_ppm == right.total_regret_ppm
        && left.total_chosen_cost_ppm == right.total_chosen_cost_ppm
        && left.max_sequence_regret_ppm == right.max_sequence_regret_ppm
}

fn validate_grid(
    values: &mut Vec<u32>,
    empty_error: MorSequenceTrainingError,
    invalid: fn(u32) -> MorSequenceTrainingError,
) -> Result<(), MorSequenceTrainingError> {
    if values.is_empty() {
        return Err(empty_error);
    }
    values.sort_unstable();
    values.dedup();
    for value in values {
        if *value == 0 || *value > MOR_SCALE_PPM {
            return Err(invalid(*value));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorSequenceTrainingError {
    ZeroSearchPasses,
    EmptySignatureWeightGrid,
    EmptyProposalScoreGrid,
    InvalidSignatureWeight(u32),
    InvalidProposalScore(u32),
    InternalInvariant,
    Affinity(MorAffinityError),
    SequenceObjective(MorRouteSequenceError),
}

impl fmt::Display for MorSequenceTrainingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorSequenceTrainingError {}
