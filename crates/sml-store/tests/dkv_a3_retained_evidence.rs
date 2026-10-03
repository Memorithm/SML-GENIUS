use sml_core::delta_kv::{DeltaKvCounters, DeltaKvMemory};
use sml_store::observed_history::{
    FrozenDeltaKvRead, HistoryLimits, ObservationEvent, ObservationLog, SubjectKey,
};
use sml_store::raw_context::sha256_raw_bytes;

#[allow(dead_code)]
#[path = "../examples/dkv_observed_history_a3.rs"]
mod experiment;

const SUMMARY: &str = include_str!("../../../research/dkv-a3/SUMMARY.csv");
const MANIFEST: &str = include_str!("../../../research/dkv-a3/MANIFEST.txt");
const OUTPUT_HASH: &str = "2b9793b6ad72c8d473cb418264786bac6268b496c01baaf6f1e5d7e9def4e22f";

fn manifest_value(name: &str) -> &str {
    let prefix = format!("{name}=");
    let values: Vec<_> = MANIFEST
        .lines()
        .filter_map(|line| line.strip_prefix(&prefix))
        .collect();
    assert_eq!(values.len(), 1, "missing or duplicate manifest identity");
    values[0]
}

#[test]
fn full_replay_matches_observed_hash_and_retained_summary() {
    // This is a post-observation regression check, not a new confirmation study.
    let actual = experiment::run().unwrap();
    assert_eq!(sha256_raw_bytes(actual.csv.as_bytes()).unwrap().hex(), OUTPUT_HASH);
    assert_eq!(actual.summary, SUMMARY);
    let rows = actual.csv.lines().filter(|line| line.starts_with("A3_ROW,")).count();
    assert_eq!(rows, 1728);
    assert_eq!(manifest_value("full_output_sha256"), OUTPUT_HASH);
    assert_eq!(manifest_value("scientific_promotion"), "BLOCKED");
}

#[test]
fn manifest_binds_actual_example_and_prospective_protocol() {
    let example = include_str!("../examples/dkv_observed_history_a3.rs");
    let protocol = include_str!("../../../docs/SML-DKV-A3-BATCH10-PREREGISTRATION.md");
    let original_test = include_str!("dkv_observed_history_a3.rs");
    for (name, content) in [
        ("example_sha256", example),
        ("protocol_sha256", protocol),
        ("original_test_sha256", original_test),
    ] {
        let hash = sha256_raw_bytes(content.as_bytes()).unwrap().hex();
        assert_eq!(hash, manifest_value(name), "frozen source identity drift");
    }
    assert_eq!(manifest_value("new_remoteops_dispatch"), "false");
    assert_eq!(manifest_value("latent_improvement_claim"), "false");
}

#[test]
fn matched_observations_expose_old_value_loss_while_cache_preserves_numeric_result() {
    let old_key = SubjectKey::new(1, 0).unwrap();
    let new_key = SubjectKey::new(2, 0).unwrap();
    let old = ObservationEvent::new(1, old_key, [true; 2], None).unwrap();
    let new = ObservationEvent::new(2, new_key, [false; 2], None).unwrap();
    let mut direct = ObservationLog::new(HistoryLimits {
        max_events: 2,
        max_keys: 2,
        max_element_bytes: 4096,
    })
    .unwrap();
    let mut matrix = DeltaKvMemory::new(2, 2).unwrap();
    let mut ingest = DeltaKvCounters::default();
    for (event, key) in [(old, [4096, 0]), (new, [4096, 2048])] {
        direct.append(event).unwrap();
        let values = event.bits().map(|bit| if bit { 4096 } else { -4096 });
        matrix.update(&key, &values, 4096, &mut ingest).unwrap();
    }
    assert_eq!(direct.query(old_key).summary().value, Some([true; 2]));
    let key = [4096, 0];
    let mut fresh = DeltaKvCounters::default();
    let mut cache = FrozenDeltaKvRead::new(&matrix, &key, 8).unwrap();
    for _ in 0..4 {
        let numeric = matrix.read(&key, &mut fresh).unwrap();
        assert!(numeric.iter().all(|value| *value < 0));
        assert_eq!(cache.read().unwrap(), numeric);
    }
    assert_eq!(ingest.updates, 2);
    assert_eq!(ingest.read_macs, 8);
    assert_eq!(fresh.reads, 4);
    assert_eq!(fresh.read_macs, 16);
    assert_eq!(cache.numeric_counters().reads, 1);
    assert_eq!(cache.numeric_counters().read_macs, 4);
    assert_eq!(cache.cache_hits(), 3);
    assert_eq!(cache.cached_value_bytes(), 8);
    // Both controls receive the same two observations. Representation and storage
    // differ; this is a known counterexample, not an equal-resource superiority claim.
}
