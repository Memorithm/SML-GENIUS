//! Experimental AVX2 microtile: one signed Q8_K superblock is consumed
//! concurrently by 1, 2 or 4 BPB3-K weight rows, sharing a single YMM
//! activation load and +4 offset dot per 32-code pair across all rows.
//! Never claims ggml nrc=4/tiled model parity or changes model dispatch.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(test)]
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;
#[cfg(test)]
#[path = "bpb3k_pshufb_avx2.rs"]
#[allow(dead_code)]
mod old;

pub const WEIGHT_BYTES: usize = 110;
pub const Q8_K_BYTES: usize = 292;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchError {
    InvalidRowCount { actual: usize },
    WrongWeightLength { row: usize, actual: usize },
    WrongActivationLength { actual: usize },
    UnsupportedCpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchDot<const N: usize> {
    pub integer_dots: [i32; N],
    pub f16_scale_bits: [u16; N],
    pub q8_f32_scale_bits: u32,
}

#[inline]
fn scale(w: &[u8], subgroup: usize) -> i16 {
    let s = &w[96..108];
    let low = if subgroup < 8 {
        s[subgroup] & 15
    } else {
        s[subgroup - 8] >> 4
    };
    let high = (s[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
    i16::from(low | (high << 4)) - 32
}

#[inline]
fn word(w: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(w[at..at + 4].try_into().unwrap())
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::*;
    use core::arch::x86_64::*;

    #[target_feature(enable = "avx2")]
    #[inline]
    unsafe fn expand32(
        bits32: u32,
        selector: __m256i,
        bit_selector: __m256i,
        zero: __m256i,
        one: __m256i,
    ) -> __m256i {
        let packed = _mm256_set1_epi32(bits32 as i32);
        let bytes = _mm256_shuffle_epi8(packed, selector);
        let bits = _mm256_and_si256(bytes, bit_selector);
        _mm256_andnot_si256(_mm256_cmpeq_epi8(bits, zero), one)
    }

    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn dot<const N: usize>(
        weights: [&[u8]; N],
        activation: &[u8],
    ) -> [i32; N] {
        unsafe {
            // The selector visits the four contiguous source bytes per
            // bitplane and expands their eight bits into 32 coefficient
            // lanes. Each 128-bit half sees the same broadcast dword.
            let selector = _mm256_setr_epi8(
                0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3,
                3, 3, 3, 3,
            );
            let bit_selector = _mm256_setr_epi8(
                1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64,
                -128, 1, 2, 4, 8, 16, 32, 64, -128,
            );
            let zero = _mm256_setzero_si256();
            let one = _mm256_set1_epi8(1);
            let four = _mm256_set1_epi8(4);
            let mut accumulators = [zero; N];

            for group in 0..4usize {
                for pair in 0..2usize {
                    // Shared activation / correction across up to FOUR
                    // weight rows; neither pointer nor contents change.
                    let input = _mm256_loadu_si256(
                        activation
                            .as_ptr()
                            .add(4 + group * 64 + pair * 32)
                            .cast::<__m256i>(),
                    );
                    let four_times_q8 = _mm256_maddubs_epi16(four, input);
                    let subgroup = 4 * group + 2 * pair;
                    let p = 24 * group + 4 * pair;
                    for row in 0..N {
                        let w = weights[row];
                        let sign = expand32(word(w, p), selector, bit_selector, zero, one);
                        let low = expand32(word(w, p + 8), selector, bit_selector, zero, one);
                        let high = expand32(word(w, p + 16), selector, bit_selector, zero, one);
                        let b0 = _mm256_xor_si256(low, sign);
                        let b1 = _mm256_slli_epi16(_mm256_xor_si256(high, sign), 1);
                        let positive = _mm256_slli_epi16(_mm256_xor_si256(sign, one), 2);
                        let unsigned_q = _mm256_or_si256(
                            b0,
                            _mm256_or_si256(b1, positive),
                        );
                        let signed_pairs = _mm256_sub_epi16(
                            _mm256_maddubs_epi16(unsigned_q, input),
                            four_times_q8,
                        );
                        let scales = _mm256_inserti128_si256(
                            _mm256_castsi128_si256(_mm_set1_epi16(scale(w, subgroup))),
                            _mm_set1_epi16(scale(w, subgroup + 1)),
                            1,
                        );
                        accumulators[row] = _mm256_add_epi32(
                            accumulators[row],
                            _mm256_madd_epi16(signed_pairs, scales),
                        );
                    }
                }
            }

            let mut result = [0i32; N];
            for row in 0..N {
                let mut lanes = [0i32; 8];
                _mm256_storeu_si256(lanes.as_mut_ptr().cast::<__m256i>(), accumulators[row]);
                result[row] = lanes.iter().sum();
            }
            result
        }
    }
}

/// N=1,2,4 are admitted, others rejected (eight rows must be executed
/// as two honest four-row tiles; there is no implicit eight-row fusion).
/// This is a single-superblock INTEGER dot. Production ggml FP32 order,
/// mul-add placement and nrc/stride behavior require separate parity tests.
pub fn dot_bpb3k_q8_batch<const N: usize>(
    weights: [&[u8]; N],
    activation: &[u8],
) -> Result<BatchDot<N>, BatchError> {
    if ![1, 2, 4].contains(&N) {
        return Err(BatchError::InvalidRowCount { actual: N });
    }
    for (row, weight) in weights.iter().enumerate() {
        if weight.len() != WEIGHT_BYTES {
            return Err(BatchError::WrongWeightLength {
                row,
                actual: weight.len(),
            });
        }
    }
    if activation.len() != Q8_K_BYTES {
        return Err(BatchError::WrongActivationLength {
            actual: activation.len(),
        });
    }

    #[cfg(target_arch = "x86_64")]
    {
        if !std::is_x86_feature_detected!("avx2") {
            return Err(BatchError::UnsupportedCpu);
        }
        // SAFETY: exact lengths checked on all row buffers, unaligned
        // activation data loads; AVX2 feature checked before this call.
        let integer_dots = unsafe { x86::dot(weights, activation) };
        debug_assert!(integer_dots.iter().all(|v| v.abs() <= 4_194_304));
        let mut f16_scale_bits = [0u16; N];
        for row in 0..N {
            f16_scale_bits[row] =
                u16::from_le_bytes([weights[row][108], weights[row][109]]);
        }
        Ok(BatchDot {
            integer_dots,
            f16_scale_bits,
            q8_f32_scale_bits: u32::from_le_bytes(activation[..4].try_into().unwrap()),
        })
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        Err(BatchError::UnsupportedCpu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q3(case: usize) -> [u8; WEIGHT_BYTES] {
        let mut original: [u8; WEIGHT_BYTES] =
            std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8);
        original[108..110].copy_from_slice(
            &[0x3c00u16, 0xbc00, 0x7e01, 0x8000][case % 4].to_le_bytes(),
        );
        original
    }

    fn q8(case: usize, pattern: usize) -> [u8; Q8_K_BYTES] {
        let mut input = [0u8; Q8_K_BYTES];
        input[..4].copy_from_slice(
            &[0x3f800000u32, 0x7fc12345u32][pattern % 2].to_le_bytes(),
        );
        for i in 0..256 {
            input[4 + i] = match pattern % 8 {
                0 => 0,
                1 => 127,
                2 => 128,
                3 => 255,
                4 => if i % 2 == 0 { 127 } else { 128 },
                5 => if i % 2 == 0 { 1 } else { 255 },
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
        input
    }

    // Independent original Q3_K byte-field decoder, NOT a BPB3-K codec.
    fn oracle(original: &[u8; WEIGHT_BYTES], activation: &[u8; Q8_K_BYTES]) -> i32 {
        (0..256)
            .map(|i| {
                let half = i / 128;
                let plane = (i % 128) / 32;
                let lane = i % 32;
                let low = (original[32 + half * 32 + lane] >> (2 * plane)) & 3;
                let sign = (original[lane] >> (half * 4 + plane)) & 1;
                let signed = i32::from(low) - if sign == 0 { 4 } else { 0 };
                signed * i32::from(scale(original, i / 16))
                    * i32::from(activation[4 + i] as i8)
            })
            .sum()
    }

    #[test]
    fn independent_oracle_reused_activation_and_old_pshufb() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        for case in 0..96 {
            let original: [[u8; WEIGHT_BYTES]; 4] =
                std::array::from_fn(|row| q3(case * 4 + row));
            let converted: [[u8; WEIGHT_BYTES]; 4] =
                original.each_ref().map(|w| codec::q3_to_bpb3k(w).unwrap());
            for pattern in 0..16 {
                let activation = q8(case, pattern);
                let actual = dot_bpb3k_q8_batch(
                    converted.each_ref().map(|w| &w[..]),
                    &activation,
                )
                .unwrap();
                for row in 0..4 {
                    assert_eq!(
                        actual.integer_dots[row],
                        oracle(&original[row], &activation),
                        "case={case} pattern={pattern} row={row}"
                    );
                    assert_eq!(
                        actual.integer_dots[row],
                        old::dot_bpb3k_q8_pshufb(&converted[row], &activation)
                            .unwrap()
                            .weighted_integer,
                        "old PSHUFB case={case} row={row}"
                    );
                }
                assert_eq!(
                    actual.q8_f32_scale_bits,
                    [0x3f800000u32, 0x7fc12345u32][pattern % 2]
                );
                for row in 0..4 {
                    assert_eq!(
                        actual.f16_scale_bits[row],
                        [0x3c00u16, 0xbc00, 0x7e01, 0x8000][(case * 4 + row) % 4]
                    );
                }
            }
        }
    }

    #[test]
    fn one_two_four_tile_and_256_signed_q8_codes() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let original: [[u8; WEIGHT_BYTES]; 4] =
            std::array::from_fn(|row| q3(row + 11));
        let packed = original.each_ref().map(|w| codec::q3_to_bpb3k(w).unwrap());
        let input = q8(3, 6);
        for (row, expected) in original.iter().enumerate() {
            assert_eq!(
                dot_bpb3k_q8_batch([&packed[row][..]], &input)
                    .unwrap()
                    .integer_dots[0],
                oracle(expected, &input)
            );
        }
        let two = dot_bpb3k_q8_batch([&packed[0][..], &packed[1][..]], &input).unwrap();
        let four = dot_bpb3k_q8_batch(packed.each_ref().map(|w| &w[..]), &input).unwrap();
        for row in 0..2 {
            assert_eq!(two.integer_dots[row], four.integer_dots[row]);
        }
        for row in 0..4 {
            assert_eq!(four.integer_dots[row], oracle(&original[row], &input));
        }
    }

    #[test]
    fn rejected_lengths_and_unsupported_microtiles() {
        let correct = [0u8; WEIGHT_BYTES];
        let a = [0u8; Q8_K_BYTES];
        assert_eq!(
            dot_bpb3k_q8_batch([&correct[..]; 3], &a),
            Err(BatchError::InvalidRowCount { actual: 3 })
        );
        assert_eq!(
            dot_bpb3k_q8_batch([&correct[..]; 8], &a),
            Err(BatchError::InvalidRowCount { actual: 8 })
        );
        assert_eq!(
            dot_bpb3k_q8_batch([&correct[..]; 2], &a[..291]),
            Err(BatchError::WrongActivationLength { actual: 291 })
        );
        assert_eq!(
            dot_bpb3k_q8_batch([&correct[..], &correct[..109]], &a),
            Err(BatchError::WrongWeightLength { row: 1, actual: 109 })
        );
    }

    #[test]
    fn unaligned_input_guards_and_no_mutation() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let original = q3(45);
        let packed = codec::q3_to_bpb3k(&original).unwrap();
        let a = q8(44, 7);
        let reference = oracle(&original, &a);
        for offset in 0..32 {
            let mut w = vec![0x5au8; WEIGHT_BYTES + 64];
            let mut input = vec![0xa5u8; Q8_K_BYTES + 64];
            w[offset..offset + WEIGHT_BYTES].copy_from_slice(&packed);
            input[31 - offset..31 - offset + Q8_K_BYTES].copy_from_slice(&a);
            let w_before = w.clone();
            let a_before = input.clone();
            let weights = &w[offset..offset + WEIGHT_BYTES];
            let acts = &input[31 - offset..31 - offset + Q8_K_BYTES];
            let result = dot_bpb3k_q8_batch([weights; 4], acts).unwrap();
            assert_eq!(result.integer_dots, [reference; 4]);
            assert_eq!(w, w_before);
            assert_eq!(input, a_before);
        }
    }
}
