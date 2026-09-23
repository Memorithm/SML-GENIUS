use sml_core::{
    delta_kv::DELTA_KV_ONE,
    delta_kv_semantic::{
        DeltaKvSemanticError, DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector,
        DeltaKvSourceProvenance,
    },
    mor::MorDecision,
    semantic::SemanticState,
    PageId,
};

fn decision(pages: &[u32]) -> MorDecision {
    MorDecision {
        active_routers: Vec::new(),
        ranked_experts: Vec::new(),
        selected_experts: pages.iter().copied().map(PageId).collect(),
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    }
}

fn state8() -> SemanticState<8> {
    let mut state = SemanticState::<8>::zeroed().unwrap();
    state.value.set(0, true).unwrap();
    state.value.set(3, true).unwrap();
    state.meaning_mask.set(0, true).unwrap();
    state.meaning_mask.set(1, true).unwrap();
    state.ambiguity.set(7, true).unwrap();
    state.routing.set(2, true).unwrap();
    state.routing.set(5, true).unwrap();
    state
}

#[test]
fn same_sbg_and_mor_state_projects_deterministically() {
    let projector =
        DeltaKvSemanticProjector::<16>::new(DeltaKvSemanticProjectionPolicy::default(), 7).unwrap();
    let state = state8();
    let mor = decision(&[3]);

    let left = projector.project(&state, &mor, None).unwrap();
    let right = projector.project(&state, &mor, None).unwrap();

    assert_eq!(left, right);
    assert_eq!(left.key_q12.len(), 16);
    assert!(left
        .key_q12
        .iter()
        .all(|value| *value == -DELTA_KV_ONE || *value == 0 || *value == DELTA_KV_ONE));
    assert!(left.key_q12.iter().any(|value| *value != 0));
    assert_eq!(left.transient_key_bits, 256);
}

#[test]
fn routing_field_change_changes_projected_semantic_key() {
    let projector =
        DeltaKvSemanticProjector::<16>::new(DeltaKvSemanticProjectionPolicy::default(), 11)
            .unwrap();
    let left = state8();
    let mut right = left.clone();
    right.routing.set(6, true).unwrap();
    let mor = decision(&[2]);

    assert_ne!(
        projector.project(&left, &mor, None).unwrap().key_q12,
        projector.project(&right, &mor, None).unwrap().key_q12
    );
}

#[test]
fn mor_selection_contributes_to_key_without_invoking_another_router() {
    let projector = DeltaKvSemanticProjector::<16>::new(
        DeltaKvSemanticProjectionPolicy {
            semantic_weight: 1,
            mor_weight: 64,
        },
        19,
    )
    .unwrap();
    let state = state8();

    let page_one = projector.project(&state, &decision(&[1]), None).unwrap();
    let page_large = projector.project(&state, &decision(&[999]), None).unwrap();

    assert_ne!(page_one.key_q12, page_large.key_q12);
    assert_eq!(page_one.selected_experts, vec![PageId(1)]);
    assert_eq!(page_large.selected_experts, vec![PageId(999)]);
}

#[test]
fn provenance_is_carried_but_does_not_change_compressed_key() {
    let projector =
        DeltaKvSemanticProjector::<16>::new(DeltaKvSemanticProjectionPolicy::default(), 23)
            .unwrap();
    let state = state8();
    let mor = decision(&[4]);
    let first = DeltaKvSourceProvenance::new(10, vec![100, 101]).unwrap();
    let second = DeltaKvSourceProvenance::new(11, vec![900]).unwrap();

    let a = projector.project(&state, &mor, Some(&first)).unwrap();
    let b = projector.project(&state, &mor, Some(&second)).unwrap();

    assert_eq!(a.key_q12, b.key_q12);
    assert_eq!(a.provenance, Some(first));
    assert_eq!(b.provenance, Some(second));
}

#[test]
fn reference_semantic_width_has_exact_projection_accounting() {
    let projector =
        DeltaKvSemanticProjector::<32>::new(DeltaKvSemanticProjectionPolicy::default(), 31)
            .unwrap();
    let mut state = SemanticState::<16_384>::zeroed().unwrap();
    state.value.set(123, true).unwrap();
    state.meaning_mask.set(456, true).unwrap();
    state.ambiguity.set(789, true).unwrap();
    state.routing.set(1024, true).unwrap();

    let result = projector.project(&state, &decision(&[7, 8]), None).unwrap();

    assert_eq!(result.counters.semantic_fields_read, 4);
    assert_eq!(result.counters.semantic_coordinates_read, 65_536);
    assert_eq!(result.counters.mor_selected_experts_read, 2);
    assert_eq!(result.counters.bucket_accumulations, 65_538);
    assert_eq!(result.counters.quantized_key_coordinates, 32);
    assert_eq!(result.transient_key_bits, 512);
}

#[test]
fn empty_or_duplicate_mor_selection_fails_closed() {
    let projector =
        DeltaKvSemanticProjector::<8>::new(DeltaKvSemanticProjectionPolicy::default(), 1).unwrap();
    let state = state8();

    assert_eq!(
        projector.project(&state, &decision(&[]), None),
        Err(DeltaKvSemanticError::EmptyMorSelection)
    );
    assert_eq!(
        projector.project(&state, &decision(&[2, 2]), None),
        Err(DeltaKvSemanticError::DuplicateMorSelection)
    );
}

#[test]
fn malformed_raw_provenance_fails_closed() {
    assert_eq!(
        DeltaKvSourceProvenance::new(1, vec![]),
        Err(DeltaKvSemanticError::EmptyRawProvenance)
    );
    assert_eq!(
        DeltaKvSourceProvenance::new(1, vec![7, 7]),
        Err(DeltaKvSemanticError::DuplicateRawSegment)
    );

    let projector =
        DeltaKvSemanticProjector::<8>::new(DeltaKvSemanticProjectionPolicy::default(), 2).unwrap();
    let state = state8();
    let malformed = DeltaKvSourceProvenance {
        semantic_record_id: 5,
        raw_segment_ids: vec![9, 9],
    };
    assert_eq!(
        projector.project(&state, &decision(&[1]), Some(&malformed)),
        Err(DeltaKvSemanticError::DuplicateRawSegment)
    );
}

#[test]
fn invalid_projector_geometry_and_policy_fail_before_projection() {
    assert_eq!(
        DeltaKvSemanticProjector::<0>::new(DeltaKvSemanticProjectionPolicy::default(), 0),
        Err(DeltaKvSemanticError::ZeroKeyDimension)
    );
    assert_eq!(
        DeltaKvSemanticProjector::<8>::new(
            DeltaKvSemanticProjectionPolicy {
                semantic_weight: 0,
                mor_weight: 8,
            },
            0,
        ),
        Err(DeltaKvSemanticError::InvalidPolicy)
    );
}
