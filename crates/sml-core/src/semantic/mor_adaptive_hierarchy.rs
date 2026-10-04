//! Adaptive 1 -> 2 -> 4 widening for hierarchical SBG -> MOR routing.
//!
//! MOR-H2 reuses MOR-H1's bounded cluster pool and the unchanged final
//! MixtureOfRouters engine. It widens only from observable inference-time
//! signals: coarse cluster frontier margin, final MOR route margin, structural
//! MOR disagreement, and the SBG ambiguity field.
//!
//! No task label, expected expert, evaluator truth, or acceptance authorization
//! enters the widening policy. Exhaustive comparison is provided separately as
//! a research control and its work is never counted as native adaptive work.

use core::fmt;

use crate::{
    mor::{
        ExpertRuntimeHint, MixtureOfRouters, MorDecision, PreviousRoute, MOR_SCALE_PPM,
        MOR_SCORE_CEILING_PPM,
    },
    mor_registry::MorRouterRegistry,
    PageId,
};

use super::{
    mor_hierarchical::{
        SemanticHierarchicalMorBridge, SemanticHierarchicalMorRoute, SemanticMorClusterEvidence,
        SemanticMorHierarchyAccounting, SemanticMorHierarchyError, SemanticMorRouteContext,
    },
    SemanticState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticMorAdaptiveHierarchyPolicy {
    /// Widen when the selected/unselected coarse-cluster frontier is this narrow or narrower.
    pub cluster_margin_widen_ppm: u32,
    /// Widen when the final MOR top-1/top-2 score margin is this narrow or narrower.
    pub route_margin_widen_ppm: u32,
    /// Widen when native MOR structural disagreement reaches this level.
    pub disagreement_widen_ppm: u32,
    /// Widen when the fraction of active SBG ambiguity bits reaches this level.
    pub ambiguity_widen_ppm: u32,
    /// Hard cumulative budget across all adaptive stages.
    pub max_cumulative_expert_similarity_evaluations: usize,
    /// Hard cumulative source-registry edge budget across all adaptive stages.
    pub max_cumulative_candidate_edges: usize,
}

impl Default for SemanticMorAdaptiveHierarchyPolicy {
    fn default() -> Self {
        Self {
            // Experimental engineering defaults, not calibrated thresholds.
            cluster_margin_widen_ppm: 125_000,
            route_margin_widen_ppm: 50_000,
            disagreement_widen_ppm: 250_000,
            ambiguity_widen_ppm: 125_000,
            max_cumulative_expert_similarity_evaluations: 1_024,
            max_cumulative_candidate_edges: 1_024,
        }
    }
}

impl SemanticMorAdaptiveHierarchyPolicy {
    pub fn validate(self) -> Result<Self, SemanticMorAdaptiveHierarchyError> {
        for value in [
            self.cluster_margin_widen_ppm,
            self.disagreement_widen_ppm,
            self.ambiguity_widen_ppm,
        ] {
            if value > MOR_SCALE_PPM {
                return Err(SemanticMorAdaptiveHierarchyError::PolicyPpmOutOfRange(
                    value,
                ));
            }
        }
        if self.route_margin_widen_ppm > MOR_SCORE_CEILING_PPM {
            return Err(SemanticMorAdaptiveHierarchyError::RouteMarginOutOfRange(
                self.route_margin_widen_ppm,
            ));
        }
        if self.max_cumulative_expert_similarity_evaluations == 0 {
            return Err(SemanticMorAdaptiveHierarchyError::ZeroCumulativeExpertSimilarityBudget);
        }
        if self.max_cumulative_candidate_edges == 0 {
            return Err(SemanticMorAdaptiveHierarchyError::ZeroCumulativeEdgeBudget);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticMorWidenReason {
    CoarseClusterMargin,
    ExpertRouteMargin,
    StructuralDisagreement,
    SemanticAmbiguity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorAdaptiveStageReport {
    pub cluster_count: usize,
    pub primary_page: PageId,
    pub frontier_cluster_margin_ppm: u32,
    pub route_margin_ppm: u32,
    pub structural_disagreement_ppm: u32,
    pub ambiguity_ppm: u32,
    pub precision_escalation_requested: bool,
    pub widen_reasons: Vec<SemanticMorWidenReason>,
    pub widened: bool,
    pub accounting: SemanticMorHierarchyAccounting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorAdaptiveHierarchyReport {
    pub final_route: SemanticHierarchicalMorRoute,
    pub stages: Vec<SemanticMorAdaptiveStageReport>,
    pub total_cluster_similarity_evaluations: usize,
    pub cumulative_router_similarity_evaluations: usize,
    pub cumulative_expert_similarity_evaluations: usize,
    pub cumulative_candidate_edges: usize,
    pub stopped_at_cluster_count: usize,
    /// True when a widening reason remained but the fixed 1 -> 2 -> 4 schedule had no wider stage.
    pub exhausted_widening_schedule: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorAdaptiveExhaustiveControl {
    pub adaptive: SemanticMorAdaptiveHierarchyReport,
    pub exhaustive_route: SemanticHierarchicalMorRoute,
    pub adaptive_primary: PageId,
    pub exhaustive_primary: PageId,
    pub exhaustive_primary_in_adaptive_pool: bool,
    pub primary_matches_exhaustive: bool,
}

impl<const BITS: usize> SemanticHierarchicalMorBridge<BITS> {
    /// Runs the fixed 1 -> 2 -> 4 cluster schedule, stopping as soon as all
    /// observable widening signals clear.
    pub fn route_adaptive(
        &self,
        state: &SemanticState<BITS>,
        registry: &MorRouterRegistry,
        engine: &MixtureOfRouters,
        runtime_hints: &[ExpertRuntimeHint],
        previous: &PreviousRoute,
        policy: SemanticMorAdaptiveHierarchyPolicy,
    ) -> Result<SemanticMorAdaptiveHierarchyReport, SemanticMorAdaptiveHierarchyError> {
        let policy = policy.validate()?;
        let ranked_clusters = self
            .rank_clusters(state)
            .map_err(SemanticMorAdaptiveHierarchyError::Hierarchy)?;
        let ambiguity_ppm = ambiguity_fraction_ppm(state)?;
        let schedule = widening_schedule(ranked_clusters.len());

        let mut stages = Vec::with_capacity(schedule.len());
        let mut cumulative_router_similarity_evaluations = 0_usize;
        let mut cumulative_expert_similarity_evaluations = 0_usize;
        let mut cumulative_candidate_edges = 0_usize;
        let mut final_route = None;
        let mut exhausted_widening_schedule = false;

        for (stage_index, cluster_count) in schedule.iter().copied().enumerate() {
            let cluster_similarity_evaluations = if stage_index == 0 {
                self.cluster_count()
            } else {
                0
            };
            let route = self
                .route_ranked_prefix(
                    state,
                    SemanticMorRouteContext {
                        registry,
                        engine,
                        runtime_hints,
                        previous,
                    },
                    &ranked_clusters,
                    cluster_count,
                    cluster_similarity_evaluations,
                )
                .map_err(SemanticMorAdaptiveHierarchyError::Hierarchy)?;

            cumulative_router_similarity_evaluations = cumulative_router_similarity_evaluations
                .checked_add(route.accounting.router_similarity_evaluations)
                .ok_or(SemanticMorAdaptiveHierarchyError::ArithmeticOverflow)?;
            cumulative_expert_similarity_evaluations = cumulative_expert_similarity_evaluations
                .checked_add(route.accounting.expert_similarity_evaluations)
                .ok_or(SemanticMorAdaptiveHierarchyError::ArithmeticOverflow)?;
            cumulative_candidate_edges = cumulative_candidate_edges
                .checked_add(route.accounting.candidate_edges)
                .ok_or(SemanticMorAdaptiveHierarchyError::ArithmeticOverflow)?;

            if cumulative_expert_similarity_evaluations
                > policy.max_cumulative_expert_similarity_evaluations
            {
                return Err(
                    SemanticMorAdaptiveHierarchyError::CumulativeExpertSimilarityBudget {
                        actual: cumulative_expert_similarity_evaluations,
                        maximum: policy.max_cumulative_expert_similarity_evaluations,
                    },
                );
            }
            if cumulative_candidate_edges > policy.max_cumulative_candidate_edges {
                return Err(SemanticMorAdaptiveHierarchyError::CumulativeEdgeBudget {
                    actual: cumulative_candidate_edges,
                    maximum: policy.max_cumulative_candidate_edges,
                });
            }

            let primary_page = route
                .decision
                .ranked_experts
                .first()
                .map(|expert| expert.page_id)
                .ok_or(SemanticMorAdaptiveHierarchyError::EmptyDecision)?;
            let frontier_cluster_margin_ppm =
                frontier_cluster_margin_ppm(&ranked_clusters, cluster_count);
            let route_margin_ppm = decision_route_margin_ppm(&route.decision);
            let widen_reasons = widening_reasons(
                frontier_cluster_margin_ppm,
                route_margin_ppm,
                route.decision.structural_disagreement_ppm,
                ambiguity_ppm,
                cluster_count < ranked_clusters.len(),
                policy,
            );

            let has_next_stage = stage_index + 1 < schedule.len();
            let widened = has_next_stage && !widen_reasons.is_empty();
            if !has_next_stage && !widen_reasons.is_empty() {
                exhausted_widening_schedule = true;
            }

            stages.push(SemanticMorAdaptiveStageReport {
                cluster_count,
                primary_page,
                frontier_cluster_margin_ppm,
                route_margin_ppm,
                structural_disagreement_ppm: route.decision.structural_disagreement_ppm,
                ambiguity_ppm,
                precision_escalation_requested: route.decision.request_precision_escalation,
                widen_reasons,
                widened,
                accounting: route.accounting,
            });
            final_route = Some(route);

            if !widened {
                break;
            }
        }

        let final_route = final_route.ok_or(SemanticMorAdaptiveHierarchyError::EmptySchedule)?;
        let stopped_at_cluster_count = stages
            .last()
            .map(|stage| stage.cluster_count)
            .ok_or(SemanticMorAdaptiveHierarchyError::EmptySchedule)?;

        Ok(SemanticMorAdaptiveHierarchyReport {
            final_route,
            stages,
            total_cluster_similarity_evaluations: ranked_clusters.len(),
            cumulative_router_similarity_evaluations,
            cumulative_expert_similarity_evaluations,
            cumulative_candidate_edges,
            stopped_at_cluster_count,
            exhausted_widening_schedule,
        })
    }

    /// Research-only matched control. The exhaustive route is computed after the
    /// adaptive route and is deliberately excluded from adaptive work counters.
    pub fn route_adaptive_with_exhaustive_control(
        &self,
        state: &SemanticState<BITS>,
        registry: &MorRouterRegistry,
        engine: &MixtureOfRouters,
        runtime_hints: &[ExpertRuntimeHint],
        previous: &PreviousRoute,
        policy: SemanticMorAdaptiveHierarchyPolicy,
    ) -> Result<SemanticMorAdaptiveExhaustiveControl, SemanticMorAdaptiveHierarchyError> {
        let adaptive =
            self.route_adaptive(state, registry, engine, runtime_hints, previous, policy)?;
        let ranked_clusters = self
            .rank_clusters(state)
            .map_err(SemanticMorAdaptiveHierarchyError::Hierarchy)?;
        let exhaustive_route = self
            .route_ranked_prefix(
                state,
                SemanticMorRouteContext {
                    registry,
                    engine,
                    runtime_hints,
                    previous,
                },
                &ranked_clusters,
                ranked_clusters.len(),
                ranked_clusters.len(),
            )
            .map_err(SemanticMorAdaptiveHierarchyError::Hierarchy)?;

        let adaptive_primary = adaptive
            .final_route
            .decision
            .ranked_experts
            .first()
            .map(|expert| expert.page_id)
            .ok_or(SemanticMorAdaptiveHierarchyError::EmptyDecision)?;
        let exhaustive_primary = exhaustive_route
            .decision
            .ranked_experts
            .first()
            .map(|expert| expert.page_id)
            .ok_or(SemanticMorAdaptiveHierarchyError::EmptyDecision)?;
        let exhaustive_primary_in_adaptive_pool = adaptive
            .final_route
            .candidate_experts
            .contains(&exhaustive_primary);

        Ok(SemanticMorAdaptiveExhaustiveControl {
            adaptive,
            exhaustive_route,
            adaptive_primary,
            exhaustive_primary,
            exhaustive_primary_in_adaptive_pool,
            primary_matches_exhaustive: adaptive_primary == exhaustive_primary,
        })
    }
}

fn widening_schedule(available_clusters: usize) -> Vec<usize> {
    let mut schedule = Vec::with_capacity(3);
    for requested in [1_usize, 2, 4] {
        let count = requested.min(available_clusters);
        if count > 0 && schedule.last().copied() != Some(count) {
            schedule.push(count);
        }
    }
    schedule
}

fn ambiguity_fraction_ppm<const BITS: usize>(
    state: &SemanticState<BITS>,
) -> Result<u32, SemanticMorAdaptiveHierarchyError> {
    if BITS == 0 {
        return Err(SemanticMorAdaptiveHierarchyError::ZeroSemanticWidth);
    }
    let active = state.ambiguity.count_ones() as u128;
    let ppm = active
        .checked_mul(MOR_SCALE_PPM as u128)
        .ok_or(SemanticMorAdaptiveHierarchyError::ArithmeticOverflow)?
        / BITS as u128;
    u32::try_from(ppm).map_err(|_| SemanticMorAdaptiveHierarchyError::ArithmeticOverflow)
}

fn frontier_cluster_margin_ppm(
    ranked_clusters: &[SemanticMorClusterEvidence],
    selected_count: usize,
) -> u32 {
    if selected_count == 0 || selected_count >= ranked_clusters.len() {
        return MOR_SCALE_PPM;
    }
    ranked_clusters[selected_count - 1]
        .score_ppm
        .saturating_sub(ranked_clusters[selected_count].score_ppm)
}

fn decision_route_margin_ppm(decision: &MorDecision) -> u32 {
    match decision.ranked_experts.as_slice() {
        [first, second, ..] => first.final_score_ppm.saturating_sub(second.final_score_ppm),
        [_] => MOR_SCORE_CEILING_PPM,
        [] => 0,
    }
}

fn widening_reasons(
    frontier_cluster_margin_ppm: u32,
    route_margin_ppm: u32,
    structural_disagreement_ppm: u32,
    ambiguity_ppm: u32,
    has_unselected_clusters: bool,
    policy: SemanticMorAdaptiveHierarchyPolicy,
) -> Vec<SemanticMorWidenReason> {
    let mut reasons = Vec::with_capacity(4);
    if has_unselected_clusters && frontier_cluster_margin_ppm <= policy.cluster_margin_widen_ppm {
        reasons.push(SemanticMorWidenReason::CoarseClusterMargin);
    }
    if route_margin_ppm <= policy.route_margin_widen_ppm {
        reasons.push(SemanticMorWidenReason::ExpertRouteMargin);
    }
    if structural_disagreement_ppm >= policy.disagreement_widen_ppm {
        reasons.push(SemanticMorWidenReason::StructuralDisagreement);
    }
    if ambiguity_ppm >= policy.ambiguity_widen_ppm {
        reasons.push(SemanticMorWidenReason::SemanticAmbiguity);
    }
    reasons
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticMorAdaptiveHierarchyError {
    PolicyPpmOutOfRange(u32),
    RouteMarginOutOfRange(u32),
    ZeroCumulativeExpertSimilarityBudget,
    ZeroCumulativeEdgeBudget,
    CumulativeExpertSimilarityBudget { actual: usize, maximum: usize },
    CumulativeEdgeBudget { actual: usize, maximum: usize },
    ZeroSemanticWidth,
    EmptyDecision,
    EmptySchedule,
    ArithmeticOverflow,
    Hierarchy(SemanticMorHierarchyError),
}

impl fmt::Display for SemanticMorAdaptiveHierarchyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticMorAdaptiveHierarchyError {}

#[cfg(test)]
mod tests {
    use super::{
        widening_reasons, SemanticMorAdaptiveHierarchyError, SemanticMorAdaptiveHierarchyPolicy,
        SemanticMorWidenReason,
    };
    use crate::{
        mor::{
            MixtureOfRouters, MorPolicy, PreviousRoute, RouterId, MOR_SCALE_PPM,
            MOR_SCORE_CEILING_PPM,
        },
        mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
        semantic::{
            SemanticBitField, SemanticHierarchicalMorBridge, SemanticMorClusterSignature,
            SemanticMorHierarchyPolicy, SemanticMorTopK, SemanticState,
        },
        PageId,
    };

    fn bits(indices: &[usize]) -> SemanticBitField<8> {
        SemanticBitField::from_set_indices(indices).expect("bits")
    }

    fn state(routing: &[usize], ambiguity: &[usize]) -> SemanticState<8> {
        let mut state = SemanticState::zeroed().expect("state");
        state.routing = bits(routing);
        state.ambiguity = bits(ambiguity);
        state
    }

    fn registry() -> MorRouterRegistry {
        MorRouterRegistry::new(
            vec![
                RouterSpecialization {
                    router_id: RouterId(0),
                    experts: vec![PageId(0)],
                },
                RouterSpecialization {
                    router_id: RouterId(1),
                    experts: vec![PageId(1)],
                },
                RouterSpecialization {
                    router_id: RouterId(2),
                    experts: vec![PageId(2)],
                },
                RouterSpecialization {
                    router_id: RouterId(3),
                    experts: vec![PageId(3)],
                },
            ],
            MorRegistryPolicy {
                expert_count: 4,
                max_routers: 4,
                max_edges: 4,
                max_fanout: 1,
                require_full_coverage: true,
            },
        )
        .expect("registry")
    }

    fn bridge(second_signature: &[usize]) -> SemanticHierarchicalMorBridge<8> {
        SemanticHierarchicalMorBridge::new(
            vec![
                (RouterId(0), bits(&[0, 1, 2, 3])),
                (RouterId(1), bits(second_signature)),
                (RouterId(2), bits(&[4, 5, 6, 7])),
                (RouterId(3), bits(&[0, 2, 4, 6])),
            ],
            vec![
                (PageId(0), bits(&[0, 1, 2, 3])),
                (PageId(1), bits(second_signature)),
                (PageId(2), bits(&[4, 5, 6, 7])),
                (PageId(3), bits(&[0, 2, 4, 6])),
            ],
            vec![
                SemanticMorClusterSignature {
                    cluster_id: 0,
                    signature: bits(&[0, 1, 2, 3]),
                    experts: vec![PageId(0)],
                },
                SemanticMorClusterSignature {
                    cluster_id: 1,
                    signature: bits(second_signature),
                    experts: vec![PageId(1)],
                },
                SemanticMorClusterSignature {
                    cluster_id: 2,
                    signature: bits(&[4, 5, 6, 7]),
                    experts: vec![PageId(2)],
                },
                SemanticMorClusterSignature {
                    cluster_id: 3,
                    signature: bits(&[0, 2, 4, 6]),
                    experts: vec![PageId(3)],
                },
            ],
            SemanticMorHierarchyPolicy {
                min_clusters: 1,
                max_clusters: 4,
                cluster_mass_threshold_ppm: MOR_SCALE_PPM,
                max_candidate_experts: 4,
                max_candidate_edges: 4,
                proposal_top_k: SemanticMorTopK::One,
            },
        )
        .expect("bridge")
    }

    fn engine() -> MixtureOfRouters {
        MixtureOfRouters::new(MorPolicy {
            min_routers: 1,
            max_routers: 4,
            router_mass_threshold_ppm: MOR_SCALE_PPM,
            ..MorPolicy::default()
        })
        .expect("engine")
    }

    fn quiet_policy() -> SemanticMorAdaptiveHierarchyPolicy {
        SemanticMorAdaptiveHierarchyPolicy {
            cluster_margin_widen_ppm: 100_000,
            route_margin_widen_ppm: 0,
            disagreement_widen_ppm: MOR_SCALE_PPM,
            ambiguity_widen_ppm: MOR_SCALE_PPM,
            max_cumulative_expert_similarity_evaluations: 16,
            max_cumulative_candidate_edges: 16,
        }
    }

    #[test]
    fn confident_route_stops_after_one_cluster() {
        let report = bridge(&[4, 5, 6, 7])
            .route_adaptive(
                &state(&[0, 1, 2, 3], &[]),
                &registry(),
                &engine(),
                &[],
                &PreviousRoute::default(),
                quiet_policy(),
            )
            .expect("adaptive route");

        assert_eq!(report.stages.len(), 1);
        assert_eq!(report.stopped_at_cluster_count, 1);
        assert_eq!(report.final_route.candidate_experts, vec![PageId(0)]);
        assert_eq!(report.cumulative_expert_similarity_evaluations, 1);
        assert!(!report.exhausted_widening_schedule);
    }

    #[test]
    fn narrow_coarse_frontier_widens_one_to_two_then_stops() {
        let policy = SemanticMorAdaptiveHierarchyPolicy {
            cluster_margin_widen_ppm: 300_000,
            ..quiet_policy()
        };
        let report = bridge(&[0, 1, 2, 4])
            .route_adaptive(
                &state(&[0, 1, 2, 3], &[]),
                &registry(),
                &engine(),
                &[],
                &PreviousRoute::default(),
                policy,
            )
            .expect("adaptive route");

        assert_eq!(report.stages.len(), 2);
        assert_eq!(report.stages[0].cluster_count, 1);
        assert!(report.stages[0]
            .widen_reasons
            .contains(&SemanticMorWidenReason::CoarseClusterMargin));
        assert!(report.stages[0].widened);
        assert_eq!(report.stages[1].cluster_count, 2);
        assert!(!report.stages[1].widened);
        assert_eq!(report.stopped_at_cluster_count, 2);
        assert_eq!(report.cumulative_expert_similarity_evaluations, 3);
    }

    #[test]
    fn semantic_ambiguity_can_force_full_one_two_four_schedule() {
        let policy = SemanticMorAdaptiveHierarchyPolicy {
            ambiguity_widen_ppm: 100_000,
            cluster_margin_widen_ppm: 0,
            ..quiet_policy()
        };
        let report = bridge(&[4, 5, 6, 7])
            .route_adaptive(
                &state(&[0, 1, 2, 3], &[0, 1]),
                &registry(),
                &engine(),
                &[],
                &PreviousRoute::default(),
                policy,
            )
            .expect("adaptive route");

        assert_eq!(
            report
                .stages
                .iter()
                .map(|stage| stage.cluster_count)
                .collect::<Vec<_>>(),
            vec![1, 2, 4]
        );
        assert!(report.stages[0]
            .widen_reasons
            .contains(&SemanticMorWidenReason::SemanticAmbiguity));
        assert!(report.stages[1]
            .widen_reasons
            .contains(&SemanticMorWidenReason::SemanticAmbiguity));
        assert!(report.exhausted_widening_schedule);
        assert_eq!(report.cumulative_expert_similarity_evaluations, 7);
    }

    #[test]
    fn structural_disagreement_is_an_independent_widen_reason() {
        let policy = SemanticMorAdaptiveHierarchyPolicy {
            disagreement_widen_ppm: 250_000,
            ..quiet_policy()
        };
        let reasons = widening_reasons(
            MOR_SCALE_PPM,
            MOR_SCORE_CEILING_PPM,
            500_000,
            0,
            true,
            policy,
        );
        assert_eq!(
            reasons,
            vec![SemanticMorWidenReason::StructuralDisagreement]
        );
    }

    #[test]
    fn cumulative_work_budget_fails_closed_during_widening() {
        let policy = SemanticMorAdaptiveHierarchyPolicy {
            cluster_margin_widen_ppm: 300_000,
            max_cumulative_expert_similarity_evaluations: 2,
            max_cumulative_candidate_edges: 2,
            ..quiet_policy()
        };
        let error = bridge(&[0, 1, 2, 4])
            .route_adaptive(
                &state(&[0, 1, 2, 3], &[]),
                &registry(),
                &engine(),
                &[],
                &PreviousRoute::default(),
                policy,
            )
            .expect_err("cumulative budget must fail");

        assert_eq!(
            error,
            SemanticMorAdaptiveHierarchyError::CumulativeExpertSimilarityBudget {
                actual: 3,
                maximum: 2
            }
        );
    }

    #[test]
    fn exhaustive_control_reports_primary_recall_without_polluting_adaptive_cost() {
        let control = bridge(&[4, 5, 6, 7])
            .route_adaptive_with_exhaustive_control(
                &state(&[0, 1, 2, 3], &[]),
                &registry(),
                &engine(),
                &[],
                &PreviousRoute::default(),
                quiet_policy(),
            )
            .expect("control");

        assert_eq!(control.adaptive.stopped_at_cluster_count, 1);
        assert_eq!(control.adaptive_primary, PageId(0));
        assert_eq!(control.exhaustive_primary, PageId(0));
        assert!(control.exhaustive_primary_in_adaptive_pool);
        assert!(control.primary_matches_exhaustive);
        assert_eq!(control.adaptive.cumulative_expert_similarity_evaluations, 1);
        assert_eq!(
            control
                .exhaustive_route
                .accounting
                .expert_similarity_evaluations,
            4
        );
    }

    #[test]
    fn invalid_policy_ranges_fail_closed() {
        assert_eq!(
            SemanticMorAdaptiveHierarchyPolicy {
                cluster_margin_widen_ppm: MOR_SCALE_PPM + 1,
                ..SemanticMorAdaptiveHierarchyPolicy::default()
            }
            .validate(),
            Err(SemanticMorAdaptiveHierarchyError::PolicyPpmOutOfRange(
                MOR_SCALE_PPM + 1
            ))
        );
        assert_eq!(
            SemanticMorAdaptiveHierarchyPolicy {
                route_margin_widen_ppm: MOR_SCORE_CEILING_PPM + 1,
                ..SemanticMorAdaptiveHierarchyPolicy::default()
            }
            .validate(),
            Err(SemanticMorAdaptiveHierarchyError::RouteMarginOutOfRange(
                MOR_SCORE_CEILING_PPM + 1
            ))
        );
    }
}
