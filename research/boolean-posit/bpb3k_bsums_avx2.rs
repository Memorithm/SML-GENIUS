//! Research-only, native Haswell/Broadwell AVX2 BPB3-K x signed Q8_K dot.
//! Reuses the sixteen precomputed signed Q8_K subgroup sums as the
//! exact -4 offset correction, eliminating eight VPMADDUBSW and eight
//! VPSUBW operations from the previous bitplane/PSHUFB kernel.
//!
//! This computes only exact i32; it is not a GGML FP32 accumulator nor
//! a substitute for the original model inference path. No heap allocation.
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
fn plane(raw: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(raw[offset..offset + 8].try_into().unwrap())
}

/// Optional Q8_K integrity gate for input admission; not in the timed dot.
/// The actual Q8_K quantizer guarantees sum(codes[subgroup * 16..]) equals
/// the stored little-endian i16 bsums. We rely on that format contract.
pub fn q8_k_cached_sums_valid(activation: &[u8]) -> bool {
    if activation.len() != Q8_K_BYTES {
        return false;
    }
    (0..16).all(|subgroup| {
        let actual: i16 = activation[4 + subgroup * 16..4 + (subgroup + 1) * 16]
            .iter()
            .map(|&v| i16::from(v as i8))
            .sum();
        let stored = i16::from_le_bytes(
            activation[260 + subgroup * 2..262 + subgroup * 2]
                .try_into()
                .unwrap(),
        );
        actual == stored
    })
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::*;
    use core::arch::x86_64::*;

    /// Lane-wise 16-bit expansion of one signed/magnitude bitplane to 32
    /// byte-sized {0,1} values. Split across two independent YMM halves.
    #[target_feature(enable = "avx2")]
    #[inline]
    unsafe fn spread(
        low: u16,
        high: u16,
        selector: __m256i,
        bits: __m256i,
        zero: __m256i,
        ones: __m256i,
    ) -> __m256i {
        let l = _mm_cvtsi32_si128(i32::from(low));
        let h = _mm_cvtsi32_si128(i32::from(high));
        let v = _mm256_inserti128_si256(_mm256_castsi128_si256(l), h, 1);
        let shuffled = _mm256_shuffle_epi8(v, selector);
        let test = _mm256_and_si256(shuffled, bits);
        _mm256_andnot_si256(_mm256_cmpeq_epi8(test, zero), ones)
    }

    /// The only unsafe operations are unaligned 256-bit data loads/stores
    /// within validated 110/292-byte buffers. The optional independent
    /// Q8_K bsums integrity gate belongs at input-admission time, not here.
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn dot(w: &[u8], a: &[u8]) -> i32 {
        unsafe {
            let selector = _mm256_setr_epi8(
                0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1,
                1, 1, 1, 1,
            );
            let bits = _mm256_setr_epi8(
                1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64,
                -128, 1, 2, 4, 8, 16, 32, 64, -128,
            );
            let zero = _mm256_setzero_si256();
            let ones = _mm256_set1_epi8(1);
            let mut acc = _mm256_setzero_si256();
            let mut scales16 = [0i16; 16];

            for group in 0..4 {
                let sign = plane(w, group * 24);
                let low = plane(w, group * 24 + 8);
                let high = plane(w, group * 24 + 16);
                for pair in 0..2 {
                    let shift = pair * 32;
                    let s = spread(
                        (sign >> shift) as u16,
                        (sign >> (shift + 16)) as u16,
                        selector,
                        bits,
                        zero,
                        ones,
                    );
                    let b0 = spread(
                        (low >> shift) as u16,
                        (low >> (shift + 16)) as u16,
                        selector,
                        bits,
                        zero,
                        ones,
                    );
                    let b1 = spread(
                        (high >> shift) as u16,
                        (high >> (shift + 16)) as u16,
                        selector,
                        bits,
                        zero,
                        ones,
                    );

                    // q = (b0 XOR s) + 2*(b1 XOR s) - 4*s.
                    // Add 4 so the unsigned×signed VPMADDUBSW instruction
                    // can handle a signed Q3_K coefficient without two
                    // separate signed dot products.
                    let b0x = _mm256_xor_si256(b0, s);
                    let b1x = _mm256_slli_epi16(_mm256_xor_si256(b1, s), 1);
                    let not_s = _mm256_slli_epi16(_mm256_xor_si256(s, ones), 2);
                    let plus_four = _mm256_or_si256(b0x, _mm256_or_si256(b1x, not_s));

                    let input = _mm256_loadu_si256(
                        a.as_ptr().add(4 + group * 64 + pair * 32) as *const __m256i,
                    );
                    let pair_products = _mm256_maddubs_epi16(plus_four, input);

                    let index = group * 4 + pair * 2;
                    let scale0 = subgroup_scale(&w[96..108], index);
                    let scale1 = subgroup_scale(&w[96..108], index + 1);
                    scales16[index] = scale0;
                    scales16[index + 1] = scale1;
                    let scale_vec = _mm256_inserti128_si256(
                        _mm256_castsi128_si256(_mm_set1_epi16(scale0)),
                        _mm_set1_epi16(scale1),
                        1,
                    );
                    acc = _mm256_add_epi32(
                        acc,
                        _mm256_madd_epi16(pair_products, scale_vec),
                    );
                }
            }

            // The -4 displacement for each one of 16 subgroups is
            // exactly -4 * sum_i(q8_i) * scale[subgroup].
            // Q8_K stores sum_i(q8_i) in its 32-byte cached bsums payload.
            // Apply all 16 corrections with ONE signed VPMADDWD instruction.
            let sums = _mm256_loadu_si256(a.as_ptr().add(260) as *const __m256i);
            let scale_vec = _mm256_loadu_si256(scales16.as_ptr().cast::<__m256i>());
            let corrections = _mm256_madd_epi16(sums, scale_vec);
            acc = _mm256_sub_epi32(acc, _mm256_slli_epi32(corrections, 2));

            let mut result = [0i32; 8];
            _mm256_storeu_si256(result.as_mut_ptr().cast::<__m256i>(), acc);
            result.iter().sum()
        }
    }
}

