//! Diagnostic-only row-amortization benchmark: legacy direct Q8 versus
//! prepared Q8 bitplanes. Not a ggml model/stock-Q3_K performance comparison.
//! The benchmark reports raw paired measurements; never a promotion gate.
#![forbid(unsafe_code)]
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;
#[path = "bpb3k_direct.rs"]
#[allow(dead_code)]
mod direct;

use std::hint::black_box;
use std::time::Instant;

const ROUNDS: usize = 16;
const REPEATS: usize = 4;
const MAX_ROWS: usize = 1024;

fn weights() -> Vec<[u8; codec::BYTES]> {
    (0..MAX_ROWS)
        .map(|row| {
            let mut original: [u8; codec::BYTES] =
                std::array::from_fn(|i| (i * 73 + i / 4 * 19 + row * 41) as u8);
            original[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
            codec::q3_to_bpb3k(&original).expect("valid original Q3_K block")
        })
        .collect()
}

fn activations() -> [u8; direct::Q8_K_BYTES] {
    let mut result = [0u8; direct::Q8_K_BYTES];
    result[..4].copy_from_slice(&1.0f32.to_bits().to_le_bytes());
    for i in 0..256 {
        let mut x = (i as u32).wrapping_mul(0x9e37_79b9);
        x ^= x >> 16;
        x ^= x << 13;
        result[4 + i] = x as u8;
    }
    result
}

fn measure(
    rows: &[[u8; codec::BYTES]],
    activation: &[u8; direct::Q8_K_BYTES],
    prepared: bool,
) -> (u128, i64) {
    let start = Instant::now();
    let mut checksum = 0i64;
    for _ in 0..REPEATS {
        if prepared {
            // Reprepare ONCE per activation batch, INSIDE the timing window.
            let mask = direct::prepare_q8_bitplanes(black_box(activation)).unwrap();
            for row in rows {
                checksum += i64::from(
                    direct::dot_bpb3k_q8_prepared(black_box(row), black_box(&mask))
                        .unwrap()
                        .weighted_integer,
                );
            }
        } else {
            for row in rows {
                checksum += i64::from(
                    direct::dot_bpb3k_q8(black_box(row), black_box(activation))
                        .unwrap()
                        .weighted_integer,
                );
            }
        }
    }
    black_box(checksum);
    (start.elapsed().as_nanos(), checksum)
}

fn median(values: &mut [u128]) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    let w = weights();
    let act = activations();
    let prepared = direct::prepare_q8_bitplanes(&act).unwrap();
    for row in &w {
        let original = direct::dot_bpb3k_q8(row, &act).unwrap();
        assert_eq!(
            original,
            direct::dot_bpb3k_q8_prepared(row, &prepared).unwrap(),
            "candidate/reference integer and scale bits must match"
        );
    }
    println!(
        "BPB3K_Q8_BENCH provenance=synthetic_cpu_only rows={} rounds={} repeats={} baseline=per_dot_q8_transpose candidate=prepared_once_per_batch stock_Q3_K=false physical_model=false",
        w.len(), ROUNDS, REPEATS
    );

    for count in [1usize, 32, 256, MAX_ROWS] {
        let rows = &w[..count];
        for _ in 0..3 {
            let (a, ac) = measure(rows, &act, false);
            let (b, bc) = measure(rows, &act, true);
            assert_eq!(ac, bc, "warmup checksums must be identical");
            black_box((a, b));
        }
        let mut legacy_ns = Vec::with_capacity(ROUNDS);
        let mut prepared_ns = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            // Alternating AB/BA order; all paired observations retained.
            let (first, second) = if round % 2 == 0 {
                (measure(rows, &act, false), measure(rows, &act, true))
            } else {
                let b = measure(rows, &act, true);
                let a = measure(rows, &act, false);
                (a, b)
            };
            assert_eq!(first.1, second.1, "different result at round={round}");
            legacy_ns.push(first.0);
            prepared_ns.push(second.0);
            println!(
                "BPB3K_Q8_RAW rows={} round={} baseline_ns={} prepared_ns={} checksum={}",
                count, round, first.0, second.0, first.1
            );
        }
        let legacy = median(&mut legacy_ns);
        let prepared = median(&mut prepared_ns);
        let dots = (count * REPEATS) as f64;
        println!(
            "BPB3K_Q8_SUMMARY rows={} baseline_ns_per_dot={:.2} prepared_ns_per_dot={:.2} descriptive_ratio={:.4} scope=standalone_cpu_microbench_only",
            count, legacy as f64 / dots, prepared as f64 / dots, legacy as f64 / prepared as f64
        );
    }
    println!("BPB3K_Q8_BENCH_VERDICT=DIAGNOSTIC no_stock_model_or_DRAM_claim");
}
