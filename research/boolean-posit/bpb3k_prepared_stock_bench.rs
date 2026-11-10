//! Paired standalone direct-dot comparison against the ORIGINAL pinned
//! ggml AVX2 Q3_K function, extracted unchanged into a diagnostic C library.
//! This is not a SmolLM2 tokens/second or DRAM result.
#![deny(unsafe_op_in_unsafe_fn)]

#[path = "bpb3k_direct.rs"]
#[allow(dead_code)]
mod direct;
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod wire;

use std::ffi::c_void;
use std::hint::black_box;
use std::time::Instant;

extern "C" {
    fn ggml_vec_dot_q3_K_q8_K(
        n: i32,
        s: *mut f32,
        bs: usize,
        vx: *const c_void,
        bx: usize,
        vy: *const c_void,
        by: usize,
        nrc: i32,
    );
}

const MAX_ROWS: usize = 1024;
const REPEATS: usize = 4;
const ROUNDS: usize = 16;

struct Fixtures {
    q3: Vec<[u8; wire::BYTES]>,
    bpb: Vec<[u8; wire::BYTES]>,
    q8: [u8; direct::Q8_K_BYTES],
}

fn fixtures() -> Fixtures {
    let mut q8 = [0u8; direct::Q8_K_BYTES];
    q8[..4].copy_from_slice(&1.0f32.to_le_bytes());
    for (i, x) in q8[4..260].iter_mut().enumerate() {
        let mut state = (i as u32).wrapping_mul(0x9e37_79b9);
        state ^= state >> 16;
        state ^= state << 13;
        *x = state as u8;
    }
    let mut q3 = Vec::with_capacity(MAX_ROWS);
    let mut bpb = Vec::with_capacity(MAX_ROWS);
    for row in 0..MAX_ROWS {
        let mut src: [u8; wire::BYTES] =
            std::array::from_fn(|i| (i * 73 + i / 4 * 19 + row * 41) as u8);
        src[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
        bpb.push(wire::q3_to_bpb3k(&src).unwrap());
        q3.push(src);
    }
    Fixtures { q3, bpb, q8 }
}

/// Only diagnostic C FFI, never linked into the BPB3-K model runtime.
/// One original Q3_K block plus one Q8_K block; nrc=1.
fn stock_dot(q3: &[u8; wire::BYTES], q8: &[u8; direct::Q8_K_BYTES]) -> f32 {
    let mut result = 0f32;
    unsafe {
        ggml_vec_dot_q3_K_q8_K(
            256,
            &mut result,
            0,
            black_box(q3.as_ptr()).cast(),
            0,
            black_box(q8.as_ptr()).cast(),
            0,
            1,
        );
    }
    result
}

fn evaluate(
    f: &Fixtures,
    rows: usize,
    prepared: bool,
) -> (u128, i64) {
    let start = Instant::now();
    let mut checksum = 0i64;
    for _ in 0..REPEATS {
        if prepared {
            let plan = direct::prepare_q8_bitplanes(black_box(&f.q8)).unwrap();
            for row in f.bpb.iter().take(rows) {
                checksum += i64::from(
                    direct::dot_bpb3k_q8_prepared(black_box(row), black_box(&plan))
                        .unwrap()
                        .weighted_integer,
                );
            }
        } else {
            for q3 in f.q3.iter().take(rows) {
                checksum += i64::from(stock_dot(q3, &f.q8) as i32);
            }
        }
    }
    black_box(checksum);
    (start.elapsed().as_nanos(), checksum)
}

fn median(v: &mut [u128]) -> u128 {
    v.sort_unstable();
    v[v.len() / 2]
}

fn main() {
    assert!(
        cfg!(target_arch = "x86_64")
            && std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("f16c"),
        "STOP_PINNED_STOCK_AVX2_CAPABILITY"
    );
    let f = fixtures();
    let plan = direct::prepare_q8_bitplanes(&f.q8).unwrap();
    for (row, (q3, bpb)) in f.q3.iter().zip(&f.bpb).enumerate() {
        let stock = stock_dot(q3, &f.q8);
        let integer = direct::dot_bpb3k_q8_prepared(bpb, &plan)
            .unwrap()
            .weighted_integer;
        // Both scales are exactly 1.0; each single-block integer is exactly
        // representable as f32, including the upstream FMA/AVX2 reduction.
        assert!(
            stock.is_finite() && stock.fract() == 0.0,
            "STOP_STOCK_NOT_EXACT_INTEGER row={row}"
        );
        assert_eq!(stock, integer as f32, "STOP_STOCK_ORACLE_MISMATCH row={row}");
    }
    println!(
        "BPB3K_STOCK_CONTROL oracle=unchanged_pinned_upstream_C_AVX2 one_block=true compared={} exact=true stock_fp32_scaling=1.0",
        f.q3.len()
    );
    for rows in [1usize, 32, 256, MAX_ROWS] {
        for _ in 0..3 {
            let a = evaluate(&f, rows, false);
            let b = evaluate(&f, rows, true);
            assert_eq!(a.1, b.1, "STOP_STOCK_WARMUP_MISMATCH");
            black_box((a.0, b.0));
        }
        let mut stock_ns = Vec::with_capacity(ROUNDS);
        let mut prepared_ns = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            let (stock, prepared) = if round % 2 == 0 {
                (evaluate(&f, rows, false), evaluate(&f, rows, true))
            } else {
                let candidate = evaluate(&f, rows, true);
                let control = evaluate(&f, rows, false);
                (control, candidate)
            };
            assert_eq!(stock.1, prepared.1, "STOP_STOCK_CHECKSUM_MISMATCH");
            stock_ns.push(stock.0);
            prepared_ns.push(prepared.0);
            println!(
                "BPB3K_STOCK_RAW rows={} round={} stock_ns={} prepared_ns={} checksum={}",
                rows, round, stock.0, prepared.0, stock.1
            );
        }
        let stock = median(&mut stock_ns);
        let prepared = median(&mut prepared_ns);
        let dots = (rows * REPEATS) as f64;
        println!(
            "BPB3K_STOCK_SUMMARY rows={} stock_ns_per_dot={:.2} prepared_ns_per_dot={:.2} ratio_stock_over_prepared={:.4} scope=synthetic_pinned_AVX2_microbenchmark_only",
            rows, stock as f64 / dots, prepared as f64 / dots, stock as f64 / prepared as f64
        );
    }
    println!("BPB3K_STOCK_VERDICT=DIAGNOSTIC no_model_throughput_or_DRAM_claim");
}
