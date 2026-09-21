//! Sequence-level regret objective for recurrent native SML MOR routing.
//!
//! Per-state optimal routing is insufficient when changing experts has a cost.
//! This module evaluates the actual recurrent MOR path against the globally
//! optimal expert path computed by deterministic dynamic programming.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    mor::{MixtureOfRouters, PreviousRoute, MOR_SCALE_PPM},
    mor_affinity::{MorAffinityError, NativeAffinityBank},
    mor_route_objective::{MorExpertOutcome, MorRouteObjectivePolicy},
    BooleanWorkspace, PageId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorRouteSequencePolicy {
    pub step_objective: MorRouteObjectivePolicy,
    pub switch_penalty_ppm: u32,
}

impl Default for MorRouteSequencePolicy {
    fn default() -> Self {
        Self {
            step_objective: MorRouteObjectivePolicy::default(),
            switch_penalty_ppm: 50_000,
        }
    }
}

impl MorRouteSequencePolicy {
    pub fn validate(self) -> Result<Self, MorRouteSequenceError> {
        self.step_objective
            .validate()
            .map_err(MorRouteSequenceError::StepObjective)?;
        if self.switch_penalty_ppm > MOR_SCALE_PPM {
            return Err(MorRouteSequenceError::SwitchPenaltyOutOfRange(
                self.switch_penalty_ppm,
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorRouteSequenceStep {
    pub state: BooleanWorkspace,
    pub outcomes: Vec<MorExpertOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorRouteSequenceExample {
    pub steps: Vec<MorRouteSequenceStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorRouteSequenceStepEvidence {
    pub step_index: usize,
    pub chosen_page: PageId,
    pub oracle_page: PageId,
    pub chosen_base_cost_ppm: u32,
    pub oracle_base_cost_ppm: u32,
    pub chosen_switch: bool,
    pub oracle_switch: bool,
    pub chosen_step_cost_ppm: u64,
    pub oracle_step_cost_ppm: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorRouteSequenceEvidence {
    pub sequence_index: usize,
    pub steps: Vec<MorRouteSequenceStepEvidence>,
    pub chosen_total_cost_ppm: u64,
    pub oracle_total_cost_ppm: u64,
    pub regret_ppm: u64,
    pub chosen_switches: u32,
    pub oracle_switches: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorRouteSequenceObjectiveReport {
    pub sequences: usize,
    pub total_steps: usize,
    pub total_chosen_cost_ppm: u64,
    pub total_oracle_cost_ppm: u64,
    pub total_regret_ppm: u64,
    pub mean_regret_per_sequence_ppm: u64,
    pub max_sequence_regret_ppm: u64,
    pub zero_regret_sequences: usize,
    pub evidence: Vec<MorRouteSequenceEvidence>,
}

pub struct MorRouteSequenceObjective;

impl MorRouteSequenceObjective {
    pub fn evaluate(
        bank: &NativeAffinityBank,
        engine: &MixtureOfRouters,
        sequences: &[MorRouteSequenceExample],
        policy: MorRouteSequencePolicy,
    ) -> Result<MorRouteSequenceObjectiveReport, MorRouteSequenceError> {
        let policy = policy.validate()?;
        if sequences.is_empty() {
            return Err(MorRouteSequenceError::EmptySequenceBatch);
        }

        let mut evidence = Vec::with_capacity(sequences.len());
        let mut total_steps = 0_usize;
        let mut total_chosen = 0_u64;
        let mut total_oracle = 0_u64;
        let mut total_regret = 0_u64;
        let mut max_sequence_regret = 0_u64;
        let mut zero_regret_sequences = 0_usize;

        for (sequence_index, sequence) in sequences.iter().enumerate() {
            let sequence_evidence =
                evaluate_sequence(bank, engine, sequence, sequence_index, policy)?;
            total_steps = total_steps
                .checked_add(sequence_evidence.steps.len())
                .ok_or(MorRouteSequenceError::ArithmeticOverflow)?;
            total_chosen = total_chosen
                .checked_add(sequence_evidence.chosen_total_cost_ppm)
                .ok_or(MorRouteSequenceError::ArithmeticOverflow)?;
            total_oracle = total_oracle
                .checked_add(sequence_evidence.oracle_total_cost_ppm)
                .ok_or(MorRouteSequenceError::ArithmeticOverflow)?;
            total_regret = total_regret
                .checked_add(sequence_evidence.regret_ppm)
                .ok_or(MorRouteSequenceError::ArithmeticOverflow)?;
            max_sequence_regret = max_sequence_regret.max(sequence_evidence.regret_ppm);
            if sequence_evidence.regret_ppm == 0 {
                zero_regret_sequences = zero_regret_sequences.saturating_add(1);
            }
            evidence.push(sequence_evidence);
        }

        Ok(MorRouteSequenceObjectiveReport {
            sequences: sequences.len(),
            total_steps,
            total_chosen_cost_ppm: total_chosen,
            total_oracle_cost_ppm: total_oracle,
            total_regret_ppm: total_regret,
            mean_regret_per_sequence_ppm: total_regret / sequences.len() as u64,
            max_sequence_regret_ppm: max_sequence_regret,
            zero_regret_sequences,
            evidence,
        })
    }
}

fn evaluate_sequence(
    bank: &NativeAffinityBank,
    engine: &MixtureOfRouters,
    sequence: &MorRouteSequenceExample,
    sequence_index: usize,
    policy: MorRouteSequencePolicy,
) -> Result<MorRouteSequenceEvidence, MorRouteSequenceError> {
    if sequence.steps.is_empty() {
        return Err(MorRouteSequenceError::EmptySequence(sequence_index));
    }
    for (step_index, step) in sequence.steps.iter().enumerate() {
        validate_step(step, sequence_index, step_index)?;
    }

    let mut previous = PreviousRoute::default();
    let mut chosen_pages = Vec::with_capacity(sequence.steps.len());

    for (step_index, step) in sequence.steps.iter().enumerate() {
        let (decision, _) = bank
            .route(engine, &step.state, &[], &previous)
            .map_err(MorRouteSequenceError::Affinity)?;
        let chosen = decision
            .ranked_experts
            .first()
            .map(|expert| expert.page_id)
            .ok_or(MorRouteSequenceError::EmptyDecision {
                sequence_index,
                step_index,
            })?;
        if !step
            .outcomes
            .iter()
            .any(|outcome| outcome.page_id == chosen)
        {
            return Err(MorRouteSequenceError::MissingChosenOutcome {
                sequence_index,
                step_index,
                page_id: chosen,
            });
        }
        chosen_pages.push(chosen);
        previous = PreviousRoute {
            primary: Some(chosen),
            selected: decision.selected_experts,
        };
    }

    evaluate_sequence_from_chosen_pages(
        sequence,
        sequence_index,
        policy,
        &chosen_pages,
    )
}

pub(crate) fn evaluate_sequence_from_chosen_pages(
    sequence: &MorRouteSequenceExample,
    sequence_index: usize,
    policy: MorRouteSequencePolicy,
    chosen_pages: &[PageId],
) -> Result<MorRouteSequenceEvidence, MorRouteSequenceError> {
    if sequence.steps.is_empty() {
        return Err(MorRouteSequenceError::EmptySequence(sequence_index));
    }
    if chosen_pages.len() != sequence.steps.len() {
        return Err(MorRouteSequenceError::ChosenPathLength {
            sequence_index,
            expected: sequence.steps.len(),
            actual: chosen_pages.len(),
        });
    }
    for (step_index, step) in sequence.steps.iter().enumerate() {
        validate_step(step, sequence_index, step_index)?;
    }

    let oracle_path = optimal_path(sequence, policy)?;
    let mut steps = Vec::with_capacity(sequence.steps.len());
    let mut chosen_total = 0_u64;
    let mut oracle_total = 0_u64;
    let mut chosen_switches = 0_u32;
    let mut oracle_switches = 0_u32;

    for step_index in 0..sequence.steps.len() {
        let chosen_page = chosen_pages[step_index];
        let oracle_page = oracle_path[step_index];
        let chosen_base = outcome_cost(
            outcome_for(&sequence.steps[step_index], chosen_page).ok_or(
                MorRouteSequenceError::MissingChosenOutcome {
                    sequence_index,
                    step_index,
                    page_id: chosen_page,
                },
            )?,
            policy.step_objective,
        )?;
        let oracle_base = outcome_cost(
            outcome_for(&sequence.steps[step_index], oracle_page)
                .ok_or(MorRouteSequenceError::InternalInvariant)?,
            policy.step_objective,
        )?;
        let chosen_switch = step_index > 0 && chosen_pages[step_index - 1] != chosen_page;
        let oracle_switch = step_index > 0 && oracle_path[step_index - 1] != oracle_page;
        if chosen_switch {
            chosen_switches = chosen_switches.saturating_add(1);
        }
        if oracle_switch {
            oracle_switches = oracle_switches.saturating_add(1);
        }
        let chosen_step_cost =
            chosen_base as u64 + u64::from(chosen_switch) * policy.switch_penalty_ppm as u64;
        let oracle_step_cost =
            oracle_base as u64 + u64::from(oracle_switch) * policy.switch_penalty_ppm as u64;
        chosen_total = chosen_total
            .checked_add(chosen_step_cost)
            .ok_or(MorRouteSequenceError::ArithmeticOverflow)?;
        oracle_total = oracle_total
            .checked_add(oracle_step_cost)
            .ok_or(MorRouteSequenceError::ArithmeticOverflow)?;

        steps.push(MorRouteSequenceStepEvidence {
            step_index,
            chosen_page,
            oracle_page,
            chosen_base_cost_ppm: chosen_base,
            oracle_base_cost_ppm: oracle_base,
            chosen_switch,
            oracle_switch,
            chosen_step_cost_ppm: chosen_step_cost,
            oracle_step_cost_ppm: oracle_step_cost,
        });
    }

    Ok(MorRouteSequenceEvidence {
        sequence_index,
        steps,
        chosen_total_cost_ppm: chosen_total,
        oracle_total_cost_ppm: oracle_total,
        regret_ppm: chosen_total.saturating_sub(oracle_total),
        chosen_switches,
        oracle_switches,
    })
}

fn optimal_path(
    sequence: &MorRouteSequenceExample,
    policy: MorRouteSequencePolicy,
) -> Result<Vec<PageId>, MorRouteSequenceError> {
    let mut costs: BTreeMap<u32, u64> = BTreeMap::new();
    let mut predecessors: Vec<BTreeMap<u32, Option<u32>>> =
        Vec::with_capacity(sequence.steps.len());

    let mut first_predecessors = BTreeMap::new();
    for outcome in sequence.steps[0].outcomes.iter().copied() {
        let cost = outcome_cost(outcome, policy.step_objective)? as u64;
        costs.insert(outcome.page_id.0, cost);
        first_predecessors.insert(outcome.page_id.0, None);
    }
    predecessors.push(first_predecessors);

    for step in sequence.steps.iter().skip(1) {
        let mut next_costs = BTreeMap::new();
        let mut step_predecessors = BTreeMap::new();

        for outcome in step.outcomes.iter().copied() {
            let base = outcome_cost(outcome, policy.step_objective)? as u64;
            let mut best: Option<(u64, u32)> = None;

            for (previous_page, previous_cost) in &costs {
                let switch = u64::from(*previous_page != outcome.page_id.0)
                    * policy.switch_penalty_ppm as u64;
                let candidate = previous_cost
                    .checked_add(base)
                    .and_then(|value| value.checked_add(switch))
                    .ok_or(MorRouteSequenceError::ArithmeticOverflow)?;
                match best {
                    None => best = Some((candidate, *previous_page)),
                    Some((best_cost, best_page))
                        if candidate < best_cost
                            || (candidate == best_cost && *previous_page < best_page) =>
                    {
                        best = Some((candidate, *previous_page));
                    }
                    Some(_) => {}
                }
            }

            let (best_cost, best_previous) =
                best.ok_or(MorRouteSequenceError::InternalInvariant)?;
            next_costs.insert(outcome.page_id.0, best_cost);
            step_predecessors.insert(outcome.page_id.0, Some(best_previous));
        }

        costs = next_costs;
        predecessors.push(step_predecessors);
    }

    let mut final_page: Option<(u64, u32)> = None;
    for (page, cost) in &costs {
        match final_page {
            None => final_page = Some((*cost, *page)),
            Some((best_cost, best_page))
                if *cost < best_cost || (*cost == best_cost && *page < best_page) =>
            {
                final_page = Some((*cost, *page));
            }
            Some(_) => {}
        }
    }
    let (_, mut current_page) = final_page.ok_or(MorRouteSequenceError::InternalInvariant)?;
    let mut path = vec![PageId(0); sequence.steps.len()];

    for step_index in (0..sequence.steps.len()).rev() {
        path[step_index] = PageId(current_page);
        if step_index > 0 {
            current_page = predecessors[step_index]
                .get(&current_page)
                .copied()
                .flatten()
                .ok_or(MorRouteSequenceError::InternalInvariant)?;
        }
    }

    Ok(path)
}

fn validate_step(
    step: &MorRouteSequenceStep,
    sequence_index: usize,
    step_index: usize,
) -> Result<(), MorRouteSequenceError> {
    if step.outcomes.is_empty() {
        return Err(MorRouteSequenceError::EmptyOutcomes {
            sequence_index,
            step_index,
        });
    }
    let mut pages = BTreeSet::new();
    for outcome in step.outcomes.iter().copied() {
        for value in [
            outcome.task_loss_ppm,
            outcome.compute_cost_ppm,
            outcome.io_cost_ppm,
        ] {
            if value > MOR_SCALE_PPM {
                return Err(MorRouteSequenceError::OutcomePpmOutOfRange {
                    sequence_index,
                    step_index,
                    page_id: outcome.page_id,
                    value,
                });
            }
        }
        if !pages.insert(outcome.page_id.0) {
            return Err(MorRouteSequenceError::DuplicateOutcome {
                sequence_index,
                step_index,
                page_id: outcome.page_id,
            });
        }
    }
    Ok(())
}

fn outcome_for(step: &MorRouteSequenceStep, page_id: PageId) -> Option<MorExpertOutcome> {
    step.outcomes
        .iter()
        .copied()
        .find(|outcome| outcome.page_id == page_id)
}

fn outcome_cost(
    outcome: MorExpertOutcome,
    policy: MorRouteObjectivePolicy,
) -> Result<u32, MorRouteSequenceError> {
    let weighted = policy.task_loss_weight_ppm as u128 * outcome.task_loss_ppm as u128
        + policy.compute_cost_weight_ppm as u128 * outcome.compute_cost_ppm as u128
        + policy.io_cost_weight_ppm as u128 * outcome.io_cost_ppm as u128;
    let value = weighted / MOR_SCALE_PPM as u128;
    u32::try_from(value).map_err(|_| MorRouteSequenceError::ArithmeticOverflow)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorRouteSequenceError {
    StepObjective(crate::mor_route_objective::MorRouteObjectiveError),
    SwitchPenaltyOutOfRange(u32),
    EmptySequenceBatch,
    EmptySequence(usize),
    EmptyOutcomes {
        sequence_index: usize,
        step_index: usize,
    },
    DuplicateOutcome {
        sequence_index: usize,
        step_index: usize,
        page_id: PageId,
    },
    OutcomePpmOutOfRange {
        sequence_index: usize,
        step_index: usize,
        page_id: PageId,
        value: u32,
    },
    EmptyDecision {
        sequence_index: usize,
        step_index: usize,
    },
    MissingChosenOutcome {
        sequence_index: usize,
        step_index: usize,
        page_id: PageId,
    },
    ChosenPathLength {
        sequence_index: usize,
        expected: usize,
        actual: usize,
    },
    ArithmeticOverflow,
    InternalInvariant,
    Affinity(MorAffinityError),
}

impl fmt::Display for MorRouteSequenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorRouteSequenceError {}
