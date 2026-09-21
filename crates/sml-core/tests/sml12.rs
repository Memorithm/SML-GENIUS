use sml_core::{
    candidate::{SmlTaskCandidate, TaskCandidateConfig},
    task_loss::{
        evaluate_task_panel, train_state_update_from_tasks, StateUpdateTaskTrainingConfig,
        TaskLossError,
    },
    tasks::suite_from_case_id,
    train::TruthTable2,
};

#[test]
fn task_loss_recovers_overwrite_state_update_without_table_target() {
    let (candidate, report) =
        train_state_update_from_tasks(StateUpdateTaskTrainingConfig::default()).unwrap();

    assert_eq!(report.task_samples, 64);
    assert_eq!(report.input_pattern_mask, 0b1111);
    assert_eq!(report.learned_boolean_bits, 4);
    assert_eq!(report.latent_training_scalars, 16);
    assert_eq!(report.gate.final_table, TruthTable2::RIGHT);
    assert_eq!(report.gate.hard_correct, 64);
    assert!(report.gate.final_loss < report.gate.initial_loss);
    assert_eq!(candidate.state_update_table(), TruthTable2::RIGHT);
}

#[test]
fn learned_state_update_generalizes_to_disjoint_case_ids() {
    let (candidate, _) =
        train_state_update_from_tasks(StateUpdateTaskTrainingConfig::default()).unwrap();
    let holdout = suite_from_case_id(10_000, 64).unwrap();
    let evaluation = evaluate_task_panel(&candidate, &holdout).unwrap();

    assert_eq!(evaluation.tasks, 384);
    assert_eq!(evaluation.exact_tasks, 384);
    assert_eq!(evaluation.overwrite_tasks, 64);
    assert_eq!(evaluation.exact_overwrite_tasks, 64);
}

#[test]
fn learned_state_update_beats_wrong_xor_and_matches_fixed_overwrite() {
    let holdout = suite_from_case_id(10_000, 64).unwrap();

    let (learned, _) =
        train_state_update_from_tasks(StateUpdateTaskTrainingConfig::default()).unwrap();
    let learned_eval = evaluate_task_panel(&learned, &holdout).unwrap();

    let (wrong_xor, _) = SmlTaskCandidate::train_with_micro_ops(
        TaskCandidateConfig::default(),
        TruthTable2::AND,
        TruthTable2::XOR,
    )
    .unwrap();
    let wrong_eval = evaluate_task_panel(&wrong_xor, &holdout).unwrap();

    let (fixed, _) = SmlTaskCandidate::train(TaskCandidateConfig::default()).unwrap();
    let fixed_eval = evaluate_task_panel(&fixed, &holdout).unwrap();

    assert!(wrong_eval.exact_tasks < learned_eval.exact_tasks);
    assert!(wrong_eval.exact_overwrite_tasks < learned_eval.exact_overwrite_tasks);
    assert_eq!(fixed_eval.exact_tasks, learned_eval.exact_tasks);
    assert_eq!(
        fixed_eval.exact_overwrite_tasks,
        learned_eval.exact_overwrite_tasks
    );
}

#[test]
fn state_update_task_loss_training_is_deterministic() {
    let (left, left_report) =
        train_state_update_from_tasks(StateUpdateTaskTrainingConfig::default()).unwrap();
    let (right, right_report) =
        train_state_update_from_tasks(StateUpdateTaskTrainingConfig::default()).unwrap();

    assert_eq!(left, right);
    assert_eq!(left_report, right_report);
}

#[test]
fn insufficient_state_update_coverage_fails_closed() {
    let config = StateUpdateTaskTrainingConfig {
        train_cases: 1,
        ..StateUpdateTaskTrainingConfig::default()
    };
    assert!(matches!(
        train_state_update_from_tasks(config),
        Err(TaskLossError::IncompleteInputCoverage(_))
    ));
}
