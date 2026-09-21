//! Fully native SML Mixture-of-Routers runtime composition.
//!
//! This runtime connects the already-qualified SML-local mechanisms:
//! state fingerprinting and affinity, router specialization validation,
//! stateful balancing, and temporal denoising. Callers provide an SML Boolean
//! state and optional runtime hints; they no longer assemble RouterInput values.

use core::fmt;
use std::collections::BTreeSet;

use crate::{
    mor::{ExpertRuntimeHint, MixtureOfRouters, RouterInput, MOR_SCALE_PPM},
    mor_affinity::{
        MorAffinityError, NativeAffinityAccounting, NativeAffinityBank, NativeAffinityBatch,
    },
    mor_controller::MorControllerPolicy,
    mor_registry::{MorRegistryAccounting, MorRegistryError, MorRouterRegistry},
    mor_route_objective::{MorExpertOutcome, MorRouteObjectiveError, MorRouteObjectivePolicy},
    mor_temporal::{
        MorTemporalPolicy, MorTemporalTransition, TemporalMorController,
        TemporalMorControllerError, TemporalMorCycleReport,
    },
    BooleanWorkspace, PageId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMorRuntimeAccounting {
    pub affinity: NativeAffinityAccounting,
    pub registry: MorRegistryAccounting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorRuntimeReport {
    pub affinity: NativeAffinityBatch,
    pub cycle: TemporalMorCycleReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorRuntime {
    engine: MixtureOfRouters,
    affinity_bank: NativeAffinityBank,
    registry: MorRouterRegistry,
    controller: TemporalMorController,
}

impl NativeMorRuntime {
    pub fn new(
        engine: MixtureOfRouters,
        affinity_bank: NativeAffinityBank,
        registry: MorRouterRegistry,
        controller_policy: MorControllerPolicy,
        temporal_policy: MorTemporalPolicy,
    ) -> Result<Self, NativeMorRuntimeError> {
        validate_static_compatibility(&affinity_bank, &registry)?;

        let controller = TemporalMorController::new(controller_policy, temporal_policy)
            .map_err(NativeMorRuntimeError::Temporal)?;

        Ok(Self {
            engine,
            affinity_bank,
            registry,
            controller,
        })
    }

    #[must_use]
    pub fn affinity_bank(&self) -> &NativeAffinityBank {
        &self.affinity_bank
    }

    #[must_use]
    pub fn registry(&self) -> &MorRouterRegistry {
        &self.registry
    }

    #[must_use]
    pub fn controller(&self) -> &TemporalMorController {
        &self.controller
    }

    #[must_use]
    pub fn accounting(&self) -> NativeMorRuntimeAccounting {
        NativeMorRuntimeAccounting {
            affinity: self.affinity_bank.accounting(),
            registry: self.registry.accounting(),
        }
    }

    pub fn reset(&mut self) {
        self.controller.reset();
    }

    pub fn route_state(
        &mut self,
        state: &BooleanWorkspace,
        runtime_hints: &[ExpertRuntimeHint],
    ) -> Result<NativeMorRuntimeReport, NativeMorRuntimeError> {
        let affinity = self
            .affinity_bank
            .materialize(state)
            .map_err(NativeMorRuntimeError::Affinity)?;

        self.registry
            .validate_complete_inputs(&affinity.routers, runtime_hints)
            .map_err(NativeMorRuntimeError::Registry)?;

        let cycle = self
            .controller
            .route_cycle(&self.engine, &affinity.routers, runtime_hints)
            .map_err(NativeMorRuntimeError::Temporal)?;

        Ok(NativeMorRuntimeReport { affinity, cycle })
    }
}

fn validate_static_compatibility(
    affinity_bank: &NativeAffinityBank,
    registry: &MorRouterRegistry,
) -> Result<(), NativeMorRuntimeError> {
    let routers = affinity_bank
        .routers()
        .iter()
        .map(|router| RouterInput {
            router_id: router.router_id,
            affinity_ppm: 0,
            reliability_ppm: router.reliability_ppm,
            confidence_ppm: router.confidence_ppm,
            balance_bias_ppm: router.balance_bias_ppm,
            proposals: router.proposals.clone(),
        })
        .collect::<Vec<_>>();

    registry
        .validate_complete_inputs(&routers, &[])
        .map_err(NativeMorRuntimeError::Registry)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMorRuntimeError {
    Affinity(MorAffinityError),
    Registry(MorRegistryError),
    Temporal(TemporalMorControllerError),
}

impl fmt::Display for NativeMorRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for NativeMorRuntimeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMorRuntimeAuditEvidence {
    pub cycle: u64,
    pub chosen_page: PageId,
    pub oracle_page: PageId,
    pub chosen_objective_ppm: u32,
    pub oracle_objective_ppm: u32,
    pub regret_ppm: u32,
    pub zero_regret: bool,
    pub route_switch: bool,
    pub accepted_temporal_switch: bool,
    pub temporal_transition: MorTemporalTransition,
    pub precision_escalation_requested: bool,
    pub noise_ppm: u32,
    pub structural_disagreement_ppm: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NativeMorRuntimeAuditSnapshot {
    pub cycles: u64,
    pub total_chosen_objective_ppm: u64,
    pub total_oracle_objective_ppm: u64,
    pub total_regret_ppm: u64,
    pub mean_regret_ppm: u32,
    pub max_regret_ppm: u32,
    pub zero_regret_cycles: u64,
    pub zero_regret_fraction_ppm: u32,
    pub route_switches: u64,
    pub accepted_temporal_switches: u64,
    pub held_cycles: u64,
    pub urgent_cycles: u64,
    pub precision_escalation_requests: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorRuntimeAuditor {
    objective_policy: MorRouteObjectivePolicy,
    snapshot: NativeMorRuntimeAuditSnapshot,
}

impl NativeMorRuntimeAuditor {
    pub fn new(
        objective_policy: MorRouteObjectivePolicy,
    ) -> Result<Self, NativeMorRuntimeAuditError> {
        objective_policy
            .validate()
            .map_err(NativeMorRuntimeAuditError::ObjectivePolicy)?;
        Ok(Self {
            objective_policy,
            snapshot: NativeMorRuntimeAuditSnapshot::default(),
        })
    }

    #[must_use]
    pub const fn objective_policy(&self) -> MorRouteObjectivePolicy {
        self.objective_policy
    }

    #[must_use]
    pub const fn snapshot(&self) -> NativeMorRuntimeAuditSnapshot {
        self.snapshot
    }

    pub fn reset(&mut self) {
        self.snapshot = NativeMorRuntimeAuditSnapshot::default();
    }

    pub fn observe(
        &mut self,
        report: &NativeMorRuntimeReport,
        outcomes: &[MorExpertOutcome],
    ) -> Result<NativeMorRuntimeAuditEvidence, NativeMorRuntimeAuditError> {
        validate_runtime_outcomes(outcomes)?;

        let chosen_page = report
            .cycle
            .cycle
            .decision
            .ranked_experts
            .first()
            .map(|expert| expert.page_id)
            .ok_or(NativeMorRuntimeAuditError::EmptyDecision)?;

        let chosen_outcome = outcomes
            .iter()
            .copied()
            .find(|outcome| outcome.page_id == chosen_page)
            .ok_or(NativeMorRuntimeAuditError::MissingChosenOutcome(
                chosen_page,
            ))?;
        let chosen_objective_ppm = runtime_outcome_cost(self.objective_policy, chosen_outcome)?;

        let mut oracle: Option<(u32, PageId)> = None;
        for outcome in outcomes.iter().copied() {
            let cost = runtime_outcome_cost(self.objective_policy, outcome)?;
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
            oracle.ok_or(NativeMorRuntimeAuditError::EmptyOutcomes)?;
        let regret_ppm = chosen_objective_ppm.saturating_sub(oracle_objective_ppm);
        let zero_regret = regret_ppm == 0;

        self.snapshot.cycles = self
            .snapshot
            .cycles
            .checked_add(1)
            .ok_or(NativeMorRuntimeAuditError::ArithmeticOverflow)?;
        self.snapshot.total_chosen_objective_ppm = self
            .snapshot
            .total_chosen_objective_ppm
            .checked_add(chosen_objective_ppm as u64)
            .ok_or(NativeMorRuntimeAuditError::ArithmeticOverflow)?;
        self.snapshot.total_oracle_objective_ppm = self
            .snapshot
            .total_oracle_objective_ppm
            .checked_add(oracle_objective_ppm as u64)
            .ok_or(NativeMorRuntimeAuditError::ArithmeticOverflow)?;
        self.snapshot.total_regret_ppm = self
            .snapshot
            .total_regret_ppm
            .checked_add(regret_ppm as u64)
            .ok_or(NativeMorRuntimeAuditError::ArithmeticOverflow)?;
        self.snapshot.max_regret_ppm = self.snapshot.max_regret_ppm.max(regret_ppm);

        if zero_regret {
            self.snapshot.zero_regret_cycles = self.snapshot.zero_regret_cycles.saturating_add(1);
        }
        if report.cycle.cycle.metrics.route_switch {
            self.snapshot.route_switches = self.snapshot.route_switches.saturating_add(1);
        }
        if report.cycle.temporal.accepted_route_switch {
            self.snapshot.accepted_temporal_switches =
                self.snapshot.accepted_temporal_switches.saturating_add(1);
        }
        match report.cycle.temporal.transition {
            MorTemporalTransition::Held => {
                self.snapshot.held_cycles = self.snapshot.held_cycles.saturating_add(1);
            }
            MorTemporalTransition::Urgent => {
                self.snapshot.urgent_cycles = self.snapshot.urgent_cycles.saturating_add(1);
            }
            MorTemporalTransition::Initialized
            | MorTemporalTransition::Stable
            | MorTemporalTransition::Confirmed => {}
        }
        if report.cycle.cycle.metrics.precision_escalation_requested {
            self.snapshot.precision_escalation_requests = self
                .snapshot
                .precision_escalation_requests
                .saturating_add(1);
        }

        self.snapshot.mean_regret_ppm =
            (self.snapshot.total_regret_ppm / self.snapshot.cycles) as u32;
        self.snapshot.zero_regret_fraction_ppm =
            fraction_ppm_u64(self.snapshot.zero_regret_cycles, self.snapshot.cycles);

        Ok(NativeMorRuntimeAuditEvidence {
            cycle: report.cycle.cycle.metrics.cycle,
            chosen_page,
            oracle_page,
            chosen_objective_ppm,
            oracle_objective_ppm,
            regret_ppm,
            zero_regret,
            route_switch: report.cycle.cycle.metrics.route_switch,
            accepted_temporal_switch: report.cycle.temporal.accepted_route_switch,
            temporal_transition: report.cycle.temporal.transition,
            precision_escalation_requested: report
                .cycle
                .cycle
                .metrics
                .precision_escalation_requested,
            noise_ppm: report.cycle.cycle.metrics.noise_ppm,
            structural_disagreement_ppm: report.cycle.cycle.metrics.structural_disagreement_ppm,
        })
    }
}

fn validate_runtime_outcomes(
    outcomes: &[MorExpertOutcome],
) -> Result<(), NativeMorRuntimeAuditError> {
    if outcomes.is_empty() {
        return Err(NativeMorRuntimeAuditError::EmptyOutcomes);
    }

    let mut pages = BTreeSet::new();
    for outcome in outcomes.iter().copied() {
        if !pages.insert(outcome.page_id.0) {
            return Err(NativeMorRuntimeAuditError::DuplicateOutcome(
                outcome.page_id,
            ));
        }
        for value in [
            outcome.task_loss_ppm,
            outcome.compute_cost_ppm,
            outcome.io_cost_ppm,
        ] {
            if value > MOR_SCALE_PPM {
                return Err(NativeMorRuntimeAuditError::OutcomePpmOutOfRange {
                    page_id: outcome.page_id,
                    value,
                });
            }
        }
    }

    Ok(())
}

fn runtime_outcome_cost(
    policy: MorRouteObjectivePolicy,
    outcome: MorExpertOutcome,
) -> Result<u32, NativeMorRuntimeAuditError> {
    let weighted = policy.task_loss_weight_ppm as u128 * outcome.task_loss_ppm as u128
        + policy.compute_cost_weight_ppm as u128 * outcome.compute_cost_ppm as u128
        + policy.io_cost_weight_ppm as u128 * outcome.io_cost_ppm as u128;
    let value = weighted / MOR_SCALE_PPM as u128;
    u32::try_from(value).map_err(|_| NativeMorRuntimeAuditError::ArithmeticOverflow)
}

fn fraction_ppm_u64(numerator: u64, denominator: u64) -> u32 {
    if denominator == 0 {
        return 0;
    }
    ((numerator as u128 * MOR_SCALE_PPM as u128) / denominator as u128) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMorRuntimeAuditError {
    ObjectivePolicy(MorRouteObjectiveError),
    EmptyOutcomes,
    DuplicateOutcome(PageId),
    OutcomePpmOutOfRange { page_id: PageId, value: u32 },
    EmptyDecision,
    MissingChosenOutcome(PageId),
    ArithmeticOverflow,
}

impl fmt::Display for NativeMorRuntimeAuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for NativeMorRuntimeAuditError {}
