//! Regret- and cost-aware training objective for native SML MOR routing.
//!
//! This module evaluates the primary expert chosen by a native affinity bank
//! against the best admissible expert declared for each training state. The
//! objective combines task loss, compute cost and I/O cost in exact integer ppm.
//! A deterministic coordinate search can then tune router-signature weights
//! while preserving the hard sparse SML routing structure.

use core::fmt;
use std::collections::BTreeSet;

use crate::{
    mor::{MixtureOfRouters, PreviousRoute, RouterId, MOR_SCALE_PPM},
    mor_affinity::{MorAffinityError, NativeAffinityBank, NativeRouterSpec, SmlFingerprintPlan},
    BooleanWorkspace, PageId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorRouteObjectivePolicy {
    pub task_loss_weight_ppm: u32,
    pub compute_cost_weight_ppm: u32,
    pub io_cost_weight_ppm: u32,
}

impl Default for MorRouteObjectivePolicy {
    fn default() -> Self {
        Self {
            task_loss_weight_ppm: 800_000,
            compute_cost_weight_ppm: 150_000,
            io_cost_weight_ppm: 50_000,
        }
    }
}

impl MorRouteObjectivePolicy {
    pub fn validate(self) -> Result<Self, MorRouteObjectiveError> {
        for value in [
            self.task_loss_weight_ppm,
            self.compute_cost_weight_ppm,
            self.io_cost_weight_ppm,
        ] {
            if value > MOR_SCALE_PPM {
                return Err(MorRouteObjectiveError::PolicyPpmOutOfRange(value));
            }
        }
        let total = self.task_loss_weight_ppm as u64
            + self.compute_cost_weight_ppm as u64
            + self.io_cost_weight_ppm as u64;
        if total != MOR_SCALE_PPM as u64 {
            return Err(MorRouteObjectiveError::PolicyWeightsMustSumToScale { actual: total });
        }
        Ok(self)
    }

    fn combined_cost_ppm(self, outcome: MorExpertOutcome) -> Result<u32, MorRouteObjectiveError> {
        validate_outcome(outcome)?;
        let weighted = self.task_loss_weight_ppm as u128 * outcome.task_loss_ppm as u128
            + self.compute_cost_weight_ppm as u128 * outcome.compute_cost_ppm as u128
            + self.io_cost_weight_ppm as u128 * outcome.io_cost_ppm as u128;
        let cost = weighted / MOR_SCALE_PPM as u128;
        u32::try_from(cost).map_err(|_| MorRouteObjectiveError::ArithmeticOverflow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorExpertOutcome {
    pub page_id: PageId,
    pub task_loss_ppm: u32,
    pub compute_cost_ppm: u32,
    pub io_cost_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorRouteTrainingExample {
    pub state: BooleanWorkspace,
    pub outcomes: Vec<MorExpertOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorRouteCaseEvidence {
    pub case_index: usize,
    pub chosen_page: PageId,
    pub oracle_page: PageId,
    pub chosen_objective_ppm: u32,
    pub oracle_objective_ppm: u32,
    pub regret_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorRouteObjectiveReport {
    pub examples: usize,
    pub total_chosen_objective_ppm: u64,
    pub total_oracle_objective_ppm: u64,
    pub total_regret_ppm: u64,
    pub mean_regret_ppm: u32,
    pub max_regret_ppm: u32,
    pub zero_regret_examples: usize,
    pub zero_regret_fraction_ppm: u32,
    pub cases: Vec<MorRouteCaseEvidence>,
}

pub struct MorRouteObjective;

impl MorRouteObjective {
    pub fn evaluate(
        bank: &NativeAffinityBank,
        engine: &MixtureOfRouters,
        examples: &[MorRouteTrainingExample],
        policy: MorRouteObjectivePolicy,
    ) -> Result<MorRouteObjectiveReport, MorRouteObjectiveError> {
        let policy = policy.validate()?;
        if examples.is_empty() {
            return Err(MorRouteObjectiveError::EmptyTrainingSet);
        }

        let mut cases = Vec::with_capacity(examples.len());
        let mut total_chosen = 0_u64;
        let mut total_oracle = 0_u64;
        let mut total_regret = 0_u64;
        let mut max_regret = 0_u32;
        let mut zero_regret_examples = 0_usize;

        for (case_index, example) in examples.iter().enumerate() {
            validate_example(example, case_index)?;
            let (decision, _) = bank
                .route(engine, &example.state, &[], &PreviousRoute::default())
                .map_err(MorRouteObjectiveError::Affinity)?;
            let chosen_page = decision
                .ranked_experts
                .first()
                .map(|expert| expert.page_id)
                .ok_or(MorRouteObjectiveError::EmptyDecision(case_index))?;

            let chosen_outcome = example
                .outcomes
                .iter()
                .copied()
                .find(|outcome| outcome.page_id == chosen_page)
                .ok_or(MorRouteObjectiveError::MissingChosenOutcome {
                    case_index,
                    page_id: chosen_page,
                })?;
            let chosen_objective_ppm = policy.combined_cost_ppm(chosen_outcome)?;

            let mut oracle: Option<(u32, PageId)> = None;
            for outcome in example.outcomes.iter().copied() {
                let cost = policy.combined_cost_ppm(outcome)?;
                match oracle {
                    None => oracle = Some((cost, outcome.page_id)),
                    Some((best_cost, best_page))
                        if cost < best_cost
                            || (cost == best_cost && outcome.page_id.0 < best_page.0) =>
                    {
                        oracle = Some((cost, outcome.page_id));
                    }
                    Some(_) => {}
                }
            }
            let (oracle_objective_ppm, oracle_page) =
                oracle.ok_or(MorRouteObjectiveError::EmptyOutcomes(case_index))?;
            let regret_ppm = chosen_objective_ppm.saturating_sub(oracle_objective_ppm);

            total_chosen = total_chosen
                .checked_add(chosen_objective_ppm as u64)
                .ok_or(MorRouteObjectiveError::ArithmeticOverflow)?;
            total_oracle = total_oracle
                .checked_add(oracle_objective_ppm as u64)
                .ok_or(MorRouteObjectiveError::ArithmeticOverflow)?;
            total_regret = total_regret
                .checked_add(regret_ppm as u64)
                .ok_or(MorRouteObjectiveError::ArithmeticOverflow)?;
            max_regret = max_regret.max(regret_ppm);
            if regret_ppm == 0 {
                zero_regret_examples = zero_regret_examples.saturating_add(1);
            }

            cases.push(MorRouteCaseEvidence {
                case_index,
                chosen_page,
                oracle_page,
                chosen_objective_ppm,
                oracle_objective_ppm,
                regret_ppm,
            });
        }

        Ok(MorRouteObjectiveReport {
            examples: examples.len(),
            total_chosen_objective_ppm: total_chosen,
            total_oracle_objective_ppm: total_oracle,
            total_regret_ppm: total_regret,
            mean_regret_ppm: (total_regret / examples.len() as u64) as u32,
            max_regret_ppm: max_regret,
            zero_regret_examples,
            zero_regret_fraction_ppm: fraction_ppm(zero_regret_examples, examples.len()),
            cases,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorRouteWeightSearchConfig {
    pub passes: u32,
    pub candidate_weights_ppm: Vec<u32>,
}

impl Default for MorRouteWeightSearchConfig {
    fn default() -> Self {
        Self {
            passes: 4,
            candidate_weights_ppm: vec![100_000, 250_000, 500_000, 750_000, 1_000_000],
        }
    }
}

impl MorRouteWeightSearchConfig {
    pub fn validate(mut self) -> Result<Self, MorRouteObjectiveError> {
        if self.passes == 0 {
            return Err(MorRouteObjectiveError::ZeroSearchPasses);
        }
        if self.candidate_weights_ppm.is_empty() {
            return Err(MorRouteObjectiveError::EmptyWeightGrid);
        }
        self.candidate_weights_ppm.sort_unstable();
        self.candidate_weights_ppm.dedup();
        for weight in &self.candidate_weights_ppm {
            if *weight == 0 || *weight > MOR_SCALE_PPM {
                return Err(MorRouteObjectiveError::InvalidCandidateWeight(*weight));
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorRouteWeightSearchReport {
    pub baseline: MorRouteObjectiveReport,
    pub final_report: MorRouteObjectiveReport,
    pub passes_completed: u32,
    pub candidate_evaluations: u64,
    pub accepted_updates: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorRouteWeightSearchArtifact {
    pub bank: NativeAffinityBank,
    pub report: MorRouteWeightSearchReport,
}

pub struct MorRouteWeightSearch;

impl MorRouteWeightSearch {
    pub fn train(
        bank: NativeAffinityBank,
        engine: &MixtureOfRouters,
        examples: &[MorRouteTrainingExample],
        objective_policy: MorRouteObjectivePolicy,
        search_config: MorRouteWeightSearchConfig,
    ) -> Result<MorRouteWeightSearchArtifact, MorRouteObjectiveError> {
        let search_config = search_config.validate()?;
        let baseline = MorRouteObjective::evaluate(&bank, engine, examples, objective_policy)?;
        let mut current_bank = bank;
        let mut current_report = baseline.clone();
        let mut passes_completed = 0_u32;
        let mut candidate_evaluations = 0_u64;
        let mut accepted_updates = 0_u64;

        for _ in 0..search_config.passes {
            let mut changed = false;
            let router_count = current_bank.routers().len();

            for router_index in 0..router_count {
                let atom_count = current_bank.routers()[router_index].atoms.len();
                for atom_index in 0..atom_count {
                    let current_weight =
                        current_bank.routers()[router_index].atoms[atom_index].weight_ppm;
                    let mut best_weight = current_weight;
                    let mut best_report = current_report.clone();

                    for candidate_weight in &search_config.candidate_weights_ppm {
                        if *candidate_weight == current_weight {
                            continue;
                        }
                        let candidate_bank = rebuild_with_weight(
                            &current_bank,
                            router_index,
                            atom_index,
                            *candidate_weight,
                        )?;
                        let candidate_report = MorRouteObjective::evaluate(
                            &candidate_bank,
                            engine,
                            examples,
                            objective_policy,
                        )?;
                        candidate_evaluations = candidate_evaluations.saturating_add(1);

                        if report_is_better(&candidate_report, &best_report)
                            || (reports_equal_rank(&candidate_report, &best_report)
                                && *candidate_weight < best_weight)
                        {
                            best_weight = *candidate_weight;
                            best_report = candidate_report;
                        }
                    }

                    if best_weight != current_weight {
                        current_bank = rebuild_with_weight(
                            &current_bank,
                            router_index,
                            atom_index,
                            best_weight,
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

        Ok(MorRouteWeightSearchArtifact {
            bank: current_bank,
            report: MorRouteWeightSearchReport {
                baseline,
                final_report: current_report,
                passes_completed,
                candidate_evaluations,
                accepted_updates,
            },
        })
    }
}

fn rebuild_with_weight(
    bank: &NativeAffinityBank,
    router_index: usize,
    atom_index: usize,
    weight_ppm: u32,
) -> Result<NativeAffinityBank, MorRouteObjectiveError> {
    let mut routers: Vec<NativeRouterSpec> = bank.routers().to_vec();
    let router = routers
        .get_mut(router_index)
        .ok_or(MorRouteObjectiveError::InternalInvariant)?;
    let atom = router
        .atoms
        .get_mut(atom_index)
        .ok_or(MorRouteObjectiveError::InternalInvariant)?;
    atom.weight_ppm = weight_ppm;

    NativeAffinityBank::new(
        SmlFingerprintPlan::new(bank.fingerprint_plan().positions().to_vec(), bank.policy())
            .map_err(MorRouteObjectiveError::Affinity)?,
        routers,
        bank.policy(),
    )
    .map_err(MorRouteObjectiveError::Affinity)
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

fn reports_equal_rank(left: &MorRouteObjectiveReport, right: &MorRouteObjectiveReport) -> bool {
    left.total_regret_ppm == right.total_regret_ppm
        && left.total_chosen_objective_ppm == right.total_chosen_objective_ppm
        && left.max_regret_ppm == right.max_regret_ppm
}

fn validate_example(
    example: &MorRouteTrainingExample,
    case_index: usize,
) -> Result<(), MorRouteObjectiveError> {
    if example.outcomes.is_empty() {
        return Err(MorRouteObjectiveError::EmptyOutcomes(case_index));
    }
    let mut pages = BTreeSet::new();
    for outcome in example.outcomes.iter().copied() {
        validate_outcome(outcome)?;
        if !pages.insert(outcome.page_id.0) {
            return Err(MorRouteObjectiveError::DuplicateOutcome {
                case_index,
                page_id: outcome.page_id,
            });
        }
    }
    Ok(())
}

fn validate_outcome(outcome: MorExpertOutcome) -> Result<(), MorRouteObjectiveError> {
    for value in [
        outcome.task_loss_ppm,
        outcome.compute_cost_ppm,
        outcome.io_cost_ppm,
    ] {
        if value > MOR_SCALE_PPM {
            return Err(MorRouteObjectiveError::OutcomePpmOutOfRange {
                page_id: outcome.page_id,
                value,
            });
        }
    }
    Ok(())
}

fn fraction_ppm(numerator: usize, denominator: usize) -> u32 {
    if denominator == 0 {
        return 0;
    }
    ((numerator as u128 * MOR_SCALE_PPM as u128) / denominator as u128) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorRouteObjectiveError {
    PolicyPpmOutOfRange(u32),
    PolicyWeightsMustSumToScale { actual: u64 },
    EmptyTrainingSet,
    EmptyOutcomes(usize),
    DuplicateOutcome { case_index: usize, page_id: PageId },
    OutcomePpmOutOfRange { page_id: PageId, value: u32 },
    EmptyDecision(usize),
    MissingChosenOutcome { case_index: usize, page_id: PageId },
    ZeroSearchPasses,
    EmptyWeightGrid,
    InvalidCandidateWeight(u32),
    ArithmeticOverflow,
    InternalInvariant,
    Affinity(MorAffinityError),
}

impl fmt::Display for MorRouteObjectiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorRouteObjectiveError {}
