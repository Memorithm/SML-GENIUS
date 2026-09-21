use sml_core::{
    mor::{ExpertProposal, MixtureOfRouters, MorPolicy, RouterId, MOR_SCALE_PPM},
    mor_affinity::{
        MorAffinityPolicy, NativeAffinityBank, NativeRouterSpec, RouterSignatureAtom,
        SmlFingerprintPlan,
    },
    mor_route_objective::{
        MorExpertOutcome, MorRouteObjective, MorRouteObjectiveError, MorRouteObjectivePolicy,
        MorRouteTrainingExample, MorRouteWeightSearch, MorRouteWeightSearchConfig,
    },
    BooleanWorkspace, PageId,
};

fn atom(bit: u16, expected: bool, weight: u32) -> RouterSignatureAtom {
    RouterSignatureAtom {
        fingerprint_bit: bit,
        expected,
        weight_ppm: weight,
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

fn outcome(page: u32, loss: u32, compute: u32, io: u32) -> MorExpertOutcome {
    MorExpertOutcome {
        page_id: PageId(page),
        task_loss_ppm: loss,
        compute_cost_ppm: compute,
        io_cost_ppm: io,
    }
}

fn example(bits: &[bool], outcomes: Vec<MorExpertOutcome>) -> MorRouteTrainingExample {
    MorRouteTrainingExample {
        state: BooleanWorkspace::from_bits(bits),
        outcomes,
    }
}

#[test]
fn objective_prefers_lower_combined_loss_and_cost() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0], policy).unwrap();
    let bank = NativeAffinityBank::new(
        plan,
        vec![
            router(0, vec![atom(0, true, MOR_SCALE_PPM)], 0),
            router(1, vec![atom(0, false, MOR_SCALE_PPM)], 1),
        ],
        policy,
    )
    .unwrap();
    let examples = vec![example(
        &[true],
        vec![
            outcome(0, 100_000, 900_000, 0),
            outcome(1, 200_000, 100_000, 0),
        ],
    )];
    let objective = MorRouteObjectivePolicy {
        task_loss_weight_ppm: 500_000,
        compute_cost_weight_ppm: 500_000,
        io_cost_weight_ppm: 0,
    };

    let report = MorRouteObjective::evaluate(&bank, &engine(), &examples, objective).unwrap();

    assert_eq!(report.cases[0].chosen_page, PageId(0));
    assert_eq!(report.cases[0].oracle_page, PageId(1));
    assert_eq!(report.cases[0].chosen_objective_ppm, 500_000);
    assert_eq!(report.cases[0].oracle_objective_ppm, 150_000);
    assert_eq!(report.cases[0].regret_ppm, 350_000);
    assert_eq!(report.total_regret_ppm, 350_000);
}

#[test]
fn weight_search_reduces_route_regret_on_noisy_fingerprint_bit() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 1], policy).unwrap();
    let bank = NativeAffinityBank::new(
        plan,
        vec![
            router(0, vec![atom(0, true, 500_000), atom(1, true, 500_000)], 0),
            router(1, vec![atom(0, false, 500_000), atom(1, false, 500_000)], 1),
        ],
        policy,
    )
    .unwrap();

    let examples = vec![
        example(
            &[true, false],
            vec![outcome(0, 0, 0, 0), outcome(1, MOR_SCALE_PPM, 0, 0)],
        ),
        example(
            &[true, true],
            vec![outcome(0, 0, 0, 0), outcome(1, MOR_SCALE_PPM, 0, 0)],
        ),
        example(
            &[false, true],
            vec![outcome(0, MOR_SCALE_PPM, 0, 0), outcome(1, 0, 0, 0)],
        ),
        example(
            &[false, false],
            vec![outcome(0, MOR_SCALE_PPM, 0, 0), outcome(1, 0, 0, 0)],
        ),
    ];
    let objective = MorRouteObjectivePolicy {
        task_loss_weight_ppm: MOR_SCALE_PPM,
        compute_cost_weight_ppm: 0,
        io_cost_weight_ppm: 0,
    };

    let artifact = MorRouteWeightSearch::train(
        bank,
        &engine(),
        &examples,
        objective,
        MorRouteWeightSearchConfig {
            passes: 4,
            candidate_weights_ppm: vec![100_000, 900_000],
        },
    )
    .unwrap();

    assert!(artifact.report.baseline.total_regret_ppm > 0);
    assert_eq!(artifact.report.final_report.total_regret_ppm, 0);
    assert_eq!(
        artifact.report.final_report.zero_regret_fraction_ppm,
        MOR_SCALE_PPM
    );
    assert!(artifact.report.accepted_updates > 0);

    let router0 = &artifact.bank.routers()[0];
    let router1 = &artifact.bank.routers()[1];
    assert!(router0.atoms[0].weight_ppm > router0.atoms[1].weight_ppm);
    assert!(router1.atoms[0].weight_ppm > router1.atoms[1].weight_ppm);
}

