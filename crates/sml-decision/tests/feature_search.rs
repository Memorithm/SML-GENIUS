use sml_core::semantic::{SemanticBitField, SemanticState};
use sml_decision::{
    feature_search::{
        search_feature_pair_expert, FeatureSearchError, FeatureSearchOutcome, FeatureSearchPolicy,
        FixedFeatureControl,
    },
    semantic::{
        LabeledSemanticInput, ObservedFields, ProjectionBudget, SemanticDecisionProgram,
        SemanticFeature, SemanticInput, SemanticProjection,
    },
    BatchBudget, DecisionHead, DecisionOutcome, DecisionProgram, DecisionValue, GateSpec, HeadKind,
    ProgramLimits,
    SignalRef::{Gate, Input},
    TruthTable2,
};

fn candidate_projection<const BITS: usize>() -> SemanticProjection<BITS> {
    SemanticProjection::new(
        9,
        3,
        vec![
            SemanticFeature::ContextValue(0),
            SemanticFeature::ContextValue(1),
            SemanticFeature::ContextValue(2),
            SemanticFeature::ContextValue(7),
            SemanticFeature::ContextValue(9),
            SemanticFeature::ContextValue(101),
            SemanticFeature::ContextValue(130),
            SemanticFeature::ContextValue(200),
        ],
    )
    .unwrap()
}

fn state<const BITS: usize>(index: usize) -> SemanticState<BITS> {
    let a = index & 1 != 0;
    let b = index & 2 != 0;
    let c = index & 4 != 0;
    let d = index & 8 != 0;
    let mut state = SemanticState::<BITS>::zeroed().unwrap();
    state.meaning_mask = SemanticBitField::ones().unwrap();
    for (coordinate, value) in [
        (0, c),
        (1, d),
        (2, c ^ d),
        (7, a),
        (9, c & d),
        (101, b),
        (130, c | d),
        (200, c == d),
    ] {
        state.value.set(coordinate, value).unwrap();
    }
    state
}

fn observed<'a, const BITS: usize>(
    record_id: u64,
    state: &'a SemanticState<BITS>,
    mask: &'a SemanticBitField<BITS>,
) -> SemanticInput<'a, BITS> {
    SemanticInput {
        record_id,
        schema_id: 9,
        projection_version: 3,
        state,
        observed: ObservedFields {
            value: mask,
            meaning_mask: mask,
            ambiguity: mask,
            routing: mask,
        },
    }
}

fn labeled<'a, const BITS: usize>(
    start: u64,
    states: &'a [SemanticState<BITS>],
    mask: &'a SemanticBitField<BITS>,
    table: TruthTable2,
) -> Vec<LabeledSemanticInput<'a, BITS>> {
    states
        .iter()
        .enumerate()
        .map(|(index, state)| LabeledSemanticInput {
            input: observed(start + index as u64, state, mask),
            target: table.evaluate(index & 1 != 0, index & 2 != 0),
        })
        .collect()
}

fn wrong_control() -> FixedFeatureControl {
    FixedFeatureControl {
        feature_slots: [0, 1],
        table: TruthTable2::AND,
    }
}

#[test]
fn training_only_search_recovers_hidden_sbg_pair_and_truth_table() {
    let states: Vec<_> = (0..16).map(state::<257>).collect();
    let mask = SemanticBitField::ones().unwrap();
    let train = labeled(0, &states, &mask, TruthTable2::XOR);
    let validation = labeled(100, &states, &mask, TruthTable2::XOR);

    let report = search_feature_pair_expert(
        &candidate_projection(),
        &train,
        &validation,
        wrong_control(),
        FeatureSearchPolicy::default(),
        ProjectionBudget::default(),
    )
    .unwrap();

    assert_eq!(report.outcome, FeatureSearchOutcome::Accepted);
    assert_eq!(report.selected.feature_slots(), [3, 5]);
    assert_eq!(
        report.selected.features(),
        [
            SemanticFeature::ContextValue(7),
            SemanticFeature::ContextValue(101)
        ]
    );
    assert_eq!(report.selected.table, TruthTable2::XOR);
    assert_eq!(report.selected_training_errors, 0);
    assert_eq!(report.selected_validation_errors, 0);
    assert_eq!(report.selected_validation_missing, 0);
    assert_eq!(report.selected_validation_coverage, 0b1111);
    assert!(report.baseline_training_errors > report.selected_training_errors);
    assert!(report.baseline_validation_errors > report.selected_validation_errors);
    assert!(report.selection_evaluations_executed <= report.selection_evaluations_bound);
}

