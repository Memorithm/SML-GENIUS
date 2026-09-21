use sml_core::{
    mor::{ExpertProposal, MixtureOfRouters, MorPolicy, RouterId, MOR_SCALE_PPM},
    mor_affinity::{
        MorAffinityPolicy, NativeAffinityBank, NativeRouterSpec, RouterSignatureAtom,
        SmlFingerprintPlan,
    },
    mor_proposal_train::{
        MorProposalScoreSearch, MorProposalScoreSearchConfig, MorProposalTrainingError,
    },
    mor_route_objective::{MorExpertOutcome, MorRouteObjectivePolicy, MorRouteTrainingExample},
    BooleanWorkspace, PageId,
};

fn atom(bit: u16, expected: bool) -> RouterSignatureAtom {
    RouterSignatureAtom {
        fingerprint_bit: bit,
        expected,
        weight_ppm: MOR_SCALE_PPM,
    }
}

fn router(id: u32, expected: bool, proposals: &[(u32, u32)]) -> NativeRouterSpec {
    NativeRouterSpec {
        router_id: RouterId(id),
        atoms: vec![atom(0, expected)],
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

fn example(bit: bool, best_page: u32) -> MorRouteTrainingExample {
    let other = 1_u32.saturating_sub(best_page);
    MorRouteTrainingExample {
        state: BooleanWorkspace::from_bits(&[bit]),
        outcomes: vec![outcome(best_page, 0), outcome(other, MOR_SCALE_PPM)],
    }
}

fn bank() -> NativeAffinityBank {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0], policy).unwrap();
    NativeAffinityBank::new(
        plan,
        vec![
            router(0, true, &[(0, 500_000), (1, 500_000)]),
            router(1, false, &[(0, 500_000), (1, 500_000)]),
        ],
        policy,
    )
    .unwrap()
}

fn objective() -> MorRouteObjectivePolicy {
    MorRouteObjectivePolicy {
        task_loss_weight_ppm: MOR_SCALE_PPM,
        compute_cost_weight_ppm: 0,
        io_cost_weight_ppm: 0,
    }
}

#[test]
fn proposal_score_search_reduces_regret_to_zero() {
    let examples = vec![
        example(true, 0),
        example(true, 0),
        example(false, 1),
        example(false, 1),
    ];

    let artifact = MorProposalScoreSearch::train(
        bank(),
        &engine(),
        &examples,
        objective(),
        MorProposalScoreSearchConfig {
            passes: 4,
            candidate_scores_ppm: vec![100_000, 900_000],
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
    assert!(artifact
        .report
        .final_report
        .cases
        .iter()
        .all(|case| case.chosen_page == case.oracle_page));

    let router1 = &artifact.bank.routers()[1];
    let page0 = router1
        .proposals
        .iter()
        .find(|proposal| proposal.page_id == PageId(0))
        .unwrap();
    let page1 = router1
        .proposals
        .iter()
        .find(|proposal| proposal.page_id == PageId(1))
        .unwrap();
    assert!(page1.score_ppm > page0.score_ppm);
}

#[test]
fn proposal_score_search_is_deterministic() {
    let examples = vec![example(true, 0), example(false, 1)];
    let config = MorProposalScoreSearchConfig {
        passes: 3,
        candidate_scores_ppm: vec![100_000, 500_000, 900_000],
    };

    let first =
        MorProposalScoreSearch::train(bank(), &engine(), &examples, objective(), config.clone())
            .unwrap();
    let second =
        MorProposalScoreSearch::train(bank(), &engine(), &examples, objective(), config).unwrap();

    assert_eq!(first, second);
}

#[test]
fn proposal_training_preserves_signature_structure() {
    let original = bank();
    let examples = vec![example(true, 0), example(false, 1)];
    let artifact = MorProposalScoreSearch::train(
        original.clone(),
        &engine(),
        &examples,
        objective(),
        MorProposalScoreSearchConfig {
            passes: 2,
            candidate_scores_ppm: vec![100_000, 900_000],
        },
    )
    .unwrap();

    for (before, after) in original.routers().iter().zip(artifact.bank.routers()) {
        assert_eq!(before.router_id, after.router_id);
        assert_eq!(before.atoms, after.atoms);
        assert_eq!(before.reliability_ppm, after.reliability_ppm);
        assert_eq!(before.confidence_ppm, after.confidence_ppm);
    }
}

#[test]
fn invalid_score_grid_fails_closed() {
    let examples = vec![example(true, 0), example(false, 1)];

    assert_eq!(
        MorProposalScoreSearch::train(
            bank(),
            &engine(),
            &examples,
            objective(),
            MorProposalScoreSearchConfig {
                passes: 1,
                candidate_scores_ppm: vec![0, 500_000],
            },
        )
        .unwrap_err(),
        MorProposalTrainingError::InvalidCandidateScore(0)
    );
}
