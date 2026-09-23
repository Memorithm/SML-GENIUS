use sml_core::tasks::{suite_from_case_id, TaskAnswer, TaskKind};
use sml_rsi::{
    curriculum::{build_verified_curriculum, CurriculumPolicy, CurriculumProposal},
    portfolio::{
        run_curriculum_portfolio, CurriculumCandidate, CurriculumSource, PortfolioPolicy,
    },
    post_selection::{
        evaluate_post_selection_holdout, PostSelectionError, PostSelectionOutcome,
        PostSelectionPolicy,
    },
};

fn proposal(
    id: &str,
    family: TaskKind,
    start: u64,
    count: u32,
    difficulty: u16,
) -> CurriculumProposal {
    CurriculumProposal {
        proposal_id: id.into(),
        family,
        case_id_start: start,
        case_count: count,
        difficulty,
    }
}

fn curriculum(difficulty: u16) -> sml_rsi::curriculum::CurriculumReport {
    build_verified_curriculum(
        &[
            proposal("recall", TaskKind::DelayedBitRecall, 1_000_000, 32, difficulty),
            proposal(
                "overwrite",
                TaskKind::OverwriteBitRecall,
                1_100_000,
                32,
                difficulty,
            ),
            proposal(
                "copy",
                TaskKind::CopyAfterMarker,
                1_200_000,
                32,
                difficulty,
            ),
            proposal(
                "and",
                TaskKind::TwoFactConjunction,
                1_300_000,
                32,
                difficulty,
            ),
            proposal(
                "keyed",
                TaskKind::KeyedFactRecall,
                1_400_000,
                32,
                difficulty,
            ),
            proposal(
                "distractor",
                TaskKind::DistractorRejection,
                1_500_000,
                32,
                difficulty,
            ),
        ],
        CurriculumPolicy::default(),
    )
    .unwrap()
}

fn candidates() -> Vec<CurriculumCandidate> {
    vec![
        CurriculumCandidate {
            candidate_id: "fixed-control".into(),
            source: CurriculumSource::FixedControl,
            curriculum: curriculum(7),
        },
        CurriculumCandidate {
            candidate_id: "model-proposed".into(),
            source: CurriculumSource::ModelProposed,
            curriculum: curriculum(3),
        },
    ]
}

fn portfolio() -> sml_rsi::portfolio::CurriculumPortfolioReport {
    run_curriculum_portfolio(&candidates(), PortfolioPolicy::default()).unwrap()
}

#[test]
fn selected_candidate_is_frozen_before_post_selection_holdout_and_ties_fixed_quality() {
    let candidates = candidates();
    let portfolio = run_curriculum_portfolio(&candidates, PortfolioPolicy::default()).unwrap();
    assert_eq!(portfolio.selected_id, "model-proposed");
    let holdout = suite_from_case_id(5_000_000, 16).unwrap();

    let report = evaluate_post_selection_holdout(
        &portfolio,
        &candidates,
        &holdout,
        PostSelectionPolicy::default(),
    )
    .unwrap();

    assert_eq!(report.selected_id, "model-proposed");
    assert_eq!(report.fixed_control_id, "fixed-control");
    assert_eq!(report.outcome, PostSelectionOutcome::Tie);
    assert_eq!(report.exact_task_delta, 0);
    assert_eq!(report.selected_holdout.exact_tasks, report.holdout_tasks);
    assert_eq!(report.fixed_holdout.exact_tasks, report.holdout_tasks);
    assert!(report.selected_training_events < report.fixed_training_events);
}

#[test]
fn post_selection_holdout_must_not_overlap_shared_development_panel() {
    let candidates = candidates();
    let portfolio = run_curriculum_portfolio(&candidates, PortfolioPolicy::default()).unwrap();
    let holdout = suite_from_case_id(portfolio.selection_start_case_id, 1).unwrap();
    let policy = PostSelectionPolicy {
        min_holdout_tasks: 1,
        ..PostSelectionPolicy::default()
    };

    assert!(matches!(
        evaluate_post_selection_holdout(&portfolio, &candidates, &holdout, policy),
        Err(PostSelectionError::HoldoutSelectionOverlap(_))
    ));
}

