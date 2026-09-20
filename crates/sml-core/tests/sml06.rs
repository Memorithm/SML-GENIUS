use sml_core::{
    routing_train::{
        LatentDirectRouter, RouteLearningBudget, RouteLearningError, RouteTrainingConfig,
    },
    BooleanWorkspace, ExecutionCounters, PageId,
};

fn test_budget() -> RouteLearningBudget {
    RouteLearningBudget {
        max_address_width: 4,
        max_pages: 4,
        max_learned_bits: 64,
    }
}

fn target_routes() -> Vec<PageId> {
    vec![PageId(0), PageId(2), PageId(1), PageId(2)]
}

#[test]
fn latent_route_bits_learn_complete_oracle_and_freeze() {
    let mut router = LatentDirectRouter::new(2, 3, test_budget()).unwrap();
    let report = router
        .train_complete(&target_routes(), RouteTrainingConfig::default())
        .unwrap();

    assert_eq!(report.hard_correct, 4);
    assert_eq!(report.address_count, 4);
    assert!(report.final_loss < report.initial_loss);

    let frozen = router.freeze().unwrap();
    let mut counters = ExecutionCounters::default();
    let page = frozen
        .route(&BooleanWorkspace::from_bits(&[false, true]), &mut counters)
        .unwrap();

    assert_eq!(page, PageId(1));
    assert_eq!(counters.route_lookups, 1);
    assert_eq!(counters.pairwise_comparisons, 0);
    assert_eq!(counters.attention_score_ops, 0);
}

#[test]
fn route_accounting_matches_semantic_binary_parameter_form() {
    let router = LatentDirectRouter::new(2, 3, test_budget()).unwrap();
    let accounting = router.accounting();

    assert_eq!(accounting.entry_count, 4);
    assert_eq!(accounting.page_count, 3);
    assert_eq!(accounting.bits_per_route, 2);
    assert_eq!(accounting.learned_boolean_bits, 8);
    assert_eq!(accounting.latent_training_scalars, 8);
    assert_eq!(accounting.parameter_budget().boolean, 8);
}

#[test]
fn route_training_is_deterministic() {
    let mut first = LatentDirectRouter::new(2, 3, test_budget()).unwrap();
    let mut second = LatentDirectRouter::new(2, 3, test_budget()).unwrap();
    let targets = target_routes();
    let config = RouteTrainingConfig::default();

    let first_report = first.train_complete(&targets, config).unwrap();
    let second_report = second.train_complete(&targets, config).unwrap();

    assert_eq!(first, second);
    assert_eq!(first_report, second_report);
}

#[test]
fn learned_bit_budget_is_enforced_before_allocation() {
    let error = LatentDirectRouter::new(
        4,
        8,
        RouteLearningBudget {
            max_address_width: 4,
            max_pages: 8,
            max_learned_bits: 47,
        },
    )
    .unwrap_err();
    assert_eq!(
        error,
        RouteLearningError::LearnedBitsBudget {
            actual: 48,
            maximum: 47,
        }
    );
}

#[test]
fn invalid_target_page_is_rejected() {
    let mut router = LatentDirectRouter::new(2, 3, test_budget()).unwrap();
    let error = router
        .train_complete(
            &[PageId(0), PageId(1), PageId(3), PageId(2)],
            RouteTrainingConfig::default(),
        )
        .unwrap_err();
    assert_eq!(
        error,
        RouteLearningError::TargetPage {
            address: 2,
            page: 3,
            page_count: 3,
        }
    );
}

#[test]
fn training_counters_match_exact_update_work() {
    let mut router = LatentDirectRouter::new(2, 3, test_budget()).unwrap();
    let config = RouteTrainingConfig {
        epochs: 7,
        learning_rate: 0.5,
        epsilon: 1.0e-9,
    };
    let report = router.train_complete(&target_routes(), config).unwrap();

    assert_eq!(report.counters.epochs, 7);
    assert_eq!(report.counters.addresses_seen, 28);
    assert_eq!(report.counters.latent_bit_updates, 56);
}

#[test]
fn oracle_length_is_fail_closed() {
    let mut router = LatentDirectRouter::new(2, 3, test_budget()).unwrap();
    let error = router
        .train_complete(&[PageId(0), PageId(1)], RouteTrainingConfig::default())
        .unwrap_err();
    assert_eq!(
        error,
        RouteLearningError::OracleLength {
            expected: 4,
            actual: 2,
        }
    );
}

#[test]
fn structural_budgets_reject_oversized_router_before_allocation() {
    let address_error = LatentDirectRouter::new(
        5,
        3,
        RouteLearningBudget {
            max_address_width: 4,
            max_pages: 4,
            max_learned_bits: 128,
        },
    )
    .unwrap_err();
    assert_eq!(
        address_error,
        RouteLearningError::AddressWidthBudget {
            actual: 5,
            maximum: 4,
        }
    );

    let page_error = LatentDirectRouter::new(
        2,
        5,
        RouteLearningBudget {
            max_address_width: 4,
            max_pages: 4,
            max_learned_bits: 128,
        },
    )
    .unwrap_err();
    assert_eq!(
        page_error,
        RouteLearningError::PageCountBudget {
            actual: 5,
            maximum: 4,
        }
    );
}
