//! Deterministic, paired, host-local latency measurements of native posit8 ISA.
//! No performance claim from one hosted CPU run; no scalar-vs-assembly mismatch.
#![deny(unsafe_op_in_unsafe_fn)]

use std::hint::black_box;
use std::time::Instant;

const SAMPLES: usize = 8192;
const ROUNDS: usize = 7;
const WARMUPS: usize = 2;
const MODES: usize = 3;

type NativeBinary = unsafe extern "C" fn(u8, u8) -> u8;

unsafe extern "C" {
    fn mem_posit8_asm_add(a: u8, b: u8) -> u8;
    fn mem_posit8_asm_cached_add(a: u8, b: u8) -> u8;
    fn mem_posit8_ce_add(a: u8, b: u8) -> u8;
    fn mem_posit8_asm_mul(a: u8, b: u8) -> u8;
    fn mem_posit8_asm_cached_mul(a: u8, b: u8) -> u8;
    fn mem_posit8_ce_mul(a: u8, b: u8) -> u8;
}

#[derive(Clone, Copy)]
struct Candidate {
    name: &'static str,
    run: NativeBinary,
}
fn all_modes(operation: &str) -> [Candidate; MODES] {
    if operation == "add" {
        [
            Candidate { name: "direct", run: mem_posit8_asm_add },
            Candidate { name: "cached", run: mem_posit8_asm_cached_add },
            Candidate { name: "complement", run: mem_posit8_ce_add },
        ]
    } else {
        [
            Candidate { name: "direct", run: mem_posit8_asm_mul },
            Candidate { name: "cached", run: mem_posit8_asm_cached_mul },
            Candidate { name: "complement", run: mem_posit8_ce_mul },
        ]
    }
}

fn inputs() -> Vec<(u8, u8)> {
    // The multiplier is odd: 0..65535 would visit each pair exactly once.
    // This finite prefix is a fixed deterministic workload, NOT training data.
    (0..SAMPLES)
        .map(|i| {
            let encoded = (i as u16).wrapping_mul(40503).wrapping_add(12345);
            (encoded as u8, (encoded >> 8) as u8)
        })
        .collect()
}
fn sampled_checksum(function: NativeBinary, pairs: &[(u8, u8)]) -> u64 {
    let mut hash = 0x1234_5678_9abc_def0u64;
    for (index, &(a, b)) in pairs.iter().enumerate() {
        // SAFETY: Symbol signatures and SysV ABI are exercised exhaustively
        // in the corresponding exact-head GitHub Actions assembly gate.
        let answer = unsafe { function(black_box(a), black_box(b)) };
        hash = hash.rotate_left(7)
            ^ (u64::from(black_box(answer)) + (index as u64).wrapping_mul(0x9e3779b97f4a7c15));
    }
    black_box(hash)
}
fn measured_sample(function: NativeBinary, pairs: &[(u8, u8)]) -> (f64, u64) {
    let begin = Instant::now();
    let hash = sampled_checksum(function, pairs);
    let ns = begin.elapsed().as_nanos() as f64 / pairs.len() as f64;
    (ns, hash)
}
fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}
fn bench_operation(operation: &str, pairs: &[(u8, u8)]) {
    let modes = all_modes(operation);
    let direct_checksum = sampled_checksum(modes[0].run, pairs);
    for candidate in modes {
        assert_eq!(
            sampled_checksum(candidate.run, pairs),
            direct_checksum,
            "sampled parity mismatch for {} {}",
            operation,
            candidate.name
        );
    }
    let mut results: [Vec<f64>; MODES] = std::array::from_fn(|_| Vec::with_capacity(ROUNDS));
    // Rotate candidate ordering by round to reduce systematic clock drift.
    for round in 0..(WARMUPS + ROUNDS) {
        for k in 0..MODES {
            let mode = (round + k) % MODES;
            let (ns, checksum) = measured_sample(modes[mode].run, pairs);
            assert_eq!(checksum, direct_checksum);
            if round >= WARMUPS {
                results[mode].push(ns);
            }
        }
    }
    let baseline = median(results[0].clone());
    for (mode, candidate) in modes.iter().enumerate() {
        let observed = median(results[mode].clone());
        assert!(observed.is_finite() && observed > 0.0);
        println!(
            "POSIT8_BENCH,operation={},mode={},samples={},rounds={},median_ns_per_pair={:.3},ratio_to_direct={:.5},checksum={:016x}",
            operation,
            candidate.name,
            SAMPLES,
            ROUNDS,
            observed,
            observed / baseline,
            direct_checksum,
        );
    }
}
fn main() {
    let workload = inputs();
    assert_eq!(workload.len(), SAMPLES);
    // An isolated hosted benchmark is exploratory, not a model-level claim.
    println!("POSIT8_BENCH_PROTOCOL v1 paired_sysv_x86_64 samples=8192 rounds=7 warmups=2");
    bench_operation("add", &workload);
    bench_operation("mul", &workload);
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workload_is_distinct_and_deterministic() {
        let workload = inputs();
        let mut seen = std::collections::HashSet::new();
        for &(a, b) in &workload {
            assert!(seen.insert((a, b)));
        }
        assert_eq!(seen.len(), SAMPLES);
        assert_eq!(workload[0], (57, 48));
    }

    #[test]
    fn all_native_backends_agree_on_workload() {
        let workload = inputs();
        for operation in ["add", "mul"] {
            let modes = all_modes(operation);
            let first = sampled_checksum(modes[0].run, &workload);
            for candidate in modes {
                assert_eq!(sampled_checksum(candidate.run, &workload), first);
            }
        }
    }

    #[test]
    fn deterministic_median_uses_middle_observation() {
        assert_eq!(median(vec![2.0, 3.0, 1.0]), 2.0);
        assert_eq!(median(vec![3.0, 1.0, 2.0, 5.0, 4.0]), 3.0);
    }
}
