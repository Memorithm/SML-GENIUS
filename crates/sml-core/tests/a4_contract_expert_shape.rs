#![allow(dead_code)] // Each contract uses only the parsers it needs.
//! Frozen A4 closed-loop contract regression; diagnostic, not training evidence.
const GOLDEN: &str =
    include_str!("../../../docs/research/evidence/a4-closed-loop-20261009/trajectory.csv");
fn word4(s: &str, prefix: &str) -> bool {
    s.strip_prefix(prefix).is_some_and(|v| {
        let parts: Vec<_> = v.split(':').collect();
        parts.len() == 4
            && parts
                .iter()
                .all(|w| w.len() == 2 && u8::from_str_radix(w, 16).is_ok())
    })
}
fn int_vector(s: &str, prefix: &str, count: usize) -> bool {
    s.strip_prefix(prefix).is_some_and(|v| {
        let p: Vec<_> = v.split(':').collect();
        p.len() == count && p.iter().all(|x| x.parse::<i32>().is_ok())
    })
}
fn valid(report: &str) -> bool {
    let lines: Vec<_> = report.lines().collect();
    if lines.is_empty() {
        return false;
    }
    lines
        .iter()
        .filter(|l| l.starts_with("FRAME,"))
        .all(|l| word4(l.split(',').nth(6).unwrap_or(""), "expert="))
}
#[test]
fn frozen_contract_holds() {
    assert!(valid(GOLDEN));
}
#[test]
fn malformed_report_rejected() {
    assert!(!valid(""));
}
