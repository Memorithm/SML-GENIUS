use sml_core::semantic::{
    build_semantic_store, SemanticBitField, SemanticState, SemanticStoreView,
};
use sml_decision::{
    semantic::{
        LabeledSemanticInput, ObservedFields, ProjectionBudget, SemanticDecisionError,
        SemanticDecisionProgram, SemanticFeature, SemanticInput, SemanticProjection,
    },
    training::{FitError, FitOutcome, FitPolicy},
    Abstention, BatchBudget, Candidate, DecisionError, DecisionHead, DecisionOutcome,
    DecisionProgram, DecisionValue, GateSpec, HeadKind, ProgramLimits,
    SignalRef::{Gate, Input},
    TruthTable2, Uncertainty,
};

fn observation<'a, const BITS: usize>(
    id: u64,
    state: &'a SemanticState<BITS>,
    mask: &'a SemanticBitField<BITS>,
) -> SemanticInput<'a, BITS> {
    SemanticInput {
        record_id: id,
        schema_id: 9,
        projection_version: 2,
        state,
        observed: ObservedFields {
            value: mask,
            meaning_mask: mask,
            ambiguity: mask,
            routing: mask,
        },
    }
}

fn projection<const BITS: usize>() -> SemanticProjection<BITS> {
    SemanticProjection::new(
        9,
        2,
        vec![
            SemanticFeature::ContextValue(0),
            SemanticFeature::ContextValue(65),
            SemanticFeature::Meaning(256),
        ],
    )
    .unwrap()
}

fn state<const BITS: usize>(left: bool, right: bool) -> SemanticState<BITS> {
    let mut state = SemanticState::<BITS>::zeroed().unwrap();
    state.meaning_mask = SemanticBitField::ones().unwrap();
    state.value.set(0, left).unwrap();
    state.value.set(65, right).unwrap();
    state
}

fn binary(table: TruthTable2, heads: usize) -> DecisionProgram {
    DecisionProgram::compile(
        9,
        4,
        3,
        vec![GateSpec {
            left: Input(0),
            right: Input(1),
            table,
        }],
        (0..heads)
            .map(|id| DecisionHead {
                question_id: id as u64,
                applicable: Input(2),
                kind: HeadKind::Binary(Gate(0)),
            })
            .collect(),
        ProgramLimits::default(),
    )
    .unwrap()
}

#[test]
fn relevance_and_clear_ambiguity_never_substitute_for_observation() {
    let projection = projection::<257>();
    for state_bits in 0..8 {
        let mut state = state::<257>(state_bits & 1 != 0, true);
        state.meaning_mask.set(0, state_bits & 2 != 0).unwrap();
        state.ambiguity.set(0, state_bits & 4 != 0).unwrap();
        for availability in 0..8 {
            let mut v = SemanticBitField::ones().unwrap();
            let mut m = SemanticBitField::ones().unwrap();
            let mut a = SemanticBitField::ones().unwrap();
            v.set(0, availability & 1 != 0).unwrap();
            m.set(0, availability & 2 != 0).unwrap();
            a.set(0, availability & 4 != 0).unwrap();
            let mut input = observation(0, &state, &v);
            input.observed.meaning_mask = &m;
            input.observed.ambiguity = &a;
            let (record, counters) = projection.project(input).unwrap();
            let expected_known = availability == 7 && state_bits & 2 != 0 && state_bits & 4 == 0;
            assert_eq!(record.known & 1 != 0, expected_known);
            assert_eq!(
                record.values & 1 != 0,
                expected_known && state_bits & 1 != 0
            );
            assert_eq!(record.values & !record.known, 0);
            assert_eq!(counters.feature_queries, 3);
            assert!(counters.observation_mask_reads + counters.semantic_bit_reads <= 14);
        }
    }
}

