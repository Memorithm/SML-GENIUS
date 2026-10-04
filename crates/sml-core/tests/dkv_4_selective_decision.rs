#[path = "../examples/dkv_4_selective_decision.rs"]
mod experiment;

#[test]
fn selective_decision_panel_is_deterministic_and_complete() {
    let first = experiment::report().unwrap();
    let second = experiment::report().unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("MOR,"))
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
            .filter(|line| line.starts_with("SAFETY,"))
            .count(),
        4
    );
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("SUMMARY,"))
            .count(),
        6
    );
    assert!(first.contains("PROMOTION,BLOCKED"));
}

#[test]
fn frozen_route_geometry_is_measured_not_assumed() {
    let text = experiment::report().unwrap();
    for line in text.lines().filter(|line| line.starts_with("MOR,")) {
        assert!(line.contains("first_margin=40000"), "{line}");
        assert!(line.contains("first_precision=true"), "{line}");
        assert!(line.contains("first_selected=2"), "{line}");
        assert!(line.contains("second_margin=70000"), "{line}");
        assert!(line.contains("second_precision=false"), "{line}");
        assert!(line.contains("second_selected=1"), "{line}");
    }
}

#[test]
fn selective_coverage_does_not_hide_misaligned_errors() {
    let text = experiment::report().unwrap();
    let dynamic = summary(&text, "DYNAMIC_DKV4F_SELECTIVE");
    assert!(dynamic.contains("answered=8"), "{dynamic}");
    assert!(dynamic.contains("errors=4"), "{dynamic}");
    assert!(dynamic.contains("risk_ppm=500000"), "{dynamic}");

    let aligned = summary(&text, "DYNAMIC_ALIGNED");
    assert!(aligned.contains("answered=4"), "{aligned}");
    assert!(aligned.contains("errors=0"), "{aligned}");

    let misaligned = summary(&text, "DYNAMIC_MISALIGNED");
    assert!(misaligned.contains("answered=4"), "{misaligned}");
    assert!(misaligned.contains("errors=4"), "{misaligned}");
}

#[test]
fn first_route_selective_arms_abstain_and_first_primary_matches_dynamic_risk() {
    let text = experiment::report().unwrap();
    let stability = summary(&text, "STABILITY_SELECTIVE");
    let frozen = summary(&text, "FROZEN_DKV4C_SELECTIVE");
    let dynamic = summary(&text, "DYNAMIC_DKV4F_SELECTIVE");
    let aggressive = summary(&text, "FIRST_PRIMARY");

    assert!(stability.contains("answered=0"), "{stability}");
    assert!(frozen.contains("answered=0"), "{frozen}");
    assert_eq!(field(dynamic, "errors="), field(aggressive, "errors="));
    assert!(
        field_u32(aggressive, "cycles=") <= field_u32(dynamic, "cycles="),
        "aggressive={aggressive} dynamic={dynamic}"
    );
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
