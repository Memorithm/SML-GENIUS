//! Experimental exact Q3_K-equivalent Q4 nibble-transcode, 146B vs 160B.
//! 256 values in signed 4-bit two's-complement + 16 signed-i8 six-bit
//! subgroup scales + original FP16 bits; optional 14 zero padding bytes.
//! Native AVX2 uses unsigned VPMADDUBSW with -4 correction.
//! No model/GGUF dispatch and no full-model FP32 parity claim.
#![deny(unsafe_op_in_unsafe_fn)]

pub const ORIGINAL_BYTES: usize = 110;
pub const Q4_BYTES: usize = 146;
pub const Q4_PADDED_BYTES: usize = 160;
pub const Q8_BYTES: usize = 292;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Q4Error {
    InvalidFormatLength { actual: usize },
    WrongSourceLength { actual: usize },
    WrongRowLength { row: usize, actual: usize },
    WrongActivationLength { actual: usize },
    InvalidRowCount { actual: usize },
    UnsupportedCpu,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Q4Dot<const N: usize> {
    pub integer_dots: [i32; N],
    pub fp16_scale_bits: [u16; N],
    pub activation_f32_bits: u32,
}

#[inline]
fn original_scale(raw: &[u8], subgroup: usize) -> i8 {
    let scales = &raw[96..108];
    let low = if subgroup < 8 {
        scales[subgroup] & 15
    } else {
        scales[subgroup - 8] >> 4
    };
    let high = (scales[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
    (i16::from(low | (high << 4)) - 32) as i8
}

/// Independently reconstruct a code from original Q3_K wire bytes;
/// format: 32 hmask, 64 qs, 12 six-bit scales, 2 raw fp16.
#[inline]
pub fn source_q3_integer(raw: &[u8], index: usize) -> i8 {
    let half = index / 128;
    let quadrant = (index % 128) / 32;
    let lane = index % 32;
    let magnitude = (raw[32 + half * 32 + lane] >> (2 * quadrant)) & 3;
    let sign = (raw[lane] >> (half * 4 + quadrant)) & 1;
    magnitude as i8 - if sign == 0 { 4 } else { 0 }
}

/// Q4 bytes 0..128 encode signed values [-4,+3] in low-first nibbles.
/// Bytes 128..144 hold already-decoded signed i8 Q3_K subgroup scales.
/// Bytes 144..146 keep original raw FP16 scale. For LEN=160 the final
/// 14 bytes are deliberately zero; padding is NOT a speed guarantee.
pub fn transcode_q3_to_q4<const LEN: usize>(
    raw: &[u8],
) -> Result<[u8; LEN], Q4Error> {
    if LEN != Q4_BYTES && LEN != Q4_PADDED_BYTES {
        return Err(Q4Error::InvalidFormatLength { actual: LEN });
    }
    if raw.len() != ORIGINAL_BYTES {
        return Err(Q4Error::WrongSourceLength { actual: raw.len() });
    }
    let mut result = [0u8; LEN];
    for index in 0..256 {
        let code = (source_q3_integer(raw, index) as u8) & 15;
        result[index / 2] |= code << (4 * (index & 1));
    }
    for index in 0..16 {
        result[128 + index] = original_scale(raw, index) as u8;
    }
    result[144..146].copy_from_slice(&raw[108..110]);
    Ok(result)
}

#[inline]
pub fn q4_code(raw: &[u8], index: usize) -> i8 {
    let nibble = (raw[index / 2] >> (4 * (index & 1))) & 15;
    ((nibble ^ 8) as i8) - 8
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::*;
    use core::arch::x86_64::*;

    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn dot<const N: usize>(
        weights: [&[u8]; N],
        activation: &[u8],
    ) -> [i32; N] {
        unsafe {
            let nibble_mask = _mm_set1_epi8(15);
            let signflip = _mm256_set1_epi8(8);
            let four = _mm256_set1_epi8(4);
            let zero = _mm256_setzero_si256();
            let mut acc = [zero; N];
            for group in 0..8usize {
                // One signed Q8_K 32-code vector, reused for each
                // independent Q4 weight row and shared -4 correction.
                let q8 = _mm256_loadu_si256(
                    activation.as_ptr().add(4 + group * 32).cast::<__m256i>(),
                );
                let four_q8 = _mm256_maddubs_epi16(four, q8);
                for row in 0..N {
                    let w = weights[row];
                    let packed = _mm_loadu_si128(
                        w.as_ptr().add(group * 16).cast::<__m128i>(),
                    );
                    let lo = _mm_and_si128(packed, nibble_mask);
                    let hi = _mm_and_si128(_mm_srli_epi16(packed, 4), nibble_mask);
                    let expanded = _mm256_inserti128_si256(
                        _mm256_castsi128_si256(_mm_unpacklo_epi8(lo, hi)),
                        _mm_unpackhi_epi8(lo, hi),
                        1,
                    );
                    // 4-bit two's-complement q is [-4,+3]; convert
                    // directly to unsigned (q+4) in [0,7].
                    let q_plus_four =
                        _mm256_sub_epi8(_mm256_xor_si256(expanded, signflip), four);
                    let unsigned_pairs = _mm256_maddubs_epi16(q_plus_four, q8);
                    let signed_pairs = _mm256_sub_epi16(unsigned_pairs, four_q8);
                    let s0 = i16::from(w[128 + group * 2] as i8);
                    let s1 = i16::from(w[129 + group * 2] as i8);
                    let scales = _mm256_inserti128_si256(
                        _mm256_castsi128_si256(_mm_set1_epi16(s0)),
                        _mm_set1_epi16(s1),
                        1,
                    );
                    acc[row] = _mm256_add_epi32(
                        acc[row],
                        _mm256_madd_epi16(signed_pairs, scales),
                    );
                }
            }
            let mut dots = [0i32; N];
            for row in 0..N {
                let mut lanes = [0i32; 8];
                _mm256_storeu_si256(lanes.as_mut_ptr().cast::<__m256i>(), acc[row]);
                dots[row] = lanes.iter().sum();
            }
            dots
        }
    }
}

/// Safe AVX2 dispatch for N=1,2,4, LEN=146/160. A padded 160B
/// *stride* is 32-divisible, but 32B pointer alignment additionally
/// requires an aligned owning allocator; callers must not assume it.
pub fn dot_q4<const N: usize, const LEN: usize>(
    weights: [&[u8]; N],
    activation: &[u8],
) -> Result<Q4Dot<N>, Q4Error> {
    if LEN != Q4_BYTES && LEN != Q4_PADDED_BYTES {
        return Err(Q4Error::InvalidFormatLength { actual: LEN });
    }
    if ![1, 2, 4].contains(&N) {
        return Err(Q4Error::InvalidRowCount { actual: N });
    }
    for (row, weight) in weights.iter().enumerate() {
        if weight.len() != LEN {
            return Err(Q4Error::WrongRowLength {
                row,
                actual: weight.len(),
            });
        }
    }
    if activation.len() != Q8_BYTES {
        return Err(Q4Error::WrongActivationLength {
            actual: activation.len(),
        });
    }
    #[cfg(target_arch = "x86_64")]
    {
        if !std::is_x86_feature_detected!("avx2") {
            return Err(Q4Error::UnsupportedCpu);
        }
        // SAFETY: exact validated block lengths and runtime AVX2 detection.
        let integer_dots = unsafe { x86::dot(weights, activation) };
        let fp16_scale_bits = weights.map(|w| u16::from_le_bytes([w[144], w[145]]));
        Ok(Q4Dot {
            integer_dots,
            fp16_scale_bits,
            activation_f32_bits: u32::from_le_bytes(activation[..4].try_into().unwrap()),
        })
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        Err(Q4Error::UnsupportedCpu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original(case: usize) -> [u8; ORIGINAL_BYTES] {
        std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8)
    }

    fn activation(case: usize, pattern: usize) -> [u8; Q8_BYTES] {
        let mut a = [0u8; Q8_BYTES];
        a[..4].copy_from_slice(
            &[0x3f800000u32, 0x7fc12345u32][pattern % 2].to_le_bytes(),
        );
        for i in 0..256 {
            a[4 + i] = match pattern % 8 {
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
        a
    }

    fn original_oracle(w: &[u8; ORIGINAL_BYTES], a: &[u8; Q8_BYTES]) -> i32 {
        let mut sum = 0;
        for i in 0..256 {
            sum += i32::from(source_q3_integer(w, i))
                * i32::from(original_scale(w, i / 16))
                * i32::from(a[4 + i] as i8);
        }
        sum
    }

    #[test]
    fn source_wire_signed_q3_to_146_160_exact_96_by_16() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        for case in 0..96 {
            let mut w = original(case);
            w[108..110].copy_from_slice(
                &[0x3c00u16, 0xbc00, 0x7e01, 0x8000][case % 4].to_le_bytes(),
            );
            let a146 = transcode_q3_to_q4::<146>(&w).unwrap();
            let a160 = transcode_q3_to_q4::<160>(&w).unwrap();
            assert_eq!(&a160[..146], &a146);
            assert!(a160[146..].iter().all(|&x| x == 0));
            assert_eq!(&a146[144..146], &w[108..110]);
            for i in 0..256 {
                assert_eq!(q4_code(&a146, i), source_q3_integer(&w, i));
            }
            for i in 0..16 {
                assert_eq!(a146[128 + i] as i8, original_scale(&w, i));
            }
            for pattern in 0..16 {
                let q8 = activation(case, pattern);
                let oracle = original_oracle(&w, &q8);
                let one146 = dot_q4::<1, 146>([&a146], &q8).unwrap();
                let one160 = dot_q4::<1, 160>([&a160], &q8).unwrap();
                let four146 = dot_q4::<4, 146>([&a146; 4], &q8).unwrap();
                assert_eq!(one146.integer_dots, [oracle]);
                assert_eq!(one160.integer_dots, [oracle]);
                assert_eq!(four146.integer_dots, [oracle; 4]);
                assert_eq!(one146.fp16_scale_bits, [u16::from_le_bytes([w[108], w[109]])]);
                assert_eq!(one146.activation_f32_bits, u32::from_le_bytes(q8[..4].try_into().unwrap()));
            }
        }
    }

    #[test]
    fn invalid_inputs_and_sign_extremes() {
        let original = original(1);
        let expanded = transcode_q3_to_q4::<146>(&original).unwrap();
        let wide = transcode_q3_to_q4::<160>(&original).unwrap();
        let a = activation(0, 6);
        assert_eq!(
            dot_q4::<3, 146>([&expanded; 3], &a),
            Err(Q4Error::InvalidRowCount { actual: 3 })
        );
        assert_eq!(
            dot_q4::<1, 160>([&wide[..159]], &a),
            Err(Q4Error::WrongRowLength { row: 0, actual: 159 })
        );
        assert_eq!(
            dot_q4::<1, 146>([&expanded], &a[..291]),
            Err(Q4Error::WrongActivationLength { actual: 291 })
        );
        assert_eq!(
            transcode_q3_to_q4::<147>(&original),
            Err(Q4Error::InvalidFormatLength { actual: 147 })
        );
        assert_eq!(
            transcode_q3_to_q4::<146>(&original[..109]),
            Err(Q4Error::WrongSourceLength { actual: 109 })
        );
    }

    #[test]
    fn all_32_unaligned_weight_activation_offsets_and_immutable_sentinels() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let original = original(47);
        let block146 = transcode_q3_to_q4::<146>(&original).unwrap();
        let block160 = transcode_q3_to_q4::<160>(&original).unwrap();
        let act = activation(47, 7);
        let expected = original_oracle(&original, &act);
        for offset in 0..32 {
            let mut w146 = vec![0xa5u8; 146 + 64];
            let mut w160 = vec![0xa5u8; 160 + 64];
            let mut q8 = vec![0x5au8; Q8_BYTES + 64];
            w146[offset..offset + 146].copy_from_slice(&block146);
            w160[offset..offset + 160].copy_from_slice(&block160);
            q8[31 - offset..31 - offset + Q8_BYTES].copy_from_slice(&act);
            let before = (w146.clone(), w160.clone(), q8.clone());
            let a = &q8[31 - offset..31 - offset + Q8_BYTES];
            assert_eq!(
                dot_q4::<1, 146>([&w146[offset..offset + 146]], a).unwrap().integer_dots,
                [expected]
            );
            assert_eq!(
                dot_q4::<1, 160>([&w160[offset..offset + 160]], a).unwrap().integer_dots,
                [expected]
            );
            assert_eq!((w146, w160, q8), before);
        }
    }

    #[test]
    fn correct_negative_four_and_positive_three_extremes() {
        let mut source = [0u8; ORIGINAL_BYTES];
        source[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
        let mut w = transcode_q3_to_q4::<146>(&source).unwrap();
        assert_eq!(q4_code(&w, 0), -4);
        assert_eq!(q4_code(&w, 255), -4);
        for i in 0..256 {
            w[i / 2] |= (3 & 15) << (4 * (i & 1));
        }
        // Byte writes above may not replace existing negative nibbles;
        // use explicit overwrites for positive-three coverage.
        w[..128].fill(0x33);
        assert!((0..256).all(|i| q4_code(&w, i) == 3));
        w[..128].fill(0xcc);
        assert!((0..256).all(|i| q4_code(&w, i) == -4));
    }
}
