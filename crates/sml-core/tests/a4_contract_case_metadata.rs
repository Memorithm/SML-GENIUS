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
    lines.iter().filter(|l| l.starts_with("CASE,")).all(|l| {
        let f: Vec<_> = l.split(',').collect();
        f.len() == 7
            && f[2]
                .strip_prefix("seed=")
                .is_some_and(|s| s.len() == 16 && u64::from_str_radix(s, 16).is_ok())
            && f[3]
                .strip_prefix("target=")
                .is_some_and(|s| s.parse::<i32>().is_ok())
            && f[4]
                .strip_prefix("limit=")
                .is_some_and(|s| s.parse::<u32>().is_ok_and(|n| (1..=8).contains(&n)))
            && matches!(f[5], "update=0" | "update=1")
            && matches!(f[6], "fixed=0" | "fixed=1")
    })
}
#[test]
fn frozen_contract_holds() {
    assert!(valid(GOLDEN));
}
#[test]
fn malformed_report_rejected() {
    assert!(!valid(""));
}
