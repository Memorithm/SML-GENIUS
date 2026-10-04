#[path = "../examples/dkv_4_learned_applicability.rs"]
mod experiment;

const RETAINED: &str = include_str!("../../../research/dkv-4h/comparison.csv");
const SUMMARY: &str = include_str!("../../../research/dkv-4h/SUMMARY.csv");

#[test]
fn retained_learned_applicability_output_replays_exactly() {
    assert_eq!(experiment::report().unwrap(), RETAINED);
}

#[test]
fn retained_learned_applicability_summary_is_exact_projection() {
    let summary = RETAINED
        .lines()
        .filter(|line| {
            line.starts_with("SPLIT,")
                || line.starts_with("FIT,")
                || line.starts_with("SUMMARY,")
                || line.starts_with("MISSING_METADATA_SUMMARY,")
                || line.starts_with("SAFETY,")
                || line.starts_with("PROMOTION,")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(summary, SUMMARY);
}
