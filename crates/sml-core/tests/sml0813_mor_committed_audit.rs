use sml_core::{
    mor::{ExpertProposal, MixtureOfRouters, MorPolicy, RouterId, MOR_SCALE_PPM},
    mor_affinity::{
        MorAffinityPolicy, NativeAffinityBank, NativeRouterSpec, RouterSignatureAtom,
        SmlFingerprintPlan,
    },
    mor_controller::MorControllerPolicy,
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    mor_route_objective::{MorExpertOutcome, MorRouteObjectivePolicy},
    mor_runtime::{NativeMorRuntime, NativeMorRuntimeAuditor},
    mor_temporal::{MorTemporalPolicy, MorTemporalTransition},
    BooleanWorkspace, PageId,
};

fn router(id: u32, expected: bool, page: u32) -> NativeRouterSpec {
    NativeRouterSpec {
        router_id: RouterId(id),
        atoms: (0..10)
            .map(|bit| RouterSignatureAtom {
                fingerprint_bit: bit,
                expected,
                weight_ppm: 100_000,
            })
            .collect(),
        reliability_ppm: MOR_SCALE_PPM,
        confidence_ppm: MOR_SCALE_PPM,
        balance_bias_ppm: 0,
        proposals: vec![ExpertProposal {
            page_id: PageId(page),
            score_ppm: 900_000,
        }],
    }
}

fn runtime() -> NativeMorRuntime {
    let affinity_policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new((0..10).collect(), affinity_policy).unwrap();
    let bank = NativeAffinityBank::new(
        plan,
        vec![router(0, true, 0), router(1, false, 1)],
        affinity_policy,
    )
    .unwrap();
    let registry = MorRouterRegistry::new(
        vec![
            RouterSpecialization {
                router_id: RouterId(0),
                experts: vec![PageId(0)],
            },
            RouterSpecialization {
                router_id: RouterId(1),
                experts: vec![PageId(1)],
            },
        ],
        MorRegistryPolicy {
            expert_count: 2,
            max_routers: 2,
            max_edges: 2,
            max_fanout: 1,
            require_full_coverage: true,
        },
    )
    .unwrap();
    let engine = MixtureOfRouters::new(MorPolicy {
        min_routers: 2,
        max_routers: 2,
        router_mass_threshold_ppm: MOR_SCALE_PPM,
        min_experts: 1,
        max_experts: 2,
        ..MorPolicy::default()
    })
    .unwrap();

    NativeMorRuntime::new(
        engine,
        bank,
        registry,
        MorControllerPolicy {
            router_balance_gain_ppm: 0,
            expert_balance_gain_ppm: 0,
            max_router_adjustment_ppm: 0,
            max_expert_adjustment_ppm: 0,
            ..MorControllerPolicy::default()
        },
        MorTemporalPolicy::default(),
    )
    .unwrap()
}

fn state(true_bits: usize) -> BooleanWorkspace {
    BooleanWorkspace::from_bits(&(0..10).map(|index| index < true_bits).collect::<Vec<_>>())
}

fn outcome(page: u32, task_loss_ppm: u32) -> MorExpertOutcome {
    MorExpertOutcome {
        page_id: PageId(page),
        task_loss_ppm,
        compute_cost_ppm: 0,
        io_cost_ppm: 0,
    }
}

#[test]
fn audit_attributes_held_cycle_to_committed_route() {
    let mut runtime = runtime();
    let mut auditor = NativeMorRuntimeAuditor::new(MorRouteObjectivePolicy {
        task_loss_weight_ppm: MOR_SCALE_PPM,
        compute_cost_weight_ppm: 0,
        io_cost_weight_ppm: 0,
    })
    .unwrap();

    let first = runtime.route_state(&state(6), &[]).unwrap();
    auditor
        .observe(&first, &[outcome(0, 0), outcome(1, 200_000)])
        .unwrap();

    let second = runtime.route_state(&state(4), &[]).unwrap();
    assert_eq!(
        second.cycle.temporal.transition,
        MorTemporalTransition::Held
    );
    assert_eq!(
        second.cycle.cycle.decision.ranked_experts[0].page_id,
        PageId(1)
    );
    assert_eq!(second.committed_route.primary, Some(PageId(0)));

    let evidence = auditor.observe(&second, &[outcome(0, 0)]).unwrap();

    assert_eq!(evidence.observed_page, PageId(1));
    assert_eq!(evidence.chosen_page, PageId(0));
    assert_eq!(evidence.oracle_page, PageId(0));
    assert_eq!(evidence.regret_ppm, 0);
    assert!(evidence.zero_regret);
}

#[test]
fn urgent_cycle_audit_observed_and_committed_pages_match() {
    let mut runtime = runtime();
    let mut auditor = NativeMorRuntimeAuditor::new(MorRouteObjectivePolicy {
        task_loss_weight_ppm: MOR_SCALE_PPM,
        compute_cost_weight_ppm: 0,
        io_cost_weight_ppm: 0,
    })
    .unwrap();

    let first = runtime.route_state(&state(9), &[]).unwrap();
    auditor
        .observe(&first, &[outcome(0, 0), outcome(1, MOR_SCALE_PPM)])
        .unwrap();

    let second = runtime.route_state(&state(1), &[]).unwrap();
    assert_eq!(
        second.cycle.temporal.transition,
        MorTemporalTransition::Urgent
    );

    let evidence = auditor
        .observe(&second, &[outcome(0, MOR_SCALE_PPM), outcome(1, 0)])
        .unwrap();

    assert_eq!(evidence.observed_page, PageId(1));
    assert_eq!(evidence.chosen_page, PageId(1));
    assert_eq!(evidence.oracle_page, PageId(1));
    assert_eq!(evidence.regret_ppm, 0);
}
