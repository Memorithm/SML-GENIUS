//! Research-only Haswell/Broadwell AVX2 bitplane decoder: broadcast the
//! contiguous four bytes for each 32-weight pair directly into a YMM register.
//!
//! This replaces two scalar u16 loads + 128-bit lane insertion per bitplane
//! with a single four-byte load followed by VPBROADCASTD/PSHUFB. Existing
//! BPB3-K bytes/metadata (110) and signed Q8_K bytes (292) are unchanged.
//! Q8_K cached bsums are *not* required or trusted in this candidate.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(test)]
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;
#[path = "bpb3k_pshufb_avx2.rs"]
#[allow(dead_code)]
mod previous;
pub use previous::{PshufbError, SimdQ8Dot, Q8_K_BYTES};

const WEIGHT_BYTES: usize = 110;

#[inline]
fn subgroup_scale(scales: &[u8], subgroup: usize) -> i16 {
    let low = if subgroup < 8 {
        scales[subgroup] & 15
    } else {
        scales[subgroup - 8] >> 4
    };
    let high = (scales[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
    i16::from(low | (high << 4)) - 32
}

#[inline]
fn read_u32(raw: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes(raw[pos..pos + 4].try_into().unwrap())
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::*;
    use core::arch::x86_64::*;

    /// PSHUFB takes byte 0 for lanes 0..7, byte 1 for lanes 8..15,
    /// byte 2 for lanes 16..23, and byte 3 for lanes 24..31. Each
    /// 128-bit PSHUFB lane can see all four bytes because the same
    /// u32 is broadcast into each dword before the shuffle.
    #[target_feature(enable = "avx2")]
    #[inline]
    unsafe fn expand32(
        raw: u32,
        selector: __m256i,
        bits: __m256i,
        zero: __m256i,
        ones: __m256i,
    ) -> __m256i {
        let packed = _mm256_set1_epi32(raw as i32);
        let shuffled = _mm256_shuffle_epi8(packed, selector);
        let matched = _mm256_and_si256(shuffled, bits);
        _mm256_andnot_si256(_mm256_cmpeq_epi8(matched, zero), ones)
    }

    /// Exact per-block integer result, with all reduction done in AVX2.
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn dot(weight: &[u8], activation: &[u8]) -> i32 {
        unsafe {
            let selector = _mm256_setr_epi8(
                0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3,
                3, 3, 3, 3,
            );
            let bits = _mm256_setr_epi8(
                1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64,
                -128, 1, 2, 4, 8, 16, 32, 64, -128,
            );
            let zero = _mm256_setzero_si256();
            let ones = _mm256_set1_epi8(1);
            let fours = _mm256_set1_epi8(4);
            let scales = &weight[96..108];
            let mut acc = _mm256_setzero_si256();

            for group in 0..4usize {
                for pair in 0..2usize {
                    let offset = group * 24 + pair * 4;
                    let sign = expand32(
                        read_u32(weight, offset),
                        selector,
                        bits,
                        zero,
                        ones,
                    );
                    let b0 = expand32(
                        read_u32(weight, offset + 8),
                        selector,
                        bits,
                        zero,
                        ones,
                    );
                    let b1 = expand32(
                        read_u32(weight, offset + 16),
                        selector,
                        bits,
                        zero,
                        ones,
                    );

                    // Signed code q = (b0 XOR s) + 2*(b1 XOR s) - 4*s.
                    // Map to 0..7 and subtract 4 times signed Q8 before
                    // applying exactly the original subgroup scales.
                    let bit0 = _mm256_xor_si256(b0, sign);
                    let bit1 = _mm256_slli_epi16(_mm256_xor_si256(b1, sign), 1);
                    let positive = _mm256_slli_epi16(_mm256_xor_si256(sign, ones), 2);
                    let unsigned_q = _mm256_or_si256(
                        bit0,
                        _mm256_or_si256(bit1, positive),
                    );

                    let q8 = _mm256_loadu_si256(
                        activation.as_ptr().add(4 + group * 64 + pair * 32) as *const __m256i,
                    );
                    let dot_plus4 = _mm256_maddubs_epi16(unsigned_q, q8);
                    let four_times_q8 = _mm256_maddubs_epi16(fours, q8);
                    let signed_pairs = _mm256_sub_epi16(dot_plus4, four_times_q8);

                    let subgroup = group * 4 + pair * 2;
                    let s0 = subgroup_scale(scales, subgroup);
                    let s1 = subgroup_scale(scales, subgroup + 1);
                    let scale_vec = _mm256_inserti128_si256(
                        _mm256_castsi128_si256(_mm_set1_epi16(s0)),
                        _mm_set1_epi16(s1),
                        1,
                    );
                    acc = _mm256_add_epi32(
                        acc,
                        _mm256_madd_epi16(signed_pairs, scale_vec),
                    );
                }
            }

            let mut result = [0i32; 8];
            _mm256_storeu_si256(result.as_mut_ptr().cast::<__m256i>(), acc);
            result.iter().sum()
        }
    }
}

/// Safe CPU dispatch around the original exact 110/292-byte ABI.
/// Does not interpret arbitrary multiblock FP32 accumulation order.
pub fn dot_bpb3k_q8_broadcast32(
    weights: &[u8],
    activation: &[u8],
) -> Result<SimdQ8Dot, PshufbError> {
    if weights.len() != WEIGHT_BYTES {
        return Err(PshufbError::WrongWeightLength {
            actual: weights.len(),
        });
    }
    if activation.len() != Q8_K_BYTES {
        return Err(PshufbError::WrongActivationLength {
            actual: activation.len(),
        });
    }
    #[cfg(target_arch = "x86_64")]
    {
        if !std::is_x86_feature_detected!("avx2") {
            return Err(PshufbError::UnsupportedCpu);
        }
        let weighted_integer = unsafe { x86::dot(weights, activation) };
        debug_assert!(weighted_integer.abs() <= 4_194_304);
        Ok(SimdQ8Dot {
            weighted_integer,
            global_scale_f16_bits: u16::from_le_bytes([weights[108], weights[109]]),
            activation_scale_f32_bits: u32::from_le_bytes(activation[..4].try_into().unwrap()),
        })
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        Err(PshufbError::UnsupportedCpu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q3(case: usize, fp16: u16) -> [u8; WEIGHT_BYTES] {
        let mut x: [u8; WEIGHT_BYTES] =
            std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8);
        x[108..110].copy_from_slice(&fp16.to_le_bytes());
        x
    }

    fn q8(case: usize, pattern: usize, fbits: u32) -> [u8; Q8_K_BYTES] {
        let mut a = [0u8; Q8_K_BYTES];
        a[0..4].copy_from_slice(&fbits.to_le_bytes());
        for i in 0..256 {
            a[4 + i] = match pattern % 8 {
                0 => 0,
                1 => 127,
                2 => 128,
                3 => 255,
                4 => {
                    if i % 2 == 0 {
                        127
                    } else {
                        128
                    }
                }
                5 => {
                    if i % 2 == 0 {
                        1
                    } else {
                        255
                    }
                }
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
        a
    }

    fn original_q3_oracle(w: &[u8; WEIGHT_BYTES], a: &[u8; Q8_K_BYTES]) -> i32 {
        let mut acc = 0;
        for i in 0..256 {
            let half = i / 128;
            let quadrant = (i % 128) / 32;
            let low = (w[32 + half * 32 + i % 32] >> (quadrant * 2)) & 3;
            let sign = (w[i % 32] >> (half * 4 + quadrant)) & 1;
            let signed = i32::from(low) - if sign == 0 { 4 } else { 0 };
            acc += signed
                * i32::from(subgroup_scale(&w[96..108], i / 16))
                * i32::from(a[4 + i] as i8);
        }
        acc
    }

    #[test]
    fn original_q3_oracle_and_prior_pshufb_exact() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        for case in 0..96 {
            let raw = q3(case, [0x3c00, 0xbc00, 0x7e01, 0x8000][case % 4]);
            let packed = codec::q3_to_bpb3k(&raw).unwrap();
            for pattern in 0..16 {
                let a = q8(case, pattern, [0x3f800000, 0x7fc12345][pattern % 2]);
                let x = dot_bpb3k_q8_broadcast32(&packed, &a).unwrap();
                let old = previous::dot_bpb3k_q8_pshufb(&packed, &a).unwrap();
                assert_eq!(
                    x.weighted_integer,
                    original_q3_oracle(&raw, &a),
                    "case={case} pattern={pattern}"
                );
                assert_eq!(x, old, "case={case} pattern={pattern}");
            }
        }
    }

    #[test]
    fn all_256_signed_q8_values_and_extreme_inputs() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let mut original = [0u8; WEIGHT_BYTES];
        original[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
        let packed = codec::q3_to_bpb3k(&original).unwrap();
        let mut a = q8(0, 6, 0x3f800000);
        for (code, expected) in [(128u8, -4_194_304), (127u8, 4_161_536), (0u8, 0)] {
            a[4..260].fill(code);
            assert_eq!(
                dot_bpb3k_q8_broadcast32(&packed, &a)
                    .unwrap()
                    .weighted_integer,
                expected
            );
        }
        a = q8(0, 6, 0x3f800000);
        assert_eq!(
            dot_bpb3k_q8_broadcast32(&packed, &a)
                .unwrap()
                .weighted_integer,
            original_q3_oracle(&original, &a)
        );
    }

    #[test]
    fn unaligned_offsets_sentinels_and_invalid_lengths() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let raw = q3(73, 0x3c00);
        let weights = codec::q3_to_bpb3k(&raw).unwrap();
        let a = q8(71, 7, 0x3f800000);
        for offset in 0..32 {
            let mut wp = vec![0xa5u8; WEIGHT_BYTES + 64];
            let mut ap = vec![0x5au8; Q8_K_BYTES + 64];
            wp[offset..offset + WEIGHT_BYTES].copy_from_slice(&weights);
            ap[31 - offset..31 - offset + Q8_K_BYTES].copy_from_slice(&a);
            let wb = wp.clone();
            let ab = ap.clone();
            let result = dot_bpb3k_q8_broadcast32(
                &wp[offset..offset + WEIGHT_BYTES],
                &ap[31 - offset..31 - offset + Q8_K_BYTES],
            )
            .unwrap();
            assert_eq!(result.weighted_integer, original_q3_oracle(&raw, &a));
            assert_eq!(wp, wb);
            assert_eq!(ap, ab);
        }
        for n in [0, 1, 109, 111, 256] {
            assert_eq!(
                dot_bpb3k_q8_broadcast32(&vec![0u8; n], &a),
                Err(PshufbError::WrongWeightLength { actual: n })
            );
        }
        for n in [0, 1, 260, 291, 293, 512] {
            assert_eq!(
                dot_bpb3k_q8_broadcast32(&weights, &vec![0u8; n]),
                Err(PshufbError::WrongActivationLength { actual: n })
            );
        }
    }
}
