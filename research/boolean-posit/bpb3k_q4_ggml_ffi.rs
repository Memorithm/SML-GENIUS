//! Temporary Rust staticlib bridge for a pinned, *patched* llama.cpp CPU
//! GGUF research runtime. The GGUF Q3_K surrogate block is 146 bytes,
//! NOT the original 110-byte Q3_K. This must NEVER be linked to an
//! unmodified GGUF loader, production ggml or deployed service.
#![deny(unsafe_op_in_unsafe_fn)]
#[path = "bpb3k_q4_pair228_avx2.rs"]
#[allow(dead_code)]
mod q4;

use std::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
static FIRST_Q4_DOT: AtomicBool = AtomicBool::new(true);

fn f16_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = u32::from((bits >> 10) & 31);
    let fraction = u32::from(bits & 0x03ff);
    if exponent == 0 {
        if fraction == 0 {
            return f32::from_bits(sign);
        }
        // f16 subnormal has the exact value mantissa * 2^-24.
        return if sign == 0 {
            (fraction as f32) * f32::from_bits(0x3380_0000)
        } else {
            -(fraction as f32) * f32::from_bits(0x3380_0000)
        };
    }
    if exponent == 31 {
        return f32::from_bits(sign | 0x7f80_0000 | (fraction << 13));
    }
    f32::from_bits(sign | ((exponent + 112) << 23) | (fraction << 13))
}

