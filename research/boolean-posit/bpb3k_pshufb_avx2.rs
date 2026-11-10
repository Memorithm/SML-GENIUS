//! Experimental x86-64 AVX2/PSHUFB decoder+MADD for original BPB3-K wire data.
//! 110-byte Q3_K-equivalent BPB3-K weights + 292-byte signed Q8_K activation.
//! Research-only, single-block exact-integer dot. No model/runtime dispatch.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(test)]
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;

const WEIGHT_BYTES: usize = 110;
pub const Q8_K_BYTES: usize = 292;
const SUBGROUPS_PER_GROUP: usize = 4;
#[cfg(test)]
const SUBGROUP_LANES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PshufbError {
    WrongWeightLength { actual: usize },
    WrongActivationLength { actual: usize },
    UnsupportedCpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimdQ8Dot {
    pub weighted_integer: i32,
    pub global_scale_f16_bits: u16,
    pub activation_scale_f32_bits: u32,
}

#[inline]
fn read_u64(raw: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(raw[at..at + 8].try_into().unwrap())
}

#[inline]
fn signed_subgroup_scale(scales: &[u8], subgroup: usize) -> i32 {
    let low = if subgroup < 8 {
        scales[subgroup] & 15
    } else {
        scales[subgroup - 8] >> 4
    };
    let high = (scales[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
    i32::from(low | (high << 4)) - 32
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::*;
    use core::arch::x86_64::*;

    /// PSHUFB broadcasts a sign/low/high plane's two bytes into 16 lanes.
    /// Per-lane bit tests produce 0 or 1, not 0 or -1. Two 16-lane
    /// subgroups are processed in parallel by the two YMM halves.
    #[target_feature(enable = "avx2")]
    #[inline]
    unsafe fn spread(
        left: u16,
        right: u16,
        byte_selector: __m256i,
        bit_selector: __m256i,
        zero: __m256i,
        ones: __m256i,
    ) -> __m256i {
        unsafe {
            let vleft = _mm_cvtsi32_si128(i32::from(left));
            let vright = _mm_cvtsi32_si128(i32::from(right));
            let packed = _mm256_inserti128_si256(_mm256_castsi128_si256(vleft), vright, 1);
            let per_lane_byte = _mm256_shuffle_epi8(packed, byte_selector);
            let test = _mm256_and_si256(per_lane_byte, bit_selector);
            _mm256_andnot_si256(_mm256_cmpeq_epi8(test, zero), ones)
        }
    }

    /// Process eight 32-element pairs directly in AVX2 integer registers.
    /// Only the final eight i32 accumulator lanes are reduced to scalar.
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn dot(weights: &[u8], activation: &[u8]) -> i32 {
        unsafe {
            let byte_selector = _mm256_setr_epi8(
                0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1,
                1, 1, 1, 1,
            );
            let bit_selector = _mm256_setr_epi8(
                1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64,
                -128, 1, 2, 4, 8, 16, 32, 64, -128,
            );
            let zero = _mm256_setzero_si256();
            let ones = _mm256_set1_epi8(1);
            let four = _mm256_set1_epi8(4);
            let scales = &weights[96..108];
            let mut acc = _mm256_setzero_si256();

            for group in 0..4usize {
                let offset = group * 24;
                let sign_plane = read_u64(weights, offset);
                let low_plane = read_u64(weights, offset + 8);
                let high_plane = read_u64(weights, offset + 16);
                for pair in 0..2 {
                    let shift = pair * 32;
                    let s = spread(
                        (sign_plane >> shift) as u16,
                        (sign_plane >> (shift + 16)) as u16,
                        byte_selector,
                        bit_selector,
                        zero,
                        ones,
                    );
                    let b0 = spread(
                        (low_plane >> shift) as u16,
                        (low_plane >> (shift + 16)) as u16,
                        byte_selector,
                        bit_selector,
                        zero,
                        ones,
                    );
                    let b1 = spread(
                        (high_plane >> shift) as u16,
                        (high_plane >> (shift + 16)) as u16,
                        byte_selector,
                        bit_selector,
                        zero,
                        ones,
                    );

                    // q = (b0 XOR s) + 2*(b1 XOR s) - 4*s.
                    // Shift into unsigned [0,7] so VPMADDUBSW can consume it:
                    // q+4 = (b0 XOR s) + 2*(b1 XOR s) + 4*(1-s).
                    let low = _mm256_xor_si256(b0, s);
                    let high = _mm256_slli_epi16(_mm256_xor_si256(b1, s), 1);
                    let positive = _mm256_slli_epi16(_mm256_xor_si256(s, ones), 2);
                    let unsigned_q = _mm256_or_si256(low, _mm256_or_si256(high, positive));

                    let input = _mm256_loadu_si256(
                        activation.as_ptr().add(4 + group * 64 + pair * 32) as *const __m256i,
                    );
                    // Adjacent products are far below i16 saturation bounds.
                    let pairs_plus4 = _mm256_maddubs_epi16(unsigned_q, input);
                    let four_times_pairs = _mm256_maddubs_epi16(four, input);
                    let signed_pairs = _mm256_sub_epi16(pairs_plus4, four_times_pairs);

                    let sub0 = group * SUBGROUPS_PER_GROUP + pair * 2;
                    let sub1 = sub0 + 1;
                    let a = signed_subgroup_scale(scales, sub0) as i16;
                    let b = signed_subgroup_scale(scales, sub1) as i16;
                    let scale_vec = _mm256_inserti128_si256(
                        _mm256_castsi128_si256(_mm_set1_epi16(a)),
                        _mm_set1_epi16(b),
                        1,
                    );
                    let products = _mm256_madd_epi16(signed_pairs, scale_vec);
                    acc = _mm256_add_epi32(acc, products);
                }
            }

            let mut output = [0i32; 8];
            _mm256_storeu_si256(output.as_mut_ptr().cast::<__m256i>(), acc);
            output.iter().sum()
        }
    }
}

/// Safe dispatch around a dedicated AVX2 implementation.
///
/// The 3.4375-bit weight payload and 292-byte Q8_K activation are unchanged.
/// Cached Q8_K bsums are intentionally not trusted, just as in the original
/// Q3_K AVX2 dot. This computes exact i32 only; no FP32/model parity claim.
/// No per-row heap allocation or prepared activation buffer is required.
pub fn dot_bpb3k_q8_pshufb(weights: &[u8], activation: &[u8]) -> Result<SimdQ8Dot, PshufbError> {
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
        // SAFETY: CPU feature checked above, lengths prevalidated and all
        // input vector loads are unaligned (loadu); no mutable aliases.
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
        let mut v: [u8; WEIGHT_BYTES] =
            std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8);
        v[108..110].copy_from_slice(&fp16.to_le_bytes());
        v
    }

    fn q8(case: usize, pattern: usize, f32_bits: u32) -> [u8; Q8_K_BYTES] {
        let mut v = [0u8; Q8_K_BYTES];
        v[..4].copy_from_slice(&f32_bits.to_le_bytes());
        for i in 0..256 {
            v[4 + i] = match pattern {
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
        v
    }

    fn original_q3_integer_oracle(q3: &[u8; WEIGHT_BYTES], q8: &[u8; Q8_K_BYTES]) -> i32 {
        let mut result = 0i32;
        for i in 0..256 {
            let half = i / 128;
            let plane = (i % 128) / 32;
            let local = i % 32;
            let magnitude = (q3[32 + half * 32 + local] >> (plane * 2)) & 3;
            let negative = (q3[local] >> (half * 4 + plane)) & 1 == 0;
            let signed = i32::from(magnitude) - if negative { 4 } else { 0 };
            result += signed
                * signed_subgroup_scale(&q3[96..108], i / SUBGROUP_LANES)
                * i32::from(q8[4 + i] as i8);
        }
        result
    }

    #[test]
    fn exact_against_original_q3_for_all_signed_codes_and_metadata() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        for case in 0..96 {
            let fp16 = [0x3c00, 0xbc00, 0x7e01, 0x8000][case % 4];
            let original = q3(case, fp16);
            let converted = codec::q3_to_bpb3k(&original).unwrap();
            for pattern in 0..16 {
                let activation = q8(case, pattern, [0x3f800000, 0x7fc12345][pattern % 2]);
                let actual = dot_bpb3k_q8_pshufb(&converted, &activation).unwrap();
                assert_eq!(
                    actual.weighted_integer,
                    original_q3_integer_oracle(&original, &activation),
                    "case={case} pattern={pattern}"
                );
                assert_eq!(actual.global_scale_f16_bits, fp16);
                assert_eq!(
                    actual.activation_scale_f32_bits,
                    [0x3f800000, 0x7fc12345][pattern % 2]
                );
            }
        }
    }

    #[test]
    fn negative_four_and_q8_minus_128_remain_exact() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let mut original = [0u8; WEIGHT_BYTES];
        original[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
        let converted = codec::q3_to_bpb3k(&original).unwrap();
        let mut activation = q8(0, 0, 0x3f800000);
        for (code, expected) in [(128u8, -4_194_304), (127u8, 4_161_536), (0u8, 0)] {
            activation[4..260].fill(code);
            assert_eq!(
                dot_bpb3k_q8_pshufb(&converted, &activation)
                    .unwrap()
                    .weighted_integer,
                expected
            );
        }
    }

    #[test]
    fn invalid_lengths_are_rejected_before_simd_loads() {
        let weight = [0u8; WEIGHT_BYTES];
        let activation = [0u8; Q8_K_BYTES];
        for len in [0, 1, 96, 109, 111, 256] {
            assert_eq!(
                dot_bpb3k_q8_pshufb(&vec![0u8; len], &activation),
                Err(PshufbError::WrongWeightLength { actual: len })
            );
        }
        for len in [0, 1, 4, 260, 291, 293, 512] {
            assert_eq!(
                dot_bpb3k_q8_pshufb(&weight, &vec![0u8; len]),
                Err(PshufbError::WrongActivationLength { actual: len })
            );
        }
    }
}
