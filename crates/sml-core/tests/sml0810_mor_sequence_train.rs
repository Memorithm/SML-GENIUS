use sml_core::{
    mor::{ExpertProposal, MixtureOfRouters, MorPolicy, RouterId, MOR_SCALE_PPM},
    mor_affinity::{
        MorAffinityPolicy, NativeAffinityBank, NativeRouterSpec, RouterSignatureAtom,
        SmlFingerprintPlan,
    },
    mor_route_objective::{MorExpertOutcome, MorRouteObjectivePolicy},
    mor_sequence_objective::{
        MorRouteSequenceExample, MorRouteSequencePolicy, MorRouteSequenceStep,
    },
    mor_sequence_train::{
        MorSequenceSearch, MorSequenceSearchConfig, MorSequenceTrainingError,
    },
    BooleanWorkspace, PageId,
};

fn outcome(page: u32, loss: u32) -> MorExpertOutcome {
    MorExpertOutcome {
        page_id: PageId(page),
        task_loss_ppm: loss,
        compute_cost_ppm: 0,
        io_cost_ppm: 0,
    }
}

fn step(bits: &[bool], page0_loss: u32, page1_loss: u32) -> MorRouteSequenceStep {
    MorRouteSequenceStep {
        state: BooleanWorkspace::from_bits(bits),
        outcomes: vec![outcome(0, page0_loss), outcome(1, page1_loss)],
    }
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

fn proposal_bank() -> NativeAffinityBank {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0], policy).unwrap();
    NativeAffinityBank::new(
        plan,
        vec![
            NativeRouterSpec {
                router_id: RouterId(0),
                atoms: vec![RouterSignatureAtom {
                    fingerprint_bit: 0,
                    expected: true,
                    weight_ppm: MOR_SCALE_PPM,
                }],
                reliability_ppm: MOR_SCALE_PPM,
                confidence_ppm: MOR_SCALE_PPM,
                balance_bias_ppm: 0,
                proposals: vec![
                    ExpertProposal {
                        page_id: PageId(0),
                        score_ppm: 900_000,
                    },
                    ExpertProposal {
                        page_id: PageId(1),
                        score_ppm: 100_000,
                    },
                ],
            },
            NativeRouterSpec {
                router_id: RouterId(1),
                atoms: vec![RouterSignatureAtom {
                    fingerprint_bit: 0,
                    expected: false,
                    weight_ppm: MOR_SCALE_PPM,
                }],
                reliability_ppm: MOR_SCALE_PPM,
                confidence_ppm: MOR_SCALE_PPM,
                balance_bias_ppm: 0,
                proposals: vec![
                    ExpertProposal {
                        page_id: PageId(0),
                        score_ppm: 100_000,
                    },
                    ExpertProposal {
                        page_id: PageId(1),
                        score_ppm: 900_000,
                    },
                ],
            },
        ],
        policy,
    )
    .unwrap()
}

fn affinity_bank() -> NativeAffinityBank {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 1], policy).unwrap();
    NativeAffinityBank::new(
        plan,
        vec![
            NativeRouterSpec {
                router_id: RouterId(1),
                atoms: vec![
                    RouterSignatureAtom {
                        fingerprint_bit: 0,
                        expected: true,
                        weight_ppm: 500_000,
                    },
                    RouterSignatureAtom {
                        fingerprint_bit: 1,
                        expected: false,
                        weight_ppm: 500_000,
                    },
                ],
                reliability_ppm: MOR_SCALE_PPM,
                confidence_ppm: MOR_SCALE_PPM,
                balance_bias_ppm: 0,
                proposals: vec![ExpertProposal {
                    page_id: PageId(0),
                    score_ppm: 900_000,
                }],
            },
            NativeRouterSpec {
                router_id: RouterId(0),
                atoms: vec![
                    RouterSignatureAtom {
                        fingerprint_bit: 0,
                        expected: false,
                        weight_ppm: 500_000,
                    },
                    RouterSignatureAtom {
                        fingerprint_bit: 1,
                        expected: true,
                        weight_ppm: 500_000,
                    },
                ],
                reliability_ppm: MOR_SCALE_PPM,
                confidence_ppm: MOR_SCALE_PPM,
                balance_bias_ppm: 0,
                proposals: vec![ExpertProposal {
                    page_id: PageId(1),
                    score_ppm: 900_000,
                }],
            },
        ],
        policy,
    )
    .unwrap()
}

