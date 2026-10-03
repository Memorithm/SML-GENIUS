use std::collections::{BTreeMap, BTreeSet};

use sml_store::observed_history::{HistoryLimits, ObservationLog, ObservedState, SubjectKey};
use sml_store::raw_context::sha256_raw_bytes;

#[allow(dead_code)]
#[path = "../examples/dkv_observed_history_a3.rs"]
mod experiment;

const FAMILIES: [&str; 8] = [
    "isolated", "old", "recent", "overwrite", "conflict", "missing", "near_old", "near_recent",
];

#[test]
fn source_generator_is_independent_and_retains_declared_history() {
    for group in 0..2 {
        for payload in 0..4 {
            for family in FAMILIES {
                let events = experiment::source_events(family, payload, group).unwrap();
                let mut log = ObservationLog::new(HistoryLimits {
                    max_events: 16,
                    max_keys: 8,
                    max_element_bytes: 8192,
                })
                .unwrap();
                for event in events {
                    log.append(event).unwrap();
                }
                let key = SubjectKey::new(1, group).unwrap();
                let summary = log.query(key).summary();
                let expected = match family {
                    "missing" => ObservedState::Missing,
                    "conflict" => ObservedState::Conflict,
                    _ => ObservedState::Agreement,
                };
                assert_eq!(summary.state, expected);
                if expected == ObservedState::Agreement {
                    assert_eq!(summary.value, Some([payload & 1 != 0, payload & 2 != 0]));
                }
                if family == "overwrite" {
                    assert_eq!(summary.matched_records, 2);
                    assert_eq!(summary.active_records, 1);
                }
            }
        }
    }
}

#[test]
fn invalid_source_identities_and_modified_a2_input_are_rejected() {
    assert!(experiment::source_events("invented", 0, 0).is_err());
    assert!(experiment::source_events("isolated", 4, 0).is_err());
    assert!(experiment::source_events("isolated", 0, 2).is_err());
    assert!(experiment::diagnose("ROW,untrusted,substitution").is_err());
}

#[test]
fn full_a3_grid_reruns_identically_and_retains_all_outcomes() {
    let first = experiment::run().unwrap();
    let second = experiment::run().unwrap();
    assert!(first.csv == second.csv, "A3 deterministic replay mismatch");
    assert_eq!(first.packet, second.packet);
    assert_eq!(first.summary, second.summary);
    assert_eq!(first.packet.len(), 1728);
    let mut groups: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut rows = 0;
    for row in first.csv.lines().filter(|line| line.starts_with("A3_ROW,")) {
        let f: Vec<_> = row.split(',').collect();
        assert_eq!(f.len(), 19);
        assert!(groups.entry(f[1..6].join(",")).or_default().insert(f[6]));
        if f[9] == "agreement" {
            assert_eq!(f[8], f[11], "source agreement cannot repair latent output");
        } else {
            assert_eq!(f[11], "unknown");
        }
        assert_eq!(f[12] == "1", f[7] == f[8]);
        let partitions: u32 = f[13..17].iter().map(|s| s.parse::<u32>().unwrap()).sum();
        assert_eq!(partitions, 1);
        rows += 1;
    }
    assert_eq!(rows, 1728);
    assert_eq!(groups.len(), 576);
    assert!(groups.values().all(|arms| arms.len() == 3));
    assert!(first.csv.contains("PROMOTION,BLOCKED,"));
    // Existing store-evidence CI emits all rows. No test pushes code or dispatches work.
    println!("A3_FULL_BEGIN\n{}A3_FULL_END", first.csv);
    println!("A3_OUTPUT_SHA256={}", sha256_raw_bytes(first.csv.as_bytes()).unwrap().hex());
    println!("A3_PACKET_SHA256={}", sha256_raw_bytes(first.packet.as_bytes()).unwrap().hex());
    for (i, chunk) in first.packet.as_bytes().chunks(144).enumerate() {
        println!("A3_PACKET_CHUNK,{i},{}", std::str::from_utf8(chunk).unwrap());
    }
    println!(
        "A3_CODE_SHA256={}",
        sha256_raw_bytes(include_str!("../examples/dkv_observed_history_a3.rs").as_bytes())
            .unwrap()
            .hex()
    );
    println!(
        "A3_TEST_SHA256={}",
        sha256_raw_bytes(include_str!("dkv_observed_history_a3.rs").as_bytes()).unwrap().hex()
    );
    println!(
        "A3_PROTOCOL_SHA256={}",
        sha256_raw_bytes(include_str!("../../../docs/SML-DKV-A3-BATCH10-PREREGISTRATION.md").as_bytes())
            .unwrap()
            .hex()
    );
}
