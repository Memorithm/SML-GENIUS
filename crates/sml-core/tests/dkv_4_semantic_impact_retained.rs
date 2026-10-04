#[path = "../examples/dkv_4_semantic_impact_panel.rs"]
mod experiment;

const RETAINED: &str = include_str!("../../../research/dkv-4c/comparison.csv");
const SUMMARY: &str = include_str!("../../../research/dkv-4c/SUMMARY.csv");

#[test]
fn retained_semantic_impact_output_replays_exactly() {
    assert_eq!(experiment::report().unwrap(), RETAINED);
}

#[test]
fn retained_semantic_impact_summary_is_exact_projection() {
    let summary = RETAINED
        .lines()
        .filter(|line| {
            line.starts_with("SUMMARY_HEADER")
                || line.starts_with("SUMMARY,")
                || line.starts_with("INGEST,")
                || line.starts_with("PROMOTION,")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(summary, SUMMARY);
}
