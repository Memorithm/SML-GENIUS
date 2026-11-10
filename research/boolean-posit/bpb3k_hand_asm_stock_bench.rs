//! Shared exact-operand benchmark for original ggml AVX2 C, Rust intrinsic
//! PSHUFB and manually assigned-register x86-64 assembly PSHUFB+MADD.
//! Standalone single-superblock microbenchmark; no real-model or Dell claim.
#![deny(unsafe_op_in_unsafe_fn)]
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;
#[path = "bpb3k_hand_asm.rs"]
#[allow(dead_code)]
mod hand;
#[path = "bpb3k_pshufb_avx2.rs"]
#[allow(dead_code)]
mod pshufb;

use std::ffi::c_void;
use std::hint::black_box;
use std::time::Instant;

const ROWS: usize = 1024;
const REPEATS: usize = 4;
const ROUNDS: usize = 18; // 3 occurrences of all six execution orders.
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

fn weights() -> Vec<Weight> {
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

fn activation() -> [u8; pshufb::Q8_K_BYTES] {
    let mut q8 = [0u8; pshufb::Q8_K_BYTES];
    q8[..4].copy_from_slice(&1.0f32.to_le_bytes());
    for i in 0..256 {
        q8[4 + i] = i as u8; // Every signed 8-bit value exactly once.
    }
    for subgroup in 0..16 {
        let sum: i16 = q8[4 + subgroup * 16..4 + (subgroup + 1) * 16]
            .iter()
            .map(|&code| i16::from(code as i8))
            .sum();
        q8[260 + subgroup * 2..262 + subgroup * 2].copy_from_slice(&sum.to_le_bytes());
    }
    q8
}

fn stock(q3: &[u8; 110], q8: &[u8; pshufb::Q8_K_BYTES]) -> f32 {
    let mut out = 0.0f32;
    unsafe {
        ggml_vec_dot_q3_K_q8_K(
            256,
            &mut out,
            0,
            q3.as_ptr().cast(),
            0,
            q8.as_ptr().cast(),
            0,
            1,
        );
    }
    out
}

fn measure(w: &[Weight], q8: &[u8; pshufb::Q8_K_BYTES], mode: usize) -> (u128, i64) {
    let start = Instant::now();
    let mut checksum = 0i64;
    for _ in 0..REPEATS {
        for row in w {
            let value = match mode {
                0 => stock(black_box(&row.q3), black_box(q8)) as i32,
                1 => {
                    pshufb::dot_bpb3k_q8_pshufb(black_box(&row.bpb3), black_box(q8))
                        .unwrap()
                        .weighted_integer
                }
                2 => {
                    hand::dot_bpb3k_q8_hand_asm(black_box(&row.bpb3), black_box(q8))
                        .unwrap()
                        .weighted_integer
                }
                _ => unreachable!(),
            };
            checksum += i64::from(black_box(value));
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
    panic!("STOP_BPB3K_HAND_ASM_REQUIRES_X86_64");
    #[cfg(target_arch = "x86_64")]
    assert!(
        std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("f16c"),
        "STOP_BPB3K_HAND_ASM_STOCK_AVX2_ISA"
    );
    let w = weights();
    let q8 = activation();
    for (i, row) in w.iter().enumerate() {
        let original = stock(&row.q3, &q8);
        let intrinsic = pshufb::dot_bpb3k_q8_pshufb(&row.bpb3, &q8)
            .unwrap()
            .weighted_integer as f32;
        let assembly = hand::dot_bpb3k_q8_hand_asm(&row.bpb3, &q8)
            .unwrap()
            .weighted_integer as f32;
        assert!(
            original.is_finite() && intrinsic.is_finite() && assembly.is_finite(),
            "STOP_BPB3K_HAND_ASM_NONFINITE row={i}"
        );
        assert_eq!(
            original.to_bits(),
            assembly.to_bits(),
            "STOP_BPB3K_HAND_ASM_STOCK_ORACLE row={i}"
        );
        assert_eq!(
            intrinsic.to_bits(),
            assembly.to_bits(),
            "STOP_BPB3K_HAND_ASM_INTRINSIC_ORACLE row={i}"
        );
    }
    println!(
        "PASS_BPB3K_HAND_ASM_THREEWAY_ORACLE rows=1024 exact_fp32_bits=true fp16_unit=true q8_unit=true"
    );
    println!(
        "BPB3K_HAND_ASM_PROVENANCE upstream=ggml-f7591a6 reference=original_AVX2_C candidate=unrolled_SysV_AVX2 no_model_or_Dell=true all_256_signed_q8=true"
    );
    for count in [1usize, 32, 256, ROWS] {
        let rows = &w[..count];
        for _ in 0..3 {
            let a = measure(rows, &q8, 0);
            let b = measure(rows, &q8, 1);
            let c = measure(rows, &q8, 2);
            assert_eq!(a.1, b.1);
            assert_eq!(a.1, c.1);
        }
        let mut times = [
            Vec::with_capacity(ROUNDS),
            Vec::with_capacity(ROUNDS),
            Vec::with_capacity(ROUNDS),
        ];
        for round in 0..ROUNDS {
            let mut result = [(0u128, 0i64); 3];
            for mode in ORDERS[round % ORDERS.len()] {
                result[mode] = measure(rows, &q8, mode);
            }
            assert_eq!(
                result[0].1, result[1].1,
                "STOP_BPB3K_HAND_ASM_CHECKSUM_RUST round={round}"
            );
            assert_eq!(
                result[0].1, result[2].1,
                "STOP_BPB3K_HAND_ASM_CHECKSUM_ASM round={round}"
            );
            for mode in 0..3 {
                times[mode].push(result[mode].0);
            }
            println!(
                "BPB3K_HAND_ASM_RAW rows={} round={} stock_ns={} rust_ns={} asm_ns={} checksum={}",
                count, round, result[0].0, result[1].0, result[2].0, result[0].1
            );
        }
        let [stock_ns, rust_ns, asm_ns] = times.map(|mut x| median(&mut x));
        let dots = (count * REPEATS) as f64;
        println!(
            "BPB3K_HAND_ASM_SUMMARY rows={} stock_ns_per_dot={:.2} rust_ns_per_dot={:.2} asm_ns_per_dot={:.2} stock_over_asm={:.4} rust_over_asm={:.4} scope=synthetic_single_block",
            count,
            stock_ns as f64 / dots,
            rust_ns as f64 / dots,
            asm_ns as f64 / dots,
            stock_ns as f64 / asm_ns as f64,
            rust_ns as f64 / asm_ns as f64
        );
    }
    println!("BPB3K_HAND_ASM_VERDICT=DIAGNOSTIC no_model_tokens_or_Dell_claim");
}
