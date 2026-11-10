//! Research-only scale-preencoded AVX2 microtile. Same 96 bitplane bytes
//! and exact signed quantization but 16 signed i16 subgroup scales occupy
//! 32 bytes instead of 12. With unchanged 2-byte FP16 this is 130 B/block,
//! not the original 110 B. Conversion is outside steady-state timing.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(test)]
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;
#[cfg(test)]
#[path = "bpb3k_pshufb_avx2.rs"]
#[allow(dead_code)]
mod old;

pub const WEIGHT_BYTES: usize = 130;
pub const PACKED_WEIGHT_BYTES: usize = 110;
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
    let at = 96 + 2 * subgroup;
    i16::from_le_bytes([w[at], w[at + 1]])
}

fn packed_scale(packed: &[u8], subgroup: usize) -> i16 {
    let s = &packed[96..108];
    let low = if subgroup < 8 {
        s[subgroup] & 15
    } else {
        s[subgroup - 8] >> 4
    };
    let high = (s[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
    i16::from(low | (high << 4)) - 32
}

/// One-time lossless transcode, 110 B BPB3-K -> 130 B scale-preencoded.
/// All bitplane bytes (0..96) and FP16 raw bits remain unchanged.
pub fn preencode_scales(packed: &[u8]) -> Result<[u8; WEIGHT_BYTES], BatchError> {
    if packed.len() != PACKED_WEIGHT_BYTES {
        return Err(BatchError::WrongWeightLength {
            row: 0,
            actual: packed.len(),
        });
    }
    let mut encoded = [0u8; WEIGHT_BYTES];
    encoded[..96].copy_from_slice(&packed[..96]);
    for i in 0..16 {
        encoded[96 + 2 * i..98 + 2 * i].copy_from_slice(&packed_scale(packed, i).to_le_bytes());
    }
    encoded[128..130].copy_from_slice(&packed[108..110]);
    Ok(encoded)
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
    pub(super) unsafe fn dot<const N: usize>(weights: [&[u8]; N], activation: &[u8]) -> [i32; N] {
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
                        let unsigned_q = _mm256_or_si256(b0, _mm256_or_si256(b1, positive));
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
            f16_scale_bits[row] = u16::from_le_bytes([weights[row][128], weights[row][129]]);
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

    #[test]
    fn transcode_preserves_256_signed_codes_six_bit_scales_and_raw_f16() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        for case in 0..96 {
            let mut original: [u8; 110] =
                std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8);
            original[108..110]
                .copy_from_slice(&[0x3c00u16, 0xbc00, 0x7e01, 0x8000][case % 4].to_le_bytes());
            let packed = codec::q3_to_bpb3k(&original).unwrap();
            let expanded = preencode_scales(&packed).unwrap();
            assert_eq!(&expanded[..96], &packed[..96]);
            assert_eq!(&expanded[128..130], &original[108..110]);
            for g in 0..16 {
                assert_eq!(scale(&expanded, g), packed_scale(&packed, g));
            }
            let mut activation = [0u8; Q8_K_BYTES];
            activation[..4].copy_from_slice(&0x7fc12345u32.to_le_bytes());
            for (i, a) in activation[4..260].iter_mut().enumerate() {
                *a = i.wrapping_add(case) as u8;
            }
            let actual = dot_bpb3k_q8_batch([&expanded[..]; 4], &activation).unwrap();
            let original_dot = old::dot_bpb3k_q8_pshufb(&packed, &activation)
                .unwrap()
                .weighted_integer;
            assert_eq!(actual.integer_dots, [original_dot; 4]);
            assert_eq!(
                actual.f16_scale_bits,
                [u16::from_le_bytes([original[108], original[109]]); 4]
            );
            assert_eq!(actual.q8_f32_scale_bits, 0x7fc12345);
        }
    }

    #[test]
    fn malformed_source_or_runtime_weight_lengths_rejected() {
        let raw = [0u8; PACKED_WEIGHT_BYTES];
        let a = [0u8; Q8_K_BYTES];
        for n in [0, 1, 109, 111, 130] {
            assert_eq!(
                preencode_scales(&vec![0u8; n]),
                Err(BatchError::WrongWeightLength { row: 0, actual: n })
            );
        }
        let expanded = preencode_scales(&raw).unwrap();
        assert_eq!(
            dot_bpb3k_q8_batch([&expanded[..129]], &a),
            Err(BatchError::WrongWeightLength {
                row: 0,
                actual: 129
            })
        );
        assert_eq!(
            dot_bpb3k_q8_batch([&expanded[..]], &a[..291]),
            Err(BatchError::WrongActivationLength { actual: 291 })
        );
    }
}
