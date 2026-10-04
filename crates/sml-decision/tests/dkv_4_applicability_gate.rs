#[path = "../examples/dkv_4_applicability_gate.rs"]
mod experiment;

#[test]
fn applicability_panel_is_complete_and_deterministic() {
    let first = experiment::report().unwrap();
    let second = experiment::report().unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("TASK,"))
            .count(),
        4
    );
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("ROW,"))
            .count(),
        8
    );
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("SUMMARY,"))
            .count(),
        4
    );
    assert!(first.contains("PROMOTION,BLOCKED"));
}

#[test]
fn applicability_gate_trades_coverage_for_zero_answered_error_on_frozen_panel() {
    let text = experiment::report().unwrap();
    let unguarded = summary(&text, "DYNAMIC_UNGUARDED");
    assert!(unguarded.contains("answered=8"), "{unguarded}");
    assert!(unguarded.contains("errors=4"), "{unguarded}");
    assert!(unguarded.contains("risk_defined=true"), "{unguarded}");
    assert!(unguarded.contains("risk_ppm=500000"), "{unguarded}");

    let guarded = summary(&text, "DYNAMIC_APPLICABILITY");
    assert!(guarded.contains("answered=4"), "{guarded}");
    assert!(guarded.contains("coverage_ppm=500000"), "{guarded}");
    assert!(guarded.contains("errors=0"), "{guarded}");
    assert!(guarded.contains("risk_ppm=0"), "{guarded}");
    assert!(guarded.contains("abstained_not_allowed=4"), "{guarded}");
}

#[test]
fn first_primary_with_same_gate_matches_dynamic_guard_at_lower_cost() {
    let text = experiment::report().unwrap();
    let dynamic = summary(&text, "DYNAMIC_APPLICABILITY");
    let primary = summary(&text, "FIRST_PRIMARY_APPLICABILITY");

    for field_name in ["answered=", "coverage_ppm=", "errors=", "risk_ppm="] {
        assert_eq!(field(dynamic, field_name), field(primary, field_name));
    }
    assert!(field_u32(primary, "cycles=") <= field_u32(dynamic, "cycles="));
}

#[test]
fn universal_abstention_reports_undefined_risk_not_zero_empirical_risk() {
    let text = experiment::report().unwrap();
    let universal = summary(&text, "UNIVERSAL_ABSTAIN");
    assert!(universal.contains("answered=0"), "{universal}");
    assert!(universal.contains("coverage_ppm=0"), "{universal}");
    assert!(universal.contains("risk_defined=false"), "{universal}");
}

fn summary<'a>(text: &'a str, arm: &str) -> &'a str {
    text.lines()
        .find(|line| line.starts_with(&format!("SUMMARY,{arm},")))
        .unwrap()
}

fn field<'a>(line: &'a str, prefix: &str) -> &'a str {
    line.split(',')
        .find_map(|part| part.strip_prefix(prefix))
        .unwrap()
}

fn field_u32(line: &str, prefix: &str) -> u32 {
    field(line, prefix).parse().unwrap()
}
