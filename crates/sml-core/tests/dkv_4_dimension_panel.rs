#[path = "../examples/dkv_4_dimension_panel.rs"]
mod experiment;

#[test]
fn paired_dimension_panel_is_complete_and_deterministic() {
    let first = experiment::report().unwrap();
    let second = experiment::report().unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("ROW,"))
            .count(),
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
fn missing_safety_path_never_reads_or_stops_in_any_retained_row() {
    let text = experiment::report().unwrap();
    for line in text
        .lines()
        .filter(|line| line.starts_with("ROW,") && line.contains(",unknown,"))
    {
        assert!(line.contains("safe_reads=0"));
        assert!(line.contains("safe_stop=false"));
    }
}
