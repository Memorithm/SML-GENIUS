#[path = "../examples/dkv_4_dynamic_mor.rs"]
mod experiment;

const RETAINED: &str = include_str!("../../../research/dkv-4e/comparison.csv");

#[test]
fn retained_dynamic_mor_output_replays_exactly() {
    assert_eq!(experiment::report().unwrap(), RETAINED);
}
