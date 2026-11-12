//! Research-only Q5-178: 256 signed five-bit coefficients per superblock.
//! Layout: low nibbles [0..128), high bits [128..160), sixteen signed i8
//! subgroup scales [160..176), unchanged little-endian FP16 [176..178).
//! An unsigned five-bit code c represents signed coefficient c - 16.
//! Not ggml Q5_K, not an ordinary Q3_K GGUF payload, not production.
//!
//! Tiled AVX2 shares each Q8_K activation vector and bias correction
//! between 1, 2 or 4 different weight rows. No dense decompression,
//! malloc, C ABI, model rewrite or stock-Q3 fallback.
#![deny(unsafe_op_in_unsafe_fn)]

#[path = "bpb3k_q4_pair228_avx2.rs"]
#[allow(dead_code)]
mod q4;

pub const Q3_BYTES: usize = 110;
pub const Q4_BYTES: usize = 146;
pub const Q5_BYTES: usize = 178;
pub const Q8_BYTES: usize = 292;
const HIGH_OFFSET: usize = 128;
const SCALE_OFFSET: usize = 160;
const FP16_OFFSET: usize = 176;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    BadSourceLen(usize),
    BadWeightsLen(usize),
    BadActivationsLen(usize),
    BadTileRows(usize),
    InvalidCode { index: usize, code: i8 },
    UnsupportedCpu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DotLanes {
    /// Eight independent ggml-compatible integer lanes, before FP32 FMA.
    pub lanes: [i32; 8],
    pub global_scale_f16_bits: u16,
    pub q8_scale_f32_bits: u32,
}

/// Pack independently supplied five-bit coefficients without quantizing them.
/// The caller owns the high-precision quantization/scaling policy. The 16
/// subgroup scales and global FP16 are supplied and preserved verbatim.
pub fn pack_signed(
    codes: &[i8; 256],
    scales: &[i8; 16],
    global_scale_f16_bits: u16,
) -> Result<[u8; Q5_BYTES], Error> {
    for (index, &code) in codes.iter().enumerate() {
        if !(-16..=15).contains(&code) {
            return Err(Error::InvalidCode { index, code });
        }
    }
    let mut out = [0u8; Q5_BYTES];
    for group in 0..8 {
        for lane in 0..16 {
            let first = group * 32 + lane;
            let second = first + 16;
            let lo = (i16::from(codes[first]) + 16) as u8;
            let hi = (i16::from(codes[second]) + 16) as u8;
            out[group * 16 + lane] = (lo & 15) | ((hi & 15) << 4);
        }
    }
    for (index, &code) in codes.iter().enumerate() {
        let high_bit = ((i16::from(code) + 16) as u8 >> 4) & 1;
        out[HIGH_OFFSET + index / 8] |= high_bit << (index % 8);
    }
    for (index, &scale) in scales.iter().enumerate() {
        out[SCALE_OFFSET + index] = scale as u8;
    }
    out[FP16_OFFSET..].copy_from_slice(&global_scale_f16_bits.to_le_bytes());
    Ok(out)
}

fn signed_at(weights: &[u8], index: usize) -> i8 {
    let packed = weights[(index / 32) * 16 + index % 16];
    let low = if index % 32 < 16 {
        packed & 15
    } else {
        packed >> 4
    };
    let high = (weights[HIGH_OFFSET + index / 8] >> (index % 8)) & 1;
    i8::try_from(low | (high << 4)).unwrap() - 16
}

/// Lossless Q4-146 -> Q5-178 transcode. This does NOT recover any precision
/// previously discarded from original FP16/BF16 weights.
pub fn transcode_q4(source: &[u8]) -> Result<[u8; Q5_BYTES], Error> {
    if source.len() != Q4_BYTES {
        return Err(Error::BadSourceLen(source.len()));
    }
    let codes = std::array::from_fn(|index| {
        let packed = source[(index / 32) * 16 + index % 16];
        let nibble = if index % 32 < 16 {
            packed & 15
        } else {
            packed >> 4
        };
        nibble as i8 - 4
    });
    let scales = std::array::from_fn(|i| source[128 + i] as i8);
    let scale = u16::from_le_bytes(source[144..146].try_into().unwrap());
    pack_signed(&codes, &scales, scale)
}

/// Lossless Q3_K -> Q5-178 transcode, via the established independent
/// Q3_K -> Q4-146 oracle. This is a *format* test, not Q5 quality gain.
pub fn transcode_q3(source: &[u8]) -> Result<[u8; Q5_BYTES], Error> {
    if source.len() != Q3_BYTES {
        return Err(Error::BadSourceLen(source.len()));
    }
    let q4 = q4::transcode_q4::<Q4_BYTES>(source).map_err(|_| Error::BadSourceLen(source.len()))?;
    transcode_q4(&q4)
}

