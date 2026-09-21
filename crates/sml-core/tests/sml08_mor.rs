use sml_core::{
    mor::{
        ExpertProposal, ExpertRuntimeHint, MixtureOfRouters, MorError, MorPolicy, PreviousRoute,
        RouterId, RouterInput, MOR_SCALE_PPM,
    },
    PageId,
};

fn router(
    id: u32,
    affinity: u32,
    reliability: u32,
    confidence: u32,
    proposals: &[(u32, u32)],
) -> RouterInput {
    RouterInput {
        router_id: RouterId(id),
        affinity_ppm: affinity,
        reliability_ppm: reliability,
        confidence_ppm: confidence,
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

#[test]
fn singleton_router_jitter_is_aggressively_downweighted() {
    let mor = MixtureOfRouters::new(MorPolicy::default()).unwrap();
    let routers = vec![
        router(0, 1_000_000, 900_000, 900_000, &[(1, 900_000)]),
        router(1, 1_000_000, 900_000, 900_000, &[(1, 900_000)]),
        router(2, 1_000_000, 900_000, 900_000, &[(1, 900_000)]),
        router(3, 1_000_000, 800_000, 500_000, &[(9, 980_000)]),
    ];

    let decision = mor
        .route(&routers, &[], &PreviousRoute::default())
        .unwrap();

    assert_eq!(decision.selected_experts[0], PageId(1));
    assert!(decision.noise_ppm > 100_000);
    assert_eq!(decision.structural_disagreement_ppm, 0);
    assert!(!decision.request_precision_escalation);
    let noisy = decision
        .active_routers
        .iter()
        .find(|router| router.router_id == RouterId(3))
        .unwrap();
    assert_eq!(
        noisy.denoise_multiplier_ppm,
        MorPolicy::default().singleton_denoise_ppm
    );
}

#[test]
fn coherent_disagreement_expands_expert_budget() {
    let mor = MixtureOfRouters::new(MorPolicy::default()).unwrap();
    let routers = vec![
        router(0, MOR_SCALE_PPM, MOR_SCALE_PPM, MOR_SCALE_PPM, &[(1, 900_000)]),
        router(1, MOR_SCALE_PPM, MOR_SCALE_PPM, MOR_SCALE_PPM, &[(1, 900_000)]),
        router(2, MOR_SCALE_PPM, MOR_SCALE_PPM, MOR_SCALE_PPM, &[(2, 900_000)]),
        router(3, MOR_SCALE_PPM, MOR_SCALE_PPM, MOR_SCALE_PPM, &[(2, 900_000)]),
    ];

    let decision = mor
        .route(&routers, &[], &PreviousRoute::default())
        .unwrap();

    assert_eq!(decision.noise_ppm, 0);
    assert_eq!(decision.structural_disagreement_ppm, 500_000);
    assert_eq!(decision.expert_mass_threshold_ppm, 800_000);
    assert_eq!(decision.selected_experts.len(), 2);
    assert!(decision.selected_experts.contains(&PageId(1)));
    assert!(decision.selected_experts.contains(&PageId(2)));
    assert!(decision.request_precision_escalation);
}

#[test]
fn bounded_hysteresis_rejects_a_small_route_flip() {
    let mut policy = MorPolicy::default();
    policy.min_routers = 2;
    policy.max_routers = 2;
    policy.router_mass_threshold_ppm = MOR_SCALE_PPM;
    let mor = MixtureOfRouters::new(policy).unwrap();
    let routers = vec![
        router(
            0,
            MOR_SCALE_PPM,
            MOR_SCALE_PPM,
            MOR_SCALE_PPM,
            &[(1, 500_000), (2, 515_000)],
        ),
        router(
            1,
            MOR_SCALE_PPM,
            MOR_SCALE_PPM,
            MOR_SCALE_PPM,
            &[(1, 500_000), (2, 515_000)],
        ),
    ];
    let previous = PreviousRoute {
        primary: Some(PageId(1)),
        selected: vec![PageId(1)],
    };

    let decision = mor.route(&routers, &[], &previous).unwrap();

    assert_eq!(decision.ranked_experts[0].page_id, PageId(1));
    assert!(decision.hysteresis_retained_previous_primary);
}

#[test]
fn residency_cost_breaks_ties_but_is_capped() {
    let mut policy = MorPolicy::default();
    policy.min_routers = 2;
    policy.max_routers = 2;
    policy.router_mass_threshold_ppm = MOR_SCALE_PPM;
    let mor = MixtureOfRouters::new(policy).unwrap();
    let routers = vec![
        router(
            0,
            MOR_SCALE_PPM,
            MOR_SCALE_PPM,
            MOR_SCALE_PPM,
            &[(1, 600_000), (2, 600_000)],
        ),
        router(
            1,
            MOR_SCALE_PPM,
            MOR_SCALE_PPM,
            MOR_SCALE_PPM,
            &[(1, 600_000), (2, 600_000)],
        ),
    ];
    let hints = [ExpertRuntimeHint {
        page_id: PageId(2),
        balance_bias_ppm: 0,
        residency_penalty_ppm: MOR_SCALE_PPM,
    }];

    let decision = mor
        .route(&routers, &hints, &PreviousRoute::default())
        .unwrap();

    assert_eq!(decision.ranked_experts[0].page_id, PageId(1));
    let penalized = decision
        .ranked_experts
        .iter()
        .find(|expert| expert.page_id == PageId(2))
        .unwrap();
    assert_eq!(
        penalized.residency_penalty_ppm,
        MorPolicy::default().residency_penalty_cap_ppm
    );
}

#[test]
fn router_competition_activates_only_needed_mass() {
    let mut policy = MorPolicy::default();
    policy.max_routers = 5;
    policy.router_mass_threshold_ppm = 600_000;
    let mor = MixtureOfRouters::new(policy).unwrap();
    let routers = vec![
        router(0, 1_000_000, 1_000_000, 1_000_000, &[(1, 800_000)]),
        router(1, 800_000, 1_000_000, 1_000_000, &[(1, 800_000)]),
        router(2, 100_000, 1_000_000, 1_000_000, &[(1, 800_000)]),
        router(3, 50_000, 1_000_000, 1_000_000, &[(1, 800_000)]),
        router(4, 50_000, 1_000_000, 1_000_000, &[(1, 800_000)]),
    ];

    let decision = mor
        .route(&routers, &[], &PreviousRoute::default())
        .unwrap();

    assert_eq!(decision.active_routers.len(), 2);
    assert_eq!(decision.active_routers[0].router_id, RouterId(0));
    assert_eq!(decision.active_routers[1].router_id, RouterId(1));
}

#[test]
fn duplicate_router_and_proposal_fail_closed() {
    let mor = MixtureOfRouters::new(MorPolicy::default()).unwrap();
    let duplicated_router = vec![
        router(0, MOR_SCALE_PPM, MOR_SCALE_PPM, MOR_SCALE_PPM, &[(1, 900_000)]),
        router(0, MOR_SCALE_PPM, MOR_SCALE_PPM, MOR_SCALE_PPM, &[(2, 900_000)]),
    ];
    assert_eq!(
        mor.route(&duplicated_router, &[], &PreviousRoute::default())
            .unwrap_err(),
        MorError::DuplicateRouter(RouterId(0))
    );

    let duplicated_proposal = vec![router(
        7,
        MOR_SCALE_PPM,
        MOR_SCALE_PPM,
        MOR_SCALE_PPM,
        &[(1, 900_000), (1, 800_000)],
    )];
    assert_eq!(
        mor.route(&duplicated_proposal, &[], &PreviousRoute::default())
            .unwrap_err(),
        MorError::DuplicateProposal {
            router_id: RouterId(7),
            page_id: PageId(1),
        }
    );
}
