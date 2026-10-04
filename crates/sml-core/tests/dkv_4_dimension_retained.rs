#[path = "../examples/dkv_4_dimension_panel.rs"]
mod experiment;

const RETAINED: &str = include_str!("../../../research/dkv-4b/comparison.csv");
const SUMMARY: &str = include_str!("../../../research/dkv-4b/SUMMARY.csv");

#[test]
fn retained_dimension_output_replays_exactly() {
    assert_eq!(experiment::report().unwrap(), RETAINED);
}

#[test]
fn retained_summary_is_an_exact_projection() {
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
