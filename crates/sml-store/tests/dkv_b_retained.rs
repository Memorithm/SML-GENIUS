#[allow(dead_code)]
#[path = "../examples/dkv_counterpart_comparison.rs"]
mod comparison;
const OUTPUT: &str = include_str!("../../../research/dkv-b/comparison.csv");
const SUMMARY: &str = include_str!("../../../research/dkv-b/SUMMARY.csv");
const MANIFEST: &str = include_str!("../../../research/dkv-b/COMPARISON.json");
#[test]
fn frozen_bytes_hash_and_summary_match_the_executable() {
    assert_eq!(comparison::report().unwrap(), OUTPUT);
    let hash = sml_store::raw_context::sha256_raw_bytes(OUTPUT.as_bytes())
        .unwrap()
        .hex();
    assert!(MANIFEST.contains(&format!("\"output_sha256\": \"{hash}\"")));
    let summary = OUTPUT
        .lines()
        .filter(|l| l.starts_with("SUMMARY,") || l.starts_with("PROMOTION,"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(summary, SUMMARY);
}
#[test]
fn every_declared_cell_is_unique_and_partial_outputs_are_not_complete() {
    let mut seen = std::collections::BTreeSet::new();
    for line in OUTPUT.lines().filter(|l| l.starts_with("ROW,")) {
        let fields = line.split(',').collect::<Vec<_>>();
        assert_eq!(fields.len(), 21);
        assert!(seen.insert(fields[1..7].join(",")));
        assert!(["supplied", "native_fixed", "native_dual"].contains(&fields[1]));
        assert!(["standard", "contrast"].contains(&fields[2]));
        assert!(["16", "64"].contains(&fields[3]));
        assert!(["1", "2", "4", "8", "16", "32"].contains(&fields[4]));
        assert!(fields[5].parse::<usize>().unwrap() < 8);
        assert!(["old", "recent", "missing"].contains(&fields[6]));
        assert!(["complete", "partial", "unknown", "rejected"].contains(&fields[7]));
        if fields[7] != "complete" {
            assert_eq!(fields[8], "unknown");
        }
        let depth = fields[12].parse::<u64>().unwrap();
        let reads = fields[13].parse::<u64>().unwrap();
        let macs = fields[14].parse::<u64>().unwrap();
        assert!(depth <= 2);
        assert_eq!(macs, reads * 2 * fields[3].parse::<u64>().unwrap());
    }
    assert_eq!(seen.len(), 1728);
}
