use sml_core::{
    candidate::{SmlTaskCandidate, TaskCandidateConfig},
    task_loss::{
        evaluate_task_panel, train_conjunction_from_tasks, ConjunctionTaskTrainingConfig,
        TaskLossError,
    },
    tasks::suite_from_case_id,
    train::{GateSample2, GateTrainingConfig, LatentGate2, TruthTable2},
};

#[test]
fn arbitrary_sample_optimizer_learns_boolean_gate() {
    let samples = [
        GateSample2 {
            left: false,
            right: false,
            target: false,
        },
        GateSample2 {
            left: true,
            right: false,
            target: false,
        },
        GateSample2 {
            left: false,
            right: true,
            target: false,
        },
        GateSample2 {
            left: true,
            right: true,
            target: true,
        },
    ];
    let mut gate = LatentGate2::default();
    let report = gate
        .train_samples(
            &samples,
            GateTrainingConfig {
                epochs: 600,
                learning_rate: 0.2,
                epsilon: 1.0e-6,
            },
        )
        .unwrap();

    assert_eq!(report.final_table, TruthTable2::AND);
    assert_eq!(report.hard_correct, 4);
    assert_eq!(report.sample_count, 4);
    assert!(report.final_loss < report.initial_loss);
}

#[test]
fn task_loss_recovers_conjunction_without_truth_table_target() {
    let (candidate, report) =
        train_conjunction_from_tasks(ConjunctionTaskTrainingConfig::default()).unwrap();

    assert_eq!(report.task_samples, 64);
    assert_eq!(report.input_pattern_mask, 0b1111);
    assert_eq!(report.learned_boolean_bits, 4);
    assert_eq!(report.latent_training_scalars, 16);
    assert_eq!(report.gate.final_table, TruthTable2::AND);
    assert_eq!(report.gate.hard_correct, 64);
    assert!(report.gate.final_loss < report.gate.initial_loss);
    assert_eq!(candidate.conjunction_table(), TruthTable2::AND);
}

#[test]
fn learned_conjunction_generalizes_to_disjoint_task_case_ids() {
    let (candidate, _) =
        train_conjunction_from_tasks(ConjunctionTaskTrainingConfig::default()).unwrap();
    let holdout = suite_from_case_id(10_000, 64).unwrap();
    let evaluation = evaluate_task_panel(&candidate, &holdout).unwrap();

    assert_eq!(evaluation.tasks, 320);
    assert_eq!(evaluation.exact_tasks, 320);
    assert_eq!(evaluation.conjunction_tasks, 64);
    assert_eq!(evaluation.exact_conjunction_tasks, 64);
    assert!(evaluation.trace_records > evaluation.tasks);
    assert_eq!(evaluation.route_lookups, evaluation.trace_records);
}

#[test]
fn learned_gate_beats_wrong_gate_but_not_hand_coded_and_oracle() {
    let holdout = suite_from_case_id(10_000, 64).unwrap();

    let (learned, _) =
        train_conjunction_from_tasks(ConjunctionTaskTrainingConfig::default()).unwrap();
    let learned_eval = evaluate_task_panel(&learned, &holdout).unwrap();

    let (wrong_xor, _) =
        SmlTaskCandidate::train_with_conjunction(TaskCandidateConfig::default(), TruthTable2::XOR)
            .unwrap();
    let wrong_eval = evaluate_task_panel(&wrong_xor, &holdout).unwrap();

    let (fixed_and, _) = SmlTaskCandidate::train(TaskCandidateConfig::default()).unwrap();
    let fixed_eval = evaluate_task_panel(&fixed_and, &holdout).unwrap();

    assert_eq!(learned_eval.exact_tasks, 320);
    assert!(wrong_eval.exact_tasks < learned_eval.exact_tasks);
    assert!(wrong_eval.exact_conjunction_tasks < learned_eval.exact_conjunction_tasks);
    assert_eq!(fixed_eval.exact_tasks, learned_eval.exact_tasks);
    assert_eq!(
        fixed_eval.exact_conjunction_tasks,
        learned_eval.exact_conjunction_tasks
    );
}

#[test]
fn task_loss_training_is_deterministic() {
    let (left, left_report) =
        train_conjunction_from_tasks(ConjunctionTaskTrainingConfig::default()).unwrap();
    let (right, right_report) =
        train_conjunction_from_tasks(ConjunctionTaskTrainingConfig::default()).unwrap();

    assert_eq!(left, right);
    assert_eq!(left_report, right_report);
}

#[test]
fn insufficient_task_coverage_fails_closed() {
    let config = ConjunctionTaskTrainingConfig {
        train_cases: 1,
        ..ConjunctionTaskTrainingConfig::default()
    };
    assert!(matches!(
        train_conjunction_from_tasks(config),
        Err(TaskLossError::IncompleteInputCoverage(_))
    ));
}
