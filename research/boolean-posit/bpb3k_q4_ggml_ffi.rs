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
/// the scalar FP32 sum is NOT an assertion of matching ggml's FMA
/// eight-lane multiblock accumulation order. Whole-model logits and
/// tokens MUST be qualified independently against optimized stock.
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
    let mut output = 0f32;
    for i in 0..blocks {
        let w = &weights[146 * i..146 * (i + 1)];
        let a = &activations[292 * i..292 * (i + 1)];
        let dot = match q4::dot_q4::<146>(w, a) {
            Ok(dot) => dot,
            Err(_) => std::process::abort(),
        };
        let d = f32::from_bits(dot.q8_scale_f32_bits) * f16_to_f32(dot.global_scale_f16_bits);
        output = d.mul_add(dot.integer as f32, output);
    }
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
}
