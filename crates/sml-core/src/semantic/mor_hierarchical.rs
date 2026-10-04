//! Bounded hierarchical SBG -> MOR candidate narrowing.
//!
//! MOR-H1 is an experimental coarse-to-fine adapter. It first scores a small,
//! explicit set of semantic clusters, then evaluates expert similarity only on
//! experts reachable from the selected clusters. The existing MixtureOfRouters
//! engine remains the only final router.
//!
//! This module is a reference-semantic work-reduction primitive. Lower logical
//! similarity counts are not a latency or quality claim.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    mor::{
        ExpertProposal, ExpertRuntimeHint, MixtureOfRouters, MorDecision, PreviousRoute, RouterId,
        RouterInput, MOR_SCALE_PPM,
    },
    mor_registry::{MorRegistryError, MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    mor_specialization::{MorClusterId, MorSpecializationCluster, MorSpecializationError, MorSpecializationRegistry},
    PageId,
};

use super::{SemanticBitField, SemanticMorTopK, SemanticState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticMorHierarchyPolicy {
    pub min_clusters: usize,
    pub max_clusters: usize,
    pub cluster_mass_threshold_ppm: u32,
    pub max_candidate_experts: usize,
    pub max_candidate_edges: usize,
    pub proposal_top_k: SemanticMorTopK,
}

impl Default for SemanticMorHierarchyPolicy {
    fn default() -> Self {
        Self {
            min_clusters: 1,
            max_clusters: 2,
            cluster_mass_threshold_ppm: 800_000,
            max_candidate_experts: 64,
            max_candidate_edges: 256,
            proposal_top_k: SemanticMorTopK::Two,
        }
    }
}

impl SemanticMorHierarchyPolicy {
    pub fn validate(self) -> Result<Self, SemanticMorHierarchyError> {
        if self.min_clusters == 0 || self.max_clusters < self.min_clusters {
            return Err(SemanticMorHierarchyError::InvalidClusterRange {
                min: self.min_clusters,
                max: self.max_clusters,
            });
        }
        if self.cluster_mass_threshold_ppm > MOR_SCALE_PPM {
            return Err(SemanticMorHierarchyError::PolicyPpmOutOfRange(
                self.cluster_mass_threshold_ppm,
            ));
        }
        if self.max_candidate_experts == 0 {
            return Err(SemanticMorHierarchyError::ZeroCandidateExpertBudget);
        }
        if self.max_candidate_edges == 0 {
            return Err(SemanticMorHierarchyError::ZeroCandidateEdgeBudget);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorClusterSignature<const BITS: usize> {
    pub cluster_id: u32,
    pub signature: SemanticBitField<BITS>,
    pub experts: Vec<PageId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorClusterEvidence {
    pub cluster_id: u32,
    pub score_ppm: u32,
    pub expert_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticMorHierarchyAccounting {
    pub cluster_similarity_evaluations: usize,
    pub router_similarity_evaluations: usize,
    pub expert_similarity_evaluations: usize,
    pub exhaustive_expert_similarity_evaluations: usize,
    pub candidate_experts: usize,
    pub reachable_experts: usize,
    pub candidate_edges: usize,
    pub exhaustive_edges: usize,
    pub candidate_expert_fraction_ppm: u32,
    pub expert_edge_fraction_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticHierarchicalMorRoute {
    pub selected_clusters: Vec<SemanticMorClusterEvidence>,
    pub candidate_experts: Vec<PageId>,
    pub router_inputs: Vec<RouterInput>,
    pub decision: MorDecision,
    pub accounting: SemanticMorHierarchyAccounting,
}

#[derive(Clone, Copy)]
pub(crate) struct SemanticMorRouteContext<'a> {
    pub registry: &'a MorRouterRegistry,
    pub engine: &'a MixtureOfRouters,
    pub runtime_hints: &'a [ExpertRuntimeHint],
    pub previous: &'a PreviousRoute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticHierarchicalMorBridge<const BITS: usize> {
    router_signatures: BTreeMap<RouterId, SemanticBitField<BITS>>,
    expert_signatures: BTreeMap<u32, SemanticBitField<BITS>>,
    clusters: Vec<SemanticMorClusterSignature<BITS>>,
    policy: SemanticMorHierarchyPolicy,
}

impl<const BITS: usize> SemanticHierarchicalMorBridge<BITS> {
    pub fn new(
        router_signatures: Vec<(RouterId, SemanticBitField<BITS>)>,
        expert_signatures: Vec<(PageId, SemanticBitField<BITS>)>,
        clusters: Vec<SemanticMorClusterSignature<BITS>>,
        policy: SemanticMorHierarchyPolicy,
    ) -> Result<Self, SemanticMorHierarchyError> {
        let policy = policy.validate()?;
        if router_signatures.is_empty() {
            return Err(SemanticMorHierarchyError::EmptyRouterSignatures);
        }
        if expert_signatures.is_empty() {
            return Err(SemanticMorHierarchyError::EmptyExpertSignatures);
        }
        if clusters.is_empty() {
            return Err(SemanticMorHierarchyError::EmptyClusters);
        }

        let mut routers = BTreeMap::new();
        for (router_id, signature) in router_signatures {
            if routers.insert(router_id, signature).is_some() {
                return Err(SemanticMorHierarchyError::DuplicateRouterSignature(
                    router_id,
                ));
            }
        }

        let mut experts = BTreeMap::new();
        for (page_id, signature) in expert_signatures {
            if experts.insert(page_id.0, signature).is_some() {
                return Err(SemanticMorHierarchyError::DuplicateExpertSignature(page_id));
            }
        }

        let mut cluster_ids = BTreeSet::new();
        let mut clustered_experts = BTreeSet::new();
        for cluster in &clusters {
            if !cluster_ids.insert(cluster.cluster_id) {
                return Err(SemanticMorHierarchyError::DuplicateCluster(
                    cluster.cluster_id,
                ));
            }
            if cluster.experts.is_empty() {
                return Err(SemanticMorHierarchyError::EmptyCluster(cluster.cluster_id));
            }
            let mut local = BTreeSet::new();
            for page in &cluster.experts {
                if !local.insert(page.0) {
                    return Err(SemanticMorHierarchyError::DuplicateClusterExpert {
                        cluster_id: cluster.cluster_id,
                        page_id: *page,
                    });
                }
                if !clustered_experts.insert(page.0) {
                    return Err(SemanticMorHierarchyError::ExpertInMultipleClusters(*page));
                }
            }
        }

        if clusters.len() < policy.min_clusters {
            return Err(SemanticMorHierarchyError::InsufficientClusters {
                required: policy.min_clusters,
                actual: clusters.len(),
            });
        }

        Ok(Self {
            router_signatures: routers,
            expert_signatures: experts,
            clusters,
            policy,
        })
    }

    #[must_use]
    pub const fn policy(&self) -> SemanticMorHierarchyPolicy {
        self.policy
    }

    pub fn specialization_registry(
        &self,
    ) -> Result<MorSpecializationRegistry, MorSpecializationError> {
        MorSpecializationRegistry::new(
            self.clusters
                .iter()
                .map(|cluster| MorSpecializationCluster {
                    cluster_id: MorClusterId(cluster.cluster_id),
                    experts: cluster.experts.clone(),
                })
                .collect(),
        )
    }

    pub fn route(
        &self,
        state: &SemanticState<BITS>,
        registry: &MorRouterRegistry,
        engine: &MixtureOfRouters,
        runtime_hints: &[ExpertRuntimeHint],
        previous: &PreviousRoute,
    ) -> Result<SemanticHierarchicalMorRoute, SemanticMorHierarchyError> {
        self.validate_against_registry(registry)?;

        let ranked_clusters = self.rank_clusters(state)?;
        let selected_clusters = self.select_clusters_from_ranked(&ranked_clusters)?;
        self.route_selected_clusters(
            state,
            SemanticMorRouteContext {
                registry,
                engine,
                runtime_hints,
                previous,
            },
            selected_clusters,
            ranked_clusters.len(),
        )
    }

    pub(crate) fn cluster_count(&self) -> usize {
        self.clusters.len()
    }

    pub(crate) fn rank_clusters(
        &self,
        state: &SemanticState<BITS>,
    ) -> Result<Vec<SemanticMorClusterEvidence>, SemanticMorHierarchyError> {
        let mut ranked = self
            .clusters
            .iter()
            .map(|cluster| {
                Ok(SemanticMorClusterEvidence {
                    cluster_id: cluster.cluster_id,
                    score_ppm: semantic_similarity_ppm(&state.routing, &cluster.signature)?,
                    expert_count: cluster.experts.len(),
                })
            })
            .collect::<Result<Vec<_>, SemanticMorHierarchyError>>()?;

        ranked.sort_by(|left, right| {
            right
                .score_ppm
                .cmp(&left.score_ppm)
                .then_with(|| left.cluster_id.cmp(&right.cluster_id))
        });

        if ranked.iter().all(|cluster| cluster.score_ppm == 0) {
            return Err(SemanticMorHierarchyError::ZeroClusterMass);
        }
        Ok(ranked)
    }

    pub(crate) fn route_ranked_prefix(
        &self,
        state: &SemanticState<BITS>,
        context: SemanticMorRouteContext<'_>,
        ranked_clusters: &[SemanticMorClusterEvidence],
        cluster_count: usize,
        cluster_similarity_evaluations: usize,
    ) -> Result<SemanticHierarchicalMorRoute, SemanticMorHierarchyError> {
        self.validate_against_registry(context.registry)?;
        if cluster_count == 0 || cluster_count > ranked_clusters.len() {
            return Err(SemanticMorHierarchyError::InvalidExactClusterCount {
                requested: cluster_count,
                available: ranked_clusters.len(),
            });
        }
        self.route_selected_clusters(
            state,
            context,
            ranked_clusters[..cluster_count].to_vec(),
            cluster_similarity_evaluations,
        )
    }

    fn route_selected_clusters(
        &self,
        state: &SemanticState<BITS>,
        context: SemanticMorRouteContext<'_>,
        selected_clusters: Vec<SemanticMorClusterEvidence>,
        cluster_similarity_evaluations: usize,
    ) -> Result<SemanticHierarchicalMorRoute, SemanticMorHierarchyError> {
        let selected_cluster_ids = selected_clusters
            .iter()
            .map(|cluster| cluster.cluster_id)
            .collect::<BTreeSet<_>>();
        let candidate_set = self
            .clusters
            .iter()
            .filter(|cluster| selected_cluster_ids.contains(&cluster.cluster_id))
            .flat_map(|cluster| cluster.experts.iter().map(|page| page.0))
            .collect::<BTreeSet<_>>();

        if candidate_set.len() > self.policy.max_candidate_experts {
            return Err(SemanticMorHierarchyError::CandidateExpertBudget {
                actual: candidate_set.len(),
                maximum: self.policy.max_candidate_experts,
            });
        }

        let mut filtered = Vec::new();
        let mut candidate_edges = 0_usize;
        let mut max_fanout = 0_usize;
        for specialization in context.registry.specializations() {
            let experts = specialization
                .experts
                .iter()
                .copied()
                .filter(|page| candidate_set.contains(&page.0))
                .collect::<Vec<_>>();
            if experts.is_empty() {
                continue;
            }
            candidate_edges = candidate_edges
                .checked_add(experts.len())
                .ok_or(SemanticMorHierarchyError::ArithmeticOverflow)?;
            if candidate_edges > self.policy.max_candidate_edges {
                return Err(SemanticMorHierarchyError::CandidateEdgeBudget {
                    actual: candidate_edges,
                    maximum: self.policy.max_candidate_edges,
                });
            }
            max_fanout = max_fanout.max(experts.len());
            filtered.push(RouterSpecialization {
                router_id: specialization.router_id,
                experts,
            });
        }

        if filtered.is_empty() {
            return Err(SemanticMorHierarchyError::EmptyCandidateRouterSet);
        }

        let filtered_registry = MorRouterRegistry::new(
            filtered,
            MorRegistryPolicy {
                expert_count: context.registry.policy().expert_count,
                max_routers: context.registry.specializations().len(),
                max_edges: candidate_edges,
                max_fanout,
                require_full_coverage: false,
            },
        )
        .map_err(SemanticMorHierarchyError::Registry)?;

        let router_inputs = self.build_filtered_inputs(state, &filtered_registry)?;
        let decision = filtered_registry
            .route(
                context.engine,
                &router_inputs,
                context.runtime_hints,
                context.previous,
            )
            .map_err(SemanticMorHierarchyError::Registry)?;

        let candidate_experts = candidate_set
            .iter()
            .copied()
            .map(PageId)
            .collect::<Vec<_>>();
        let exhaustive_edges = context.registry.accounting().edge_count;
        let reachable_experts = context.registry.accounting().covered_experts as usize;

        Ok(SemanticHierarchicalMorRoute {
            selected_clusters,
            candidate_experts,
            accounting: SemanticMorHierarchyAccounting {
                cluster_similarity_evaluations,
                router_similarity_evaluations: router_inputs.len(),
                expert_similarity_evaluations: candidate_edges,
                exhaustive_expert_similarity_evaluations: exhaustive_edges,
                candidate_experts: candidate_set.len(),
                reachable_experts,
                candidate_edges,
                exhaustive_edges,
                candidate_expert_fraction_ppm: fraction_ppm(candidate_set.len(), reachable_experts),
                expert_edge_fraction_ppm: fraction_ppm(candidate_edges, exhaustive_edges),
            },
            router_inputs,
            decision,
        })
    }

    fn select_clusters_from_ranked(
        &self,
        ranked: &[SemanticMorClusterEvidence],
    ) -> Result<Vec<SemanticMorClusterEvidence>, SemanticMorHierarchyError> {
        let total = ranked
            .iter()
            .map(|cluster| cluster.score_ppm as u128)
            .sum::<u128>();
        if total == 0 {
            return Err(SemanticMorHierarchyError::ZeroClusterMass);
        }

        let target = mass_target(total, self.policy.cluster_mass_threshold_ppm)?;
        let mut cumulative = 0_u128;
        let mut selected = Vec::new();
        for cluster in ranked.iter().cloned() {
            if selected.len() >= self.policy.max_clusters {
                break;
            }
            if selected.len() >= self.policy.min_clusters && cumulative >= target {
                break;
            }
            cumulative = cumulative
                .checked_add(cluster.score_ppm as u128)
                .ok_or(SemanticMorHierarchyError::ArithmeticOverflow)?;
            selected.push(cluster);
        }

        if selected.len() < self.policy.min_clusters {
            return Err(SemanticMorHierarchyError::InsufficientClusters {
                required: self.policy.min_clusters,
                actual: selected.len(),
            });
        }
        Ok(selected)
    }

    fn build_filtered_inputs(
        &self,
        state: &SemanticState<BITS>,
        registry: &MorRouterRegistry,
    ) -> Result<Vec<RouterInput>, SemanticMorHierarchyError> {
        let mut inputs = Vec::with_capacity(registry.specializations().len());
        for specialization in registry.specializations() {
            let router_signature = self
                .router_signatures
                .get(&specialization.router_id)
                .ok_or(SemanticMorHierarchyError::MissingRouterSignature(
                    specialization.router_id,
                ))?;
            let affinity_ppm = semantic_similarity_ppm(&state.routing, router_signature)?;

            let mut proposals = specialization
                .experts
                .iter()
                .map(|page_id| {
                    let signature = self
                        .expert_signatures
                        .get(&page_id.0)
                        .ok_or(SemanticMorHierarchyError::MissingExpertSignature(*page_id))?;
                    Ok(ExpertProposal {
                        page_id: *page_id,
                        score_ppm: semantic_similarity_ppm(&state.routing, signature)?,
                    })
                })
                .collect::<Result<Vec<_>, SemanticMorHierarchyError>>()?;

            proposals.sort_by(|left, right| {
                right
                    .score_ppm
                    .cmp(&left.score_ppm)
                    .then_with(|| left.page_id.0.cmp(&right.page_id.0))
            });
            proposals.truncate(self.policy.proposal_top_k.as_usize().min(proposals.len()));

            inputs.push(RouterInput {
                router_id: specialization.router_id,
                affinity_ppm,
                reliability_ppm: MOR_SCALE_PPM,
                confidence_ppm: MOR_SCALE_PPM,
                balance_bias_ppm: 0,
                proposals,
            });
        }
        Ok(inputs)
    }

    fn validate_against_registry(
        &self,
        registry: &MorRouterRegistry,
    ) -> Result<(), SemanticMorHierarchyError> {
        let required_routers = registry
            .specializations()
            .iter()
            .map(|specialization| specialization.router_id)
            .collect::<BTreeSet<_>>();
        for router_id in &required_routers {
            if !self.router_signatures.contains_key(router_id) {
                return Err(SemanticMorHierarchyError::MissingRouterSignature(
                    *router_id,
                ));
            }
        }
        for router_id in self.router_signatures.keys() {
            if !required_routers.contains(router_id) {
                return Err(SemanticMorHierarchyError::UnknownRouterSignature(
                    *router_id,
                ));
            }
        }

        let reachable = registry
            .specializations()
            .iter()
            .flat_map(|specialization| specialization.experts.iter().map(|page| page.0))
            .collect::<BTreeSet<_>>();

        let mut clustered = BTreeSet::new();
        for cluster in &self.clusters {
            for page in &cluster.experts {
                if !reachable.contains(&page.0) {
                    return Err(SemanticMorHierarchyError::UnknownClusterExpert {
                        cluster_id: cluster.cluster_id,
                        page_id: *page,
                    });
                }
                clustered.insert(page.0);
            }
        }

        for page in &reachable {
            let page_id = PageId(*page);
            if !self.expert_signatures.contains_key(page) {
                return Err(SemanticMorHierarchyError::MissingExpertSignature(page_id));
            }
            if !clustered.contains(page) {
                return Err(SemanticMorHierarchyError::UnclusteredExpert(page_id));
            }
        }

        for page in self.expert_signatures.keys().copied() {
            if page >= registry.policy().expert_count {
                return Err(SemanticMorHierarchyError::ExpertSignatureOutOfRange {
                    page_id: PageId(page),
                    expert_count: registry.policy().expert_count,
                });
            }
        }
        Ok(())
    }
}

fn semantic_similarity_ppm<const BITS: usize>(
    routing: &SemanticBitField<BITS>,
    signature: &SemanticBitField<BITS>,
) -> Result<u32, SemanticMorHierarchyError> {
    if BITS == 0 {
        return Err(SemanticMorHierarchyError::ZeroSemanticWidth);
    }
    let equal = routing.xnor_popcount(signature) as u128;
    let ppm = equal
        .checked_mul(MOR_SCALE_PPM as u128)
        .ok_or(SemanticMorHierarchyError::ArithmeticOverflow)?
        / BITS as u128;
    u32::try_from(ppm).map_err(|_| SemanticMorHierarchyError::ArithmeticOverflow)
}

fn mass_target(total: u128, threshold_ppm: u32) -> Result<u128, SemanticMorHierarchyError> {
    let scaled = total
        .checked_mul(threshold_ppm as u128)
        .ok_or(SemanticMorHierarchyError::ArithmeticOverflow)?;
    Ok(scaled.div_ceil(MOR_SCALE_PPM as u128))
}

fn fraction_ppm(numerator: usize, denominator: usize) -> u32 {
    if denominator == 0 {
        return 0;
    }
    ((numerator as u128 * MOR_SCALE_PPM as u128) / denominator as u128) as u32
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticMorHierarchyError {
    InvalidClusterRange { min: usize, max: usize },
    PolicyPpmOutOfRange(u32),
    ZeroCandidateExpertBudget,
    ZeroCandidateEdgeBudget,
    EmptyRouterSignatures,
    EmptyExpertSignatures,
    EmptyClusters,
    DuplicateRouterSignature(RouterId),
    DuplicateExpertSignature(PageId),
    DuplicateCluster(u32),
    EmptyCluster(u32),
    DuplicateClusterExpert { cluster_id: u32, page_id: PageId },
    ExpertInMultipleClusters(PageId),
    InsufficientClusters { required: usize, actual: usize },
    InvalidExactClusterCount { requested: usize, available: usize },
    MissingRouterSignature(RouterId),
    UnknownRouterSignature(RouterId),
    MissingExpertSignature(PageId),
    ExpertSignatureOutOfRange { page_id: PageId, expert_count: u32 },
    UnknownClusterExpert { cluster_id: u32, page_id: PageId },
    UnclusteredExpert(PageId),
    ZeroSemanticWidth,
    ZeroClusterMass,
    CandidateExpertBudget { actual: usize, maximum: usize },
    CandidateEdgeBudget { actual: usize, maximum: usize },
    EmptyCandidateRouterSet,
    ArithmeticOverflow,
    Registry(MorRegistryError),
}

impl fmt::Display for SemanticMorHierarchyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticMorHierarchyError {}

#[cfg(test)]
mod tests {
    use super::{
        SemanticHierarchicalMorBridge, SemanticMorClusterSignature, SemanticMorHierarchyError,
        SemanticMorHierarchyPolicy,
    };
    use crate::{
        mor::{MixtureOfRouters, MorPolicy, PreviousRoute, RouterId, MOR_SCALE_PPM},
        mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
        mor_specialization::MorClusterId,
        semantic::{
            SemanticBitField, SemanticExpertSignature, SemanticMorBridge, SemanticMorTopK,
            SemanticRouterSignature, SemanticState,
        },
        PageId,
    };

    fn bits(indices: &[usize]) -> SemanticBitField<8> {
        SemanticBitField::from_set_indices(indices).expect("bits")
    }

    fn state(indices: &[usize]) -> SemanticState<8> {
        let mut state = SemanticState::zeroed().expect("state");
        state.routing = bits(indices);
        state
    }

    fn registry() -> MorRouterRegistry {
        MorRouterRegistry::new(
            vec![
                RouterSpecialization {
                    router_id: RouterId(0),
                    experts: vec![PageId(0), PageId(1)],
                },
                RouterSpecialization {
                    router_id: RouterId(1),
                    experts: vec![PageId(2), PageId(3)],
                },
            ],
            MorRegistryPolicy {
                expert_count: 4,
                max_routers: 2,
                max_edges: 4,
                max_fanout: 2,
                require_full_coverage: true,
            },
        )
        .expect("registry")
    }

    fn hierarchy(policy: SemanticMorHierarchyPolicy) -> SemanticHierarchicalMorBridge<8> {
        SemanticHierarchicalMorBridge::new(
            vec![
                (RouterId(0), bits(&[0, 1, 2, 3])),
                (RouterId(1), bits(&[4, 5, 6, 7])),
            ],
            vec![
                (PageId(0), bits(&[0, 1, 2, 3])),
                (PageId(1), bits(&[0, 1, 2, 4])),
                (PageId(2), bits(&[4, 5, 6, 7])),
                (PageId(3), bits(&[3, 5, 6, 7])),
            ],
            vec![
                SemanticMorClusterSignature {
                    cluster_id: 10,
                    signature: bits(&[0, 1, 2, 3]),
                    experts: vec![PageId(0), PageId(1)],
                },
                SemanticMorClusterSignature {
                    cluster_id: 20,
                    signature: bits(&[4, 5, 6, 7]),
                    experts: vec![PageId(2), PageId(3)],
                },
            ],
            policy,
        )
        .expect("hierarchy")
    }

    #[test]
    fn coarse_pool_reduces_expert_similarity_work_and_preserves_primary() {
        let registry = registry();
        let state = state(&[0, 1, 2, 3]);
        let engine = MixtureOfRouters::new(MorPolicy {
            min_routers: 1,
            max_routers: 1,
            router_mass_threshold_ppm: MOR_SCALE_PPM,
            ..MorPolicy::default()
        })
        .expect("engine");

        let policy = SemanticMorHierarchyPolicy {
            min_clusters: 1,
            max_clusters: 1,
            cluster_mass_threshold_ppm: 500_000,
            max_candidate_experts: 2,
            max_candidate_edges: 2,
            proposal_top_k: SemanticMorTopK::Two,
        };
        let hierarchical = hierarchy(policy)
            .route(&state, &registry, &engine, &[], &PreviousRoute::default())
            .expect("hierarchical route");

        let exhaustive_bridge = SemanticMorBridge::new(
            vec![
                SemanticRouterSignature {
                    router_id: RouterId(0),
                    signature: bits(&[0, 1, 2, 3]),
                },
                SemanticRouterSignature {
                    router_id: RouterId(1),
                    signature: bits(&[4, 5, 6, 7]),
                },
            ],
            vec![
                SemanticExpertSignature {
                    page_id: PageId(0),
                    signature: bits(&[0, 1, 2, 3]),
                },
                SemanticExpertSignature {
                    page_id: PageId(1),
                    signature: bits(&[0, 1, 2, 4]),
                },
                SemanticExpertSignature {
                    page_id: PageId(2),
                    signature: bits(&[4, 5, 6, 7]),
                },
                SemanticExpertSignature {
                    page_id: PageId(3),
                    signature: bits(&[3, 5, 6, 7]),
                },
            ],
            SemanticMorTopK::Two,
        )
        .expect("exhaustive bridge");
        let exhaustive = exhaustive_bridge
            .route(&state, &registry, &engine, &[], &PreviousRoute::default())
            .expect("exhaustive route");

        assert_eq!(hierarchical.selected_clusters.len(), 1);
        assert_eq!(hierarchical.selected_clusters[0].cluster_id, 10);
        assert_eq!(hierarchical.candidate_experts, vec![PageId(0), PageId(1)]);
        assert_eq!(hierarchical.accounting.expert_similarity_evaluations, 2);
        assert_eq!(
            hierarchical
                .accounting
                .exhaustive_expert_similarity_evaluations,
            4
        );
        assert_eq!(hierarchical.accounting.expert_edge_fraction_ppm, 500_000);
        assert_eq!(
            hierarchical.decision.selected_experts[0],
            exhaustive.decision.selected_experts[0]
        );
        assert_eq!(hierarchical.decision.selected_experts[0], PageId(0));
    }

    #[test]
    fn mass_threshold_can_expand_to_two_clusters() {
        let route = hierarchy(SemanticMorHierarchyPolicy {
            min_clusters: 1,
            max_clusters: 2,
            cluster_mass_threshold_ppm: 900_000,
            max_candidate_experts: 4,
            max_candidate_edges: 4,
            proposal_top_k: SemanticMorTopK::Two,
        })
        .route(
            &state(&[0, 1, 4, 5]),
            &registry(),
            &MixtureOfRouters::new(MorPolicy {
                min_routers: 1,
                max_routers: 2,
                router_mass_threshold_ppm: MOR_SCALE_PPM,
                ..MorPolicy::default()
            })
            .expect("engine"),
            &[],
            &PreviousRoute::default(),
        )
        .expect("route");

        assert_eq!(route.selected_clusters.len(), 2);
        assert_eq!(route.accounting.expert_similarity_evaluations, 4);
        assert_eq!(route.accounting.expert_edge_fraction_ppm, MOR_SCALE_PPM);
    }

    #[test]
    fn candidate_edge_budget_fails_closed() {
        let error = hierarchy(SemanticMorHierarchyPolicy {
            min_clusters: 1,
            max_clusters: 2,
            cluster_mass_threshold_ppm: MOR_SCALE_PPM,
            max_candidate_experts: 4,
            max_candidate_edges: 1,
            proposal_top_k: SemanticMorTopK::One,
        })
        .route(
            &state(&[0, 1, 2, 3]),
            &registry(),
            &MixtureOfRouters::new(MorPolicy::default()).expect("engine"),
            &[],
            &PreviousRoute::default(),
        )
        .expect_err("edge budget must fail");

        assert!(matches!(
            error,
            SemanticMorHierarchyError::CandidateEdgeBudget {
                actual: 2,
                maximum: 1
            }
        ));
    }

    #[test]
    fn specialization_registry_reuses_exact_hierarchy_partition() {
        let registry = hierarchy(SemanticMorHierarchyPolicy {
            min_clusters: 1,
            max_clusters: 2,
            cluster_mass_threshold_ppm: MOR_SCALE_PPM,
            max_candidate_experts: 4,
            max_candidate_edges: 4,
            proposal_top_k: SemanticMorTopK::Two,
        })
        .specialization_registry()
        .expect("specialization registry");

        assert_eq!(registry.expert_count(), 4);
        assert_eq!(registry.cluster_of(PageId(0)), Some(MorClusterId(10)));
        assert_eq!(registry.cluster_of(PageId(1)), Some(MorClusterId(10)));
        assert_eq!(registry.cluster_of(PageId(2)), Some(MorClusterId(20)));
        assert_eq!(registry.cluster_of(PageId(3)), Some(MorClusterId(20)));
    }

    #[test]
    fn every_reachable_expert_must_be_clustered() {
        let bridge = SemanticHierarchicalMorBridge::new(
            vec![
                (RouterId(0), bits(&[0, 1, 2, 3])),
                (RouterId(1), bits(&[4, 5, 6, 7])),
            ],
            vec![
                (PageId(0), bits(&[0, 1, 2, 3])),
                (PageId(1), bits(&[0, 1, 2, 4])),
                (PageId(2), bits(&[4, 5, 6, 7])),
                (PageId(3), bits(&[3, 5, 6, 7])),
            ],
            vec![SemanticMorClusterSignature {
                cluster_id: 10,
                signature: bits(&[0, 1, 2, 3]),
                experts: vec![PageId(0), PageId(1)],
            }],
            SemanticMorHierarchyPolicy {
                min_clusters: 1,
                max_clusters: 1,
                cluster_mass_threshold_ppm: MOR_SCALE_PPM,
                max_candidate_experts: 2,
                max_candidate_edges: 2,
                proposal_top_k: SemanticMorTopK::One,
            },
        )
        .expect("bridge");

        let error = bridge
            .route(
                &state(&[0, 1, 2, 3]),
                &registry(),
                &MixtureOfRouters::new(MorPolicy::default()).expect("engine"),
                &[],
                &PreviousRoute::default(),
            )
            .expect_err("coverage must fail");

        assert_eq!(
            error,
            SemanticMorHierarchyError::UnclusteredExpert(PageId(2))
        );
    }
}
