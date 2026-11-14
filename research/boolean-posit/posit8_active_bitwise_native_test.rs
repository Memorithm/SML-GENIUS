//! Full native bit-for-bit ABI parity for both learned per-output-bit DAGs.
#[path = "posit8_active_learn.rs"]
#[allow(dead_code)]
mod source;

unsafe extern "C" {
    fn mem_posit8_bitwise_add(a: u8, b: u8) -> u8;
    fn mem_posit8_bitwise_mul(a: u8, b: u8) -> u8;
}
fn inputs(raw: u16) -> (u8, u8) {
    let mut a = 0u8;
    let mut b = 0u8;
    for bit in 0..8 {
        a |= (((raw >> (bit * 2)) & 1) as u8) << bit;
        b |= (((raw >> (bit * 2 + 1)) & 1) as u8) << bit;
    }
    (a, b)
}
fn check(op: source::Operation, native: unsafe extern "C" fn(u8, u8) -> u8) {
    let trained = source::bitwise::learn_active_bitwise(op);
    let mut errors = 0usize;
    for raw in 0..=u16::MAX {
        let (a, b) = inputs(raw);
        // SAFETY: the code under test uses the pinned GNU x86_64 SysV
        // ABI and exports a direct byte-input/byte-output function.
        let actual = unsafe { native(a, b) };
        errors += usize::from(actual != trained.circuit.evaluate(raw));
        if let Some(&expected) = trained.samples.get(&raw) {
            assert_eq!(actual, expected, "native teacher query mismatch");
        }
    }
    assert_eq!(errors, 0);
    assert_eq!(trained.report.train_errors, 0);
    println!(
        "SYNTH_LEARN1_BITWISE_NATIVE,op={:?},pairs=65536,native_mismatches={},train_queries={},holdout_errors={},full_errors={},gates={}",
        op,
        errors,
        trained.report.teacher_queries,
        trained.report.holdout_errors,
        trained.report.full_errors,
        trained.report.gates
    );
}
#[test]
fn exact_machine_predictions_add() {
    check(source::Operation::Add, mem_posit8_bitwise_add);
}
#[test]
fn exact_machine_predictions_mul() {
    check(source::Operation::Mul, mem_posit8_bitwise_mul);
}
