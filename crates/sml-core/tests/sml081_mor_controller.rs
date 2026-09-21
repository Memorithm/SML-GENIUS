use sml_core::{
    mor::{ExpertProposal, MixtureOfRouters, MorPolicy, RouterId, RouterInput, MOR_SCALE_PPM},
    mor_controller::{MorController, MorControllerPolicy},
    PageId,
};

fn router(id: u32, affinity_ppm: u32, proposals: &[(u32, u32)]) -> RouterInput {
    RouterInput {
        router_id: RouterId(id),
        affinity_ppm,
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

#[test]
fn controller_balancing_penalizes_hot_router_and_rewards_cold_router() {
    let engine = MixtureOfRouters::new(MorPolicy {
        min_routers: 1,
        max_routers: 1,
        router_mass_threshold_ppm: MOR_SCALE_PPM,
        ..MorPolicy::default()
    })
    .unwrap();
    let mut controller = MorController::new(MorControllerPolicy {
        load_ema_retention_ppm: 500_000,
        router_balance_gain_ppm: MOR_SCALE_PPM,
        expert_balance_gain_ppm: 0,
        max_router_adjustment_ppm: 300_000,
        max_expert_adjustment_ppm: 0,
    })
    .unwrap();
    let routers = vec![
        router(0, 800_000, &[(1, 900_000)]),
        router(1, 790_000, &[(1, 900_000)]),
    ];

    let first = controller.route_cycle(&engine, &routers, &[]).unwrap();
    assert_eq!(first.decision.active_routers[0].router_id, RouterId(0));
    assert_eq!(controller.router_load_ppm(RouterId(0)), 500_000);
    assert_eq!(controller.router_load_ppm(RouterId(1)), 0);

    let second = controller.route_cycle(&engine, &routers, &[]).unwrap();
    let hot = second
        .router_balance
        .iter()
        .find(|entry| entry.router_id == RouterId(0))
        .unwrap();
    let cold = second
        .router_balance
        .iter()
        .find(|entry| entry.router_id == RouterId(1))
        .unwrap();

    assert_eq!(hot.adaptive_adjustment_ppm, -250_000);
    assert_eq!(cold.adaptive_adjustment_ppm, 250_000);
    assert_eq!(second.decision.active_routers[0].router_id, RouterId(1));
}

#[test]
fn controller_carries_previous_route_into_recurrent_hysteresis() {
    let engine = MixtureOfRouters::new(MorPolicy {
        min_routers: 1,
        max_routers: 1,
        router_mass_threshold_ppm: MOR_SCALE_PPM,
        ..MorPolicy::default()
    })
    .unwrap();
    let mut controller = MorController::new(MorControllerPolicy {
        router_balance_gain_ppm: 0,
        expert_balance_gain_ppm: 0,
        max_router_adjustment_ppm: 0,
        max_expert_adjustment_ppm: 0,
        ..MorControllerPolicy::default()
    })
    .unwrap();

    let first_routers = vec![router(0, MOR_SCALE_PPM, &[(1, 515_000), (2, 500_000)])];
    let first = controller
        .route_cycle(&engine, &first_routers, &[])
        .unwrap();
    assert_eq!(first.decision.ranked_experts[0].page_id, PageId(1));

    let second_routers = vec![router(0, MOR_SCALE_PPM, &[(1, 500_000), (2, 515_000)])];
    let second = controller
        .route_cycle(&engine, &second_routers, &[])
        .unwrap();

    assert_eq!(second.decision.ranked_experts[0].page_id, PageId(1));
    assert!(second.decision.hysteresis_retained_previous_primary);
    assert!(!second.metrics.route_switch);
    assert_eq!(controller.previous_route().primary, Some(PageId(1)));
}

#[test]
fn cycle_metrics_expose_sparse_router_and_expert_fractions() {
    let engine = MixtureOfRouters::new(MorPolicy {
        min_routers: 1,
        max_routers: 2,
        router_mass_threshold_ppm: 600_000,
        min_experts: 1,
        max_experts: 1,
        ..MorPolicy::default()
    })
    .unwrap();
    let mut controller = MorController::new(MorControllerPolicy::default()).unwrap();
    let routers = vec![
        router(0, 1_000_000, &[(1, 900_000), (2, 100_000)]),
        router(1, 800_000, &[(1, 850_000), (2, 150_000)]),
        router(2, 100_000, &[(1, 700_000), (2, 300_000)]),
        router(3, 50_000, &[(1, 600_000), (2, 400_000)]),
    ];

    let report = controller.route_cycle(&engine, &routers, &[]).unwrap();

    assert_eq!(report.metrics.cycle, 1);
    assert_eq!(report.metrics.total_routers, 4);
    assert_eq!(report.metrics.active_routers, 2);
    assert_eq!(report.metrics.active_router_fraction_ppm, 500_000);
    assert_eq!(report.metrics.candidate_experts, 2);
    assert_eq!(report.metrics.selected_experts, 1);
    assert_eq!(report.metrics.selected_expert_fraction_ppm, 500_000);
    assert!(report.metrics.route_margin_ppm > 0);
    assert!(!report.metrics.route_switch);
}

#[test]
fn expert_balancing_is_native_and_bounded() {
    let engine = MixtureOfRouters::new(MorPolicy {
        min_routers: 1,
        max_routers: 1,
        router_mass_threshold_ppm: MOR_SCALE_PPM,
        min_experts: 1,
        max_experts: 1,
        ..MorPolicy::default()
    })
    .unwrap();
    let mut controller = MorController::new(MorControllerPolicy {
        load_ema_retention_ppm: 500_000,
        router_balance_gain_ppm: 0,
        expert_balance_gain_ppm: MOR_SCALE_PPM,
        max_router_adjustment_ppm: 0,
        max_expert_adjustment_ppm: 100_000,
    })
    .unwrap();
    let routers = vec![router(0, MOR_SCALE_PPM, &[(1, 900_000), (2, 890_000)])];

    let first = controller.route_cycle(&engine, &routers, &[]).unwrap();
    assert_eq!(first.decision.selected_experts, vec![PageId(1)]);
    assert_eq!(controller.expert_load_ppm(PageId(1)), 500_000);
    assert_eq!(controller.expert_load_ppm(PageId(2)), 0);

    let second = controller.route_cycle(&engine, &routers, &[]).unwrap();
    let hot = second
        .expert_balance
        .iter()
        .find(|entry| entry.page_id == PageId(1))
        .unwrap();
    let cold = second
        .expert_balance
        .iter()
        .find(|entry| entry.page_id == PageId(2))
        .unwrap();

    assert_eq!(hot.adaptive_adjustment_ppm, -100_000);
    assert_eq!(cold.adaptive_adjustment_ppm, 100_000);
    assert_eq!(second.decision.selected_experts, vec![PageId(2)]);
}

#[test]
fn reset_clears_all_controller_history() {
    let engine = MixtureOfRouters::new(MorPolicy::default()).unwrap();
    let mut controller = MorController::new(MorControllerPolicy::default()).unwrap();
    let routers = vec![router(0, MOR_SCALE_PPM, &[(1, 900_000)])];

    controller.route_cycle(&engine, &routers, &[]).unwrap();
    assert_eq!(controller.cycle(), 1);
    assert!(controller.router_load_ppm(RouterId(0)) > 0);

    controller.reset();

    assert_eq!(controller.cycle(), 0);
    assert_eq!(controller.previous_route().primary, None);
    assert_eq!(controller.router_load_ppm(RouterId(0)), 0);
    assert_eq!(controller.expert_load_ppm(PageId(1)), 0);
}
