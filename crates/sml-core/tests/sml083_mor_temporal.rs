use sml_core::{
    mor::{
        ActiveRouterEvidence, ExpertEvidence, ExpertProposal, MixtureOfRouters, MorDecision,
        MorPolicy, RouterId, RouterInput, MOR_SCALE_PPM,
    },
    mor_controller::{MorControllerPolicy, MorCycleReport},
    mor_temporal::{
        MorTemporalDenoiser, MorTemporalPolicy, MorTemporalTransition, TemporalMorController,
    },
    PageId,
};

fn router(id: u32, proposals: &[(u32, u32)]) -> RouterInput {
    RouterInput {
        router_id: RouterId(id),
        affinity_ppm: MOR_SCALE_PPM,
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

fn two_router_engine() -> MixtureOfRouters {
    MixtureOfRouters::new(MorPolicy {
        min_routers: 2,
        max_routers: 2,
        router_mass_threshold_ppm: MOR_SCALE_PPM,
        min_experts: 1,
        max_experts: 2,
        ..MorPolicy::default()
    })
    .unwrap()
}

fn no_balancing() -> MorControllerPolicy {
    MorControllerPolicy {
        router_balance_gain_ppm: 0,
        expert_balance_gain_ppm: 0,
        max_router_adjustment_ppm: 0,
        max_expert_adjustment_ppm: 0,
        ..MorControllerPolicy::default()
    }
}

#[test]
fn one_cycle_low_margin_flip_is_held_out_of_recurrent_memory() {
    let engine = two_router_engine();
    let mut controller =
        TemporalMorController::new(no_balancing(), MorTemporalPolicy::default()).unwrap();

    let initial = vec![
        router(0, &[(1, 900_000), (2, 100_000)]),
        router(1, &[(1, 900_000), (2, 100_000)]),
    ];
    let first = controller.route_cycle(&engine, &initial, &[]).unwrap();
    assert_eq!(
        first.temporal.transition,
        MorTemporalTransition::Initialized
    );
    assert_eq!(
        controller.controller().previous_route().primary,
        Some(PageId(1))
    );

    let flip = vec![
        router(0, &[(1, 400_000), (2, 520_000)]),
        router(1, &[(1, 400_000), (2, 520_000)]),
    ];
    let second = controller.route_cycle(&engine, &flip, &[]).unwrap();

    assert_eq!(second.cycle.decision.ranked_experts[0].page_id, PageId(2));
    assert_eq!(second.temporal.transition, MorTemporalTransition::Held);
    assert_eq!(second.temporal.pending_primary, Some(PageId(2)));
    assert_eq!(second.temporal.pending_streak, 1);
    assert_eq!(
        controller.controller().previous_route().primary,
        Some(PageId(1))
    );
}

#[test]
fn repeated_low_margin_flip_is_confirmed() {
    let engine = two_router_engine();
    let mut controller =
        TemporalMorController::new(no_balancing(), MorTemporalPolicy::default()).unwrap();

    let initial = vec![
        router(0, &[(1, 900_000), (2, 100_000)]),
        router(1, &[(1, 900_000), (2, 100_000)]),
    ];
    controller.route_cycle(&engine, &initial, &[]).unwrap();

    let flip = vec![
        router(0, &[(1, 400_000), (2, 520_000)]),
        router(1, &[(1, 400_000), (2, 520_000)]),
    ];
    let held = controller.route_cycle(&engine, &flip, &[]).unwrap();
    assert_eq!(held.temporal.transition, MorTemporalTransition::Held);

    let confirmed = controller.route_cycle(&engine, &flip, &[]).unwrap();
    assert_eq!(
        confirmed.temporal.transition,
        MorTemporalTransition::Confirmed
    );
    assert!(confirmed.temporal.accepted_route_switch);
    assert_eq!(
        controller.controller().previous_route().primary,
        Some(PageId(2))
    );
}

#[test]
fn high_margin_change_bypasses_temporal_confirmation() {
    let engine = two_router_engine();
    let mut controller =
        TemporalMorController::new(no_balancing(), MorTemporalPolicy::default()).unwrap();

    let initial = vec![
        router(0, &[(1, 900_000), (2, 100_000)]),
        router(1, &[(1, 900_000), (2, 100_000)]),
    ];
    controller.route_cycle(&engine, &initial, &[]).unwrap();

    let urgent = vec![
        router(0, &[(1, 100_000), (2, 950_000)]),
        router(1, &[(1, 100_000), (2, 950_000)]),
    ];
    let report = controller.route_cycle(&engine, &urgent, &[]).unwrap();

    assert_eq!(report.temporal.transition, MorTemporalTransition::Urgent);
    assert!(report.temporal.accepted_route_switch);
    assert_eq!(
        controller.controller().previous_route().primary,
        Some(PageId(2))
    );
}

#[test]
fn high_noise_requires_extra_confirmation() {
    let mut denoiser = MorTemporalDenoiser::new(MorTemporalPolicy::default()).unwrap();
    let initial = synthetic_decision(PageId(1), PageId(2), 400_000, 0, 0);
    denoiser.observe(&initial).unwrap();

    let noisy = synthetic_decision(PageId(2), PageId(1), 100_000, 500_000, 0);
    let first = denoiser.observe(&noisy).unwrap();
    assert_eq!(first.transition, MorTemporalTransition::Held);
    assert_eq!(first.required_streak, 3);
    assert_eq!(first.pending_streak, 1);

    let second = denoiser.observe(&noisy).unwrap();
    assert_eq!(second.transition, MorTemporalTransition::Held);
    assert_eq!(second.pending_streak, 2);

    let third = denoiser.observe(&noisy).unwrap();
    assert_eq!(third.transition, MorTemporalTransition::Confirmed);
    assert_eq!(third.stable_primary, PageId(2));
}

#[test]
fn temporal_reset_clears_stable_and_pending_routes() {
    let mut denoiser = MorTemporalDenoiser::new(MorTemporalPolicy::default()).unwrap();
    denoiser
        .observe(&synthetic_decision(PageId(1), PageId(2), 400_000, 0, 0))
        .unwrap();
    denoiser
        .observe(&synthetic_decision(PageId(2), PageId(1), 100_000, 0, 0))
        .unwrap();

    assert_eq!(denoiser.stable_route().primary, Some(PageId(1)));
    assert_eq!(denoiser.pending_primary(), Some(PageId(2)));

    denoiser.reset();

    assert_eq!(denoiser.stable_route().primary, None);
    assert_eq!(denoiser.pending_primary(), None);
    assert_eq!(denoiser.pending_streak(), 0);
}

fn synthetic_decision(
    primary: PageId,
    secondary: PageId,
    margin_ppm: u32,
    noise_ppm: u32,
    disagreement_ppm: u32,
) -> MorDecision {
    let secondary_score = 400_000;
    MorDecision {
        active_routers: vec![ActiveRouterEvidence {
            router_id: RouterId(0),
            eligibility_ppm: MOR_SCALE_PPM,
            effective_weight_ppm: MOR_SCALE_PPM,
            denoise_multiplier_ppm: MOR_SCALE_PPM,
            top_expert: primary,
        }],
        ranked_experts: vec![
            ExpertEvidence {
                page_id: primary,
                consensus_ppm: secondary_score + margin_ppm,
                support_routers: 1,
                support_weight_ppm: MOR_SCALE_PPM,
                consensus_bonus_ppm: 0,
                residency_penalty_ppm: 0,
                hysteresis_bonus_ppm: 0,
                final_score_ppm: secondary_score + margin_ppm,
            },
            ExpertEvidence {
                page_id: secondary,
                consensus_ppm: secondary_score,
                support_routers: 1,
                support_weight_ppm: MOR_SCALE_PPM,
                consensus_bonus_ppm: 0,
                residency_penalty_ppm: 0,
                hysteresis_bonus_ppm: 0,
                final_score_ppm: secondary_score,
            },
        ],
        selected_experts: vec![primary],
        noise_ppm,
        structural_disagreement_ppm: disagreement_ppm,
        expert_mass_threshold_ppm: 700_000,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    }
}

#[allow(dead_code)]
fn _assert_cycle_report_is_public(_: MorCycleReport) {}
