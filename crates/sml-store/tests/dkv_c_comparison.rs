#[path = "../examples/dkv_aliasing_comparison_c.rs"]
mod experiment;
#[test]
fn adapted_c_grid_is_complete_without_asserting_favorable_counts() {
    let text = experiment::report().unwrap();
    assert_eq!(text.lines().filter(|l| l.starts_with("ROW,")).count(), 3072);
    assert_eq!(
        text.lines().filter(|l| l.starts_with("SUMMARY,")).count(),
        32
    );
    assert!(text.contains("PROMOTION,BLOCKED"));
}
