#[path = "../examples/dkv_4_semantic_impact_panel.rs"]
mod experiment;

#[test]
fn semantic_impact_panel_is_complete_and_deterministic() {
    let first = experiment::report().unwrap();
    let second = experiment::report().unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first.lines().filter(|line| line.starts_with("ROW,")).count(),
        2048
    );
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("SUMMARY,"))
            .count(),
        16
    );
    assert!(first.contains("PROMOTION,BLOCKED"));
}

#[test]
fn semantic_unknown_boundaries_hold_in_every_row() {
    let text = experiment::report().unwrap();
    for line in text
        .lines()
        .filter(|line| line.starts_with("ROW,") && line.contains(",unknown,"))
    {
        assert!(line.contains("semantic_safe_reads=0"));
        assert!(line.contains("semantic_safe_stop=false"));
        assert!(line.contains("semantic_unverified_stop=false"));
    }
}

#[test]
fn panel_retains_both_old_and_semantic_controller_metrics() {
    let text = experiment::report().unwrap();
    for line in text
        .lines()
        .filter(|line| line.starts_with("ROW,") && line.contains(",written,"))
    {
        assert!(line.contains("dkv4_depth="));
        assert!(line.contains("semantic_depth="));
        assert!(line.contains("stability_depth="));
    }
}
