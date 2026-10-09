//! Independent std-only reconstruction of the frozen A4 closed-loop report.
//! No sml-core, native router, ANF projector/executor or Delta-KV call.
//! In this fixture top-k=1 sees R=all-zero/all-one and one exact candidate;
//! therefore the selected page is V1. This is not a general MOR implementation.
use std::{fmt::Write as _, io::Read};
const SUMMARY:&str="A4_CLOSED_SUMMARY,cases=108,cycles=456,key_changes=158,route_changes=28,full_pipeline_resumes=108,mismatches=0";
fn hash(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
fn initial(seed: u64) -> [u64; 4] {
    [seed, seed ^ 1, seed ^ 2, seed ^ 3].map(|x| hash(x.wrapping_add(0x9e3779b97f4a7c15)) & 255)
}
fn key(state: [u64; 4], page: u64) -> [i16; 8] {
    let mut buckets = [0i64; 8];
    for (field, word) in state.into_iter().enumerate() {
        for bit in 0..8 {
            let h = hash(17 ^ (field as u64).wrapping_mul(0x9e3779b97f4a7c15) ^ bit);
            let projection = if h & (1 << 62) == 0 { 1 } else { -1 };
            let sign = if word & (1 << bit) != 0 { 1 } else { -1 };
            buckets[(h % 8) as usize] += projection * sign;
        }
    }
    let h = hash(17 ^ 0x6d6f725f70616765 ^ page);
    buckets[(h % 8) as usize] += if h & (1 << 63) == 0 { 32 } else { -32 };
    buckets.map(|x| (x.signum() * 4096) as i16)
}
fn rounded(n: i128, d: i128) -> i128 {
    if n >= 0 {
        (n + d / 2) / d
    } else {
        -((-n + d / 2) / d)
    }
}
fn read(weights: [i16; 8], key: [i16; 8]) -> i32 {
    let sum = weights
        .into_iter()
        .zip(key)
        .map(|(w, k)| i128::from(w) * i128::from(k))
        .sum();
    rounded(sum, 4096) as i32
}
fn seeded_memory(seed: u64, target: i16) -> [i16; 8] {
    let mut weights = [0i16; 8];
    for route in 0..2 {
        for value in 0..2 {
            let mut state = initial(seed);
            state[0] = (state[0] & !3) | value | (route << 1);
            state[3] = if route == 0 { 0 } else { 255 };
            let key = key(state, route);
            let prediction = read(weights, key);
            let target = if route == 0 {
                i128::from(target)
            } else {
                -i128::from(target)
            };
            let norm = key.iter().map(|&k| i128::from(k) * i128::from(k)).sum();
            for (i, w) in weights.iter_mut().enumerate() {
                let delta = rounded(
                    4096 * i128::from(key[i]) * (target - i128::from(prediction)),
                    norm,
                );
                *w = (i128::from(*w) + delta).clamp(i128::from(i16::MIN), i128::from(i16::MAX))
                    as i16;
            }
        }
    }
    weights
}
fn fields(state: [u64; 4]) -> String {
    format!(
        "{:02x}:{:02x}:{:02x}:{:02x}",
        state[0], state[1], state[2], state[3]
    )
}
fn decimal(values: &[i16]) -> String {
    values
        .iter()
        .map(i16::to_string)
        .collect::<Vec<_>>()
        .join(":")
}
#[derive(Default)]
struct Counts {
    cases: usize,
    cycles: usize,
    key_changes: usize,
    route_changes: usize,
}
fn case(
    report: &mut String,
    counts: &mut Counts,
    seed: u64,
    target: i16,
    limit: u32,
    update: bool,
    fixed: bool,
) {
    let id = counts.cases;
    let mut state = initial(seed);
    let weights = seeded_memory(seed, target);
    writeln!(
        report,
        "CASE,{id},seed={seed:016x},target={target},limit={limit},update={},fixed={}",
        u8::from(update),
        u8::from(fixed)
    )
    .unwrap();
    writeln!(report, "MEMORY,{id},weights={}", decimal(&weights)).unwrap();
    let mut previous_page = None;
    let mut previous_key = None;
    for depth in 1..=limit {
        let input = state;
        let selected = (state[0] >> 1) & 1;
        state[3] = if selected == 0 { 0 } else { 255 };
        let revision = u64::from(selected == 0 && update && depth >= 3);
        // Closed-form ANF polynomials of the frozen fixture's page definitions.
        let output = if selected == 1 {
            (state[0] ^ (state[0] >> 1)) & 1
        } else if revision == 1 {
            1 ^ ((state[0] >> 1) & 1)
        } else {
            1 ^ (state[0] & 1)
        };
        state[0] = (state[0] & !1) | output;
        let expert = state;
        let key = key(state, selected);
        let value = read(weights, key);
        let blocked = value.unsigned_abs() <= 128;
        if blocked {
            state[2] |= 2;
        } else {
            state[0] = (state[0] & !2) | (u64::from(value > 0) << 1);
            state[2] &= !2;
        }
        writeln!(report,"FRAME,{id},{depth},page={selected},revision={revision},input={},expert={},output={},key={},read={value},blocked={},read_macs=8,projected_coordinates=32",fields(input),fields(expert),fields(state),decimal(&key),u8::from(blocked)).unwrap();
        counts.cycles += 1;
        counts.key_changes += usize::from(previous_key.is_some_and(|old| old != key));
        counts.route_changes += usize::from(previous_page.is_some_and(|old| old != selected));
        previous_key = Some(key);
        previous_page = Some(selected);
        if !fixed && input == state && !blocked {
            break;
        }
    }
    counts.cases += 1;
}
fn reference() -> String {
    let mut report=String::from("A4_CLOSED_LOOP,schema=1,S=8,D=64,Q=8,source_id=77,raw=1000:1001,seed_updates=4,scope=synthetic_correctness\n");
    let mut counts = Counts::default();
    for seed in [0, 1, 0x4144434c, u64::MAX] {
        for target in [-2000, 0, 2000] {
            for limit in [1, 2, 4, 8] {
                for update in [false, true] {
                    case(&mut report, &mut counts, seed, target, limit, update, true);
                }
            }
        }
    }
    assert_eq!((counts.cases, counts.cycles), (96, 360));
    for seed in [0, 1, 0x4144434c, u64::MAX] {
        for target in [-2000, 0, 2000] {
            case(&mut report, &mut counts, seed, target, 8, false, false);
        }
    }
    assert_eq!(
        (
            counts.cases,
            counts.cycles,
            counts.key_changes,
            counts.route_changes
        ),
        (108, 456, 158, 28)
    );
    writeln!(report, "{SUMMARY}").unwrap();
    report.push_str("PROMOTION,BLOCKED,no_learning_speed_or_persistent_checkpoint_claim\n");
    report
}
fn verify(actual: &str) -> Result<(), String> {
    let expected = reference();
    if actual == expected {
        return Ok(());
    }
    let line = actual
        .lines()
        .zip(expected.lines())
        .position(|(a, b)| a != b)
        .map(|n| n + 1);
    Err(format!(
        "closed-loop mismatch at line {line:?}, actual bytes={}, expected bytes={}",
        actual.len(),
        expected.len()
    ))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("usage: verify-a4-closed-loop trajectory.csv".into());
    }
    let mut data = String::new();
    std::fs::File::open(&args[1])?
        .take(1_048_577)
        .read_to_string(&mut data)?;
    if data.len() > 1_048_576 {
        return Err("report exceeds 1 MiB cap".into());
    }
    verify(&data).map_err(std::io::Error::other)?;
    println!("A4_CLOSED_INDEPENDENT,cases=108,frames=456,seed_updates=432,key_changes=158,route_changes=28,mismatches=0");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_shape_and_signed_rounding_are_exact() {
        let report = reference();
        assert_eq!(report.lines().count(), 675);
        assert!(verify(&report).is_ok());
        for (n, d, result) in [
            (1, 2, 1),
            (-1, 2, -1),
            (3, 2, 2),
            (-3, 2, -2),
            (0, 2, 0),
            (4095, 4096, 1),
            (-4095, 4096, -1),
        ] {
            assert_eq!(rounded(n, d), result);
        }
    }
    #[test]
    fn changed_state_key_read_page_and_revision_are_rejected() {
        let report = reference();
        let original = report.lines().find(|l| l.starts_with("FRAME,")).unwrap();
        for column in [3, 4, 5, 6, 7, 8, 9, 10] {
            let mut fields = original.split(',').map(str::to_owned).collect::<Vec<_>>();
            let index = fields[column].find('=').unwrap() + 1;
            let old = fields[column].as_bytes()[index];
            fields[column].replace_range(index..index + 1, if old == b'0' { "1" } else { "0" });
            assert!(verify(&report.replacen(original, &fields.join(","), 1)).is_err());
        }
    }
    #[test]
    fn changed_seed_matrix_and_case_metadata_are_rejected() {
        let report = reference();
        let line = report.lines().find(|l| l.starts_with("MEMORY,")).unwrap();
        assert!(verify(&report.replacen(line, "MEMORY,0,weights=0:0:0:0:0:0:0:0", 1)).is_err());
        assert!(verify(&report.replacen("target=-2000", "target=-1999", 1)).is_err());
        assert!(verify(&report.replacen("source_id=77", "source_id=78", 1)).is_err());
    }
    #[test]
    fn missing_duplicate_extra_reordered_and_forged_success_are_rejected() {
        let report = reference();
        assert!(verify(&report[..report.len() - 1]).is_err());
        assert!(verify(&(report.clone() + "EXTRA\n")).is_err());
        let mut lines = report.lines().collect::<Vec<_>>();
        lines.swap(2, 3);
        assert!(verify(&(lines.join("\n") + "\n")).is_err());
        assert!(verify(&report.replacen("mismatches=0", "mismatches=1", 1)).is_err());
        assert!(verify(&report.replacen("FRAME,0,1,", "FRAME,0,2,", 1)).is_err());
    }
}
