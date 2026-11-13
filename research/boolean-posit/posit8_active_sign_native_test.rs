//! Confirms learned native sign masks on every declared arithmetic subdomain.
#[path = "posit8_active_sign_gates.rs"]
#[allow(dead_code)]
mod active;
unsafe extern "C" {
    fn mem_posit8_learned_add_sign(a: u8, b: u8) -> u8;
    fn mem_posit8_learned_mul_sign(a: u8, b: u8) -> u8;
}
#[test]
fn compiled_add_sign_is_exhaustively_exact_where_declared() {
    let result = active::discover(active::Operation::Add);
    let mut checked = 0usize;
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            if a == 0x80 || b == 0x80 {
                continue;
            }
            if a != 0 && b != 0 && (a >> 7) != (b >> 7) {
                continue;
            }
            // SAFETY: linked object is emitted from our 8-bit SysV GAS gate.
            let actual = unsafe { mem_posit8_learned_add_sign(a, b) };
            assert_eq!(actual, result.chosen_gate.evaluate(a, b) << 7);
            checked += 1;
        }
    }
    assert_eq!(checked, result.eligible_count);
    assert_eq!(result.unseen_errors, 0);
    println!("SYNTH_LEARN1C_NATIVE,op=add,eligible={checked},native_mismatches=0,unseen_errors=0");
}
#[test]
fn compiled_mul_sign_is_exhaustively_exact_where_declared() {
    let result = active::discover(active::Operation::Mul);
    let mut checked = 0usize;
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            if a == 0 || b == 0 || a == 0x80 || b == 0x80 {
                continue;
            }
            // SAFETY: linked object is emitted from our 8-bit SysV GAS gate.
            let actual = unsafe { mem_posit8_learned_mul_sign(a, b) };
            assert_eq!(actual, result.chosen_gate.evaluate(a, b) << 7);
            checked += 1;
        }
    }
    assert_eq!(checked, 254 * 254);
    assert_eq!(checked, result.eligible_count);
    assert_eq!(result.unseen_errors, 0);
    println!("SYNTH_LEARN1C_NATIVE,op=mul,eligible={checked},native_mismatches=0,unseen_errors=0");
}
