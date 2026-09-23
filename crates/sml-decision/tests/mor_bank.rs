use sml_core::{
    mor::{MixtureOfRouters, MorPolicy, RouterId},
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    semantic::{
        SemanticBitField, SemanticExpertSignature, SemanticMorBridge, SemanticMorTopK,
        SemanticRouterSignature, SemanticState,
    },
    PageId,
};
use sml_decision::{
    mor_bank::{
        DecisionExpert, MorDecisionExpertBank, SparseDecisionBudget, SparseDecisionError,
    },
    semantic::{
        ObservedFields, ProjectionBudget, SemanticDecisionProgram, SemanticFeature, SemanticInput,
        SemanticProjection,
    },
    BatchBudget, DecisionHead, DecisionOutcome, DecisionProgram, DecisionValue, GateSpec, HeadKind,
    ProgramLimits,
    SignalRef::{Gate, Input},
    TruthTable2,
};

fn registry() -> MorRouterRegistry {
    MorRouterRegistry::new(
        vec![RouterSpecialization {
            router_id: RouterId(7),
            experts: vec![PageId(0), PageId(1)],
        }],
        MorRegistryPolicy {
            expert_count: 2,
            max_routers: 1,
            max_edges: 2,
            max_fanout: 2,
            require_full_coverage: true,
        },
    )
    .unwrap()
}

fn signature<const BITS: usize>(page: u32) -> SemanticBitField<BITS> {
    match page {
        0 => SemanticBitField::from_set_indices(&[0]).unwrap(),
        1 => SemanticBitField::from_set_indices(&[1]).unwrap(),
        _ => unreachable!(),
    }
}

fn bridge<const BITS: usize>(top_k: SemanticMorTopK) -> SemanticMorBridge<BITS> {
    SemanticMorBridge::new(
        vec![SemanticRouterSignature {
            router_id: RouterId(7),
            signature: SemanticBitField::zeroed().unwrap(),
        }],
        vec![
            SemanticExpertSignature {
                page_id: PageId(0),
                signature: signature(0),
            },
            SemanticExpertSignature {
                page_id: PageId(1),
                signature: signature(1),
            },
        ],
        top_k,
    )
    .unwrap()
}

fn projection<const BITS: usize>(version: u64) -> SemanticProjection<BITS> {
    SemanticProjection::new(
        9,
        version,
        vec![
            SemanticFeature::ContextValue(0),
            SemanticFeature::ContextValue(1),
            SemanticFeature::Meaning(2),
        ],
    )
    .unwrap()
}

fn program<const BITS: usize>(
    version: u64,
    model_version: u64,
    question_id: u64,
    table: TruthTable2,
) -> SemanticDecisionProgram<BITS> {
    let decision = DecisionProgram::compile(
        9,
        model_version,
        3,
        vec![GateSpec {
            left: Input(0),
            right: Input(1),
            table,
        }],
        vec![DecisionHead {
            question_id,
            applicable: Input(2),
            kind: HeadKind::Binary(Gate(0)),
        }],
        ProgramLimits::default(),
    )
    .unwrap();
    SemanticDecisionProgram::new(projection(version), decision).unwrap()
}

fn bank<const BITS: usize>() -> MorDecisionExpertBank<BITS> {
    MorDecisionExpertBank::new(
        bridge(SemanticMorTopK::One),
        registry(),
        MixtureOfRouters::new(MorPolicy::default()).unwrap(),
        vec![
            DecisionExpert {
                page_id: PageId(0),
                program: program(5, 10, 100, TruthTable2::AND),
            },
            DecisionExpert {
                page_id: PageId(1),
                program: program(5, 11, 200, TruthTable2::XOR),
            },
        ],
    )
    .unwrap()
}

