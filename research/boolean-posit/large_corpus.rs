//! BPB corpus qualification: resident, exact, resource-admitted, synthetic only.
//! Standalone Rust 1.89 binary; source resides with prototype.rs.
#[path = "prototype.rs"]
mod prototype;

use prototype::{encode_scale, Bpb3};
use std::hint::black_box;
use std::mem::size_of;
use std::time::Instant;

const WIDTH: usize = 64;
const PACKED_BYTES: usize = 26;
const DEFAULT_RESERVE_MIB: u64 = 16 * 1024;

#[derive(Debug, Clone, Copy)]
struct Settings {
    blocks: usize,
    repeats: usize,
    max_ram_percent: u64,
    reserve_mib: u64,
    seed: u64,
    verify_each_block: bool,
}

fn settings(args: impl Iterator<Item = String>) -> Result<Settings, String> {
    let mut conf = Settings {
        blocks: 1_048_576,
        repeats: 2,
        max_ram_percent: 50,
        reserve_mib: DEFAULT_RESERVE_MIB,
        seed: 0x6eb8_8c54_aa17_9301,
        verify_each_block: false,
    };
    let mut iter = args;
    while let Some(key) = iter.next() {
        if key == "--verify-each-block" {
            conf.verify_each_block = true;
            continue;
        }
        let value = iter
            .next()
            .ok_or_else(|| format!("missing value after {key}"))?;
        match key.as_str() {
            "--blocks" => conf.blocks = value.parse().map_err(|_| "invalid blocks")?,
            "--repeats" => conf.repeats = value.parse().map_err(|_| "invalid repeats")?,
            "--max-ram-percent" => {
                conf.max_ram_percent = value.parse().map_err(|_| "invalid percent")?;
            }
            "--reserve-mib" => conf.reserve_mib = value.parse().map_err(|_| "invalid reserve")?,
            "--seed" => conf.seed = value.parse().map_err(|_| "invalid seed")?,
            _ => return Err(format!("unknown argument: {key}")),
        }
    }
    if !(1..=1_000_000_000).contains(&conf.blocks) {
        return Err("blocks must be between 1 and 1,000,000,000".into());
    }
    if !(1..=6).contains(&conf.repeats) {
        return Err("repeats must be between 1 and 6".into());
    }
    if !(1..=70).contains(&conf.max_ram_percent) {
        return Err("max-ram-percent must be between 1 and 70".into());
    }
    if conf.reserve_mib < 256 {
        return Err("reserve-mib must be at least 256".into());
    }
    Ok(conf)
}

fn free_memory_from_proc() -> Result<u64, String> {
    let meminfo = std::fs::read_to_string("/proc/meminfo")
        .map_err(|e| format!("cannot read /proc/meminfo: {e}"))?;
    let value = meminfo
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemAvailable:")
                .and_then(|tail| tail.split_whitespace().next())
        })
        .ok_or("MemAvailable missing")?;
    value
        .parse::<u64>()
        .map_err(|_| "MemAvailable invalid".to_string())?
        .checked_mul(1024)
        .ok_or_else(|| "MemAvailable overflow".into())
}

fn cgroup_remaining_memory() -> Option<u64> {
    // Resolve the process-specific cgroup-v2 directory. Failure is unknown,
    // never interpreted as evidence of unlimited memory.
    let group = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let rel = group.lines().find_map(|line| line.strip_prefix("0::"))?;
    if !rel.starts_with('/') || rel.contains("..") {
        return None;
    }
    let dir = std::path::Path::new("/sys/fs/cgroup").join(rel.trim_start_matches('/'));
    let limit_text = std::fs::read_to_string(dir.join("memory.max")).ok()?;
    if limit_text.trim() == "max" {
        return None;
    }
    let limit = limit_text.trim().parse::<u64>().ok()?;
    let usage = std::fs::read_to_string(dir.join("memory.current"))
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(limit.saturating_sub(usage))
}

fn estimated_bytes(blocks: usize) -> Result<u64, String> {
    let per_block = size_of::<Bpb3>()
        .checked_add(size_of::<[i8; WIDTH]>())
        .and_then(|v| v.checked_add(size_of::<u64>()))
        .ok_or("element-size overflow")?;
    let whole = blocks
        .checked_mul(per_block)
        .ok_or("allocation-size overflow")?;
    u64::try_from(whole).map_err(|_| "allocation-size conversion overflow".into())
}

fn admission(conf: Settings) -> Result<(u64, u64), String> {
    let needed = estimated_bytes(conf.blocks)?;
    let host_available = free_memory_from_proc()?;
    let effective_available = cgroup_remaining_memory()
        .map(|v| v.min(host_available))
        .unwrap_or(host_available);
    let reserved = conf
        .reserve_mib
        .checked_mul(1024 * 1024)
        .ok_or("reserve overflow")?;
    let percent_budget =
        (u128::from(effective_available) * u128::from(conf.max_ram_percent) / 100) as u64;
    let budget = percent_budget.min(effective_available.saturating_sub(reserved));
    println!(
        "ADMISSION blocks={} coefficients={} resident_estimate_bytes={} available_bytes={} reserve_bytes={} budget_bytes={}",
        conf.blocks,
        (conf.blocks as u128) * WIDTH as u128,
        needed,
        effective_available,
        reserved,
        budget
    );
    if needed > budget {
        return Err("STOP_RESOURCE: requested resident corpus exceeds guarded budget".into());
    }
    Ok((needed, effective_available))
}

