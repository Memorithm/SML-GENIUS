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
    mor_runtime::NativeMorRuntime,
    mor_runtime_sequence_objective::{
        NativeMorRuntimeSequenceExample, NativeMorRuntimeSequenceObjective,
        NativeMorRuntimeSequenceStep,
    },
    mor_sequence_objective::MorRouteSequencePolicy,
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
    let bits = (0..10).map(|index| index < true_bits).collect::<Vec<_>>();
    BooleanWorkspace::from_bits(&bits)
}

fn outcome(page: u32, loss: u32) -> MorExpertOutcome {
    MorExpertOutcome {
        page_id: PageId(page),
        task_loss_ppm: loss,
        compute_cost_ppm: 0,
        io_cost_ppm: 0,
    }
}

fn step(true_bits: usize, page0_loss: u32, page1_loss: u32) -> NativeMorRuntimeSequenceStep {
    NativeMorRuntimeSequenceStep {
        state: state(true_bits),
        runtime_hints: Vec::new(),
        outcomes: vec![outcome(0, page0_loss), outcome(1, page1_loss)],
    }
}

fn policy(switch_penalty_ppm: u32) -> MorRouteSequencePolicy {
    MorRouteSequencePolicy {
        step_objective: MorRouteObjectivePolicy {
            task_loss_weight_ppm: MOR_SCALE_PPM,
            compute_cost_weight_ppm: 0,
            io_cost_weight_ppm: 0,
        },
        switch_penalty_ppm,
    }
}

#[test]
fn temporal_commit_suppresses_low_margin_observed_switch() {
    let sequences = vec![NativeMorRuntimeSequenceExample {
        steps: vec![step(6, 0, 200_000), step(4, 200_000, 0)],
    }];

    let report =
        NativeMorRuntimeSequenceObjective::evaluate(&runtime(), &sequences, policy(300_000))
            .unwrap();
    let sequence = &report.evidence[0];

    assert_eq!(sequence.runtime_steps[0].observed_primary, PageId(0));
    assert_eq!(sequence.runtime_steps[0].committed_primary, PageId(0));
    assert_eq!(
        sequence.runtime_steps[0].temporal_transition,
        MorTemporalTransition::Initialized
    );

    assert_eq!(sequence.runtime_steps[1].observed_primary, PageId(1));
    assert_eq!(sequence.runtime_steps[1].committed_primary, PageId(0));
    assert_eq!(
        sequence.runtime_steps[1].temporal_transition,
        MorTemporalTransition::Held
    );
    assert!(sequence.runtime_steps[1].temporal_held);

    assert_eq!(sequence.objective.chosen_switches, 0);
    assert_eq!(sequence.objective.chosen_total_cost_ppm, 200_000);
    assert_eq!(sequence.objective.oracle_total_cost_ppm, 200_000);
    assert_eq!(sequence.objective.regret_ppm, 0);
    assert_eq!(report.temporal_held_steps, 1);
    assert_eq!(report.committed_switches, 0);
}

#[test]
fn urgent_margin_commits_switch_immediately() {
    let sequences = vec![NativeMorRuntimeSequenceExample {
        steps: vec![step(9, 0, 900_000), step(1, 900_000, 0)],
    }];

    let report =
        NativeMorRuntimeSequenceObjective::evaluate(&runtime(), &sequences, policy(100_000))
            .unwrap();
    let sequence = &report.evidence[0];

    assert_eq!(sequence.runtime_steps[1].observed_primary, PageId(1));
    assert_eq!(sequence.runtime_steps[1].committed_primary, PageId(1));
    assert_eq!(
        sequence.runtime_steps[1].temporal_transition,
        MorTemporalTransition::Urgent
    );
    assert_eq!(report.temporal_urgent_steps, 1);
    assert_eq!(report.committed_switches, 1);
    assert_eq!(sequence.objective.regret_ppm, 0);
}

#[test]
fn each_sequence_starts_from_reset_runtime_state() {
    let sequences = vec![
        NativeMorRuntimeSequenceExample {
            steps: vec![step(6, 0, 200_000)],
        },
        NativeMorRuntimeSequenceExample {
            steps: vec![step(4, 200_000, 0)],
        },
    ];

    let report =
        NativeMorRuntimeSequenceObjective::evaluate(&runtime(), &sequences, policy(0)).unwrap();

    assert_eq!(
        report.evidence[0].runtime_steps[0].temporal_transition,
        MorTemporalTransition::Initialized
    );
    assert_eq!(
        report.evidence[1].runtime_steps[0].temporal_transition,
        MorTemporalTransition::Initialized
    );
    assert_eq!(
        report.evidence[1].runtime_steps[0].committed_primary,
        PageId(1)
    );
}

#[test]
fn invalid_runtime_hint_fails_closed_through_runtime_objective() {
    let sequences = vec![NativeMorRuntimeSequenceExample {
        steps: vec![NativeMorRuntimeSequenceStep {
            state: state(6),
            runtime_hints: vec![ExpertRuntimeHint {
                page_id: PageId(7),
                balance_bias_ppm: 0,
                residency_penalty_ppm: 0,
            }],
            outcomes: vec![outcome(0, 0), outcome(1, 1)],
        }],
    }];

    assert!(
        NativeMorRuntimeSequenceObjective::evaluate(&runtime(), &sequences, policy(0)).is_err()
    );
}