fn state<const BITS: usize>(page: u32, left: bool, right: bool) -> SemanticState<BITS> {
    let mut state = SemanticState::<BITS>::zeroed().unwrap();
    state.meaning_mask = SemanticBitField::ones().unwrap();
    state.value.set(0, left).unwrap();
    state.value.set(1, right).unwrap();
    state.routing = signature(page);
    state
}

fn input<'a, const BITS: usize>(
    id: u64,
    state: &'a SemanticState<BITS>,
    observed: &'a SemanticBitField<BITS>,
) -> SemanticInput<'a, BITS> {
    SemanticInput {
        record_id: id,
        schema_id: 9,
        projection_version: 5,
        state,
        observed: ObservedFields {
            value: observed,
            meaning_mask: observed,
            ambiguity: observed,
            routing: observed,
        },
    }
}

#[test]
fn mixed_records_route_to_only_the_selected_decision_experts() {
    let states = [
        state::<8>(0, true, false),
        state::<8>(1, true, false),
        state::<8>(0, true, true),
        state::<8>(1, false, true),
    ];
    let observed = SemanticBitField::<8>::ones().unwrap();
    let inputs: Vec<_> = states
        .iter()
        .enumerate()
        .map(|(index, state)| input(index as u64, state, &observed))
        .collect();

    let result = bank()
        .evaluate(
            &inputs,
            &[],
            BatchBudget::default(),
            ProjectionBudget::default(),
            SparseDecisionBudget::default(),
        )
        .unwrap();

    assert_eq!(
        result
            .routes
            .iter()
            .map(|route| route.selected_page)
            .collect::<Vec<_>>(),
        vec![PageId(0), PageId(1), PageId(0), PageId(1)]
    );
    assert_eq!(result.expert_batches.len(), 2);
    assert_eq!(result.routing_calls, 4);
    assert_eq!(result.text_tokens_generated, 0);
    assert_eq!(result.decisions.len(), 4);

    assert_eq!(result.decisions[0].expert_page, PageId(0));
    assert_eq!(
        result.decisions[0].decision.outcome,
        DecisionOutcome::Proposed(DecisionValue::Binary(false))
    );
    assert_eq!(result.decisions[1].expert_page, PageId(1));
    assert_eq!(
        result.decisions[1].decision.outcome,
        DecisionOutcome::Proposed(DecisionValue::Binary(true))
    );
    assert_eq!(result.decisions[2].expert_page, PageId(0));
    assert_eq!(
        result.decisions[2].decision.outcome,
        DecisionOutcome::Proposed(DecisionValue::Binary(true))
    );
    assert_eq!(result.decisions[3].expert_page, PageId(1));
    assert_eq!(
        result.decisions[3].decision.outcome,
        DecisionOutcome::Proposed(DecisionValue::Binary(true))
    );
}

#[test]
fn one_selected_page_keeps_one_expert_batch_and_bit_slices_128_records() {
    let state = state::<8>(0, true, true);
    let observed = SemanticBitField::<8>::ones().unwrap();
    let inputs: Vec<_> = (0..128)
        .map(|record_id| input(record_id, &state, &observed))
        .collect();

    let result = bank()
        .evaluate(
            &inputs,
            &[],
            BatchBudget::default(),
            ProjectionBudget::default(),
            SparseDecisionBudget::default(),
        )
        .unwrap();

    assert_eq!(result.expert_batches.len(), 1);
    assert_eq!(result.expert_batches[0].page_id, PageId(0));
    assert_eq!(result.expert_batches[0].records, 128);
    assert_eq!(result.expert_batches[0].word_gate_evaluations, 2);
    assert_eq!(result.decisions.len(), 128);
    assert!(result
        .decisions
        .iter()
        .all(|record| record.expert_page == PageId(0)));
}