/// Preserve IEEE 754 binary16 values exactly while widening to f32.
/// Used by the experimental batched decode path to match ggml semantics.
pub fn f16_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = u32::from((bits >> 10) & 31);
    let fraction = u32::from(bits & 0x03ff);
    if exponent == 0 {
        if fraction == 0 {
            return f32::from_bits(sign);
        }
        let value = (fraction as f32) * f32::from_bits(0x3380_0000);
        return if sign == 0 { value } else { -value };
    }
    if exponent == 31 {
        return f32::from_bits(sign | 0x7f80_0000 | (fraction << 13));
    }
    f32::from_bits(sign | ((exponent + 112) << 23) | (fraction << 13))
}

/// Scalar integer reference, independent of the AVX2 packed-bit decoder.
/// Preserves the eight lane partitions needed for ggml-style multiblock
/// floating-point FMA accumulation; it does not assert FP32 equivalence.
pub fn scalar_lanes(weights: &[u8], activation: &[u8]) -> Result<DotLanes, Error> {
    if weights.len() != Q5_BYTES {
        return Err(Error::BadWeightsLen(weights.len()));
    }
    if activation.len() != Q8_BYTES {
        return Err(Error::BadActivationsLen(activation.len()));
    }
    let mut lanes = [0i32; 8];
    for index in 0..256 {
        let lane = (index % 32) / 4;
        let scale = i32::from(weights[SCALE_OFFSET + index / 16] as i8);
        let activation_code = i32::from(activation[4 + index] as i8);
        lanes[lane] += i32::from(signed_at(weights, index)) * scale * activation_code;
    }
    Ok(DotLanes {
        lanes,
        global_scale_f16_bits: u16::from_le_bytes(
            weights[FP16_OFFSET..Q5_BYTES].try_into().unwrap(),
        ),
        q8_scale_f32_bits: u32::from_le_bytes(activation[0..4].try_into().unwrap()),
    })
}

#[cfg(target_arch = "x86_64")]
mod avx2 {
    use super::*;
    use core::arch::x86_64::*;

    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn tiled<const ROWS: usize>(
        weights: &[u8],
        activation: &[u8],
    ) -> [[i32; 8]; ROWS] {
        unsafe {
            let zero = _mm256_setzero_si256();
            let fifteen = _mm256_set1_epi8(15);
            let sixteen = _mm256_set1_epi8(16);
            let selector = _mm256_setr_epi8(
                0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3,
                3, 3, 3, 3,
            );
            let bit_selector = _mm256_setr_epi8(
                1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64,
                -128, 1, 2, 4, 8, 16, 32, 64, -128,
            );
            let mut accumulators = [zero; ROWS];
            for group in 0..8 {
                // ONE activation load and ONE bias product per 32-code group,
                // shared by every row of the microtile.
                let input =
                    _mm256_loadu_si256(activation.as_ptr().add(4 + group * 32).cast::<__m256i>());
                let bias = _mm256_maddubs_epi16(sixteen, input);
                for (row, accumulator) in accumulators.iter_mut().enumerate() {
                    let block = &weights[row * Q5_BYTES..(row + 1) * Q5_BYTES];
                    let packed = _mm_loadu_si128(block.as_ptr().add(group * 16).cast::<__m128i>());
                    let broadcast = _mm256_broadcastsi128_si256(packed);
                    let low = _mm256_and_si256(broadcast, fifteen);
                    let high = _mm256_and_si256(_mm256_srli_epi16(broadcast, 4), fifteen);
                    let low_nibbles = _mm256_permute2x128_si256(low, high, 0x20);

                    // Four consecutive high-bit bytes encode all 32 codes.
                    // VPSHUFB extracts each byte in its native 128-bit lane;
                    // no 256-element dense intermediate is created.
                    let h = u32::from_le_bytes(
                        block[HIGH_OFFSET + group * 4..HIGH_OFFSET + group * 4 + 4]
                            .try_into()
                            .unwrap(),
                    );
                    let selected = _mm256_shuffle_epi8(_mm256_set1_epi32(h as i32), selector);
                    let masked = _mm256_and_si256(selected, bit_selector);
                    let top = _mm256_andnot_si256(_mm256_cmpeq_epi8(masked, zero), sixteen);
                    let unsigned_codes = _mm256_or_si256(low_nibbles, top);
                    let signed_pairs =
                        _mm256_sub_epi16(_mm256_maddubs_epi16(unsigned_codes, input), bias);
                    let s0 = i16::from(block[SCALE_OFFSET + group * 2] as i8);
                    let s1 = i16::from(block[SCALE_OFFSET + group * 2 + 1] as i8);
                    let scales = _mm256_inserti128_si256(
                        _mm256_castsi128_si256(_mm_set1_epi16(s0)),
                        _mm_set1_epi16(s1),
                        1,
                    );
                    *accumulator =
                        _mm256_add_epi32(*accumulator, _mm256_madd_epi16(signed_pairs, scales));
                }
            }
            std::array::from_fn(|row| {
                let mut lanes = [0i32; 8];
                _mm256_storeu_si256(lanes.as_mut_ptr().cast::<__m256i>(), accumulators[row]);
                lanes
            })
        }
    }

