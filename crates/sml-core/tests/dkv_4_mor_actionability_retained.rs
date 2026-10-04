#[path = "../examples/dkv_4_mor_actionability.rs"]
mod experiment;

const RETAINED: &str = include_str!("../../../research/dkv-4d/comparison.csv");

#[test]
fn retained_mor_actionability_output_replays_exactly() {
    assert_eq!(experiment::report().unwrap(), RETAINED);
}
