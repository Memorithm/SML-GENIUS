//! Same-run original Q3_K optimized native C, 110 B BPB3-K reused-Q8
//! AVX2 tile and 130 B BPB3-K losslessly expanded 16 signed scales.
//! Both transcodes occur once before timing; extra 20 B per 256 weights
//! must be costed separately in complete-model load/bandwidth studies.
#![deny(unsafe_op_in_unsafe_fn)]
#[path = "bpb3k_batch4_avx2.rs"]
#[allow(dead_code)]
mod batch;
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;
#[path = "bpb3k_preencoded130_avx2.rs"]
#[allow(dead_code)]
mod preencoded;

use std::ffi::c_void;
use std::hint::black_box;
use std::time::Instant;

const MAX_ROWS: usize = 1024;
const REPEATS: usize = 4;
const ROUNDS: usize = 18;
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
    original: [u8; 110],
    packed: [u8; 110],
    preencoded: [u8; 130],
}

fn weights() -> Vec<Weight> {
    (0..MAX_ROWS)
        .map(|case| {
            let mut original: [u8; 110] =
                std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8);
            original[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
            let packed = codec::q3_to_bpb3k(&original).unwrap();
            let preencoded = preencoded::preencode_scales(&packed).unwrap();
            Weight { original, packed, preencoded }
        })
        .collect()
}

fn q8() -> [u8; batch::Q8_K_BYTES] {
    let mut input = [0u8; batch::Q8_K_BYTES];
    input[..4].copy_from_slice(&1.0f32.to_le_bytes());
    for (i, element) in input[4..260].iter_mut().enumerate() {
        *element = i as u8; // complete signed 8-bit code domain
    }
    for subgroup in 0..16 {
        let sum: i16 = input[4 + 16 * subgroup..4 + 16 * (subgroup + 1)]
            .iter()
            .map(|v| i16::from(*v as i8))
            .sum();
        input[260 + 2 * subgroup..262 + 2 * subgroup].copy_from_slice(&sum.to_le_bytes());
    }
    input
}

fn stock(w: &[u8; 110], a: &[u8; batch::Q8_K_BYTES]) -> f32 {
    let mut output = 0.0f32;
    // SAFETY: exact 110-byte original Q3_K, 292-byte Q8_K blocks, n=256,
    // nrc=1 and pinned upstream C AVX2 ABI. Scalar FP32 scale=1 exactly.
    unsafe {
        ggml_vec_dot_q3_K_q8_K(
            256,
            &mut output,
            0,
            w.as_ptr().cast(),
            0,
            a.as_ptr().cast(),
            0,
            1,
        );
    }
    output
}

#[inline(never)]
fn time_arm<const N: usize>(
    rows: &[Weight],
    a: &[u8; batch::Q8_K_BYTES],
    mode: usize,
) -> (u128, i64) {
    assert!([1, 2, 4].contains(&N));
    assert_eq!(rows.len() % N, 0);
    let begin = Instant::now();
    let mut checksum: i64 = 0;
    for _ in 0..REPEATS {
        for tile in rows.chunks_exact(N) {
            match mode {
                0 => {
                    for row in tile {
                        checksum += i64::from(black_box(stock(black_box(&row.original), black_box(a)) as i32));
                    }
                }
                1 => {
                    let refs: [&[u8]; N] = std::array::from_fn(|i| &tile[i].packed[..]);
                    let actual = batch::dot_bpb3k_q8_batch(black_box(refs), black_box(a)).unwrap();
                    for value in actual.integer_dots {
                        checksum += i64::from(black_box(value));
                    }
                }
                2 => {
                    let refs: [&[u8]; N] = std::array::from_fn(|i| &tile[i].preencoded[..]);
                    let actual = preencoded::dot_bpb3k_q8_batch(black_box(refs), black_box(a)).unwrap();
                    for value in actual.integer_dots {
                        checksum += i64::from(black_box(value));
                    }
                }
                _ => unreachable!(),
            }
        }
    }
    (begin.elapsed().as_nanos(), black_box(checksum))
}