#[test]
fn post_selection_holdout_must_not_overlap_any_training_curriculum_identity() {
    let candidates = candidates();
    let portfolio = run_curriculum_portfolio(&candidates, PortfolioPolicy::default()).unwrap();
    let holdout = suite_from_case_id(1_000_000, 1).unwrap();
    let policy = PostSelectionPolicy {
        min_holdout_tasks: 1,
        ..PostSelectionPolicy::default()
    };

    assert!(matches!(
        evaluate_post_selection_holdout(&portfolio, &candidates, &holdout, policy),
        Err(PostSelectionError::HoldoutCurriculumOverlap(_))
    ));
}

#[test]
fn holdout_oracle_is_revalidated_after_selection() {
    let candidates = candidates();
    let portfolio = run_curriculum_portfolio(&candidates, PortfolioPolicy::default()).unwrap();
    let mut holdout = suite_from_case_id(5_000_000, 16).unwrap();
    holdout[0].expected = match holdout[0].expected {
        TaskAnswer::Bit(value) => TaskAnswer::Bit(!value),
        TaskAnswer::Bits(_) => unreachable!(),
    };

    assert!(matches!(
        evaluate_post_selection_holdout(
            &portfolio,
            &candidates,
            &holdout,
            PostSelectionPolicy::default(),
        ),
        Err(PostSelectionError::OracleMismatch(_))
    ));
}

#[test]
fn holdout_requires_all_six_task_families_under_baseline_policy() {
    let candidates = candidates();
    let portfolio = run_curriculum_portfolio(&candidates, PortfolioPolicy::default()).unwrap();
    let holdout = vec![sml_core::tasks::delayed_bit_recall(5_000_000, 3)];
    let policy = PostSelectionPolicy {
        min_holdout_tasks: 1,
        require_all_task_families: true,
    };

    assert!(matches!(
        evaluate_post_selection_holdout(&portfolio, &candidates, &holdout, policy),
        Err(PostSelectionError::IncompleteFamilyCoverage(_))
    ));
}

#[test]
fn duplicate_holdout_task_identity_fails_closed() {
    let candidates = candidates();
    let portfolio = run_curriculum_portfolio(&candidates, PortfolioPolicy::default()).unwrap();
    let task = sml_core::tasks::delayed_bit_recall(5_000_000, 3);
    let holdout = vec![task.clone(), task];
    let policy = PostSelectionPolicy {
        min_holdout_tasks: 1,
        require_all_task_families: false,
    };

    assert_eq!(
        evaluate_post_selection_holdout(&portfolio, &candidates, &holdout, policy),
        Err(PostSelectionError::DuplicateHoldoutTask(5_000_000))
    );
}

#[test]
fn portfolio_and_candidate_identity_sets_must_match_at_holdout_time() {
    let all = candidates();
    let portfolio = run_curriculum_portfolio(&all, PortfolioPolicy::default()).unwrap();
    let holdout = suite_from_case_id(5_000_000, 16).unwrap();

    assert_eq!(
        evaluate_post_selection_holdout(
            &portfolio,
            &all[..1],
            &holdout,
            PostSelectionPolicy::default(),
        ),
        Err(PostSelectionError::CandidateBindingMismatch)
    );
}

#[test]
fn tampered_selected_id_is_rejected_before_holdout_execution() {
    let candidates = candidates();
    let mut portfolio = run_curriculum_portfolio(&candidates, PortfolioPolicy::default()).unwrap();
    portfolio.selected_id = "forged-selection".into();
    let holdout = suite_from_case_id(5_000_000, 16).unwrap();

    assert_eq!(
        evaluate_post_selection_holdout(
            &portfolio,
            &candidates,
            &holdout,
            PostSelectionPolicy::default(),
        ),
        Err(PostSelectionError::InvalidPortfolio)
    );
}

#[test]
fn minimum_holdout_size_is_checked_before_candidate_evaluation() {
    let candidates = candidates();
    let portfolio = run_curriculum_portfolio(&candidates, PortfolioPolicy::default()).unwrap();
    let holdout = suite_from_case_id(5_000_000, 1).unwrap();

    assert_eq!(
        evaluate_post_selection_holdout(
            &portfolio,
            &candidates,
            &holdout,
            PostSelectionPolicy {
                min_holdout_tasks: 100,
                ..PostSelectionPolicy::default()
            },
        ),
        Err(PostSelectionError::InsufficientHoldout {
            actual: 6,
            minimum: 100,
        })
    );
}

#[test]
fn helper_portfolio_remains_deterministic() {
    let a = portfolio();
    let b = portfolio();
    assert_eq!(a.selected_id, b.selected_id);
    assert_eq!(a.selected_index, b.selected_index);
    assert_eq!(a.selection_tasks, b.selection_tasks);
}
