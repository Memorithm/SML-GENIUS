//! Exact native x86-64 SysV parity for a sample-learned 5-gate full adder.
//! Covers every pair of bytes and both possible incoming carries.
#[path = "posit8_active_ripple_add.rs"]
#[allow(dead_code)]
mod learner;

unsafe extern "C" {
    fn mem_posit8_learned_ripple_add_u8(a: u8, b: u8, carry: u8) -> u16;
}
#[test]
fn real_machine_code_adds_every_byte_pair_with_both_carries() {
    let learned = learner::LearnedRippleAdder::train();
    assert_eq!(learned.audit_all_pairs(), 0);
    let mut model_mismatches = 0usize;
    let mut teacher_mismatches = 0usize;
    let mut operations = 0usize;
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            for cin in 0..=1u8 {
                // SAFETY: the source-generated GAS function has the exact
                // declared x86-64 System V u8/u8/u8 -> u16 signature.
                let native = unsafe { mem_posit8_learned_ripple_add_u8(a, b, cin) };
                model_mismatches += usize::from(native != learned.add_byte(a, b, cin));
                teacher_mismatches +=
                    usize::from(native != u16::from(a) + u16::from(b) + u16::from(cin));
                operations += 1;
            }
        }
    }
    assert_eq!(operations, 131072);
    assert_eq!(model_mismatches, 0);
    assert_eq!(teacher_mismatches, 0);
    println!(
        "SYNTH_LEARN1I_NATIVE,operation=unsigned_u8_add_with_carry,inputs={},teacher_queries={},model_mismatches={},oracle_mismatches={}",
        operations,
        learned.report.membership_queries,
        model_mismatches,
        teacher_mismatches
    );
}
