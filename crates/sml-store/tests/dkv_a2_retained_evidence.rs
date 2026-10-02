//! Artifact replay/integrity checks added after observation. No quality selection.
use std::collections::BTreeMap;

#[allow(dead_code)]
#[path = "../examples/dkv_projection_contrast_a2.rs"]
mod experiment;

const RETAINED: &str = include_str!("../../../research/dkv-r1a2/development.csv");
const SUMMARY: &str = include_str!("../../../research/dkv-r1a2/SUMMARY.csv");

#[test]
fn frozen_evidence_matches_executable_and_compact_summary() {
    let actual = experiment::report().expect("A2 replay");
    assert!(actual == RETAINED, "A2 output differs from frozen evidence; do not silently overwrite it");
    let summary = RETAINED
        .lines()
        .filter(|line| line.starts_with("SUMMARY,") || line.starts_with("FAMILY,") || line.starts_with("PROMOTION,"))
        .collect::<Vec<_>>()
        .join("\n") + "\n";
    assert_eq!(summary, SUMMARY, "compact summary is not the retained output subset");
    let hash = sml_store::raw_context::sha256_raw_bytes(RETAINED.as_bytes()).unwrap();
    assert_eq!(hash.hex(), "084b3de117f5ad1859528e1dce237618fa1bdd1e8bea9fec875783a042e04448");
}

#[test]
fn paired_grid_has_each_arm_once_and_reports_partitioned_outcomes() {
    let mut groups: BTreeMap<String, BTreeMap<&str, Vec<&str>>> = BTreeMap::new();
    for row in RETAINED.lines().filter(|line| line.starts_with("ROW,")) {
        let fields: Vec<_> = row.split(',').collect();
        assert_eq!(fields.len(), 22);
        let identity = fields[1..6].join(",");
        let arm = fields[6];
        assert!(["standard", "contrast", "direct"].contains(&arm));
        assert!(groups.entry(identity).or_default().insert(arm, fields).is_none());
    }
    assert_eq!(groups.len(), 576);
    for arms in groups.values() {
        assert_eq!(arms.len(), 3);
        let gold = arms["standard"][7];
        for row in arms.values() {
            assert_eq!(row[7], gold, "paired arms must share the same evaluator target");
            let correct = row[9] == "1";
            let wrong_known = row[10] == "1";
            let unknown = row[11] == "1";
            assert_eq!(correct, row[7] == row[8]);
            assert_eq!(unknown, row[8] == "unknown");
            assert_eq!(wrong_known, !unknown && !correct);
            // Correct unknowns overlap the 'correct' and 'unknown' counts.
            assert_eq!(usize::from(correct && !unknown) + usize::from(wrong_known) + usize::from(unknown), 1);
        }
    }
}
