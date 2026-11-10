//! BPB3-K direct Boolean bitplane dot product (research-only, CPU reference).
//!
//! This is a compute path over the unchanged 110-byte BPB3-K wire format:
//! four groups of three u64 bitplanes and the inherited ggml Q3_K scales.
//! No 256-element dense weight buffer is materialized.
//!
//! Two independently guarded APIs: the original bipolar (±1) popcount dot,
//! and a new signed-Q8_K (−128..127) popcount bitplane dot. Neither path
//! establishes SIMD speed, end-to-end model parity, or attention replacement.
//!
//! Official upstream pin: ggml-org/llama.cpp
//! f7591a6a01f9273de03331732bdc0b39721168be.

#![forbid(unsafe_code)]

#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod bpb3k;

#[path = "gguf_k.rs"]
#[allow(dead_code)]
mod gguf_k;

pub use bpb3k::Error;

const GROUPS: usize = 4;
const LANES_PER_GROUP: usize = 64;
const SUBGROUPS_PER_GROUP: usize = 4;
const LANES_PER_SUBGROUP: usize = 16;

/// Integer coefficient dot and the original FP16 global scale bits.
///
/// The exact mathematical value is weighted_integer times the FP16 scale.
/// Float reduction order and rounding are intentionally not specified here.
/// Even non-finite FP16 metadata remains unchanged; callers must check it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectDot {
    pub weighted_integer: i32,
    pub global_scale_f16_bits: u16,
}

fn read_u64_le(raw: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        raw[offset..offset + 8]
            .try_into()
            .expect("fixed in-bounds plane after length check"),
    )
}

fn signed_subgroup_scale(scales: &[u8], subgroup: usize) -> i32 {
    // Sixteen signed 6-bit Q3_K scales stored in twelve bytes.
    let low = if subgroup < 8 {
        scales[subgroup] & 15
    } else {
        scales[subgroup - 8] >> 4
    };
    let high = (scales[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
    i32::from(low | (high << 4)) - 32
}

/// Compute the 256-lane bipolar dot directly from BPB3-K planes.
///
/// A 1 input bit denotes -1. For each lane, the stored coefficient is:
/// sign=0 => magnitude (0..3); sign=1 => -(magnitude+1) (-4..-1).
///
/// E = !(negative XOR input_negative). For each amplitude mask M,
/// C(M)=2*popcount(E & M)-popcount(M).
/// The mandatory negative-offset correction is
/// 2*popcount(negative & input_negative)-popcount(negative).
///
/// Each 16-lane subgroup receives its own original signed Q3_K scale.
/// The integer bound is 256 * 4 * 32 = 32768 (safe in i32).
///
/// No heap allocation, floating-point weight decoding, or dense temporary
/// occurs on this path. This is not a measured performance result.
pub fn dot_bpb3k_bipolar(raw: &[u8], input_negative: &[u64; GROUPS]) -> Result<DirectDot, Error> {
    if raw.len() != bpb3k::BYTES {
        return Err(Error::WrongLength { actual: raw.len() });
    }
    let scales = &raw[bpb3k::PLANES_BYTES..bpb3k::PLANES_BYTES + 12];
    let global_scale_f16_bits = u16::from_le_bytes([raw[108], raw[109]]);
    let mut weighted_integer = 0i32;

    for (group, &x_negative) in input_negative.iter().enumerate() {
        let offset = group * 24;
        let negative = read_u64_le(raw, offset);
        let bit0 = read_u64_le(raw, offset + 8);
        let bit1 = read_u64_le(raw, offset + 16);
        let equal_sign = !(negative ^ x_negative);

        for within_group in 0..SUBGROUPS_PER_GROUP {
            let subgroup = group * SUBGROUPS_PER_GROUP + within_group;
            let lane_mask =
                ((1u64 << LANES_PER_SUBGROUP) - 1) << (within_group * LANES_PER_SUBGROUP);
            let lo = bit0 & lane_mask;
            let hi = bit1 & lane_mask;
            let neg = negative & lane_mask;
            let magnitude_low = 2 * (equal_sign & lo).count_ones() as i32 - lo.count_ones() as i32;
            let magnitude_high = 2 * (equal_sign & hi).count_ones() as i32 - hi.count_ones() as i32;
            let negative_offset =
                2 * (neg & x_negative).count_ones() as i32 - neg.count_ones() as i32;
            let subgroup_dot = magnitude_low + 2 * magnitude_high + negative_offset;
            debug_assert!(subgroup_dot.abs() <= (LANES_PER_SUBGROUP * 4) as i32);
            weighted_integer += subgroup_dot * signed_subgroup_scale(scales, subgroup);
        }
    }

    debug_assert!(weighted_integer.abs() <= (GROUPS * LANES_PER_GROUP * 4 * 32) as i32);
    Ok(DirectDot {
        weighted_integer,
        global_scale_f16_bits,
    })
}

/// Full ggml Q8_K superblock: f32 scale, 256 signed i8 codes and 16 i16
/// cached subtotals. Subtotals are not needed for this exact direct dot.
pub const Q8_K_BYTES: usize = 4 + 256 + 32;

/// Input-domain failures for the general signed Q8_K direct dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Q8DotError {
    WrongWeightLength { actual: usize },
    WrongActivationLength { actual: usize },
}

/// An exact integer dot for Q3_K-equivalent BPB3-K and arbitrary signed Q8_K
/// activations, with both source scale bit patterns preserved unchanged.
///
/// This is intentionally not an f32 result: ggml AVX2/tiled accumulation and
/// FMA order are distinct contracts. Consumers must qualify any chosen f32
/// epilogue independently against the actual model runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Q8DirectDot {
    pub weighted_integer: i32,
    pub global_scale_f16_bits: u16,
    pub activation_scale_f32_bits: u32,
}

