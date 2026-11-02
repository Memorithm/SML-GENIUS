//! Table-driven rejection matrix for task-loss training and panel evaluation.

use sml_core::{
    boolean_integral::{BooleanGateTrainingConfig, BooleanIntegralError},
    candidate::{SmlTaskCandidate, TaskCandidateConfig, TaskCandidateError},
    task_loss::{
        evaluate_task_panel, train_conjunction_from_tasks,
        train_conjunction_from_tasks_boolean_integral, train_recall_copy_from_tasks,
        train_state_update_from_tasks, train_state_update_from_tasks_boolean_integral,
        BooleanIntegralTaskTrainingConfig, ConjunctionTaskTrainingConfig,
        RecallCopyTaskTrainingConfig, StateUpdateTaskTrainingConfig, TaskLossError,
    },
    tasks::{TaskAnswer, TaskError, TaskEvent, TaskInstance, TaskKind},
    train::{GateTrainingConfig, TrainError},
};

type Outcome = Result<(), TaskLossError>;

/// Runs every gradient-trained entry point with the same shared knobs.
fn gradient_trainers(
    train_start_case_id: u64,
    train_cases: u32,
    gate_training: GateTrainingConfig,
    candidate: TaskCandidateConfig,
) -> [(&'static str, Outcome); 3] {
    [
        (
            "conjunction",
            train_conjunction_from_tasks(ConjunctionTaskTrainingConfig {
                train_start_case_id,
                train_cases,
                gate_training,
                candidate,
            })
            .map(|_| ()),
        ),
        (
            "state_update",
            train_state_update_from_tasks(StateUpdateTaskTrainingConfig {
                train_start_case_id,
                train_cases,
                gate_training,
                candidate,
            })
            .map(|_| ()),
        ),
        (
            "recall_copy",
            train_recall_copy_from_tasks(RecallCopyTaskTrainingConfig {
                train_start_case_id,
                train_cases,
                gate_training,
                candidate,
            })
            .map(|_| ()),
        ),
    ]
}

/// Runs both Boolean-integral entry points with the same config.
fn integral_trainers(config: BooleanIntegralTaskTrainingConfig) -> [(&'static str, Outcome); 2] {
    [
        (
            "integral_conjunction",
            train_conjunction_from_tasks_boolean_integral(config).map(|_| ()),
        ),
        (
            "integral_state_update",
            train_state_update_from_tasks_boolean_integral(config).map(|_| ()),
        ),
    ]
}

fn default_gate() -> GateTrainingConfig {
    ConjunctionTaskTrainingConfig::default().gate_training
}

#[test]
fn positive_control_all_trainers_accept_defaults() {
    for (label, outcome) in gradient_trainers(0, 64, default_gate(), TaskCandidateConfig::default())
    {
        assert_eq!(outcome, Ok(()), "{label}");
    }
    for (label, outcome) in integral_trainers(BooleanIntegralTaskTrainingConfig::default()) {
        assert_eq!(outcome, Ok(()), "{label}");
    }
}

#[test]
fn zero_training_cases_is_rejected_before_any_other_check() {
    // Every other knob is also invalid: the zero-case guard must win.
    let bad_gate = GateTrainingConfig {
        epochs: 0,
        learning_rate: f64::NAN,
        epsilon: 0.0,
    };
    let bad_candidate = TaskCandidateConfig {
        memory_capacity: 0,
        max_payload_bits: 0,
        ..TaskCandidateConfig::default()
    };
    for (label, outcome) in gradient_trainers(u64::MAX, 0, bad_gate, bad_candidate) {
        assert_eq!(outcome, Err(TaskLossError::ZeroTrainingCases), "{label}");
    }
    let integral = BooleanIntegralTaskTrainingConfig {
        train_start_case_id: u64::MAX,
        train_cases: 0,
        gate_training: BooleanGateTrainingConfig {
            max_passes: 0,
            decision_confidence_ppm: u32::MAX,
        },
        candidate: bad_candidate,
    };
    for (label, outcome) in integral_trainers(integral) {
        assert_eq!(outcome, Err(TaskLossError::ZeroTrainingCases), "{label}");
    }
}

#[test]
fn case_id_overflow_is_rejected() {
    let expected = Err(TaskLossError::Task(TaskError::CaseIdOverflow));
    for (label, outcome) in
        gradient_trainers(u64::MAX, 1, default_gate(), TaskCandidateConfig::default())
    {
        assert_eq!(outcome, expected, "{label}");
    }
    let integral = BooleanIntegralTaskTrainingConfig {
        train_start_case_id: u64::MAX - 3,
        train_cases: 4,
        ..BooleanIntegralTaskTrainingConfig::default()
    };
    for (label, outcome) in integral_trainers(integral) {
        assert_eq!(outcome, expected, "{label}");
    }
}

#[test]
fn incomplete_input_coverage_is_rejected() {
    // A single case per kind cannot cover every Boolean input row.
    let [(_, conjunction), (_, state_update), (_, recall_copy)] =
        gradient_trainers(0, 1, default_gate(), TaskCandidateConfig::default());
    for (label, outcome) in [("conjunction", conjunction), ("state_update", state_update)] {
        assert!(
            matches!(outcome, Err(TaskLossError::IncompleteInputCoverage(mask)) if mask != 0b1111),
            "{label}: {outcome:?}"
        );
    }
    assert!(
        matches!(
            recall_copy,
            Err(TaskLossError::IncompleteUnaryInputCoverage(mask)) if mask != 0b11
        ),
        "recall_copy: {recall_copy:?}"
    );

    let integral = BooleanIntegralTaskTrainingConfig {
        train_cases: 1,
        ..BooleanIntegralTaskTrainingConfig::default()
    };
    for (label, outcome) in integral_trainers(integral) {
        assert!(
            matches!(outcome, Err(TaskLossError::IncompleteInputCoverage(mask)) if mask != 0b1111),
            "{label}: {outcome:?}"
        );
    }
}

#[test]
fn invalid_gradient_gate_config_is_rejected() {
    let base = default_gate();
    let cases = [
        (
            "zero epochs",
            GateTrainingConfig { epochs: 0, ..base },
            TrainError::ZeroEpochs,
        ),
        (
            "zero learning rate",
            GateTrainingConfig {
                learning_rate: 0.0,
                ..base
            },
            TrainError::InvalidLearningRate,
        ),
        (
            "negative learning rate",
            GateTrainingConfig {
                learning_rate: -0.2,
                ..base
            },
            TrainError::InvalidLearningRate,
        ),
        (
            "NaN learning rate",
            GateTrainingConfig {
                learning_rate: f64::NAN,
                ..base
            },
            TrainError::InvalidLearningRate,
        ),
        (
            "infinite learning rate",
            GateTrainingConfig {
                learning_rate: f64::INFINITY,
                ..base
            },
            TrainError::InvalidLearningRate,
        ),
        (
            "zero epsilon",
            GateTrainingConfig {
                epsilon: 0.0,
                ..base
            },
            TrainError::InvalidEpsilon,
        ),
        (
            "epsilon at one half",
            GateTrainingConfig {
                epsilon: 0.5,
                ..base
            },
            TrainError::InvalidEpsilon,
        ),
        (
            "NaN epsilon",
            GateTrainingConfig {
                epsilon: f64::NAN,
                ..base
            },
            TrainError::InvalidEpsilon,
        ),
    ];
    for (case, gate, expected) in cases {
        for (label, outcome) in gradient_trainers(0, 64, gate, TaskCandidateConfig::default()) {
            assert_eq!(
                outcome,
                Err(TaskLossError::Train(expected)),
                "{label}: {case}"
            );
        }
    }
}

#[test]
fn invalid_boolean_integral_gate_config_is_rejected() {
    let cases = [
        (
            "zero passes",
            BooleanGateTrainingConfig {
                max_passes: 0,
                ..BooleanGateTrainingConfig::default()
            },
            BooleanIntegralError::ZeroTrainingPasses,
        ),
        (
            "confidence above one million ppm",
            BooleanGateTrainingConfig {
                decision_confidence_ppm: 1_000_001,
                ..BooleanGateTrainingConfig::default()
            },
            BooleanIntegralError::ConfidenceOutOfRange(1_000_001),
        ),
    ];
    for (case, gate_training, expected) in cases {
        let config = BooleanIntegralTaskTrainingConfig {
            gate_training,
            ..BooleanIntegralTaskTrainingConfig::default()
        };
        for (label, outcome) in integral_trainers(config) {
            assert_eq!(
                outcome,
                Err(TaskLossError::BooleanIntegral(expected)),
                "{label}: {case}"
            );
        }
    }
}

#[test]
fn unconverged_gate_is_never_committed() {
    // One pass with maximal confidence cannot reach the overwrite table
    // from FALSE; the trainer must refuse to build a candidate.
    let config = BooleanIntegralTaskTrainingConfig {
        gate_training: BooleanGateTrainingConfig {
            max_passes: 1,
            decision_confidence_ppm: 1_000_000,
        },
        ..BooleanIntegralTaskTrainingConfig::default()
    };
    let outcome = train_state_update_from_tasks_boolean_integral(config).map(|_| ());
    assert!(
        matches!(
            outcome,
            Err(TaskLossError::GateNotConverged { correct, total }) if correct < total
        ),
        "{outcome:?}"
    );
}

#[test]
fn invalid_candidate_config_is_rejected_after_gate_training() {
    let zero_memory = TaskCandidateConfig {
        memory_capacity: 0,
        ..TaskCandidateConfig::default()
    };
    let zero_payload = TaskCandidateConfig {
        max_payload_bits: 0,
        ..TaskCandidateConfig::default()
    };
    let cases = [
        (zero_memory, TaskCandidateError::ZeroMemoryCapacity),
        (zero_payload, TaskCandidateError::ZeroPayloadBudget),
    ];
    for (candidate, expected) in cases {
        for (label, outcome) in gradient_trainers(0, 64, default_gate(), candidate) {
            assert_eq!(outcome, Err(TaskLossError::Candidate(expected)), "{label}");
        }
        let integral = BooleanIntegralTaskTrainingConfig {
            candidate,
            ..BooleanIntegralTaskTrainingConfig::default()
        };
        for (label, outcome) in integral_trainers(integral) {
            assert_eq!(outcome, Err(TaskLossError::Candidate(expected)), "{label}");
        }
    }
}

#[test]
fn panel_evaluation_propagates_execution_rejections() {
    use TaskEvent::*;
    let candidate = SmlTaskCandidate::fixed_route(TaskCandidateConfig::default()).unwrap();
    let forged = |kind, events| TaskInstance {
        kind,
        case_id: 88_888,
        events,
        expected: TaskAnswer::Bit(false),
    };
    let valid = forged(
        TaskKind::DelayedBitRecall,
        vec![StoreBit(true), QueryRecall],
    );

    // Positive control: empty panel yields zero counters; valid panel is counted.
    assert_eq!(evaluate_task_panel(&candidate, &[]), Ok(Default::default()));
    let report = evaluate_task_panel(&candidate, std::slice::from_ref(&valid)).unwrap();
    assert_eq!(report.tasks, 1);
    assert_eq!(report.delayed_recall_tasks, 1);

    let cases = [
        (
            "no answer emitted",
            forged(TaskKind::DelayedBitRecall, vec![StoreBit(true)]),
            TaskCandidateError::MissingPrediction,
        ),
        (
            "two answers emitted",
            forged(
                TaskKind::DelayedBitRecall,
                vec![StoreBit(true), QueryRecall, QueryRecall],
            ),
            TaskCandidateError::MultiplePredictions,
        ),
        (
            "recall before store",
            forged(TaskKind::DelayedBitRecall, vec![QueryRecall]),
            TaskCandidateError::MissingStoredBit,
        ),
        (
            "conjunction with unknown key",
            forged(
                TaskKind::TwoFactConjunction,
                vec![
                    Fact {
                        key: 1,
                        value: true,
                    },
                    QueryConjunction { left: 1, right: 9 },
                ],
            ),
            TaskCandidateError::MissingFact(9),
        ),
        (
            "payload before marker",
            forged(TaskKind::CopyAfterMarker, vec![PayloadBit(true)]),
            TaskCandidateError::PayloadBeforeMarker,
        ),
    ];
    for (label, bad, expected) in cases {
        // The bad instance must fail the whole panel even after a valid one.
        let panel = [valid.clone(), bad];
        assert_eq!(
            evaluate_task_panel(&candidate, &panel),
            Err(TaskLossError::Candidate(expected)),
            "{label}"
        );
    }
}
