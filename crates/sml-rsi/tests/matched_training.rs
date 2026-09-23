use sml_core::{
    tasks::{TaskAnswer, TaskKind},
    train::TruthTable2,
};
use sml_rsi::{
    curriculum::{
        build_verified_curriculum, CurriculumPartition, CurriculumPolicy, CurriculumProposal,
    },
    matched_training::{
        run_matched_conjunction_training, MatchedTrainingError, MatchedTrainingOutcome,
        MatchedTrainingPolicy,
    },
};

fn proposed_curriculum(
    start: u64,
    count: u32,
    difficulty: u16,
) -> sml_rsi::curriculum::CurriculumReport {
    build_verified_curriculum(
        &[CurriculumProposal {
            proposal_id: "self-proposed-conjunction".into(),
            family: TaskKind::TwoFactConjunction,
            case_id_start: start,
            case_count: count,
            difficulty,
        }],
        CurriculumPolicy::default(),
    )
    .unwrap()
}

#[test]
fn proposed_and_fixed_curricula_train_real_candidates_at_matched_task_count() {
    let curriculum = proposed_curriculum(1_000_000, 64, 13);
    let report =
        run_matched_conjunction_training(&curriculum, MatchedTrainingPolicy::default()).unwrap();

    assert_eq!(report.proposed.training_tasks, report.fixed.training_tasks);
    assert_eq!(
        report.proposed.training_tasks,
        report.matched_training_tasks
    );
    assert_eq!(report.proposed.input_coverage, 0b1111);
    assert_eq!(report.fixed.input_coverage, 0b1111);
    assert_eq!(report.proposed.table, TruthTable2::AND);
    assert_eq!(report.fixed.table, TruthTable2::AND);
    assert_eq!(
        report.proposed.gate.hard_correct,
        report.proposed.gate.sample_count
    );
    assert_eq!(
        report.fixed.gate.hard_correct,
        report.fixed.gate.sample_count
    );
    assert_eq!(report.holdout_tasks, 64);
}

#[test]
fn current_exact_conjunction_fixture_reports_a_tie_not_a_false_rsi_gain() {
    let curriculum = proposed_curriculum(1_000_000, 64, 13);
    let report =
        run_matched_conjunction_training(&curriculum, MatchedTrainingPolicy::default()).unwrap();

    assert_eq!(report.outcome, MatchedTrainingOutcome::Tie);
    assert_eq!(report.exact_holdout_delta, 0);
    assert_eq!(
        report.proposed.holdout.exact_conjunction_tasks,
        report.holdout_tasks
    );
    assert_eq!(
        report.fixed.holdout.exact_conjunction_tasks,
        report.holdout_tasks
    );
}

#[test]
fn proposal_difficulty_may_change_event_work_without_changing_exact_truth() {
    let easy = proposed_curriculum(1_000_000, 64, 1);
    let hard = proposed_curriculum(1_100_000, 64, 127);

    let easy_report =
        run_matched_conjunction_training(&easy, MatchedTrainingPolicy::default()).unwrap();
    let hard_report =
        run_matched_conjunction_training(&hard, MatchedTrainingPolicy::default()).unwrap();

    assert_eq!(easy_report.proposed.table, TruthTable2::AND);
    assert_eq!(hard_report.proposed.table, TruthTable2::AND);
    assert_eq!(easy_report.outcome, MatchedTrainingOutcome::Tie);
    assert_eq!(hard_report.outcome, MatchedTrainingOutcome::Tie);
    assert!(hard.total_events > easy.total_events);
}

#[test]
fn proposed_fixed_and_holdout_case_ranges_must_be_disjoint() {
    let curriculum = proposed_curriculum(2_000_000, 64, 7);
    let policy = MatchedTrainingPolicy {
        fixed_start_case_id: 2_000_000,
        ..MatchedTrainingPolicy::default()
    };
    assert!(matches!(
        run_matched_conjunction_training(&curriculum, policy),
        Err(MatchedTrainingError::OverlappingCaseId(_))
    ));

    let curriculum = proposed_curriculum(3_000_000, 64, 7);
    let policy = MatchedTrainingPolicy {
        holdout_start_case_id: 3_000_000,
        ..MatchedTrainingPolicy::default()
    };
    assert!(matches!(
        run_matched_conjunction_training(&curriculum, policy),
        Err(MatchedTrainingError::OverlappingCaseId(_))
    ));
}

#[test]
fn development_rows_are_evaluated_but_do_not_change_training_sample_count() {
    let curriculum = proposed_curriculum(1_000_000, 128, 5);
    let expected_training = curriculum
        .tasks
        .iter()
        .filter(|item| item.partition == CurriculumPartition::Training)
        .count() as u64;
    let expected_development = curriculum
        .tasks
        .iter()
        .filter(|item| item.partition == CurriculumPartition::Development)
        .count() as u64;

    let report =
        run_matched_conjunction_training(&curriculum, MatchedTrainingPolicy::default()).unwrap();

    assert_eq!(report.proposed.training_tasks, expected_training);
    assert_eq!(report.proposed.development.tasks, expected_development);
    assert_eq!(
        report.proposed.development.exact_tasks,
        expected_development
    );
}

#[test]
fn tampered_curriculum_answers_are_rejected_before_training() {
    let mut curriculum = proposed_curriculum(1_000_000, 64, 5);
    curriculum.tasks[0].task.expected = match curriculum.tasks[0].task.expected {
        TaskAnswer::Bit(value) => TaskAnswer::Bit(!value),
        TaskAnswer::Bits(_) => unreachable!(),
    };

    assert!(matches!(
        run_matched_conjunction_training(&curriculum, MatchedTrainingPolicy::default()),
        Err(MatchedTrainingError::OracleMismatch(_))
    ));
}

#[test]
fn tampered_curriculum_accounting_is_rejected_before_training() {
    let mut curriculum = proposed_curriculum(1_000_000, 64, 5);
    curriculum.total_events = curriculum.total_events.saturating_add(1);

    assert_eq!(
        run_matched_conjunction_training(&curriculum, MatchedTrainingPolicy::default()),
        Err(MatchedTrainingError::InvalidCurriculumReport)
    );
}

#[test]
fn insufficient_proposed_training_fails_closed() {
    let curriculum = proposed_curriculum(1_000_000, 4, 5);
    let policy = MatchedTrainingPolicy {
        min_training_tasks: 16,
        ..MatchedTrainingPolicy::default()
    };

    assert!(matches!(
        run_matched_conjunction_training(&curriculum, policy),
        Err(MatchedTrainingError::InsufficientProposedTraining { .. })
    ));
}

#[test]
fn fixed_and_holdout_work_are_independent_of_proposed_case_ids() {
    let left = proposed_curriculum(1_000_000, 64, 9);
    let right = proposed_curriculum(1_500_000, 64, 9);

    let a = run_matched_conjunction_training(&left, MatchedTrainingPolicy::default()).unwrap();
    let b = run_matched_conjunction_training(&right, MatchedTrainingPolicy::default()).unwrap();

    assert_eq!(a.fixed.table, b.fixed.table);
    assert_eq!(a.fixed.holdout, b.fixed.holdout);
    assert_eq!(a.holdout_tasks, b.holdout_tasks);
}
