//! Exact-head three-way original ggml Q3_K AVX2 C vs existing BPB3-K
//! PSHUFB and new single-u32-broadcast BPB3-K AVX2 kernel.
//! Same Q3_K original source weights; Q3->BPB3 conversion outside timing.
#![deny(unsafe_op_in_unsafe_fn)]

#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;
#[path = "bpb3k_pshufb_avx2.rs"]
#[allow(dead_code)]
mod old;
#[path = "bpb3k_broadcast32_avx2.rs"]
#[allow(dead_code)]
mod broadcast32;

use std::ffi::c_void;
use std::hint::black_box;
use std::time::Instant;

const ROWS: usize = 1024;
const ROUNDS: usize = 24;
const REPEATS: usize = 4;
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

struct Weights {
    original: [u8; 110],
    packed: [u8; 110],
}

fn original_weights() -> Vec<Weights> {
    (0..ROWS)
        .map(|case| {
            let mut original: [u8; 110] =
                std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8);
            original[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
            Weights {
                packed: codec::q3_to_bpb3k(&original).unwrap(),
                original,
            }
        })
        .collect()
}

fn q8() -> [u8; 292] {
    let mut a = [0u8; 292];
    a[..4].copy_from_slice(&1.0f32.to_le_bytes());
    for i in 0..256 {
        a[4 + i] = i as u8;
    }
    for subgroup in 0..16 {
        let sum: i16 = a[4 + subgroup * 16..4 + (subgroup + 1) * 16]
            .iter()
            .map(|&v| i16::from(v as i8))
            .sum();
        a[260 + subgroup * 2..262 + subgroup * 2].copy_from_slice(&sum.to_le_bytes());
    }
    a
}

fn stock(original: &[u8; 110], activation: &[u8; 292]) -> f32 {
    let mut result = 0.0f32;
    unsafe {
        ggml_vec_dot_q3_K_q8_K(
            256,
            &mut result,
            0,
            original.as_ptr().cast::<c_void>(),
            0,
            activation.as_ptr().cast::<c_void>(),
            0,
            1,
        );
    }
    result
}

fn measure(rows: &[Weights], activation: &[u8; 292], mode: usize) -> (u128, i64) {
    let start = Instant::now();
    let mut checksum = 0i64;
    for _ in 0..REPEATS {
        for row in rows {
            let result = match mode {
                0 => stock(black_box(&row.original), black_box(activation)) as i32,
                1 => old::dot_bpb3k_q8_pshufb(
                    black_box(&row.packed),
                    black_box(activation),
                )
                .unwrap()
                .weighted_integer,
                2 => broadcast32::dot_bpb3k_q8_broadcast32(
                    black_box(&row.packed),
                    black_box(activation),
                )
                .unwrap()
                .weighted_integer,
                _ => unreachable!(),
            };
            checksum += i64::from(black_box(result));
        }
    }
    (start.elapsed().as_nanos(), black_box(checksum))
}

fn median(times: &mut [u128]) -> u128 {
    times.sort_unstable();
    times[times.len() / 2]
}

fn main() {
    #[cfg(not(target_arch = "x86_64"))]
    panic!("STOP_BPB3K_BCAST_REQUIRES_X86_64");
    #[cfg(target_arch = "x86_64")]
    assert!(
        std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("f16c"),
        "STOP_BPB3K_BCAST_REQUIRES_HASWELL_ISA"
    );

    let rows = original_weights();
    let activation = q8();
    for (i, row) in rows.iter().enumerate() {
        let original = stock(&row.original, &activation);
        let old = old::dot_bpb3k_q8_pshufb(&row.packed, &activation)
            .unwrap()
            .weighted_integer as f32;
        let new = broadcast32::dot_bpb3k_q8_broadcast32(&row.packed, &activation)
            .unwrap()
            .weighted_integer as f32;
        assert!(
            original.is_finite() && old.is_finite() && new.is_finite(),
            "STOP_BPB3K_BCAST_NONFINITE row={i}"
        );
        assert_eq!(
            original.to_bits(),
            new.to_bits(),
            "STOP_BPB3K_BCAST_NATIVE_ORACLE row={i}"
        );
        assert_eq!(
            old.to_bits(),
            new.to_bits(),
            "STOP_BPB3K_BCAST_OLD_ORACLE row={i}"
        );
    }
    println!(
        "PASS_BPB3K_BCAST_THREEWAY_ORACLE rows=1024 exact_fp32_bits=true q8_full_signed_codes=true f16_f32_unit=true"
    );

    for count in [1usize, 32, 256, ROWS] {
        let batch = &rows[..count];
        for _ in 0..3 {
            let expected = measure(batch, &activation, 0).1;
            assert_eq!(expected, measure(batch, &activation, 1).1);
            assert_eq!(expected, measure(batch, &activation, 2).1);
        }
        let mut timings = [
            Vec::with_capacity(ROUNDS),
            Vec::with_capacity(ROUNDS),
            Vec::with_capacity(ROUNDS),
        ];
        for round in 0..ROUNDS {
            let mut result = [(0u128, 0i64); 3];
            for mode in ORDERS[round % ORDERS.len()] {
                result[mode] = measure(batch, &activation, mode);
            }
            for mode in 1..3 {
                assert_eq!(
                    result[0].1, result[mode].1,
                    "STOP_BPB3K_BCAST_CHECKSUM rows={count} round={round} mode={mode}"
                );
            }
            for mode in 0..3 {
                timings[mode].push(result[mode].0);
            }
            println!(
                "BPB3K_BCAST_RAW rows={} round={} stock_ns={} old_pshufb_ns={} broadcast32_ns={} checksum={}",
                count, round, result[0].0, result[1].0, result[2].0, result[0].1
            );
        }
        let [stock_ns, old_ns, new_ns] = timings.map(|mut t| median(&mut t));
        let dots = (count * REPEATS) as f64;
        println!(
            "BPB3K_BCAST_SUMMARY rows={} stock_ns_per_dot={:.2} old_pshufb_ns_per_dot={:.2} broadcast32_ns_per_dot={:.2} stock_over_bcast={:.4} old_over_bcast={:.4} scope=hosted_single_block",
            count,
            stock_ns as f64 / dots,
            old_ns as f64 / dots,
            new_ns as f64 / dots,
            stock_ns as f64 / new_ns as f64,
            old_ns as f64 / new_ns as f64
        );
    }
    println!("BPB3K_BCAST_VERDICT=DIAGNOSTIC no_Dell_or_model_tokens_claim");
}