fn random(state: &mut u64) -> u64 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    (*state).wrapping_mul(2685821657736338717)
}

struct Corpus {
    compact: Vec<Bpb3>,
    int8: Vec<[i8; WIDTH]>,
    inputs: Vec<u64>,
}

fn generate(conf: Settings) -> Result<Corpus, String> {
    let mut corpus = Corpus {
        compact: Vec::new(),
        int8: Vec::new(),
        inputs: Vec::new(),
    };
    corpus
        .compact
        .try_reserve_exact(conf.blocks)
        .map_err(|e| format!("packed allocation refused: {e}"))?;
    corpus
        .int8
        .try_reserve_exact(conf.blocks)
        .map_err(|e| format!("dense allocation refused: {e}"))?;
    corpus
        .inputs
        .try_reserve_exact(conf.blocks)
        .map_err(|e| format!("input allocation refused: {e}"))?;
    let mut state = conf.seed;
    let scale = encode_scale(0).expect("known valid exponent").to_le_bytes();
    for _ in 0..conf.blocks {
        let lo = random(&mut state);
        let hi = random(&mut state);
        let sign = random(&mut state) & (lo | hi);
        let mut bytes = [0u8; PACKED_BYTES];
        bytes[..2].copy_from_slice(&scale);
        bytes[2..10].copy_from_slice(&sign.to_le_bytes());
        bytes[10..18].copy_from_slice(&lo.to_le_bytes());
        bytes[18..26].copy_from_slice(&hi.to_le_bytes());
        let compact = Bpb3::from_bytes(&bytes).expect("canonical generator");
        corpus.int8.push(compact.unpack());
        corpus.compact.push(compact);
        corpus.inputs.push(random(&mut state));
    }
    Ok(corpus)
}

fn dense_dot(weights: &[i8; WIDTH], input: u64) -> i32 {
    weights
        .iter()
        .enumerate()
        .map(|(i, &w)| {
            let sign = if (input >> i) & 1 == 0 { 1 } else { -1 };
            i32::from(w) * sign
        })
        .sum()
}

fn compact_sum(data: &Corpus) -> i128 {
    let mut sum = 0i128;
    for (block, &x) in data.compact.iter().zip(&data.inputs) {
        sum += i128::from(black_box(block.dot(black_box(x))));
    }
    black_box(sum)
}

fn dense_sum(data: &Corpus) -> i128 {
    let mut sum = 0i128;
    for (weights, &x) in data.int8.iter().zip(&data.inputs) {
        sum += i128::from(black_box(dense_dot(black_box(weights), black_box(x))));
    }
    black_box(sum)
}

/// Check every individual block against the independent scalar INT8 oracle.
/// Full comparison is mandatory; aggregate checksum parity alone can conceal
/// two or more offsetting block errors. The data is generated deterministically.
fn verify_block_oracle(data: &Corpus) -> Result<(i128, u64), String> {
    let len = data.compact.len();
    if data.int8.len() != len || data.inputs.len() != len {
        return Err(format!(
            "FAIL_BLOCK_LENGTH packed={} int8={} inputs={}",
            len,
            data.int8.len(),
            data.inputs.len()
        ));
    }

    let mut sum = 0i128;
    let mut digest = 0x6a09_e667_f3bc_c909u64;
    for index in 0..len {
        let input = black_box(data.inputs[index]);
        let observed = data.compact[index].dot(input);
        let oracle = dense_dot(&data.int8[index], input);
        if observed != oracle {
            return Err(format!(
                "FAIL_BLOCK_EXACTNESS index={index} compact={observed} int8={oracle}"
            ));
        }
        sum += i128::from(observed);
        let token = (index as u64)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ (observed as i64 as u64);
        digest = digest.rotate_left(9) ^ token.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    }
    Ok((black_box(sum), black_box(digest)))
}

