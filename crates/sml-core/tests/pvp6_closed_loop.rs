#[path = "support/a4_closed_loop.rs"]
mod fixture;

#[test]
fn full_expert_dynamic_memory_loop_matches_literal_control_and_owned_continuation() {
    let report = fixture::qualification_report();
    assert!(report.starts_with("A4_CLOSED_LOOP,schema=1,"));
    assert!(report.contains("A4_CLOSED_SUMMARY,cases=108,"));
    for line in report
        .lines()
        .filter(|line| line.starts_with("A4_CLOSED_SUMMARY,"))
    {
        println!("{line}");
    }
}