#[test]
fn metadata_fields_have_independent_observation_masks() {
    let projection = SemanticProjection::new(
        9,
        2,
        vec![
            SemanticFeature::Meaning(0),
            SemanticFeature::Ambiguity(0),
            SemanticFeature::Routing(0),
        ],
    )
    .unwrap();
    let mut state = state::<257>(false, false);
    state.ambiguity.set(0, true).unwrap();
    state.routing.set(0, true).unwrap();
    let yes = SemanticBitField::ones().unwrap();
    let no = SemanticBitField::zeroed().unwrap();
    for availability in 0..8 {
        let mut input = observation(0, &state, &no);
        input.observed.meaning_mask = if availability & 1 != 0 { &yes } else { &no };
        input.observed.ambiguity = if availability & 2 != 0 { &yes } else { &no };
        input.observed.routing = if availability & 4 != 0 { &yes } else { &no };
        let (record, counters) = projection.project(input).unwrap();
        assert_eq!(record.known, availability);
        assert_eq!(record.values, availability);
        assert_eq!(counters.observation_mask_reads, 3);
        assert_eq!(
            counters.semantic_bit_reads,
            availability.count_ones() as u64
        );
    }
}

#[test]
fn observed_false_is_distinct_from_unknown_and_ambiguity_causes_abstention() {
    let mask = SemanticBitField::ones().unwrap();
    let absent = SemanticBitField::zeroed().unwrap();
    let false_state = state::<257>(false, true);
    let mut ambiguous = state::<257>(true, true);
    ambiguous.ambiguity.set(0, true).unwrap();
    let mut irrelevant = state::<257>(true, true);
    irrelevant.meaning_mask.set(0, false).unwrap();
    let mut missing = observation(3, &false_state, &mask);
    missing.observed.value = &absent;
    let program = SemanticDecisionProgram::new(projection(), binary(TruthTable2::AND, 1)).unwrap();
    let output = program
        .evaluate(
            &[
                observation(0, &false_state, &mask),
                observation(1, &ambiguous, &mask),
                observation(2, &irrelevant, &mask),
                missing,
            ],
            BatchBudget::default(),
            ProjectionBudget::default(),
        )
        .unwrap();
    assert_eq!(
        output.decisions.decisions[0].outcome,
        DecisionOutcome::Proposed(DecisionValue::Binary(false))
    );
    for result in &output.decisions.decisions[1..] {
        assert_eq!(
            result.outcome,
            DecisionOutcome::Abstain(Abstention::MissingInformation)
        );
    }
    assert_eq!(output.decisions.abstentions, 3);
}

#[test]
fn coordinate_256_and_feature_slot_63_are_preserved() {
    let mut features: Vec<_> = (0..63).map(SemanticFeature::ContextValue).collect();
    features.push(SemanticFeature::ContextValue(256));
    let projection = SemanticProjection::new(9, 2, features).unwrap();
    let mut state = state::<257>(false, false);
    state.value.set(256, true).unwrap();
    let mask = SemanticBitField::ones().unwrap();
    let (record, counters) = projection.project(observation(1, &state, &mask)).unwrap();
    assert_eq!(record.known, u64::MAX);
    assert_eq!(record.values, 1_u64 << 63);
    assert_eq!(counters.known_features, 64);
    assert_eq!(counters.observation_mask_reads, 192);
    assert_eq!(counters.semantic_bit_reads, 192);
}

#[test]
fn projection_construction_rejects_bad_dimensions_versions_and_duplicate_selectors() {
    assert!(matches!(
        SemanticProjection::<0>::new(9, 2, vec![SemanticFeature::Meaning(0)]),
        Err(SemanticDecisionError::InvalidProjection)
    ));
    assert!(matches!(
        SemanticProjection::<257>::new(9, 0, vec![SemanticFeature::Meaning(0)]),
        Err(SemanticDecisionError::InvalidProjection)
    ));
    assert!(matches!(
        SemanticProjection::<257>::new(9, 2, vec![]),
        Err(SemanticDecisionError::InvalidProjection)
    ));
    assert!(matches!(
        SemanticProjection::<257>::new(9, 2, vec![SemanticFeature::Meaning(0); 65]),
        Err(SemanticDecisionError::InvalidProjection)
    ));
    assert!(matches!(
        SemanticProjection::<257>::new(9, 2, vec![SemanticFeature::Meaning(257)]),
        Err(SemanticDecisionError::InvalidCoordinate(257))
    ));
    assert!(matches!(
        SemanticProjection::<257>::new(9, 2, vec![SemanticFeature::Meaning(0); 2]),
        Err(SemanticDecisionError::DuplicateFeature)
    ));
}