/// A single 16-coefficient subgroup product from Boolean weight and activation
/// bitplanes. Signed activation bytes use two's-complement bit seven.
///
/// The coefficient is q=(1-2*s)*(b0+2*b1)-s, including the essential -1
/// offset on negative BPB3-K weights. For a given activation bitplane A:
///
/// sum(q_i*A_i) = pc(b0&A) - 2*pc(s&b0&A)
///              + 2*pc(b1&A) - 4*pc(s&b1&A) - pc(s&A).
///
/// Accumulate bitplanes 0..6 at +2^k and bitplane 7 at -128. Every bit
/// count and shift is exact; no per-coefficient multiplication is performed.
fn signed_q8_subgroup_dot(sign: u16, bit0: u16, bit1: u16, q8: &[u8]) -> i32 {
    debug_assert_eq!(q8.len(), LANES_PER_SUBGROUP);
    let mut activations = [0u16; 8];
    for (lane, &value) in q8.iter().enumerate() {
        for (bit, plane) in activations.iter_mut().enumerate() {
            *plane |= u16::from((value >> bit) & 1) << lane;
        }
    }

    let mut sum = 0i32;
    for (bit, &activation) in activations.iter().enumerate() {
        let s = sign & activation;
        let low = bit0 & activation;
        let high = bit1 & activation;
        let weight_sum = low.count_ones() as i32 - 2 * (s & bit0).count_ones() as i32
            + 2 * high.count_ones() as i32
            - 4 * (s & bit1).count_ones() as i32
            - s.count_ones() as i32;
        let contribution = weight_sum << bit;
        if bit == 7 {
            sum -= contribution;
        } else {
            sum += contribution;
        }
    }
    debug_assert!(sum.abs() <= (LANES_PER_SUBGROUP * 4 * 128) as i32);
    sum
}

