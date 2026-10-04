#[path = "../examples/dkv_4_dynamic_mor.rs"]
mod experiment;

#[test]
fn dynamic_mor_panel_is_deterministic() {
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
    assert!(first.contains("PROMOTION,BLOCKED"));
}

#[test]
fn resolving_margin_is_measured_as_40k_then_70k() {
    let text = experiment::report().unwrap();
    let row = text
        .lines()
        .find(|line| line.starts_with("MOR,RESOLVING_MARGIN,"))
        .unwrap();
    assert!(row.contains("first_margin=40000"));
    assert!(row.contains("first_precision=true"));
    assert!(row.contains("second_margin=70000"));
    assert!(row.contains("second_precision=false"));
}

#[test]
fn persistent_controls_do_not_claim_resolution() {
    let text = experiment::report().unwrap();

    let margin = text
        .lines()
        .find(|line| line.starts_with("MOR,PERSISTENT_MARGIN,"))
        .unwrap();
    assert!(margin.contains("second_precision=true"));

    let disagreement = text
        .lines()
        .find(|line| line.starts_with("MOR,STRUCTURAL_DISAGREEMENT,"))
        .unwrap();
    assert!(disagreement.contains("second_disagreement=500000"));
    assert!(disagreement.contains("second_precision=true"));
}

#[test]
fn provenance_safety_rows_measure_zero_missing_and_conflict_reads() {
    let text = experiment::report().unwrap();
    for line in text.lines().filter(|line| line.starts_with("SAFETY,")) {
        assert!(line.contains("missing_depth=0"), "{line}");
        assert!(line.contains("missing_reads=0"), "{line}");
        assert!(line.contains("conflict_depth=0"), "{line}");
        assert!(line.contains("conflict_reads=0"), "{line}");
        assert!(
            line.contains("unverified_decision=Abstain(UnverifiedEvidence)"),
            "{line}"
        );
    }
}

#[test]
fn traces_retain_each_semantic_delta_field() {
    let text = experiment::report().unwrap();
    for line in text.lines().filter(|line| line.starts_with("TRACE,")) {
        assert!(line.contains("delta_v="), "{line}");
        assert!(line.contains("delta_m="), "{line}");
        assert!(line.contains("delta_a="), "{line}");
        assert!(line.contains("delta_r="), "{line}");
    }
}
