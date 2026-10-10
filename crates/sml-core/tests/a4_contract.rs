//! A4 closed-loop contract checks (merged from the 20 former `a4_contract_*.rs`
//! files). Diagnostic, not training evidence.
//!
//! Previously every validator only parsed the frozen CSV in `docs/`, so these
//! tests could not detect a change in the code. Each validator below is kept
//! verbatim and now also runs on the report regenerated *live* by the real
//! ANF -> SBG/MOR -> expert -> Delta-KV closed loop
//! (`support/a4_closed_loop.rs::qualification_report`), and the live report must
//! be byte-identical to the frozen golden.
#[path = "support/a4_closed_loop.rs"]
mod fixture;

use std::sync::OnceLock;

const GOLDEN: &str =
    include_str!("../../../docs/research/evidence/a4-closed-loop-20261009/trajectory.csv");

/// Report produced by executing the closed loop now (computed once per binary).
fn live_report() -> &'static str {
    static REPORT: OnceLock<String> = OnceLock::new();
    REPORT.get_or_init(fixture::qualification_report)
}

#[test]
fn live_report_matches_frozen_golden() {
    assert_eq!(live_report(), GOLDEN);
}

#[allow(dead_code)] // Each contract uses only the parsers it needs.
fn word4(s: &str, prefix: &str) -> bool {
    s.strip_prefix(prefix).is_some_and(|v| {
        let parts: Vec<_> = v.split(':').collect();
        parts.len() == 4
            && parts
                .iter()
                .all(|w| w.len() == 2 && u8::from_str_radix(w, 16).is_ok())
    })
}
#[allow(dead_code)]
fn int_vector(s: &str, prefix: &str, count: usize) -> bool {
    s.strip_prefix(prefix).is_some_and(|v| {
        let p: Vec<_> = v.split(':').collect();
        p.len() == count && p.iter().all(|x| x.parse::<i32>().is_ok())
    })
}

/// One former `a4_contract_<name>.rs` file: same `valid` and the same two tests,
/// plus `live_report_holds`, which applies `valid` to the live report.
macro_rules! contract {
    ($name:ident, $valid:item) => {
        mod $name {
            #[allow(unused_imports)]
            use super::{int_vector, live_report, word4, GOLDEN};
            $valid
            #[test]
            fn frozen_contract_holds() {
                assert!(valid(GOLDEN));
            }
            #[test]
            fn malformed_report_rejected() {
                assert!(!valid(""));
            }
            #[test]
            fn live_report_holds() {
                assert!(valid(live_report()));
            }
        }
    };
}

contract!(
    blocked_flag,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines
            .iter()
            .filter(|l| l.starts_with("FRAME,"))
            .all(|l| matches!(l.split(',').nth(10), Some("blocked=0" | "blocked=1")))
    }
);

contract!(
    case_count,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines.iter().filter(|l| l.starts_with("CASE,")).count() == 108
    }
);

contract!(
    case_ids,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines
            .iter()
            .filter(|l| l.starts_with("CASE,"))
            .enumerate()
            .all(|(i, l)| l.split(',').nth(1).unwrap_or("").parse::<usize>() == Ok(i))
    }
);

contract!(
    case_metadata,
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
);

contract!(
    expert_shape,
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
);

contract!(
    frame_case_bounds,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines.iter().filter(|l| l.starts_with("FRAME,")).all(|l| {
            l.split(',')
                .nth(1)
                .unwrap_or("")
                .parse::<usize>()
                .is_ok_and(|i| i < 108)
        })
    }
);

contract!(
    frame_count,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines.iter().filter(|l| l.starts_with("FRAME,")).count() == 456
    }
);

contract!(
    frame_depth_bounds,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines.iter().filter(|l| l.starts_with("FRAME,")).all(|l| {
            l.split(',')
                .nth(2)
                .unwrap_or("")
                .parse::<usize>()
                .is_ok_and(|i| (1..=8).contains(&i))
        })
    }
);

contract!(
    input_shape,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines
            .iter()
            .filter(|l| l.starts_with("FRAME,"))
            .all(|l| word4(l.split(',').nth(5).unwrap_or(""), "input="))
    }
);

contract!(
    key_dimension,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines
            .iter()
            .filter(|l| l.starts_with("FRAME,"))
            .all(|l| int_vector(l.split(',').nth(8).unwrap_or(""), "key=", 8))
    }
);

contract!(
    memory_count,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines.iter().filter(|l| l.starts_with("MEMORY,")).count() == 108
    }
);

contract!(
    memory_ids,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines
            .iter()
            .filter(|l| l.starts_with("MEMORY,"))
            .enumerate()
            .all(|(i, l)| l.split(',').nth(1).unwrap_or("").parse::<usize>() == Ok(i))
    }
);

contract!(
    memory_vector,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines
            .iter()
            .filter(|l| l.starts_with("MEMORY,"))
            .all(|l| int_vector(l.split(',').nth(2).unwrap_or(""), "weights=", 8))
    }
);

contract!(
    output_shape,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines
            .iter()
            .filter(|l| l.starts_with("FRAME,"))
            .all(|l| word4(l.split(',').nth(7).unwrap_or(""), "output="))
    }
);

contract!(
    projected_coordinates,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines.iter().filter(|l| l.starts_with("FRAME,")).all(|l| {
            l.split(',')
                .nth(12)
                .unwrap_or("")
                .strip_prefix("projected_coordinates=")
                .is_some_and(|v| v.parse::<u64>().is_ok())
        })
    }
);

contract!(
    promotion_boundary,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines.last() == Some(&"PROMOTION,BLOCKED,no_learning_speed_or_persistent_checkpoint_claim")
            && lines
                .iter()
                .filter(|l| l.starts_with("A4_CLOSED_SUMMARY,"))
                .count()
                == 1
    }
);

contract!(
    read_macs,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines.iter().filter(|l| l.starts_with("FRAME,")).all(|l| {
            l.split(',')
                .nth(11)
                .unwrap_or("")
                .strip_prefix("read_macs=")
                .is_some_and(|v| v.parse::<u64>().is_ok())
        })
    }
);

contract!(
    read_value,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines.iter().filter(|l| l.starts_with("FRAME,")).all(|l| {
            l.split(',')
                .nth(9)
                .unwrap_or("")
                .strip_prefix("read=")
                .is_some_and(|v| v.parse::<i32>().is_ok())
        })
    }
);

contract!(
    revision_bounds,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines.iter().filter(|l| l.starts_with("FRAME,")).all(|l| {
            l.split(',')
                .nth(4)
                .unwrap_or("")
                .strip_prefix("revision=")
                .is_some_and(|s| s.parse::<u64>().is_ok())
        })
    }
);

contract!(
    selected_page_bounds,
    fn valid(report: &str) -> bool {
        let lines: Vec<_> = report.lines().collect();
        if lines.is_empty() {
            return false;
        }
        lines.iter().filter(|l| l.starts_with("FRAME,")).all(|l| {
            l.split(',')
                .nth(3)
                .unwrap_or("")
                .strip_prefix("page=")
                .is_some_and(|s| s == "0" || s == "1")
        })
    }
);