#[test]
fn mixed_128_record_batch_groups_into_two_sparse_bit_sliced_batches() {
    let left = state::<8>(0, true, true);
    let right = state::<8>(1, true, false);
    let observed = SemanticBitField::<8>::ones().unwrap();
    let inputs: Vec<_> = (0..128)
        .map(|record_id| {
            if record_id % 2 == 0 {
                input(record_id, &left, &observed)
            } else {
                input(record_id, &right, &observed)
            }
        })
        .collect();

    let result = bank()
        .evaluate(
            &inputs,
            &[],
            BatchBudget::default(),
            ProjectionBudget::default(),
            SparseDecisionBudget::default(),
        )
        .unwrap();

    assert_eq!(result.expert_batches.len(), 2);
    assert_eq!(
        result
            .expert_batches
            .iter()
            .map(|batch| batch.records)
            .sum::<usize>(),
        128
    );
    assert_eq!(
        result
            .expert_batches
            .iter()
            .map(|batch| batch.word_gate_evaluations)
            .sum::<u64>(),
        2
    );
    assert_eq!(result.decisions.len(), 128);
}

#[test]
fn unobserved_routing_fails_before_sparse_expert_execution() {
    let state = state::<8>(0, true, true);
    let full = SemanticBitField::<8>::ones().unwrap();
    let missing = SemanticBitField::<8>::zeroed().unwrap();
    let mut second = input(1, &state, &full);
    second.observed.routing = &missing;
    let inputs = [input(0, &state, &full), second];

    assert_eq!(
        bank().evaluate(
            &inputs,
            &[],
            BatchBudget::default(),
            ProjectionBudget::default(),
            SparseDecisionBudget::default(),
        ),
        Err(SparseDecisionError::RoutingNotFullyObserved(1))
    );
}

#[test]
fn constructor_rejects_incomplete_duplicate_and_mismatched_expert_banks() {
    let mk = |experts| {
        MorDecisionExpertBank::new(
            bridge::<8>(SemanticMorTopK::One),
            registry(),
            MixtureOfRouters::new(MorPolicy::default()).unwrap(),
            experts,
        )
    };

    assert!(matches!(
        mk(vec![DecisionExpert {
            page_id: PageId(0),
            program: program(5, 10, 100, TruthTable2::AND),
        }]),
        Err(SparseDecisionError::ExpertCoverage)
    ));

    assert!(matches!(
        mk(vec![
            DecisionExpert {
                page_id: PageId(0),
                program: program(5, 10, 100, TruthTable2::AND),
            },
            DecisionExpert {
                page_id: PageId(0),
                program: program(5, 11, 200, TruthTable2::XOR),
            },
        ]),
        Err(SparseDecisionError::DuplicateExpert(PageId(0)))
            | Err(SparseDecisionError::MissingExpert(PageId(1)))
    ));

    assert!(matches!(
        mk(vec![
            DecisionExpert {
                page_id: PageId(0),
                program: program(5, 10, 100, TruthTable2::AND),
            },
            DecisionExpert {
                page_id: PageId(1),
                program: program(6, 11, 200, TruthTable2::XOR),
            },
        ]),
        Err(SparseDecisionError::ProgramBindingMismatch(PageId(1)))
    ));

    assert!(matches!(
        MorDecisionExpertBank::new(
            bridge::<8>(SemanticMorTopK::Two),
            registry(),
            MixtureOfRouters::new(MorPolicy::default()).unwrap(),
            vec![
                DecisionExpert {
                    page_id: PageId(0),
                    program: program(5, 10, 100, TruthTable2::AND),
                },
                DecisionExpert {
                    page_id: PageId(1),
                    program: program(5, 11, 200, TruthTable2::XOR),
                },
            ],
        ),
        Err(SparseDecisionError::NonSingletonProposalFanout)
    ));
}