fn run(conf: Settings) -> Result<(), String> {
    let (estimate, available) = admission(conf)?;
    let started = Instant::now();
    let corpus = generate(conf)?;
    println!(
        "GENERATED blocks={} coefficients={} elapsed_ms={} resident_estimate_bytes={} serialized_BPB_bytes={} INT8_payload_bytes={} available_at_admission_bytes={}",
        conf.blocks,
        (conf.blocks as u128) * WIDTH as u128,
        started.elapsed().as_millis(),
        estimate,
        (conf.blocks as u128) * PACKED_BYTES as u128,
        (conf.blocks as u128) * WIDTH as u128,
        available
    );

    let mut expected = None;
    for pass in 0..conf.repeats {
        let mut timings = [0u128; 2];
        let mut sums = [0i128; 2];
        // Alternate arm order to partially control clock/cache/order effects.
        for arm in if pass % 2 == 0 { [0, 1] } else { [1, 0] } {
            let now = Instant::now();
            sums[arm] = if arm == 0 {
                compact_sum(&corpus)
            } else {
                dense_sum(&corpus)
            };
            timings[arm] = now.elapsed().as_nanos();
        }
        if sums[0] != sums[1] {
            return Err(format!(
                "FAIL_EXACTNESS pass={pass} compact_sum={} dense_sum={}",
                sums[0], sums[1]
            ));
        }
        if let Some(prior) = expected {
            if sums[0] != prior {
                return Err(format!("FAIL_REPLAY pass={pass}"));
            }
        } else {
            expected = Some(sums[0]);
        }
        println!(
            "PAIRED pass={} compact_ns={} int8_ns={} checksum={} identical=true",
            pass, timings[0], timings[1], sums[0]
        );
    }
    if conf.verify_each_block {
        let started = Instant::now();
        let (per_block_sum, ordered_digest) = verify_block_oracle(&corpus)?;
        if expected != Some(per_block_sum) {
            return Err(format!(
                "FAIL_BLOCK_REPLAY aggregate={expected:?} block_sum={per_block_sum}"
            ));
        }
        println!(
            "PASS_BLOCK_ORACLE blocks={} coefficients={} mismatches=0 sum={} ordered_digest={:016x} elapsed_ms={} oracle=independent_dense_i8",
            conf.blocks,
            (conf.blocks as u128) * WIDTH as u128,
            per_block_sum,
            ordered_digest,
            started.elapsed().as_millis()
        );
    }
    println!(
        "PASS_CORPUS exact_full_scan=true blocks={} coefficients={} passes={} synthetic=true",
        conf.blocks,
        (conf.blocks as u128) * WIDTH as u128,
        conf.repeats
    );
    println!("NOTE: no DRAM traffic, energy, cache misses, real-data quality or SML model parity inferred");
    Ok(())
}

fn main() {
    match settings(std::env::args().skip(1)).and_then(run) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_budget_rejects_gigantic_request() {
        let c = Settings {
            blocks: 1_000_000_000,
            repeats: 1,
            max_ram_percent: 1,
            reserve_mib: 16 * 1024,
            seed: 5,
            verify_each_block: false,
        };
        assert!(admission(c).is_err());
    }

    #[test]
    fn thousand_blocks_exactness() {
        let c = Settings {
            blocks: 1024,
            repeats: 2,
            max_ram_percent: 70,
            reserve_mib: 256,
            seed: 42,
            verify_each_block: true,
        };
        run(c).unwrap();
    }

    #[test]
    fn block_oracle_detects_offsetting_errors() {
        let conf = Settings {
            blocks: 32,
            repeats: 1,
            max_ram_percent: 50,
            reserve_mib: 256,
            seed: 42,
            verify_each_block: true,
        };
        let mut corpus = generate(conf).unwrap();
        let baseline = dense_sum(&corpus);
        assert!(verify_block_oracle(&corpus).is_ok());

        // Corrupt two independent INT8 blocks by +1/-1 contribution.
        // An aggregate checksum still matches, but block 3 must fail.
        for (block_index, desired_contribution) in [(3, 1i8), (4, -1i8)] {
            let input = corpus.inputs[block_index];
            let weights = &mut corpus.int8[block_index];
            let position = (0..WIDTH)
                .find(|&i| {
                    let sign = if (input >> i) & 1 == 0 { 1 } else { -1 };
                    let delta = desired_contribution * sign;
                    (-3..=3).contains(&(weights[i] + delta))
                })
                .expect("a bounded mutable coefficient exists");
            let sign = if (input >> position) & 1 == 0 {
                1
            } else {
                -1
            };
            weights[position] += desired_contribution * sign;
        }
        assert_eq!(dense_sum(&corpus), baseline);
        let mismatch = verify_block_oracle(&corpus).unwrap_err();
        assert!(mismatch.contains("FAIL_BLOCK_EXACTNESS index=3"));
    }

    #[test]
    fn block_oracle_refuses_mismatched_lengths() {
        let conf = Settings {
            blocks: 32,
            repeats: 1,
            max_ram_percent: 50,
            reserve_mib: 256,
            seed: 42,
            verify_each_block: true,
        };
        let mut corpus = generate(conf).unwrap();
        corpus.inputs.pop();
        assert!(verify_block_oracle(&corpus)
            .unwrap_err()
            .contains("FAIL_BLOCK_LENGTH"));
    }

    #[test]
    fn reject_bad_settings() {
        let x = settings(["--blocks".into(), "0".into()].into_iter());
        assert!(x.is_err());
        let x = settings(["--max-ram-percent".into(), "100".into()].into_iter());
        assert!(x.is_err());
    }
}
