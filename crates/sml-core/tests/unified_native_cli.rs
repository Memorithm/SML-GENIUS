//! Exercise the real executable, not only its in-process constructor.
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sml-unified-loop-smoke"))
        .args(args)
        .output()
        .expect("native unified-loop smoke executable must run")
}

#[test]
fn cli_runs_all_stages_for_eight_cycles_with_identical_replay() {
    let first = run(&["8", "165", "255", "toggle"]);
    let second = run(&["8", "165", "255", "toggle"]);
    assert!(first.status.success(), "{:?}", first);
    assert!(second.status.success(), "{:?}", second);
    assert_eq!(first.stdout, second.stdout);
    assert!(first.stderr.is_empty());
    let text = String::from_utf8(first.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 10);
    assert!(lines[0].contains("\"schema\":\"sml.unified-loop-smoke/v1\""));
    assert!(lines[0].contains("\"large_anf_selected_experts\":false"));
    for (index, line) in lines[1..9].iter().enumerate() {
        assert!(line.contains(&format!("\"depth\":{}", index + 1)));
        assert!(line.contains("\"executed_pages\":1"));
        assert!(line.contains("\"memory_read_macs\":8"));
    }
    for stage in [
        "cycles_committed",
        "anf_cycles",
        "routing_cycles",
        "expert_cycles",
        "memory_cycles",
    ] {
        assert!(lines[9].contains(&format!("\"{stage}\":8")));
    }
}

#[test]
fn cli_anf_control_changes_executed_first_stage() {
    let identity = run(&["1", "0", "0", "identity"]);
    let toggle = run(&["1", "0", "0", "toggle"]);
    assert!(identity.status.success());
    assert!(toggle.status.success());
    let identity = String::from_utf8(identity.stdout).unwrap();
    let toggle = String::from_utf8(toggle.stdout).unwrap();
    assert!(identity
        .lines()
        .nth(1)
        .unwrap()
        .contains("\"anf_words\":[0]"));
    assert!(toggle
        .lines()
        .nth(1)
        .unwrap()
        .contains("\"anf_words\":[128]"));
}

#[test]
fn invalid_cli_bounds_exit_nonzero_without_success_evidence() {
    for args in [vec!["0"], vec!["65"], vec!["4", "256"]] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}
