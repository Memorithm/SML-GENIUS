//! Same-machine, exact original Q3_K vs old BPB3-K PSHUFB vs Q8_K-bsums
//! fused correction. No ggml model integration or physical Dell result.
#![deny(unsafe_op_in_unsafe_fn)]

#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;
#[path = "bpb3k_pshufb_avx2.rs"]
#[allow(dead_code)]
mod old;
#[path = "bpb3k_bsums_avx2.rs"]
#[allow(dead_code)]
mod bsums;

use std::ffi::c_void;
use std::hint::black_box;
use std::time::Instant;

const ROWS: usize = 1024;
const REPEATS: usize = 4;
const ROUNDS: usize = 24;
const ORDERS: [[usize; 3]; 6] = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
];

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
    q3: [u8; 110],
    bpb3: [u8; 110],
}

fn original_q3_weights() -> Vec<Weight> {
    (0..ROWS)
        .map(|case| {
            let mut q3: [u8; 110] =
                std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8);
            q3[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
            Weight {
                bpb3: codec::q3_to_bpb3k(&q3).unwrap(),
                q3,
            }
        })
        .collect()
}

fn signed_q8() -> [u8; bsums::Q8_K_BYTES] {
    let mut q8 = [0u8; bsums::Q8_K_BYTES];
    q8[..4].copy_from_slice(&1.0f32.to_le_bytes());
    for (i, dst) in q8[4..260].iter_mut().enumerate() {
        *dst = i as u8;
    }
    for subgroup in 0..16 {
        let sum: i16 = q8[4 + subgroup * 16..4 + (subgroup + 1) * 16]
            .iter()
            .map(|&q| i16::from(q as i8))
            .sum();
        q8[260 + subgroup * 2..262 + subgroup * 2].copy_from_slice(&sum.to_le_bytes());
    }
    assert!(bsums::q8_k_cached_sums_valid(&q8));
    q8
}

fn stock(w: &[u8; 110], q8: &[u8; bsums::Q8_K_BYTES]) -> f32 {
    let mut output = 0.0f32;
    unsafe {
        ggml_vec_dot_q3_K_q8_K(
            256,
            &mut output,
            0,
            w.as_ptr().cast::<c_void>(),
            0,
            q8.as_ptr().cast::<c_void>(),
            0,
            1,
        );
    }
    output
}

fn measure(weights: &[Weight], q8: &[u8; bsums::Q8_K_BYTES], mode: usize) -> (u128, i64) {
    let start = Instant::now();
    let mut checksum = 0i64;
    for _ in 0..REPEATS {
        for row in weights {
            let v = match mode {
                0 => stock(black_box(&row.q3), black_box(q8)) as i32,
                1 => old::dot_bpb3k_q8_pshufb(black_box(&row.bpb3), black_box(q8))
                    .unwrap()
                    .weighted_integer,
                2 => bsums::dot_bpb3k_q8_bsums_avx2(black_box(&row.bpb3), black_box(q8))
                    .unwrap()
                    .weighted_integer,
                _ => unreachable!(),
            };
            checksum += i64::from(black_box(v));
        }
    }
    (start.elapsed().as_nanos(), black_box(checksum))
}

fn median(values: &mut [u128]) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    #[cfg(not(target_arch = "x86_64"))]
    panic!("STOP_BPB3K_BSUMS_REQUIRES_X86_64");
    #[cfg(target_arch = "x86_64")]
    assert!(
        std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("f16c")
            && std::is_x86_feature_detected!("fma"),
        "STOP_BPB3K_BSUMS_REQUIRES_ORIGINAL_GGML_AVX2_ISA"
    );

    let weights = original_q3_weights();
    let q8 = signed_q8();
    for (i, w) in weights.iter().enumerate() {
        let baseline = stock(&w.q3, &q8);
        let before = old::dot_bpb3k_q8_pshufb(&w.bpb3, &q8)
            .unwrap()
            .weighted_integer as f32;
        let after = bsums::dot_bpb3k_q8_bsums_avx2(&w.bpb3, &q8)
            .unwrap()
            .weighted_integer as f32;
        assert!(
            baseline.is_finite() && before.is_finite() && after.is_finite(),
            "STOP_BPB3K_BSUMS_FINITE_CASE row={i}"
        );
        assert_eq!(
            baseline.to_bits(),
            after.to_bits(),
            "STOP_BPB3K_BSUMS_ORIGINAL_FP32 row={i}"
        );
        assert_eq!(
            before.to_bits(),
            after.to_bits(),
            "STOP_BPB3K_BSUMS_OLD_PSHUFB row={i}"
        );
    }
    println!(
        "PASS_BPB3K_BSUMS_THREEWAY_ORACLE rows=1024 exact_bits=true valid_q8_bsums=true fp16_unit=true activation_f32_unit=true"
    );
    println!(
        "BPB3K_BSUMS_PROVENANCE original=ggml_f7591a6 original_signed_q8_all_256_codes=true original_q3_to_bpb3k_transcode_outside_timing=true no_Dell_or_model=true"
    );

    for count in [1usize, 32, 256, ROWS] {
        let rows = &weights[..count];
        for _ in 0..3 {
            let base = measure(rows, &q8, 0).1;
            assert_eq!(base, measure(rows, &q8, 1).1);
            assert_eq!(base, measure(rows, &q8, 2).1);
        }

        let mut samples = [
            Vec::with_capacity(ROUNDS),
            Vec::with_capacity(ROUNDS),
            Vec::with_capacity(ROUNDS),
        ];
        for round in 0..ROUNDS {
            let mut timings = [(0u128, 0i64); 3];
            for mode in ORDERS[round % ORDERS.len()] {
                timings[mode] = measure(rows, &q8, mode);
            }
            assert_eq!(
                timings[0].1, timings[1].1,
                "STOP_BPB3K_BSUMS_OLD_TIMED_SUM row_count={count} round={round}"
            );
            assert_eq!(
                timings[0].1, timings[2].1,
                "STOP_BPB3K_BSUMS_NEW_TIMED_SUM row_count={count} round={round}"
            );
            for mode in 0..3 {
                samples[mode].push(timings[mode].0);
            }
            println!(
                "BPB3K_BSUMS_RAW rows={} round={} stock_ns={} old_pshufb_ns={} cached_bsums_ns={} checksum={}",
                count, round, timings[0].0, timings[1].0, timings[2].0, timings[0].1
            );
        }
        let [stock_time, old_time, new_time] = samples.map(|mut v| median(&mut v));
        let dots = (count * REPEATS) as f64;
        println!(
            "BPB3K_BSUMS_SUMMARY rows={} stock_ns_per_dot={:.2} old_pshufb_ns_per_dot={:.2} cached_bsums_ns_per_dot={:.2} stock_over_bsums={:.4} old_over_bsums={:.4} scope=single_block_hosted",
            count,
            stock_time as f64 / dots,
            old_time as f64 / dots,
            new_time as f64 / dots,
            stock_time as f64 / new_time as f64,
            old_time as f64 / new_time as f64
        );
    }
    println!("BPB3K_BSUMS_VERDICT=DIAGNOSTIC model_and_Dell_not_qualified");
}
