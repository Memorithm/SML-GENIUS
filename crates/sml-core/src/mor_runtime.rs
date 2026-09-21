//! Fully native SML Mixture-of-Routers runtime composition.
//!
//! This runtime connects the already-qualified SML-local mechanisms:
//! state fingerprinting and affinity, router specialization validation,
//! stateful balancing, and temporal denoising. Callers provide an SML Boolean
//! state and optional runtime hints; they no longer assemble RouterInput values.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    mor::{ExpertRuntimeHint, MixtureOfRouters, PreviousRoute, RouterInput, MOR_SCALE_PPM},
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
    /// Route committed by temporal denoising for execution/recurrent continuity.
    pub committed_route: PreviousRoute,
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

        let committed_route = self.controller.denoiser().stable_route().clone();
        Ok(NativeMorRuntimeReport {
            affinity,
            cycle,
            committed_route,
        })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMorOnlineAdaptationPolicy {
    pub ema_retention_ppm: u32,
    pub objective_gain_ppm: u32,
    pub max_bias_adjustment_ppm: u32,
}

impl Default for NativeMorOnlineAdaptationPolicy {
    fn default() -> Self {
        Self {
            ema_retention_ppm: 900_000,
            objective_gain_ppm: 500_000,
            max_bias_adjustment_ppm: 150_000,
        }
    }
}

