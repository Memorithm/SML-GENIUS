//! Stateful native control loop for SML Mixture-of-Routers.
//!
//! The controller owns only SML-local state: previous routes, exponentially
//! decayed router/expert loads, bounded balancing biases, and deterministic
//! per-cycle telemetry. It has no dependency on an external runtime.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    mor::{
        ExpertRuntimeHint, MixtureOfRouters, MorDecision, MorError, PreviousRoute, RouterId,
        RouterInput, MOR_SCALE_PPM, MOR_SCORE_CEILING_PPM,
    },
    PageId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorControllerPolicy {
    pub load_ema_retention_ppm: u32,
    pub router_balance_gain_ppm: u32,
    pub expert_balance_gain_ppm: u32,
    pub max_router_adjustment_ppm: u32,
    pub max_expert_adjustment_ppm: u32,
}

impl Default for MorControllerPolicy {
    fn default() -> Self {
        Self {
            load_ema_retention_ppm: 900_000,
            router_balance_gain_ppm: 250_000,
            expert_balance_gain_ppm: 250_000,
            max_router_adjustment_ppm: 100_000,
            max_expert_adjustment_ppm: 100_000,
        }
    }
}

impl MorControllerPolicy {
    pub fn validate(self) -> Result<Self, MorControllerError> {
        for value in [
            self.load_ema_retention_ppm,
            self.router_balance_gain_ppm,
            self.expert_balance_gain_ppm,
            self.max_router_adjustment_ppm,
            self.max_expert_adjustment_ppm,
        ] {
            if value > MOR_SCALE_PPM {
                return Err(MorControllerError::PolicyPpmOutOfRange(value));
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouterBalanceEvidence {
    pub router_id: RouterId,
    pub historical_load_ppm: u32,
    pub adaptive_adjustment_ppm: i32,
    pub effective_balance_bias_ppm: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpertBalanceEvidence {
    pub page_id: PageId,
    pub historical_load_ppm: u32,
    pub adaptive_adjustment_ppm: i32,
    pub effective_balance_bias_ppm: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorCycleMetrics {
    pub cycle: u64,
    pub total_routers: usize,
    pub active_routers: usize,
    pub candidate_experts: usize,
    pub selected_experts: usize,
    pub active_router_fraction_ppm: u32,
    pub selected_expert_fraction_ppm: u32,
    pub route_margin_ppm: u32,
    pub noise_ppm: u32,
    pub structural_disagreement_ppm: u32,
    pub route_switch: bool,
    pub precision_escalation_requested: bool,
    pub router_load_spread_ppm: u32,
    pub expert_load_spread_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorCycleReport {
    pub decision: MorDecision,
    pub metrics: MorCycleMetrics,
    pub router_balance: Vec<RouterBalanceEvidence>,
    pub expert_balance: Vec<ExpertBalanceEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorController {
    policy: MorControllerPolicy,
    cycle: u64,
    previous: PreviousRoute,
    router_load_ppm: BTreeMap<RouterId, u32>,
    expert_load_ppm: BTreeMap<u32, u32>,
}

impl MorController {
    pub fn new(policy: MorControllerPolicy) -> Result<Self, MorControllerError> {
        Ok(Self {
            policy: policy.validate()?,
            cycle: 0,
            previous: PreviousRoute::default(),
            router_load_ppm: BTreeMap::new(),
            expert_load_ppm: BTreeMap::new(),
        })
    }

    #[must_use]
    pub const fn cycle(&self) -> u64 {
        self.cycle
    }

    #[must_use]
    pub const fn policy(&self) -> MorControllerPolicy {
        self.policy
    }

    #[must_use]
    pub fn previous_route(&self) -> &PreviousRoute {
        &self.previous
    }

    pub(crate) fn replace_previous_route(&mut self, previous: PreviousRoute) {
        self.previous = previous;
    }

    #[must_use]
    pub fn router_load_ppm(&self, router_id: RouterId) -> u32 {
        self.router_load_ppm.get(&router_id).copied().unwrap_or(0)
    }

    #[must_use]
    pub fn expert_load_ppm(&self, page_id: PageId) -> u32 {
        self.expert_load_ppm.get(&page_id.0).copied().unwrap_or(0)
    }

    pub fn reset(&mut self) {
        self.cycle = 0;
        self.previous = PreviousRoute::default();
        self.router_load_ppm.clear();
        self.expert_load_ppm.clear();
    }

    pub fn route_cycle(
        &mut self,
        engine: &MixtureOfRouters,
        routers: &[RouterInput],
        runtime_hints: &[ExpertRuntimeHint],
    ) -> Result<MorCycleReport, MorControllerError> {
        if self.cycle == u64::MAX {
            return Err(MorControllerError::CycleOverflow);
        }

        let router_ids = routers
            .iter()
            .map(|router| router.router_id)
            .collect::<BTreeSet<_>>();
        if router_ids.len() != routers.len() {
            return engine
                .route(routers, runtime_hints, &self.previous)
                .map(|_| unreachable!())
                .map_err(MorControllerError::Mor);
        }

        let candidate_pages = candidate_pages(routers);
        let hint_map = runtime_hint_map(runtime_hints)?;

        for router_id in &router_ids {
            self.router_load_ppm.entry(*router_id).or_insert(0);
        }
        for page in &candidate_pages {
            self.expert_load_ppm.entry(*page).or_insert(0);
        }

        let router_mean = mean_load_router(&self.router_load_ppm, &router_ids);
        let expert_mean = mean_load_expert(&self.expert_load_ppm, &candidate_pages);

        let mut adjusted_routers = Vec::with_capacity(routers.len());
        let mut router_balance = Vec::with_capacity(routers.len());
        for router in routers {
            let historical = self.router_load_ppm(router.router_id);
            let adaptive = adaptive_adjustment(
                router_mean,
                historical,
                self.policy.router_balance_gain_ppm,
                self.policy.max_router_adjustment_ppm,
            );
            let effective = bounded_bias_add(router.balance_bias_ppm, adaptive);
            let mut adjusted = router.clone();
            adjusted.balance_bias_ppm = effective;
            adjusted_routers.push(adjusted);
            router_balance.push(RouterBalanceEvidence {
                router_id: router.router_id,
                historical_load_ppm: historical,
                adaptive_adjustment_ppm: adaptive,
                effective_balance_bias_ppm: effective,
            });
        }

        let mut adjusted_hints = Vec::with_capacity(candidate_pages.len());
        let mut expert_balance = Vec::with_capacity(candidate_pages.len());
        for page in &candidate_pages {
            let page_id = PageId(*page);
            let base = hint_map.get(page).copied().unwrap_or(ExpertRuntimeHint {
                page_id,
                balance_bias_ppm: 0,
                residency_penalty_ppm: 0,
            });
            let historical = self.expert_load_ppm(page_id);
            let adaptive = adaptive_adjustment(
                expert_mean,
                historical,
                self.policy.expert_balance_gain_ppm,
                self.policy.max_expert_adjustment_ppm,
            );
            let effective = bounded_bias_add(base.balance_bias_ppm, adaptive);
            adjusted_hints.push(ExpertRuntimeHint {
                page_id,
                balance_bias_ppm: effective,
                residency_penalty_ppm: base.residency_penalty_ppm,
            });
            expert_balance.push(ExpertBalanceEvidence {
                page_id,
                historical_load_ppm: historical,
                adaptive_adjustment_ppm: adaptive,
                effective_balance_bias_ppm: effective,
            });
        }

        let old_primary = self.previous.primary;
        let decision = engine
            .route(&adjusted_routers, &adjusted_hints, &self.previous)
            .map_err(MorControllerError::Mor)?;
        let new_primary = decision.ranked_experts.first().map(|expert| expert.page_id);
        let route_switch =
            matches!((old_primary, new_primary), (Some(old), Some(new)) if old != new);

        update_loads(
            &mut self.router_load_ppm,
            router_ids.iter().copied(),
            decision
                .active_routers
                .iter()
                .map(|router| router.router_id)
                .collect(),
            self.policy.load_ema_retention_ppm,
        );
        update_expert_loads(
            &mut self.expert_load_ppm,
            candidate_pages.iter().copied(),
            decision
                .selected_experts
                .iter()
                .map(|page| page.0)
                .collect(),
            self.policy.load_ema_retention_ppm,
        );

        self.cycle += 1;
        self.previous = PreviousRoute {
            primary: new_primary,
            selected: decision.selected_experts.clone(),
        };

        let metrics = MorCycleMetrics {
            cycle: self.cycle,
            total_routers: routers.len(),
            active_routers: decision.active_routers.len(),
            candidate_experts: candidate_pages.len(),
            selected_experts: decision.selected_experts.len(),
            active_router_fraction_ppm: fraction_ppm(decision.active_routers.len(), routers.len()),
            selected_expert_fraction_ppm: fraction_ppm(
                decision.selected_experts.len(),
                candidate_pages.len(),
            ),
            route_margin_ppm: route_margin(&decision),
            noise_ppm: decision.noise_ppm,
            structural_disagreement_ppm: decision.structural_disagreement_ppm,
            route_switch,
            precision_escalation_requested: decision.request_precision_escalation,
            router_load_spread_ppm: load_spread_router(&self.router_load_ppm, &router_ids),
            expert_load_spread_ppm: load_spread_expert(&self.expert_load_ppm, &candidate_pages),
        };

        Ok(MorCycleReport {
            decision,
            metrics,
            router_balance,
            expert_balance,
        })
    }
}

fn candidate_pages(routers: &[RouterInput]) -> BTreeSet<u32> {
    routers
        .iter()
        .flat_map(|router| router.proposals.iter().map(|proposal| proposal.page_id.0))
        .collect()
}

fn runtime_hint_map(
    runtime_hints: &[ExpertRuntimeHint],
) -> Result<BTreeMap<u32, ExpertRuntimeHint>, MorControllerError> {
    let mut map = BTreeMap::new();
    for hint in runtime_hints {
        if map.insert(hint.page_id.0, *hint).is_some() {
            return Err(MorControllerError::Mor(MorError::DuplicateRuntimeHint(
                hint.page_id,
            )));
        }
    }
    Ok(map)
}

fn mean_load_router(loads: &BTreeMap<RouterId, u32>, ids: &BTreeSet<RouterId>) -> u32 {
    if ids.is_empty() {
        return 0;
    }
    let sum = ids
        .iter()
        .map(|id| loads.get(id).copied().unwrap_or(0) as u64)
        .sum::<u64>();
    (sum / ids.len() as u64) as u32
}

fn mean_load_expert(loads: &BTreeMap<u32, u32>, ids: &BTreeSet<u32>) -> u32 {
    if ids.is_empty() {
        return 0;
    }
    let sum = ids
        .iter()
        .map(|id| loads.get(id).copied().unwrap_or(0) as u64)
        .sum::<u64>();
    (sum / ids.len() as u64) as u32
}

fn adaptive_adjustment(mean: u32, load: u32, gain_ppm: u32, cap_ppm: u32) -> i32 {
    let delta = mean as i64 - load as i64;
    let scaled = delta * gain_ppm as i64 / MOR_SCALE_PPM as i64;
    scaled.clamp(-(cap_ppm as i64), cap_ppm as i64) as i32
}

fn bounded_bias_add(base: i32, adjustment: i32) -> i32 {
    (base as i64 + adjustment as i64).clamp(-(MOR_SCALE_PPM as i64), MOR_SCALE_PPM as i64) as i32
}

fn update_loads(
    loads: &mut BTreeMap<RouterId, u32>,
    ids: impl Iterator<Item = RouterId>,
    active: BTreeSet<RouterId>,
    retention_ppm: u32,
) {
    for id in ids {
        let old = loads.get(&id).copied().unwrap_or(0);
        let observed = if active.contains(&id) {
            MOR_SCALE_PPM
        } else {
            0
        };
        loads.insert(id, ema_ppm(old, observed, retention_ppm));
    }
}

fn update_expert_loads(
    loads: &mut BTreeMap<u32, u32>,
    ids: impl Iterator<Item = u32>,
    active: BTreeSet<u32>,
    retention_ppm: u32,
) {
    for id in ids {
        let old = loads.get(&id).copied().unwrap_or(0);
        let observed = if active.contains(&id) {
            MOR_SCALE_PPM
        } else {
            0
        };
        loads.insert(id, ema_ppm(old, observed, retention_ppm));
    }
}

fn ema_ppm(old: u32, observed: u32, retention_ppm: u32) -> u32 {
    let retained = old as u64 * retention_ppm as u64;
    let fresh = observed as u64 * (MOR_SCALE_PPM - retention_ppm) as u64;
    ((retained + fresh) / MOR_SCALE_PPM as u64) as u32
}

fn fraction_ppm(numerator: usize, denominator: usize) -> u32 {
    if denominator == 0 {
        return 0;
    }
    ((numerator as u128 * MOR_SCALE_PPM as u128) / denominator as u128) as u32
}

fn route_margin(decision: &MorDecision) -> u32 {
    match decision.ranked_experts.as_slice() {
        [first, second, ..] => first.final_score_ppm.saturating_sub(second.final_score_ppm),
        [_] => MOR_SCORE_CEILING_PPM,
        [] => 0,
    }
}

fn load_spread_router(loads: &BTreeMap<RouterId, u32>, ids: &BTreeSet<RouterId>) -> u32 {
    let values = ids
        .iter()
        .map(|id| loads.get(id).copied().unwrap_or(0))
        .collect::<Vec<_>>();
    load_spread(&values)
}

fn load_spread_expert(loads: &BTreeMap<u32, u32>, ids: &BTreeSet<u32>) -> u32 {
    let values = ids
        .iter()
        .map(|id| loads.get(id).copied().unwrap_or(0))
        .collect::<Vec<_>>();
    load_spread(&values)
}

fn load_spread(values: &[u32]) -> u32 {
    match (values.iter().min(), values.iter().max()) {
        (Some(minimum), Some(maximum)) => maximum.saturating_sub(*minimum),
        _ => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorControllerError {
    PolicyPpmOutOfRange(u32),
    CycleOverflow,
    Mor(MorError),
}

impl fmt::Display for MorControllerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorControllerError {}
