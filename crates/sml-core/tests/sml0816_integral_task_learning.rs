use sml_core::{
    task_loss::{
        evaluate_task_panel, train_conjunction_from_tasks_boolean_integral,
        train_state_update_from_tasks_boolean_integral, BooleanIntegralTaskTrainingConfig,
    },
    tasks::suite_from_case_id,
    train::TruthTable2,
};

#[test]
fn boolean_integral_task_learning_recovers_conjunction_without_latent_gradient() {
    let (candidate, report) =
        train_conjunction_from_tasks_boolean_integral(BooleanIntegralTaskTrainingConfig::default())
            .unwrap();

    assert_eq!(report.gate.initial_table, TruthTable2::FALSE);
    assert_eq!(report.gate.final_table, TruthTable2::AND);
    assert!(report.gate.initial_loss_ppm > 0);
    assert_eq!(report.gate.final_loss_ppm, 0);
    assert_eq!(report.gate.accepted_flips, 1);
    assert_eq!(report.active_credit_bits, 16);

    let holdout = suite_from_case_id(10_000, 32).unwrap();
    let evaluation = evaluate_task_panel(&candidate, &holdout).unwrap();
    assert_eq!(evaluation.exact_tasks, evaluation.tasks);
}

#[test]
fn boolean_integral_task_learning_recovers_overwrite_projection_without_latent_gradient() {
    let (candidate, report) =
        train_state_update_from_tasks_boolean_integral(BooleanIntegralTaskTrainingConfig::default())
            .unwrap();

    assert_eq!(report.gate.initial_table, TruthTable2::FALSE);
    assert_eq!(report.gate.final_table, TruthTable2::RIGHT);
    assert!(report.gate.initial_loss_ppm > 0);
    assert_eq!(report.gate.final_loss_ppm, 0);
    assert_eq!(report.gate.accepted_flips, 2);
    assert_eq!(report.active_credit_bits, 16);

    let holdout = suite_from_case_id(10_000, 32).unwrap();
    let evaluation = evaluate_task_panel(&candidate, &holdout).unwrap();
    assert_eq!(evaluation.exact_tasks, evaluation.tasks);
}

#[test]
fn boolean_gate_training_is_integer_only_and_deterministic() {
    let config = BooleanIntegralTaskTrainingConfig::default();
    let (_, first) = train_conjunction_from_tasks_boolean_integral(config).unwrap();
    let (_, second) = train_conjunction_from_tasks_boolean_integral(config).unwrap();

    assert_eq!(first.gate, second.gate);
    assert_eq!(first.gate.final_loss_ppm, 0);
    assert_eq!(first.gate.interventions, 4);
}
