use sml_core::delta_kv_panel::{
    frozen_delta_kv_cases, run_frozen_delta_kv_panel, DeltaKvCaseReport, DeltaKvPanelClass,
};

fn case<'a>(reports: &'a [DeltaKvCaseReport], class: DeltaKvPanelClass) -> &'a DeltaKvCaseReport {
    reports
        .iter()
        .find(|report| report.class == class)
        .expect("frozen case must exist")
}

#[test]
fn frozen_panel_has_all_preregistered_cases_once() {
    let definitions = frozen_delta_kv_cases();
    let report = run_frozen_delta_kv_panel().unwrap();

    assert_eq!(definitions.len(), 14);
    assert_eq!(report.total_cases, 14);
    assert_eq!(report.total_updates, 21);

    let mut ids = definitions.iter().map(|item| item.id).collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), definitions.len());

    let mut classes = definitions
        .iter()
        .map(|item| item.class)
        .collect::<Vec<_>>();
    classes.sort_unstable();
    classes.dedup();
    assert_eq!(classes.len(), definitions.len());
}

#[test]
fn orthogonal_writes_preserve_previous_key_exactly() {
    let panel = run_frozen_delta_kv_panel().unwrap();
    let orthogonal = case(&panel.cases, DeltaKvPanelClass::Orthogonal);

    assert_eq!(orthogonal.writes.len(), 2);
    assert_eq!(orthogonal.writes[0].current_key_error_q12, 0);
    assert_eq!(orthogonal.writes[1].current_key_error_q12, 0);
    assert_eq!(orthogonal.writes[1].prior_keys_error_before_q12, 0);
    assert_eq!(orthogonal.writes[1].prior_keys_error_after_q12, 0);
    assert_eq!(orthogonal.counters.saturations, 0);
}

#[test]
fn overlap_bands_expose_monotonic_previous_key_interference() {
    let panel = run_frozen_delta_kv_panel().unwrap();

    let q25 = case(&panel.cases, DeltaKvPanelClass::Overlap25).writes[1]
        .prior_keys_error_after_q12;
    let q50 = case(&panel.cases, DeltaKvPanelClass::Overlap50).writes[1]
        .prior_keys_error_after_q12;
    let q75 = case(&panel.cases, DeltaKvPanelClass::Overlap75).writes[1]
        .prior_keys_error_after_q12;

    assert!(q25 > 0);
    assert!(q25 < q50);
    assert!(q50 < q75);

    for class in [
        DeltaKvPanelClass::Overlap25,
        DeltaKvPanelClass::Overlap50,
        DeltaKvPanelClass::Overlap75,
    ] {
        let report = case(&panel.cases, class);
        assert!(report.writes[1].current_key_error_q12 <= 1);
    }
}

#[test]
fn repeated_overwrite_repairs_current_key_each_time() {
    let panel = run_frozen_delta_kv_panel().unwrap();
    let report = case(&panel.cases, DeltaKvPanelClass::RepeatedOverwrite);

    assert_eq!(report.writes.len(), 3);
    assert!(report
        .writes
        .iter()
        .all(|write| write.current_key_error_q12 <= 1));
    assert_eq!(report.final_active_keys, 1);
    assert_eq!(report.counters.updates, 3);
}

#[test]
fn conflicting_identical_keys_make_incompatibility_explicit() {
    let panel = run_frozen_delta_kv_panel().unwrap();
    let report = case(&panel.cases, DeltaKvPanelClass::ConflictingTarget);

    assert_eq!(report.writes.len(), 2);
    assert!(report.writes[1].current_key_error_q12 <= 1);
    assert!(report.writes[1].prior_keys_error_after_q12 >= 3_500);
}

#[test]
fn partial_learning_rates_reduce_current_key_error_monotonically() {
    let panel = run_frozen_delta_kv_panel().unwrap();
    let e25 = case(&panel.cases, DeltaKvPanelClass::PartialRate25).writes[0]
        .current_key_error_q12;
    let e50 = case(&panel.cases, DeltaKvPanelClass::PartialRate50).writes[0]
        .current_key_error_q12;
    let e75 = case(&panel.cases, DeltaKvPanelClass::PartialRate75).writes[0]
        .current_key_error_q12;
    let e100 = case(&panel.cases, DeltaKvPanelClass::PartialRate100).writes[0]
        .current_key_error_q12;

    assert_eq!((e25, e50, e75, e100), (1200, 800, 400, 0));
}

#[test]
fn saturation_stress_is_not_hidden_as_success() {
    let panel = run_frozen_delta_kv_panel().unwrap();
    let report = case(&panel.cases, DeltaKvPanelClass::SaturationStress);

    assert!(report.counters.saturations > 0);
    assert!(report.writes[0].current_key_error_q12 > 0);
}

#[test]
fn dimension_ladder_increases_semantic_bits_and_update_work() {
    let panel = run_frozen_delta_kv_panel().unwrap();
    let d2 = case(&panel.cases, DeltaKvPanelClass::Dimension2x1);
    let d4 = case(&panel.cases, DeltaKvPanelClass::Dimension4x2);
    let d8 = case(&panel.cases, DeltaKvPanelClass::Dimension8x4);

    assert!(d2.semantic_bits < d4.semantic_bits);
    assert!(d4.semantic_bits < d8.semantic_bits);
    assert!(d2.counters.weight_updates < d4.counters.weight_updates);
    assert!(d4.counters.weight_updates < d8.counters.weight_updates);
    assert!(d2.counters.read_macs < d4.counters.read_macs);
    assert!(d4.counters.read_macs < d8.counters.read_macs);
}

#[test]
fn interference_matrix_retains_each_active_key_after_every_write() {
    let panel = run_frozen_delta_kv_panel().unwrap();
    let overlap = case(&panel.cases, DeltaKvPanelClass::Overlap50);

    assert_eq!(overlap.interference_matrix.len(), 3);
    assert_eq!(overlap.interference_matrix[0].after_write, 0);
    assert_eq!(overlap.interference_matrix[0].key_index, 0);
    assert_eq!(overlap.interference_matrix[1].after_write, 1);
    assert_eq!(overlap.interference_matrix[1].key_index, 0);
    assert_eq!(overlap.interference_matrix[2].after_write, 1);
    assert_eq!(overlap.interference_matrix[2].key_index, 1);
}

#[test]
fn aggregate_panel_counters_include_saturation_and_all_update_work() {
    let panel = run_frozen_delta_kv_panel().unwrap();

    assert_eq!(panel.total_updates, 21);
    assert!(panel.total_reads > 0);
    assert!(panel.total_read_macs > panel.total_reads);
    assert!(panel.total_weight_updates > panel.total_updates);
    assert!(panel.total_saturations > 0);
}
