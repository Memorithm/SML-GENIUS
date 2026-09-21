use sml_core::{
    candidate::{SmlTaskCandidate, TaskCandidateConfig},
    task_loss::{
        evaluate_task_panel, train_recall_copy_from_tasks, RecallCopyTaskTrainingConfig,
        TaskLossError,
    },
    tasks::suite_from_case_id,
    train::{TruthTable1, TruthTable2},
};

#[test]
fn task_loss_recovers_unary_recall_and_copy_without_table_targets() {
    let (candidate, report) =
        train_recall_copy_from_tasks(RecallCopyTaskTrainingConfig::default()).unwrap();

    assert_eq!(report.recall_samples, 64);
    assert_eq!(report.copy_samples, 288);
    assert_eq!(report.recall_input_pattern_mask, 0b11);
    assert_eq!(report.copy_input_pattern_mask, 0b11);
    assert_eq!(report.learned_boolean_bits, 4);
    assert_eq!(report.latent_training_scalars, 8);
    assert_eq!(report.recall_gate.final_table, TruthTable1::IDENTITY);
    assert_eq!(report.copy_gate.final_table, TruthTable1::IDENTITY);
    assert_eq!(report.recall_gate.hard_correct, report.recall_samples);
    assert_eq!(report.copy_gate.hard_correct, report.copy_samples);
    assert!(report.recall_gate.final_loss < report.recall_gate.initial_loss);
    assert!(report.copy_gate.final_loss < report.copy_gate.initial_loss);
    assert_eq!(candidate.recall_table(), TruthTable1::IDENTITY);
    assert_eq!(candidate.copy_table(), TruthTable1::IDENTITY);
}

#[test]
fn learned_recall_and_copy_generalize_to_disjoint_case_ids() {
    let (candidate, _) =
        train_recall_copy_from_tasks(RecallCopyTaskTrainingConfig::default()).unwrap();
    let holdout = suite_from_case_id(10_000, 64).unwrap();
    let evaluation = evaluate_task_panel(&candidate, &holdout).unwrap();

    assert_eq!(evaluation.tasks, 384);
    assert_eq!(evaluation.exact_tasks, 384);
    assert_eq!(evaluation.delayed_recall_tasks, 64);
    assert_eq!(evaluation.exact_delayed_recall_tasks, 64);
    assert_eq!(evaluation.copy_tasks, 64);
    assert_eq!(evaluation.exact_copy_tasks, 64);
}

#[test]
fn learned_unary_semantics_beat_wrong_not_and_match_fixed_identity() {
    let holdout = suite_from_case_id(10_000, 64).unwrap();

    let (learned, _) =
        train_recall_copy_from_tasks(RecallCopyTaskTrainingConfig::default()).unwrap();
    let learned_eval = evaluate_task_panel(&learned, &holdout).unwrap();

    let (wrong_not, _) = SmlTaskCandidate::train_with_all_micro_ops(
        TaskCandidateConfig::default(),
        TruthTable2::AND,
        TruthTable2::RIGHT,
        TruthTable1::NOT,
        TruthTable1::NOT,
    )
    .unwrap();
    let wrong_eval = evaluate_task_panel(&wrong_not, &holdout).unwrap();

    let (fixed, _) = SmlTaskCandidate::train(TaskCandidateConfig::default()).unwrap();
    let fixed_eval = evaluate_task_panel(&fixed, &holdout).unwrap();

    assert!(wrong_eval.exact_delayed_recall_tasks < learned_eval.exact_delayed_recall_tasks);
    assert!(wrong_eval.exact_copy_tasks < learned_eval.exact_copy_tasks);
    assert_eq!(
        fixed_eval.exact_delayed_recall_tasks,
        learned_eval.exact_delayed_recall_tasks
    );
    assert_eq!(fixed_eval.exact_copy_tasks, learned_eval.exact_copy_tasks);
    assert_eq!(fixed_eval.exact_tasks, learned_eval.exact_tasks);
}

#[test]
fn recall_copy_task_loss_training_is_deterministic() {
    let (left, left_report) =
        train_recall_copy_from_tasks(RecallCopyTaskTrainingConfig::default()).unwrap();
    let (right, right_report) =
        train_recall_copy_from_tasks(RecallCopyTaskTrainingConfig::default()).unwrap();

    assert_eq!(left, right);
    assert_eq!(left_report, right_report);
}

#[test]
fn insufficient_unary_coverage_fails_closed() {
    let config = RecallCopyTaskTrainingConfig {
        train_cases: 1,
        ..RecallCopyTaskTrainingConfig::default()
    };
    assert!(matches!(
        train_recall_copy_from_tasks(config),
        Err(TaskLossError::IncompleteUnaryInputCoverage(_))
    ));
}