#[test]
fn weight_search_is_deterministic() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 1], policy).unwrap();
    let bank = NativeAffinityBank::new(
        plan,
        vec![
            router(0, vec![atom(0, true, 500_000), atom(1, true, 500_000)], 0),
            router(1, vec![atom(0, false, 500_000), atom(1, false, 500_000)], 1),
        ],
        policy,
    )
    .unwrap();
    let examples = vec![
        example(
            &[true, false],
            vec![outcome(0, 0, 0, 0), outcome(1, MOR_SCALE_PPM, 0, 0)],
        ),
        example(
            &[false, true],
            vec![outcome(0, MOR_SCALE_PPM, 0, 0), outcome(1, 0, 0, 0)],
        ),
    ];
    let objective = MorRouteObjectivePolicy {
        task_loss_weight_ppm: MOR_SCALE_PPM,
        compute_cost_weight_ppm: 0,
        io_cost_weight_ppm: 0,
    };
    let search = MorRouteWeightSearchConfig {
        passes: 2,
        candidate_weights_ppm: vec![100_000, 900_000],
    };

    let first = MorRouteWeightSearch::train(
        bank.clone(),
        &engine(),
        &examples,
        objective,
        search.clone(),
    )
    .unwrap();
    let second =
        MorRouteWeightSearch::train(bank, &engine(), &examples, objective, search).unwrap();

    assert_eq!(first, second);
}

#[test]
fn malformed_objective_weights_fail_closed() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0], policy).unwrap();
    let bank = NativeAffinityBank::new(
        plan,
        vec![
            router(0, vec![atom(0, true, 1)], 0),
            router(1, vec![atom(0, false, 1)], 1),
        ],
        policy,
    )
    .unwrap();
    let examples = vec![example(
        &[true],
        vec![outcome(0, 0, 0, 0), outcome(1, 0, 0, 0)],
    )];

    assert_eq!(
        MorRouteObjective::evaluate(
            &bank,
            &engine(),
            &examples,
            MorRouteObjectivePolicy {
                task_loss_weight_ppm: 700_000,
                compute_cost_weight_ppm: 200_000,
                io_cost_weight_ppm: 0,
            },
        )
        .unwrap_err(),
        MorRouteObjectiveError::PolicyWeightsMustSumToScale { actual: 900_000 }
    );
}

#[test]
fn duplicate_outcomes_fail_closed() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0], policy).unwrap();
    let bank = NativeAffinityBank::new(
        plan,
        vec![
            router(0, vec![atom(0, true, 1)], 0),
            router(1, vec![atom(0, false, 1)], 1),
        ],
        policy,
    )
    .unwrap();
    let examples = vec![example(
        &[true],
        vec![outcome(0, 0, 0, 0), outcome(0, 1, 0, 0)],
    )];

    assert_eq!(
        MorRouteObjective::evaluate(
            &bank,
            &engine(),
            &examples,
            MorRouteObjectivePolicy {
                task_loss_weight_ppm: MOR_SCALE_PPM,
                compute_cost_weight_ppm: 0,
                io_cost_weight_ppm: 0,
            },
        )
        .unwrap_err(),
        MorRouteObjectiveError::DuplicateOutcome {
            case_index: 0,
            page_id: PageId(0),
        }
    );
}
