#[path = "../examples/dkv_aliasing_comparison_c.rs"]
mod experiment;
const RETAINED: &str = include_str!("../../../research/dkv-c/comparison.csv");
const SUMMARY: &str = include_str!("../../../research/dkv-c/SUMMARY.csv");
#[test]
fn c_output_replays_without_replacing_the_frozen_evidence() {
    assert!(
        experiment::report().unwrap() == RETAINED,
        "investigate drift; do not overwrite evidence"
    );
    let summary = RETAINED
        .lines()
        .filter(|l| l.starts_with("SUMMARY") || l.starts_with("PROMOTION,"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(summary, SUMMARY);
}
#[test]
fn c_rows_pair_both_projections_and_retain_original_attribution() {
    let mut groups = std::collections::BTreeMap::new();
    for l in RETAINED.lines().filter(|l| l.starts_with("ROW,")) {
        let r: Vec<_> = l.split(',').collect();
        assert_eq!(r.len(), 19);
        let key = r[1..5].join(",");
        let pair = groups.entry(key).or_insert([false; 2]);
        let i = usize::from(r[5] == "true");
        assert!(!pair[i]);
        pair[i] = true;
        assert_eq!(
            r[14],
            if r[8] == "1" && r[11] == "0" {
                "1"
            } else {
                "0"
            }
        );
    }
    assert_eq!(groups.len(), 1536);
    assert!(groups.values().all(|p| *p == [true, true]));
}
