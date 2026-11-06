//! PCEQ-5 native exact Boolean XOR microbenchmark, research only.
//!
//! Measures *prebuilt immutable operand* execution and output, not model
//! inference, producer creation costs, actual DDR events, or total PVP runtime.

use sml_core::pceq5::{xor_sparse_exact, CanonicalBitplane};
use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

const SEED: u64 = 0x5050_2026_1106_0005;
const PAIRS: usize = 32;
const WARMUPS: usize = 3;
const SAMPLES: usize = 9;

struct Pair {
    lhs: Arc<CanonicalBitplane>,
    rhs: Arc<CanonicalBitplane>,
}

fn word(seed: u64, index: usize) -> u64 {
    let mut x = seed.wrapping_add(index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x
}

fn build_pairs(bits: usize, same_percent: usize) -> Vec<Pair> {
    let word_count = bits.div_ceil(64);
    (0..PAIRS)
        .map(|index| {
            let data = (0..word_count)
                .map(|i| word(SEED ^ index as u64, i))
                .collect::<Vec<_>>();
            let lhs = Arc::new(CanonicalBitplane::from_words(bits, data).unwrap());
            let rhs = if (index * 37 + 19) % 100 < same_percent {
                Arc::clone(&lhs)
            } else {
                let other = (0..word_count)
                    .map(|i| word((SEED + 1) ^ index as u64, i))
                    .collect::<Vec<_>>();
                Arc::new(CanonicalBitplane::from_words(bits, other).unwrap())
            };
            Pair { lhs, rhs }
        })
        .collect()
}

#[derive(Clone, Copy)]
enum Variant {
    DenseReference,
    SparseIdentity,
}

impl Variant {
    fn label(self) -> &'static str {
        match self {
            Self::DenseReference => "dense_xor",
            Self::SparseIdentity => "sparse_identity",
        }
    }
}

fn execute(pairs: &[Pair], variant: Variant) -> (u64, u128, usize) {
    let mut popcount = 0_u64;
    let mut logical_bytes = 0_u128;
    let mut source_reads = 0_usize;
    for pair in pairs {
        match variant {
            Variant::DenseReference => {
                let dense = pair
                    .lhs
                    .words()
                    .iter()
                    .zip(pair.rhs.words())
                    .map(|(&a, &b)| a ^ b)
                    .collect::<Vec<_>>();
                popcount += dense.iter().map(|x| u64::from(x.count_ones())).sum::<u64>();
                logical_bytes += dense.len() as u128 * 8;
                source_reads += dense.len() * 2;
                black_box(dense);
            }
            Variant::SparseIdentity => {
                let sparse = xor_sparse_exact(&pair.lhs, &pair.rhs).unwrap();
                popcount += sparse
                    .payload()
                    .iter()
                    .map(|x| u64::from(x.count_ones()))
                    .sum::<u64>();
                logical_bytes += sparse.logical_bytes();
                source_reads += sparse.source_word_reads();
                black_box(sparse);
            }
        }
    }
    black_box((popcount, logical_bytes, source_reads))
}

fn preflight(pairs: &[Pair], bits: usize, same_percent: usize) {
    let mut observed_same = 0;
    for pair in pairs {
        let sparse = xor_sparse_exact(&pair.lhs, &pair.rhs).unwrap();
        let control = pair
            .lhs
            .words()
            .iter()
            .zip(pair.rhs.words())
            .map(|(&a, &b)| a ^ b)
            .collect::<Vec<_>>();
        assert_eq!(sparse.reconstruct_words(), control);
        if Arc::ptr_eq(&pair.lhs, &pair.rhs) {
            observed_same += 1;
            assert_eq!(sparse.source_word_reads(), 0);
            assert!(sparse.zero_by_identity());
        } else {
            assert_eq!(sparse.source_word_reads(), bits.div_ceil(64) * 2);
        }
    }
    let expected_same = (0..PAIRS)
        .filter(|&index| (index * 37 + 19) % 100 < same_percent)
        .count();
    assert_eq!(observed_same, expected_same);
    assert_eq!(
        execute(pairs, Variant::DenseReference).0,
        execute(pairs, Variant::SparseIdentity).0
    );
}

fn benchmark(bits: usize, same_percent: usize) {
    let pairs = build_pairs(bits, same_percent);
    preflight(&pairs, bits, same_percent);
    let variants = [Variant::DenseReference, Variant::SparseIdentity];
    let mut samples = [Vec::with_capacity(SAMPLES), Vec::with_capacity(SAMPLES)];
    let mut results = [(0_u64, 0_u128, 0_usize); 2];
    for trial in 0..(WARMUPS + SAMPLES) {
        for offset in 0..2 {
            let index = (trial + offset) % 2;
            let started = Instant::now();
            let output = execute(&pairs, variants[index]);
            let elapsed = started.elapsed().as_nanos();
            results[index] = output;
            if trial >= WARMUPS {
                samples[index].push(elapsed);
            }
        }
    }
    assert_eq!(results[0].0, results[1].0);
    for (index, variant) in variants.iter().enumerate() {
        samples[index].sort_unstable();
        let median_ns = samples[index][SAMPLES / 2];
        let p95_ns = samples[index][(SAMPLES * 95).div_ceil(100) - 1];
        println!(
            "PCEQ5_RESULT_V1,bits={bits},pairs={PAIRS},self_xor_threshold={same_percent},variant={},seed={SEED:#x},warmups={WARMUPS},samples={SAMPLES},median_ns={median_ns},p95_ns={p95_ns},logical_bytes={},algorithmic_source_word_reads={},popcount={},drambytes=UNMEASURED",
            variant.label(), results[index].1, results[index].2, results[index].0
        );
    }
}

fn main() {
    println!(
        "PCEQ5_PROTOCOL_V1,prebuilt_immutable_operands=true,drambytes=UNMEASURED,model_quality=NOT_EVALUATED"
    );
    for bits in [256, 4096, 65536] {
        for same_percent in [0, 25, 50, 100] {
            benchmark(bits, same_percent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_pvp_like_inputs_have_mathematically_exact_results() {
        for bits in [256, 4096, 65536] {
            for pct in [0, 25, 50, 100] {
                let pairs = build_pairs(bits, pct);
                preflight(&pairs, bits, pct);
                assert_eq!(
                    execute(&pairs, Variant::DenseReference).0,
                    execute(&pairs, Variant::SparseIdentity).0
                );
            }
        }
    }
}
