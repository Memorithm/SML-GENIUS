#[path = "../examples/dkv_4_heldout_family.rs"]
mod experiment;

const RETAINED: &str = include_str!("../../../research/dkv-4i/comparison.csv");
const SUMMARY: &str = include_str!("../../../research/dkv-4i/SUMMARY.csv");

#[test]
fn retained_heldout_family_output_replays_exactly() {
    assert_eq!(experiment::report().unwrap(), RETAINED);
}

#[test]
fn retained_heldout_family_summary_is_exact_projection() {
    let summary = RETAINED
        .lines()
        .filter(|line| {
            line.starts_with("BASE,")
                || line.starts_with("FIT,")
                || line.starts_with("GATE_DATA,")
                || line.starts_with("HOLDOUT,")
                || line.starts_with("SUMMARY,")
                || line.starts_with("MISSING_FEATURE_SUMMARY,")
                || line.starts_with("SAFETY,")
                || line.starts_with("PROMOTION,")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(summary, SUMMARY);
}