#[test]
fn binding_mismatch_rejects_whole_batch_and_incompatible_program() {
    let state = state::<257>(true, true);
    let mask = SemanticBitField::ones().unwrap();
    let good = observation(0, &state, &mask);
    let program = SemanticDecisionProgram::new(projection(), binary(TruthTable2::AND, 1)).unwrap();
    for wrong in [
        SemanticInput {
            schema_id: 10,
            ..good
        },
        SemanticInput {
            projection_version: 3,
            ..good
        },
    ] {
        assert!(matches!(
            program.evaluate(
                &[good, wrong],
                BatchBudget::default(),
                ProjectionBudget::default()
            ),
            Err(SemanticDecisionError::BindingMismatch)
        ));
    }
    let wrong_shape =
        SemanticProjection::<257>::new(9, 2, vec![SemanticFeature::Meaning(0)]).unwrap();
    assert!(matches!(
        SemanticDecisionProgram::new(wrong_shape, binary(TruthTable2::AND, 1)),
        Err(SemanticDecisionError::BindingMismatch)
    ));
    let wrong_schema = SemanticProjection::<257>::new(
        10,
        2,
        vec![
            SemanticFeature::Meaning(0),
            SemanticFeature::Meaning(1),
            SemanticFeature::Meaning(2),
        ],
    )
    .unwrap();
    assert!(matches!(
        SemanticDecisionProgram::new(wrong_schema, binary(TruthTable2::AND, 1)),
        Err(SemanticDecisionError::BindingMismatch)
    ));
}

#[test]
fn all_projection_budgets_and_decision_preflight_are_enforced() {
    let state = state::<257>(true, true);
    let mask = SemanticBitField::ones().unwrap();
    let inputs = [observation(0, &state, &mask), observation(1, &state, &mask)];
    let projection = projection();
    assert!(matches!(
        projection.plan(usize::MAX, ProjectionBudget::default()),
        Err(SemanticDecisionError::Overflow)
    ));
    let program = SemanticDecisionProgram::new(projection, binary(TruthTable2::AND, 1)).unwrap();
    for budget in [
        ProjectionBudget {
            max_records: 1,
            ..ProjectionBudget::default()
        },
        ProjectionBudget {
            max_buffer_bytes: 1,
            ..ProjectionBudget::default()
        },
        ProjectionBudget {
            max_bit_reads: 27,
            ..ProjectionBudget::default()
        },
    ] {
        assert!(matches!(
            program.evaluate(&inputs, BatchBudget::default(), budget),
            Err(SemanticDecisionError::ProjectionBudget)
        ));
    }
    assert!(matches!(
        program.evaluate(
            &inputs,
            BatchBudget {
                max_results: 0,
                ..BatchBudget::default()
            },
            ProjectionBudget::default(),
        ),
        Err(SemanticDecisionError::Decision(DecisionError::BatchBudget))
    ));
}

#[test]
fn empty_batch_needs_no_projection_buffer_or_reads() {
    let program =
        SemanticDecisionProgram::new(projection::<257>(), binary(TruthTable2::AND, 1)).unwrap();
    let result = program
        .evaluate(
            &[],
            BatchBudget::default(),
            ProjectionBudget {
                max_records: 0,
                max_buffer_bytes: 0,
                max_bit_reads: 0,
            },
        )
        .unwrap();
    assert_eq!(result.working_payload_bytes_bound, 0);
    assert_eq!(result.projection.feature_queries, 0);
    assert_eq!(result.projection.semantic_bit_reads, 0);
    assert!(result.decisions.decisions.is_empty());
}

#[test]
fn reference_width_multi_head_batch_reads_only_projected_coordinates_once() {
    let state = state::<16_384>(true, true);
    let before = state.clone();
    let mask = SemanticBitField::ones().unwrap();
    let program = SemanticDecisionProgram::new(projection(), binary(TruthTable2::AND, 8)).unwrap();
    for count in [1, 63, 64, 65, 129, 4096] {
        let inputs: Vec<_> = (0..count).map(|i| observation(i, &state, &mask)).collect();
        let result = program
            .evaluate(&inputs, BatchBudget::default(), ProjectionBudget::default())
            .unwrap();
        assert_eq!(result.decisions.decisions.len(), count as usize * 8);
        assert_eq!(result.projection_version, 2);
        assert_eq!(result.projection.feature_queries, count * 3);
        assert_eq!(result.projection.known_features, count * 3);
        assert_eq!(result.projection.observation_mask_reads, count * 7);
        assert_eq!(result.projection.semantic_bit_reads, count * 7);
        assert_eq!(result.projection_plan.bit_reads_bound, count * 14);
        assert_eq!(
            result.decisions.plan.word_gate_evaluations,
            count.div_ceil(64)
        );
        assert_eq!(result.decisions.text_tokens_generated, 0);
        for (i, decision) in result.decisions.decisions.iter().enumerate() {
            assert_eq!(decision.record_id, (i / 8) as u64);
            assert_eq!(decision.question_id, (i % 8) as u64);
            assert_eq!(decision.uncertainty, Uncertainty::Uncalibrated);
            assert_eq!(
                decision.outcome,
                DecisionOutcome::Proposed(DecisionValue::Binary(true))
            );
        }
        if count == 4096 {
            println!(
                "SBG-D1b records={} heads=8 decisions={} mask_reads={} state_reads={} word_gates={}",
                count,
                result.decisions.decisions.len(),
                result.projection.observation_mask_reads,
                result.projection.semantic_bit_reads,
                result.decisions.plan.word_gate_evaluations,
            );
        }
    }
    assert_eq!(state, before);
}