/// C ABI mirrors ggml_vec_dot_q3_K_q8_K for one row. The Rust SIMD
/// implementation reads only Q4-146 and original Q8_K (292 bytes) and
/// never calls stock ggml Q3_K. The signed integer dot is exact;
/// its per-lane FP32 FMA and final reduction follow the original ggml
/// AVX2 multiblock accumulation order. Whole-model logits and tokens
/// remain subject to independent exact-head qualification.
#[no_mangle]
pub unsafe extern "C" fn mem_q4_146_vec_dot(
    n: c_int,
    result: *mut f32,
    _bs: usize,
    vx: *const c_void,
    _bx: usize,
    vy: *const c_void,
    _by: usize,
    nrc: c_int,
) {
    if n <= 0
        || n % 256 != 0
        || nrc != 1
        || result.is_null()
        || vx.is_null()
        || vy.is_null()
        || !std::is_x86_feature_detected!("avx2")
    {
        // No silent stock-Q3_K fallback or UB in an unexpected ggml path.
        std::process::abort();
    }
    if FIRST_Q4_DOT.swap(false, Ordering::SeqCst) {
        eprintln!("BPB3K_Q4_146_REAL_DOT_EXECUTED model_live=true payload_bytes=146 no_stock_fallback=true");
    }
    let blocks = (n / 256) as usize;
    // SAFETY: ggml's pinned Q3_K surrogate is patched to 146 B/block;
    // tensor data spans exactly n/256 blocks; Q8_K is unchanged.
    let weights = unsafe { std::slice::from_raw_parts(vx.cast::<u8>(), blocks * 146) };
    let activations = unsafe { std::slice::from_raw_parts(vy.cast::<u8>(), blocks * 292) };
    // Original optimized ggml Q3_K performs eight independent FP32 FMA
    // chains, each surviving across ALL superblocks, then hsum_float_8.
    // Summing each 256-weight block first changes the FP32 reduction graph
    // and can flip full-model tokens despite identical integer products.
    let mut accumulators = [0f32; 8];
    for i in 0..blocks {
        let w = &weights[146 * i..146 * (i + 1)];
        let a = &activations[292 * i..292 * (i + 1)];
        let dot = match q4::dot_q4_lanes::<146>(w, a) {
            Ok(dot) => dot,
            Err(_) => std::process::abort(),
        };
        let d = f32::from_bits(dot.q8_scale_f32_bits) * f16_to_f32(dot.global_scale_f16_bits);
        for (acc, integer) in accumulators.iter_mut().zip(dot.lanes) {
            *acc = d.mul_add(integer as f32, *acc);
        }
    }
    // Same association as original ggml hsum_float_8 (upper half + lower
    // half; then lane 0+2 and 1+3; then final lane 0+1).
    let halves = [
        accumulators[0] + accumulators[4],
        accumulators[1] + accumulators[5],
        accumulators[2] + accumulators[6],
        accumulators[3] + accumulators[7],
    ];
    let output = (halves[0] + halves[2]) + (halves[1] + halves[3]);
    // SAFETY: ggml provides a writable result float for nrc=1.
    unsafe { result.write_unaligned(output) };
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_f16_subnormal_zero_sign_and_round_trip() {
        assert_eq!(f16_to_f32(0x3c00).to_bits(), 1.0f32.to_bits());
        assert_eq!(f16_to_f32(0xbc00).to_bits(), (-1f32).to_bits());
        assert_eq!(f16_to_f32(0x0000).to_bits(), 0f32.to_bits());
        assert_eq!(f16_to_f32(0x8000).to_bits(), (-0f32).to_bits());
        assert_eq!(f16_to_f32(0x0001), 2f32.powi(-24));
        assert!(f16_to_f32(0x7e01).is_nan());
        assert_eq!(f16_to_f32(0x7c00), f32::INFINITY);
    }

    #[test]
    fn two_blocks_match_independent_q3_stock_lane_order() {
        // Regression for the real-model failure caused by summing each
        // Q3_K superblock before applying FP32 FMA to independent lanes.
        let mut weights = Vec::<u8>::new();
        let mut activations = Vec::<u8>::new();
        let mut reference_lanes = [0f32; 8];
        for block in 0..2usize {
            let mut original: [u8; 110] =
                std::array::from_fn(|i| (i.wrapping_mul(23) + 17 + block * 19) as u8);
            let f16_scale = [0x3c00u16, 0x3e00u16][block];
            original[108..110].copy_from_slice(&f16_scale.to_le_bytes());
            weights.extend_from_slice(&q4::transcode_q4::<146>(&original).unwrap());

            let mut q8 = [0u8; 292];
            let q8_scale = [0.03125f32, -0.0234375f32][block];
            q8[..4].copy_from_slice(&q8_scale.to_le_bytes());
            for i in 0..256 {
                q8[4 + i] = (((i * 37 + block * 29) % 221) as i16 - 110) as i8 as u8;
            }
            activations.extend_from_slice(&q8);

            let d = q8_scale * f16_to_f32(f16_scale);
            let mut lanes = [0i32; 8];
            for i in 0..256 {
                // Independently decode the ORIGINAL 110-byte Q3_K.
                let subgroup = i / 16;
                let scales = &original[96..108];
                let low = if subgroup < 8 {
                    scales[subgroup] & 15
                } else {
                    scales[subgroup - 8] >> 4
                };
                let high = (scales[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
                let signed_scale = i32::from(low | (high << 4)) - 32;
                let signed_q = i32::from(q4::original_q3_code(&original, i));
                let activation = i32::from(q8[4 + i] as i8);
                let lane = i % 16 / 4 + if i % 32 < 16 { 0 } else { 4 };
                lanes[lane] += signed_scale * signed_q * activation;
            }
            for i in 0..8 {
                reference_lanes[i] = d.mul_add(lanes[i] as f32, reference_lanes[i]);
            }
        }
        let half: [f32; 4] = std::array::from_fn(|i| reference_lanes[i] + reference_lanes[i + 4]);
        let expected = (half[0] + half[2]) + (half[1] + half[3]);
        let mut got = f32::NAN;
        unsafe {
            mem_q4_146_vec_dot(
                512,
                &mut got,
                0,
                weights.as_ptr().cast(),
                0,
                activations.as_ptr().cast(),
                0,
                1,
            );
        }
        assert_eq!(got.to_bits(), expected.to_bits());
    }
}
