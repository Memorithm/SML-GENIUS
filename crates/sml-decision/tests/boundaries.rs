use sml_decision::{
    Abstention, BatchBudget, DecisionError, DecisionHead, DecisionOutcome, DecisionProgram,
    DecisionValue, GateSpec, HeadKind, InputRecord, ProgramLimits,
    SignalRef::{Gate, Input},
    TruthTable2,
};

#[test]
fn highest_feature_bit_remains_distinct_from_unknown_and_false() {
    let program = DecisionProgram::compile(
        1,
        1,
        64,
        vec![GateSpec {
            left: Input(0),
            right: Input(63),
            table: TruthTable2::RIGHT,
        }],
        vec![DecisionHead {
            question_id: 1,
            applicable: Input(62),
            kind: HeadKind::Binary(Gate(0)),
        }],
        ProgramLimits::default(),
    )
    .unwrap();
    let guard = 1_u64 << 62;
    let high = 1_u64 << 63;
    let inputs: Vec<_> = [
        (guard | high, guard | high),
        (guard, guard | high),
        (guard, guard),
    ]
    .into_iter()
    .enumerate()
    .map(|(id, (values, known))| InputRecord {
        record_id: id as u64,
        schema_id: 1,
        values,
        known,
    })
    .collect();
    let result = program.evaluate(&inputs, BatchBudget::default()).unwrap();
    assert_eq!(
        result.decisions[0].outcome,
        DecisionOutcome::Proposed(DecisionValue::Binary(true))
    );
    assert_eq!(
        result.decisions[1].outcome,
        DecisionOutcome::Proposed(DecisionValue::Binary(false))
    );
    assert_eq!(
        result.decisions[2].outcome,
        DecisionOutcome::Abstain(Abstention::MissingInformation)
    );
}

#[test]
fn multiple_heads_do_not_repeat_shared_gate_or_feature_execution() {
    let heads: Vec<_> = (0..8)
        .map(|question_id| DecisionHead {
            question_id,
            applicable: Input(2),
            kind: HeadKind::Binary(Gate(0)),
        })
        .collect();
    let program = DecisionProgram::compile(
        1,
        1,
        3,
        vec![GateSpec {
            left: Input(0),
            right: Input(1),
            table: TruthTable2::AND,
        }],
        heads,
        ProgramLimits::default(),
    )
    .unwrap();
    let inputs: Vec<_> = (0..64)
        .map(|record_id| InputRecord {
            record_id,
            schema_id: 1,
            values: 7,
            known: 7,
        })
        .collect();
    let result = program.evaluate(&inputs, BatchBudget::default()).unwrap();
    assert_eq!(result.decisions.len(), 512);
    assert_eq!(result.plan.logical_gate_evaluations, 64);
    assert_eq!(result.plan.word_gate_evaluations, 1);
    assert_eq!(result.plan.feature_extractions, 192);
    assert_eq!(result.head_signal_reads, 1024);
    assert_eq!(program.logical_table_bits(), 4);
    for (index, decision) in result.decisions.iter().enumerate() {
        assert_eq!(decision.record_id, (index / 8) as u64);
        assert_eq!(decision.question_id, (index % 8) as u64);
        assert_eq!(
            decision.outcome,
            DecisionOutcome::Proposed(DecisionValue::Binary(true))
        );
    }
}

#[test]
fn reconvergent_unknowns_may_abstain_but_never_invent_determined_answers() {
    for code in 0..16 {
        let final_table = TruthTable2::new(code).unwrap();
        let program = DecisionProgram::compile(
            1,
            1,
            3,
            vec![
                GateSpec {
                    left: Input(0),
                    right: Input(1),
                    table: TruthTable2::AND,
                },
                GateSpec {
                    left: Input(0),
                    right: Input(1),
                    table: TruthTable2::XOR,
                },
                GateSpec {
                    left: Gate(0),
                    right: Gate(1),
                    table: final_table,
                },
            ],
            vec![DecisionHead {
                question_id: 1,
                applicable: Input(2),
                kind: HeadKind::Binary(Gate(2)),
            }],
            ProgramLimits::default(),
        )
        .unwrap();
        for left in [None, Some(false), Some(true)] {
            for right in [None, Some(false), Some(true)] {
                let input = InputRecord {
                    record_id: 1,
                    schema_id: 1,
                    values: 4
                        | u64::from(left.unwrap_or(false))
                        | (u64::from(right.unwrap_or(false)) << 1),
                    known: 4 | u64::from(left.is_some()) | (u64::from(right.is_some()) << 1),
                };
                let result = program.evaluate(&[input], BatchBudget::default()).unwrap();
                match result.decisions[0].outcome {
                    DecisionOutcome::Proposed(DecisionValue::Binary(value)) => {
                        for l in [false, true] {
                            for r in [false, true] {
                                if left.is_some_and(|x| x != l) || right.is_some_and(|x| x != r) {
                                    continue;
                                }
                                assert_eq!(value, final_table.evaluate(l & r, l ^ r));
                            }
                        }
                    }
                    DecisionOutcome::Abstain(Abstention::MissingInformation) => {}
                    other => panic!("unexpected binary outcome: {other:?}"),
                }
            }
        }
    }
}

#[test]
fn each_program_admission_dimension_is_enforced() {
    for limits in [
        ProgramLimits {
            max_gates: 0,
            ..ProgramLimits::default()
        },
        ProgramLimits {
            max_heads: 0,
            ..ProgramLimits::default()
        },
        ProgramLimits {
            max_head_signals: 1,
            ..ProgramLimits::default()
        },
    ] {
        let result = DecisionProgram::compile(
            1,
            1,
            2,
            vec![GateSpec {
                left: Input(0),
                right: Input(1),
                table: TruthTable2::AND,
            }],
            vec![DecisionHead {
                question_id: 1,
                applicable: Input(1),
                kind: HeadKind::Binary(Gate(0)),
            }],
            limits,
        );
        assert!(matches!(result, Err(DecisionError::ProgramBudget)));
    }
}
