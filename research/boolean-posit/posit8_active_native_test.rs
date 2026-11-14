//! Checks that learned sample-only ADD/MUL Boolean circuits and the generated
//! GNU x86-64 SysV machine code make the SAME predictions on all 65536 inputs.
//! Learned predictions need not equal the integer arithmetic teacher elsewhere.
#[path = "posit8_active_learn.rs"]
#[allow(dead_code)]
mod active;

unsafe extern "C" {
    fn mem_posit8_active_add(a: u8, b: u8) -> u8;
    fn mem_posit8_active_mul(a: u8, b: u8) -> u8;
}
fn check(operation: active::Operation, native: unsafe extern "C" fn(u8, u8) -> u8) {
    let trained = active::learn_active(operation);
    let mut mismatches = 0usize;
    for packed in 0..=u16::MAX {
        let (a, b) = active_unpack(packed);
        // SAFETY: GNU as assembles symbols emitted by our pinned, bounded
        // compiler. All inputs are byte values and native ABI is SysV x86-64.
        let predicted = unsafe { native(a, b) };
        if predicted != trained.circuit.evaluate(packed) {
            mismatches += 1;
        }
        if let Some(&label) = trained.samples.get(&packed) {
            assert_eq!(predicted, label, "native code violates a teacher query");
        }
    }
    assert_eq!(mismatches, 0, "native-vs-Rust circuit mismatch");
    assert_eq!(trained.report.train_errors, 0);
    println!(
        "SYNTH_LEARN1_NATIVE,op={:?},pairs=65536,native_mismatches={},train_queries={},holdout_errors={},full_errors={},gates={}",
        operation, mismatches, trained.report.teacher_queries,
        trained.report.holdout_errors, trained.report.full_errors, trained.report.gates
    );
}
fn active_unpack(packed: u16) -> (u8, u8) {
    let mut a = 0u8;
    let mut b = 0u8;
    for bit in 0..8 {
        a |= (((packed >> (bit * 2)) & 1) as u8) << bit;
        b |= (((packed >> (bit * 2 + 1)) & 1) as u8) << bit;
    }
    (a, b)
}
#[test]
fn exact_native_assembly_matches_the_learned_add_circuit() {
    check(active::Operation::Add, mem_posit8_active_add);
}
#[test]
fn exact_native_assembly_matches_the_learned_mul_circuit() {
    check(active::Operation::Mul, mem_posit8_active_mul);
}