fn labeled<'a>(
    start: u64,
    states: &'a [SemanticState<257>],
    mask: &'a SemanticBitField<257>,
    table: TruthTable2,
) -> Vec<LabeledSemanticInput<'a, 257>> {
    states
        .iter()
        .enumerate()
        .map(|(i, state)| LabeledSemanticInput {
            input: observation(start + i as u64, state, mask),
            target: table.evaluate(i & 1 != 0, i & 2 != 0),
        })
        .collect()
}

#[test]
fn all_finite_experts_fit_sbg_inputs_and_run_through_the_same_projection() {
    let mask = SemanticBitField::ones().unwrap();
    let states: Vec<_> = (0..16)
        .map(|i| state::<257>(i & 1 != 0, i & 2 != 0))
        .collect();
    let projection = projection();
    for code in 0..16 {
        let target = TruthTable2::new(code).unwrap();
        let train = labeled(0, &states, &mask, target);
        let validation = labeled(100, &states, &mask, target);
        let fit = projection
            .fit_expert(
                &train,
                &validation,
                [0, 1],
                TruthTable2::new(code ^ 15).unwrap(),
                FitPolicy::default(),
                ProjectionBudget::default(),
            )
            .unwrap();
        assert_eq!(fit.fit.outcome, FitOutcome::Accepted(target));
        assert_eq!(fit.fit.accepted_logical_table_bits, 4);
        assert_eq!(fit.projection.records, 32);
        let program =
            SemanticDecisionProgram::new(projection.clone(), binary(fit.fit.selected, 1)).unwrap();
        let test = labeled(10_000, &states, &mask, target);
        let inputs: Vec<_> = test.iter().map(|sample| sample.input).collect();
        let output = program
            .evaluate(&inputs, BatchBudget::default(), ProjectionBudget::default())
            .unwrap();
        for (decision, sample) in output.decisions.decisions.iter().zip(test) {
            assert_eq!(
                decision.outcome,
                DecisionOutcome::Proposed(DecisionValue::Binary(sample.target))
            );
        }
    }
}

#[test]
fn semantic_validation_only_rejects_and_does_not_select_a_different_table() {
    let mask = SemanticBitField::ones().unwrap();
    let states: Vec<_> = (0..16)
        .map(|i| state::<257>(i & 1 != 0, i & 2 != 0))
        .collect();
    let train = labeled(0, &states, &mask, TruthTable2::AND);
    let validation = labeled(100, &states, &mask, TruthTable2::XOR);
    let report = projection()
        .fit_expert(
            &train,
            &validation,
            [0, 1],
            TruthTable2::XOR,
            FitPolicy::default(),
            ProjectionBudget::default(),
        )
        .unwrap();
    assert_eq!(report.fit.selected, TruthTable2::AND);
    assert_eq!(report.fit.outcome, FitOutcome::Rejected);
    assert_eq!(report.fit.accepted_logical_table_bits, 0);
}

#[test]
fn unknown_training_inputs_are_not_silently_dropped_or_filled() {
    let mask = SemanticBitField::ones().unwrap();
    let absent = SemanticBitField::zeroed().unwrap();
    let states: Vec<_> = (0..4)
        .map(|i| state::<257>(i & 1 != 0, i & 2 != 0))
        .collect();
    let mut train = labeled(0, &states, &mask, TruthTable2::AND);
    let validation = labeled(100, &states, &mask, TruthTable2::AND);
    train[2].input.observed.ambiguity = &absent;
    assert_eq!(
        projection().fit_expert(
            &train,
            &validation,
            [0, 1],
            TruthTable2::FALSE,
            FitPolicy::default(),
            ProjectionBudget::default(),
        ),
        Err(SemanticDecisionError::UnknownTrainingFeature(2))
    );
}

