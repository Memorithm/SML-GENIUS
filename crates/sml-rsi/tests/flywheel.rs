use sml_core::tasks::{suite_from_case_id, TaskKind};
use sml_rsi::{
    curriculum::{build_verified_curriculum, CurriculumPolicy, CurriculumProposal},
    flywheel::{FlywheelError, FlywheelSignal, RsiFlywheelRecord, RSI_FLYWHEEL_SCHEMA_V1},
    portfolio::{
        run_curriculum_portfolio, CurriculumCandidate, CurriculumSource, PortfolioPolicy,
    },
    post_selection::{
        evaluate_post_selection_holdout, PostSelectionOutcome, PostSelectionPolicy,
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

fn candidates(proposed_difficulty: u16) -> Vec<CurriculumCandidate> {
    vec![
        CurriculumCandidate {
            candidate_id: "fixed-control".into(),
            source: CurriculumSource::FixedControl,
            curriculum: curriculum(7),
        },
        CurriculumCandidate {
            candidate_id: "model-proposed".into(),
            source: CurriculumSource::ModelProposed,
            curriculum: curriculum(proposed_difficulty),
        },
    ]
}

fn reports() -> (
    sml_rsi::portfolio::CurriculumPortfolioReport,
    sml_rsi::post_selection::PostSelectionReport,
) {
    let candidates = candidates(3);
    let portfolio = run_curriculum_portfolio(&candidates, PortfolioPolicy::default()).unwrap();
    let holdout = suite_from_case_id(5_000_000, 16).unwrap();
    let post = evaluate_post_selection_holdout(
        &portfolio,
        &candidates,
        &holdout,
        PostSelectionPolicy::default(),
    )
    .unwrap();
    (portfolio, post)
}

#[test]
fn measured_tie_with_less_training_work_becomes_efficiency_signal() {
    let (portfolio, post) = reports();
    let record = RsiFlywheelRecord::from_reports(&portfolio, &post).unwrap();

    assert_eq!(record.schema, RSI_FLYWHEEL_SCHEMA_V1);
    assert_eq!(record.selected_id, "model-proposed");
    assert_eq!(record.selected_source, CurriculumSource::ModelProposed);
    assert_eq!(record.fixed_control_id, "fixed-control");
    assert_eq!(
        record.signal,
        FlywheelSignal::EquivalentQualityLowerTrainingWork
    );
    assert_eq!(record.selected_holdout_exact, record.holdout_tasks);
    assert_eq!(record.fixed_holdout_exact, record.holdout_tasks);
    assert!(record.selected_training_events < record.fixed_training_events);
}

#[test]
fn flywheel_record_carries_learned_micro_op_codes_but_no_holdout_examples() {
    let (portfolio, post) = reports();
    let record = RsiFlywheelRecord::from_reports(&portfolio, &post).unwrap();
    let text = record.canonical_text().unwrap();

    assert_eq!(record.conjunction_table_code, 0b1000);
    assert_eq!(record.overwrite_table_code, 0b1100);
    assert_eq!(record.recall_table_code, 0b10);
    assert_eq!(record.copy_table_code, 0b10);
    assert!(!text.contains("5000000"));
    assert!(!text.contains("QueryConjunction"));
    assert!(!text.contains("PayloadBit"));
    assert!(text.contains("holdout_exact="));
}

#[test]
fn canonical_flywheel_record_is_deterministic() {
    let (portfolio, post) = reports();
    let record = RsiFlywheelRecord::from_reports(&portfolio, &post).unwrap();
    assert_eq!(record.canonical_text().unwrap(), record.canonical_text().unwrap());
}

#[test]
fn selected_binding_mismatch_is_rejected() {
    let (mut portfolio, post) = reports();
    portfolio.selected_id = "forged".into();
    assert_eq!(
        RsiFlywheelRecord::from_reports(&portfolio, &post),
        Err(FlywheelError::SelectedBindingMismatch)
    );
}

#[test]
fn fixed_control_binding_must_resolve_to_a_fixed_source() {
    let (portfolio, mut post) = reports();
    post.fixed_control_id = "model-proposed".into();
    assert_eq!(
        RsiFlywheelRecord::from_reports(&portfolio, &post),
        Err(FlywheelError::FixedControlBindingMismatch)
    );
}

#[test]
fn training_work_counters_must_match_frozen_portfolio() {
    let (portfolio, mut post) = reports();
    post.selected_training_events = post.selected_training_events.saturating_add(1);
    assert_eq!(
        RsiFlywheelRecord::from_reports(&portfolio, &post),
        Err(FlywheelError::TrainingWorkMismatch)
    );
}

#[test]
fn holdout_outcome_and_delta_are_recomputed_not_trusted() {
    let (portfolio, mut post) = reports();
    post.exact_task_delta = 1;
    assert_eq!(
        RsiFlywheelRecord::from_reports(&portfolio, &post),
        Err(FlywheelError::OutcomeMismatch)
    );

    let (_, mut post) = reports();
    post.outcome = PostSelectionOutcome::SelectedBetter;
    assert_eq!(
        RsiFlywheelRecord::from_reports(&portfolio, &post),
        Err(FlywheelError::OutcomeMismatch)
    );
}

#[test]
fn quality_improvement_and_regression_signals_derive_from_measured_counts() {
    let (portfolio, mut improved) = reports();
    improved.fixed_holdout.exact_tasks = improved.fixed_holdout.exact_tasks.saturating_sub(1);
    improved.exact_task_delta = 1;
    improved.outcome = PostSelectionOutcome::SelectedBetter;
    let record = RsiFlywheelRecord::from_reports(&portfolio, &improved).unwrap();
    assert_eq!(record.signal, FlywheelSignal::HoldoutQualityImproved);

    let (portfolio, mut regressed) = reports();
    regressed.selected_holdout.exact_tasks =
        regressed.selected_holdout.exact_tasks.saturating_sub(1);
    regressed.exact_task_delta = -1;
    regressed.outcome = PostSelectionOutcome::SelectedWorse;
    let record = RsiFlywheelRecord::from_reports(&portfolio, &regressed).unwrap();
    assert_eq!(record.signal, FlywheelSignal::HoldoutQualityRegressed);
}

#[test]
fn equal_work_and_equal_quality_produce_no_efficiency_gain() {
    let equal_candidates = vec![
        CurriculumCandidate {
            candidate_id: "fixed-control".into(),
            source: CurriculumSource::FixedControl,
            curriculum: curriculum(3),
        },
        CurriculumCandidate {
            candidate_id: "model-proposed".into(),
            source: CurriculumSource::ModelProposed,
            curriculum: curriculum(3),
        },
    ];
    let portfolio =
        run_curriculum_portfolio(&equal_candidates, PortfolioPolicy::default()).unwrap();
    assert_eq!(portfolio.selected_id, "fixed-control");
    let holdout = suite_from_case_id(5_000_000, 16).unwrap();
    let post = evaluate_post_selection_holdout(
        &portfolio,
        &equal_candidates,
        &holdout,
        PostSelectionPolicy::default(),
    )
    .unwrap();
    let record = RsiFlywheelRecord::from_reports(&portfolio, &post).unwrap();

    assert_eq!(
        record.signal,
        FlywheelSignal::EquivalentQualityNoEfficiencyGain
    );
}

#[test]
fn invalid_holdout_panel_accounting_is_rejected() {
    let (portfolio, mut post) = reports();
    post.selected_holdout.exact_tasks = post.selected_holdout.tasks.saturating_add(1);
    assert_eq!(
        RsiFlywheelRecord::from_reports(&portfolio, &post),
        Err(FlywheelError::InvalidPanelAccounting)
    );
}
