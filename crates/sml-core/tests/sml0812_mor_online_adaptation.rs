use sml_core::{
    mor::{
        ExpertProposal, ExpertRuntimeHint, MixtureOfRouters, MorPolicy, RouterId, MOR_SCALE_PPM,
    },
    mor_affinity::{
        MorAffinityPolicy, NativeAffinityBank, NativeRouterSpec, RouterSignatureAtom,
        SmlFingerprintPlan,
    },
    mor_controller::MorControllerPolicy,
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    mor_route_objective::{MorExpertOutcome, MorRouteObjectivePolicy},
    mor_runtime::{
        NativeMorAdaptiveRuntime, NativeMorOnlineAdaptationError,
        NativeMorOnlineAdaptationPolicy, NativeMorRuntime,
    },
    mor_temporal::MorTemporalPolicy,
    BooleanWorkspace, PageId,
};

fn atom() -> RouterSignatureAtom {
    RouterSignatureAtom {
        fingerprint_bit: 0,
        expected: true,
        weight_ppm: MOR_SCALE_PPM,
    }
}

fn runtime() -> NativeMorRuntime {
    let affinity_policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0], affinity_policy).unwrap();
    let bank = NativeAffinityBank::new(
        plan,
        vec![NativeRouterSpec {
            router_id: RouterId(0),
            atoms: vec![atom()],
            reliability_ppm: MOR_SCALE_PPM,
            confidence_ppm: MOR_SCALE_PPM,
            balance_bias_ppm: 0,
            proposals: vec![
                ExpertProposal {
                    page_id: PageId(0),
                    score_ppm: 500_000,
                },
                ExpertProposal {
                    page_id: PageId(1),
                    score_ppm: 500_000,
                },
            ],
        }],
        affinity_policy,
    )
    .unwrap();
    let registry = MorRouterRegistry::new(
        vec![RouterSpecialization {
            router_id: RouterId(0),
            experts: vec![PageId(0), PageId(1)],
        }],
        MorRegistryPolicy {
            expert_count: 2,
            max_routers: 2,
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

fn adaptive(policy: NativeMorOnlineAdaptationPolicy) -> NativeMorAdaptiveRuntime {
    NativeMorAdaptiveRuntime::new(runtime(), MorRouteObjectivePolicy::default(), policy).unwrap()
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
fn observed_objective_cost_biases_next_cycle_toward_better_expert() {
    let mut runtime = adaptive(NativeMorOnlineAdaptationPolicy {
        ema_retention_ppm: 900_000,
        objective_gain_ppm: MOR_SCALE_PPM,
        max_bias_adjustment_ppm: 300_000,
    });
    let state = BooleanWorkspace::from_bits(&[true]);

    let first = runtime.route_state(&state, &[]).unwrap();
    assert_eq!(
        first.runtime.cycle.cycle.decision.ranked_experts[0].page_id,
        PageId(0)
    );

    runtime
        .observe(
            &first,
            &[
                outcome(0, 800_000, 100_000, 100_000),
                outcome(1, 100_000, 100_000, 100_000),
            ],
        )
        .unwrap();

    let values = runtime.value_evidence();
    let page0 = values.iter().find(|value| value.page_id == PageId(0)).unwrap();
    let page1 = values.iter().find(|value| value.page_id == PageId(1)).unwrap();
    assert_eq!(page0.ema_objective_ppm, 660_000);
    assert_eq!(page1.ema_objective_ppm, 100_000);
    assert_eq!(page0.adaptive_bias_ppm, -280_000);
    assert_eq!(page1.adaptive_bias_ppm, 280_000);

    let second = runtime.route_state(&state, &[]).unwrap();
    assert_eq!(
        second.runtime.cycle.cycle.decision.ranked_experts[0].page_id,
        PageId(1)
    );
}

#[test]
fn adaptive_bias_preserves_caller_residency_and_combines_balance_bias() {
    let mut runtime = adaptive(NativeMorOnlineAdaptationPolicy {
        ema_retention_ppm: 900_000,
        objective_gain_ppm: MOR_SCALE_PPM,
        max_bias_adjustment_ppm: 300_000,
    });
    let state = BooleanWorkspace::from_bits(&[true]);
    let first = runtime.route_state(&state, &[]).unwrap();
    runtime
        .observe(
            &first,
            &[
                outcome(0, 800_000, 100_000, 100_000),
                outcome(1, 100_000, 100_000, 100_000),
            ],
        )
        .unwrap();

    let second = runtime
        .route_state(
            &state,
            &[ExpertRuntimeHint {
                page_id: PageId(1),
                balance_bias_ppm: -50_000,
                residency_penalty_ppm: 12_345,
            }],
        )
        .unwrap();

    let page1 = second
        .effective_hints
        .iter()
        .find(|hint| hint.page_id == PageId(1))
        .unwrap();
    assert_eq!(page1.balance_bias_ppm, 230_000);
    assert_eq!(page1.residency_penalty_ppm, 12_345);
}

#[test]
fn adaptive_bias_is_strictly_capped() {
    let mut runtime = adaptive(NativeMorOnlineAdaptationPolicy {
        ema_retention_ppm: 900_000,
        objective_gain_ppm: MOR_SCALE_PPM,
        max_bias_adjustment_ppm: 100_000,
    });
    let state = BooleanWorkspace::from_bits(&[true]);
    let first = runtime.route_state(&state, &[]).unwrap();
    runtime
        .observe(
            &first,
            &[
                outcome(0, MOR_SCALE_PPM, MOR_SCALE_PPM, MOR_SCALE_PPM),
                outcome(1, 0, 0, 0),
            ],
        )
        .unwrap();

    let values = runtime.value_evidence();
    assert_eq!(
        values
            .iter()
            .find(|value| value.page_id == PageId(0))
            .unwrap()
            .adaptive_bias_ppm,
        -100_000
    );
    assert_eq!(
        values
            .iter()
            .find(|value| value.page_id == PageId(1))
            .unwrap()
            .adaptive_bias_ppm,
        100_000
    );
}

#[test]
fn repeated_feedback_updates_objective_ema_deterministically() {
    let mut runtime = adaptive(NativeMorOnlineAdaptationPolicy {
        ema_retention_ppm: 500_000,
        objective_gain_ppm: MOR_SCALE_PPM,
        max_bias_adjustment_ppm: 300_000,
    });
    let state = BooleanWorkspace::from_bits(&[true]);

    let first = runtime.route_state(&state, &[]).unwrap();
    runtime
        .observe(
            &first,
            &[
                outcome(0, 800_000, 100_000, 100_000),
                outcome(1, 100_000, 100_000, 100_000),
            ],
        )
        .unwrap();

    let second = runtime.route_state(&state, &[]).unwrap();
    runtime
        .observe(
            &second,
            &[
                outcome(0, 100_000, 100_000, 100_000),
                outcome(1, 800_000, 100_000, 100_000),
            ],
        )
        .unwrap();

    let values = runtime.value_evidence();
    for value in values {
        assert_eq!(value.observations, 2);
        assert_eq!(value.ema_objective_ppm, 380_000);
        assert_eq!(value.adaptive_bias_ppm, 0);
    }
}

#[test]
fn adaptation_rejects_unknown_outcomes_and_duplicate_caller_hints() {
    let mut runtime = adaptive(NativeMorOnlineAdaptationPolicy::default());
    let state = BooleanWorkspace::from_bits(&[true]);
    let first = runtime.route_state(&state, &[]).unwrap();

    assert_eq!(
        runtime
            .observe(&first, &[outcome(2, 0, 0, 0)])
            .unwrap_err(),
        NativeMorOnlineAdaptationError::OutcomeOutOfRegistry {
            page_id: PageId(2),
            expert_count: 2,
        }
    );
    assert_eq!(runtime.auditor().snapshot().cycles, 0);
    assert!(runtime.value_evidence().is_empty());

    assert_eq!(
        runtime
            .route_state(
                &state,
                &[
                    ExpertRuntimeHint {
                        page_id: PageId(0),
                        balance_bias_ppm: 0,
                        residency_penalty_ppm: 0,
                    },
                    ExpertRuntimeHint {
                        page_id: PageId(0),
                        balance_bias_ppm: 1,
                        residency_penalty_ppm: 0,
                    },
                ],
            )
            .unwrap_err(),
        NativeMorOnlineAdaptationError::DuplicateCallerHint(PageId(0))
    );
}

#[test]
fn adaptation_reset_clears_runtime_audit_and_value_history() {
    let mut runtime = adaptive(NativeMorOnlineAdaptationPolicy::default());
    let state = BooleanWorkspace::from_bits(&[true]);
    let first = runtime.route_state(&state, &[]).unwrap();
    runtime
        .observe(
            &first,
            &[
                outcome(0, 100_000, 100_000, 100_000),
                outcome(1, 900_000, 100_000, 100_000),
            ],
        )
        .unwrap();

    assert_eq!(runtime.runtime().controller().controller().cycle(), 1);
    assert_eq!(runtime.auditor().snapshot().cycles, 1);
    assert!(!runtime.value_evidence().is_empty());

    runtime.reset();

    assert_eq!(runtime.runtime().controller().controller().cycle(), 0);
    assert_eq!(runtime.auditor().snapshot().cycles, 0);
    assert!(runtime.value_evidence().is_empty());
}