#[test]
fn selected_pair_runs_through_the_existing_sbg_decision_path() {
    let states: Vec<_> = (0..16).map(state::<257>).collect();
    let mask = SemanticBitField::ones().unwrap();
    let train = labeled(0, &states, &mask, TruthTable2::XOR);
    let validation = labeled(100, &states, &mask, TruthTable2::XOR);
    let report = search_feature_pair_expert(
        &candidate_projection(),
        &train,
        &validation,
        wrong_control(),
        FeatureSearchPolicy::default(),
        ProjectionBudget::default(),
    )
    .unwrap();

    let selected = report.selected.features();
    let projection = SemanticProjection::new(
        9,
        4,
        vec![selected[0], selected[1], SemanticFeature::Meaning(256)],
    )
    .unwrap();
    let decision = DecisionProgram::compile(
        9,
        7,
        3,
        vec![GateSpec {
            left: Input(0),
            right: Input(1),
            table: report.selected.table,
        }],
        vec![DecisionHead {
            question_id: 44,
            applicable: Input(2),
            kind: HeadKind::Binary(Gate(0)),
        }],
        ProgramLimits::default(),
    )
    .unwrap();
    let program = SemanticDecisionProgram::new(projection, decision).unwrap();

    let runtime_inputs: Vec<_> = states
        .iter()
        .enumerate()
        .map(|(index, state)| SemanticInput {
            projection_version: 4,
            ..observed(10_000 + index as u64, state, &mask)
        })
        .collect();
    let output = program
        .evaluate(
            &runtime_inputs,
            BatchBudget::default(),
            ProjectionBudget::default(),
        )
        .unwrap();

    assert_eq!(output.decisions.text_tokens_generated, 0);
    assert_eq!(output.decisions.decisions.len(), 16);
    for (index, decision) in output.decisions.decisions.iter().enumerate() {
        assert_eq!(
            decision.outcome,
            DecisionOutcome::Proposed(DecisionValue::Binary(
                TruthTable2::XOR.evaluate(index & 1 != 0, index & 2 != 0)
            ))
        );
    }
}

#[test]
fn validation_shift_rejects_without_reselecting_the_training_winner() {
    let states: Vec<_> = (0..16).map(state::<257>).collect();
    let mask = SemanticBitField::ones().unwrap();
    let train = labeled(0, &states, &mask, TruthTable2::XOR);
    let validation = labeled(100, &states, &mask, TruthTable2::AND);

    let report = search_feature_pair_expert(
        &candidate_projection(),
        &train,
        &validation,
        wrong_control(),
        FeatureSearchPolicy::default(),
        ProjectionBudget::default(),
    )
    .unwrap();

    assert_eq!(report.outcome, FeatureSearchOutcome::Rejected);
    assert_eq!(report.selected.feature_slots(), [3, 5]);
    assert_eq!(report.selected.table, TruthTable2::XOR);
    assert_eq!(report.selected_training_errors, 0);
    assert!(report.selected_validation_errors > 0);
}

#[test]
fn missing_selected_validation_feature_counts_as_failure_and_rejects() {
    let states: Vec<_> = (0..16).map(state::<257>).collect();
    let full = SemanticBitField::ones().unwrap();
    let mut missing_value = SemanticBitField::ones().unwrap();
    missing_value.set(101, false).unwrap();
    let train = labeled(0, &states, &full, TruthTable2::XOR);
    let mut validation = labeled(100, &states, &full, TruthTable2::XOR);
    validation[0].input.observed.value = &missing_value;

    let report = search_feature_pair_expert(
        &candidate_projection(),
        &train,
        &validation,
        wrong_control(),
        FeatureSearchPolicy::default(),
        ProjectionBudget::default(),
    )
    .unwrap();

    assert_eq!(report.selected.feature_slots(), [3, 5]);
    assert_eq!(report.selected.table, TruthTable2::XOR);
    assert_eq!(report.selected_validation_missing, 1);
    assert_eq!(report.outcome, FeatureSearchOutcome::Rejected);
}

#[test]
fn a_perfect_fixed_control_prevents_false_improvement_claim() {
    let states: Vec<_> = (0..16).map(state::<257>).collect();
    let mask = SemanticBitField::ones().unwrap();
    let train = labeled(0, &states, &mask, TruthTable2::XOR);
    let validation = labeled(100, &states, &mask, TruthTable2::XOR);
    let baseline = FixedFeatureControl {
        feature_slots: [3, 5],
        table: TruthTable2::XOR,
    };

    let report = search_feature_pair_expert(
        &candidate_projection(),
        &train,
        &validation,
        baseline,
        FeatureSearchPolicy::default(),
        ProjectionBudget::default(),
    )
    .unwrap();

    assert_eq!(report.selected.feature_slots(), [3, 5]);
    assert_eq!(report.selected.table, TruthTable2::XOR);
    assert_eq!(report.baseline_training_errors, 0);
    assert_eq!(report.selected_training_errors, 0);
    assert_eq!(report.outcome, FeatureSearchOutcome::Rejected);
}

