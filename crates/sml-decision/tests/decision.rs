use sml_decision::{
    training::{fit_boolean_expert, FitError, FitOutcome, FitPolicy, LabeledDecision},
    Abstention, BatchBudget, Candidate, DecisionError, DecisionHead, DecisionOutcome,
    DecisionProgram, DecisionValue, GateSpec, HeadKind, InputRecord, ProgramLimits,
    SignalRef::{Gate, Input}, TruthTable2, Uncertainty,
};

fn record(id: u64, values: u64, known: u64) -> InputRecord {
    InputRecord { record_id: id, schema_id: 7, values, known }
}

fn binary_program(table: TruthTable2) -> DecisionProgram {
    DecisionProgram::compile(7, 1, 3, vec![GateSpec { left: Input(0), right: Input(1), table }],
        vec![DecisionHead { question_id: 42, applicable: Input(2), kind: HeadKind::Binary(Gate(0)) }],
        ProgramLimits::default()).unwrap()
}

fn scalar(table: TruthTable2, left: Option<bool>, right: Option<bool>) -> Option<bool> {
    let mut outputs = Vec::new();
    for l in [false, true] {
        if left.is_some_and(|x| x != l) { continue; }
        for r in [false, true] {
            if right.is_some_and(|x| x != r) { continue; }
            outputs.push(table.evaluate(l, r));
        }
    }
    if outputs.iter().all(|x| *x == outputs[0]) { Some(outputs[0]) } else { None }
}

#[test]
fn all_tables_and_partial_inputs_match_independent_scalar_oracle() {
    for code in 0..16 {
        let table = TruthTable2::new(code).unwrap();
        let program = binary_program(table);
        let mut records = Vec::new();
        let mut expected = Vec::new();
        for l in [None, Some(false), Some(true)] {
            for r in [None, Some(false), Some(true)] {
                let known = 4 | u64::from(l.is_some()) | (u64::from(r.is_some()) << 1);
                let values = 4 | u64::from(l.unwrap_or(false)) | (u64::from(r.unwrap_or(false)) << 1);
                records.push(record(records.len() as u64, values, known));
                expected.push(match scalar(table, l, r) {
                    Some(value) => DecisionOutcome::Proposed(DecisionValue::Binary(value)),
                    None => DecisionOutcome::Abstain(Abstention::MissingInformation),
                });
            }
        }
        let actual = program.evaluate(&records, BatchBudget::default()).unwrap();
        assert_eq!(actual.decisions.iter().map(|d| d.outcome).collect::<Vec<_>>(), expected);
        assert_eq!(actual.text_tokens_generated, 0);
        assert!(actual.decisions.iter().all(|d| d.uncertainty == Uncertainty::Uncalibrated));
    }
}

#[test]
fn tails_group_boundaries_and_multi_gate_composition_are_exact() {
    let program = DecisionProgram::compile(7, 9, 3,
        vec![GateSpec { left: Input(0), right: Input(1), table: TruthTable2::XOR },
             GateSpec { left: Gate(0), right: Input(1), table: TruthTable2::AND }],
        vec![DecisionHead { question_id: 1, applicable: Input(2), kind: HeadKind::Binary(Gate(1)) }],
        ProgramLimits::default()).unwrap();
    for count in [0, 1, 63, 64, 65, 127, 128, 129, 4096] {
        let rows: Vec<_> = (0..count).map(|i| record(i as u64, 4 | (i as u64 & 3), 7)).collect();
        let batch = program.evaluate(&rows, BatchBudget::default()).unwrap();
        assert_eq!(batch.decisions.len(), count);
        assert_eq!(batch.plan.logical_gate_evaluations, count as u64 * 2);
        assert_eq!(batch.plan.word_gate_evaluations, count.div_ceil(64) as u64 * 2);
        assert!(batch.head_signal_reads <= batch.plan.head_signal_read_bound);
        for (i, decision) in batch.decisions.iter().enumerate() {
            let left = i & 1 != 0;
            let right = i & 2 != 0;
            assert_eq!(decision.outcome, DecisionOutcome::Proposed(DecisionValue::Binary((left ^ right) & right)));
            assert_eq!(decision.record_id, i as u64);
            assert_eq!(decision.model_version, 9);
        }
    }
}

#[test]
fn unknown_aliased_input_does_not_invent_two_independent_inputs() {
    let program = DecisionProgram::compile(7, 1, 2,
        vec![GateSpec { left: Input(0), right: Input(0), table: TruthTable2::XOR }],
        vec![DecisionHead { question_id: 1, applicable: Input(1), kind: HeadKind::Binary(Gate(0)) }],
        ProgramLimits::default()).unwrap();
    let batch = program.evaluate(&[record(1, 2, 2)], BatchBudget::default()).unwrap();
    assert_eq!(batch.decisions[0].outcome, DecisionOutcome::Proposed(DecisionValue::Binary(false)));
}

