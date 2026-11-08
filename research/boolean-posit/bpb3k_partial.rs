//! BPB3-K selective-access bipolar dot reference for honest BPB3-X
//! partial-span comparison. Reads the selected groups' bitplanes from
//! the original 110-byte BPB3-K layout; reads only the signed subgroup
//! scale bytes addressed for the chosen range, plus original FP16.
//! The code neither expands all weights nor measures physical I/O.
//!
//! A real optimized Q3_K / BPB3-K SIMD competitor is still required.
#![forbid(unsafe_code)]

pub const BYTES: usize = 110;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    WrongLength { actual: usize },
    InvalidSpan { first_group: usize, groups: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialDot {
    pub weighted_integer: i32,
    pub global_scale_f16_bits: u16,
    pub first_coefficient: usize,
    pub coefficients: usize,
}

fn read_word(raw: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        raw[offset..offset + 8]
            .try_into()
            .expect("validated 110-byte BPB3-K input"),
    )
}

fn subgroup_scale(scales: &[u8], subgroup: usize) -> i32 {
    let low = if subgroup < 8 {
        scales[subgroup] & 15
    } else {
        scales[subgroup - 8] >> 4
    };
    let high = (scales[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
    i32::from(low | high << 4) - 32
}

/// Compute selected contiguous 64-coefficient groups on BPB3-K directly.
///
/// Supported group counts: 1, 2 and 4. Starts must lie within the
/// 256-coefficient source superblock. An unselected group's 3 planes
/// are not accessed by the arithmetic path. Logical access counts are
/// *not* cache-line, DRAM or runtime benchmark results.
pub fn dot_selected_bpb3k(
    raw: &[u8],
    first_group: usize,
    groups: usize,
    input_negative: &[u64; 4],
) -> Result<PartialDot, Error> {
    if raw.len() != BYTES {
        return Err(Error::WrongLength { actual: raw.len() });
    }
    if !matches!(groups, 1 | 2 | 4) || first_group >= 4 || first_group + groups > 4 {
        return Err(Error::InvalidSpan {
            first_group,
            groups,
        });
    }
    let scales = &raw[96..108];
    let mut weighted_integer = 0i32;
    for group in first_group..first_group + groups {
        let offset = group * 24;
        let negative = read_word(raw, offset);
        let bit0 = read_word(raw, offset + 8);
        let bit1 = read_word(raw, offset + 16);
        let activation = input_negative[group];
        let equal_sign = !(negative ^ activation);
        for subgroup in 0..4 {
            let mask = 0xffffu64 << (16 * subgroup);
            let lo = bit0 & mask;
            let hi = bit1 & mask;
            let neg = negative & mask;
            let c0 = 2 * (equal_sign & lo).count_ones() as i32 - lo.count_ones() as i32;
            let c1 = 2 * (equal_sign & hi).count_ones() as i32 - hi.count_ones() as i32;
            let offset = 2 * (neg & activation).count_ones() as i32 - neg.count_ones() as i32;
            let signed_scale = subgroup_scale(scales, group * 4 + subgroup);
            weighted_integer += (c0 + 2 * c1 + offset) * signed_scale;
        }
    }
    debug_assert!(weighted_integer.abs() <= (groups * 64 * 4 * 32) as i32);
    Ok(PartialDot {
        weighted_integer,
        global_scale_f16_bits: u16::from_le_bytes([raw[108], raw[109]]),
        first_coefficient: first_group * 64,
        coefficients: groups * 64,
    })
}

#[cfg(test)]
#[path = "gguf_k.rs"]
#[allow(dead_code)]
mod gguf_k;

#[cfg(test)]
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod bpb3k;

#[cfg(test)]
#[path = "bpb3k_direct.rs"]
#[allow(dead_code)]
mod bpb3k_direct;

#[cfg(test)]
#[path = "bpb3x.rs"]
#[allow(dead_code)]
mod bpb3x;

#[cfg(test)]
mod tests {
    use super::*;

    fn q3_fixture(case: usize) -> [u8; BYTES] {
        let mut bytes: [u8; BYTES] =
            std::array::from_fn(|i| (i * 73 + (i / 4) * 19 + case * 41) as u8);
        bytes[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
        bytes
    }

    #[test]
    fn all_six_x_layouts_match_original_k_selective_and_q3_oracle() {
        for case in 0..32 {
            let q3 = q3_fixture(case);
            let k = bpb3k::q3_to_bpb3k(&q3).unwrap();
            let original = gguf_k::decode_q3_k(&q3).unwrap();
            for pattern in 0..16 {
                let masks: [u64; 4] = std::array::from_fn(|group| match pattern {
                    0 => 0,
                    1 => u64::MAX,
                    2 => 0xaaaa_aaaa_aaaa_aaaa,
                    3 => 0x5555_5555_5555_5555,
                    _ => 1u64 << ((case + pattern + group) % 64),
                });
                let full = bpb3k_direct::dot_bpb3k_bipolar(&k, &masks).unwrap();
                for layout in bpb3x::Layout::ALL {
                    let wire = bpb3x::encode(&k, layout).unwrap();
                    let mut first_group = 0usize;
                    let mut total = 0i32;
                    for (segment_index, &groups) in layout.parts_in_groups().iter().enumerate() {
                        let selected = dot_selected_bpb3k(&k, first_group, groups, &masks).unwrap();
                        let x = bpb3x::dot_segment_bipolar(&wire, segment_index, &masks).unwrap();
                        assert_eq!(selected.weighted_integer, x.weighted_integer);
                        assert_eq!(selected.global_scale_f16_bits, x.global_scale_f16_bits);
                        assert_eq!(selected.first_coefficient, x.first_coefficient);
                        assert_eq!(selected.coefficients, x.coefficients);
                        let start = first_group * 64;
                        let expected = (start..start + groups * 64)
                            .map(|i| {
                                let negative = (masks[i / 64] >> (i % 64)) & 1 != 0;
                                f64::from(original[i]) * if negative { -1.0 } else { 1.0 }
                            })
                            .sum::<f64>();
                        assert_eq!(
                            f64::from(selected.weighted_integer),
                            expected,
                            "case={case} mask={pattern} layout={layout:?}"
                        );
                        total += selected.weighted_integer;
                        first_group += groups;
                    }
                    assert_eq!(first_group, 4);
                    assert_eq!(total, full.weighted_integer);
                }
            }
        }
    }

    #[test]
    fn reject_invalid_spans_and_lengths() {
        for len in [0, 24, 96, 109, 111, 256] {
            assert_eq!(
                dot_selected_bpb3k(&vec![0; len], 0, 1, &[0; 4]),
                Err(Error::WrongLength { actual: len })
            );
        }
        let raw = [0u8; BYTES];
        for (first, groups) in [(0, 0), (0, 3), (1, 4), (3, 2), (4, 1)] {
            assert_eq!(
                dot_selected_bpb3k(&raw, first, groups, &[0; 4]),
                Err(Error::InvalidSpan {
                    first_group: first,
                    groups,
                })
            );
        }
    }

    #[test]
    fn preserve_global_fp16_bits_without_floating_point_assumptions() {
        let scale_bits: [u16; 8] = [0, 0x8000, 1, 0x3c00, 0x7bff, 0x7c00, 0x7e00, 0xfc00];
        for bits in scale_bits {
            let mut k = bpb3k::q3_to_bpb3k(&q3_fixture(11)).unwrap();
            k[108..110].copy_from_slice(&bits.to_le_bytes());
            for first in 0..4 {
                let dot = dot_selected_bpb3k(&k, first, 1, &[0; 4]).unwrap();
                assert_eq!(dot.global_scale_f16_bits, bits);
            }
        }
    }
}
