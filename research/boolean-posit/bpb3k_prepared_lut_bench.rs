//! Standalone, single-Q3_K-superblock comparison against the unchanged pinned
//! ggml AVX2 C oracle using a 16 KiB activation lookup. This is NOT a full-model or physical Dell benchmark.
//! Requires liboriginal_q3.a extracted unchanged by run_bpb3k_stock_oracle.sh.
#![forbid(unsafe_op_in_unsafe_fn)]

#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;
#[path = "bpb3k_direct.rs"]
#[allow(dead_code)]
mod direct;

use std::ffi::c_void;
use std::hint::black_box;
use std::time::Instant;

const ROWS: usize = 1024;
const REPEATS: usize = 4;
const ROUNDS: usize = 16;

unsafe extern "C" {
    fn ggml_vec_dot_q3_K_q8_K(
        n: i32,
        s: *mut f32,
        bs: usize,
        x: *const c_void,
        bx: usize,
        y: *const c_void,
        by: usize,
        nrc: i32,
    );
}

struct Weight {
    original_q3: [u8; codec::BYTES],
    converted_bpb3: [u8; codec::BYTES],
}

fn weights() -> Vec<Weight> {
    (0..ROWS)
        .map(|row| {
            let mut original_q3: [u8; codec::BYTES] =
                std::array::from_fn(|i| (i * 73 + i / 4 * 19 + row * 41) as u8);
            original_q3[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
            let converted_bpb3 = codec::q3_to_bpb3k(&original_q3).unwrap();
            Weight {
                original_q3,
                converted_bpb3,
            }
        })
        .collect()
}

fn activation() -> [u8; direct::Q8_K_BYTES] {
    let mut result = [0u8; direct::Q8_K_BYTES];
    result[..4].copy_from_slice(&1.0f32.to_le_bytes());
    // All 256 signed i8 codes occur exactly once. Cached i16 Q8_K subgroup
    // sums are computed from the same bytes, as in a proper Q8_K block.
    for i in 0..256 {
        result[4 + i] = i as u8;
    }
    for group in 0..16 {
        let sum: i16 = result[4 + group * 16..4 + (group + 1) * 16]
            .iter()
            .map(|&code| i16::from(code as i8))
            .sum();
        result[260 + group * 2..262 + group * 2].copy_from_slice(&sum.to_le_bytes());
    }
    result
}

fn stock(row: &[u8; codec::BYTES], input: &[u8; direct::Q8_K_BYTES]) -> f32 {
    let mut result = 0.0f32;
    unsafe {
        ggml_vec_dot_q3_K_q8_K(
            256,
            &mut result,
            0,
            row.as_ptr().cast::<c_void>(),
            0,
            input.as_ptr().cast::<c_void>(),
            0,
            1,
        );
    }
    result
}

fn measure(rows: &[Weight], input: &[u8; direct::Q8_K_BYTES], candidate: bool) -> (u128, i64) {
    let start = Instant::now();
    let mut checksum = 0i64;
    for _ in 0..REPEATS {
        if candidate {
            // Timed: all 16 KiB of signed-Q8 subset-sum tables are rebuilt
            // ONCE per batch, not once per row. This cost is included.
            let plan = direct::prepare_q8_byte_tables(black_box(input)).unwrap();
            for weight in rows {
                let integer = direct::dot_bpb3k_q8_byte_tables(
                    black_box(&weight.converted_bpb3),
                    black_box(&plan),
                )
                .unwrap()
                .weighted_integer;
                checksum += i64::from(black_box(integer));
            }
        } else {
            for weight in rows {
                let value = stock(black_box(&weight.original_q3), black_box(input));
                // Scales are both exactly 1.0f32 and the integer result
                // magnitude is <= 4_194_304, so f32 remains exact here.
                checksum += i64::from(black_box(value as i32));
            }
        }
    }
    (start.elapsed().as_nanos(), black_box(checksum))
}

fn median(samples: &mut [u128]) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn main() {
    #[cfg(not(target_arch = "x86_64"))]
    panic!("STOP_BPB3K_STOCK_LUT_COMPARE_REQUIRES_X86_64");
    #[cfg(target_arch = "x86_64")]
    assert!(
        std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("f16c"),
        "STOP_BPB3K_STOCK_LUT_COMPARE_ISA"
    );
    let rows = weights();
    let input = activation();
    let plan = direct::prepare_q8_byte_tables(&input).unwrap();
    for (row, weight) in rows.iter().enumerate() {
        let reference = stock(&weight.original_q3, &input);
        let candidate = direct::dot_bpb3k_q8_byte_tables(&weight.converted_bpb3, &plan)
            .unwrap()
            .weighted_integer as f32;
        assert!(
            reference.is_finite() && candidate.is_finite(),
            "STOP_BPB3K_STOCK_LUT_COMPARE_NONFINITE row={row}"
        );
        assert_eq!(
            reference.to_bits(),
            candidate.to_bits(),
            "STOP_BPB3K_STOCK_LUT_COMPARE_MISMATCH row={row} stock={reference} candidate={candidate}"
        );
    }
    println!(
        "PASS_BPB3K_STOCK_Q8_LUT_SINGLE_BLOCK_ORACLE rows={} exact_fp32_bits=true unit_scales_only=true candidate=Q8_signed_byte_LUT_16KiB original=ggml_pinned_AVX2",
        rows.len()
    );
    println!(
        "BPB3K_STOCK_LUT_COMPARE_PROVENANCE generated_Q3_K=true original_Q8_signed_domain=all_256 source_bytes_per_block=110 stock_original_C=true model=false physical_Dell=false 1_thread=true"
    );
    for size in [1usize, 32, 256, ROWS] {
        let group = &rows[..size];
        for _ in 0..3 {
            let a = measure(group, &input, false);
            let b = measure(group, &input, true);
            assert_eq!(a.1, b.1, "STOP_BPB3K_STOCK_LUT_COMPARE_WARMUP");
        }
        let mut stock_times = Vec::with_capacity(ROUNDS);
        let mut prepared_times = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            let (stock_timed, candidate_timed) = if round % 2 == 0 {
                (measure(group, &input, false), measure(group, &input, true))
            } else {
                let b = measure(group, &input, true);
                let a = measure(group, &input, false);
                (a, b)
            };
            assert_eq!(stock_timed.1, candidate_timed.1);
            stock_times.push(stock_timed.0);
            prepared_times.push(candidate_timed.0);
            println!(
                "BPB3K_STOCK_LUT_COMPARE_RAW rows={} round={} stock_ns={} prepared_ns={} checksum={}",
                size, round, stock_timed.0, candidate_timed.0, stock_timed.1
            );
        }
        let t_stock = median(&mut stock_times);
        let t_prepared = median(&mut prepared_times);
        let operations = (size * REPEATS) as f64;
        println!(
            "BPB3K_STOCK_LUT_COMPARE_SUMMARY rows={} stock_ns_per_dot={:.2} prepared_ns_per_dot={:.2} descriptive_speedup={:.4} scope=one_block_single_thread_synthetic",
            size,
            t_stock as f64 / operations,
            t_prepared as f64 / operations,
            t_stock as f64 / t_prepared as f64
        );
    }
    println!("BPB3K_STOCK_LUT_COMPARE_VERDICT=DIAGNOSTIC no_model_tokens_or_Dell_speed_claim");
}
