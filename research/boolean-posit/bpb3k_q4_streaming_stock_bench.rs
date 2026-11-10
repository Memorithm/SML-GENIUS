//! Large sequential weight-stream experiment. Real separate contiguous arrays
//! for each layout, not a stride that fetches unrelated candidate layouts.
//! Still synthetic, single-superblock, unit-scale FP32; not a real model.
#![deny(unsafe_op_in_unsafe_fn)]
#[path = "bpb3k_q4_pair228_avx2.rs"]
#[allow(dead_code)]
mod variants;
use std::ffi::c_void;
use std::hint::black_box;
use std::time::Instant;
use variants::*;

const ROWS: usize = 262_144;
const ROUNDS: usize = 12;
const ORDERS: [[usize; 4]; 8] = [
    [0, 1, 2, 3],
    [1, 2, 3, 0],
    [2, 3, 0, 1],
    [3, 0, 1, 2],
    [3, 2, 1, 0],
    [2, 1, 0, 3],
    [1, 0, 3, 2],
    [0, 3, 2, 1],
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
fn stock(w: &[u8; Q3_BYTES], a: &[u8; Q8_BYTES]) -> i32 {
    let mut out = 0f32;
    unsafe {
        ggml_vec_dot_q3_K_q8_K(
            256,
            &mut out,
            0,
            w.as_ptr().cast(),
            0,
            a.as_ptr().cast(),
            0,
            1,
        );
    }
    out as i32
}
fn fill(index: usize) -> [u8; Q3_BYTES] {
    // More unique blocks than simple mod256 byte arithmetic: deterministic
    // xorshift32 with lane-varying seed, no RNG dependency in the hot path.
    let mut x = (index as u32).wrapping_mul(0x9e37_79b9) ^ 0xa5a5_5a5a;
    let mut w = [0u8; Q3_BYTES];
    for byte in &mut w[..108] {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *byte = x as u8;
    }
    w[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
    w
}
fn activation() -> [u8; Q8_BYTES] {
    let mut a = [0u8; Q8_BYTES];
    a[..4].copy_from_slice(&1.0f32.to_le_bytes());
    for i in 0..256 {
        a[4 + i] = i as u8;
    }
    for g in 0..16 {
        let sum: i16 = a[4 + 16 * g..4 + 16 * (g + 1)]
            .iter()
            .map(|&x| i16::from(x as i8))
            .sum();
        a[260 + 2 * g..262 + 2 * g].copy_from_slice(&sum.to_le_bytes());
    }
    a
}
fn median(v: &mut [u128]) -> u128 {
    v.sort_unstable();
    v[v.len() / 2]
}

#[inline(never)]
fn bench(
    base: &[[u8; Q3_BYTES]],
    ref114: &[[u8; SCALE114_BYTES]],
    q146: &[[u8; Q4_BYTES]],
    q160: &[[u8; Q4_PADDED_BYTES]],
    a: &[u8; Q8_BYTES],
    mode: usize,
) -> (u128, i64) {
    let start = Instant::now();
    let mut checksum = 0i64;
    match mode {
        0 => {
            for w in base {
                checksum += i64::from(black_box(stock(black_box(w), black_box(a))));
            }
        }
        1 => {
            for w in ref114 {
                checksum += i64::from(black_box(
                    variants::previous::dot_bpb3k_q8_batch([black_box(&w[..])], black_box(a))
                        .unwrap()
                        .integer_dots[0],
                ));
            }
        }
        2 => {
            for w in q146 {
                checksum += i64::from(black_box(
                    dot_q4::<Q4_BYTES>(black_box(w), black_box(a))
                        .unwrap()
                        .integer,
                ));
            }
        }
        3 => {
            for w in q160 {
                checksum += i64::from(black_box(
                    dot_q4::<Q4_PADDED_BYTES>(black_box(w), black_box(a))
                        .unwrap()
                        .integer,
                ));
            }
        }
        _ => unreachable!(),
    }
    (start.elapsed().as_nanos(), black_box(checksum))
}

fn main() {
    #[cfg(not(target_arch = "x86_64"))]
    panic!("STOP_Q4_STREAM_REQUIRES_X86_64");
    #[cfg(target_arch = "x86_64")]
    assert!(
        std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("f16c"),
        "STOP_Q4_STREAM_ORIGINAL_C_ISA"
    );
    let mut base = Vec::with_capacity(ROWS);
    let mut ref114 = Vec::with_capacity(ROWS);
    let mut q146 = Vec::with_capacity(ROWS);
    let mut q160 = Vec::with_capacity(ROWS);
    for i in 0..ROWS {
        let source = fill(i);
        let bp = variants::bpb3k::q3_to_bpb3k(&source).unwrap();
        ref114.push(variants::previous::preencode_scales(&bp).unwrap());
        q146.push(transcode_q4::<Q4_BYTES>(&source).unwrap());
        q160.push(transcode_q4::<Q4_PADDED_BYTES>(&source).unwrap());
        base.push(source);
    }
    let input = activation();
    // Same exact original-Q3_K oracle, on both boundaries and many
    // spread-out positions; timed checksum covers every row.
    for i in (0..ROWS).step_by(1024).chain([ROWS - 1]) {
        let x = stock(&base[i], &input);
        assert_eq!(original_integer_dot(&base[i], &input), x);
        assert_eq!(
            variants::previous::dot_bpb3k_q8_batch([&ref114[i][..]], &input)
                .unwrap()
                .integer_dots[0],
            x
        );
        assert_eq!(dot_q4::<Q4_BYTES>(&q146[i], &input).unwrap().integer, x);
        assert_eq!(
            dot_q4::<Q4_PADDED_BYTES>(&q160[i], &input).unwrap().integer,
            x
        );
    }
    println!("PASS_Q4_STREAM_ORACLE spreadout_original_q3_c_unit_f32=true rows=262144");
    println!(
        "Q4_STREAM_BYTES rows={} stock={} ref114={} q146={} q160={} layout=contiguous_separate_buffers",
        ROWS,
        ROWS*Q3_BYTES, ROWS*SCALE114_BYTES,
        ROWS*Q4_BYTES, ROWS*Q4_PADDED_BYTES
    );

    for _ in 0..2 {
        let expected = bench(&base, &ref114, &q146, &q160, &input, 0).1;
        for mode in 1..4 {
            assert_eq!(
                expected,
                bench(&base, &ref114, &q146, &q160, &input, mode).1
            );
        }
    }
    let mut samples = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for round in 0..ROUNDS {
        let mut data = [(0u128, 0i64); 4];
        for mode in ORDERS[round % ORDERS.len()] {
            data[mode] = bench(&base, &ref114, &q146, &q160, &input, mode);
        }
        for mode in 1..4 {
            assert_eq!(data[0].1, data[mode].1);
        }
        for mode in 0..4 {
            samples[mode].push(data[mode].0);
        }
        println!(
            "Q4_STREAM_RAW rows={} round={} stock_ns={} ref114_ns={} q146_ns={} q160_ns={} checksum={}",
            ROWS,round,data[0].0,data[1].0,data[2].0,data[3].0,data[0].1
        );
    }
    let values = samples.map(|mut v| median(&mut v));
    println!(
        "Q4_STREAM_SUMMARY rows={} stock_ns_per_row={:.2} ref114_ns_per_row={:.2} q146_ns_per_row={:.2} q160_ns_per_row={:.2}",
        ROWS,
        values[0] as f64 / ROWS as f64,
        values[1] as f64 / ROWS as f64,
        values[2] as f64 / ROWS as f64,
        values[3] as f64 / ROWS as f64
    );
    println!("Q4_STREAM_VERDICT=DIAGNOSTIC no_Dell_model_token_speed_or_DRAM_counter_claim");
}