#[test]
fn semantic_fitting_preserves_overlap_pair_and_sample_budget_checks() {
    let mask = SemanticBitField::ones().unwrap();
    let states: Vec<_> = (0..4)
        .map(|i| state::<257>(i & 1 != 0, i & 2 != 0))
        .collect();
    let train = labeled(0, &states, &mask, TruthTable2::AND);
    let validation = labeled(100, &states, &mask, TruthTable2::AND);
    let projection = projection();
    assert_eq!(
        projection.fit_expert(
            &train,
            &train,
            [0, 1],
            TruthTable2::FALSE,
            FitPolicy::default(),
            ProjectionBudget::default(),
        ),
        Err(SemanticDecisionError::Fit(FitError::DuplicateCaseId))
    );
    for pair in [[0, 0], [0, 3]] {
        assert_eq!(
            projection.fit_expert(
                &train,
                &validation,
                pair,
                TruthTable2::FALSE,
                FitPolicy::default(),
                ProjectionBudget::default(),
            ),
            Err(SemanticDecisionError::InvalidFeaturePair)
        );
    }
    assert_eq!(
        projection.fit_expert(
            &train,
            &validation,
            [0, 1],
            TruthTable2::FALSE,
            FitPolicy {
                max_samples: 7,
                ..FitPolicy::default()
            },
            ProjectionBudget::default(),
        ),
        Err(SemanticDecisionError::Fit(FitError::SampleBudget))
    );
}

#[test]
fn typed_candidate_and_ordinal_heads_accept_projected_sbg() {
    let state = state::<257>(true, false);
    let mask = SemanticBitField::ones().unwrap();
    let decision = DecisionProgram::compile(
        9,
        4,
        3,
        vec![],
        vec![
            DecisionHead {
                question_id: 1,
                applicable: Input(2),
                kind: HeadKind::Choice(vec![
                    Candidate {
                        id: 900,
                        eligible: Input(0),
                    },
                    Candidate {
                        id: 12,
                        eligible: Input(1),
                    },
                ]),
            },
            DecisionHead {
                question_id: 2,
                applicable: Input(2),
                kind: HeadKind::Ordinal {
                    bits: vec![Input(0), Input(1)],
                    maximum: 2,
                },
            },
        ],
        ProgramLimits::default(),
    )
    .unwrap();
    let program = SemanticDecisionProgram::new(projection(), decision).unwrap();
    let output = program
        .evaluate(
            &[observation(0, &state, &mask)],
            BatchBudget::default(),
            ProjectionBudget::default(),
        )
        .unwrap();
    assert_eq!(
        output.decisions.decisions[0].outcome,
        DecisionOutcome::Proposed(DecisionValue::Choice(900))
    );
    assert_eq!(
        output.decisions.decisions[1].outcome,
        DecisionOutcome::Proposed(DecisionValue::Ordinal(1))
    );
}

#[test]
fn decoded_semantic_store_record_requires_external_observation_evidence() {
    let state = state::<257>(true, true);
    let bytes = build_semantic_store(&[(73, state)]).unwrap();
    let stored = SemanticStoreView::<257>::parse(&bytes)
        .unwrap()
        .get(73)
        .unwrap()
        .unwrap();
    let mask = SemanticBitField::ones().unwrap();
    let absent = SemanticBitField::zeroed().unwrap();
    let program = SemanticDecisionProgram::new(projection(), binary(TruthTable2::AND, 1)).unwrap();
    let output = program
        .evaluate(
            &[
                observation(73, &stored, &mask),
                observation(74, &stored, &absent),
            ],
            BatchBudget::default(),
            ProjectionBudget::default(),
        )
        .unwrap();
    assert_eq!(
        output.decisions.decisions[0].outcome,
        DecisionOutcome::Proposed(DecisionValue::Binary(true))
    );
    assert_eq!(
        output.decisions.decisions[1].outcome,
        DecisionOutcome::Abstain(Abstention::MissingInformation)
    );
}