    /// Fused one-row batched decode. Builds the constant VPSHUFB masks
    /// exactly once across all superblocks and keeps the eight independent
    /// floating-point FMA chains alive until the terminal ggml reduction.
    /// No repeated AVX2 feature detection or block-length validation.
    ///
    /// SAFETY: caller must have checked AVX2 support, n > 0, n % 256 == 0
    /// and exact 178*blocks / 292*blocks source spans before entering.
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn batched_single_row_dot(
        weights: &[u8],
        activations: &[u8],
        blocks: usize,
    ) -> f32 {
        unsafe {
            let zero = _mm256_setzero_si256();
            let nibble_mask = _mm256_set1_epi8(15);
            let sixteen = _mm256_set1_epi8(16);
            let selector = _mm256_setr_epi8(
                0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3,
                3, 3, 3, 3,
            );
            let bit_selector = _mm256_setr_epi8(
                1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64,
                -128, 1, 2, 4, 8, 16, 32, 64, -128,
            );
            let mut fp32_lanes = [0f32; 8];
            for block in 0..blocks {
                let w = weights.as_ptr().add(block * Q5_BYTES);
                let a = activations.as_ptr().add(block * Q8_BYTES);
                let mut integer_lanes = zero;
                for group in 0..8 {
                    let input = _mm256_loadu_si256(a.add(4 + group * 32).cast::<__m256i>());
                    // Bias is applied at the exact 16-bit pair stage before
                    // i8 subgroup scales; neither saturating pair products
                    // nor ggml FP32 FMA association may be changed.
                    let bias = _mm256_maddubs_epi16(sixteen, input);
                    let packed = _mm_loadu_si128(w.add(group * 16).cast::<__m128i>());
                    let broadcast = _mm256_broadcastsi128_si256(packed);
                    let low = _mm256_and_si256(broadcast, nibble_mask);
                    let high = _mm256_and_si256(_mm256_srli_epi16(broadcast, 4), nibble_mask);
                    let lower = _mm256_permute2x128_si256(low, high, 0x20);
                    let high_ptr = w.add(HIGH_OFFSET + group * 4);
                    let high_word = u32::from_le_bytes([
                        *high_ptr,
                        *high_ptr.add(1),
                        *high_ptr.add(2),
                        *high_ptr.add(3),
                    ]);
                    let selected =
                        _mm256_shuffle_epi8(_mm256_set1_epi32(high_word as i32), selector);
                    let high_mask = _mm256_and_si256(selected, bit_selector);
                    let high_codes =
                        _mm256_andnot_si256(_mm256_cmpeq_epi8(high_mask, zero), sixteen);
                    let unsigned_codes = _mm256_or_si256(lower, high_codes);
                    let pair = _mm256_sub_epi16(_mm256_maddubs_epi16(unsigned_codes, input), bias);
                    let scale0 = i16::from(*w.add(SCALE_OFFSET + group * 2) as i8);
                    let scale1 = i16::from(*w.add(SCALE_OFFSET + group * 2 + 1) as i8);
                    let subgroup_scales = _mm256_inserti128_si256(
                        _mm256_castsi128_si256(_mm_set1_epi16(scale0)),
                        _mm_set1_epi16(scale1),
                        1,
                    );
                    integer_lanes =
                        _mm256_add_epi32(integer_lanes, _mm256_madd_epi16(pair, subgroup_scales));
                }
                let mut lanes = [0i32; 8];
                _mm256_storeu_si256(lanes.as_mut_ptr().cast::<__m256i>(), integer_lanes);
                let global_bits =
                    u16::from_le_bytes([*w.add(FP16_OFFSET), *w.add(FP16_OFFSET + 1)]);
                let q8_bits = u32::from_le_bytes([*a, *a.add(1), *a.add(2), *a.add(3)]);
                let d = f32::from_bits(q8_bits) * f16_to_f32(global_bits);
                for (acc, lane) in fp32_lanes.iter_mut().zip(lanes) {
                    *acc = d.mul_add(lane as f32, *acc);
                }
            }
            // hsum_float_8: low/high lanes, 0+2 and 1+3, final sum.
            let halves = [
                fp32_lanes[0] + fp32_lanes[4],
                fp32_lanes[1] + fp32_lanes[5],
                fp32_lanes[2] + fp32_lanes[6],
                fp32_lanes[3] + fp32_lanes[7],
            ];
            (halves[0] + halves[2]) + (halves[1] + halves[3])
        }
    }
}

