//! Software conformance only, never a source/context promotion gate.
#[allow(dead_code)]
#[path = "../examples/dkv_projection_contrast_a2.rs"]
mod experiment;

#[test]
fn complement_is_an_involution_and_preserves_other_fields() {
    experiment::conformance(0).unwrap();
}
#[test]
fn standard_uses_actual_mor_and_unmodified_dkv2a() {
    experiment::conformance(1).unwrap();
}
#[test]
fn contrast_is_exact_and_zero_or_malformed_keys_are_rejected() {
    experiment::conformance(2).unwrap();
}
#[test]
fn paired_projection_has_exact_antisymmetry() {
    experiment::conformance(3).unwrap();
}
#[test]
fn cached_and_fresh_recurrence_have_identical_states_and_traces() {
    experiment::conformance(4).unwrap();
}
#[test]
fn isolated_association_matches_independent_truth_tables() {
    experiment::conformance(5).unwrap();
}
#[test]
fn payload_does_not_enter_projection_and_extra_work_is_counted() {
    experiment::conformance(6).unwrap();
}
#[test]
fn complete_panel_reruns_are_identical_without_quality_assertions() {
    experiment::conformance(7).unwrap();
}
