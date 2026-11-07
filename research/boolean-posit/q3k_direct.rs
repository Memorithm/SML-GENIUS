//! Research-only direct bipolar dot product over the original GGUF Q3_K wire.
//!
//! This is an independent, byte-addressed control for BPB3-K, not an
//! optimized or proven competitive Q3_K implementation. All 110 input bytes
//! retain their original ggml meaning: 32-byte hmask, 64-byte 2-bit codes,
//! 12-byte subgroup scales, and 2-byte FP16 global scale.
//!
//! This kernel never constructs a dense 256-weight array or allocates a
//! temporary. It currently accepts only bipolar activations (+1 or -1).
//!
//! Upstream format source: ggml-org/llama.cpp at
//! f7591a6a01f9273de03331732bdc0b39721168be.

#![forbid(unsafe_code)]

pub const VALUES: usize = 256;
pub const WIRE_BYTES: usize = 110;
pub const INPUT_MASK_WORDS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    WrongLength { actual: usize },
}

/// Exact integer-weighted sum and unchanged Q3_K global FP16 scale bits.
///
/// The represented mathematical result is weighted_integer multiplied by
/// the FP16 value identified by global_scale_f16_bits, without choosing
/// any floating-point reduction or rounding policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectDot {
    pub weighted_integer: i32,
    pub global_scale_f16_bits: u16,
}

fn signed_subgroup_scale(scales: &[u8], subgroup: usize) -> i32 {
    let low = if subgroup < 8 {
        scales[subgroup] & 15
    } else {
        scales[subgroup - 8] >> 4
    };
    let high = (scales[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
    i32::from(low | (high << 4)) - 32
}

/// Evaluate 256 original Q3_K coefficients directly from their wire bytes.
///
/// Each subgroup has 16 lanes. The low two-bit code is recovered from the
/// source byte group and the hmask bit decides whether to subtract four.
/// The signed 6-bit subgroup scale multiplies the 16 signed bipolar terms.
///
/// No Q3_K -> BPB3-K conversion is performed. No dense decompressed weight
/// tensor is created. This byte-indexed reference is intentionally simple:
/// it is NOT an optimized ggml/AVX2/SIMD benchmark competitor.
pub fn dot_q3k_bipolar(
    raw: &[u8],
    input_negative: &[u64; INPUT_MASK_WORDS],
) -> Result<DirectDot, Error> {
    if raw.len() != WIRE_BYTES {
        return Err(Error::WrongLength { actual: raw.len() });
    }

    let scales = &raw[96..108];
    let global_scale_f16_bits = u16::from_le_bytes([raw[108], raw[109]]);
    let mut weighted_integer = 0i32;

    for subgroup in 0..16 {
        let half = subgroup / 8;
        let group_in_half = subgroup % 8;
        let q_base = half * 32 + (group_in_half % 2) * 16;
        let low_shift = 2 * (group_in_half / 2);
        let sign_bit = 1u8 << (half * 4 + group_in_half / 2);
        let mut integer_dot = 0i32;

        for lane in 0..16 {
            let index = subgroup * 16 + lane;
            let low = i32::from((raw[32 + q_base + lane] >> low_shift) & 3);
            let is_negative = (raw[(group_in_half % 2) * 16 + lane] & sign_bit) == 0;
            let q = low - if is_negative { 4 } else { 0 };
            let x_negative = (input_negative[index / 64] >> (index % 64)) & 1 != 0;
            integer_dot += if x_negative { -q } else { q };
        }

        debug_assert!(integer_dot.abs() <= 64);
        weighted_integer += integer_dot * signed_subgroup_scale(scales, subgroup);
    }

    debug_assert!(weighted_integer.abs() <= 32_768);
    Ok(DirectDot {
        weighted_integer,
        global_scale_f16_bits,
    })
}
