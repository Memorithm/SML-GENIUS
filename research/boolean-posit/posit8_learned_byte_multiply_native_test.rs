//! Verify a real linked x86-64 SysV program that multiplies two bytes using
//! only a previously learned Boolean ripple-carry adder, 65536 inputs.
#[path = "posit8_learned_byte_multiply.rs"]
#[allow(dead_code)]
mod learner;
unsafe extern "C" {
    fn mem_posit8_learned_u8_mul_u16(a: u8, b: u8) -> u16;
}
#[test]
fn native_composition_agrees_with_full_unsigned_multiplication_domain() {
    let model = learner::LearnedByteMultiplier::train();
    assert_eq!(model.audit_all_pairs(), 0);
    let mut native_errors = 0usize;
    let mut oracle_errors = 0usize;
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            // SAFETY: generated and linked GAS SysV function accepts
            // unsigned-byte arguments and returns a sixteen-bit product.
            let native = unsafe { mem_posit8_learned_u8_mul_u16(a, b) };
            native_errors += usize::from(native != model.multiply(a, b));
            oracle_errors += usize::from(native != u16::from(a) * u16::from(b));
        }
    }
    assert_eq!(native_errors, 0);
    assert_eq!(oracle_errors, 0);
    println!(
        "SYNTH_LEARN1J_NATIVE,operation=unsigned_u8_mul_u16,pairs=65536,teacher_queries={},native_mismatches={},oracle_mismatches={}",
        model.membership_queries(),
        native_errors,
        oracle_errors
    );
}