#[test]
fn typed_heads_share_circuit_and_preserve_abstention_reasons() {
    let program = DecisionProgram::compile(7, 1, 3, vec![], vec![
        DecisionHead { question_id: 10, applicable: Input(2), kind: HeadKind::Binary(Input(0)) },
        DecisionHead { question_id: 11, applicable: Input(2), kind: HeadKind::Choice(vec![
            Candidate { id: 900, eligible: Input(0) }, Candidate { id: 12, eligible: Input(1) }]) },
        DecisionHead { question_id: 12, applicable: Input(2), kind: HeadKind::Ordinal {
            bits: vec![Input(0), Input(1)], maximum: 2 } },
    ], ProgramLimits::default()).unwrap();
    let batch = program.evaluate(&[record(0, 5, 7), record(1, 7, 7), record(2, 4, 7),
        record(3, 5, 5), record(4, 1, 7), record(5, 1, 3)], BatchBudget::default()).unwrap();
    assert_eq!(batch.decisions.len(), 18);
    assert_eq!(batch.decisions[1].outcome, DecisionOutcome::Proposed(DecisionValue::Choice(900)));
    assert_eq!(batch.decisions[2].outcome, DecisionOutcome::Proposed(DecisionValue::Ordinal(1)));
    assert_eq!(batch.decisions[4].outcome, DecisionOutcome::Abstain(Abstention::AmbiguousCandidates));
    assert_eq!(batch.decisions[5].outcome, DecisionOutcome::Abstain(Abstention::OrdinalOutOfRange));
    assert_eq!(batch.decisions[7].outcome, DecisionOutcome::Abstain(Abstention::NoEligibleCandidate));
    assert_eq!(batch.decisions[10].outcome, DecisionOutcome::Abstain(Abstention::MissingInformation));
    assert_eq!(batch.decisions[12].outcome, DecisionOutcome::Abstain(Abstention::Inapplicable));
    assert_eq!(batch.decisions[15].outcome, DecisionOutcome::Abstain(Abstention::MissingInformation));
    assert_eq!(batch.plan.feature_extractions, 18);
}

#[test]
fn budgets_and_overflow_fail_before_execution() {
    let program = binary_program(TruthTable2::AND);
    let rows = [record(0, 7, 7), record(1, 7, 7)];
    for budget in [
        BatchBudget { max_records: 1, ..BatchBudget::default() },
        BatchBudget { max_results: 1, ..BatchBudget::default() },
        BatchBudget { max_result_bytes: 1, ..BatchBudget::default() },
        BatchBudget { max_scratch_bytes: 1, ..BatchBudget::default() },
        BatchBudget { max_logical_gate_evaluations: 1, ..BatchBudget::default() },
    ] {
        assert_eq!(program.evaluate(&rows, budget), Err(DecisionError::BatchBudget));
    }
    assert_eq!(program.plan(usize::MAX, BatchBudget::default()), Err(DecisionError::Overflow));
    let zero = BatchBudget { max_records: 0, max_results: 0, max_result_bytes: 0,
        max_scratch_bytes: 0, max_logical_gate_evaluations: 0 };
    assert_eq!(program.evaluate(&[], zero).unwrap().plan.scratch_bytes, 0);
}

#[test]
fn malformed_records_reject_the_whole_batch_without_changing_program() {
    let program = binary_program(TruthTable2::AND);
    for row in [record(1, 1, 0), record(1, 0, 8)] {
        assert_eq!(program.evaluate(&[record(0, 7, 7), row], BatchBudget::default()), Err(DecisionError::InvalidRecord));
    }
    let mut wrong = record(1, 7, 7);
    wrong.schema_id = 8;
    assert_eq!(program.evaluate(&[wrong], BatchBudget::default()), Err(DecisionError::SchemaMismatch));
    assert_eq!(program.evaluate(&[record(0, 7, 7)], BatchBudget::default()).unwrap().decisions.len(), 1);
}

#[test]
fn constructor_rejects_cycles_duplicate_ids_and_invalid_shapes() {
    let head = DecisionHead { question_id: 1, applicable: Input(0), kind: HeadKind::Binary(Input(0)) };
    let bad_gate = GateSpec { left: Gate(0), right: Input(0), table: TruthTable2::AND };
    assert!(matches!(DecisionProgram::compile(7, 1, 1, vec![bad_gate], vec![head.clone()], ProgramLimits::default()), Err(DecisionError::InvalidSignal)));
    assert!(matches!(DecisionProgram::compile(7, 1, 1, vec![], vec![head.clone(), head.clone()], ProgramLimits::default()), Err(DecisionError::DuplicateQuestion)));
    assert!(matches!(DecisionProgram::compile(7, 1, 65, vec![], vec![head.clone()], ProgramLimits::default()), Err(DecisionError::InvalidShape)));
    let duplicate = DecisionHead { kind: HeadKind::Choice(vec![Candidate { id: 1, eligible: Input(0) }; 2]), ..head.clone() };
    assert!(matches!(DecisionProgram::compile(7, 1, 1, vec![], vec![duplicate], ProgramLimits::default()), Err(DecisionError::DuplicateCandidate)));
    let ordinal = DecisionHead { kind: HeadKind::Ordinal { bits: vec![Input(0)], maximum: 2 }, ..head };
    assert!(matches!(DecisionProgram::compile(7, 1, 1, vec![], vec![ordinal], ProgramLimits::default()), Err(DecisionError::InvalidShape)));
}