#[test]
fn selection_is_deterministic_and_validation_ids_do_not_change_it() {
    let states: Vec<_> = (0..16).map(state::<257>).collect();
    let mask = SemanticBitField::ones().unwrap();
    let train = labeled(0, &states, &mask, TruthTable2::XOR);
    let validation_a = labeled(100, &states, &mask, TruthTable2::XOR);
    let validation_b = labeled(10_000, &states, &mask, TruthTable2::XOR);

    let left = search_feature_pair_expert(
        &candidate_projection(),
        &train,
        &validation_a,
        wrong_control(),
        FeatureSearchPolicy::default(),
        ProjectionBudget::default(),
    )
    .unwrap();
    let right = search_feature_pair_expert(
        &candidate_projection(),
        &train,
        &validation_b,
        wrong_control(),
        FeatureSearchPolicy::default(),
        ProjectionBudget::default(),
    )
    .unwrap();

    assert_eq!(left.selected, right.selected);
    assert_eq!(
        left.selected_training_errors,
        right.selected_training_errors
    );
    assert_eq!(
        left.selection_evaluations_executed,
        right.selection_evaluations_executed
    );
}

#[test]
fn candidate_pair_and_evaluation_budgets_fail_before_search() {
    let states: Vec<_> = (0..16).map(state::<257>).collect();
    let mask = SemanticBitField::ones().unwrap();
    let train = labeled(0, &states, &mask, TruthTable2::XOR);
    let validation = labeled(100, &states, &mask, TruthTable2::XOR);

    for policy in [
        FeatureSearchPolicy {
            max_candidates: 7,
            ..FeatureSearchPolicy::default()
        },
        FeatureSearchPolicy {
            max_candidate_pairs: 27,
            ..FeatureSearchPolicy::default()
        },
        FeatureSearchPolicy {
            max_selection_evaluations: 1,
            ..FeatureSearchPolicy::default()
        },
    ] {
        assert!(matches!(
            search_feature_pair_expert(
                &candidate_projection(),
                &train,
                &validation,
                wrong_control(),
                policy,
                ProjectionBudget::default(),
            ),
            Err(FeatureSearchError::CandidateBudget
                | FeatureSearchError::PairBudget
                | FeatureSearchError::EvaluationBudget)
        ));
    }
}

#[test]
fn duplicate_case_ids_and_invalid_fixed_control_fail_closed() {
    let states: Vec<_> = (0..16).map(state::<257>).collect();
    let mask = SemanticBitField::ones().unwrap();
    let train = labeled(0, &states, &mask, TruthTable2::XOR);
    let overlapping = labeled(0, &states, &mask, TruthTable2::XOR);

    assert_eq!(
        search_feature_pair_expert(
            &candidate_projection(),
            &train,
            &overlapping,
            wrong_control(),
            FeatureSearchPolicy::default(),
            ProjectionBudget::default(),
        ),
        Err(FeatureSearchError::DuplicateCaseId)
    );

    let validation = labeled(100, &states, &mask, TruthTable2::XOR);
    assert_eq!(
        search_feature_pair_expert(
            &candidate_projection(),
            &train,
            &validation,
            FixedFeatureControl {
                feature_slots: [0, 0],
                table: TruthTable2::AND,
            },
            FeatureSearchPolicy::default(),
            ProjectionBudget::default(),
        ),
        Err(FeatureSearchError::InvalidBaselinePair)
    );
}

#[test]
fn reference_width_search_uses_only_declared_coordinates() {
    let states: Vec<_> = (0..16).map(state::<16_384>).collect();
    let mask = SemanticBitField::ones().unwrap();
    let projection = candidate_projection::<16_384>();
    let train = labeled(0, &states, &mask, TruthTable2::XOR);
    let validation = labeled(100, &states, &mask, TruthTable2::XOR);

    let report = search_feature_pair_expert(
        &projection,
        &train,
        &validation,
        wrong_control(),
        FeatureSearchPolicy::default(),
        ProjectionBudget::default(),
    )
    .unwrap();

    assert_eq!(report.outcome, FeatureSearchOutcome::Accepted);
    assert_eq!(report.selected.feature_slots(), [3, 5]);
    assert_eq!(report.projection.records, 32);
    assert_eq!(
        report.projection.feature_queries,
        32 * projection.features().len() as u64
    );
    assert_eq!(report.selected_training_errors, 0);
    assert_eq!(report.selected_validation_errors, 0);
}
