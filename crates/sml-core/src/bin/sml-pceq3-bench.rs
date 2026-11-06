//! PCEQ-3 controlled native CPU microbenchmark: same trusted source and bounds.
//!
//! Dense PCEQ-2 and sparse PCEQ-3 receive the same block recipe, quantization
//! envelopes and weight vector. Measures end-to-end production + quantization +
//! signed weighted-sum, including producer metadata callback. Does not measure
//! physical DDR bytes. See docs/research/PCEQ3_SPARSE_PAYLOAD.md.

use sml_core::pceq0::{quantize_streaming_bounded, ProducerBound};
use sml_core::pceq3::quantize_sparse_bounded;
use std::hint::black_box;
use std::time::Instant;

const SEED: u64 = 0x3ce0_2026_1106_0019;
const WARMUPS: usize = 3;
const ROUNDS: usize = 9;

#[derive(Clone, Copy)]
enum Variant {
    Dense,
    Sparse,
}

impl Variant {
    fn name(self) -> &'static str {
        match self {
            Self::Dense => "dense_pceq2",
            Self::Sparse => "sparse_pceq3",
        }
    }
}

fn active_block(block: usize, zero_pct: usize) -> bool {
    (block.wrapping_mul(73).wrapping_add(41) % 100) >= zero_pct
}

fn block_bound(block: usize, zero_pct: usize) -> ProducerBound {
    if active_block(block, zero_pct) {
        ProducerBound::MaxAbs(32_768)
    } else {
        ProducerBound::CertifiedZero
    }
}

fn value(index: usize, block_size: usize) -> i16 {
    if index % block_size == 0 {
        i16::MIN
    } else {
        let mut v = (index as u64).wrapping_add(SEED);
        v = v.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        v ^= v >> 29;
        v as i16
    }
}

fn execute(
    variant: Variant,
    len: usize,
    block: usize,
    zero_pct: usize,
    weights: &[i16],
) -> (i128, u128, usize) {
    match variant {
        Variant::Dense => {
            let e = quantize_streaming_bounded(
                len,
                block,
                |i| value(i, block),
                |b| block_bound(b, zero_pct),
            )
            .expect("valid dense bounds");
            let result = (
                e.weighted_sum(weights).expect("weight shape"),
                e.logical_bytes(),
                e.metadata().iter().filter(|m| m.active()).count(),
            );
            black_box(result)
        }
        Variant::Sparse => {
            let e = quantize_sparse_bounded(
                len,
                block,
                |i| value(i, block),
                |b| block_bound(b, zero_pct),
            )
            .expect("valid sparse bounds");
            let result = (
                e.weighted_sum(weights).expect("weight shape"),
                e.logical_bytes(),
                e.active_blocks(),
            );
            black_box(result)
        }
    }
}

fn preflight(len: usize, block: usize, zero_pct: usize, weights: &[i16]) {
    let mut dense_calls = 0;
    let d = quantize_streaming_bounded(
        len,
        block,
        |i| {
            dense_calls += 1;
            value(i, block)
        },
        |b| block_bound(b, zero_pct),
    )
    .unwrap();

    let mut sparse_calls = 0;
    let s = quantize_sparse_bounded(
        len,
        block,
        |i| {
            sparse_calls += 1;
            value(i, block)
        },
        |b| block_bound(b, zero_pct),
    )
    .unwrap();

    let expected_calls = (0..len)
        .filter(|&i| active_block(i / block, zero_pct))
        .count();
    assert_eq!(dense_calls, expected_calls);
    assert_eq!(sparse_calls, expected_calls);
    assert_eq!(s.reconstruct(), d.reconstruct());
    assert_eq!(s.weighted_sum(weights), d.weighted_sum(weights));
    assert_eq!(s.active_blocks(), d.metadata().iter().filter(|m| m.active()).count());
    assert!(s.codes().len() <= d.codes().len());
}

fn benchmark(len: usize, block: usize, zero_pct: usize) {
    let weights = (0..len)
        .map(|i| ((i as u32).wrapping_mul(17) % 41) as i16 - 20)
        .collect::<Vec<_>>();
    preflight(len, block, zero_pct, &weights);

    let variants = [Variant::Dense, Variant::Sparse];
    let mut samples = [Vec::with_capacity(ROUNDS), Vec::with_capacity(ROUNDS)];
    let mut results = [(0_i128, 0_u128, 0_usize); 2];
    for trial in 0..WARMUPS + ROUNDS {
        for offset in 0..2 {
            let slot = (trial + offset) % 2;
            let started = Instant::now();
            let result = execute(variants[slot], len, block, zero_pct, &weights);
            let duration = started.elapsed();
            results[slot] = result;
            if trial >= WARMUPS {
                samples[slot].push(duration.as_nanos());
            }
        }
    }
    assert_eq!(results[0].0, results[1].0);
    assert_eq!(results[0].2, results[1].2);
    for i in 0..2 {
        let v = variants[i];
        samples[i].sort_unstable();
        let median_ns = samples[i][ROUNDS / 2];
        let p95_index = (ROUNDS * 95).div_ceil(100) - 1;
        let p95_ns = samples[i][p95_index];
        println!(
            "PCEQ3_RESULT_V1,len={len},block={block},zero_percent={zero_pct},variant={},seed={SEED:#x},samples={ROUNDS},warmups={WARMUPS},median_ns={median_ns},p95_ns={p95_ns},logical_bytes={},active_blocks={},checksum={},drambytes=UNMEASURED",
            v.name(), results[i].1, results[i].2, results[i].0
        );
    }
}

fn main() {
    println!(
        "PCEQ3_PROTOCOL_V1,source=trusted_block_recipe,measured=producer_bound_quantization_consumer,drambytes=UNMEASURED,model_quality=NOT_EVALUATED"
    );
    for len in [4_096, 65_536, 262_144] {
        for block in [32, 64, 128] {
            for zero_pct in [0, 50, 90, 100] {
                benchmark(len, block, zero_pct);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn producer_and_sparse_dense_oracles_are_identical() {
        for len in [0, 1, 31, 32, 33, 63, 64, 65, 129, 1025] {
            for block in [32, 64, 128] {
                for zero_pct in [0, 50, 90, 100] {
                    let weights = vec![2_i16; len];
                    preflight(len, block, zero_pct, &weights);
                    let dense = execute(Variant::Dense, len, block, zero_pct, &weights);
                    let sparse = execute(Variant::Sparse, len, block, zero_pct, &weights);
                    assert_eq!(dense.0, sparse.0);
                    assert!(sparse.1 <= dense.1 + len.div_ceil(block).div_ceil(64) as u128 * 8);
                }
            }
        }
    }
}
