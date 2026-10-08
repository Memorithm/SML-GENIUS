//! Exact native Delta-KV -> ANF -> MOR integration against a literal ANF control.
//! The memory key is frozen, as in DKV-2b; no learned or dynamic-key claim.
use sml_core::{
    delta_kv::{DeltaKvMemory, DELTA_KV_SCALE},
    delta_kv_recurrent::{DeltaKvRecurrentKernel, DeltaKvRecurrentPolicy},
    delta_kv_semantic::{
        DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector, DeltaKvSourceProvenance,
    },
    mor::{MixtureOfRouters, MorPolicy, PreviousRoute, RouterId},
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    pascal_vector_projection::Pvp6PopulationProfile,
    pvp6_executor::{Pvp6Bank, Pvp6Gate},
    pvp6_sbg::{SbgAnfPlan, SbgCoordinate, SbgField},
    pvp6_sbg_kernel::SbgAnfKernel,
    pvp6_sbg_routing::{RoutedSemantic, RoutedSemanticError, SbgRoutingContext},
    semantic::transactional::SemanticChain,
    semantic::{
        run_semantic_recurrent, SemanticBitField, SemanticExpertSignature, SemanticMorBridge,
        SemanticMorTopK, SemanticRewriteStep, SemanticRewriter, SemanticRouterSignature,
        SemanticState,
    },
    HaltingPolicy, PageId,
};
fn context(missing: bool) -> SbgRoutingContext<8> {
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
    let mut experts = vec![SemanticExpertSignature {
        page_id: PageId(0),
        signature: SemanticBitField::zeroed().unwrap(),
    }];
    if !missing {
        experts.push(SemanticExpertSignature {
            page_id: PageId(1),
            signature: SemanticBitField::ones().unwrap(),
        });
    }
    let bridge = SemanticMorBridge::new(
        vec![SemanticRouterSignature {
            router_id: RouterId(0),
            signature: SemanticBitField::from_set_indices(&[0, 1, 2, 3]).unwrap(),
        }],
        experts,
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
fn plan() -> SbgAnfPlan<8, 64> {
    let inputs = vec![
        SbgCoordinate {
            field: SbgField::Value,
            bit: 0,
        },
        SbgCoordinate {
            field: SbgField::Value,
            bit: 1,
        },
        SbgCoordinate {
            field: SbgField::Meaning,
            bit: 0,
        },
        SbgCoordinate {
            field: SbgField::Meaning,
            bit: 1,
        },
        SbgCoordinate {
            field: SbgField::Ambiguity,
            bit: 0,
        },
        SbgCoordinate {
            field: SbgField::Routing,
            bit: 0,
        },
    ];
    SbgAnfPlan::new(
        vec![inputs],
        (0..8)
            .map(|bit| SbgCoordinate {
                field: SbgField::Routing,
                bit,
            })
            .collect(),
    )
    .unwrap()
}
fn memory(state: &SemanticState<8>, target: [i16; 2]) -> DeltaKvRecurrentKernel<8> {
    let ctx = context(false);
    let decision = ctx
        .bridge
        .route(
            state,
            &ctx.registry,
            &ctx.engine,
            &[],
            &PreviousRoute::default(),
        )
        .unwrap()
        .decision;
    let projector = DeltaKvSemanticProjector::<8>::new(
        DeltaKvSemanticProjectionPolicy {
            semantic_weight: 1,
            mor_weight: 32,
        },
        17,
    )
    .unwrap();
    let provenance = DeltaKvSourceProvenance::new(77, vec![1000, 1001]).unwrap();
    let key = projector
        .project(state, &decision, Some(&provenance))
        .unwrap();
    let mut memory = DeltaKvRecurrentKernel::new(
        DeltaKvMemory::new(8, 2).unwrap(),
        key,
        vec![0, 1],
        DeltaKvRecurrentPolicy {
            ambiguity_threshold_q12: 128,
        },
    )
    .unwrap();
    memory.seed_target(&target, DELTA_KV_SCALE as u16).unwrap();
    memory
}
fn anf(limit: u32) -> SbgAnfKernel<8, 64> {
    SbgAnfKernel::new(
        Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[1]).unwrap(); 8]).unwrap(),
        plan(),
        Pvp6PopulationProfile::AffineUniform,
        limit,
    )
    .unwrap()
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct LiteralControl {
    outputs: Vec<Vec<u64>>,
}
impl SemanticRewriter<8> for LiteralControl {
    type Error = ();
    fn rewrite(
        &mut self,
        state: &SemanticState<8>,
        _: u32,
    ) -> Result<SemanticRewriteStep<8>, Self::Error> {
        // Independent input construction and literal-by-literal evaluation.
        let values = [
            state.value.get(0).unwrap(),
            state.value.get(1).unwrap(),
            state.meaning_mask.get(0).unwrap(),
            state.meaning_mask.get(1).unwrap(),
            state.ambiguity.get(0).unwrap(),
            state.routing.get(0).unwrap(),
        ];
        let monomials = [1u16];
        let mut result = false;
        for mask in monomials {
            let mut product = true;
            for (bit, &value) in values.iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    product &= value;
                }
            }
            result ^= product;
        }
        let mut next = state.clone();
        let mut word = 0;
        for gate in 0..8 {
            next.routing.set(gate, result).unwrap();
            if result {
                word |= 1u64 << gate;
            }
        }
        self.outputs.push(vec![word]);
        Ok(SemanticRewriteStep {
            state: next,
            stability_blocked: false,
        })
    }
}
#[test]
fn complete_native_memory_anf_mor_matrix_matches_direct_control() {
    let mut cases = 0;
    for target in [[2000, -2000], [-2000, 2000], [0, 0]] {
        for initial in 0..4 {
            for depth in [1, 2, 4, 8] {
                let mut state = SemanticState::zeroed().unwrap();
                state.value.set(0, initial & 1 != 0).unwrap();
                state.value.set(1, initial & 2 != 0).unwrap();
                state.meaning_mask.set(0, true).unwrap();
                state.meaning_mask.set(1, true).unwrap();
                let memory = memory(&state, target);
                let mut candidate = RoutedSemantic::new(
                    SemanticChain::new(memory.clone(), anf(depth)),
                    context(false),
                    depth,
                )
                .unwrap();
                let mut control = RoutedSemantic::new(
                    SemanticChain::new(memory, LiteralControl { outputs: vec![] }),
                    context(false),
                    depth,
                )
                .unwrap();
                let policy = HaltingPolicy {
                    min_depth: 1,
                    max_depth: depth,
                    stable_hamming_threshold: 0,
                };
                let actual = run_semantic_recurrent(state.clone(), &mut candidate, policy).unwrap();
                let expected = run_semantic_recurrent(state, &mut control, policy).unwrap();
                assert_eq!(actual, expected);
                assert_eq!(candidate.cycles(), control.cycles());
                assert_eq!(
                    candidate.inner().first().cycle_evidence(),
                    control.inner().first().cycle_evidence()
                );
                assert_eq!(
                    candidate.inner().first().counters(),
                    control.inner().first().counters()
                );
                assert_eq!(
                    candidate
                        .inner()
                        .second()
                        .cycles()
                        .iter()
                        .map(|c| c.outputs.clone())
                        .collect::<Vec<_>>(),
                    control.inner().second().outputs
                );
                assert_eq!(candidate.inner().second().usage().cycles, actual.depth);
                assert_eq!(
                    candidate
                        .inner()
                        .first()
                        .key_evidence()
                        .provenance
                        .as_ref()
                        .unwrap()
                        .raw_segment_ids,
                    vec![1000, 1001]
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 48);
    println!("A4_DKV_ANF_MOR,cases=48,trajectory_mismatches=0,learned=false,key=frozen");
}
#[test]
fn failed_routing_rolls_back_native_memory_counters_and_anf_evidence() {
    let state = SemanticState::zeroed().unwrap();
    let mut candidate = RoutedSemantic::new(
        SemanticChain::new(memory(&state, [2000, -2000]), anf(4)),
        context(true),
        4,
    )
    .unwrap();
    let before = candidate.clone();
    assert!(matches!(
        candidate.rewrite(&state, 1),
        Err(RoutedSemanticError::Routing(_))
    ));
    assert_eq!(candidate, before);
    assert_eq!(candidate.inner().first().counters().reads, 0);
    assert!(candidate.inner().second().cycles().is_empty());
    assert!(candidate.cycles().is_empty());
}
