use sml_core::delta_kv_controls::{
    frozen_dkv_control_script, run_dkv_matched_controls, DkvControlBackend,
};

fn backend(
    report: &sml_core::delta_kv_controls::DkvMatchedControlReport,
    backend: DkvControlBackend,
) -> &sml_core::delta_kv_controls::DkvControlBackendReport {
    report
        .backends
        .iter()
        .find(|item| item.backend == backend)
        .expect("backend report must exist")
}

#[test]
fn all_four_backends_run_the_same_frozen_script() {
    let script = frozen_dkv_control_script();
    let report = run_dkv_matched_controls().unwrap();

    assert_eq!(script.len(), 14);
    assert_eq!(report.operations, 14);
    assert_eq!(report.writes, 6);
    assert_eq!(report.reads, 8);
    assert_eq!(report.backends.len(), 4);

    for item in &report.backends {
        assert_eq!(item.writes_attempted, report.writes);
        assert_eq!(item.reads, report.reads);
        assert_eq!(
            item.exact_reads + item.inexact_reads + item.misses,
            report.reads
        );
    }
}

#[test]
fn declared_storage_budget_is_shared_and_never_exceeded() {
    let report = run_dkv_matched_controls().unwrap();

    assert_eq!(report.storage_budget_bits, 704);
    assert_eq!(report.direct_capacity, 3);
    assert_eq!(report.append_capacity, 3);

    for item in &report.backends {
        assert!(item.semantic_bits <= report.storage_budget_bits);
        assert_eq!(
            item.semantic_bits + item.unused_budget_bits,
            report.storage_budget_bits
        );
    }

    assert_eq!(
        backend(&report, DkvControlBackend::DeltaKv).semantic_bits,
        704
    );
    assert_eq!(
        backend(&report, DkvControlBackend::AppendLedger).semantic_bits,
        704
    );
    assert_eq!(
        backend(&report, DkvControlBackend::DirectRejectCollision).semantic_bits,
        676
    );
}

#[test]
fn delta_kv_commits_every_write_but_retains_approximation_error_as_evidence() {
    let report = run_dkv_matched_controls().unwrap();
    let delta = backend(&report, DkvControlBackend::DeltaKv);

    assert_eq!(delta.writes_committed, 6);
    assert_eq!(delta.write_rejections, 0);
    assert_eq!(delta.misses, 0);
    assert_eq!(delta.memory_collisions, 0);
    assert_eq!(delta.memory_replacements, 0);
    assert!(delta.read_macs > 0);
    assert!(delta.weight_updates > 0);
    assert_eq!(delta.saturations, 0);
}

#[test]
fn reject_collision_control_preserves_residents_and_rejects_colliding_new_keys() {
    let report = run_dkv_matched_controls().unwrap();
    let reject = backend(&report, DkvControlBackend::DirectRejectCollision);

    assert_eq!(reject.writes_committed, 4);
    assert_eq!(reject.write_rejections, 2);
    assert_eq!(reject.memory_collisions, 2);
    assert_eq!(reject.memory_replacements, 0);
    assert_eq!(reject.misses, 2);
    assert_eq!(reject.inexact_reads, 0);
    assert_eq!(reject.exact_reads, 6);
}

#[test]
fn replace_existing_control_trades_resident_stability_for_new_colliding_keys() {
    let report = run_dkv_matched_controls().unwrap();
    let replace = backend(&report, DkvControlBackend::DirectReplaceExisting);

    assert_eq!(replace.writes_committed, 6);
    assert_eq!(replace.write_rejections, 0);
    assert_eq!(replace.memory_collisions, 2);
    assert_eq!(replace.memory_replacements, 2);
    assert_eq!(replace.misses, 2);
    assert_eq!(replace.inexact_reads, 0);
    assert_eq!(replace.exact_reads, 6);
}

#[test]
fn bounded_append_ledger_exposes_capacity_and_stale_value_failure_modes() {
    let report = run_dkv_matched_controls().unwrap();
    let ledger = backend(&report, DkvControlBackend::AppendLedger);

    assert_eq!(ledger.writes_committed, 3);
    assert_eq!(ledger.write_rejections, 3);
    assert_eq!(ledger.exact_reads, 5);
    assert_eq!(ledger.inexact_reads, 1);
    assert_eq!(ledger.misses, 2);
    assert!(ledger.l1_error_q12 > 0);
    assert!(ledger.ledger_scan_comparisons > 0);
}

#[test]
fn direct_controls_use_key_comparisons_while_delta_uses_matrix_macs() {
    let report = run_dkv_matched_controls().unwrap();
    let delta = backend(&report, DkvControlBackend::DeltaKv);
    let reject = backend(&report, DkvControlBackend::DirectRejectCollision);
    let replace = backend(&report, DkvControlBackend::DirectReplaceExisting);

    assert!(delta.read_macs > 0);
    assert_eq!(delta.key_comparisons, 0);
    assert_eq!(reject.read_macs, 0);
    assert_eq!(replace.read_macs, 0);
    assert!(reject.key_comparisons > 0);
    assert!(replace.key_comparisons > 0);
}

#[test]
fn matched_control_report_is_deterministic() {
    assert_eq!(
        run_dkv_matched_controls().unwrap(),
        run_dkv_matched_controls().unwrap()
    );
}

#[test]
fn report_does_not_collapse_backends_into_a_single_winner_metric() {
    let report = run_dkv_matched_controls().unwrap();

    let exact = report
        .backends
        .iter()
        .map(|item| (item.backend, item.exact_reads, item.l1_error_q12))
        .collect::<Vec<_>>();
    assert_eq!(exact.len(), 4);
    assert!(exact
        .iter()
        .any(|(_, exact_reads, _)| *exact_reads < report.reads));
}
