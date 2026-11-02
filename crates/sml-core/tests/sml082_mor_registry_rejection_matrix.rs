//! Table-driven rejection matrix for every fail-closed branch of
//! `MorRouterRegistry::new` and `validate_complete_inputs`.
//! Tests only: no behavior change.

use sml_core::{
    mor::{ExpertProposal, ExpertRuntimeHint, MorError, RouterId, RouterInput, MOR_SCALE_PPM},
    mor_registry::{MorRegistryError, MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    PageId,
};

fn spec(router: u32, experts: &[u32]) -> RouterSpecialization {
    RouterSpecialization {
        router_id: RouterId(router),
        experts: experts.iter().copied().map(PageId).collect(),
    }
}

fn input(router: u32, proposals: &[u32]) -> RouterInput {
    RouterInput {
        router_id: RouterId(router),
        affinity_ppm: MOR_SCALE_PPM,
        reliability_ppm: MOR_SCALE_PPM,
        confidence_ppm: MOR_SCALE_PPM,
        balance_bias_ppm: 0,
        proposals: proposals
            .iter()
            .map(|page| ExpertProposal {
                page_id: PageId(*page),
                score_ppm: 500_000,
            })
            .collect(),
    }
}

fn hint(page: u32) -> ExpertRuntimeHint {
    ExpertRuntimeHint {
        page_id: PageId(page),
        balance_bias_ppm: 0,
        residency_penalty_ppm: 0,
    }
}

const BASE: MorRegistryPolicy = MorRegistryPolicy {
    expert_count: 4,
    max_routers: 4,
    max_edges: 8,
    max_fanout: 3,
    require_full_coverage: true,
};

#[test]
fn registry_construction_rejection_matrix() {
    let ok = || vec![spec(0, &[0, 1]), spec(1, &[2, 3])];
    let cases: Vec<(
        &str,
        Vec<RouterSpecialization>,
        MorRegistryPolicy,
        MorRegistryError,
    )> = vec![
        (
            "zero experts",
            ok(),
            MorRegistryPolicy {
                expert_count: 0,
                ..BASE
            },
            MorRegistryError::ZeroExperts,
        ),
        (
            "zero router budget",
            ok(),
            MorRegistryPolicy {
                max_routers: 0,
                ..BASE
            },
            MorRegistryError::ZeroRouterBudget,
        ),
        (
            "zero edge budget",
            ok(),
            MorRegistryPolicy {
                max_edges: 0,
                ..BASE
            },
            MorRegistryError::ZeroEdgeBudget,
        ),
        (
            "zero fanout budget",
            ok(),
            MorRegistryPolicy {
                max_fanout: 0,
                ..BASE
            },
            MorRegistryError::ZeroFanoutBudget,
        ),
        (
            "empty registry",
            vec![],
            BASE,
            MorRegistryError::EmptyRegistry,
        ),
        (
            "router budget",
            vec![
                spec(0, &[0]),
                spec(1, &[1]),
                spec(2, &[2]),
                spec(3, &[3]),
                spec(4, &[0]),
            ],
            BASE,
            MorRegistryError::RouterBudget {
                actual: 5,
                maximum: 4,
            },
        ),
        (
            "duplicate router",
            vec![spec(0, &[0, 1]), spec(0, &[2, 3])],
            BASE,
            MorRegistryError::DuplicateRouter(RouterId(0)),
        ),
        (
            "empty specialization",
            vec![spec(0, &[0, 1]), spec(1, &[])],
            BASE,
            MorRegistryError::EmptySpecialization(RouterId(1)),
        ),
        (
            "fanout budget",
            vec![spec(0, &[0, 1, 2, 3])],
            BASE,
            MorRegistryError::FanoutBudget {
                router_id: RouterId(0),
                actual: 4,
                maximum: 3,
            },
        ),
        (
            "expert out of range",
            vec![spec(0, &[0, 4])],
            BASE,
            MorRegistryError::ExpertOutOfRange {
                router_id: RouterId(0),
                page_id: PageId(4),
                expert_count: 4,
            },
        ),
        (
            "duplicate edge",
            vec![spec(0, &[1, 1])],
            BASE,
            MorRegistryError::DuplicateEdge {
                router_id: RouterId(0),
                page_id: PageId(1),
            },
        ),
        (
            "edge budget",
            vec![
                spec(0, &[0, 1, 2]),
                spec(1, &[1, 2, 3]),
                spec(2, &[0, 3, 2]),
            ],
            BASE,
            MorRegistryError::EdgeBudget {
                actual: 9,
                maximum: 8,
            },
        ),
        (
            "uncovered expert reports lowest gap",
            vec![spec(0, &[0, 3])],
            BASE,
            MorRegistryError::UncoveredExpert(PageId(1)),
        ),
    ];
    for (name, specs, policy, expected) in cases {
        assert_eq!(
            MorRouterRegistry::new(specs, policy).unwrap_err(),
            expected,
            "case: {name}"
        );
    }
}

#[test]
fn partial_coverage_is_accepted_when_not_required() {
    let registry = MorRouterRegistry::new(
        vec![spec(0, &[0, 3])],
        MorRegistryPolicy {
            require_full_coverage: false,
            ..BASE
        },
    )
    .unwrap();
    let a = registry.accounting();
    assert_eq!(a.covered_experts, 2);
    assert_eq!(a.coverage_ppm, 500_000);
    assert_eq!(a.overlap_edges, 0);
}

#[test]
fn budgets_are_inclusive_at_the_boundary() {
    // Exactly max_routers routers, max_edges edges, and max_fanout fanout.
    let registry = MorRouterRegistry::new(
        vec![
            spec(0, &[0, 1, 2]),
            spec(1, &[1, 2, 3]),
            spec(2, &[0, 3]),
            spec(3, &[2]),
        ],
        MorRegistryPolicy {
            max_edges: 9,
            ..BASE
        },
    )
    .unwrap();
    let a = registry.accounting();
    assert_eq!(a.router_count, 4);
    assert_eq!(a.edge_count, 9);
    assert_eq!(a.max_fanout, 3);
    assert_eq!(a.overlap_edges, 5);
}

#[test]
fn complete_input_rejection_matrix() {
    let registry = MorRouterRegistry::new(vec![spec(0, &[0, 1]), spec(1, &[2, 3])], BASE).unwrap();
    let cases: Vec<(
        &str,
        Vec<RouterInput>,
        Vec<ExpertRuntimeHint>,
        MorRegistryError,
    )> = vec![
        (
            "incomplete router set",
            vec![input(0, &[0])],
            vec![],
            MorRegistryError::IncompleteRouterSet {
                expected: 2,
                actual: 1,
            },
        ),
        (
            "unknown router",
            vec![input(0, &[0]), input(9, &[2])],
            vec![],
            MorRegistryError::UnknownRouter(RouterId(9)),
        ),
        (
            "duplicate router input",
            vec![input(0, &[0]), input(0, &[1])],
            vec![],
            MorRegistryError::DuplicateRouterInput(RouterId(0)),
        ),
        (
            "unauthorized proposal",
            vec![input(0, &[0]), input(1, &[1])],
            vec![],
            MorRegistryError::UnauthorizedProposal {
                router_id: RouterId(1),
                page_id: PageId(1),
            },
        ),
        (
            "runtime hint out of range",
            vec![input(0, &[0]), input(1, &[2])],
            vec![hint(0), hint(4)],
            MorRegistryError::RuntimeHintOutOfRange {
                page_id: PageId(4),
                expert_count: 4,
            },
        ),
        (
            "duplicate runtime hint",
            vec![input(0, &[0]), input(1, &[2])],
            vec![hint(1), hint(1)],
            MorRegistryError::Mor(MorError::DuplicateRuntimeHint(PageId(1))),
        ),
    ];
    for (name, routers, hints, expected) in cases {
        assert_eq!(
            registry
                .validate_complete_inputs(&routers, &hints)
                .unwrap_err(),
            expected,
            "case: {name}"
        );
    }
    registry
        .validate_complete_inputs(&[input(1, &[3]), input(0, &[])], &[hint(0), hint(3)])
        .unwrap();
}