/// Native AVX2 1/2/4-row microtile; one Q8_K activation is reused by every
/// row. There is no scalar fallback; unsupported CPUs fail explicitly.
pub fn dot_tiled<const ROWS: usize>(
    weights: &[u8],
    activation: &[u8],
) -> Result<[DotLanes; ROWS], Error> {
    if ROWS != 1 && ROWS != 2 && ROWS != 4 {
        return Err(Error::BadTileRows(ROWS));
    }
    if weights.len() != Q5_BYTES * ROWS {
        return Err(Error::BadWeightsLen(weights.len()));
    }
    if activation.len() != Q8_BYTES {
        return Err(Error::BadActivationsLen(activation.len()));
    }
    #[cfg(target_arch = "x86_64")]
    {
        if !std::is_x86_feature_detected!("avx2") {
            return Err(Error::UnsupportedCpu);
        }
        let lanes = unsafe { avx2::tiled::<ROWS>(weights, activation) };
        Ok(std::array::from_fn(|row| {
            let base = row * Q5_BYTES;
            DotLanes {
                lanes: lanes[row],
                global_scale_f16_bits: u16::from_le_bytes(
                    weights[base + FP16_OFFSET..base + Q5_BYTES]
                        .try_into()
                        .unwrap(),
                ),
                q8_scale_f32_bits: u32::from_le_bytes(activation[..4].try_into().unwrap()),
            }
        }))
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        Err(Error::UnsupportedCpu)
    }
}

