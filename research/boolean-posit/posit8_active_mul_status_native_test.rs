//! Compare the actually assembled learned status classifier against all 65,536
//! operand pairs. Returns a status flag: 1=NaR, 2=zero, 4=negative, 0=positive.
#[path = "posit8_active_mul_status.rs"]
#[allow(dead_code)]
mod learner;
unsafe extern "C" {
    fn mem_posit8_learned_mul_status(a: u8, b: u8) -> u8;
}
#[test]
fn x86_64_native_composed_status_matches_full_integer_posit_oracle() {
    let model = learner::LearnedMulStatus::train();
    assert_eq!(model.audit_all(), 0);
    let mut native_differences = 0usize;
    let mut oracle_differences = 0usize;
    let mut status_counts = [0usize; 4];
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            // SAFETY: the checked native GAS symbol takes two byte arguments
            // and produces one byte result under the x86-64 System V ABI.
            let raw = unsafe { mem_posit8_learned_mul_status(a, b) };
            let rust = model.classify(a, b);
            let teacher = learner::LearnedMulStatus::oracle_class(a, b);
            native_differences += usize::from(raw != rust);
            oracle_differences += usize::from(raw != teacher);
            let class_index = match raw {
                0 => 0,
                1 => 1,
                2 => 2,
                4 => 3,
                _ => panic!("invalid native status {raw} for {a:#x}, {b:#x}"),
            };
            status_counts[class_index] += 1;
        }
    }
    assert_eq!(native_differences, 0);
    assert_eq!(oracle_differences, 0);
    assert_eq!(status_counts.iter().sum::<usize>(), 65536);
    println!(
        "SYNTH_LEARN1D_NATIVE,operation=mul,pairs=65536,teacher_queries={},native_mismatches={},oracle_mismatches={},positive={},nar={},zero={},negative={}",
        model.total_queries(), native_differences, oracle_differences,
        status_counts[0], status_counts[1], status_counts[2], status_counts[3]
    );
}
