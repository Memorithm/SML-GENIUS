//! Direct original Q3_K vs Q8_K: compiler auto-vectorization experiment.
//! Research only: stock optimized C, Clang O3 SIMD, Clang O3 no-vector,
//! GCC O3 SIMD, and Rustc O3 with identical quantization and timing scope.
//! No machine/real-model conclusions and no model-mode changes.
#![deny(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::hint::black_box;
use std::time::Instant;

const WEIGHT_BYTES: usize = 110;
const Q8_BYTES: usize = 292;
const ROWS: usize = 1024;
const REPEATS: usize = 4;
const ROUNDS: usize = 20;
const MODES: usize = 5;

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
    fn mem_q3_clang_auto(q3: *const u8, q8: *const u8) -> i32;
    fn mem_q3_clang_novec(q3: *const u8, q8: *const u8) -> i32;
    fn mem_q3_gcc_auto(q3: *const u8, q8: *const u8) -> i32;
}

#[inline]
fn q3_quant(q3: &[u8; WEIGHT_BYTES], i: usize) -> i32 {
    let half = i / 128;
    let plane = (i % 128) / 32;
    let lane = i % 32;
    let two_bits = (q3[32 + half * 32 + lane] >> (2 * plane)) & 3;
    let sign = (q3[lane] >> (half * 4 + plane)) & 1;
    i32::from(two_bits) - if sign == 0 { 4 } else { 0 }
}

