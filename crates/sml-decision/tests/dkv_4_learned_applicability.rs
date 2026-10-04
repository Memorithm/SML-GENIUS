#[path = "../examples/dkv_4_learned_applicability.rs"]
mod experiment;

#[test]
fn learned_applicability_report_is_deterministic_and_falsifiable() {
    let first = experiment::report().unwrap();
    let second = experiment::report().unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("FIT,"))
            .count(),
        2
    );
    assert!(first.contains("PROMOTION,BLOCKED"));

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
            128
        );
        assert_eq!(
            first
                .lines()
                .filter(|line| line.starts_with("MISSING_METADATA,"))
                .count(),
            16
        );
        assert_eq!(
            first
                .lines()
                .filter(|line| line.starts_with("SUMMARY,"))
                .count(),
            6
        );
    }
}

#[test]
fn split_identity_is_explicit_and_disjoint_by_declared_ranges() {
    let text = experiment::report().unwrap();
    let split = text
        .lines()
        .find(|line| line.starts_with("SPLIT,"))
        .unwrap();
    assert!(split.contains("train_records=64"), "{split}");
    assert!(split.contains("validation_records=64"), "{split}");
    assert!(split.contains("test_metadata_records=64"), "{split}");
    assert!(split.contains("10000-10063|30000-30063"), "{split}");
    assert!(split.contains("20000-20063|40000-40063"), "{split}");
    assert!(split.contains("50000-50127"), "{split}");
}

#[test]
fn zero_answer_arm_marks_selective_risk_undefined_when_downstream_runs() {
    let text = experiment::report().unwrap();
    if text.contains("DOWNSTREAM,SKIPPED") {
        return;
    }
    let universal = text
        .lines()
        .find(|line| line.starts_with("SUMMARY,UNIVERSAL_ABSTAIN,"))
        .unwrap();
    assert!(universal.contains("answered=0"), "{universal}");
    assert!(universal.contains("risk_defined=false"), "{universal}");
}
