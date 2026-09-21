use sml_core::{
    mor::{ExpertProposal, MixtureOfRouters, MorPolicy, RouterId, MOR_SCALE_PPM},
    mor_affinity::{
        MorAffinityPolicy, NativeAffinityBank, NativeRouterSpec, RouterSignatureAtom,
        SmlFingerprintPlan,
    },
    mor_controller::MorControllerPolicy,
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    mor_route_objective::{MorExpertOutcome, MorRouteObjectivePolicy},
    mor_runtime::NativeMorRuntime,
    mor_runtime_sequence_objective::{
        NativeMorRuntimeSequenceExample, NativeMorRuntimeSequenceStep,
    },
    mor_sequence_objective::MorRouteSequencePolicy,
    mor_temporal::MorTemporalPolicy,
    mor_temporal_search::{
        MorTemporalPolicySearch, MorTemporalPolicySearchConfig, MorTemporalPolicySearchError,
    },
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

fn runtime(temporal_policy: MorTemporalPolicy) -> NativeMorRuntime {
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
        temporal_policy,
    )
    .unwrap()
}

fn state(true_bits: usize) -> BooleanWorkspace {
    BooleanWorkspace::from_bits(&(0..10).map(|index| index < true_bits).collect::<Vec<_>>())
}

fn outcome(page: u32, loss: u32) -> MorExpertOutcome {
    MorExpertOutcome {
        page_id: PageId(page),
        task_loss_ppm: loss,
        compute_cost_ppm: 0,
        io_cost_ppm: 0,
    }
}

fn sequence() -> Vec<NativeMorRuntimeSequenceExample> {
    vec![NativeMorRuntimeSequenceExample {
        steps: vec![
            NativeMorRuntimeSequenceStep {
                state: state(6),
                runtime_hints: Vec::new(),
                outcomes: vec![outcome(0, 0), outcome(1, 200_000)],
            },
            NativeMorRuntimeSequenceStep {
                state: state(4),
                runtime_hints: Vec::new(),
                outcomes: vec![outcome(0, 200_000), outcome(1, 0)],
            },
        ],
    }]
}

fn objective() -> MorRouteSequencePolicy {
    MorRouteSequencePolicy {
        step_objective: MorRouteObjectivePolicy {
            task_loss_weight_ppm: MOR_SCALE_PPM,
            compute_cost_weight_ppm: 0,
            io_cost_weight_ppm: 0,
        },
        switch_penalty_ppm: 300_000,
    }
}

fn temporal(confirmation_cycles: u32) -> MorTemporalPolicy {
    MorTemporalPolicy {
        confirmation_cycles,
        noise_extra_confirmation_cycles: 0,
        disagreement_extra_confirmation_cycles: 0,
        max_confirmation_cycles: confirmation_cycles,
        noise_threshold_ppm: MOR_SCALE_PPM,
        disagreement_threshold_ppm: MOR_SCALE_PPM,
        urgent_margin_ppm: 250_000,
    }
}

#[test]
fn search_learns_to_confirm_low_margin_switch() {
    let baseline_runtime = runtime(temporal(1));
    let artifact = MorTemporalPolicySearch::train(
        &baseline_runtime,
        &sequence(),
        objective(),
        MorTemporalPolicySearchConfig {
            candidates: vec![temporal(1), temporal(2)],
        },
    )
    .unwrap();

    assert_eq!(artifact.report.baseline.total_regret_ppm, 100_000);
    assert_eq!(artifact.report.final_report.total_regret_ppm, 0);
    assert_eq!(artifact.report.selected_policy, temporal(2));
    assert!(artifact.report.accepted_change);
    assert_eq!(artifact.report.final_report.committed_switches, 0);
    assert_eq!(artifact.report.final_report.temporal_held_steps, 1);
    assert_eq!(
        artifact.runtime.controller().denoiser().policy(),
        temporal(2)
    );
}

#[test]
fn equal_evidence_keeps_current_policy() {
    let baseline_policy = temporal(2);
    let baseline_runtime = runtime(baseline_policy);
    let stable_sequences = vec![NativeMorRuntimeSequenceExample {
        steps: vec![
            NativeMorRuntimeSequenceStep {
                state: state(6),
                runtime_hints: Vec::new(),
                outcomes: vec![outcome(0, 0), outcome(1, 200_000)],
            },
            NativeMorRuntimeSequenceStep {
                state: state(6),
                runtime_hints: Vec::new(),
                outcomes: vec![outcome(0, 0), outcome(1, 200_000)],
            },
        ],
    }];

    let artifact = MorTemporalPolicySearch::train(
        &baseline_runtime,
        &stable_sequences,
        objective(),
        MorTemporalPolicySearchConfig {
            candidates: vec![temporal(1), temporal(2)],
        },
    )
    .unwrap();

    assert_eq!(artifact.report.baseline.total_regret_ppm, 0);
    assert_eq!(artifact.report.final_report.total_regret_ppm, 0);
    assert_eq!(artifact.report.selected_policy, baseline_policy);
    assert!(!artifact.report.accepted_change);
}

#[test]
fn invalid_temporal_candidate_fails_closed() {
    let invalid = MorTemporalPolicy {
        confirmation_cycles: 0,
        ..MorTemporalPolicy::default()
    };

    assert!(matches!(
        MorTemporalPolicySearch::train(
            &runtime(temporal(1)),
            &sequence(),
            objective(),
            MorTemporalPolicySearchConfig {
                candidates: vec![invalid],
            },
        )
        .unwrap_err(),
        MorTemporalPolicySearchError::InvalidTemporalPolicy(_)
    ));
}

#[test]
fn empty_candidate_set_fails_closed() {
    assert_eq!(
        MorTemporalPolicySearch::train(
            &runtime(temporal(1)),
            &sequence(),
            objective(),
            MorTemporalPolicySearchConfig {
                candidates: Vec::new(),
            },
        )
        .unwrap_err(),
        MorTemporalPolicySearchError::EmptyCandidateSet
    );
}
