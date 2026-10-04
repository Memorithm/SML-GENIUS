#[path = "../examples/dkv_4_selective_decision.rs"]
mod experiment;

const RETAINED: &str = include_str!("../../../research/dkv-4f/comparison.csv");
const SUMMARY: &str = include_str!("../../../research/dkv-4f/SUMMARY.csv");

#[test]
fn retained_selective_decision_output_replays_exactly() {
    assert_eq!(experiment::report().unwrap(), RETAINED);
}

#[test]
fn retained_selective_decision_summary_is_exact_projection() {
    let summary = RETAINED
        .lines()
        .filter(|line| {
            line.starts_with("MOR,")
                || line.starts_with("SAFETY,")
                || line.starts_with("SUMMARY,")
                || line.starts_with("PROMOTION,")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(summary, SUMMARY);
}
