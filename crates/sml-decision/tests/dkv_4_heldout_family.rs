#[path = "../examples/dkv_4_heldout_family.rs"]
mod experiment;

#[test]
fn heldout_family_report_is_deterministic_and_retains_refutations() {
    let first = experiment::report().unwrap();
    let second = experiment::report().unwrap();
    assert_eq!(first, second);
    assert!(first.contains("PROMOTION,BLOCKED"));
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("FIT,"))
            .count(),
        1
    );

    if first.contains("DOWNSTREAM,SKIPPED") {
        assert_eq!(
            first
                .lines()
                .filter(|line| line.starts_with("ROW,"))
                .count(),
            0
        );
    } else {
        assert_eq!(
            first
                .lines()
                .filter(|line| line.starts_with("ROW,"))
                .count(),
            256
        );
        assert_eq!(
            first
                .lines()
                .filter(|line| line.starts_with("SUMMARY,"))
                .count(),
            7
        );
        assert_eq!(
            first
                .lines()
                .filter(|line| line.starts_with("MISSING_FEATURE,"))
                .count(),
            16
        );
    }
}

#[test]
fn route_choice_is_case_parity_only_in_retained_rows() {
    let text = experiment::report().unwrap();
    if text.contains("DOWNSTREAM,SKIPPED") {
        return;
    }
    for line in text.lines().filter(|line| line.starts_with("ROW,")) {
        let parity = field(line, "parity=");
        let favored = field(line, "favored=");
        assert_eq!(favored, if parity == "true" { "1" } else { "0" });
        assert!(line.contains("first_margin=40000"), "{line}");
        assert!(line.contains("first_selected=2"), "{line}");
        assert!(line.contains("second_margin=70000"), "{line}");
        assert!(line.contains("second_selected=1"), "{line}");
    }
}

#[test]
fn zero_answer_control_marks_risk_undefined_when_downstream_runs() {
    let text = experiment::report().unwrap();
    if text.contains("DOWNSTREAM,SKIPPED") {
        return;
    }
    let universal = summary(&text, "UNIVERSAL_ABSTAIN");
    assert!(universal.contains("answered=0"), "{universal}");
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
