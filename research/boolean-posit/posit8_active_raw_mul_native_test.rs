//! Exhaustive parity of a genuinely learned raw-bit status classifier and
//! the generated native SysV x86-64 machine code, including all NaR encodings.
#[path = "posit8_active_raw_mul.rs"]
#[allow(dead_code)]
mod learner;

unsafe extern "C" {
    fn mem_posit8_learned_raw_mul_status(a: u8, b: u8) -> u8;
}

#[test]
fn compiled_raw_bit_predicates_match_teacher_on_all_65536_mul_pairs() {
    let learned = learner::LearnedRawMulStatus::train();
    assert_eq!(learned.audit_full(), 0);
    let mut raw_vs_rust = 0usize;
    let mut raw_vs_teacher = 0usize;
    let mut counts = [0usize; 4];
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            // SAFETY: the emitted GNU GAS routine uses the x86-64 System V
            // convention with two byte inputs and one byte status output.
            let native = unsafe { mem_posit8_learned_raw_mul_status(a, b) };
            raw_vs_rust += usize::from(native != learned.classify(a, b));
            raw_vs_teacher +=
                usize::from(native != learner::LearnedRawMulStatus::oracle_status(a, b));
            let category = match native {
                0 => 0,
                1 => 1,
                2 => 2,
                4 => 3,
                _ => panic!("invalid native status {native} for {a:#x}/{b:#x}"),
            };
            counts[category] += 1;
        }
    }
    assert_eq!(raw_vs_rust, 0);
    assert_eq!(raw_vs_teacher, 0);
    assert_eq!(counts.iter().sum::<usize>(), 65536);
    println!(
        "SYNTH_LEARN1F_NATIVE,operation=mul,pairs=65536,teacher_queries={},rust_mismatches={},oracle_mismatches={},positive={},nar={},zero={},negative={}",
        learned.teacher_queries(),
        raw_vs_rust,
        raw_vs_teacher,
        counts[0],
        counts[1],
        counts[2],
        counts[3]
    );
}
