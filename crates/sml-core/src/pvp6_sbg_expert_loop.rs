//! Native SBG rewrite -> MOR -> selected ANF bodies, with owned-error rollback.
//! Clone must isolate mutable state; external effects are not transactional.
use crate::{
    pvp6_sbg_atlas::{SbgAtlasError, SbgExpertAtlas, SbgExpertWork},
    pvp6_sbg_experts::SbgExpertEvidence,
    pvp6_sbg_routing::{RoutedSemantic, RoutedSemanticError},
    semantic::{SemanticRewriteStep, SemanticRewriter, SemanticState, SemanticStateDelta},
    PageId,
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgExecutedCycle {
    pub depth: u32,
    pub selected: Vec<PageId>,
    pub pages: Vec<SbgExpertEvidence>,
    pub delta: SemanticStateDelta,
    pub work: SbgExpertWork,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedExpertSemantic<K, const S: usize, const D: usize> {
    routed: RoutedSemantic<K, S>,
    atlas: SbgExpertAtlas<S, D>,
    budget: SbgExpertWork,
    usage: SbgExpertWork,
    cycles: Vec<SbgExecutedCycle>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutedExpertError<E> {
    Depth,
    Budget,
    Overflow,
    Allocation,
    Routing(RoutedSemanticError<E>),
    Expert(SbgAtlasError),
}
impl<K: SemanticRewriter<S> + Clone, const S: usize, const D: usize> RoutedExpertSemantic<K, S, D> {
    pub fn new(
        routed: RoutedSemantic<K, S>,
        atlas: SbgExpertAtlas<S, D>,
        budget: SbgExpertWork,
    ) -> Result<Self, RoutedExpertError<K::Error>> {
        if !routed.cycles().is_empty() {
            return Err(RoutedExpertError::Depth);
        }
        Ok(Self {
            routed,
            atlas,
            budget,
            usage: SbgExpertWork::default(),
            cycles: vec![],
        })
    }
    pub fn replace_expert(
        &mut self,
        id: PageId,
        expected_revision: u64,
        bank: crate::pvp6_executor::Pvp6Bank<D>,
    ) -> Result<u64, RoutedExpertError<K::Error>> {
        self.atlas
            .replace_bank(id, expected_revision, bank)
            .map_err(RoutedExpertError::Expert)
    }
    #[must_use]
    pub fn routed(&self) -> &RoutedSemantic<K, S> {
        &self.routed
    }
    #[must_use]
    pub fn atlas(&self) -> &SbgExpertAtlas<S, D> {
        &self.atlas
    }
    #[must_use]
    pub fn cycles(&self) -> &[SbgExecutedCycle] {
        &self.cycles
    }
    #[must_use]
    pub fn usage(&self) -> SbgExpertWork {
        self.usage
    }
}
impl<K: SemanticRewriter<S> + Clone, const S: usize, const D: usize> SemanticRewriter<S>
    for RoutedExpertSemantic<K, S, D>
{
    type Error = RoutedExpertError<K::Error>;
    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        if usize::try_from(depth).ok() != self.cycles.len().checked_add(1) {
            return Err(RoutedExpertError::Depth);
        }
        let mut routed = self.routed.clone();
        let inner = routed
            .rewrite(state, depth)
            .map_err(RoutedExpertError::Routing)?;
        let decision = &routed
            .cycles()
            .last()
            .ok_or(RoutedExpertError::Depth)?
            .route
            .decision;
        let work = self
            .atlas
            .work_for(&decision.selected_experts)
            .map_err(RoutedExpertError::Expert)?;
        let usage = SbgExpertWork {
            pages: self
                .usage
                .pages
                .checked_add(work.pages)
                .ok_or(RoutedExpertError::Overflow)?,
            gate_queries: self
                .usage
                .gate_queries
                .checked_add(work.gate_queries)
                .ok_or(RoutedExpertError::Overflow)?,
            retained_words: self
                .usage
                .retained_words
                .checked_add(work.retained_words)
                .ok_or(RoutedExpertError::Overflow)?,
        };
        if usage.pages > self.budget.pages
            || usage.gate_queries > self.budget.gate_queries
            || usage.retained_words > self.budget.retained_words
        {
            return Err(RoutedExpertError::Budget);
        }
        let batch = self
            .atlas
            .execute(&inner.state, decision)
            .map_err(RoutedExpertError::Expert)?;
        self.cycles
            .try_reserve(1)
            .map_err(|_| RoutedExpertError::Allocation)?;
        self.cycles.push(SbgExecutedCycle {
            depth,
            selected: decision.selected_experts.clone(),
            pages: batch.pages,
            delta: SemanticStateDelta::between(state, &batch.state),
            work,
        });
        self.routed = routed;
        self.usage = usage;
        Ok(SemanticRewriteStep {
            state: batch.state,
            stability_blocked: inner.stability_blocked,
        })
    }
}
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use crate::{
        mor::{MixtureOfRouters, MorPolicy, RouterId},
        mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
        pascal_vector_projection::Pvp6PopulationProfile,
        pvp6_executor::{Pvp6Bank, Pvp6Gate},
        pvp6_sbg::{SbgAnfPlan, SbgCoordinate, SbgField},
        pvp6_sbg_experts::SbgAnfExpert,
        pvp6_sbg_routing::SbgRoutingContext,
        semantic::{
            SemanticBitField, SemanticExpertSignature, SemanticMorBridge, SemanticMorTopK,
            SemanticRouterSignature,
        },
    };
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Identity {
        pub calls: u32,
    }
    impl SemanticRewriter<8> for Identity {
        type Error = ();
        fn rewrite(
            &mut self,
            state: &SemanticState<8>,
            _: u32,
        ) -> Result<SemanticRewriteStep<8>, ()> {
            self.calls += 1;
            Ok(SemanticRewriteStep {
                state: state.clone(),
                stability_blocked: false,
            })
        }
    }
    pub fn context() -> SbgRoutingContext<8> {
        let registry = MorRouterRegistry::new(
            vec![RouterSpecialization {
                router_id: RouterId(0),
                experts: vec![PageId(0), PageId(1)],
            }],
            MorRegistryPolicy {
                expert_count: 2,
                max_routers: 1,
                max_edges: 2,
                max_fanout: 2,
                require_full_coverage: true,
            },
        )
        .unwrap();
        let bridge = SemanticMorBridge::new(
            vec![SemanticRouterSignature {
                router_id: RouterId(0),
                signature: SemanticBitField::from_set_indices(&[0, 1, 2, 3]).unwrap(),
            }],
            vec![
                SemanticExpertSignature {
                    page_id: PageId(0),
                    signature: SemanticBitField::zeroed().unwrap(),
                },
                SemanticExpertSignature {
                    page_id: PageId(1),
                    signature: SemanticBitField::ones().unwrap(),
                },
            ],
            SemanticMorTopK::One,
        )
        .unwrap();
        SbgRoutingContext {
            bridge,
            registry,
            engine: MixtureOfRouters::new(MorPolicy::default()).unwrap(),
            runtime_hints: vec![],
        }
    }
    pub fn page(id: u32, terms: &[u16]) -> SbgAnfExpert<8, 64> {
        let plan = SbgAnfPlan::new(
            vec![(0..6)
                .map(|bit| SbgCoordinate {
                    field: SbgField::Value,
                    bit,
                })
                .collect()],
            vec![SbgCoordinate {
                field: SbgField::Value,
                bit: 0,
            }],
        )
        .unwrap();
        SbgAnfExpert::new(
            PageId(id),
            Pvp6Bank::new(vec![Pvp6Gate::from_monomials(terms).unwrap()]).unwrap(),
            plan,
            Pvp6PopulationProfile::AffineUniform,
        )
        .unwrap()
    }
    pub fn kernel(max: u32, budget: SbgExpertWork) -> RoutedExpertSemantic<Identity, 8, 64> {
        let routed = RoutedSemantic::new(Identity { calls: 0 }, context(), max).unwrap();
        RoutedExpertSemantic::new(
            routed,
            SbgExpertAtlas::new(vec![page(0, &[0]), page(1, &[1])], 2).unwrap(),
            budget,
        )
        .unwrap()
    }
    pub fn budget() -> SbgExpertWork {
        SbgExpertWork {
            pages: 8,
            gate_queries: 8,
            retained_words: 16,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::{
        semantic::{run_semantic_recurrent, SemanticHaltReason},
        HaltingPolicy,
    };
    #[test]
    fn selected_page_body_is_executed_and_native_halting_observes_its_output() {
        let mut k = kernel(8, budget());
        let initial = SemanticState::zeroed().unwrap();
        let out = run_semantic_recurrent(
            initial,
            &mut k,
            HaltingPolicy {
                min_depth: 1,
                max_depth: 8,
                stable_hamming_threshold: 0,
            },
        )
        .unwrap();
        assert!(out.state.value.get(0).unwrap());
        assert_eq!(out.depth, 2);
        assert_eq!(out.halt_reason, SemanticHaltReason::Stable);
        assert_eq!(k.cycles()[0].selected, vec![PageId(0)]);
        assert_eq!(k.cycles()[0].pages[0].outputs, vec![1]);
        assert_eq!(k.routed().inner().calls, 2);
        assert_eq!(
            k.usage(),
            SbgExpertWork {
                pages: 2,
                gate_queries: 2,
                retained_words: 4
            }
        );
    }
    #[test]
    fn missing_page_rolls_back_inner_calls_route_history_and_expert_trace() {
        let routed = RoutedSemantic::new(Identity { calls: 0 }, context(), 8).unwrap();
        let mut k = RoutedExpertSemantic::new(
            routed,
            SbgExpertAtlas::new(vec![page(1, &[1])], 1).unwrap(),
            budget(),
        )
        .unwrap();
        let before = k.clone();
        assert!(matches!(
            k.rewrite(&SemanticState::zeroed().unwrap(), 1),
            Err(RoutedExpertError::Expert(SbgAtlasError::MissingPage(
                PageId(0)
            )))
        ));
        assert_eq!(k, before);
    }
    #[test]
    fn all_budget_axes_and_duplicate_depth_fail_without_mutation() {
        for budget in [
            SbgExpertWork {
                pages: 0,
                gate_queries: 8,
                retained_words: 16,
            },
            SbgExpertWork {
                pages: 8,
                gate_queries: 0,
                retained_words: 16,
            },
            SbgExpertWork {
                pages: 8,
                gate_queries: 8,
                retained_words: 1,
            },
        ] {
            let mut k = kernel(8, budget);
            let before = k.clone();
            assert_eq!(
                k.rewrite(&SemanticState::zeroed().unwrap(), 1),
                Err(RoutedExpertError::Budget)
            );
            assert_eq!(k, before);
        }
        let mut k = kernel(8, budget());
        let state = SemanticState::zeroed().unwrap();
        k.rewrite(&state, 1).unwrap();
        let before = k.clone();
        assert_eq!(k.rewrite(&state, 1), Err(RoutedExpertError::Depth));
        assert_eq!(k, before);
    }
}
