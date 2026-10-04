#[path = "../examples/dkv_4_mor_actionability.rs"]
mod experiment;

#[test]
fn mor_actionability_panel_is_deterministic_and_complete() {
    let first = experiment::report().unwrap();
    let second = experiment::report().unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("MOR,"))
            .count(),
        3
    );
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("ROW,"))
            .count(),
        6
    );
    assert_eq!(
        first
            .lines()
            .filter(|line| line.starts_with("SAFETY,"))
            .count(),
        3
    );
    assert!(first.contains("PROMOTION,BLOCKED"));
}

#[test]
fn native_mor_signal_classes_are_observed() {
    let text = experiment::report().unwrap();
    let consensus = text
        .lines()
        .find(|line| line.starts_with("MOR,CONSENSUS,"))
        .unwrap();
    assert!(consensus.ends_with(",0,false"));

    let margin = text
        .lines()
        .find(|line| line.starts_with("MOR,MARGIN_ESCALATION,"))
        .unwrap();
    assert!(margin.ends_with(",0,true"));

    let structural = text
        .lines()
        .find(|line| line.starts_with("MOR,STRUCTURAL_DISAGREEMENT,"))
        .unwrap();
    assert!(structural.ends_with(",500000,true"));
}

#[test]
fn fixed_mor_actionability_never_spends_more_cycles_than_dkv4c() {
    let text = experiment::report().unwrap();
    for line in text.lines().filter(|line| line.starts_with("ROW,")) {
        let semantic = field_u32(line, "semantic_depth=");
        let fixed = field_u32(line, "fixed_mor_depth=");
        let dynamic = field_u32(line, "dynamic_mor_depth=");
        assert!(fixed <= semantic, "{line}");
        assert_eq!(dynamic, semantic, "{line}");
        assert!(line.ends_with("final_correct=true"));
    }
}

fn field_u32(line: &str, prefix: &str) -> u32 {
    line.split(',')
        .find_map(|field| field.strip_prefix(prefix))
        .unwrap()
        .parse()
        .unwrap()
}
