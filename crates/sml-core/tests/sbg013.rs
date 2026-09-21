use sml_core::semantic::{
    build_semantic_store, SemanticBitField, SemanticState, SemanticStoreError, SemanticStoreView,
    SEMANTIC_STORE_PAGE_ALIGNMENT,
};

fn patterned_state<const BITS: usize>(seed: usize) -> SemanticState<BITS> {
    let field = |stride: usize, offset: usize| {
        let indices: Vec<usize> = (offset..BITS).step_by(stride).collect();
        SemanticBitField::<BITS>::from_set_indices(&indices).unwrap()
    };

    SemanticState {
        value: field(3 + seed % 3, seed % 3),
        meaning_mask: field(5 + seed % 5, (seed + 1) % 5),
        ambiguity: field(7 + seed % 7, (seed + 2) % 7),
        routing: field(11 + seed % 11, (seed + 3) % 11),
    }
}

#[test]
fn store_round_trip_sorts_index_and_decodes_on_demand() {
    const BITS: usize = 257;
    let seven = patterned_state::<BITS>(7);
    let forty_two = patterned_state::<BITS>(42);
    let ninety_nine = patterned_state::<BITS>(99);
    let records = vec![
        (42, forty_two.clone()),
        (99, ninety_nine.clone()),
        (7, seven.clone()),
    ];

    let bytes = build_semantic_store(&records).unwrap();
    let view = SemanticStoreView::<BITS>::parse(&bytes).unwrap();
    let header = view.header();

    assert_eq!(view.record_count(), 3);
    assert_eq!(header.payload_bytes, 160);
    assert_eq!(header.index_offset % SEMANTIC_STORE_PAGE_ALIGNMENT, 0);
    assert_eq!(header.records_offset % SEMANTIC_STORE_PAGE_ALIGNMENT, 0);
    assert_eq!(header.record_stride, SEMANTIC_STORE_PAGE_ALIGNMENT);
    assert_eq!(view.index_entry(0).unwrap().semantic_id, 7);
    assert_eq!(view.index_entry(1).unwrap().semantic_id, 42);
    assert_eq!(view.index_entry(2).unwrap().semantic_id, 99);

    assert_eq!(view.get(7).unwrap(), Some(seven));
    assert_eq!(view.get(42).unwrap(), Some(forty_two));
    assert_eq!(view.get(99).unwrap(), Some(ninety_nine));
    assert_eq!(view.get(123_456).unwrap(), None);
}

#[test]
fn default_width_records_are_page_aligned_without_extra_payload_padding() {
    const BITS: usize = 16_384;
    let state = patterned_state::<BITS>(3);
    let bytes = build_semantic_store(&[(1, state.clone())]).unwrap();
    let view = SemanticStoreView::<BITS>::parse(&bytes).unwrap();
    let header = view.header();

    assert_eq!(header.payload_bytes, 8_192);
    assert_eq!(header.record_stride, 8_192);
    assert_eq!(header.records_offset, 8_192);
    assert_eq!(view.index_entry(0).unwrap().record_offset, 8_192);
    assert_eq!(view.get(1).unwrap(), Some(state));
}

#[test]
fn payload_corruption_is_lazy_and_fails_when_record_is_accessed() {
    const BITS: usize = 257;
    let mut bytes = build_semantic_store(&[(5, patterned_state::<BITS>(5))]).unwrap();

    let entry = {
        let view = SemanticStoreView::<BITS>::parse(&bytes).unwrap();
        view.index_entry(0).unwrap()
    };
    bytes[entry.record_offset as usize] ^= 0x01;

    let view = SemanticStoreView::<BITS>::parse(&bytes).unwrap();
    assert!(matches!(
        view.get(5),
        Err(SemanticStoreError::RecordChecksumMismatch { semantic_id: 5, .. })
    ));
}

#[test]
fn header_corruption_fails_before_index_or_payload_access() {
    const BITS: usize = 257;
    let mut bytes = build_semantic_store(&[(5, patterned_state::<BITS>(5))]).unwrap();
    bytes[24] ^= 0x01;

    assert!(matches!(
        SemanticStoreView::<BITS>::parse(&bytes),
        Err(SemanticStoreError::HeaderChecksumMismatch { .. })
    ));
}

#[test]
fn duplicate_semantic_ids_fail_closed() {
    const BITS: usize = 65;
    let first = patterned_state::<BITS>(1);
    let second = patterned_state::<BITS>(2);

    assert_eq!(
        build_semantic_store(&[(9, first), (9, second)]),
        Err(SemanticStoreError::DuplicateSemanticId(9))
    );
}

#[test]
fn width_mismatch_fails_closed() {
    let bytes = build_semantic_store(&[(1, patterned_state::<257>(1))]).unwrap();

    assert!(matches!(
        SemanticStoreView::<258>::parse(&bytes),
        Err(SemanticStoreError::DimensionMismatch {
            expected: 258,
            actual: 257
        })
    ));
}

#[test]
fn zero_width_store_is_rejected() {
    assert_eq!(
        build_semantic_store::<0>(&[]),
        Err(SemanticStoreError::ZeroWidth)
    );
}
