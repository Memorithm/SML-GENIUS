use sml_core::{
    elastic_weights::{
        ElasticWeightError, ElasticWeightPage, ElasticWeightPlanner, ElasticWeightPolicy,
        ResidencyTier, WeightPrecision,
    },
    mor::{MixtureOfRouters, MorPolicy, RouterId},
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    semantic::{
        SemanticBitField, SemanticExpertSignature, SemanticMorBridge, SemanticMorTopK,
        SemanticRouterSignature, SemanticState,
    },
    PageId,
};
use sml_decision::{
    elastic_bank::{ElasticAdmissionError, ElasticMorDecisionBank},
    mor_bank::{DecisionExpert, MorDecisionExpertBank, SparseDecisionBudget},
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

fn decision_bank<const BITS: usize>() -> MorDecisionExpertBank<BITS> {
    let bridge = SemanticMorBridge::new(
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
        SemanticMorTopK::One,
    )
    .unwrap();

    MorDecisionExpertBank::new(
        bridge,
        registry(),
        MixtureOfRouters::new(MorPolicy::default()).unwrap(),
        vec![
            DecisionExpert {
                page_id: PageId(0),
                program: program(10, 100, TruthTable2::AND),
            },
            DecisionExpert {
                page_id: PageId(1),
                program: program(11, 200, TruthTable2::XOR),
            },
        ],
    )
    .unwrap()
}

fn program<const BITS: usize>(
    model_version: u64,
    question_id: u64,
    table: TruthTable2,
) -> SemanticDecisionProgram<BITS> {
    let projection = SemanticProjection::new(
        9,
        5,
        vec![
            SemanticFeature::ContextValue(0),
            SemanticFeature::ContextValue(1),
            SemanticFeature::Meaning(2),
        ],
    )
    .unwrap();
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
    SemanticDecisionProgram::new(projection, decision).unwrap()
}

fn planner(
    max_active_parameters: u64,
    ram_limit_bytes: u64,
    vram_limit_bytes: u64,
) -> ElasticWeightPlanner {
    ElasticWeightPlanner::new(
        vec![
            ElasticWeightPage::new(
                PageId(0),
                8,
                WeightPrecision::Boolean,
                ResidencyTier::Disk,
                950_000,
            )
            .unwrap(),
            ElasticWeightPage::new(
                PageId(1),
                8,
                WeightPrecision::Boolean,
                ResidencyTier::Disk,
                500_000,
            )
            .unwrap(),
        ],
        ElasticWeightPolicy {
            ram_limit_bytes,
            vram_limit_bytes,
            max_active_parameters,
            ternary_threshold_ppm: 600_000,
            residual4_threshold_ppm: 900_000,
            vram_threshold_ppm: 900_000,
        },
    )
    .unwrap()
}

fn bank<const BITS: usize>() -> ElasticMorDecisionBank<BITS> {
    ElasticMorDecisionBank::new(decision_bank(), planner(16, 1, 4)).unwrap()
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
    record_id: u64,
    state: &'a SemanticState<BITS>,
    observed: &'a SemanticBitField<BITS>,
) -> SemanticInput<'a, BITS> {
    SemanticInput {
        record_id,
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
fn one_mor_page_is_the_only_elastic_active_page_before_decision_execution() {
    let state = state::<8>(0, true, true);
    let observed = SemanticBitField::<8>::ones().unwrap();
    let inputs: Vec<_> = (0..65)
        .map(|record_id| input(record_id, &state, &observed))
        .collect();

    let bank = bank();
    let result = bank
        .evaluate(
            &inputs,
            &[],
            7,
            BatchBudget::default(),
            ProjectionBudget::default(),
            SparseDecisionBudget::default(),
        )
        .unwrap();

    let plan = result.elastic_plan.as_ref().unwrap();
    let active: Vec<_> = plan
        .transitions
        .iter()
        .filter(|transition| transition.to_active)
        .map(|transition| transition.page_id)
        .collect();
    assert_eq!(active, vec![PageId(0)]);

    let admission = result.admission.unwrap();
    assert_eq!(admission.selected_pages, 1);
    assert_eq!(admission.active_parameters, 8);
    assert_eq!(admission.active_storage_bits, 32);
    assert_eq!(admission.ram_bytes, 0);
    assert_eq!(admission.vram_bytes, 4);
    assert_eq!(admission.precision_changes, 1);
    assert_eq!(result.decisions.expert_batches.len(), 1);
    assert_eq!(result.decisions.expert_batches[0].page_id, PageId(0));
    assert_eq!(result.decisions.text_tokens_generated, 0);

    // Planning is non-mutating: D1e does not pretend to materialize or commit.
    assert_eq!(bank.planner().generation(), 0);
    assert_eq!(bank.planner().accounting().unwrap().active_parameters, 0);
}

#[test]
fn mixed_mor_pages_form_exact_elastic_union_before_both_experts_run() {
    let left = state::<8>(0, true, true);
    let right = state::<8>(1, true, false);
    let observed = SemanticBitField::<8>::ones().unwrap();
    let inputs = [input(0, &left, &observed), input(1, &right, &observed)];

    let result = bank()
        .evaluate(
            &inputs,
            &[],
            9,
            BatchBudget::default(),
            ProjectionBudget::default(),
            SparseDecisionBudget::default(),
        )
        .unwrap();

    let plan = result.elastic_plan.as_ref().unwrap();
    let active: Vec<_> = plan
        .transitions
        .iter()
        .filter(|transition| transition.to_active)
        .map(|transition| transition.page_id)
        .collect();
    assert_eq!(active, vec![PageId(0), PageId(1)]);

    let admission = result.admission.unwrap();
    assert_eq!(admission.selected_pages, 2);
    assert_eq!(admission.active_parameters, 16);
    assert_eq!(admission.active_storage_bits, 40);
    assert_eq!(admission.ram_bytes, 1);
    assert_eq!(admission.vram_bytes, 4);
    assert_eq!(result.decisions.expert_batches.len(), 2);
    assert_eq!(
        result.decisions.decisions[0].decision.outcome,
        DecisionOutcome::Proposed(DecisionValue::Binary(true))
    );
    assert_eq!(
        result.decisions.decisions[1].decision.outcome,
        DecisionOutcome::Proposed(DecisionValue::Binary(true))
    );
}

#[test]
fn elastic_active_parameter_budget_rejects_mor_union_before_expert_execution() {
    let left = state::<8>(0, true, true);
    let right = state::<8>(1, true, false);
    let observed = SemanticBitField::<8>::ones().unwrap();
    let inputs = [input(0, &left, &observed), input(1, &right, &observed)];
    let bank = ElasticMorDecisionBank::new(decision_bank(), planner(8, 1, 4)).unwrap();

    assert_eq!(
        bank.evaluate(
            &inputs,
            &[],
            11,
            BatchBudget::default(),
            ProjectionBudget::default(),
            SparseDecisionBudget::default(),
        ),
        Err(ElasticAdmissionError::Elastic(
            ElasticWeightError::ActiveParameterBudget {
                actual: 16,
                maximum: 8,
            }
        ))
    );
    assert_eq!(bank.planner().generation(), 0);
    assert_eq!(bank.planner().accounting().unwrap().active_parameters, 0);
}

#[test]
fn elastic_residency_budget_rejects_selected_page_before_expert_execution() {
    let state = state::<8>(1, true, false);
    let observed = SemanticBitField::<8>::ones().unwrap();
    let bank = ElasticMorDecisionBank::new(decision_bank(), planner(16, 0, 0)).unwrap();

    assert!(matches!(
        bank.evaluate(
            &[input(1, &state, &observed)],
            &[],
            12,
            BatchBudget::default(),
            ProjectionBudget::default(),
            SparseDecisionBudget::default(),
        ),
        Err(ElasticAdmissionError::Elastic(
            ElasticWeightError::ActiveResidencyBudget {
                page_id: PageId(1),
                ..
            }
        ))
    ));
    assert_eq!(bank.planner().generation(), 0);
}

#[test]
fn elastic_inventory_must_exactly_match_decision_expert_pages() {
    let planner = ElasticWeightPlanner::new(
        vec![ElasticWeightPage::new(
            PageId(0),
            8,
            WeightPrecision::Boolean,
            ResidencyTier::Disk,
            500_000,
        )
        .unwrap()],
        ElasticWeightPolicy {
            ram_limit_bytes: 1,
            vram_limit_bytes: 0,
            max_active_parameters: 8,
            ternary_threshold_ppm: 600_000,
            residual4_threshold_ppm: 900_000,
            vram_threshold_ppm: 900_000,
        },
    )
    .unwrap();

    assert!(matches!(
        ElasticMorDecisionBank::new(decision_bank::<8>(), planner),
        Err(ElasticAdmissionError::PageInventoryMismatch)
    ));
}

#[test]
fn empty_batch_skips_elastic_plan_and_preserves_empty_decision_result() {
    let result = bank::<8>()
        .evaluate(
            &[],
            &[],
            13,
            BatchBudget::default(),
            ProjectionBudget::default(),
            SparseDecisionBudget {
                max_records: 0,
                ..SparseDecisionBudget::default()
            },
        )
        .unwrap();

    assert!(result.elastic_plan.is_none());
    assert!(result.admission.is_none());
    assert!(result.decisions.decisions.is_empty());
    assert_eq!(result.decisions.routing_calls, 0);
}

#[test]
fn reference_width_preserves_mor_to_elastic_to_decision_contract() {
    let state = state::<16_384>(1, true, false);
    let observed = SemanticBitField::<16_384>::ones().unwrap();
    let inputs: Vec<_> = (0..65)
        .map(|record_id| input(record_id, &state, &observed))
        .collect();

    let result = bank()
        .evaluate(
            &inputs,
            &[],
            14,
            BatchBudget::default(),
            ProjectionBudget::default(),
            SparseDecisionBudget::default(),
        )
        .unwrap();

    let admission = result.admission.unwrap();
    assert_eq!(admission.selected_pages, 1);
    assert_eq!(admission.active_parameters, 8);
    assert_eq!(admission.active_storage_bits, 8);
    assert_eq!(admission.ram_bytes, 1);
    assert_eq!(admission.vram_bytes, 0);
    assert_eq!(result.decisions.expert_batches[0].page_id, PageId(1));
    assert_eq!(result.decisions.expert_batches[0].word_gate_evaluations, 2);
    assert_eq!(result.decisions.text_tokens_generated, 0);
}
