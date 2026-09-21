use sml_core::{
    mor::{
        ExpertProposal, ExpertRuntimeHint, MixtureOfRouters, MorPolicy, PreviousRoute, RouterId,
        RouterInput, MOR_SCALE_PPM,
    },
    mor_registry::{
        MorRegistryError, MorRegistryPolicy, MorRouterRegistry, RouterSpecialization,
    },
    PageId,
};

fn specialization(router: u32, experts: &[u32]) -> RouterSpecialization {
    RouterSpecialization {
        router_id: RouterId(router),
        experts: experts.iter().copied().map(PageId).collect(),
    }
}

fn input(router: u32, affinity: u32, proposals: &[(u32, u32)]) -> RouterInput {
    RouterInput {
        router_id: RouterId(router),
        affinity_ppm: affinity,
        reliability_ppm: MOR_SCALE_PPM,
        confidence_ppm: MOR_SCALE_PPM,
        balance_bias_ppm: 0,
        proposals: proposals
            .iter()
            .map(|(page, score)| ExpertProposal {
                page_id: PageId(*page),
                score_ppm: *score,
            })
            .collect(),
    }
}

fn policy(expert_count: u32) -> MorRegistryPolicy {
    MorRegistryPolicy {
        expert_count,
        max_routers: 8,
        max_edges: 32,
        max_fanout: 4,
        require_full_coverage: true,
    }
}

#[test]
fn sparse_registry_has_exact_graph_accounting() {
    let registry = MorRouterRegistry::new(
        vec![
            specialization(0, &[0, 1]),
            specialization(1, &[2, 3]),
            specialization(2, &[4, 5]),
            specialization(3, &[6, 7]),
        ],
        policy(8),
    )
    .unwrap();

    let accounting = registry.accounting();
    assert_eq!(accounting.router_count, 4);
    assert_eq!(accounting.expert_count, 8);
    assert_eq!(accounting.covered_experts, 8);
    assert_eq!(accounting.edge_count, 8);
    assert_eq!(accounting.dense_edge_count, 32);
    assert_eq!(accounting.max_fanout, 2);
    assert_eq!(accounting.average_fanout_milli, 2_000);
    assert_eq!(accounting.coverage_ppm, MOR_SCALE_PPM);
    assert_eq!(accounting.edge_density_ppm, 250_000);
    assert_eq!(accounting.sparsity_ppm, 750_000);
    assert_eq!(accounting.overlap_edges, 0);
}

#[test]
fn out_of_specialization_proposal_fails_closed() {
    let registry = MorRouterRegistry::new(
        vec![specialization(0, &[0, 1]), specialization(1, &[2, 3])],
        policy(4),
    )
    .unwrap();
    let routers = vec![
        input(0, MOR_SCALE_PPM, &[(2, 900_000)]),
        input(1, MOR_SCALE_PPM, &[(2, 900_000)]),
    ];

    assert_eq!(
        registry
            .validate_complete_inputs(&routers, &[])
            .unwrap_err(),
        MorRegistryError::UnauthorizedProposal {
            router_id: RouterId(0),
            page_id: PageId(2),
        }
    );
}

#[test]
fn full_coverage_gate_rejects_unreachable_expert() {
    let error = MorRouterRegistry::new(
        vec![specialization(0, &[0]), specialization(1, &[1])],
        policy(3),
    )
    .unwrap_err();

    assert_eq!(error, MorRegistryError::UncoveredExpert(PageId(2)));
}

#[test]
fn duplicate_edges_and_router_ids_are_rejected() {
    let duplicate_edge = MorRouterRegistry::new(
        vec![specialization(0, &[0, 0])],
        MorRegistryPolicy {
            expert_count: 1,
            max_routers: 2,
            max_edges: 4,
            max_fanout: 4,
            require_full_coverage: true,
        },
    )
    .unwrap_err();
    assert_eq!(
        duplicate_edge,
        MorRegistryError::DuplicateEdge {
            router_id: RouterId(0),
            page_id: PageId(0),
        }
    );

    let duplicate_router = MorRouterRegistry::new(
        vec![specialization(0, &[0]), specialization(0, &[1])],
        policy(2),
    )
    .unwrap_err();
    assert_eq!(
        duplicate_router,
        MorRegistryError::DuplicateRouter(RouterId(0))
    );
}

#[test]
fn registered_routing_accepts_only_complete_native_graph_inputs() {
    let registry = MorRouterRegistry::new(
        vec![
            specialization(0, &[0, 1]),
            specialization(1, &[1, 2]),
            specialization(2, &[2, 3]),
        ],
        MorRegistryPolicy {
            expert_count: 4,
            max_routers: 4,
            max_edges: 8,
            max_fanout: 3,
            require_full_coverage: true,
        },
    )
    .unwrap();
    let engine = MixtureOfRouters::new(MorPolicy {
        min_routers: 1,
        max_routers: 3,
        ..MorPolicy::default()
    })
    .unwrap();
    let routers = vec![
        input(0, 900_000, &[(0, 300_000), (1, 900_000)]),
        input(1, 850_000, &[(1, 800_000), (2, 400_000)]),
        input(2, 200_000, &[(2, 700_000), (3, 600_000)]),
    ];
    let hints = [ExpertRuntimeHint {
        page_id: PageId(1),
        balance_bias_ppm: 0,
        residency_penalty_ppm: 0,
    }];

    let decision = registry
        .route(&engine, &routers, &hints, &PreviousRoute::default())
        .unwrap();

    assert_eq!(decision.ranked_experts[0].page_id, PageId(1));
    assert!(decision.selected_experts.contains(&PageId(1)));

    assert_eq!(
        registry
            .validate_complete_inputs(&routers[..2], &hints)
            .unwrap_err(),
        MorRegistryError::IncompleteRouterSet {
            expected: 3,
            actual: 2,
        }
    );
}
