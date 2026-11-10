//! Research-only AVX2/PSHUFB-vs-original-Q3_K benchmark on identical data.
//! No native Dell, long-vector FMA parity, GGUF model or tokens/s claim.
#![deny(unsafe_op_in_unsafe_fn)]

#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;
#[path = "bpb3k_pshufb_avx2.rs"]
#[allow(dead_code)]
mod pshufb;

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
        q8[4 + i] = i as u8;
    }
    // Cached Q8_K subgroup sums are reconstructed consistently.
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

fn measure(w: &[Weight], q8: &[u8; pshufb::Q8_K_BYTES], simd: bool) -> (u128, i64) {
    let start = Instant::now();
    let mut checksum = 0i64;
    for _ in 0..REPEATS {
        for row in w {
            if simd {
                let v = pshufb::dot_bpb3k_q8_pshufb(black_box(&row.bpb3), black_box(q8))
                    .unwrap()
                    .weighted_integer;
                checksum += i64::from(black_box(v));
            } else {
                let v = stock(black_box(&row.q3), black_box(q8));
                checksum += i64::from(black_box(v as i32));
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
    panic!("STOP_PSHUFB_REQUIRES_X86_64");
    #[cfg(target_arch = "x86_64")]
    assert!(
        std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("f16c"),
        "STOP_PSHUFB_STOCK_AVX2_ISA"
    );
    let w = weights();
    let q8 = activation();
    for (i, row) in w.iter().enumerate() {
        let c = stock(&row.q3, &q8);
        let simd = pshufb::dot_bpb3k_q8_pshufb(&row.bpb3, &q8)
            .unwrap()
            .weighted_integer as f32;
        assert!(
            c.is_finite() && simd.is_finite(),
            "STOP_PSHUFB_NONFINITE row={i}"
        );
        assert_eq!(
            c.to_bits(),
            simd.to_bits(),
            "STOP_PSHUFB_ORACLE_MISMATCH row={i} original={c} candidate={simd}"
        );
    }
    println!(
        "PASS_BPB3K_PSHUFB_STOCK_ORACLE rows=1024 exact_bits=true fp16_unit=true q8_unit=true"
    );
    println!(
        "BPB3K_PSHUFB_PROVENANCE upstream=ggml-f7591a6 reference=original_AVX2_C candidate=rust_core_arch_AVX2 no_model_or_dell=true signed_q8_all_codes=true"
    );
    for size in [1usize, 32, 256, ROWS] {
        let batch = &w[..size];
        for _ in 0..3 {
            let a = measure(batch, &q8, false);
            let b = measure(batch, &q8, true);
            assert_eq!(a.1, b.1, "STOP_PSHUFB_WARMUP_PARITY");
        }
        let mut stock_times = Vec::with_capacity(ROUNDS);
        let mut simd_times = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            let (a, b) = if round % 2 == 0 {
                (measure(batch, &q8, false), measure(batch, &q8, true))
            } else {
                let b = measure(batch, &q8, true);
                let a = measure(batch, &q8, false);
                (a, b)
            };
            assert_eq!(a.1, b.1, "STOP_PSHUFB_CHECKSUM_MISMATCH round={round}");
            stock_times.push(a.0);
            simd_times.push(b.0);
            println!(
                "BPB3K_PSHUFB_RAW rows={} round={} stock_ns={} pshufb_ns={} checksum={}",
                size, round, a.0, b.0, a.1
            );
        }
        let a = median(&mut stock_times);
        let b = median(&mut simd_times);
        let dots = (size * REPEATS) as f64;
        println!(
            "BPB3K_PSHUFB_SUMMARY rows={} stock_ns_per_dot={:.2} pshufb_ns_per_dot={:.2} stock_over_pshufb={:.4} scope=hosted_single_block_synthetic",
            size, a as f64 / dots, b as f64 / dots, a as f64 / b as f64
        );
    }
    println!("BPB3K_PSHUFB_VERDICT=DIAGNOSTIC no_model_tokens_or_Dell_claim");
}
