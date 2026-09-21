//! Sparse specialization registry for SML Mixture-of-Routers.
//!
//! The registry makes the router-to-expert graph explicit and auditable. A
//! router may only propose experts declared in its native SML specialization.
//! Complete coverage and graph-size budgets can be enforced before routing.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    mor::{
        ExpertRuntimeHint, MixtureOfRouters, MorDecision, MorError, PreviousRoute, RouterId,
        RouterInput, MOR_SCALE_PPM,
    },
    PageId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorRegistryPolicy {
    pub expert_count: u32,
    pub max_routers: usize,
    pub max_edges: usize,
    pub max_fanout: usize,
    pub require_full_coverage: bool,
}

impl MorRegistryPolicy {
    pub fn validate(self) -> Result<Self, MorRegistryError> {
        if self.expert_count == 0 {
            return Err(MorRegistryError::ZeroExperts);
        }
        if self.max_routers == 0 {
            return Err(MorRegistryError::ZeroRouterBudget);
        }
        if self.max_edges == 0 {
            return Err(MorRegistryError::ZeroEdgeBudget);
        }
        if self.max_fanout == 0 {
            return Err(MorRegistryError::ZeroFanoutBudget);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouterSpecialization {
    pub router_id: RouterId,
    pub experts: Vec<PageId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorRegistryAccounting {
    pub router_count: usize,
    pub expert_count: u32,
    pub covered_experts: u32,
    pub edge_count: usize,
    pub dense_edge_count: u64,
    pub max_fanout: usize,
    pub average_fanout_milli: u32,
    pub coverage_ppm: u32,
    pub edge_density_ppm: u32,
    pub sparsity_ppm: u32,
    pub overlap_edges: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorRouterRegistry {
    policy: MorRegistryPolicy,
    specializations: Vec<RouterSpecialization>,
    index: BTreeMap<RouterId, usize>,
    accounting: MorRegistryAccounting,
}

impl MorRouterRegistry {
    pub fn new(
        specializations: Vec<RouterSpecialization>,
        policy: MorRegistryPolicy,
    ) -> Result<Self, MorRegistryError> {
        let policy = policy.validate()?;
        if specializations.is_empty() {
            return Err(MorRegistryError::EmptyRegistry);
        }
        if specializations.len() > policy.max_routers {
            return Err(MorRegistryError::RouterBudget {
                actual: specializations.len(),
                maximum: policy.max_routers,
            });
        }

        let mut index = BTreeMap::new();
        let mut covered = BTreeSet::new();
        let mut edge_count = 0_usize;
        let mut max_fanout = 0_usize;

        for (position, specialization) in specializations.iter().enumerate() {
            if index.insert(specialization.router_id, position).is_some() {
                return Err(MorRegistryError::DuplicateRouter(specialization.router_id));
            }
            if specialization.experts.is_empty() {
                return Err(MorRegistryError::EmptySpecialization(
                    specialization.router_id,
                ));
            }
            if specialization.experts.len() > policy.max_fanout {
                return Err(MorRegistryError::FanoutBudget {
                    router_id: specialization.router_id,
                    actual: specialization.experts.len(),
                    maximum: policy.max_fanout,
                });
            }

            let mut local = BTreeSet::new();
            for page in &specialization.experts {
                if page.0 >= policy.expert_count {
                    return Err(MorRegistryError::ExpertOutOfRange {
                        router_id: specialization.router_id,
                        page_id: *page,
                        expert_count: policy.expert_count,
                    });
                }
                if !local.insert(page.0) {
                    return Err(MorRegistryError::DuplicateEdge {
                        router_id: specialization.router_id,
                        page_id: *page,
                    });
                }
                covered.insert(page.0);
            }

            edge_count = edge_count
                .checked_add(specialization.experts.len())
                .ok_or(MorRegistryError::ArithmeticOverflow)?;
            if edge_count > policy.max_edges {
                return Err(MorRegistryError::EdgeBudget {
                    actual: edge_count,
                    maximum: policy.max_edges,
                });
            }
            max_fanout = max_fanout.max(specialization.experts.len());
        }

        if policy.require_full_coverage && covered.len() != policy.expert_count as usize {
            for page in 0..policy.expert_count {
                if !covered.contains(&page) {
                    return Err(MorRegistryError::UncoveredExpert(PageId(page)));
                }
            }
        }

        let dense_edge_count = (specializations.len() as u64)
            .checked_mul(policy.expert_count as u64)
            .ok_or(MorRegistryError::ArithmeticOverflow)?;
        let edge_density_ppm = ratio_ppm(edge_count as u64, dense_edge_count);
        let coverage_ppm = ratio_ppm(covered.len() as u64, policy.expert_count as u64);
        let average_fanout_milli =
            ((edge_count as u128 * 1_000) / specializations.len() as u128) as u32;
        let overlap_edges = edge_count.saturating_sub(covered.len());
        let router_count = specializations.len();

        Ok(Self {
            policy,
            specializations,
            index,
            accounting: MorRegistryAccounting {
                router_count,
                expert_count: policy.expert_count,
                covered_experts: covered.len() as u32,
                edge_count,
                dense_edge_count,
                max_fanout,
                average_fanout_milli,
                coverage_ppm,
                edge_density_ppm,
                sparsity_ppm: MOR_SCALE_PPM.saturating_sub(edge_density_ppm),
                overlap_edges,
            },
        })
    }

    #[must_use]
    pub const fn policy(&self) -> MorRegistryPolicy {
        self.policy
    }

    #[must_use]
    pub const fn accounting(&self) -> MorRegistryAccounting {
        self.accounting
    }

    #[must_use]
    pub fn specializations(&self) -> &[RouterSpecialization] {
        &self.specializations
    }

    #[must_use]
    pub fn allowed_experts(&self, router_id: RouterId) -> Option<&[PageId]> {
        self.index
            .get(&router_id)
            .map(|position| self.specializations[*position].experts.as_slice())
    }

    pub fn validate_complete_inputs(
        &self,
        routers: &[RouterInput],
        runtime_hints: &[ExpertRuntimeHint],
    ) -> Result<(), MorRegistryError> {
        if routers.len() != self.specializations.len() {
            return Err(MorRegistryError::IncompleteRouterSet {
                expected: self.specializations.len(),
                actual: routers.len(),
            });
        }

        let mut seen = BTreeSet::new();
        for router in routers {
            let Some(allowed) = self.allowed_experts(router.router_id) else {
                return Err(MorRegistryError::UnknownRouter(router.router_id));
            };
            if !seen.insert(router.router_id) {
                return Err(MorRegistryError::DuplicateRouterInput(router.router_id));
            }

            for proposal in &router.proposals {
                if !allowed.contains(&proposal.page_id) {
                    return Err(MorRegistryError::UnauthorizedProposal {
                        router_id: router.router_id,
                        page_id: proposal.page_id,
                    });
                }
            }
        }

        for specialization in &self.specializations {
            if !seen.contains(&specialization.router_id) {
                return Err(MorRegistryError::MissingRouter(specialization.router_id));
            }
        }

        let mut hint_pages = BTreeSet::new();
        for hint in runtime_hints {
            if hint.page_id.0 >= self.policy.expert_count {
                return Err(MorRegistryError::RuntimeHintOutOfRange {
                    page_id: hint.page_id,
                    expert_count: self.policy.expert_count,
                });
            }
            if !hint_pages.insert(hint.page_id.0) {
                return Err(MorRegistryError::Mor(MorError::DuplicateRuntimeHint(
                    hint.page_id,
                )));
            }
        }

        Ok(())
    }

    pub fn route(
        &self,
        engine: &MixtureOfRouters,
        routers: &[RouterInput],
        runtime_hints: &[ExpertRuntimeHint],
        previous: &PreviousRoute,
    ) -> Result<MorDecision, MorRegistryError> {
        self.validate_complete_inputs(routers, runtime_hints)?;
        engine
            .route(routers, runtime_hints, previous)
            .map_err(MorRegistryError::Mor)
    }
}

fn ratio_ppm(numerator: u64, denominator: u64) -> u32 {
    if denominator == 0 {
        return 0;
    }
    ((numerator as u128 * MOR_SCALE_PPM as u128) / denominator as u128) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorRegistryError {
    ZeroExperts,
    ZeroRouterBudget,
    ZeroEdgeBudget,
    ZeroFanoutBudget,
    EmptyRegistry,
    RouterBudget {
        actual: usize,
        maximum: usize,
    },
    EdgeBudget {
        actual: usize,
        maximum: usize,
    },
    FanoutBudget {
        router_id: RouterId,
        actual: usize,
        maximum: usize,
    },
    DuplicateRouter(RouterId),
    EmptySpecialization(RouterId),
    ExpertOutOfRange {
        router_id: RouterId,
        page_id: PageId,
        expert_count: u32,
    },
    DuplicateEdge {
        router_id: RouterId,
        page_id: PageId,
    },
    UncoveredExpert(PageId),
    IncompleteRouterSet {
        expected: usize,
        actual: usize,
    },
    UnknownRouter(RouterId),
    DuplicateRouterInput(RouterId),
    MissingRouter(RouterId),
    UnauthorizedProposal {
        router_id: RouterId,
        page_id: PageId,
    },
    RuntimeHintOutOfRange {
        page_id: PageId,
        expert_count: u32,
    },
    ArithmeticOverflow,
    Mor(MorError),
}

impl fmt::Display for MorRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorRegistryError {}
