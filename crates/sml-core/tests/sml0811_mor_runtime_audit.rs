use sml_core::{
    mor::{ExpertProposal, MixtureOfRouters, MorPolicy, RouterId, MOR_SCALE_PPM},
    mor_affinity::{
        MorAffinityPolicy, NativeAffinityBank, NativeRouterSpec, RouterSignatureAtom,
        SmlFingerprintPlan,
    },
    mor_controller::MorControllerPolicy,
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    mor_route_objective::{MorExpertOutcome, MorRouteObjectivePolicy},
    mor_runtime::{
        NativeMorRuntime, NativeMorRuntimeAuditError, NativeMorRuntimeAuditor,
    },
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
            max_routers: 4,
            max_edges: 4,
            max_fanout: 2,
            require_full_coverage: true,
        },
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

fn outcome(page: u32, task: u32, compute: u32, io: u32) -> MorExpertOutcome {
    MorExpertOutcome {
        page_id: PageId(page),
        task_loss_ppm: task,
        compute_cost_ppm: compute,
        io_cost_ppm: io,
    }
}

#[test]
fn runtime_auditor_reports_exact_zero_regret_cycle() {
    let mut runtime = runtime();
    let mut auditor = NativeMorRuntimeAuditor::new(MorRouteObjectivePolicy::default()).unwrap();
    let report = runtime
        .route_state(&BooleanWorkspace::from_bits(&[true, false]), &[])
        .unwrap();

    let evidence = auditor
        .observe(
            &report,
            &[
                outcome(0, 100_000, 100_000, 100_000),
                outcome(1, 900_000, 100_000, 100_000),
            ],
        )
        .unwrap();

    assert_eq!(evidence.chosen_page, PageId(0));
    assert_eq!(evidence.oracle_page, PageId(0));
    assert_eq!(evidence.regret_ppm, 0);
    assert!(evidence.zero_regret);

    let snapshot = auditor.snapshot();
    assert_eq!(snapshot.cycles, 1);
    assert_eq!(snapshot.zero_regret_cycles, 1);
    assert_eq!(snapshot.zero_regret_fraction_ppm, MOR_SCALE_PPM);
}

#[test]
fn runtime_auditor_accumulates_exact_regret() {
    let mut runtime = runtime();
    let mut auditor = NativeMorRuntimeAuditor::new(MorRouteObjectivePolicy::default()).unwrap();
    let state = BooleanWorkspace::from_bits(&[true, false]);

    let first = runtime.route_state(&state, &[]).unwrap();
    let first_evidence = auditor
        .observe(
            &first,
            &[
                outcome(0, 800_000, 100_000, 100_000),
                outcome(1, 100_000, 100_000, 100_000),
            ],
        )
        .unwrap();
    assert_eq!(first_evidence.chosen_objective_ppm, 660_000);
    assert_eq!(first_evidence.oracle_objective_ppm, 100_000);
    assert_eq!(first_evidence.regret_ppm, 560_000);

    let second = runtime.route_state(&state, &[]).unwrap();
    let second_evidence = auditor
        .observe(
            &second,
            &[
                outcome(0, 100_000, 100_000, 100_000),
                outcome(1, 900_000, 100_000, 100_000),
            ],
        )
        .unwrap();
    assert_eq!(second_evidence.regret_ppm, 0);

    let snapshot = auditor.snapshot();
    assert_eq!(snapshot.cycles, 2);
    assert_eq!(snapshot.total_regret_ppm, 560_000);
    assert_eq!(snapshot.mean_regret_ppm, 280_000);
    assert_eq!(snapshot.max_regret_ppm, 560_000);
    assert_eq!(snapshot.zero_regret_cycles, 1);
    assert_eq!(snapshot.zero_regret_fraction_ppm, 500_000);
}

#[test]
fn runtime_auditor_tracks_urgent_temporal_switches() {
    let mut runtime = runtime();
    let mut auditor = NativeMorRuntimeAuditor::new(MorRouteObjectivePolicy::default()).unwrap();

    let first = runtime
        .route_state(&BooleanWorkspace::from_bits(&[true, false]), &[])
        .unwrap();
    auditor
        .observe(
            &first,
            &[
                outcome(0, 100_000, 100_000, 100_000),
                outcome(1, 900_000, 100_000, 100_000),
            ],
        )
        .unwrap();

    let second = runtime
        .route_state(&BooleanWorkspace::from_bits(&[false, true]), &[])
        .unwrap();
    let evidence = auditor
        .observe(
            &second,
            &[
                outcome(0, 900_000, 100_000, 100_000),
                outcome(1, 100_000, 100_000, 100_000),
            ],
        )
        .unwrap();

    assert!(evidence.route_switch);
    assert!(evidence.accepted_temporal_switch);
    assert_eq!(evidence.temporal_transition, MorTemporalTransition::Urgent);

    let snapshot = auditor.snapshot();
    assert_eq!(snapshot.route_switches, 1);
    assert_eq!(snapshot.accepted_temporal_switches, 1);
    assert_eq!(snapshot.urgent_cycles, 1);
}

#[test]
fn runtime_auditor_rejects_duplicate_and_out_of_range_outcomes() {
    let mut runtime = runtime();
    let mut auditor = NativeMorRuntimeAuditor::new(MorRouteObjectivePolicy::default()).unwrap();
    let report = runtime
        .route_state(&BooleanWorkspace::from_bits(&[true, false]), &[])
        .unwrap();

    assert_eq!(
        auditor
            .observe(
                &report,
                &[
                    outcome(0, 100_000, 100_000, 100_000),
                    outcome(0, 200_000, 100_000, 100_000),
                ],
            )
            .unwrap_err(),
        NativeMorRuntimeAuditError::DuplicateOutcome(PageId(0))
    );

    assert_eq!(
        auditor
            .observe(
                &report,
                &[
                    outcome(0, MOR_SCALE_PPM + 1, 0, 0),
                    outcome(1, 0, 0, 0),
                ],
            )
            .unwrap_err(),
        NativeMorRuntimeAuditError::OutcomePpmOutOfRange {
            page_id: PageId(0),
            value: MOR_SCALE_PPM + 1,
        }
    );
}

#[test]
fn runtime_auditor_rejects_missing_chosen_outcome_and_resets() {
    let mut runtime = runtime();
    let mut auditor = NativeMorRuntimeAuditor::new(MorRouteObjectivePolicy::default()).unwrap();
    let report = runtime
        .route_state(&BooleanWorkspace::from_bits(&[true, false]), &[])
        .unwrap();

    assert_eq!(
        auditor
            .observe(&report, &[outcome(1, 100_000, 100_000, 100_000)])
            .unwrap_err(),
        NativeMorRuntimeAuditError::MissingChosenOutcome(PageId(0))
    );

    auditor
        .observe(
            &report,
            &[
                outcome(0, 100_000, 100_000, 100_000),
                outcome(1, 900_000, 100_000, 100_000),
            ],
        )
        .unwrap();
    assert_eq!(auditor.snapshot().cycles, 1);

    auditor.reset();

    assert_eq!(auditor.snapshot().cycles, 0);
    assert_eq!(auditor.snapshot().total_regret_ppm, 0);
}