#[inline]
fn q3_subscale(q3: &[u8; WEIGHT_BYTES], subgroup: usize) -> i32 {
    let low = if subgroup < 8 {
        q3[96 + subgroup] & 15
    } else {
        q3[96 + subgroup - 8] >> 4
    };
    let high = (q3[104 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
    i32::from(low | (high << 4)) - 32
}

/// The direct original-Q3 scalar oracle reads *unpacked wire fields* on
/// every coefficient. It never calls the experimental candidate decoders.
fn oracle(q3: &[u8; WEIGHT_BYTES], q8: &[u8; Q8_BYTES]) -> i32 {
    let mut result = 0i32;
    for i in 0..256 {
        result += q3_quant(q3, i)
            * q3_subscale(q3, i / 16)
            * i32::from(q8[4 + i] as i8);
    }
    result
}

/// Rustc/LLVM -C opt-level=3 candidate. Like the three C implementations,
/// *the full Q3_K decode into local i16 codes occurs inside every dot*.
/// The final contiguous multiply/reduce loop is intentionally simple.
#[inline(never)]
fn rust_auto(q3: &[u8; WEIGHT_BYTES], q8: &[u8; Q8_BYTES]) -> i32 {
    let mut scaled = [0i16; 256];
    for (i, v) in scaled.iter_mut().enumerate() {
        *v = (q3_quant(q3, i) * q3_subscale(q3, i / 16)) as i16;
    }
    let mut result = 0i32;
    for (i, v) in scaled.iter().enumerate() {
        result += i32::from(*v) * i32::from(q8[4 + i] as i8);
    }
    result
}

fn weight_fixture(case: usize, fp16: u16) -> [u8; WEIGHT_BYTES] {
    let mut q3: [u8; WEIGHT_BYTES] =
        std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8);
    q3[108..110].copy_from_slice(&fp16.to_le_bytes());
    q3
}

fn activation_fixture(case: usize, pattern: usize, bits: u32) -> [u8; Q8_BYTES] {
    let mut q8 = [0u8; Q8_BYTES];
    q8[..4].copy_from_slice(&bits.to_le_bytes());
    for i in 0..256 {
        q8[4 + i] = match pattern % 8 {
            0 => 0,
            1 => 127,
            2 => 128,
            3 => 255,
            4 => if i & 1 == 0 { 127 } else { 128 },
            5 => if i & 1 == 0 { 1 } else { 255 },
            6 => i as u8,
            _ => {
                let mut x = (case as u32) ^ (i as u32).wrapping_mul(0x9e37_79b9);
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            }
        };
    }
    for subgroup in 0..16 {
        let signed_sum: i16 = q8[4 + subgroup * 16..4 + (subgroup + 1) * 16]
            .iter()
            .map(|&code| i16::from(code as i8))
            .sum();
        q8[260 + 2 * subgroup..262 + 2 * subgroup]
            .copy_from_slice(&signed_sum.to_le_bytes());
    }
    q8
}

#[inline]
fn compiled_c(mode: usize, weight: &[u8; WEIGHT_BYTES], q8: &[u8; Q8_BYTES]) -> i32 {
    // SAFETY: both buffers have exact Q3_K and Q8_K fixed lengths,
    // C prototypes take only readonly pointers and perform no writes.
    unsafe {
        match mode {
            1 => mem_q3_clang_auto(weight.as_ptr(), q8.as_ptr()),
            2 => mem_q3_clang_novec(weight.as_ptr(), q8.as_ptr()),
            3 => mem_q3_gcc_auto(weight.as_ptr(), q8.as_ptr()),
            _ => unreachable!(),
        }
    }
}

fn stock(q3: &[u8; WEIGHT_BYTES], q8: &[u8; Q8_BYTES]) -> f32 {
    let mut result = 0.0f32;
    // SAFETY: the frozen reference Q3_K and Q8_K ABIs require exactly
    // 110 and 292 bytes; n=256, nrc=1. Float scales are 1 in timed runs.
    unsafe {
        ggml_vec_dot_q3_K_q8_K(
            256,
            &mut result,
            0,
            q3.as_ptr().cast::<c_void>(),
            0,
            q8.as_ptr().cast::<c_void>(),
            0,
            1,
        );
    }
    result
}

fn tested_modes(q3: &[u8; WEIGHT_BYTES], q8: &[u8; Q8_BYTES]) -> [i32; 4] {
    [
        compiled_c(1, q3, q8),
        compiled_c(2, q3, q8),
        compiled_c(3, q3, q8),
        rust_auto(q3, q8),
    ]
}

fn crosscheck_signed_domain() {
    let mut tests = 0usize;
    for case in 0..96 {
        let weight = weight_fixture(case, [0x3c00, 0xbc00, 0x7e01, 0x8000][case % 4]);
        for pattern in 0..16 {
            let q8 = activation_fixture(case, pattern, [0x3f800000, 0x7fc12345][pattern % 2]);
            let expected = oracle(&weight, &q8);
            for (mode, actual) in tested_modes(&weight, &q8).iter().enumerate() {
                assert_eq!(*actual, expected, "STOP_AUTOVEC_Q3_ORACLE case={case} pattern={pattern} mode={mode}");
                tests += 1;
            }
        }
    }
    println!("PASS_BPB3K_AUTOVEC_ORACLE original_Q3_wire=true candidate_checks={tests} unit_scales_not_required_for_integer_dot=true");
}

fn measure(rows: &[[u8; WEIGHT_BYTES]], activation: &[u8; Q8_BYTES], mode: usize) -> (u128, i64) {
    let start = Instant::now();
    let mut checksum = 0i64;
    for _ in 0..REPEATS {
        for row in rows {
            let result = match mode {
                0 => stock(black_box(row), black_box(activation)) as i32,
                1..=3 => compiled_c(mode, black_box(row), black_box(activation)),
                4 => rust_auto(black_box(row), black_box(activation)),
                _ => unreachable!(),
            };
            checksum += i64::from(black_box(result));
        }
    }
    (start.elapsed().as_nanos(), black_box(checksum))
}

fn median(samples: &mut [u128]) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn order(round: usize) -> [usize; MODES] {
    // Two opposite orientations for every cyclic rotation, repeated twice.
    // Each mode occupies each of five timing positions exactly four times
    // per batch; all 20 paired rounds are retained without exclusions.
    let rotation = round % 5;
    let backwards = (round / 5) % 2 == 1;
    std::array::from_fn(|i| {
        let index = if backwards { 4 - i } else { i };
        (index + rotation) % MODES
    })
}

fn main() {
    #[cfg(not(target_arch = "x86_64"))]
    panic!("STOP_BPB3K_AUTOVEC_TARGET_NON_X86_64");
    #[cfg(target_arch = "x86_64")]
    assert!(
        std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("f16c")
            && std::is_x86_feature_detected!("fma"),
        "STOP_BPB3K_AUTOVEC_REFERENCE_REQUIRES_AVX2_F16C_FMA"
    );
    crosscheck_signed_domain();
    let q8 = activation_fixture(0, 6, 1.0f32.to_bits());
    let weights: Vec<[u8; WEIGHT_BYTES]> =
        (0..ROWS).map(|case| weight_fixture(case, 0x3c00)).collect();
    for (case, q3) in weights.iter().enumerate() {
        let stock_result = stock(q3, &q8);
        assert!(stock_result.is_finite(), "STOP_BPB3K_AUTOVEC_NONFINITE case={case}");
        let expected = oracle(q3, &q8) as f32;
        assert_eq!(stock_result.to_bits(), expected.to_bits(), "STOP_BPB3K_AUTOVEC_STOCK_PARITY case={case}");
        for (mode, result) in tested_modes(q3, &q8).iter().enumerate() {
            assert_eq!(*result, expected as i32, "STOP_BPB3K_AUTOVEC_COMPILER_PARITY case={case} mode={mode}");
        }
    }
    println!("PASS_BPB3K_AUTOVEC_STOCK rows=1024 unit_FP16_FP32_scale=true exact_bits=true signed_codes=256");
    println!("BPB3K_AUTOVEC_PROVENANCE source_Q3_original=true q8_signed=true decoded_weights_inside_timed_function=true stock=original_ggml_f7591a6 host=GitHub_not_Dell");
    for count in [1usize, 32, 256, ROWS] {
        let batch = &weights[..count];
        for _ in 0..3 {
            let expected = measure(batch, &q8, 0).1;
            for mode in 1..MODES {
                assert_eq!(measure(batch, &q8, mode).1, expected);
            }
        }
        let mut samples: [Vec<u128>; MODES] = std::array::from_fn(|_| Vec::with_capacity(ROUNDS));
        for round in 0..ROUNDS {
            let mut times = [(0u128, 0i64); MODES];
            for mode in order(round) {
                times[mode] = measure(batch, &q8, mode);
            }
            for mode in 1..MODES {
                assert_eq!(times[0].1, times[mode].1, "STOP_BPB3K_AUTOVEC_TIMED_PARITY rows={count} round={round} mode={mode}");
            }
            for mode in 0..MODES {
                samples[mode].push(times[mode].0);
            }
            println!(
                "BPB3K_AUTOVEC_RAW rows={} round={} stock_ns={} clang_auto_ns={} clang_novec_ns={} gcc_auto_ns={} rust_auto_ns={} checksum={}",
                count, round, times[0].0, times[1].0, times[2].0, times[3].0, times[4].0, times[0].1
            );
        }
        let medians = samples.map(|mut v| median(&mut v));
        let factor = (count * REPEATS) as f64;
        println!(
            "BPB3K_AUTOVEC_SUMMARY rows={} stock_ns_per_dot={:.2} clang_auto_ns_per_dot={:.2} clang_novec_ns_per_dot={:.2} gcc_auto_ns_per_dot={:.2} rust_auto_ns_per_dot={:.2} clang_novec_over_auto={:.4} stock_over_clang_auto={:.4} scope=one_block_single_thread_synthetic",
            count,
            medians[0] as f64 / factor,
            medians[1] as f64 / factor,
            medians[2] as f64 / factor,
            medians[3] as f64 / factor,
            medians[4] as f64 / factor,
            medians[2] as f64 / medians[1] as f64,
            medians[0] as f64 / medians[1] as f64,
        );
    }
    println!("BPB3K_AUTOVEC_VERDICT=DIAGNOSTIC no_Dell_or_model_token_claim");
}