/// General signed-Q8_K Boolean dot without a dense weight buffer or per-weight
/// multiply. The only integer multiplications are the 16 original Q3_K
/// subgroup scales, not 256 scalar coefficient/activation MACs.
///
/// Both inputs must be complete, unmodified blocks (110 and 292 bytes).
/// Q8_K's optional cached bsums are not read; all 256 signed activation codes
/// contribute directly. Max magnitude <= 256 * 4 * 32 * 128 = 4_194_304.
/// No heap allocation, floating-point arithmetic or unsafe code is used.
pub fn dot_bpb3k_q8(raw: &[u8], q8: &[u8]) -> Result<Q8DirectDot, Q8DotError> {
    if raw.len() != bpb3k::BYTES {
        return Err(Q8DotError::WrongWeightLength { actual: raw.len() });
    }
    if q8.len() != Q8_K_BYTES {
        return Err(Q8DotError::WrongActivationLength { actual: q8.len() });
    }
    let scales = &raw[bpb3k::PLANES_BYTES..bpb3k::PLANES_BYTES + 12];
    let mut total = 0i32;
    for group in 0..GROUPS {
        let base = group * 24;
        let negative = read_u64_le(raw, base);
        let low = read_u64_le(raw, base + 8);
        let high = read_u64_le(raw, base + 16);
        for within in 0..SUBGROUPS_PER_GROUP {
            let subgroup = group * SUBGROUPS_PER_GROUP + within;
            let shift = within * LANES_PER_SUBGROUP;
            let act =
                &q8[4 + subgroup * LANES_PER_SUBGROUP..4 + (subgroup + 1) * LANES_PER_SUBGROUP];
            let sum = signed_q8_subgroup_dot(
                (negative >> shift) as u16,
                (low >> shift) as u16,
                (high >> shift) as u16,
                act,
            );
            total += sum * signed_subgroup_scale(scales, subgroup);
        }
    }
    debug_assert!(total.abs() <= 4_194_304);
    Ok(Q8DirectDot {
        weighted_integer: total,
        global_scale_f16_bits: u16::from_le_bytes([raw[108], raw[109]]),
        activation_scale_f32_bits: u32::from_le_bytes(q8[..4].try_into().unwrap()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q3_fixture(case: usize, global_scale: u16) -> [u8; bpb3k::BYTES] {
        let mut raw: [u8; bpb3k::BYTES] =
            std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8);
        raw[108..110].copy_from_slice(&global_scale.to_le_bytes());
        raw
    }

    fn bipolar_masks(case: usize, pattern: usize) -> [u64; GROUPS] {
        std::array::from_fn(|group| match pattern {
            0 => 0,
            1 => u64::MAX,
            2 => 0xaaaa_aaaa_aaaa_aaaa,
            3 => 0x5555_5555_5555_5555,
            4 => 1u64 << ((case + group) % 64),
            5 => !(1u64 << ((case + group) % 64)),
            _ => {
                // Fixed SplitMix64-style mixing (not a security RNG).
                let mut z = (case as u64)
                    .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                    .wrapping_add((pattern as u64) << 16)
                    .wrapping_add(group as u64);
                z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                z ^ (z >> 31)
            }
        })
    }

    fn expected_dot_from_original_q3(raw_q3: &[u8; bpb3k::BYTES], inputs: &[u64; GROUPS]) -> f64 {
        // Independent reference on original Q3_K bytes, then scalar dense dot.
        let weights = gguf_k::decode_q3_k(raw_q3).expect("valid Q3_K fixture");
        weights
            .iter()
            .enumerate()
            .map(|(i, &weight)| {
                let input_negative =
                    ((inputs[i / LANES_PER_GROUP] >> (i % LANES_PER_GROUP)) & 1) != 0;
                f64::from(weight) * if input_negative { -1.0 } else { 1.0 }
            })
            .sum()
    }

    fn q3_q8_integer_oracle(q3: &[u8; bpb3k::BYTES], q8: &[u8; Q8_K_BYTES]) -> i32 {
        // Independent, original Q3_K address arithmetic. Never inspects the
        // BPB3-K planes or the Boolean-popcount implementation.
        let mut result = 0i32;
        for i in 0..256 {
            let half = i / 128;
            let plane = (i % 128) / 32;
            let local = i % 32;
            let mag = (q3[32 + half * 32 + local] >> (plane * 2)) & 3;
            let sign_set = (q3[local] >> (half * 4 + plane)) & 1 == 0;
            let weight = i32::from(mag) - if sign_set { 4 } else { 0 };
            let subgroup = i / 16;
            let scale = signed_subgroup_scale(&q3[96..108], subgroup);
            let activation = i32::from(q8[4 + i] as i8);
            result += weight * scale * activation;
        }
        result
    }

    fn q8_fixture(case: usize, pattern: usize, scale: u32) -> [u8; Q8_K_BYTES] {
        let mut block = [0u8; Q8_K_BYTES];
        block[..4].copy_from_slice(&scale.to_le_bytes());
        for i in 0..256 {
            block[4 + i] = match pattern {
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
                6 => (i as u8).wrapping_add(case as u8),
                _ => {
                    let mut v = (case as u32) ^ (i as u32).wrapping_mul(0x9e37_79b9);
                    v ^= v << 13;
                    v ^= v >> 17;
                    v ^= v << 5;
                    v as u8
                }
            };
        }
        block
    }

    #[test]
    fn q8_boolean_all_signed_codes_and_scale_metadata_match_original_q3_oracle() {
        // Scale metadata crosses every sign/zero/subnormal/NaN bit pattern
        // without imposing a floating-point reduction contract.
        let meta = [
            (0x3c00, 0x3f800000),
            (0xbc00, 0xbf800000),
            (1, 0x00000001),
            (0x8000, 0x80000000),
            (0x7c00, 0x7f800000),
            (0x7e01, 0x7fc12345),
        ];
        for case in 0..96 {
            let (weight_scale, activation_scale) = meta[case % meta.len()];
            let raw = q3_fixture(case, weight_scale);
            let converted = bpb3k::q3_to_bpb3k(&raw).unwrap();
            for pattern in 0..16 {
                let activation = q8_fixture(case, pattern, activation_scale);
                let result = dot_bpb3k_q8(&converted, &activation).unwrap();
                assert_eq!(
                    result.weighted_integer,
                    q3_q8_integer_oracle(&raw, &activation),
                    "case={case} pattern={pattern}"
                );
                assert_eq!(result.global_scale_f16_bits, weight_scale);
                assert_eq!(result.activation_scale_f32_bits, activation_scale);
            }
        }
    }

    #[test]
    fn q8_boolean_extreme_negative_four_and_128_activation_bounds() {
        // Original Q3_K all-zero bitplanes: coefficient -4, scale -32.
        let mut original = [0u8; bpb3k::BYTES];
        original[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
        let weights = bpb3k::q3_to_bpb3k(&original).unwrap();
        let mut activations = q8_fixture(0, 0, 0x3f800000);
        for (code, expected) in [(128u8, -4_194_304i32), (127, 4_161_536), (0, 0)] {
            activations[4..260].fill(code);
            assert_eq!(
                dot_bpb3k_q8(&weights, &activations)
                    .unwrap()
                    .weighted_integer,
                expected,
                "activation={code}"
            );
        }
    }

    #[test]
    fn q8_boolean_bipolar_specialization_agrees_with_prior_word_kernel() {
        for case in 0..32 {
            let q3 = q3_fixture(case, 0x3c00);
            let weights = bpb3k::q3_to_bpb3k(&q3).unwrap();
            let mut activations = q8_fixture(case, 5, 0x3f800000);
            let mut negatives = [0u64; GROUPS];
            for i in 0..256 {
                // Preserve a deterministic sign pattern.
                let negative = (i + case) % 3 == 0;
                activations[4 + i] = if negative { 255 } else { 1 };
                if negative {
                    negatives[i / 64] |= 1u64 << (i % 64);
                }
            }
            let q8 = dot_bpb3k_q8(&weights, &activations).unwrap();
            let bipolar = dot_bpb3k_bipolar(&weights, &negatives).unwrap();
            assert_eq!(q8.weighted_integer, bipolar.weighted_integer);
        }
    }

    #[test]
    fn q8_boolean_rejects_truncated_and_extended_inputs() {
        let weights = [0u8; bpb3k::BYTES];
        let q8 = [0u8; Q8_K_BYTES];
        for len in [0, 1, 96, 109, 111, 256] {
            assert_eq!(
                dot_bpb3k_q8(&vec![0u8; len], &q8),
                Err(Q8DotError::WrongWeightLength { actual: len })
            );
        }
        for len in [0, 1, 4, 260, 291, 293, 512] {
            assert_eq!(
                dot_bpb3k_q8(&weights, &vec![0u8; len]),
                Err(Q8DotError::WrongActivationLength { actual: len })
            );
        }
    }

    #[test]
    fn direct_dot_matches_independent_q3_scalar_oracle() {
        // These binary power-of-two FP16 scales reconstruct exactly in f32.
        let scale_cases = [
            (0x3c00u16, 1.0f64),
            (0xbc00u16, -1.0),
            (0x3800u16, 0.5),
            (0x0001u16, 2.0f64.powi(-24)),
            (0x0000u16, 0.0),
            (0x8000u16, -0.0),
        ];
        for case in 0..36 {
            let (scale_bits, scale) = scale_cases[case % scale_cases.len()];
            let q3 = q3_fixture(case, scale_bits);
            let bpb = bpb3k::q3_to_bpb3k(&q3).unwrap();
            for pattern in 0..32 {
                let input_negative = bipolar_masks(case, pattern);
                let result = dot_bpb3k_bipolar(&bpb, &input_negative).unwrap();
                assert_eq!(result.global_scale_f16_bits, scale_bits);
                let oracle = expected_dot_from_original_q3(&q3, &input_negative);
                let actual = f64::from(result.weighted_integer) * scale;
                assert_eq!(
                    actual, oracle,
                    "case={case} pattern={pattern} scale={scale_bits:#06x}"
                );
            }
        }
    }

    #[test]
    fn correction_for_negative_four_is_not_lost() {
        // Q3_K zeros encode q=-4, signed subgroup scale=-32.
        // With d=1, every original weight equals +128.
        let mut raw_q3 = [0u8; bpb3k::BYTES];
        raw_q3[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
        let bpb = bpb3k::q3_to_bpb3k(&raw_q3).unwrap();
        let positive = dot_bpb3k_bipolar(&bpb, &[0; GROUPS]).unwrap();
        let negative = dot_bpb3k_bipolar(&bpb, &[u64::MAX; GROUPS]).unwrap();
        assert_eq!(positive.weighted_integer, 256 * 128);
        assert_eq!(negative.weighted_integer, -(256 * 128));
        let balanced = dot_bpb3k_bipolar(&bpb, &[0xaaaa_aaaa_aaaa_aaaa; GROUPS]).unwrap();
        assert_eq!(balanced.weighted_integer, 0);
    }

    #[test]
    fn wrong_length_fails_closed() {
        for len in [0, 1, 24, 96, 109, 111, 144, 256] {
            assert_eq!(
                dot_bpb3k_bipolar(&vec![0u8; len], &[0; GROUPS]),
                Err(Error::WrongLength { actual: len })
            );
        }
    }

    #[test]
    fn global_scale_bits_are_preserved_without_floating_rounding() {
        for bits in [0u16, 0x8000, 1, 0x7bff, 0x7c00, 0x7e00, 0xfc00] {
            let q3 = q3_fixture(5, bits);
            let bpb = bpb3k::q3_to_bpb3k(&q3).unwrap();
            assert_eq!(
                dot_bpb3k_bipolar(&bpb, &[0; GROUPS])
                    .unwrap()
                    .global_scale_f16_bits,
                bits
            );
        }
    }
}