fn samples(start: u64, count: usize, table: TruthTable2) -> Vec<LabeledDecision> {
    (0..count).map(|i| LabeledDecision { case_id: start + i as u64,
        left: i & 1 != 0, right: i & 2 != 0,
        target: table.evaluate(i & 1 != 0, i & 2 != 0) }).collect()
}

#[test]
fn finite_boolean_learning_recovers_all_tables_with_separate_validation() {
    for code in 0..16 {
        let target = TruthTable2::new(code).unwrap();
        let incumbent = TruthTable2::new(code ^ 15).unwrap();
        let train = samples(0, 16, target);
        let validation = samples(100, 8, target);
        let report = fit_boolean_expert(&train, &validation, incumbent, FitPolicy::default()).unwrap();
        assert_eq!(report.outcome, FitOutcome::Accepted(target));
        assert_eq!(report.candidate_training_errors, 0);
        assert_eq!(report.candidate_validation_errors, 0);
        assert_eq!(report.committed_table_bits, 4);
        assert_eq!(report.selection_gate_evaluations, 256);
        assert_eq!(report.total_gate_evaluations, 288);
        assert_eq!(report, fit_boolean_expert(&train, &validation, incumbent, FitPolicy::default()).unwrap());
        let final_test = samples(10_000, 129, target);
        let inputs: Vec<_> = final_test.iter().map(|s| record(s.case_id, 4 | u64::from(s.left) | (u64::from(s.right) << 1), 7)).collect();
        let program = binary_program(match report.outcome { FitOutcome::Accepted(table) => table, FitOutcome::Rejected => panic!("accepted fixture") });
        let output = program.evaluate(&inputs, BatchBudget::default()).unwrap();
        for (actual, expected) in output.decisions.iter().zip(final_test) {
            assert_eq!(actual.outcome, DecisionOutcome::Proposed(DecisionValue::Binary(expected.target)));
        }
    }
}

#[test]
fn validation_rejects_regression_without_selecting_another_table() {
    let train = samples(0, 16, TruthTable2::AND);
    let validation = samples(100, 8, TruthTable2::XOR);
    let report = fit_boolean_expert(&train, &validation, TruthTable2::XOR, FitPolicy::default()).unwrap();
    assert_eq!(report.selected, TruthTable2::AND);
    assert_eq!(report.outcome, FitOutcome::Rejected);
    assert_eq!(report.committed_table_bits, 0);
}

#[test]
fn fitter_rejects_overlap_duplicates_missing_coverage_and_sample_budget() {
    let train = samples(0, 8, TruthTable2::AND);
    let validation = samples(100, 8, TruthTable2::AND);
    assert_eq!(fit_boolean_expert(&train, &train, TruthTable2::FALSE, FitPolicy::default()), Err(FitError::DuplicateCaseId));
    assert_eq!(fit_boolean_expert(&train[..1], &validation, TruthTable2::FALSE, FitPolicy::default()), Err(FitError::IncompleteInputCoverage));
    assert_eq!(fit_boolean_expert(&[], &validation, TruthTable2::FALSE, FitPolicy::default()), Err(FitError::EmptySplit));
    assert_eq!(fit_boolean_expert(&train, &validation, TruthTable2::FALSE, FitPolicy { max_samples: 15, ..FitPolicy::default() }), Err(FitError::SampleBudget));
}

#[test]
fn reference_evidence_reports_bit_slicing_not_claimed_speedup() {
    let rows: Vec<_> = (0..4096).map(|i| record(i, 4 | (i & 3), 7)).collect();
    let result = binary_program(TruthTable2::XOR).evaluate(&rows, BatchBudget::default()).unwrap();
    assert_eq!(result.plan.logical_gate_evaluations, 4096);
    assert_eq!(result.plan.word_gate_evaluations, 64);
    assert_eq!(result.plan.feature_extractions, 12_288);
    assert_eq!(result.decisions.len(), 4096);
    assert_eq!(result.abstentions, 0);
    println!("SML-D0 records={} logical_gate_evaluations={} word_gate_evaluations={} feature_extractions={} decisions={} text_tokens={}",
        result.plan.records, result.plan.logical_gate_evaluations, result.plan.word_gate_evaluations,
        result.plan.feature_extractions, result.decisions.len(), result.text_tokens_generated);
}
