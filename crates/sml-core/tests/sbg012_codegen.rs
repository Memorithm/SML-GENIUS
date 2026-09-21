use sml_core::semantic::{packed_word4_score, packed_word4_score_16384, SemanticBitField};

#[test]
fn fixed_width_codegen_entry_point_matches_generic_kernel() {
    const BITS: usize = 16_384;
    let left_indices: Vec<usize> = (0..BITS).step_by(3).collect();
    let right_indices: Vec<usize> = (1..BITS).step_by(5).collect();
    let mask_indices: Vec<usize> = (2..BITS).step_by(7).collect();

    let left = SemanticBitField::<BITS>::from_set_indices(&left_indices).unwrap();
    let right = SemanticBitField::<BITS>::from_set_indices(&right_indices).unwrap();
    let mask = SemanticBitField::<BITS>::from_set_indices(&mask_indices).unwrap();

    assert_eq!(
        packed_word4_score_16384(&left, &right, &mask).unwrap(),
        packed_word4_score(&left, &right, &mask).unwrap()
    );
}
