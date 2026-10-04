use sml_core::{
    delta_kv_semantic::{
        DeltaKvAmbiguityProjection, DeltaKvSemanticError, DeltaKvSemanticProjectionPolicy,
        DeltaKvSemanticProjector, DeltaKvSourceProvenance,
    },
    mor::MorDecision,
    semantic::SemanticState,
    PageId,
};

const BITS: usize = 64;
const K: usize = 256;

fn decision(page: PageId) -> MorDecision {
    MorDecision {
        active_routers: Vec::new(),
        ranked_experts: Vec::new(),
        selected_experts: vec![page],
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    }
}

fn state() -> SemanticState<BITS> {
    let mut state = SemanticState::zeroed().unwrap();
    for bit in [1, 5, 9, 17, 33, 48] {
        state.value.set(bit, true).unwrap();
    }
    for bit in [0, 2, 6, 12, 25, 51] {
        state.meaning_mask.set(bit, true).unwrap();
    }
    for bit in [3, 7, 14, 29, 45, 63] {
        state.routing.set(bit, true).unwrap();
    }
    state
}

#[test]
fn legacy_constructor_is_explicit_bipolar_compatible() {
    let policy = DeltaKvSemanticProjectionPolicy::default();
    let legacy = DeltaKvSemanticProjector::<K>::new(policy, 77).unwrap();
    let explicit = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        policy,
        77,
        DeltaKvAmbiguityProjection::Bipolar,
    )
    .unwrap();
    assert_eq!(
        legacy.ambiguity_projection(),
        DeltaKvAmbiguityProjection::Bipolar
    );

    let provenance = DeltaKvSourceProvenance::new(9, vec![11, 13]).unwrap();
    let state = state();
    let mor = decision(PageId(5));
    assert_eq!(
        legacy.project(&state, &mor, Some(&provenance)).unwrap(),
        explicit.project(&state, &mor, Some(&provenance)).unwrap()
    );
}

#[test]
fn active_only_zero_ambiguity_reads_but_does_not_accumulate_false_a_bits() {
    let projector = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        91,
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .unwrap();
    let evidence = projector
        .project(&state(), &decision(PageId(5)), None)
        .unwrap();

    assert_eq!(evidence.counters.semantic_fields_read, 4);
    assert_eq!(
        evidence.counters.semantic_coordinates_read,
        (4 * BITS) as u64
    );
    assert_eq!(evidence.counters.mor_selected_experts_read, 1);
    assert_eq!(
        evidence.counters.bucket_accumulations,
        (3 * BITS + 1) as u64
    );
    assert_eq!(evidence.counters.quantized_key_coordinates, K as u64);
}

#[test]
fn active_ambiguity_bits_add_exactly_one_accumulation_each() {
    let projector = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        91,
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .unwrap();
    let mut state = state();
    state.ambiguity.set(4, true).unwrap();
    state.ambiguity.set(37, true).unwrap();

    let evidence = projector
        .project(&state, &decision(PageId(5)), None)
        .unwrap();
    assert_eq!(
        evidence.counters.semantic_coordinates_read,
        (4 * BITS) as u64
    );
    assert_eq!(
        evidence.counters.bucket_accumulations,
        (3 * BITS + 2 + 1) as u64
    );
}

#[test]
fn candidate_preserves_fail_closed_mor_and_provenance_validation() {
    let projector = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        13,
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .unwrap();
    let state = state();

    let mut duplicate = decision(PageId(5));
    duplicate.selected_experts.push(PageId(5));
    assert_eq!(
        projector.project(&state, &duplicate, None),
        Err(DeltaKvSemanticError::DuplicateMorSelection)
    );

    let invalid_provenance = DeltaKvSourceProvenance {
        semantic_record_id: 7,
        raw_segment_ids: vec![3, 3],
    };
    assert_eq!(
        projector.project(&state, &decision(PageId(5)), Some(&invalid_provenance)),
        Err(DeltaKvSemanticError::DuplicateRawSegment)
    );
}
