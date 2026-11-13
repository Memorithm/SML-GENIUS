//! Full real x86-64 SysV assembly validation for learned ADD status classifier.
#[path = "posit8_active_add_status.rs"]
#[allow(dead_code)]
mod learner;
unsafe extern "C" {
    fn mem_posit8_learned_add_status(a: u8, b: u8) -> u8;
}
#[test]
fn native_add_status_matches_the_integer_oracle_on_every_pair() {
    let learned = learner::LearnedAddStatus::train();
    assert_eq!(learned.audit(), 0);
    let mut native_differences = 0usize;
    let mut teacher_differences = 0usize;
    let mut counts = [0usize; 4];
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            // SAFETY: exact SysV byte args and byte return are confirmed by
            // the assembled symbol signature and full-domain parity below.
            let result = unsafe { mem_posit8_learned_add_status(a, b) };
            native_differences += usize::from(result != learned.classify(a, b));
            teacher_differences +=
                usize::from(result != learner::LearnedAddStatus::oracle_status(a, b));
            let class = match result {
                0 => 0,
                1 => 1,
                2 => 2,
                4 => 3,
                _ => panic!("unexpected native status {result}"),
            };
            counts[class] += 1;
        }
    }
    assert_eq!(native_differences, 0);
    assert_eq!(teacher_differences, 0);
    assert_eq!(counts.iter().sum::<usize>(), 65536);
    println!(
        "SYNTH_LEARN1E_NATIVE,operation=add,pairs=65536,queries={},native_mismatches={},oracle_mismatches={},positive={},nar={},zero={},negative={}",
        learned.query_count(), native_differences, teacher_differences,
        counts[0], counts[1], counts[2], counts[3]
    );
}