fn time_eight(rows: &[Weight], a: &[u8; batch::Q8_K_BYTES], mode: usize) -> (u128, i64) {
    assert_eq!(rows.len() % 8, 0);
    // Specially note: this is an honest sequence of TWO four-row
    // microtiles, not an eight-way vector kernel.
    time_arm::<4>(rows, a, mode)
}

fn median(v: &mut [u128]) -> u128 {
    v.sort_unstable();
    v[v.len() / 2]
}

fn measure_batch<const N: usize>(all: &[Weight], a: &[u8; batch::Q8_K_BYTES], count: usize) {
    let rows = &all[..count];
    assert_eq!(count % N, 0);
    for _ in 0..3 {
        let baseline = time_arm::<N>(rows, a, 0).1;
        assert_eq!(baseline, time_arm::<N>(rows, a, 1).1);
        assert_eq!(baseline, time_arm::<N>(rows, a, 2).1);
    }
    let mut samples = [
        Vec::with_capacity(ROUNDS),
        Vec::with_capacity(ROUNDS),
        Vec::with_capacity(ROUNDS),
    ];
    for round in 0..ROUNDS {
        let mut result = [(0u128, 0i64); 3];
        for mode in ORDERS[round % ORDERS.len()] {
            result[mode] = if N == 4 && count % 8 == 0 && count >= 8 {
                // Entire 8-row-batch case still keeps explicit four-row tiles.
                time_eight(rows, a, mode)
            } else {
                time_arm::<N>(rows, a, mode)
            };
        }
        assert_eq!(result[0].1, result[1].1);
        assert_eq!(result[0].1, result[2].1);
        for mode in 0..3 {
            samples[mode].push(result[mode].0);
        }
        println!(
            "BPB3K_PREENCODE_RAW tile={} rows={} round={} stock_ns={} packed110_ns={} preencoded130_ns={} checksum={}",
            N, count, round, result[0].0, result[1].0, result[2].0, result[0].1
        );
    }
    let [stock_ns, old_ns, shared_ns] = samples.map(|mut s| median(&mut s));
    let count = count as f64 * REPEATS as f64;
    println!(
        "BPB3K_PREENCODE_SUMMARY tile={} rows={} stock_ns_per_row={:.2} packed110_ns_per_row={:.2} preencoded130_ns_per_row={:.2} packed_over_preencoded={:.4} stock_over_preencoded={:.4} scope=hosted_1superblock",
        N, rows.len(), stock_ns as f64 / count, old_ns as f64 / count,
        shared_ns as f64 / count, old_ns as f64 / shared_ns as f64,
        stock_ns as f64 / shared_ns as f64
    );
}

fn main() {
    #[cfg(not(target_arch = "x86_64"))]
    panic!("STOP_BPB3K_PREENCODE_X86_64_REQUIRED");
    #[cfg(target_arch = "x86_64")]
    assert!(
        std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("f16c")
            && std::is_x86_feature_detected!("fma"),
        "STOP_BPB3K_PREENCODE_PINNED_STOCK_ISA"
    );
    let all = weights();
    let a = q8();
    for (case, w) in all.iter().enumerate() {
        let original = stock(&w.original, &a);
        let old = batch::dot_bpb3k_q8_batch([&w.packed[..]], &a)
            .unwrap()
            .integer_dots[0] as f32;
        let batch_one = preencoded::dot_bpb3k_q8_batch([&w.preencoded[..]], &a)
            .unwrap()
            .integer_dots[0] as f32;
        assert!(
            original.is_finite() && old.is_finite() && batch_one.is_finite(),
            "STOP_BPB3K_PREENCODE_NONFINITE row={case}"
        );
        assert_eq!(original.to_bits(), batch_one.to_bits());
        assert_eq!(old.to_bits(), batch_one.to_bits());
    }
    println!("PASS_BPB3K_PREENCODE_ORACLE rows=1024 original_q3_and_bp3_identical_bits=true unit_f16_and_q8_scales=true all_256_signed_q8=true");
    for count in [32usize, 256, 1024] {
        measure_batch::<1>(&all, &a, count);
        measure_batch::<2>(&all, &a, count);
        measure_batch::<4>(&all, &a, count);
    }
    println!("BPB3K_PREENCODE_VERDICT=DIAGNOSTIC no_model_or_Dell_speed_claim");
}
