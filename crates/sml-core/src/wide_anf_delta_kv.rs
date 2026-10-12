//! Atomic wide-sparse ANF page execution and current-route Delta-KV recurrence.
//! No online memory update or physical resource-bound claim.
use crate::{
    delta_kv_dynamic::{
        DeltaKvDynamicCycleEvidence, DeltaKvDynamicRekeyError, DeltaKvDynamicRekeyKernel,
    },
    wide_anf_routed::{WideRoutedError, WideRoutedExperts},
    semantic::{SemanticRewriteStep, SemanticRewriter, SemanticState},
    PageId,
};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WideCoupledCycle<const S: usize> {
    pub depth: u32,
    pub selected: Vec<PageId>,
    pub post_expert: SemanticState<S>,
    pub output: SemanticState<S>,
    pub memory: DeltaKvDynamicCycleEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WideCoupledError<E> {
    Depth,
    Budget,
    Experts(WideRoutedError<E>),
    Memory(DeltaKvDynamicRekeyError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WideAnfDeltaKvLoop<K, const S: usize, const Q: usize> {
    experts: WideRoutedExperts<K, S>,
    memory: DeltaKvDynamicRekeyKernel<S, Q>,
    max_cycles: u32,
    cycles: Arc<Vec<WideCoupledCycle<S>>>,
}

impl<K: SemanticRewriter<S> + Clone, const S: usize, const Q: usize> WideAnfDeltaKvLoop<K, S, Q> {
    pub fn new(
        experts: WideRoutedExperts<K, S>,
        memory: DeltaKvDynamicRekeyKernel<S, Q>,
        max_cycles: u32,
    ) -> Result<Self, WideCoupledError<K::Error>> {
        if max_cycles == 0
            || !experts.cycles().is_empty()
            || !memory.cycle_evidence().is_empty()
            || experts.routed().max_cycles() < max_cycles
        {
            return Err(WideCoupledError::Budget);
        }
        Ok(Self {
            experts,
            memory,
            max_cycles,
            cycles: Arc::new(Vec::new()),
        })
    }
    pub fn cycles(&self) -> &[WideCoupledCycle<S>] {
        &self.cycles
    }
    pub fn experts(&self) -> &WideRoutedExperts<K, S> {
        &self.experts
    }
    pub fn memory(&self) -> &DeltaKvDynamicRekeyKernel<S, Q> {
        &self.memory
    }
}

impl<K: SemanticRewriter<S> + Clone, const S: usize, const Q: usize> SemanticRewriter<S>
    for WideAnfDeltaKvLoop<K, S, Q>
{
    type Error = WideCoupledError<K::Error>;
    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        if usize::try_from(depth).ok() != self.cycles.len().checked_add(1) {
            return Err(WideCoupledError::Depth);
        }
        if depth > self.max_cycles {
            return Err(WideCoupledError::Budget);
        }
        let mut experts = self.experts.clone();
        let rewritten = experts
            .rewrite(state, depth)
            .map_err(WideCoupledError::Experts)?;
        let decision = &experts
            .routed()
            .cycles()
            .last()
            .ok_or(WideCoupledError::Depth)?
            .route
            .decision;
        let mut memory = self.memory.clone();
        let read = memory
            .rewrite_with_mor(&rewritten.state, depth, decision)
            .map_err(WideCoupledError::Memory)?;
        let evidence = memory
            .cycle_evidence()
            .last()
            .ok_or(WideCoupledError::Depth)?
            .clone();
        let mut cycles = self.cycles.clone();
        Arc::make_mut(&mut cycles)
            .try_reserve(1)
            .map_err(|_| WideCoupledError::Budget)?;
        Arc::make_mut(&mut cycles).push(WideCoupledCycle {
            depth,
            selected: decision.selected_experts.clone(),
            post_expert: rewritten.state,
            output: read.state.clone(),
            memory: evidence,
        });
        self.experts = experts;
        self.memory = memory;
        self.cycles = cycles;
        Ok(SemanticRewriteStep {
            state: read.state,
            stability_blocked: rewritten.stability_blocked || read.stability_blocked,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        anf_wide_sparse::{WideAnfGate, WideAnfLimits},
        delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_SCALE},
        delta_kv_recurrent::DeltaKvRecurrentPolicy,
        delta_kv_semantic::{
            DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector,
            DeltaKvSourceProvenance,
        },
        mor::{MixtureOfRouters, MorPolicy, PreviousRoute, RouterId},
        mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
        pvp6_sbg::{SbgCoordinate, SbgField},
        pvp6_sbg_routing::{RoutedSemantic, SbgRoutingContext},
        semantic::{
            SemanticBitField, SemanticExpertSignature, SemanticMorBridge, SemanticMorTopK,
            SemanticRewriteStep, SemanticRouterSignature,
        },
        wide_anf_selected_pages::{WideAnfPage, WideAnfPageAtlas},
    };

    const S: usize = 1024;
    const Q: usize = 8;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Identity {
        calls: u32,
    }

    impl SemanticRewriter<S> for Identity {
        type Error = ();
        fn rewrite(
            &mut self,
            state: &SemanticState<S>,
            _: u32,
        ) -> Result<SemanticRewriteStep<S>, ()> {
            self.calls += 1;
            Ok(SemanticRewriteStep {
                state: state.clone(),
                stability_blocked: false,
            })
        }
    }

    fn limits() -> WideAnfLimits {
        WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
    }

    fn context() -> SbgRoutingContext<S> {
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

    fn atlas() -> WideAnfPageAtlas<S> {
        let all_k1024 = WideAnfGate::from_monomials(
            (0..S).collect(),
            vec![(0..S).collect()],
            S,
            limits(),
        )
        .unwrap();
        let one = WideAnfGate::from_monomials(
            vec![0],
            vec![vec![]],
            1,
            limits(),
        )
        .unwrap();
        WideAnfPageAtlas::new(
            vec![
                WideAnfPage {
                    id: PageId(0),
                    gates: vec![all_k1024],
                    inputs: (0..S)
                        .map(|bit| SbgCoordinate {
                            field: SbgField::Value,
                            bit,
                        })
                        .collect(),
                    outputs: vec![SbgCoordinate {
                        field: SbgField::Meaning,
                        bit: 0,
                    }],
                },
                WideAnfPage {
                    id: PageId(1),
                    gates: vec![one],
                    inputs: vec![SbgCoordinate {
                        field: SbgField::Value,
                        bit: 0,
                    }],
                    outputs: vec![SbgCoordinate {
                        field: SbgField::Meaning,
                        bit: 1,
                    }],
                },
            ],
            2,
        )
        .unwrap()
        .with_scratch_word_budget(1025)
        .unwrap()
    }

    fn initial() -> SemanticState<S> {
        let mut state = SemanticState::<S>::zeroed().unwrap();
        for bit in 0..S {
            state.value.set(bit, true).unwrap();
        }
        state
    }

    fn memory(invalid_provenance: bool) -> DeltaKvDynamicRekeyKernel<S, Q> {
        let initial = initial();
        let context = context();
        let initial_decision = context
            .bridge
            .route(
                &initial,
                &context.registry,
                &context.engine,
                &[],
                &PreviousRoute::default(),
            )
            .unwrap()
            .decision;
        let projector = DeltaKvSemanticProjector::<Q>::new(
            DeltaKvSemanticProjectionPolicy {
                semantic_weight: 1,
                mor_weight: 32,
            },
            17,
        )
        .unwrap();
        let key = projector
            .project(&initial, &initial_decision, None)
            .unwrap();
        let mut bank = DeltaKvMemory::new(Q, 1).unwrap();
        bank.update(
            &key.key_q12,
            &[2000],
            DELTA_KV_SCALE as u16,
            &mut DeltaKvCounters::default(),
        )
        .unwrap();
        DeltaKvDynamicRekeyKernel::new(
            bank,
            projector,
            initial_decision,
            Some(DeltaKvSourceProvenance {
                semantic_record_id: 7,
                raw_segment_ids: if invalid_provenance {
                    vec![]
                } else {
                    vec![11, 13]
                },
            }),
            vec![S - 1],
            DeltaKvRecurrentPolicy::default(),
        )
        .unwrap()
    }

    fn routed() -> WideRoutedExperts<Identity, S> {
        let routing =
            RoutedSemantic::new(Identity { calls: 0 }, context(), 4).unwrap();
        WideRoutedExperts::new(routing, atlas(), 4)
            .unwrap()
            .with_trace_word_budget(4)
            .unwrap()
    }

    #[test]
    fn actual_mor_wide_k1024_page_and_current_route_delta_kv_match_manual_native_cycles() {
        let mut manual_experts = routed();
        let mut manual_memory = memory(false);
        let mut looped = WideAnfDeltaKvLoop::new(routed(), memory(false), 2).unwrap();
        let mut state = initial();
        for depth in 1..=2 {
            let manual_intermediate = manual_experts.rewrite(&state, depth).unwrap();
            let decision = &manual_experts
                .routed()
                .cycles()
                .last()
                .unwrap()
                .route
                .decision;
            let manual_post_memory = manual_memory
                .rewrite_with_mor(&manual_intermediate.state, depth, decision)
                .unwrap();
            let actual = looped.rewrite(&state, depth).unwrap();
            assert_eq!(actual, manual_post_memory);
            assert_eq!(looped.cycles().len(), depth as usize);
            assert_eq!(looped.cycles().last().unwrap().post_expert, manual_intermediate.state);
            assert_eq!(looped.cycles().last().unwrap().output, actual.state);
            assert_eq!(
                looped.cycles().last().unwrap().selected,
                decision.selected_experts
            );
            assert_eq!(looped.cycles().last().unwrap().memory.key_q12,
                manual_memory.cycle_evidence().last().unwrap().key_q12);
            assert_eq!(looped.experts(), &manual_experts);
            assert_eq!(looped.memory(), &manual_memory);
            assert_eq!(looped.experts().used_gates(), depth as usize);
            assert_eq!(looped.experts().used_trace_words(), depth as usize);
            state = actual.state;
        }
        assert_eq!(looped.experts().routed().inner().calls, 2);
        let before = looped.clone();
        assert_eq!(looped.rewrite(&state, 2), Err(WideCoupledError::Depth));
        assert_eq!(looped, before);
    }

    #[test]
    fn delta_kv_failure_never_commits_wide_pages_mor_or_memory() {
        let mut looped = WideAnfDeltaKvLoop::new(routed(), memory(true), 2).unwrap();
        let original = looped.clone();
        assert!(matches!(
            looped.rewrite(&initial(), 1),
            Err(WideCoupledError::Memory(_))
        ));
        assert_eq!(looped, original);
        assert!(looped.experts().cycles().is_empty());
        assert!(looped.experts().routed().cycles().is_empty());
        assert_eq!(looped.memory().counters().reads, 0);
    }

    #[test]
    fn expert_budget_rejection_does_not_advance_mor_or_memory() {
        let routing = RoutedSemantic::new(Identity { calls: 0 }, context(), 4).unwrap();
        let budgeted = WideRoutedExperts::new(routing, atlas(), 1)
            .unwrap()
            .with_trace_word_budget(1)
            .unwrap();
        let mut looped = WideAnfDeltaKvLoop::new(budgeted, memory(false), 2).unwrap();
        let initial = initial();
        looped.rewrite(&initial, 1).unwrap();
        let before = looped.clone();
        assert!(matches!(
            looped.rewrite(&looped.cycles().last().unwrap().output.clone(), 2),
            Err(WideCoupledError::Experts(WideRoutedError::Budget))
                | Err(WideCoupledError::Experts(WideRoutedError::Page(_)))
        ));
        assert_eq!(looped, before);
        assert_eq!(looped.memory().cycle_evidence().len(), 1);
    }
}
