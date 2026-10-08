//! Independent std-only replay of the declared A4 golden trajectory.
//! Plain four-word states and direct subset-membership parity, no sml-core API.
use std::{fmt::Write as _, io::Read};
const SUMMARY:&str="A4_SBG_SUMMARY,cases=192,cycles=720,state_words=2880,output_words=11880,checkpoint_replays=192,trace_roundtrips=192,pascal=540,sparse=120,submask=60,mismatches=0";
fn seed_word(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
fn reference() -> String {
    let mut report =
        String::from("A4_SBG_PANEL,schema=1,state_bits=64,domain=64,scope=synthetic_correctness\n");
    let mut case = 0;
    for lanes in [1, 32] {
        for family in 0..2 {
            for hint in ["AFFINE_HINT", "LOW_HINT", "HIGH_HINT"] {
                for seed in [0, 1, 0x50425636, u64::MAX] {
                    for depth in [1, 2, 4, 8] {
                        writeln!(report,"CASE,{case},lanes={lanes},family={family},hint={hint},seed={seed:016x},depth={depth}").unwrap();
                        let mut state = [
                            seed_word(seed),
                            seed_word(seed ^ 1),
                            seed_word(seed ^ 2),
                            seed_word(seed ^ 3),
                        ];
                        let terms = if family == 0 {
                            &[0u64, 1][..]
                        } else {
                            &[0u64, 1, 3, 7][..]
                        };
                        for step in 1..=depth {
                            let mut queries = Vec::new();
                            let mut outputs = Vec::new();
                            let mut new_value = state[0];
                            for lane in 0..lanes {
                                let sources = [
                                    (0, lane),
                                    (0, lane + 1),
                                    (1, lane + 2),
                                    (1, lane + 3),
                                    (2, lane + 4),
                                    (3, lane + 5),
                                ];
                                let address = sources.iter().enumerate().fold(
                                    0u64,
                                    |address, (i, &(field, bit))| {
                                        address | (((state[field] >> (bit % 64)) & 1) << i)
                                    },
                                );
                                let value =
                                    (terms.iter().filter(|&&mask| address & mask == mask).count()
                                        % 2) as u64;
                                new_value = (new_value & !(1u64 << lane)) | (value << lane);
                                queries.push(format!("{address:02x}"));
                                outputs.push(format!("{value:016x}"));
                            }
                            state[0] = new_value;
                            writeln!(
                                report,
                                "FRAME,{case},{step},0,{:016x},{:016x},{:016x},{:016x},{},{}",
                                state[0],
                                state[1],
                                state[2],
                                state[3],
                                queries.join(":"),
                                outputs.join(":")
                            )
                            .unwrap();
                        }
                        case += 1;
                    }
                }
            }
        }
    }
    writeln!(report, "{SUMMARY}").unwrap();
    report.push_str("PROMOTION,BLOCKED,declared_SBG_execution_only_not_learning_or_speed\n");
    report
}
fn verify(text: &str) -> Result<(), String> {
    let expected = reference();
    if text == expected {
        return Ok(());
    }
    let mismatch = text
        .lines()
        .zip(expected.lines())
        .position(|(a, b)| a != b)
        .map(|i| i + 1);
    Err(format!(
        "A4 complete-trajectory mismatch at line {mismatch:?}; actual bytes={}, expected bytes={}",
        text.len(),
        expected.len()
    ))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("usage: verify-a4-sbg trajectory.csv".into());
    }
    let mut text = String::new();
    std::fs::File::open(&args[1])?
        .take(1_048_577)
        .read_to_string(&mut text)?;
    if text.len() > 1_048_576 {
        return Err("trajectory exceeds 1 MiB input cap".into());
    }
    verify(&text).map_err(std::io::Error::other)?;
    println!("A4_INDEPENDENT_REPLAY,cases=192,frames=720,state_words=2880,output_words=11880,mismatches=0");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_reference_has_frozen_shape() {
        let text = reference();
        assert_eq!(text.lines().count(), 915);
        assert_eq!(text.len(), 310218);
        assert!(verify(&text).is_ok());
    }
    #[test]
    fn missing_extra_and_reordered_records_are_rejected() {
        let text = reference();
        assert!(verify(&text[..text.len() - 1]).is_err());
        assert!(verify(&(text.clone() + "EXTRA\n")).is_err());
        let mut rows = text.lines().collect::<Vec<_>>();
        rows.swap(1, 2);
        assert!(verify(&(rows.join("\n") + "\n")).is_err());
    }
    #[test]
    fn changed_state_query_or_output_is_rejected() {
        let text = reference();
        let original = text
            .lines()
            .find(|line| line.starts_with("FRAME,"))
            .unwrap();
        for column in [4, 8, 9] {
            let mut fields = original.split(',').map(str::to_owned).collect::<Vec<_>>();
            let first = fields[column].as_bytes()[0];
            fields[column].replace_range(..1, if first == b'0' { "1" } else { "0" });
            assert!(verify(&text.replacen(original, &fields.join(","), 1)).is_err());
        }
    }
    #[test]
    fn wrong_metadata_and_forged_success_summary_are_rejected() {
        let text = reference();
        assert!(verify(&text.replacen("lanes=1", "lanes=32", 1)).is_err());
        assert!(verify(&text.replacen("mismatches=0", "mismatches=1", 1)).is_err());
    }
}
