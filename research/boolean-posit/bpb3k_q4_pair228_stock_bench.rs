//! Same-host paired original GGML C AVX2 against 114/146/160 and 228 B.
//! Only original one/two superblock, FP16/f32 unit-scale, exact FP32 cases.
//! Conversion is outside all timed regions. No full model claims.
#![deny(unsafe_op_in_unsafe_fn)]
#[path = "bpb3k_q4_pair228_avx2.rs"]
#[allow(dead_code)]
mod variants;
use std::ffi::c_void;
use std::hint::black_box;
use std::time::Instant;
use variants::*;

const CASES: usize = 1024;
const ROUNDS: usize = 24;
const REPEATS: usize = 4;
const ORDER4: [[usize; 4]; 8] = [
    [0, 1, 2, 3],
    [1, 2, 3, 0],
    [2, 3, 0, 1],
    [3, 0, 1, 2],
    [3, 2, 1, 0],
    [2, 1, 0, 3],
    [1, 0, 3, 2],
    [0, 3, 2, 1],
];
const ORDER3: [[usize; 3]; 6] = [
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
struct Case {
    q3: [u8; Q3_BYTES],
    q4_146: [u8; Q4_BYTES],
    q4_160: [u8; Q4_PADDED_BYTES],
    scale114: [u8; SCALE114_BYTES],
}
struct Double {
    q3: [u8; 2 * Q3_BYTES],
    pair228: [u8; PAIR228_BYTES],
}

fn source(index: usize) -> [u8; Q3_BYTES] {
    let mut raw: [u8; Q3_BYTES] = std::array::from_fn(|i| (i * 73 + i / 4 * 19 + index * 41) as u8);
    raw[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
    raw
}
fn activation(seed: usize) -> [u8; Q8_BYTES] {
    let mut a = [0u8; Q8_BYTES];
    a[..4].copy_from_slice(&1.0f32.to_le_bytes());
    for i in 0..256 {
        a[4 + i] = (i + seed) as u8;
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
fn stock(q3: &[u8], q8: &[u8], n: usize) -> f32 {
    let mut out = 0.0f32;
    unsafe {
        ggml_vec_dot_q3_K_q8_K(
            n as i32,
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
fn prepare() -> (Vec<Case>, Vec<Double>, [u8; Q8_BYTES], [u8; 2 * Q8_BYTES]) {
    let mut rows = Vec::with_capacity(CASES);
    let mut pairs = Vec::with_capacity(CASES / 2);
    let a0 = activation(0);
    let a1 = activation(17);
    let mut both = [0u8; 2 * Q8_BYTES];
    both[..Q8_BYTES].copy_from_slice(&a0);
    both[Q8_BYTES..].copy_from_slice(&a1);
    for i in 0..CASES {
        let q3 = source(i);
        let q4_146 = transcode_q4::<Q4_BYTES>(&q3).unwrap();
        let q4_160 = transcode_q4::<Q4_PADDED_BYTES>(&q3).unwrap();
        let bp = variants::bpb3k::q3_to_bpb3k(&q3).unwrap();
        let scale114 = variants::previous::preencode_scales(&bp).unwrap();
        rows.push(Case {
            q3,
            q4_146,
            q4_160,
            scale114,
        });
        if i % 2 == 1 {
            let first = source(i - 1);
            let mut original = [0u8; 2 * Q3_BYTES];
            original[..Q3_BYTES].copy_from_slice(&first);
            original[Q3_BYTES..].copy_from_slice(&q3);
            pairs.push(Double {
                q3: original,
                pair228: transcode_pair228(&first, &q3).unwrap(),
            });
        }
    }
    (rows, pairs, a0, both)
}

#[inline(never)]
fn time_single(rows: &[Case], a: &[u8; Q8_BYTES], mode: usize) -> (u128, i64) {
    let start = Instant::now();
    let mut checksum = 0i64;
    for _ in 0..REPEATS {
        for row in rows {
            let val = match mode {
                0 => stock(black_box(&row.q3), black_box(a), 256) as i32,
                1 => {
                    variants::previous::dot_bpb3k_q8_batch(
                        [black_box(&row.scale114[..])],
                        black_box(a),
                    )
                    .unwrap()
                    .integer_dots[0]
                }
                2 => {
                    dot_q4::<Q4_BYTES>(black_box(&row.q4_146), black_box(a))
                        .unwrap()
                        .integer
                }
                3 => {
                    dot_q4::<Q4_PADDED_BYTES>(black_box(&row.q4_160), black_box(a))
                        .unwrap()
                        .integer
                }
                _ => unreachable!(),
            };
            checksum += i64::from(black_box(val));
        }
    }
    (start.elapsed().as_nanos(), black_box(checksum))
}
#[inline(never)]
fn time_pair(rows: &[Double], a: &[u8; 2 * Q8_BYTES], mode: usize) -> (u128, i64) {
    let start = Instant::now();
    let mut checksum = 0i64;
    for _ in 0..REPEATS {
        for row in rows {
            let value = match mode {
                0 => stock(black_box(&row.q3), black_box(a), 512) as i32,
                1 => {
                    let pa = &row.pair228[..114];
                    let pb = &row.pair228[114..];
                    let aa = &a[..292];
                    let ab = &a[292..];
                    let d0 = variants::previous::dot_bpb3k_q8_batch([black_box(pa)], black_box(aa))
                        .unwrap()
                        .integer_dots[0];
                    let d1 = variants::previous::dot_bpb3k_q8_batch([black_box(pb)], black_box(ab))
                        .unwrap()
                        .integer_dots[0];
                    d0 + d1
                }
                2 => {
                    let d = dot_pair228(black_box(&row.pair228), black_box(a)).unwrap();
                    d[0].integer + d[1].integer
                }
                _ => unreachable!(),
            };
            checksum += i64::from(black_box(value));
        }
    }
    (start.elapsed().as_nanos(), black_box(checksum))
}
fn median(v: &mut [u128]) -> u128 {
    v.sort_unstable();
    v[v.len() / 2]
}

fn main() {
    #[cfg(not(target_arch = "x86_64"))]
    panic!("STOP_Q4_PAIR228_REQUIRES_X86_64");
    #[cfg(target_arch = "x86_64")]
    assert!(
        std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("f16c"),
        "STOP_Q4_PAIR228_GGML_NATIVE_ISA"
    );

    let (rows, pairs, a, both) = prepare();
    for (i, row) in rows.iter().enumerate() {
        let ori = stock(&row.q3, &a, 256);
        let simple = original_integer_dot(&row.q3, &a) as f32;
        let q146 = dot_q4::<Q4_BYTES>(&row.q4_146, &a).unwrap().integer as f32;
        let q160 = dot_q4::<Q4_PADDED_BYTES>(&row.q4_160, &a).unwrap().integer as f32;
        let r114 = variants::previous::dot_bpb3k_q8_batch([&row.scale114[..]], &a)
            .unwrap()
            .integer_dots[0] as f32;
        assert_eq!(
            ori.to_bits(),
            simple.to_bits(),
            "original-Q3 parity row={i}"
        );
        assert_eq!(ori.to_bits(), q146.to_bits(), "q146 parity row={i}");
        assert_eq!(ori.to_bits(), q160.to_bits(), "q160 parity row={i}");
        assert_eq!(ori.to_bits(), r114.to_bits(), "114 parity row={i}");
    }
    println!(
        "PASS_Q4_SINGLE_ORACLE=1024 source_q3_native_c_exact_f32=true q114_q146_q160_exact=true"
    );
    for (i, row) in pairs.iter().enumerate() {
        let actual = stock(&row.q3, &both, 512);
        let vals = dot_pair228(&row.pair228, &both).unwrap();
        let integer = vals[0].integer + vals[1].integer;
        assert_eq!(
            actual.to_bits(),
            (integer as f32).to_bits(),
            "stock512 parity pair={i}"
        );
    }
    println!("PASS_PAIR228_ORACLE=512 source_q3_native_512_exact_f32=true two_q8_blocks=true");

    for count in [32, 256, 1024] {
        let batch = &rows[..count];
        for _ in 0..3 {
            let expected = time_single(batch, &a, 0).1;
            for mode in 1..4 {
                assert_eq!(expected, time_single(batch, &a, mode).1);
            }
        }
        let mut samples = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        for round in 0..ROUNDS {
            let mut timed = [(0u128, 0i64); 4];
            for mode in ORDER4[round % 8] {
                timed[mode] = time_single(batch, &a, mode);
            }
            for v in timed.iter().skip(1) {
                assert_eq!(timed[0].1, v.1);
            }
            for mode in 0..4 {
                samples[mode].push(timed[mode].0);
            }
            println!(
                "Q4_RAW rows={} round={} stock_ns={} q114_ns={} q146_ns={} q160_ns={} checksum={}",
                count, round, timed[0].0, timed[1].0, timed[2].0, timed[3].0, timed[0].1
            );
        }
        let times = samples.map(|mut v| median(&mut v));
        let count_n = (count * REPEATS) as f64;
        println!("Q4_SUMMARY rows={} stock_ns_per_row={:.2} q114_ns_per_row={:.2} q146_ns_per_row={:.2} q160_ns_per_row={:.2}",
            count,times[0] as f64/count_n,times[1] as f64/count_n,
            times[2] as f64/count_n,times[3] as f64/count_n);
    }
    for count in [16, 128, 512] {
        let batch = &pairs[..count];
        for _ in 0..3 {
            let expected = time_pair(batch, &both, 0).1;
            assert_eq!(expected, time_pair(batch, &both, 1).1);
            assert_eq!(expected, time_pair(batch, &both, 2).1);
        }
        let mut samples = [Vec::new(), Vec::new(), Vec::new()];
        for round in 0..ROUNDS {
            let mut timed = [(0u128, 0i64); 3];
            for mode in ORDER3[round % 6] {
                timed[mode] = time_pair(batch, &both, mode);
            }
            for v in timed.iter().skip(1) {
                assert_eq!(timed[0].1, v.1);
            }
            for mode in 0..3 {
                samples[mode].push(timed[mode].0);
            }
            println!("PAIR228_RAW pairs={} round={} stock512_ns={} two_q114_ns={} pair228_ns={} checksum={}",
                count,round,timed[0].0,timed[1].0,timed[2].0,timed[0].1);
        }
        let times = samples.map(|mut v| median(&mut v));
        let pairs_n = (count * REPEATS) as f64;
        println!("PAIR228_SUMMARY pairs={} stock512_ns_per_pair={:.2} two_q114_ns_per_pair={:.2} pair228_ns_per_pair={:.2}",
            count,times[0] as f64/pairs_n,times[1] as f64/pairs_n,times[2] as f64/pairs_n);
    }
    println!("Q4_PAIR228_VERDICT=DIAGNOSTIC no_model_Dell_or_multiblock_arbitrary_fp32_claim");
}