/// Safe AVX2 dispatch. The Q8_K cached-sum invariant must hold; callers can
/// use q8_k_cached_sums_valid once at an input-admission boundary.
pub fn dot_bpb3k_q8_bsums_avx2(
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
            activation_scale_f32_bits: u32::from_le_bytes(activation[0..4].try_into().unwrap()),
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

    fn original_q3(case: usize, fp16: u16) -> [u8; WEIGHT_BYTES] {
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
        for subgroup in 0..16 {
            let sum: i16 = a[4 + subgroup * 16..4 + (subgroup + 1) * 16]
                .iter()
                .map(|&c| i16::from(c as i8))
                .sum();
            a[260 + subgroup * 2..262 + subgroup * 2].copy_from_slice(&sum.to_le_bytes());
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
    fn independent_original_q3_and_old_pshufb_match_with_valid_bsums() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        for case in 0..96 {
            let fp16 = [0x3c00, 0xbc00, 0x7e01, 0x8000][case % 4];
            let original = original_q3(case, fp16);
            let transcode = codec::q3_to_bpb3k(&original).unwrap();
            for pattern in 0..16 {
                let a = q8(case, pattern, [0x3f800000, 0x7fc12345][pattern % 2]);
                assert!(q8_k_cached_sums_valid(&a));
                let old = previous::dot_bpb3k_q8_pshufb(&transcode, &a).unwrap();
                let new = dot_bpb3k_q8_bsums_avx2(&transcode, &a).unwrap();
                assert_eq!(
                    new.weighted_integer,
                    original_q3_oracle(&original, &a),
                    "case={case} pattern={pattern}"
                );
                assert_eq!(new, old, "case={case} pattern={pattern}");
                assert_eq!(new.global_scale_f16_bits, fp16);
                assert_eq!(
                    new.activation_scale_f32_bits,
                    [0x3f800000, 0x7fc12345][pattern % 2]
                );
            }
        }
    }

    #[test]
    fn q8_cached_sums_reject_corruption_without_overreading() {
        let mut a = q8(8, 7, 0x3f800000);
        assert!(q8_k_cached_sums_valid(&a));
        a[260] ^= 1;
        assert!(!q8_k_cached_sums_valid(&a));
        a[260] ^= 1;
        a[4 + 65] ^= 1;
        assert!(!q8_k_cached_sums_valid(&a));
        for n in [0, 1, 260, 291, 293, 512] {
            assert!(!q8_k_cached_sums_valid(&vec![0u8; n]));
        }
    }

    #[test]
    fn exact_negative_four_extremes_and_256_q8_signed_codes() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let mut original = [0u8; WEIGHT_BYTES];
        original[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
        let transcode = codec::q3_to_bpb3k(&original).unwrap();
        let mut a = q8(0, 6, 0x3f800000);
        for (code, expected) in [(128u8, -4_194_304), (127u8, 4_161_536), (0u8, 0)] {
            a[4..260].fill(code);
            for subgroup in 0..16 {
                let sum = i16::from(code as i8) * 16;
                a[260 + subgroup * 2..262 + subgroup * 2].copy_from_slice(&sum.to_le_bytes());
            }
            assert!(q8_k_cached_sums_valid(&a));
            assert_eq!(
                dot_bpb3k_q8_bsums_avx2(&transcode, &a)
                    .unwrap()
                    .weighted_integer,
                expected
            );
        }
        a = q8(0, 6, 0x3f800000);
        assert!(q8_k_cached_sums_valid(&a));
        assert_eq!(
            dot_bpb3k_q8_bsums_avx2(&transcode, &a)
                .unwrap()
                .weighted_integer,
            original_q3_oracle(&original, &a)
        );
    }

    #[test]
    fn unaligned_q8_and_weight_inputs_do_not_change_results() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let original = original_q3(83, 0x3c00);
        let weight = codec::q3_to_bpb3k(&original).unwrap();
        let input = q8(77, 7, 0x3f800000);
        for offset in 0..32 {
            let mut wb = vec![0x55; 110 + 64];
            let mut ab = vec![0xaa; Q8_K_BYTES + 64];
            wb[offset..offset + 110].copy_from_slice(&weight);
            ab[31 - offset..31 - offset + Q8_K_BYTES].copy_from_slice(&input);
            let w_before = wb.clone();
            let a_before = ab.clone();
            let got = dot_bpb3k_q8_bsums_avx2(
                &wb[offset..offset + 110],
                &ab[31 - offset..31 - offset + Q8_K_BYTES],
            )
            .unwrap();
            assert_eq!(got.weighted_integer, original_q3_oracle(&original, &input));
            assert_eq!(wb, w_before);
            assert_eq!(ab, a_before);
        }
    }

    #[test]
    fn malformed_length_rejected_before_raw_vector_load() {
        let w = [0u8; WEIGHT_BYTES];
        let a = [0u8; Q8_K_BYTES];
        for size in [0, 1, 109, 111, 256] {
            assert_eq!(
                dot_bpb3k_q8_bsums_avx2(&vec![0u8; size], &a),
                Err(PshufbError::WrongWeightLength { actual: size })
            );
        }
        for size in [0, 1, 4, 260, 291, 293, 512] {
            assert_eq!(
                dot_bpb3k_q8_bsums_avx2(&w, &vec![0u8; size]),
                Err(PshufbError::WrongActivationLength { actual: size })
            );
        }
    }
}