/// Batched decode for the source-pinned ggml one-row FFI. The fast
/// integer AVX2 unpack and floating-point association are identical to
/// `dot_tiled::<1>`; only the constant setup, feature check and per-block
/// validation are hoisted out of the superblock loop.
///
/// # Safety
///
/// x86_64 AVX2 must already have been checked by the caller, and
/// `weights.len() == blocks * 178`, `activations.len() == blocks * 292`,
/// `blocks > 0`. These invariants are enforced by the external FFI
/// before calling this function; never call on untrusted slices.
#[cfg(target_arch = "x86_64")]
pub unsafe fn dot_batched_single_avx2_prechecked(
    weights: &[u8],
    activations: &[u8],
    blocks: usize,
) -> f32 {
    unsafe { avx2::batched_single_row_dot(weights, activations, blocks) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(seed: usize) -> ([i8; 256], [i8; 16], [u8; Q8_BYTES]) {
        let codes = std::array::from_fn(|i| match seed % 4 {
            0 => (i % 32) as i8 - 16,
            1 => {
                if i % 2 == 0 {
                    -16
                } else {
                    15
                }
            }
            2 => (i.wrapping_mul(19).wrapping_add(seed) % 32) as i8 - 16,
            _ => 0,
        });
        let scales = std::array::from_fn(|i| if i % 2 == 0 { -128 } else { 127 });
        let mut activations = [0u8; Q8_BYTES];
        activations[..4].copy_from_slice(&0x7fc12345u32.to_le_bytes());
        for i in 0..256 {
            activations[4 + i] = match seed % 5 {
                0 => 128,
                1 => 127,
                2 => {
                    if i % 2 == 0 {
                        255
                    } else {
                        128
                    }
                }
                3 => i as u8,
                _ => i.wrapping_mul(71).wrapping_add(seed) as u8,
            };
        }
        (codes, scales, activations)
    }

    #[test]
    fn full_five_bit_domain_and_layout() {
        let (codes, scales, _) = fixture(0);
        let bytes = pack_signed(&codes, &scales, 0x7e01).unwrap();
        assert_eq!(bytes.len(), 178);
        assert_eq!(bytes[176..178], 0x7e01u16.to_le_bytes());
        assert_eq!(bytes[160] as i8, -128);
        assert_eq!(bytes[161] as i8, 127);
        for (i, code) in codes.iter().enumerate() {
            assert_eq!(signed_at(&bytes, i), *code, "code {i}");
        }
        let (_, _, q8) = fixture(0);
        let d = scalar_lanes(&bytes, &q8).unwrap();
        assert_eq!(d.global_scale_f16_bits, 0x7e01);
        assert_eq!(d.q8_scale_f32_bits, 0x7fc12345);
    }

    #[test]
    fn q3_and_q4_transcodes_preserve_exact_integer_oracle() {
        for seed in 0..64 {
            let mut original: [u8; Q3_BYTES] =
                std::array::from_fn(|i| i.wrapping_mul(37).wrapping_add(seed * 19) as u8);
            original[108..110]
                .copy_from_slice(&[0x3c00u16, 0x8000, 0x7e01, 0xbc00][seed % 4].to_le_bytes());
            let via_q3 = transcode_q3(&original).unwrap();
            let q4_block = q4::transcode_q4::<Q4_BYTES>(&original).unwrap();
            let via_q4 = transcode_q4(&q4_block).unwrap();
            assert_eq!(via_q3, via_q4);
            assert_eq!(&via_q3[176..178], &original[108..110]);
            for i in 0..256 {
                assert_eq!(signed_at(&via_q3, i), q4::original_q3_code(&original, i));
            }
            let (_, _, q8) = fixture(seed);
            assert_eq!(
                scalar_lanes(&via_q3, &q8)
                    .unwrap()
                    .lanes
                    .iter()
                    .sum::<i32>(),
                q4::original_integer_dot(&original, &q8),
                "seed {seed}",
            );
        }
    }

    #[test]
    fn scalar_and_genuine_avx2_tiled_rows_1_2_4_agree() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        for seed in 0..40 {
            let (_, _, activation) = fixture(seed);
            let mut blocks = Vec::new();
            for row in 0..4 {
                let (codes, scales, _) = fixture(seed + row);
                blocks.extend_from_slice(
                    &pack_signed(&codes, &scales, [0x3c00, 0xbc00, 0x7e01, 0x8000][row]).unwrap(),
                );
            }
            let expected: Vec<_> = (0..4)
                .map(|row| scalar_lanes(&blocks[row * 178..(row + 1) * 178], &activation).unwrap())
                .collect();
            assert_eq!(
                dot_tiled::<1>(&blocks[..178], &activation).unwrap(),
                [expected[0]]
            );
            assert_eq!(
                dot_tiled::<2>(&blocks[..356], &activation).unwrap(),
                [expected[0], expected[1]],
            );
            assert_eq!(
                dot_tiled::<4>(&blocks, &activation).unwrap(),
                [expected[0], expected[1], expected[2], expected[3]],
            );
            // No alignment precondition on user-provided weight/activation slices.
            let mut unaligned = vec![0x55u8];
            unaligned.extend_from_slice(&blocks);
            let mut unaligned_q8 = vec![0xabu8];
            unaligned_q8.extend_from_slice(&activation);
            assert_eq!(
                dot_tiled::<4>(&unaligned[1..], &unaligned_q8[1..]).unwrap(),
                [expected[0], expected[1], expected[2], expected[3]],
            );
        }
    }

    #[test]
    fn rejects_malformed_and_out_of_range_inputs() {
        let (mut codes, scales, activation) = fixture(0);
        codes[27] = 16;
        assert_eq!(
            pack_signed(&codes, &scales, 0x3c00),
            Err(Error::InvalidCode {
                index: 27,
                code: 16
            }),
        );
        codes[27] = -17;
        assert_eq!(
            pack_signed(&codes, &scales, 0x3c00),
            Err(Error::InvalidCode {
                index: 27,
                code: -17
            }),
        );
        assert_eq!(transcode_q3(&[0u8; 109]), Err(Error::BadSourceLen(109)));
        assert_eq!(transcode_q4(&[0u8; 145]), Err(Error::BadSourceLen(145)));
        let valid = [0u8; Q5_BYTES];
        assert_eq!(
            scalar_lanes(&valid[..177], &activation),
            Err(Error::BadWeightsLen(177))
        );
        assert_eq!(
            scalar_lanes(&valid, &activation[..291]),
            Err(Error::BadActivationsLen(291))
        );
        assert_eq!(
            dot_tiled::<3>(&[0u8; 534], &activation),
            Err(Error::BadTileRows(3))
        );
        assert_eq!(
            dot_tiled::<1>(&valid[..177], &activation),
            Err(Error::BadWeightsLen(177))
        );
    }
}
