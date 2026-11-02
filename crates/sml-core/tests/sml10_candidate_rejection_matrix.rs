//! Table-driven rejection matrix for the integrated SML-1 task candidate.

use sml_core::{
    candidate::{AssociativeMemoryMode, SmlTaskCandidate, TaskCandidateConfig, TaskCandidateError},
    tasks::{TaskAnswer, TaskEvent, TaskInstance, TaskKind},
};

fn instance(kind: TaskKind, events: Vec<TaskEvent>) -> TaskInstance {
    TaskInstance {
        kind,
        case_id: 77_777,
        events,
        expected: TaskAnswer::Bit(false),
    }
}

fn fixed(config: TaskCandidateConfig) -> SmlTaskCandidate {
    SmlTaskCandidate::fixed_route(config).unwrap()
}

#[test]
fn config_rejections_are_shared_by_train_and_fixed_route() {
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
    for (config, expected) in cases {
        assert_eq!(SmlTaskCandidate::fixed_route(config), Err(expected));
        assert_eq!(SmlTaskCandidate::train(config).unwrap_err(), expected);
    }

    // Zero capacity is only invalid when keyed memory is actually used.
    let disabled = TaskCandidateConfig {
        memory_capacity: 0,
        memory_mode: AssociativeMemoryMode::DisabledZeroRead,
        ..TaskCandidateConfig::default()
    };
    let candidate = fixed(disabled);
    assert_eq!(
        candidate.memory_mode(),
        AssociativeMemoryMode::DisabledZeroRead
    );
    let evidence = candidate
        .execute(&instance(
            TaskKind::KeyedFactRecall,
            vec![TaskEvent::QueryKey(3)],
        ))
        .unwrap();
    assert_eq!(evidence.predicted, TaskAnswer::Bit(false));
}

#[test]
fn execution_rejections() {
    use TaskEvent::*;
    let candidate = fixed(TaskCandidateConfig::default());
    let cases: Vec<(&str, TaskKind, Vec<TaskEvent>, TaskCandidateError)> = vec![
        (
            "no answer emitted",
            TaskKind::DelayedBitRecall,
            vec![StoreBit(true), Distractor(1)],
            TaskCandidateError::MissingPrediction,
        ),
        (
            "empty event stream",
            TaskKind::DelayedBitRecall,
            vec![],
            TaskCandidateError::MissingPrediction,
        ),
        (
            "two answers emitted",
            TaskKind::DelayedBitRecall,
            vec![StoreBit(true), QueryRecall, QueryRecall],
            TaskCandidateError::MultiplePredictions,
        ),
        (
            "recall before store",
            TaskKind::DelayedBitRecall,
            vec![QueryRecall],
            TaskCandidateError::MissingStoredBit,
        ),
        (
            "query unknown key",
            TaskKind::KeyedFactRecall,
            vec![
                Fact {
                    key: 1,
                    value: true,
                },
                QueryKey(3),
            ],
            TaskCandidateError::MissingFact(3),
        ),
        (
            "conjunction with unknown right key",
            TaskKind::KeyedFactRecall,
            vec![
                Fact {
                    key: 1,
                    value: true,
                },
                QueryConjunction { left: 1, right: 4 },
            ],
            TaskCandidateError::MissingFact(4),
        ),
        (
            "copy without marker",
            TaskKind::DelayedBitRecall,
            vec![QueryCopy],
            TaskCandidateError::MissingPayload,
        ),
        (
            "copy with empty payload",
            TaskKind::DelayedBitRecall,
            vec![Marker, QueryCopy],
            TaskCandidateError::MissingPayload,
        ),
        (
            "payload before marker",
            TaskKind::DelayedBitRecall,
            vec![PayloadBit(true)],
            TaskCandidateError::PayloadBeforeMarker,
        ),
    ];
    for (label, kind, events, expected) in cases {
        assert_eq!(
            candidate.execute(&instance(kind, events)).unwrap_err(),
            expected,
            "{label}"
        );
    }
}

#[test]
fn payload_budget_is_exact() {
    use TaskEvent::*;
    let candidate = fixed(TaskCandidateConfig {
        max_payload_bits: 2,
        ..TaskCandidateConfig::default()
    });
    let at_budget = candidate
        .execute(&instance(
            TaskKind::DelayedBitRecall,
            vec![Marker, PayloadBit(true), PayloadBit(false), QueryCopy],
        ))
        .unwrap();
    assert_eq!(at_budget.predicted, TaskAnswer::Bits(vec![true, false]));

    assert_eq!(
        candidate
            .execute(&instance(
                TaskKind::DelayedBitRecall,
                vec![
                    Marker,
                    PayloadBit(true),
                    PayloadBit(false),
                    PayloadBit(true)
                ],
            ))
            .unwrap_err(),
        TaskCandidateError::PayloadBudget { maximum: 2 }
    );

    // A new marker clears the payload, so the budget is per marker window.
    let rearmed = candidate
        .execute(&instance(
            TaskKind::DelayedBitRecall,
            vec![
                Marker,
                PayloadBit(true),
                PayloadBit(true),
                Marker,
                PayloadBit(false),
                QueryCopy,
            ],
        ))
        .unwrap();
    assert_eq!(rearmed.predicted, TaskAnswer::Bits(vec![false]));
}