#[test]
fn sequence_search_can_learn_to_avoid_expensive_switch_via_proposals() {
    let sequences = vec![MorRouteSequenceExample {
        steps: vec![
            step(&[true], 0, 200_000),
            step(&[false], 200_000, 0),
        ],
    }];

    let artifact = MorSequenceSearch::train(
        proposal_bank(),
        &engine(),
        &sequences,
        objective(),
        MorSequenceSearchConfig {
            passes: 4,
            candidate_signature_weights_ppm: vec![500_000, 1_000_000],
            candidate_proposal_scores_ppm: vec![100_000, 900_000],
        },
    )
    .unwrap();

    assert_eq!(artifact.report.baseline.total_regret_ppm, 100_000);
    assert_eq!(artifact.report.final_report.total_regret_ppm, 0);
    assert!(artifact.report.proposal_score_updates > 0);
    let final_sequence = &artifact.report.final_report.evidence[0];
    assert_eq!(final_sequence.steps[0].chosen_page, PageId(0));
    assert_eq!(final_sequence.steps[1].chosen_page, PageId(0));
    assert_eq!(final_sequence.chosen_switches, 0);
}

#[test]
fn sequence_search_can_learn_to_avoid_switch_via_affinity_weights() {
    let sequences = vec![MorRouteSequenceExample {
        steps: vec![
            step(&[true, false], 0, 200_000),
            step(&[false, false], 200_000, 0),
        ],
    }];

    let artifact = MorSequenceSearch::train(
        affinity_bank(),
        &engine(),
        &sequences,
        objective(),
        MorSequenceSearchConfig {
            passes: 4,
            candidate_signature_weights_ppm: vec![100_000, 900_000],
            candidate_proposal_scores_ppm: vec![900_000],
        },
    )
    .unwrap();

    assert_eq!(artifact.report.baseline.total_regret_ppm, 100_000);
    assert_eq!(artifact.report.final_report.total_regret_ppm, 0);
    assert!(artifact.report.signature_weight_updates > 0);
    let final_sequence = &artifact.report.final_report.evidence[0];
    assert_eq!(final_sequence.steps[0].chosen_page, PageId(0));
    assert_eq!(final_sequence.steps[1].chosen_page, PageId(0));
}

#[test]
fn sequence_search_is_deterministic() {
    let sequences = vec![MorRouteSequenceExample {
        steps: vec![
            step(&[true], 0, 200_000),
            step(&[false], 200_000, 0),
        ],
    }];
    let config = MorSequenceSearchConfig {
        passes: 3,
        candidate_signature_weights_ppm: vec![500_000, 1_000_000],
        candidate_proposal_scores_ppm: vec![100_000, 900_000],
    };

    let first = MorSequenceSearch::train(
        proposal_bank(),
        &engine(),
        &sequences,
        objective(),
        config.clone(),
    )
    .unwrap();
    let second =
        MorSequenceSearch::train(proposal_bank(), &engine(), &sequences, objective(), config)
            .unwrap();

    assert_eq!(first, second);
}

#[test]
fn invalid_sequence_search_grid_fails_closed() {
    let sequences = vec![MorRouteSequenceExample {
        steps: vec![step(&[true], 0, 1)],
    }];

    assert_eq!(
        MorSequenceSearch::train(
            proposal_bank(),
            &engine(),
            &sequences,
            objective(),
            MorSequenceSearchConfig {
                passes: 1,
                candidate_signature_weights_ppm: vec![0],
                candidate_proposal_scores_ppm: vec![100_000],
            },
        )
        .unwrap_err(),
        MorSequenceTrainingError::InvalidSignatureWeight(0)
    );
}
