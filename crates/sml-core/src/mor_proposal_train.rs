//! Regret-aware training of native MOR router-to-expert proposal scores.
//!
//! Router affinity decides which specialized routers participate. This module
//! trains the relative expert preferences inside those routers while preserving
//! the hard sparse router structure and native inference representation.

use core::fmt;

use crate::{
    mor::{MixtureOfRouters, MOR_SCALE_PPM},
    mor_affinity::{MorAffinityError, NativeAffinityBank, NativeRouterSpec, SmlFingerprintPlan},
    mor_route_objective::{
        MorRouteObjective, MorRouteObjectiveError, MorRouteObjectivePolicy, MorRouteObjectiveReport,
        MorRouteTrainingExample,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorProposalScoreSearchConfig {
    pub passes: u32,
    pub candidate_scores_ppm: Vec<u32>,
}

impl Default for MorProposalScoreSearchConfig {
    fn default() -> Self {
        Self {
            passes: 4,
            candidate_scores_ppm: vec![100_000, 250_000, 500_000, 750_000, 1_000_000],
        }
    }
}

impl MorProposalScoreSearchConfig {
    pub fn validate(mut self) -> Result<Self, MorProposalTrainingError> {
        if self.passes == 0 {
            return Err(MorProposalTrainingError::ZeroSearchPasses);
        }
        if self.candidate_scores_ppm.is_empty() {
            return Err(MorProposalTrainingError::EmptyScoreGrid);
        }
        self.candidate_scores_ppm.sort_unstable();
        self.candidate_scores_ppm.dedup();
        for score in &self.candidate_scores_ppm {
            if *score == 0 || *score > MOR_SCALE_PPM {
                return Err(MorProposalTrainingError::InvalidCandidateScore(*score));
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorProposalScoreSearchReport {
    pub baseline: MorRouteObjectiveReport,
    pub final_report: MorRouteObjectiveReport,
    pub passes_completed: u32,
    pub candidate_evaluations: u64,
    pub accepted_updates: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorProposalScoreSearchArtifact {
    pub bank: NativeAffinityBank,
    pub report: MorProposalScoreSearchReport,
}

pub struct MorProposalScoreSearch;

impl MorProposalScoreSearch {
    pub fn train(
        bank: NativeAffinityBank,
        engine: &MixtureOfRouters,
        examples: &[MorRouteTrainingExample],
        objective_policy: MorRouteObjectivePolicy,
        search_config: MorProposalScoreSearchConfig,
    ) -> Result<MorProposalScoreSearchArtifact, MorProposalTrainingError> {
        let search_config = search_config.validate()?;
        let baseline =
            MorRouteObjective::evaluate(&bank, engine, examples, objective_policy)
                .map_err(MorProposalTrainingError::Objective)?;
        let mut current_bank = bank;
        let mut current_report = baseline.clone();
        let mut passes_completed = 0_u32;
        let mut candidate_evaluations = 0_u64;
        let mut accepted_updates = 0_u64;

        for _ in 0..search_config.passes {
            let mut changed = false;
            let router_count = current_bank.routers().len();

            for router_index in 0..router_count {
                let proposal_count = current_bank.routers()[router_index].proposals.len();

                for proposal_index in 0..proposal_count {
                    let current_score =
                        current_bank.routers()[router_index].proposals[proposal_index].score_ppm;
                    let mut best_score = current_score;
                    let mut best_report = current_report.clone();

                    for candidate_score in &search_config.candidate_scores_ppm {
                        if *candidate_score == current_score {
                            continue;
                        }
                        let candidate_bank = rebuild_with_score(
                            &current_bank,
                            router_index,
                            proposal_index,
                            *candidate_score,
                        )?;
                        let candidate_report = MorRouteObjective::evaluate(
                            &candidate_bank,
                            engine,
                            examples,
                            objective_policy,
                        )
                        .map_err(MorProposalTrainingError::Objective)?;
                        candidate_evaluations = candidate_evaluations.saturating_add(1);

                        if report_is_better(&candidate_report, &best_report)
                            || (reports_equal_rank(&candidate_report, &best_report)
                                && *candidate_score < best_score)
                        {
                            best_score = *candidate_score;
                            best_report = candidate_report;
                        }
                    }

                    if best_score != current_score {
                        current_bank = rebuild_with_score(
                            &current_bank,
                            router_index,
                            proposal_index,
                            best_score,
                        )?;
                        current_report = best_report;
                        accepted_updates = accepted_updates.saturating_add(1);
                        changed = true;
                    }
                }
            }

            passes_completed = passes_completed.saturating_add(1);
            if !changed {
                break;
            }
        }

        Ok(MorProposalScoreSearchArtifact {
            bank: current_bank,
            report: MorProposalScoreSearchReport {
                baseline,
                final_report: current_report,
                passes_completed,
                candidate_evaluations,
                accepted_updates,
            },
        })
    }
}

fn rebuild_with_score(
    bank: &NativeAffinityBank,
    router_index: usize,
    proposal_index: usize,
    score_ppm: u32,
) -> Result<NativeAffinityBank, MorProposalTrainingError> {
    let mut routers: Vec<NativeRouterSpec> = bank.routers().to_vec();
    let router = routers
        .get_mut(router_index)
        .ok_or(MorProposalTrainingError::InternalInvariant)?;
    let proposal = router
        .proposals
        .get_mut(proposal_index)
        .ok_or(MorProposalTrainingError::InternalInvariant)?;
    proposal.score_ppm = score_ppm;

    let plan = SmlFingerprintPlan::new(
        bank.fingerprint_plan().positions().to_vec(),
        bank.policy(),
    )
    .map_err(MorProposalTrainingError::Affinity)?;

    NativeAffinityBank::new(plan, routers, bank.policy()).map_err(MorProposalTrainingError::Affinity)
}

fn report_is_better(
    candidate: &MorRouteObjectiveReport,
    incumbent: &MorRouteObjectiveReport,
) -> bool {
    candidate.total_regret_ppm < incumbent.total_regret_ppm
        || (candidate.total_regret_ppm == incumbent.total_regret_ppm
            && candidate.total_chosen_objective_ppm < incumbent.total_chosen_objective_ppm)
        || (candidate.total_regret_ppm == incumbent.total_regret_ppm
            && candidate.total_chosen_objective_ppm == incumbent.total_chosen_objective_ppm
            && candidate.max_regret_ppm < incumbent.max_regret_ppm)
}

fn reports_equal_rank(
    left: &MorRouteObjectiveReport,
    right: &MorRouteObjectiveReport,
) -> bool {
    left.total_regret_ppm == right.total_regret_ppm
        && left.total_chosen_objective_ppm == right.total_chosen_objective_ppm
        && left.max_regret_ppm == right.max_regret_ppm
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorProposalTrainingError {
    ZeroSearchPasses,
    EmptyScoreGrid,
    InvalidCandidateScore(u32),
    InternalInvariant,
    Affinity(MorAffinityError),
    Objective(MorRouteObjectiveError),
}

impl fmt::Display for MorProposalTrainingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorProposalTrainingError {}