impl NativeMorOnlineAdaptationPolicy {
    pub fn validate(self) -> Result<Self, NativeMorOnlineAdaptationError> {
        for value in [
            self.ema_retention_ppm,
            self.objective_gain_ppm,
            self.max_bias_adjustment_ppm,
        ] {
            if value > MOR_SCALE_PPM {
                return Err(NativeMorOnlineAdaptationError::PolicyPpmOutOfRange(value));
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMorPageValueEvidence {
    pub page_id: PageId,
    pub observations: u64,
    pub ema_objective_ppm: u32,
    pub adaptive_bias_ppm: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorAdaptiveRuntimeReport {
    pub runtime: NativeMorRuntimeReport,
    pub effective_hints: Vec<ExpertRuntimeHint>,
    pub values_before_route: Vec<NativeMorPageValueEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeMorPageValueState {
    observations: u64,
    ema_objective_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorAdaptiveRuntime {
    runtime: NativeMorRuntime,
    auditor: NativeMorRuntimeAuditor,
    policy: NativeMorOnlineAdaptationPolicy,
    page_values: BTreeMap<u32, NativeMorPageValueState>,
}

impl NativeMorAdaptiveRuntime {
    pub fn new(
        runtime: NativeMorRuntime,
        objective_policy: MorRouteObjectivePolicy,
        adaptation_policy: NativeMorOnlineAdaptationPolicy,
    ) -> Result<Self, NativeMorOnlineAdaptationError> {
        let policy = adaptation_policy.validate()?;
        let auditor = NativeMorRuntimeAuditor::new(objective_policy)
            .map_err(NativeMorOnlineAdaptationError::Audit)?;
        Ok(Self {
            runtime,
            auditor,
            policy,
            page_values: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn runtime(&self) -> &NativeMorRuntime {
        &self.runtime
    }

    #[must_use]
    pub fn auditor(&self) -> &NativeMorRuntimeAuditor {
        &self.auditor
    }

    #[must_use]
    pub const fn policy(&self) -> NativeMorOnlineAdaptationPolicy {
        self.policy
    }

    #[must_use]
    pub fn value_evidence(&self) -> Vec<NativeMorPageValueEvidence> {
        let mean = mean_page_objective(&self.page_values);
        self.page_values
            .iter()
            .map(|(page, state)| NativeMorPageValueEvidence {
                page_id: PageId(*page),
                observations: state.observations,
                ema_objective_ppm: state.ema_objective_ppm,
                adaptive_bias_ppm: adaptive_objective_bias(
                    mean,
                    state.ema_objective_ppm,
                    self.policy,
                ),
            })
            .collect()
    }

    pub fn reset(&mut self) {
        self.runtime.reset();
        self.auditor.reset();
        self.page_values.clear();
    }

    pub fn route_state(
        &mut self,
        state: &BooleanWorkspace,
        caller_hints: &[ExpertRuntimeHint],
    ) -> Result<NativeMorAdaptiveRuntimeReport, NativeMorOnlineAdaptationError> {
        let values_before_route = self.value_evidence();
        let effective_hints = self.effective_hints(caller_hints, &values_before_route)?;
        let runtime = self
            .runtime
            .route_state(state, &effective_hints)
            .map_err(NativeMorOnlineAdaptationError::Runtime)?;

        Ok(NativeMorAdaptiveRuntimeReport {
            runtime,
            effective_hints,
            values_before_route,
        })
    }

    pub fn observe(
        &mut self,
        report: &NativeMorAdaptiveRuntimeReport,
        outcomes: &[MorExpertOutcome],
    ) -> Result<NativeMorRuntimeAuditEvidence, NativeMorOnlineAdaptationError> {
        let expert_count = self.runtime.registry().accounting().expert_count;
        for outcome in outcomes {
            if outcome.page_id.0 >= expert_count {
                return Err(NativeMorOnlineAdaptationError::OutcomeOutOfRegistry {
                    page_id: outcome.page_id,
                    expert_count,
                });
            }
        }

        let evidence = self
            .auditor
            .observe(&report.runtime, outcomes)
            .map_err(NativeMorOnlineAdaptationError::Audit)?;

        for outcome in outcomes.iter().copied() {
            let objective = runtime_outcome_cost(self.auditor.objective_policy(), outcome)
                .map_err(NativeMorOnlineAdaptationError::Audit)?;
            match self.page_values.get_mut(&outcome.page_id.0) {
                Some(state) => {
                    state.ema_objective_ppm = ema_objective_ppm(
                        state.ema_objective_ppm,
                        objective,
                        self.policy.ema_retention_ppm,
                    );
                    state.observations = state.observations.saturating_add(1);
                }
                None => {
                    self.page_values.insert(
                        outcome.page_id.0,
                        NativeMorPageValueState {
                            observations: 1,
                            ema_objective_ppm: objective,
                        },
                    );
                }
            }
        }

        Ok(evidence)
    }

    fn effective_hints(
        &self,
        caller_hints: &[ExpertRuntimeHint],
        values: &[NativeMorPageValueEvidence],
    ) -> Result<Vec<ExpertRuntimeHint>, NativeMorOnlineAdaptationError> {
        let mut hints = BTreeMap::new();
        for hint in caller_hints {
            if hints.insert(hint.page_id.0, *hint).is_some() {
                return Err(NativeMorOnlineAdaptationError::DuplicateCallerHint(
                    hint.page_id,
                ));
            }
        }

        for value in values {
            let base = hints
                .get(&value.page_id.0)
                .copied()
                .unwrap_or(ExpertRuntimeHint {
                    page_id: value.page_id,
                    balance_bias_ppm: 0,
                    residency_penalty_ppm: 0,
                });
            hints.insert(
                value.page_id.0,
                ExpertRuntimeHint {
                    page_id: value.page_id,
                    balance_bias_ppm: bounded_runtime_bias_add(
                        base.balance_bias_ppm,
                        value.adaptive_bias_ppm,
                    ),
                    residency_penalty_ppm: base.residency_penalty_ppm,
                },
            );
        }

        Ok(hints.into_values().collect())
    }
}

fn mean_page_objective(values: &BTreeMap<u32, NativeMorPageValueState>) -> u32 {
    if values.is_empty() {
        return 0;
    }
    let sum = values
        .values()
        .map(|state| state.ema_objective_ppm as u64)
        .sum::<u64>();
    (sum / values.len() as u64) as u32
}

fn adaptive_objective_bias(
    mean_objective_ppm: u32,
    page_objective_ppm: u32,
    policy: NativeMorOnlineAdaptationPolicy,
) -> i32 {
    let delta = mean_objective_ppm as i64 - page_objective_ppm as i64;
    let scaled = delta * policy.objective_gain_ppm as i64 / MOR_SCALE_PPM as i64;
    scaled.clamp(
        -(policy.max_bias_adjustment_ppm as i64),
        policy.max_bias_adjustment_ppm as i64,
    ) as i32
}

fn ema_objective_ppm(old: u32, observed: u32, retention_ppm: u32) -> u32 {
    let retained = old as u64 * retention_ppm as u64;
    let fresh = observed as u64 * (MOR_SCALE_PPM - retention_ppm) as u64;
    ((retained + fresh) / MOR_SCALE_PPM as u64) as u32
}

fn bounded_runtime_bias_add(base: i32, adjustment: i32) -> i32 {
    (base as i64 + adjustment as i64).clamp(-(MOR_SCALE_PPM as i64), MOR_SCALE_PPM as i64) as i32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMorOnlineAdaptationError {
    PolicyPpmOutOfRange(u32),
    DuplicateCallerHint(PageId),
    OutcomeOutOfRegistry { page_id: PageId, expert_count: u32 },
    Runtime(NativeMorRuntimeError),
    Audit(NativeMorRuntimeAuditError),
}

impl fmt::Display for NativeMorOnlineAdaptationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for NativeMorOnlineAdaptationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMorOnlineAdaptationPolicy {
    pub ema_retention_ppm: u32,
    pub objective_gain_ppm: u32,
    pub max_bias_adjustment_ppm: u32,
}

impl Default for NativeMorOnlineAdaptationPolicy {
    fn default() -> Self {
        Self {
            ema_retention_ppm: 900_000,
            objective_gain_ppm: 500_000,
            max_bias_adjustment_ppm: 150_000,
        }
    }
}

impl NativeMorOnlineAdaptationPolicy {
    pub fn validate(self) -> Result<Self, NativeMorOnlineAdaptationError> {
        for value in [
            self.ema_retention_ppm,
            self.objective_gain_ppm,
            self.max_bias_adjustment_ppm,
        ] {
            if value > MOR_SCALE_PPM {
                return Err(NativeMorOnlineAdaptationError::PolicyPpmOutOfRange(value));
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMorPageValueEvidence {
    pub page_id: PageId,
    pub observations: u64,
    pub ema_objective_ppm: u32,
    pub adaptive_bias_ppm: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorAdaptiveRuntimeReport {
    pub runtime: NativeMorRuntimeReport,
    pub effective_hints: Vec<ExpertRuntimeHint>,
    pub values_before_route: Vec<NativeMorPageValueEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeMorPageValueState {
    observations: u64,
    ema_objective_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorAdaptiveRuntime {
    runtime: NativeMorRuntime,
    auditor: NativeMorRuntimeAuditor,
    policy: NativeMorOnlineAdaptationPolicy,
    page_values: BTreeMap<u32, NativeMorPageValueState>,
}

impl NativeMorAdaptiveRuntime {
    pub fn new(
        runtime: NativeMorRuntime,
        objective_policy: MorRouteObjectivePolicy,
        adaptation_policy: NativeMorOnlineAdaptationPolicy,
    ) -> Result<Self, NativeMorOnlineAdaptationError> {
        let policy = adaptation_policy.validate()?;
        let auditor = NativeMorRuntimeAuditor::new(objective_policy)
            .map_err(NativeMorOnlineAdaptationError::Audit)?;
        Ok(Self {
            runtime,
            auditor,
            policy,
            page_values: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn runtime(&self) -> &NativeMorRuntime {
        &self.runtime
    }

    #[must_use]
    pub fn auditor(&self) -> &NativeMorRuntimeAuditor {
        &self.auditor
    }

    #[must_use]
    pub const fn policy(&self) -> NativeMorOnlineAdaptationPolicy {
        self.policy
    }

    #[must_use]
    pub fn value_evidence(&self) -> Vec<NativeMorPageValueEvidence> {
        let mean = mean_page_objective(&self.page_values);
        self.page_values
            .iter()
            .map(|(page, state)| NativeMorPageValueEvidence {
                page_id: PageId(*page),
                observations: state.observations,
                ema_objective_ppm: state.ema_objective_ppm,
                adaptive_bias_ppm: adaptive_objective_bias(
                    mean,
                    state.ema_objective_ppm,
                    self.policy,
                ),
            })
            .collect()
    }

    pub fn reset(&mut self) {
        self.runtime.reset();
        self.auditor.reset();
        self.page_values.clear();
    }

    pub fn route_state(
        &mut self,
        state: &BooleanWorkspace,
        caller_hints: &[ExpertRuntimeHint],
    ) -> Result<NativeMorAdaptiveRuntimeReport, NativeMorOnlineAdaptationError> {
        let values_before_route = self.value_evidence();
        let effective_hints = self.effective_hints(caller_hints, &values_before_route)?;
        let runtime = self
            .runtime
            .route_state(state, &effective_hints)
            .map_err(NativeMorOnlineAdaptationError::Runtime)?;

        Ok(NativeMorAdaptiveRuntimeReport {
            runtime,
            effective_hints,
            values_before_route,
        })
    }

    pub fn observe(
        &mut self,
        report: &NativeMorAdaptiveRuntimeReport,
        outcomes: &[MorExpertOutcome],
    ) -> Result<NativeMorRuntimeAuditEvidence, NativeMorOnlineAdaptationError> {
        let expert_count = self.runtime.registry().accounting().expert_count;
        for outcome in outcomes {
            if outcome.page_id.0 >= expert_count {
                return Err(NativeMorOnlineAdaptationError::OutcomeOutOfRegistry {
                    page_id: outcome.page_id,
                    expert_count,
                });
            }
        }

        let evidence = self
            .auditor
            .observe(&report.runtime, outcomes)
            .map_err(NativeMorOnlineAdaptationError::Audit)?;

        for outcome in outcomes.iter().copied() {
            let objective = runtime_outcome_cost(self.auditor.objective_policy(), outcome)
                .map_err(NativeMorOnlineAdaptationError::Audit)?;
            match self.page_values.get_mut(&outcome.page_id.0) {
                Some(state) => {
                    state.ema_objective_ppm = ema_objective_ppm(
                        state.ema_objective_ppm,
                        objective,
                        self.policy.ema_retention_ppm,
                    );
                    state.observations = state.observations.saturating_add(1);
                }
                None => {
                    self.page_values.insert(
                        outcome.page_id.0,
                        NativeMorPageValueState {
                            observations: 1,
                            ema_objective_ppm: objective,
                        },
                    );
                }
            }
        }

        Ok(evidence)
    }

    fn effective_hints(
        &self,
        caller_hints: &[ExpertRuntimeHint],
        values: &[NativeMorPageValueEvidence],
    ) -> Result<Vec<ExpertRuntimeHint>, NativeMorOnlineAdaptationError> {
        let mut hints = BTreeMap::new();
        for hint in caller_hints {
            if hints.insert(hint.page_id.0, *hint).is_some() {
                return Err(NativeMorOnlineAdaptationError::DuplicateCallerHint(
                    hint.page_id,
                ));
            }
        }

        for value in values {
            let base = hints
                .get(&value.page_id.0)
                .copied()
                .unwrap_or(ExpertRuntimeHint {
                    page_id: value.page_id,
                    balance_bias_ppm: 0,
                    residency_penalty_ppm: 0,
                });
            hints.insert(
                value.page_id.0,
                ExpertRuntimeHint {
                    page_id: value.page_id,
                    balance_bias_ppm: bounded_runtime_bias_add(
                        base.balance_bias_ppm,
                        value.adaptive_bias_ppm,
                    ),
                    residency_penalty_ppm: base.residency_penalty_ppm,
                },
            );
        }

        Ok(hints.into_values().collect())
    }
}

fn mean_page_objective(values: &BTreeMap<u32, NativeMorPageValueState>) -> u32 {
    if values.is_empty() {
        return 0;
    }
    let sum = values
        .values()
        .map(|state| state.ema_objective_ppm as u64)
        .sum::<u64>();
    (sum / values.len() as u64) as u32
}

fn adaptive_objective_bias(
    mean_objective_ppm: u32,
    page_objective_ppm: u32,
    policy: NativeMorOnlineAdaptationPolicy,
) -> i32 {
    let delta = mean_objective_ppm as i64 - page_objective_ppm as i64;
    let scaled = delta * policy.objective_gain_ppm as i64 / MOR_SCALE_PPM as i64;
    scaled.clamp(
        -(policy.max_bias_adjustment_ppm as i64),
        policy.max_bias_adjustment_ppm as i64,
    ) as i32
}

fn ema_objective_ppm(old: u32, observed: u32, retention_ppm: u32) -> u32 {
    let retained = old as u64 * retention_ppm as u64;
    let fresh = observed as u64 * (MOR_SCALE_PPM - retention_ppm) as u64;
    ((retained + fresh) / MOR_SCALE_PPM as u64) as u32
}

fn bounded_runtime_bias_add(base: i32, adjustment: i32) -> i32 {
    (base as i64 + adjustment as i64).clamp(-(MOR_SCALE_PPM as i64), MOR_SCALE_PPM as i64) as i32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMorOnlineAdaptationError {
    PolicyPpmOutOfRange(u32),
    DuplicateCallerHint(PageId),
    OutcomeOutOfRegistry { page_id: PageId, expert_count: u32 },
    Runtime(NativeMorRuntimeError),
    Audit(NativeMorRuntimeAuditError),
}

impl fmt::Display for NativeMorOnlineAdaptationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for NativeMorOnlineAdaptationError {}

