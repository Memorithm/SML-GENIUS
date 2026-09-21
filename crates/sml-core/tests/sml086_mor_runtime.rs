use sml_core::{
    mor::{
        ExpertProposal, ExpertRuntimeHint, MixtureOfRouters, MorPolicy, RouterId, MOR_SCALE_PPM,
    },
    mor_affinity::{
        MorAffinityPolicy, NativeAffinityBank, NativeRouterSpec, RouterSignatureAtom,
        SmlFingerprintPlan,
    },
    mor_controller::MorControllerPolicy,
    mor_registry::{MorRegistryError, MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    mor_runtime::{NativeMorRuntime, NativeMorRuntimeError},
    mor_temporal::{MorTemporalPolicy, MorTemporalTransition},
    BooleanWorkspace, PageId,
};

fn atom(bit: u16, expected: bool) -> RouterSignatureAtom {
    RouterSignatureAtom {
        fingerprint_bit: bit,
        expected,
        weight_ppm: MOR_SCALE_PPM,
    }
}

fn router(id: u32, atoms: Vec<RouterSignatureAtom>, page: u32) -> NativeRouterSpec {
    NativeRouterSpec {
        router_id: RouterId(id),
        atoms,
        reliability_ppm: MOR_SCALE_PPM,
        confidence_ppm: MOR_SCALE_PPM,
        balance_bias_ppm: 0,
        proposals: vec![ExpertProposal {
            page_id: PageId(page),
            score_ppm: 900_000,
        }],
    }
}

fn registry(expert_count: u32) -> MorRouterRegistry {
    MorRouterRegistry::new(
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
            expert_count,
            max_routers: 4,
            max_edges: 8,
            max_fanout: 2,
            require_full_coverage: true,
        },
    )
    .unwrap()
}

fn runtime() -> NativeMorRuntime {
    let affinity_policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 1], affinity_policy).unwrap();
    let bank = NativeAffinityBank::new(
        plan,
        vec![
            router(0, vec![atom(0, true), atom(1, false)], 0),
            router(1, vec![atom(0, false), atom(1, true)], 1),
        ],
        affinity_policy,
    )
    .unwrap();
    let engine = MixtureOfRouters::new(MorPolicy {
        min_routers: 1,
        max_routers: 1,
        router_mass_threshold_ppm: MOR_SCALE_PPM,
        ..MorPolicy::default()
    })
    .unwrap();

    NativeMorRuntime::new(
        engine,
        bank,
        registry(2),
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

#[test]
fn native_runtime_routes_directly_from_sml_state() {
    let mut runtime = runtime();
    let state = BooleanWorkspace::from_bits(&[true, false]);

    let report = runtime.route_state(&state, &[]).unwrap();

    assert_eq!(report.affinity.evidence[0].affinity_ppm, MOR_SCALE_PPM);
    assert_eq!(report.affinity.evidence[1].affinity_ppm, 0);
    assert_eq!(
        report.cycle.cycle.decision.active_routers[0].router_id,
        RouterId(0)
    );
    assert_eq!(
        report.cycle.cycle.decision.ranked_experts[0].page_id,
        PageId(0)
    );
    assert_eq!(
        report.cycle.temporal.transition,
        MorTemporalTransition::Initialized
    );
    assert_eq!(
        runtime.controller().controller().previous_route().primary,
        Some(PageId(0))
    );
}

#[test]
fn runtime_validates_specialization_before_first_cycle() {
    let affinity_policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0], affinity_policy).unwrap();
    let bank = NativeAffinityBank::new(
        plan,
        vec![
            router(0, vec![atom(0, true)], 1),
            router(1, vec![atom(0, false)], 0),
        ],
        affinity_policy,
    )
    .unwrap();

    let error = NativeMorRuntime::new(
        MixtureOfRouters::new(MorPolicy::default()).unwrap(),
        bank,
        registry(2),
        MorControllerPolicy::default(),
        MorTemporalPolicy::default(),
    )
    .unwrap_err();

    assert_eq!(
        error,
        NativeMorRuntimeError::Registry(MorRegistryError::UnauthorizedProposal {
            router_id: RouterId(0),
            page_id: PageId(1),
        })
    );
}

#[test]
fn runtime_revalidates_runtime_hints_each_cycle() {
    let mut runtime = runtime();
    let state = BooleanWorkspace::from_bits(&[true, false]);
    let hints = [ExpertRuntimeHint {
        page_id: PageId(7),
        balance_bias_ppm: 0,
        residency_penalty_ppm: 0,
    }];

    assert_eq!(
        runtime.route_state(&state, &hints).unwrap_err(),
        NativeMorRuntimeError::Registry(MorRegistryError::RuntimeHintOutOfRange {
            page_id: PageId(7),
            expert_count: 2,
        })
    );
}

#[test]
fn runtime_accounting_combines_affinity_and_registry_surfaces() {
    let runtime = runtime();
    let accounting = runtime.accounting();

    assert_eq!(accounting.affinity.fingerprint_bits, 2);
    assert_eq!(accounting.affinity.router_count, 2);
    assert_eq!(accounting.affinity.signature_atoms, 4);
    assert_eq!(accounting.registry.router_count, 2);
    assert_eq!(accounting.registry.expert_count, 2);
    assert_eq!(accounting.registry.edge_count, 2);
}

#[test]
fn runtime_reset_clears_recurrent_and_load_state() {
    let mut runtime = runtime();
    let state = BooleanWorkspace::from_bits(&[true, false]);

    runtime.route_state(&state, &[]).unwrap();
    assert_eq!(runtime.controller().controller().cycle(), 1);

    runtime.reset();

    assert_eq!(runtime.controller().controller().cycle(), 0);
    assert_eq!(
        runtime.controller().controller().previous_route().primary,
        None
    );
    assert_eq!(runtime.controller().denoiser().stable_route().primary, None);
}
