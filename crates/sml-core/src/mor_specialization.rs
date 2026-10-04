//! Specialization-preserving expert balancing for native SML MOR.
//!
//! MOR-H3 separates balancing across semantic expert clusters from balancing
//! among experts inside the same cluster. Expert-load EMAs are conditional on
//! actual candidate exposure, and newly exposed clusters/experts are initialized
//! neutrally instead of being treated as unused at load zero.
//!
//! The native MixtureOfRouters remains the final routing engine. This module
//! supplies bounded expert runtime-hint adjustments and auditable state.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    mor::{
        ExpertRuntimeHint, MixtureOfRouters, MorDecision, MorError, PreviousRoute, RouterInput,
        MOR_SCALE_PPM,
    },
    PageId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MorClusterId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorSpecializationCluster {
    pub cluster_id: MorClusterId,
    pub experts: Vec<PageId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorSpecializationRegistry {
    clusters: Vec<MorSpecializationCluster>,
    expert_to_cluster: BTreeMap<u32, MorClusterId>,
}

impl MorSpecializationRegistry {
    pub fn new(clusters: Vec<MorSpecializationCluster>) -> Result<Self, MorSpecializationError> {
        if clusters.is_empty() {
            return Err(MorSpecializationError::EmptyRegistry);
        }

        let mut cluster_ids = BTreeSet::new();
        let mut expert_to_cluster = BTreeMap::new();
        for cluster in &clusters {
            if !cluster_ids.insert(cluster.cluster_id) {
                return Err(MorSpecializationError::DuplicateCluster(cluster.cluster_id));
            }
            if cluster.experts.is_empty() {
                return Err(MorSpecializationError::EmptyCluster(cluster.cluster_id));
            }

            let mut local = BTreeSet::new();
            for page in &cluster.experts {
                if !local.insert(page.0) {
                    return Err(MorSpecializationError::DuplicateClusterExpert {
                        cluster_id: cluster.cluster_id,
                        page_id: *page,
                    });
                }
                if let Some(existing) = expert_to_cluster.insert(page.0, cluster.cluster_id) {
                    return Err(MorSpecializationError::ExpertInMultipleClusters {
                        page_id: *page,
                        first: existing,
                        second: cluster.cluster_id,
                    });
                }
            }
        }

        Ok(Self {
            clusters,
            expert_to_cluster,
        })
    }

    #[must_use]
    pub fn clusters(&self) -> &[MorSpecializationCluster] {
        &self.clusters
    }

    #[must_use]
    pub fn cluster_of(&self, page_id: PageId) -> Option<MorClusterId> {
        self.expert_to_cluster.get(&page_id.0).copied()
    }

    #[must_use]
    pub fn expert_count(&self) -> usize {
        self.expert_to_cluster.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorSpecializationBalancePolicy {
    pub load_ema_retention_ppm: u32,
    pub inter_cluster_balance_gain_ppm: u32,
    pub intra_cluster_balance_gain_ppm: u32,
    pub max_inter_cluster_adjustment_ppm: u32,
    pub max_intra_cluster_adjustment_ppm: u32,
    pub max_total_adjustment_ppm: u32,
}

impl Default for MorSpecializationBalancePolicy {
    fn default() -> Self {
        Self {
            load_ema_retention_ppm: 900_000,
            inter_cluster_balance_gain_ppm: 100_000,
            intra_cluster_balance_gain_ppm: 250_000,
            max_inter_cluster_adjustment_ppm: 25_000,
            max_intra_cluster_adjustment_ppm: 100_000,
            max_total_adjustment_ppm: 100_000,
        }
    }
}

impl MorSpecializationBalancePolicy {
    pub fn validate(self) -> Result<Self, MorSpecializationError> {
        for value in [
            self.load_ema_retention_ppm,
            self.inter_cluster_balance_gain_ppm,
            self.intra_cluster_balance_gain_ppm,
            self.max_inter_cluster_adjustment_ppm,
            self.max_intra_cluster_adjustment_ppm,
            self.max_total_adjustment_ppm,
        ] {
            if value > MOR_SCALE_PPM {
                return Err(MorSpecializationError::PolicyPpmOutOfRange(value));
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorClusterBalanceEvidence {
    pub cluster_id: MorClusterId,
    pub historical_load_ppm: u32,
    pub inter_cluster_adjustment_ppm: i32,
    pub candidate_experts: usize,
    pub selected_experts: usize,
    pub neutral_initialized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorExpertSpecializationEvidence {
    pub page_id: PageId,
    pub cluster_id: MorClusterId,
    pub historical_conditional_load_ppm: u32,
    pub inter_cluster_adjustment_ppm: i32,
    pub intra_cluster_adjustment_ppm: i32,
    pub combined_adjustment_ppm: i32,
    pub effective_balance_bias_ppm: i32,
    pub neutral_initialized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorSpecializationCycleMetrics {
    pub cycle: u64,
    pub candidate_clusters: usize,
    pub selected_clusters: usize,
    pub candidate_experts: usize,
    pub selected_experts: usize,
    pub newly_initialized_clusters: usize,
    pub newly_initialized_experts: usize,
    pub cluster_load_spread_ppm: u32,
    pub max_intra_cluster_load_spread_ppm: u32,
    pub route_switch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorSpecializationCycleReport {
    pub decision: MorDecision,
    pub metrics: MorSpecializationCycleMetrics,
    pub cluster_balance: Vec<MorClusterBalanceEvidence>,
    pub expert_balance: Vec<MorExpertSpecializationEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorSpecializationBalancer {
    registry: MorSpecializationRegistry,
    policy: MorSpecializationBalancePolicy,
    cycle: u64,
    previous: PreviousRoute,
    cluster_load_ppm: BTreeMap<MorClusterId, u32>,
    expert_conditional_load_ppm: BTreeMap<u32, u32>,
}

impl MorSpecializationBalancer {
    pub fn new(
        registry: MorSpecializationRegistry,
        policy: MorSpecializationBalancePolicy,
    ) -> Result<Self, MorSpecializationError> {
        Ok(Self {
            registry,
            policy: policy.validate()?,
            cycle: 0,
            previous: PreviousRoute::default(),
            cluster_load_ppm: BTreeMap::new(),
            expert_conditional_load_ppm: BTreeMap::new(),
        })
    }

    #[must_use]
    pub const fn cycle(&self) -> u64 {
        self.cycle
    }

    #[must_use]
    pub const fn policy(&self) -> MorSpecializationBalancePolicy {
        self.policy
    }

    #[must_use]
    pub fn registry(&self) -> &MorSpecializationRegistry {
        &self.registry
    }

    #[must_use]
    pub fn previous_route(&self) -> &PreviousRoute {
        &self.previous
    }

    #[must_use]
    pub fn cluster_load_ppm(&self, cluster_id: MorClusterId) -> Option<u32> {
        self.cluster_load_ppm.get(&cluster_id).copied()
    }

    #[must_use]
    pub fn expert_conditional_load_ppm(&self, page_id: PageId) -> Option<u32> {
        self.expert_conditional_load_ppm.get(&page_id.0).copied()
    }

    pub fn reset(&mut self) {
        self.cycle = 0;
        self.previous = PreviousRoute::default();
        self.cluster_load_ppm.clear();
        self.expert_conditional_load_ppm.clear();
    }

    pub fn route_cycle(
        &mut self,
        engine: &MixtureOfRouters,
        routers: &[RouterInput],
        runtime_hints: &[ExpertRuntimeHint],
    ) -> Result<MorSpecializationCycleReport, MorSpecializationError> {
        if self.cycle == u64::MAX {
            return Err(MorSpecializationError::CycleOverflow);
        }

        let candidate_pages = candidate_pages(routers);
        let candidate_clusters = candidate_clusters(&self.registry, &candidate_pages)?;
        let hint_map = runtime_hint_map(&self.registry, runtime_hints)?;

        let mut cluster_loads = self.cluster_load_ppm.clone();
        let mut expert_loads = self.expert_conditional_load_ppm.clone();

        let newly_initialized_clusters =
            initialize_cluster_loads(&mut cluster_loads, &candidate_clusters);
        let newly_initialized_experts =
            initialize_expert_loads(&self.registry, &mut expert_loads, &candidate_pages)?;

        let cluster_mean = mean_cluster_load(&cluster_loads, &candidate_clusters);
        let mut cluster_adjustments = BTreeMap::new();
        for cluster_id in &candidate_clusters {
            let historical = cluster_loads.get(cluster_id).copied().unwrap_or(0);
            cluster_adjustments.insert(
                *cluster_id,
                adaptive_adjustment(
                    cluster_mean,
                    historical,
                    self.policy.inter_cluster_balance_gain_ppm,
                    self.policy.max_inter_cluster_adjustment_ppm,
                ),
            );
        }

        let cluster_candidate_pages = pages_by_cluster(&self.registry, &candidate_pages)?;
        let mut intra_means = BTreeMap::new();
        for (cluster_id, pages) in &cluster_candidate_pages {
            intra_means.insert(*cluster_id, mean_expert_load(&expert_loads, pages));
        }

        let mut adjusted_hints = Vec::with_capacity(candidate_pages.len());
        let mut expert_balance = Vec::with_capacity(candidate_pages.len());
        for page in &candidate_pages {
            let page_id = PageId(*page);
            let cluster_id = self
                .registry
                .cluster_of(page_id)
                .ok_or(MorSpecializationError::UnregisteredExpert(page_id))?;
            let historical = expert_loads.get(page).copied().unwrap_or(0);
            let intra_mean = intra_means.get(&cluster_id).copied().unwrap_or(0);
            let inter_adjustment = cluster_adjustments.get(&cluster_id).copied().unwrap_or(0);
            let intra_adjustment = adaptive_adjustment(
                intra_mean,
                historical,
                self.policy.intra_cluster_balance_gain_ppm,
                self.policy.max_intra_cluster_adjustment_ppm,
            );
            let combined_adjustment = (inter_adjustment as i64 + intra_adjustment as i64).clamp(
                -(self.policy.max_total_adjustment_ppm as i64),
                self.policy.max_total_adjustment_ppm as i64,
            ) as i32;
            let base = hint_map.get(page).copied().unwrap_or(ExpertRuntimeHint {
                page_id,
                balance_bias_ppm: 0,
                residency_penalty_ppm: 0,
            });
            let effective_balance_bias_ppm =
                bounded_bias_add(base.balance_bias_ppm, combined_adjustment);

            adjusted_hints.push(ExpertRuntimeHint {
                page_id,
                balance_bias_ppm: effective_balance_bias_ppm,
                residency_penalty_ppm: base.residency_penalty_ppm,
            });
            expert_balance.push(MorExpertSpecializationEvidence {
                page_id,
                cluster_id,
                historical_conditional_load_ppm: historical,
                inter_cluster_adjustment_ppm: inter_adjustment,
                intra_cluster_adjustment_ppm: intra_adjustment,
                combined_adjustment_ppm: combined_adjustment,
                effective_balance_bias_ppm,
                neutral_initialized: newly_initialized_experts.contains(page),
            });
        }

        let old_primary = self.previous.primary;
        let decision = engine
            .route(routers, &adjusted_hints, &self.previous)
            .map_err(MorSpecializationError::Mor)?;
        let new_primary = decision.ranked_experts.first().map(|expert| expert.page_id);
        let route_switch =
            matches!((old_primary, new_primary), (Some(old), Some(new)) if old != new);

        let selected_pages = decision
            .selected_experts
            .iter()
            .map(|page| page.0)
            .collect::<BTreeSet<_>>();
        let selected_clusters = selected_cluster_set(&self.registry, &decision.selected_experts)?;

        update_cluster_loads(
            &mut cluster_loads,
            &candidate_clusters,
            &selected_clusters,
            self.policy.load_ema_retention_ppm,
        );
        update_expert_loads(
            &mut expert_loads,
            &candidate_pages,
            &selected_pages,
            self.policy.load_ema_retention_ppm,
        );

        let mut cluster_balance = Vec::with_capacity(candidate_clusters.len());
        for cluster_id in &candidate_clusters {
            let pages = cluster_candidate_pages
                .get(cluster_id)
                .ok_or(MorSpecializationError::InternalInvariant)?;
            cluster_balance.push(MorClusterBalanceEvidence {
                cluster_id: *cluster_id,
                historical_load_ppm: self
                    .cluster_load_ppm
                    .get(cluster_id)
                    .copied()
                    .unwrap_or_else(|| {
                        neutral_cluster_baseline(&self.cluster_load_ppm, &candidate_clusters)
                    }),
                inter_cluster_adjustment_ppm: cluster_adjustments
                    .get(cluster_id)
                    .copied()
                    .unwrap_or(0),
                candidate_experts: pages.len(),
                selected_experts: pages
                    .iter()
                    .filter(|page| selected_pages.contains(page))
                    .count(),
                neutral_initialized: newly_initialized_clusters.contains(cluster_id),
            });
        }

        self.cluster_load_ppm = cluster_loads;
        self.expert_conditional_load_ppm = expert_loads;
        self.cycle += 1;
        self.previous = PreviousRoute {
            primary: new_primary,
            selected: decision.selected_experts.clone(),
        };

        let metrics = MorSpecializationCycleMetrics {
            cycle: self.cycle,
            candidate_clusters: candidate_clusters.len(),
            selected_clusters: selected_clusters.len(),
            candidate_experts: candidate_pages.len(),
            selected_experts: decision.selected_experts.len(),
            newly_initialized_clusters: newly_initialized_clusters.len(),
            newly_initialized_experts: newly_initialized_experts.len(),
            cluster_load_spread_ppm: load_spread(
                &candidate_clusters
                    .iter()
                    .map(|cluster_id| self.cluster_load_ppm.get(cluster_id).copied().unwrap_or(0))
                    .collect::<Vec<_>>(),
            ),
            max_intra_cluster_load_spread_ppm: max_intra_cluster_spread(
                &self.expert_conditional_load_ppm,
                &cluster_candidate_pages,
            ),
            route_switch,
        };

        Ok(MorSpecializationCycleReport {
            decision,
            metrics,
            cluster_balance,
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

fn candidate_clusters(
    registry: &MorSpecializationRegistry,
    pages: &BTreeSet<u32>,
) -> Result<BTreeSet<MorClusterId>, MorSpecializationError> {
    pages
        .iter()
        .map(|page| {
            registry
                .cluster_of(PageId(*page))
                .ok_or(MorSpecializationError::UnregisteredExpert(PageId(*page)))
        })
        .collect()
}

fn pages_by_cluster(
    registry: &MorSpecializationRegistry,
    pages: &BTreeSet<u32>,
) -> Result<BTreeMap<MorClusterId, BTreeSet<u32>>, MorSpecializationError> {
    let mut grouped = BTreeMap::<MorClusterId, BTreeSet<u32>>::new();
    for page in pages {
        let cluster_id = registry
            .cluster_of(PageId(*page))
            .ok_or(MorSpecializationError::UnregisteredExpert(PageId(*page)))?;
        grouped.entry(cluster_id).or_default().insert(*page);
    }
    Ok(grouped)
}

fn selected_cluster_set(
    registry: &MorSpecializationRegistry,
    selected: &[PageId],
) -> Result<BTreeSet<MorClusterId>, MorSpecializationError> {
    selected
        .iter()
        .map(|page| {
            registry
                .cluster_of(*page)
                .ok_or(MorSpecializationError::UnregisteredExpert(*page))
        })
        .collect()
}

fn runtime_hint_map(
    registry: &MorSpecializationRegistry,
    runtime_hints: &[ExpertRuntimeHint],
) -> Result<BTreeMap<u32, ExpertRuntimeHint>, MorSpecializationError> {
    let mut map = BTreeMap::new();
    for hint in runtime_hints {
        if registry.cluster_of(hint.page_id).is_none() {
            return Err(MorSpecializationError::UnregisteredRuntimeHint(
                hint.page_id,
            ));
        }
        if map.insert(hint.page_id.0, *hint).is_some() {
            return Err(MorSpecializationError::DuplicateRuntimeHint(hint.page_id));
        }
    }
    Ok(map)
}

fn initialize_cluster_loads(
    loads: &mut BTreeMap<MorClusterId, u32>,
    candidate_clusters: &BTreeSet<MorClusterId>,
) -> BTreeSet<MorClusterId> {
    let baseline = neutral_cluster_baseline(loads, candidate_clusters);
    let mut initialized = BTreeSet::new();
    for cluster_id in candidate_clusters {
        if !loads.contains_key(cluster_id) {
            loads.insert(*cluster_id, baseline);
            initialized.insert(*cluster_id);
        }
    }
    initialized
}

fn neutral_cluster_baseline(
    loads: &BTreeMap<MorClusterId, u32>,
    candidate_clusters: &BTreeSet<MorClusterId>,
) -> u32 {
    let values = candidate_clusters
        .iter()
        .filter_map(|cluster_id| loads.get(cluster_id).copied())
        .collect::<Vec<_>>();
    mean_values(&values)
}

fn initialize_expert_loads(
    registry: &MorSpecializationRegistry,
    loads: &mut BTreeMap<u32, u32>,
    candidate_pages: &BTreeSet<u32>,
) -> Result<BTreeSet<u32>, MorSpecializationError> {
    let mut initialized = BTreeSet::new();
    let grouped = pages_by_cluster(registry, candidate_pages)?;
    for (cluster_id, pages) in grouped {
        let existing = registry
            .clusters()
            .iter()
            .find(|cluster| cluster.cluster_id == cluster_id)
            .ok_or(MorSpecializationError::InternalInvariant)?
            .experts
            .iter()
            .filter_map(|page| loads.get(&page.0).copied())
            .collect::<Vec<_>>();
        let baseline = mean_values(&existing);
        for page in pages {
            if let std::collections::btree_map::Entry::Vacant(entry) = loads.entry(page) {
                entry.insert(baseline);
                initialized.insert(page);
            }
        }
    }
    Ok(initialized)
}

fn mean_cluster_load(
    loads: &BTreeMap<MorClusterId, u32>,
    candidate_clusters: &BTreeSet<MorClusterId>,
) -> u32 {
    let values = candidate_clusters
        .iter()
        .map(|cluster_id| loads.get(cluster_id).copied().unwrap_or(0))
        .collect::<Vec<_>>();
    mean_values(&values)
}

fn mean_expert_load(loads: &BTreeMap<u32, u32>, pages: &BTreeSet<u32>) -> u32 {
    let values = pages
        .iter()
        .map(|page| loads.get(page).copied().unwrap_or(0))
        .collect::<Vec<_>>();
    mean_values(&values)
}

fn mean_values(values: &[u32]) -> u32 {
    if values.is_empty() {
        return 0;
    }
    let sum = values.iter().map(|value| *value as u64).sum::<u64>();
    (sum / values.len() as u64) as u32
}

fn adaptive_adjustment(mean: u32, load: u32, gain_ppm: u32, cap_ppm: u32) -> i32 {
    let delta = mean as i64 - load as i64;
    let scaled = delta * gain_ppm as i64 / MOR_SCALE_PPM as i64;
    scaled.clamp(-(cap_ppm as i64), cap_ppm as i64) as i32
}

fn bounded_bias_add(base: i32, adjustment: i32) -> i32 {
    (base as i64 + adjustment as i64).clamp(-(MOR_SCALE_PPM as i64), MOR_SCALE_PPM as i64) as i32
}

fn update_cluster_loads(
    loads: &mut BTreeMap<MorClusterId, u32>,
    candidate_clusters: &BTreeSet<MorClusterId>,
    selected_clusters: &BTreeSet<MorClusterId>,
    retention_ppm: u32,
) {
    for cluster_id in candidate_clusters {
        let old = loads.get(cluster_id).copied().unwrap_or(0);
        let observed = if selected_clusters.contains(cluster_id) {
            MOR_SCALE_PPM
        } else {
            0
        };
        loads.insert(*cluster_id, ema_ppm(old, observed, retention_ppm));
    }
}

fn update_expert_loads(
    loads: &mut BTreeMap<u32, u32>,
    candidate_pages: &BTreeSet<u32>,
    selected_pages: &BTreeSet<u32>,
    retention_ppm: u32,
) {
    for page in candidate_pages {
        let old = loads.get(page).copied().unwrap_or(0);
        let observed = if selected_pages.contains(page) {
            MOR_SCALE_PPM
        } else {
            0
        };
        loads.insert(*page, ema_ppm(old, observed, retention_ppm));
    }
}

fn ema_ppm(old: u32, observed: u32, retention_ppm: u32) -> u32 {
    let retained = old as u64 * retention_ppm as u64;
    let fresh = observed as u64 * (MOR_SCALE_PPM - retention_ppm) as u64;
    ((retained + fresh) / MOR_SCALE_PPM as u64) as u32
}

fn max_intra_cluster_spread(
    loads: &BTreeMap<u32, u32>,
    grouped: &BTreeMap<MorClusterId, BTreeSet<u32>>,
) -> u32 {
    grouped
        .values()
        .map(|pages| {
            load_spread(
                &pages
                    .iter()
                    .map(|page| loads.get(page).copied().unwrap_or(0))
                    .collect::<Vec<_>>(),
            )
        })
        .max()
        .unwrap_or(0)
}

fn load_spread(values: &[u32]) -> u32 {
    match (values.iter().min(), values.iter().max()) {
        (Some(minimum), Some(maximum)) => maximum.saturating_sub(*minimum),
        _ => 0,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MorSpecializationError {
    EmptyRegistry,
    DuplicateCluster(MorClusterId),
    EmptyCluster(MorClusterId),
    DuplicateClusterExpert {
        cluster_id: MorClusterId,
        page_id: PageId,
    },
    ExpertInMultipleClusters {
        page_id: PageId,
        first: MorClusterId,
        second: MorClusterId,
    },
    PolicyPpmOutOfRange(u32),
    UnregisteredExpert(PageId),
    UnregisteredRuntimeHint(PageId),
    DuplicateRuntimeHint(PageId),
    CycleOverflow,
    InternalInvariant,
    Mor(MorError),
}

impl fmt::Display for MorSpecializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorSpecializationError {}

#[cfg(test)]
mod tests {
    use super::{
        MorClusterId, MorSpecializationBalancePolicy, MorSpecializationBalancer,
        MorSpecializationCluster, MorSpecializationError, MorSpecializationRegistry,
    };
    use crate::{
        mor::{ExpertProposal, MixtureOfRouters, MorPolicy, RouterId, RouterInput, MOR_SCALE_PPM},
        mor_controller::{MorController, MorControllerPolicy},
        PageId,
    };

    fn router(id: u32, page: u32, score_ppm: u32) -> RouterInput {
        RouterInput {
            router_id: RouterId(id),
            affinity_ppm: MOR_SCALE_PPM,
            reliability_ppm: MOR_SCALE_PPM,
            confidence_ppm: MOR_SCALE_PPM,
            balance_bias_ppm: 0,
            proposals: vec![ExpertProposal {
                page_id: PageId(page),
                score_ppm,
            }],
        }
    }

    fn engine() -> MixtureOfRouters {
        MixtureOfRouters::new(MorPolicy {
            min_routers: 1,
            max_routers: 2,
            router_mass_threshold_ppm: MOR_SCALE_PPM,
            min_experts: 1,
            max_experts: 1,
            base_expert_mass_threshold_ppm: MOR_SCALE_PPM,
            ..MorPolicy::default()
        })
        .expect("engine")
    }

    fn two_cluster_registry() -> MorSpecializationRegistry {
        MorSpecializationRegistry::new(vec![
            MorSpecializationCluster {
                cluster_id: MorClusterId(10),
                experts: vec![PageId(0)],
            },
            MorSpecializationCluster {
                cluster_id: MorClusterId(20),
                experts: vec![PageId(1)],
            },
        ])
        .expect("registry")
    }

    fn aggressive_policy() -> MorSpecializationBalancePolicy {
        MorSpecializationBalancePolicy {
            load_ema_retention_ppm: 900_000,
            inter_cluster_balance_gain_ppm: MOR_SCALE_PPM,
            intra_cluster_balance_gain_ppm: MOR_SCALE_PPM,
            max_inter_cluster_adjustment_ppm: 200_000,
            max_intra_cluster_adjustment_ppm: 200_000,
            max_total_adjustment_ppm: 200_000,
        }
    }

    #[test]
    fn neutral_first_exposure_avoids_global_cold_cluster_flip() {
        let engine = engine();
        let mut global = MorController::new(MorControllerPolicy {
            load_ema_retention_ppm: 900_000,
            router_balance_gain_ppm: 0,
            expert_balance_gain_ppm: MOR_SCALE_PPM,
            max_router_adjustment_ppm: 0,
            max_expert_adjustment_ppm: 200_000,
        })
        .expect("global controller");
        let mut specialized =
            MorSpecializationBalancer::new(two_cluster_registry(), aggressive_policy())
                .expect("specialized");

        for _ in 0..10 {
            global
                .route_cycle(&engine, &[router(0, 0, 900_000)], &[])
                .expect("global warmup");
            specialized
                .route_cycle(&engine, &[router(0, 0, 900_000)], &[])
                .expect("specialized warmup");
        }

        let final_inputs = [router(0, 0, 600_000), router(1, 1, 550_000)];
        let global_report = global
            .route_cycle(&engine, &final_inputs, &[])
            .expect("global final");
        let specialized_report = specialized
            .route_cycle(&engine, &final_inputs, &[])
            .expect("specialized final");

        assert_eq!(global_report.decision.selected_experts, vec![PageId(1)]);
        assert_eq!(
            specialized_report.decision.selected_experts,
            vec![PageId(0)]
        );

        let new_cluster = specialized_report
            .cluster_balance
            .iter()
            .find(|evidence| evidence.cluster_id == MorClusterId(20))
            .expect("new cluster evidence");
        assert!(new_cluster.neutral_initialized);
        assert_eq!(new_cluster.inter_cluster_adjustment_ppm, 0);
    }

    #[test]
    fn absent_cluster_and_expert_load_do_not_decay_without_exposure() {
        let engine = engine();
        let mut balancer =
            MorSpecializationBalancer::new(two_cluster_registry(), aggressive_policy())
                .expect("balancer");

        balancer
            .route_cycle(&engine, &[router(1, 1, 900_000)], &[])
            .expect("prime B");
        let cluster_before = balancer
            .cluster_load_ppm(MorClusterId(20))
            .expect("cluster B load");
        let expert_before = balancer
            .expert_conditional_load_ppm(PageId(1))
            .expect("expert B load");

        for _ in 0..8 {
            balancer
                .route_cycle(&engine, &[router(0, 0, 900_000)], &[])
                .expect("A-only cycle");
        }

        assert_eq!(
            balancer.cluster_load_ppm(MorClusterId(20)),
            Some(cluster_before)
        );
        assert_eq!(
            balancer.expert_conditional_load_ppm(PageId(1)),
            Some(expert_before)
        );
    }

    #[test]
    fn intra_cluster_balance_can_shift_work_without_cross_cluster_pressure() {
        let registry = MorSpecializationRegistry::new(vec![MorSpecializationCluster {
            cluster_id: MorClusterId(7),
            experts: vec![PageId(0), PageId(1)],
        }])
        .expect("registry");
        let policy = MorSpecializationBalancePolicy {
            inter_cluster_balance_gain_ppm: MOR_SCALE_PPM,
            intra_cluster_balance_gain_ppm: MOR_SCALE_PPM,
            max_inter_cluster_adjustment_ppm: 200_000,
            max_intra_cluster_adjustment_ppm: 100_000,
            max_total_adjustment_ppm: 100_000,
            ..MorSpecializationBalancePolicy::default()
        };
        let engine = engine();
        let mut balancer = MorSpecializationBalancer::new(registry, policy).expect("balancer");

        balancer
            .route_cycle(
                &engine,
                &[router(0, 0, 900_000), router(1, 1, 100_000)],
                &[],
            )
            .expect("prime expert 0");

        let report = balancer
            .route_cycle(
                &engine,
                &[router(0, 0, 500_000), router(1, 1, 500_000)],
                &[],
            )
            .expect("balanced route");

        let page0 = report
            .expert_balance
            .iter()
            .find(|evidence| evidence.page_id == PageId(0))
            .expect("page 0");
        let page1 = report
            .expert_balance
            .iter()
            .find(|evidence| evidence.page_id == PageId(1))
            .expect("page 1");

        assert_eq!(page0.inter_cluster_adjustment_ppm, 0);
        assert_eq!(page1.inter_cluster_adjustment_ppm, 0);
        assert!(page0.intra_cluster_adjustment_ppm < 0);
        assert!(page1.intra_cluster_adjustment_ppm > 0);
        assert_eq!(report.decision.selected_experts, vec![PageId(1)]);
    }

    #[test]
    fn inter_cluster_adjustment_has_its_own_hard_cap() {
        let policy = MorSpecializationBalancePolicy {
            inter_cluster_balance_gain_ppm: MOR_SCALE_PPM,
            intra_cluster_balance_gain_ppm: 0,
            max_inter_cluster_adjustment_ppm: 10_000,
            max_intra_cluster_adjustment_ppm: 0,
            max_total_adjustment_ppm: 10_000,
            ..MorSpecializationBalancePolicy::default()
        };
        let engine = engine();
        let mut balancer =
            MorSpecializationBalancer::new(two_cluster_registry(), policy).expect("balancer");

        balancer
            .route_cycle(
                &engine,
                &[router(0, 0, 900_000), router(1, 1, 100_000)],
                &[],
            )
            .expect("prime A");
        let report = balancer
            .route_cycle(
                &engine,
                &[router(0, 0, 500_000), router(1, 1, 500_000)],
                &[],
            )
            .expect("second route");

        let adjustments = report
            .cluster_balance
            .iter()
            .map(|evidence| evidence.inter_cluster_adjustment_ppm)
            .collect::<Vec<_>>();
        assert!(adjustments.iter().all(|value| value.abs() <= 10_000));
        assert!(adjustments.contains(&-10_000));
        assert!(adjustments.contains(&10_000));
    }

    #[test]
    fn malformed_specialization_registry_fails_closed() {
        let duplicate = MorSpecializationRegistry::new(vec![
            MorSpecializationCluster {
                cluster_id: MorClusterId(1),
                experts: vec![PageId(0)],
            },
            MorSpecializationCluster {
                cluster_id: MorClusterId(2),
                experts: vec![PageId(0)],
            },
        ]);
        assert_eq!(
            duplicate,
            Err(MorSpecializationError::ExpertInMultipleClusters {
                page_id: PageId(0),
                first: MorClusterId(1),
                second: MorClusterId(2),
            })
        );
    }

    #[test]
    fn unregistered_candidate_expert_fails_before_routing() {
        let mut balancer =
            MorSpecializationBalancer::new(two_cluster_registry(), aggressive_policy())
                .expect("balancer");
        let error = balancer
            .route_cycle(&engine(), &[router(9, 9, 500_000)], &[])
            .expect_err("unknown page");
        assert_eq!(error, MorSpecializationError::UnregisteredExpert(PageId(9)));
        assert_eq!(balancer.cycle(), 0);
    }
}
