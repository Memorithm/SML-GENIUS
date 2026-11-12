//! Experimental native Q5-178 × Q8_K ggml one-row ABI bridge.
//! Must be linked ONLY into the exact-source-pinned matching patched runtime.
//! Never link to ordinary ggml or reinterpret standard Q3_K/Q5_K blocks.
#![deny(unsafe_op_in_unsafe_fn)]
#[path = "bpb3k_q5_178_tiled_avx2.rs"]
#[allow(dead_code)]
mod q5;

use std::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
static FIRST_Q5_DOT: AtomicBool = AtomicBool::new(true);

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
/// implementation reads only Q5-178 and original Q8_K (292 bytes) and
/// never calls stock ggml Q3_K. The signed integer dot is exact;
/// its per-lane FP32 FMA and final reduction follow the original ggml
/// AVX2 multiblock accumulation order. Whole-model logits and tokens
/// remain subject to independent exact-head qualification.
#[no_mangle]
pub unsafe extern "C" fn mem_q5_178_vec_dot(
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
    if FIRST_Q5_DOT.swap(false, Ordering::SeqCst) {
        eprintln!("BPB3K_Q5_178_REAL_DOT_EXECUTED model_live=true payload_bytes=178 no_stock_fallback=true");
    }
    let blocks = (n / 256) as usize;
    // SAFETY: ggml's pinned Q3_K surrogate is patched to 178 B/block;
    // tensor data spans exactly n/256 blocks; Q8_K is unchanged.
    let weights = unsafe { std::slice::from_raw_parts(vx.cast::<u8>(), blocks * 178) };
    let activations = unsafe { std::slice::from_raw_parts(vy.cast::<u8>(), blocks * 292) };
    // Original optimized ggml Q3_K performs eight independent FP32 FMA
    // chains, each surviving across ALL superblocks, then hsum_float_8.
    // Summing each 256-weight block first changes the FP32 reduction graph
    // and can flip full-model tokens despite identical integer products.
    let mut accumulators = [0f32; 8];
    for i in 0..blocks {
        let w = &weights[178 * i..178 * (i + 1)];
        let a = &activations[292 * i..292 * (i + 1)];
        let dot = match q5::dot_tiled::<1>(w, a).map(|blocks| blocks[0]) {
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

    fn scalar_reference(weights: &[u8], activations: &[u8], blocks: usize) -> f32 {
        let mut lanes = [0f32; 8];
        for block in 0..blocks {
            let w = &weights[178 * block..178 * (block + 1)];
            let a = &activations[292 * block..292 * (block + 1)];
            let integer = q5::scalar_lanes(w, a).unwrap();
            let global = f16_to_f32(integer.global_scale_f16_bits);
            let activation_scale = f32::from_bits(integer.q8_scale_f32_bits);
            let multiplier = activation_scale * global;
            for (acc, lane) in lanes.iter_mut().zip(integer.lanes) {
                *acc = multiplier.mul_add(lane as f32, *acc);
            }
        }
        let half: [f32; 4] = std::array::from_fn(|i| lanes[i] + lanes[i + 4]);
        (half[0] + half[2]) + (half[1] + half[3])
    }

    fn fixtures(blocks: usize) -> (Vec<u8>, Vec<u8>) {
        let mut w = Vec::new();
        let mut a = Vec::new();
        for block in 0..blocks {
            let codes: [i8; 256] = std::array::from_fn(|i| ((i * 23 + block * 7) % 32) as i8 - 16);
            let scales: [i8; 16] = std::array::from_fn(|i| ((i * 19 + block * 3) % 64) as i8 - 32);
            w.extend_from_slice(
                &q5::pack_signed(&codes, &scales, [0x3c00u16, 0xbc00, 0x3800][block % 3]).unwrap(),
            );
            let mut input = [0u8; 292];
            input[..4].copy_from_slice(&[0.015625f32, -0.0078125, 0.125][block % 3].to_le_bytes());
            for i in 0..256 {
                input[4 + i] = (i as i16 - 128 + block as i16) as i8 as u8;
            }
            a.extend_from_slice(&input);
        }
        (w, a)
    }

    #[test]
    fn independent_q5_scalar_and_live_ffi_match_fp32_fma_lane_order() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        for blocks in [1usize, 2, 3, 17] {
            let (weights, activations) = fixtures(blocks);
            let expected = scalar_reference(&weights, &activations, blocks);
            let mut actual = f32::NAN;
            unsafe {
                mem_q5_178_vec_dot(
                    (256 * blocks) as i32,
                    &mut actual,
                    0,
                    weights.as_ptr().cast(),
                    0,
                    activations.as_ptr().cast(),
                    0,
                    1,
                );
            }
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "block count = {blocks}"
            );
        }
    }

    #[test]
    fn f16_widening_preserves_subnormals_and_signed_zeros() {
        assert_eq!(f16_to_f32(0x0000).to_bits(), 0);
        assert_eq!(f16_to_f32(0x8000).to_bits(), 0x80000000);
        assert_eq!(f16_to_f32(0x3c00), 1.0);
        assert_eq!(f16_to_f32(0xbc00), -1.0);
        assert_eq!(f16_to_f32(0x0001), 2f32.powi(-24));
        assert!(f16_to_f32(0x7e01).is_nan());
    }
}
