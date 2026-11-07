//! BPB3-K direct Boolean bitplane dot product (research-only, CPU reference).
//!
//! This is a compute path over the unchanged 110-byte BPB3-K wire format:
//! four groups of three u64 bitplanes and the inherited ggml Q3_K scales.
//! No 256-element dense weight buffer is materialized.
//!
//! Supported inputs are bipolar only (+1/-1, bit 1 meaning -1).
//! No arbitrary activations, SIMD, GPU, or end-to-end attention are claimed.
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
