//! Execute the actual learned SysV x86-64 comparator and learned negation
//! byte arithmetic: all 65536 raw pairs and all finite posit magnitudes.
#[path = "posit8_active_magnitude_compare.rs"]
#[allow(dead_code)]
mod learned;

unsafe extern "C" {
    fn mem_posit8_learned_byte_cmp(a: u8, b: u8) -> u8;
    fn mem_posit8_learned_posit_mag_cmp(a: u8, b: u8) -> u8;
}

#[test]
fn native_byte_comparator_and_linked_magnitude_match_full_integer_oracle() {
    let model = learned::LearnedMagnitudeComparator::train();
    assert_eq!(model.audit_all_byte_pairs(), 0);
    assert_eq!(model.audit_all_posit_pairs(), 0);
    let mut mismatched_raw = 0usize;
    let mut mismatched_posit = 0usize;
    let mut invalid_count = 0usize;
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            // SAFETY: both functions are GNU GAS SysV u8/u8 -> u8 and are
            // compiled from the exact same learned gates under test.
            let native_raw = unsafe { mem_posit8_learned_byte_cmp(a, b) };
            let native_posit = unsafe { mem_posit8_learned_posit_mag_cmp(a, b) };
            mismatched_raw += usize::from(native_raw != model.compare_unsigned(a, b));
            mismatched_posit += usize::from(
                native_posit != learned::LearnedMagnitudeComparator::oracle_posit_magnitude(a, b),
            );
            invalid_count += usize::from(a == 0x80 || b == 0x80);
        }
    }
    assert_eq!(mismatched_raw, 0);
    assert_eq!(mismatched_posit, 0);
    assert_eq!(invalid_count, 511);
    println!(
        "SYNTH_LEARN1H_NATIVE,raw_pairs=65536,posit_pairs=65536,teacher_queries={},raw_mismatches={},posit_mismatches={},nar_pairs={}",
        model.teacher_queries(),
        mismatched_raw,
        mismatched_posit,
        invalid_count
    );
}
