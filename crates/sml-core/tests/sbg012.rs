use sml_core::semantic::{
    packed_word4_score, qualify_packed_word4, scalar_reference_score, SemanticBitField,
    SemanticKernelKind, SemanticSimdError,
};

fn patterned_field<const BITS: usize>(stride: usize, offset: usize) -> SemanticBitField<BITS> {
    let indices: Vec<usize> = (offset..BITS).step_by(stride).collect();
    SemanticBitField::from_set_indices(&indices).unwrap()
}

#[test]
fn packed_word_kernel_matches_scalar_oracle_at_reference_width() {
    const BITS: usize = 16_384;
    let left = patterned_field::<BITS>(3, 0);
    let right = patterned_field::<BITS>(5, 1);
    let mask = patterned_field::<BITS>(7, 2);

    let report = qualify_packed_word4(&left, &right, &mask).unwrap();

    assert_eq!(report.scalar.kernel, SemanticKernelKind::ScalarReference);
    assert_eq!(report.packed.kernel, SemanticKernelKind::PackedWord4);
    assert_eq!(report.scalar.logical_bits, BITS as u64);
    assert_eq!(report.scalar.logical_coordinate_steps, BITS as u64);
    assert_eq!(report.packed.packed_words, 256);
    assert_eq!(report.packed.packed_word_groups, 64);
    assert_eq!(report.packed.packed_tail_words, 0);
    assert_eq!(report.packed.popcount_calls, 768);
    assert_eq!(
        report.scalar.full_differing_bits,
        report.packed.full_differing_bits
    );
    assert_eq!(
        report.scalar.masked_differing_bits,
        report.packed.masked_differing_bits
    );
}

#[test]
fn packed_word_kernel_preserves_partial_tail_semantics() {
    const BITS: usize = 257;
    let left = patterned_field::<BITS>(2, 0);
    let right = patterned_field::<BITS>(3, 0);
    let mask = patterned_field::<BITS>(4, 1);

    let report = qualify_packed_word4(&left, &right, &mask).unwrap();

    assert_eq!(report.packed.packed_words, 5);
    assert_eq!(report.packed.packed_word_groups, 1);
    assert_eq!(report.packed.packed_tail_words, 1);
    assert_eq!(report.packed.popcount_calls, 15);
    assert_eq!(
        report.packed.full_differing_bits + report.packed.full_matching_bits,
        BITS as u64
    );
    assert_eq!(
        report.packed.masked_differing_bits + report.packed.masked_matching_bits,
        report.packed.masked_active_bits
    );
}

#[test]
fn empty_mask_fails_closed_on_both_kernels() {
    let left = patterned_field::<128>(2, 0);
    let right = patterned_field::<128>(3, 0);
    let mask = SemanticBitField::<128>::zeroed().unwrap();

    assert_eq!(
        scalar_reference_score(&left, &right, &mask),
        Err(SemanticSimdError::EmptyMask)
    );
    assert_eq!(
        packed_word4_score(&left, &right, &mask),
        Err(SemanticSimdError::EmptyMask)
    );
}

#[test]
fn qualification_is_deterministic() {
    let left = patterned_field::<1024>(11, 0);
    let right = patterned_field::<1024>(13, 1);
    let mask = patterned_field::<1024>(17, 2);

    let first = qualify_packed_word4(&left, &right, &mask).unwrap();
    let second = qualify_packed_word4(&left, &right, &mask).unwrap();
    assert_eq!(first, second);
}