#[test]
fn mor_multi_expert_route_is_rejected_before_any_decision_program_runs() {
    let registry = MorRouterRegistry::new(
        vec![
            RouterSpecialization {
                router_id: RouterId(0),
                experts: vec![PageId(0)],
            },
            RouterSpecialization {
                router_id: RouterId(1),
                experts: vec![PageId(1)],
            },
        ],
        MorRegistryPolicy {
            expert_count: 2,
            max_routers: 2,
            max_edges: 2,
            max_fanout: 1,
            require_full_coverage: true,
        },
    )
    .unwrap();
    let bridge = SemanticMorBridge::new(
        vec![
            SemanticRouterSignature {
                router_id: RouterId(0),
                signature: SemanticBitField::<8>::zeroed().unwrap(),
            },
            SemanticRouterSignature {
                router_id: RouterId(1),
                signature: SemanticBitField::<8>::zeroed().unwrap(),
            },
        ],
        vec![
            SemanticExpertSignature {
                page_id: PageId(0),
                signature: SemanticBitField::<8>::zeroed().unwrap(),
            },
            SemanticExpertSignature {
                page_id: PageId(1),
                signature: SemanticBitField::<8>::zeroed().unwrap(),
            },
        ],
        SemanticMorTopK::One,
    )
    .unwrap();
    let mut policy = MorPolicy::default();
    policy.min_routers = 2;
    policy.max_routers = 2;
    policy.min_experts = 2;
    policy.max_experts = 2;
    let bank = MorDecisionExpertBank::new(
        bridge,
        registry,
        MixtureOfRouters::new(policy).unwrap(),
        vec![
            DecisionExpert {
                page_id: PageId(0),
                program: program(5, 10, 100, TruthTable2::AND),
            },
            DecisionExpert {
                page_id: PageId(1),
                program: program(5, 11, 200, TruthTable2::XOR),
            },
        ],
    )
    .unwrap();

    let state = SemanticState::<8>::zeroed().unwrap();
    let observed = SemanticBitField::<8>::ones().unwrap();
    assert_eq!(
        bank.evaluate(
            &[input(9, &state, &observed)],
            &[],
            BatchBudget::default(),
            ProjectionBudget::default(),
            SparseDecisionBudget::default(),
        ),
        Err(SparseDecisionError::NonSingletonSelection {
            record_id: 9,
            selected: 2
        })
    );
}

#[test]
fn sparse_global_budgets_are_checked_before_expert_batches_execute() {
    let zero = state::<8>(0, true, true);
    let one = state::<8>(1, true, false);
    let observed = SemanticBitField::<8>::ones().unwrap();
    let inputs = [input(0, &zero, &observed), input(1, &one, &observed)];

    for budget in [
        SparseDecisionBudget {
            max_expert_batches: 1,
            ..SparseDecisionBudget::default()
        },
        SparseDecisionBudget {
            max_total_results: 1,
            ..SparseDecisionBudget::default()
        },
        SparseDecisionBudget {
            max_total_working_payload_bytes: 1,
            ..SparseDecisionBudget::default()
        },
    ] {
        assert_eq!(
            bank().evaluate(
                &inputs,
                &[],
                BatchBudget::default(),
                ProjectionBudget::default(),
                budget,
            ),
            Err(SparseDecisionError::BatchBudget)
        );
    }
}

#[test]
fn reference_width_routes_sparse_decisions_without_text_generation() {
    let state = state::<16_384>(1, true, false);
    let observed = SemanticBitField::<16_384>::ones().unwrap();
    let inputs: Vec<_> = (0..65)
        .map(|record_id| input(record_id, &state, &observed))
        .collect();

    let result = bank()
        .evaluate(
            &inputs,
            &[],
            BatchBudget::default(),
            ProjectionBudget::default(),
            SparseDecisionBudget::default(),
        )
        .unwrap();

    assert_eq!(result.routes.len(), 65);
    assert_eq!(result.expert_batches.len(), 1);
    assert_eq!(result.expert_batches[0].page_id, PageId(1));
    assert_eq!(result.expert_batches[0].word_gate_evaluations, 2);
    assert_eq!(result.decisions.len(), 65);
    assert_eq!(result.text_tokens_generated, 0);
}
