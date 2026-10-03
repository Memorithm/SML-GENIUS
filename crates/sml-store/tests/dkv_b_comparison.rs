#[allow(dead_code)]
#[path = "../examples/dkv_counterpart_comparison.rs"]
mod comparison;
#[test]
fn differential_grid_is_complete_and_reproducible() {
    let a = comparison::report().unwrap();
    let b = comparison::report().unwrap();
    assert_eq!(a, b);
    assert_eq!(a.lines().filter(|l| l.starts_with("ROW,")).count(), 1728);
    assert_eq!(a.lines().filter(|l| l.starts_with("SUMMARY,")).count(), 18);
    assert!(a.contains("PROMOTION,BLOCKED"));
}
