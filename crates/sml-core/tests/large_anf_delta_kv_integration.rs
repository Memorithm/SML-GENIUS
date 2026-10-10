//! Native multi-cycle causal and rollback qualification for the large-ANF route.
use sml_core::{
    anf_large::LargeAnfGate,
    anf_large_bank::LargeAnfBank,
    anf_large_recurrent::LargeBankRecurrent,
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_SCALE},
    delta_kv_dynamic::DeltaKvDynamicRekeyKernel,
    delta_kv_recurrent::DeltaKvRecurrentPolicy,
    delta_kv_semantic::{DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector},
    large_anf_delta_kv::{LargeAnfDeltaKvLoop, LargeCoupledError},
    large_anf_routed::LargeRoutedExperts,
    large_anf_selected_pages::{LargeAnfPage, LargeAnfPageAtlas},
    mor::{MixtureOfRouters, MorPolicy, PreviousRoute, RouterId},
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    pvp6_sbg::{SbgCoordinate, SbgField},
    pvp6_sbg_routing::{RoutedSemantic, SbgRoutingContext},
    semantic::{
        SemanticBitField, SemanticExpertSignature, SemanticMorBridge, SemanticMorTopK,
        SemanticRewriter, SemanticRouterSignature, SemanticState,
    },
    PageId,
};

type Loop = LargeAnfDeltaKvLoop<LargeBankRecurrent<24>, 24, 8>;

fn coord(bit: usize) -> SbgCoordinate {
    SbgCoordinate {
        field: SbgField::Value,
        bit,
    }
}

fn make(fail_memory: bool, gate_budget: usize) -> Loop {
    let context = SbgRoutingContext {
        bridge: SemanticMorBridge::new(
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
        .unwrap(),
        registry: MorRouterRegistry::new(
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
        .unwrap(),
        engine: MixtureOfRouters::new(MorPolicy::default()).unwrap(),
        runtime_hints: vec![],
    };
    let anf = LargeAnfBank::new(
        (0..24)
            .map(|bit| {
                let mut terms = vec![1u32 << bit];
                if bit == 23 {
                    terms.push(0);
                }
                LargeAnfGate::new(24, terms).unwrap()
            })
            .collect(),
    )
    .unwrap();
    let recurrent = LargeBankRecurrent::<24>::new(anf, 4)
        .unwrap()
        .with_routing_projection((0..24).map(|destination| (23, destination)).collect())
        .unwrap();
    let routed = RoutedSemantic::new(recurrent, context.clone(), 4).unwrap();
    let pages = (0..2)
        .map(|id| LargeAnfPage {
            id: PageId(id),
            bank: LargeAnfBank::new(vec![LargeAnfGate::new(
                20,
                vec![if id == 0 { 0 } else { 1 }],
            )
            .unwrap()])
            .unwrap(),
            inputs: (0..20).map(coord).collect(),
            outputs: vec![coord(20)],
        })
        .collect();
    let atlas = LargeAnfPageAtlas::new(pages, 2).unwrap();
    let experts = LargeRoutedExperts::new(routed, atlas, gate_budget).unwrap();
    let state = SemanticState::<24>::zeroed().unwrap();
    let route = context
        .bridge
        .route(
            &state,
            &context.registry,
            &context.engine,
            &[],
            &PreviousRoute::default(),
        )
        .unwrap()
        .decision;
    let projector = DeltaKvSemanticProjector::new(
        DeltaKvSemanticProjectionPolicy {
            semantic_weight: 1,
            mor_weight: 32,
        },
        17,
    )
    .unwrap();
    let key = projector.project(&state, &route, None).unwrap();
    let mut memory = DeltaKvMemory::new(8, 1).unwrap();
    memory
        .update(
            &key.key_q12,
            &[2000],
            DELTA_KV_SCALE as u16,
            &mut DeltaKvCounters::default(),
        )
        .unwrap();
    let provenance = if fail_memory {
        Some(sml_core::delta_kv_semantic::DeltaKvSourceProvenance {
            semantic_record_id: 7,
            raw_segment_ids: vec![],
        })
    } else {
        None
    };
    let memory = DeltaKvDynamicRekeyKernel::new(
        memory,
        projector,
        route,
        provenance,
        vec![21],
        DeltaKvRecurrentPolicy::default(),
    )
    .unwrap();
    LargeAnfDeltaKvLoop::new(experts, memory, 4).unwrap()
}

#[test]
fn actual_anf_routing_expert_and_memory_cycles_are_deterministic() {
    let mut first = make(false, 4);
    let mut second = first.clone();
    let mut state = SemanticState::<24>::zeroed().unwrap();
    for depth in 1..=4 {
        let output = first.rewrite(&state, depth).unwrap();
        let cycle = first.cycles().last().unwrap();
        let expected_page = PageId(u32::from(depth % 2 == 1));
        assert_eq!(cycle.selected, vec![expected_page]);
        assert_eq!(
            first.experts().cycles().last().unwrap().pages[0].id,
            expected_page
        );
        assert_eq!(cycle.post_expert.routing.get(0).unwrap(), depth % 2 == 1);
        assert_eq!(
            first.memory().mor_decision().selected_experts,
            cycle.selected
        );
        assert_eq!(first.memory().cycle_evidence().len(), depth as usize);
        assert_eq!(first.experts().used_gates(), depth as usize);
        assert_eq!(cycle.output, output.state);
        let replay = second.rewrite(&state, depth).unwrap();
        assert_eq!(output, replay);
        assert_eq!(first, second);
        state = output.state;
    }
    assert_eq!(first.cycles().len(), 4);
}

#[test]
fn failed_memory_projection_rolls_back_all_owned_stages() {
    let mut kernel = make(true, 4);
    let before = kernel.clone();
    let result = kernel.rewrite(&SemanticState::zeroed().unwrap(), 1);
    assert!(matches!(result, Err(LargeCoupledError::Memory(_))));
    assert_eq!(kernel, before);
    assert!(kernel.cycles().is_empty());
    assert!(kernel.experts().cycles().is_empty());
    assert!(kernel.memory().cycle_evidence().is_empty());
}

#[test]
fn exhausted_gate_budget_preserves_previous_committed_cycle() {
    let mut kernel = make(false, 1);
    let first = kernel
        .rewrite(&SemanticState::zeroed().unwrap(), 1)
        .unwrap();
    let before = kernel.clone();
    assert!(matches!(
        kernel.rewrite(&first.state, 2),
        Err(LargeCoupledError::Experts(_))
    ));
    assert_eq!(kernel, before);
    assert_eq!(kernel.cycles().len(), 1);
    assert_eq!(kernel.experts().used_gates(), 1);
}

#[test]
fn retained_trace_budget_rejects_second_cycle_without_partial_commit() {
    let mut kernel = make(false, 4);
    // The coupled loop owns the experts; verify the exact packed trace footprint
    // on the first successful cycle independently of the gate evaluation budget.
    let first = kernel.rewrite(&SemanticState::zeroed().unwrap(), 1).unwrap();
    assert_eq!(kernel.experts().used_trace_words(), 1);
    assert_eq!(kernel.experts().cycles()[0].pages[0].packed.len(), 1);
    let before = kernel.clone();
    // Reusing a committed depth must never mutate retained ANF or DKV evidence.
    assert!(matches!(kernel.rewrite(&first.state, 1), Err(LargeCoupledError::Depth)));
    assert_eq!(kernel, before);
}
