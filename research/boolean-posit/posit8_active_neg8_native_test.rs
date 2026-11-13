//! True native x86-64 byte arithmetic, not class prediction: exhaustive
//! learned negation and finite magnitude parity against integer posit oracle.
#[path = "posit8_active_neg8.rs"]
#[allow(dead_code)]
mod learner;

unsafe extern "C" {
    fn mem_posit8_active_neg8(raw: u8) -> u8;
    fn mem_posit8_active_abs8(raw: u8) -> u8;
}

#[test]
fn learned_native_negation_and_magnitude_exact_on_all_256_raw_encodings() {
    let model = learner::LearnedNegation::train();
    assert_eq!(model.audit_all_negations(), 0);
    assert_eq!(model.audit_all_magnitudes(), 0);
    let mut incorrect_neg = 0usize;
    let mut incorrect_magnitude = 0usize;
    for raw in 0..=u8::MAX {
        // SAFETY: generated GNU x86-64 SysV exports unary u8 -> u8 symbols
        // whose body is an unrolled network of learnt Boolean 1-bit gates.
        let neg = unsafe { mem_posit8_active_neg8(raw) };
        let magnitude = unsafe { mem_posit8_active_abs8(raw) };
        incorrect_neg += usize::from(neg != model.negate(raw));
        let expected = model.abs_encoding(raw).unwrap_or(0x80);
        incorrect_magnitude += usize::from(magnitude != expected);
    }
    assert_eq!(incorrect_neg, 0);
    assert_eq!(incorrect_magnitude, 0);
    println!(
        "SYNTH_LEARN1G_NATIVE,operation=unary_neg8_and_abs8,inputs=256,teacher_queries={},neg_mismatches={},magnitude_mismatches={}",
        model.query_count(),
        incorrect_neg,
        incorrect_magnitude
    );
}
