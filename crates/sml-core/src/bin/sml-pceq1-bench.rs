//! PCEQ-1 controlled, bounded, synthetic end-to-end benchmark.
//!
//! Run with `cargo run --locked --release -p sml-core --bin sml-pceq1-bench`.
//! This is timing evidence only on the actual host, not DRAM traffic proof
//! and not model-level quality evidence. See PCEQ1_BENCHMARK_PROTOCOL.md.

use sml_core::pceq0::{quantize_fused, quantize_materialized, quantize_or_envelope, PceqI16};
use std::hint::black_box;
use std::time::Instant;

const SEED: u64 = 0x5174_2026_1106_f00d;
const ROUNDS: usize = 9;
const WARMUPS: usize = 3;

#[derive(Clone, Copy, Debug)]
enum Pattern {
    Balanced,
    Sparse,
    Outliers,
}

impl Pattern {
    fn name(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::Sparse => "sparse",
            Self::Outliers => "outliers",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Variant {
    Materialized,
    FusedExact,
    FusedOr,
}

impl Variant {
    fn name(self) -> &'static str {
        match self {
            Self::Materialized => "materialized",
            Self::FusedExact => "fused_exact",
            Self::FusedOr => "fused_or",
        }
    }
}

fn generated(index: usize, seed: u64, pattern: Pattern) -> i16 {
    let mut value = (index as u64)
        .wrapping_add(seed)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    match pattern {
        Pattern::Balanced => value as i16,
        Pattern::Sparse => {
            if value & 63 < 59 {
                0
            } else {
                ((value >> 10) % 253) as i16 - 126
            }
        }
        Pattern::Outliers => {
            if index % 64 == 0 {
                i16::MIN
            } else if index % 64 == 1 {
                i16::MAX
            } else {
                ((value >> 8) % 201) as i16 - 100
            }
        }
    }
}

fn execute(
    variant: Variant,
    length: usize,
    block: usize,
    pattern: Pattern,
    weights: &[i16],
) -> (i128, u128) {
    let encoded = match variant {
        Variant::Materialized => {
            let input: Vec<i16> = (0..length).map(|i| generated(i, SEED, pattern)).collect();
            black_box(&input);
            quantize_materialized(&input, block).expect("valid block size")
        }
        Variant::FusedExact => quantize_fused(length, block, |i| generated(i, SEED, pattern))
            .expect("valid block size"),
        Variant::FusedOr => quantize_or_envelope(length, block, |i| generated(i, SEED, pattern))
            .expect("valid block size"),
    };
    let sum = encoded.weighted_sum(weights).expect("matched weight size");
    let size = encoded.logical_bytes();
    black_box((sum, size));
    (sum, size)
}

fn reconstruction_error(encoded: &PceqI16, length: usize, pattern: Pattern) -> (u128, u32) {
    let decoded = encoded.reconstruct();
    let mut squared_error = 0_u128;
    let mut max_abs_error = 0_u32;
    for (i, actual) in decoded.into_iter().enumerate() {
        let delta = (i32::from(generated(i, SEED, pattern)) - actual).unsigned_abs();
        squared_error += u128::from(delta) * u128::from(delta);
        max_abs_error = max_abs_error.max(delta);
    }
    assert_eq!(encoded.codes().len(), length);
    (squared_error, max_abs_error)
}

fn benchmark(length: usize, block: usize, pattern: Pattern) {
    let weights: Vec<i16> = (0..length)
        .map(|i| generated(i, SEED ^ 0x1357_2468_0909_3333, Pattern::Balanced))
        .collect();

    let baseline_values: Vec<i16> = (0..length).map(|i| generated(i, SEED, pattern)).collect();
    let baseline = quantize_materialized(&baseline_values, block).unwrap();
    let fused = quantize_fused(length, block, |i| generated(i, SEED, pattern)).unwrap();
    let or = quantize_or_envelope(length, block, |i| generated(i, SEED, pattern)).unwrap();
    assert_eq!(baseline, fused, "exact fused/reference mismatch");
    let baseline_sum = baseline.weighted_sum(&weights).unwrap();
    let exact_sum = fused.weighted_sum(&weights).unwrap();
    assert_eq!(baseline_sum, exact_sum);
    let or_sum = or.weighted_sum(&weights).unwrap();

    let variants = [Variant::Materialized, Variant::FusedExact, Variant::FusedOr];
    let mut elapsed = [
        Vec::with_capacity(ROUNDS),
        Vec::with_capacity(ROUNDS),
        Vec::with_capacity(ROUNDS),
    ];

    // Rotate measurement order to limit systematic order bias.
    for trial in 0..(ROUNDS + WARMUPS) {
        for offset in 0..3 {
            let ix = (trial + offset) % 3;
            let variant = variants[ix];
            let start = Instant::now();
            let (sum, bytes) = execute(variant, length, block, pattern, &weights);
            let duration = start.elapsed();
            assert_eq!(bytes, baseline.logical_bytes());
            assert_eq!(sum, if ix == 2 { or_sum } else { baseline_sum });
            if trial >= WARMUPS {
                elapsed[ix].push(duration.as_nanos());
            }
        }
    }

    for (i, variant) in variants.iter().enumerate() {
        elapsed[i].sort_unstable();
        let median_ns = elapsed[i][ROUNDS / 2];
        let p95_ns = elapsed[i][(ROUNDS - 1) * 95 / 100];
        let candidate = if i == 2 { &or } else { &baseline };
        let (sum_sq_err, max_abs_err) = reconstruction_error(candidate, length, pattern);
        let mse = sum_sq_err as f64 / length as f64;
        // The FP32 baseline is NOT part of this i16 experiment.
        println!(
            "PCEQ1_RESULT_V1,pattern={},length={},block={},variant={},seed={:#x},warmups={},samples={},median_ns={},p95_ns={},logical_bytes={},materialized_i16_bytes={},mse={:.6},max_abs_error={},checksum={},drambytes=UNMEASURED",
            pattern.name(), length, block, variant.name(), SEED, WARMUPS, ROUNDS,
            median_ns, p95_ns, candidate.logical_bytes(), length as u128 * 2,
            mse, max_abs_err, if i == 2 { or_sum } else { baseline_sum }
        );
    }
}

fn main() {
    println!(
        "PCEQ1_PROTOCOL_V1,synthetic=true,actual_dram_bytes=unknown,model_quality=not_evaluated,metric=whole_producer_quantizer_consumer"
    );
    // Sizes are fixed prospectively; no runtime tuning or host-specific branch.
    for length in [4096, 65536, 262144] {
        for block in [32, 64, 128] {
            for pattern in [Pattern::Balanced, Pattern::Sparse, Pattern::Outliers] {
                benchmark(length, block, pattern);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entire_producer_consumer_matches_materialized_baseline() {
        for pattern in [Pattern::Balanced, Pattern::Sparse, Pattern::Outliers] {
            for block in [32, 64, 128] {
                for length in [1, 31, 32, 33, 127, 128, 129, 1001] {
                    let weights = vec![3_i16; length];
                    let a = execute(Variant::Materialized, length, block, pattern, &weights);
                    let b = execute(Variant::FusedExact, length, block, pattern, &weights);
                    assert_eq!(a, b);
                    let c = execute(Variant::FusedOr, length, block, pattern, &weights);
                    let codes =
                        quantize_or_envelope(length, block, |i| generated(i, SEED, pattern))
                            .unwrap();
                    assert_eq!(c.0, codes.weighted_sum(&weights).unwrap());
                }
            }
        }
    }
}
