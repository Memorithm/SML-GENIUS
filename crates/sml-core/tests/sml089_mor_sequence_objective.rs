use sml_core::{
    mor::{ExpertProposal, MixtureOfRouters, MorPolicy, RouterId, MOR_SCALE_PPM},
    mor_affinity::{
        MorAffinityPolicy, NativeAffinityBank, NativeRouterSpec, RouterSignatureAtom,
        SmlFingerprintPlan,
    },
    mor_route_objective::{MorExpertOutcome, MorRouteObjectivePolicy},
    mor_sequence_objective::{
        MorRouteSequenceError, MorRouteSequenceExample, MorRouteSequenceObjective,
        MorRouteSequencePolicy, MorRouteSequenceStep,
    },
    BooleanWorkspace, PageId,
};

fn atom(expected: bool) -> RouterSignatureAtom {
    RouterSignatureAtom {
        fingerprint_bit: 0,
        expected,
        weight_ppm: MOR_SCALE_PPM,
    }
}

fn router(id: u32, expected: bool, page: u32) -> NativeRouterSpec {
    NativeRouterSpec {
        router_id: RouterId(id),
        atoms: vec![atom(expected)],
        reliability_ppm: MOR_SCALE_PPM,
        confidence_ppm: MOR_SCALE_PPM,
        balance_bias_ppm: 0,
        proposals: vec![ExpertProposal {
            page_id: PageId(page),
            score_ppm: 900_000,
        }],
    }
}

fn bank() -> NativeAffinityBank {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0], policy).unwrap();
    NativeAffinityBank::new(
        plan,
        vec![router(0, true, 0), router(1, false, 1)],
        policy,
    )
    .unwrap()
}

fn engine() -> MixtureOfRouters {
    MixtureOfRouters::new(MorPolicy {
        min_routers: 1,
        max_routers: 1,
        router_mass_threshold_ppm: MOR_SCALE_PPM,
        min_experts: 1,
        max_experts: 1,
        ..MorPolicy::default()
    })
    .unwrap()
}

fn outcome(page: u32, loss: u32) -> MorExpertOutcome {
    MorExpertOutcome {
        page_id: PageId(page),
        task_loss_ppm: loss,
        compute_cost_ppm: 0,
        io_cost_ppm: 0,
    }
}

fn step(bit: bool, page0_loss: u32, page1_loss: u32) -> MorRouteSequenceStep {
    MorRouteSequenceStep {
        state: BooleanWorkspace::from_bits(&[bit]),
        outcomes: vec![outcome(0, page0_loss), outcome(1, page1_loss)],
    }
}

fn objective(switch_penalty_ppm: u32) -> MorRouteSequencePolicy {
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
fn dynamic_oracle_can_prefer_staying_over_per_step_best() {
    let sequences = vec![MorRouteSequenceExample {
        steps: vec![
            step(true, 0, 200_000),
            step(false, 200_000, 0),
        ],
    }];

    let report =
        MorRouteSequenceObjective::evaluate(&bank(), &engine(), &sequences, objective(300_000))
            .unwrap();

    let sequence = &report.evidence[0];
    assert_eq!(sequence.steps[0].chosen_page, PageId(0));
    assert_eq!(sequence.steps[1].chosen_page, PageId(1));
    assert_eq!(sequence.steps[0].oracle_page, PageId(0));
    assert_eq!(sequence.steps[1].oracle_page, PageId(0));
    assert_eq!(sequence.chosen_switches, 1);
    assert_eq!(sequence.oracle_switches, 0);
    assert_eq!(sequence.chosen_total_cost_ppm, 300_000);
    assert_eq!(sequence.oracle_total_cost_ppm, 200_000);
    assert_eq!(sequence.regret_ppm, 100_000);
}

#[test]
fn zero_switch_penalty_recovers_per_step_oracle_path() {
    let sequences = vec![MorRouteSequenceExample {
        steps: vec![
            step(true, 0, 200_000),
            step(false, 200_000, 0),
        ],
    }];

    let report =
        MorRouteSequenceObjective::evaluate(&bank(), &engine(), &sequences, objective(0)).unwrap();

    let sequence = &report.evidence[0];
    assert_eq!(sequence.steps[0].oracle_page, PageId(0));
    assert_eq!(sequence.steps[1].oracle_page, PageId(1));
    assert_eq!(sequence.regret_ppm, 0);
    assert_eq!(sequence.chosen_switches, 1);
    assert_eq!(sequence.oracle_switches, 1);
}

#[test]
fn batch_report_aggregates_sequence_regret() {
    let sequences = vec![
        MorRouteSequenceExample {
            steps: vec![
                step(true, 0, 200_000),
                step(false, 200_000, 0),
            ],
        },
        MorRouteSequenceExample {
            steps: vec![
                step(true, 0, 900_000),
                step(true, 0, 900_000),
            ],
        },
    ];

    let report =
        MorRouteSequenceObjective::evaluate(&bank(), &engine(), &sequences, objective(300_000))
            .unwrap();

    assert_eq!(report.sequences, 2);
    assert_eq!(report.total_steps, 4);
    assert_eq!(report.total_regret_ppm, 100_000);
    assert_eq!(report.max_sequence_regret_ppm, 100_000);
    assert_eq!(report.zero_regret_sequences, 1);
}

#[test]
fn duplicate_outcomes_fail_closed() {
    let sequences = vec![MorRouteSequenceExample {
        steps: vec![MorRouteSequenceStep {
            state: BooleanWorkspace::from_bits(&[true]),
            outcomes: vec![outcome(0, 0), outcome(0, 10)],
        }],
    }];

    assert_eq!(
        MorRouteSequenceObjective::evaluate(&bank(), &engine(), &sequences, objective(0))
            .unwrap_err(),
        MorRouteSequenceError::DuplicateOutcome {
            sequence_index: 0,
            step_index: 0,
            page_id: PageId(0),
        }
    );
}

#[test]
fn switch_penalty_range_fails_closed() {
    let sequences = vec![MorRouteSequenceExample {
        steps: vec![step(true, 0, 1)],
    }];

    assert_eq!(
        MorRouteSequenceObjective::evaluate(
            &bank(),
            &engine(),
            &sequences,
            objective(MOR_SCALE_PPM + 1),
        )
        .unwrap_err(),
        MorRouteSequenceError::SwitchPenaltyOutOfRange(MOR_SCALE_PPM + 1)
    );
}
