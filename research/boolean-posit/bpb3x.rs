//! BPB3-X/v0: reversible Elastic block-length layouts over one BPB3-K
//! 256-weight source superblock. This codec is an experimental research
//! reference, not a generic ElasticXxx runtime or adaptive optimizer.
//!
//! Each 64-weight group contains 3 u64 bitplanes (24 bytes), four 6-bit
//! signed scales (3 bytes). Each segment additionally repeats the same
//! source 2-byte FP16 global scale. One byte labels one of six legal
//! partitions. The output has 111, 113, 115 or 117 bytes per 256 weights.
//!
//! No inter-superblock fusion, quantization adjustment, SIMD, GPU, or
//! measured performance is claimed.
#![forbid(unsafe_code)]

pub const SOURCE_BYTES: usize = 110;
pub const SOURCE_VALUES: usize = 256;
const SOURCE_PLANE_BYTES: usize = 96;
const SCALE_BYTES: usize = 12;
const GROUP_PLANES: usize = 24;
const GROUP_SCALES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Layout {
    One256 = 0,
    Two128 = 1,
    First128Then64_64 = 2,
    First64Then128Then64 = 3,
    First64_64Then128 = 4,
    Four64 = 5,
}

impl Layout {
    pub const ALL: [Self; 6] = [
        Self::One256,
        Self::Two128,
        Self::First128Then64_64,
        Self::First64Then128Then64,
        Self::First64_64Then128,
        Self::Four64,
    ];

    pub fn parts_in_groups(self) -> &'static [usize] {
        match self {
            Self::One256 => &[4],
            Self::Two128 => &[2, 2],
            Self::First128Then64_64 => &[2, 1, 1],
            Self::First64Then128Then64 => &[1, 2, 1],
            Self::First64_64Then128 => &[1, 1, 2],
            Self::Four64 => &[1, 1, 1, 1],
        }
    }

    pub fn encoded_bytes(self) -> usize {
        // Four 64-lane groups: 4*(24B planes + 3B scale) = 108B.
        // One layout byte and two FP16 bytes per segment.
        1 + 4 * (GROUP_PLANES + GROUP_SCALES) + 2 * self.parts_in_groups().len()
    }

    fn parse(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    WrongLength { actual: usize, expected: usize },
    InvalidLayout { code: u8 },
    GlobalScaleMismatch { expected: u16, actual: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectDot {
    pub weighted_integer: i32,
    pub global_scale_f16_bits: u16,
}

fn strict_length(raw: &[u8], expected: usize) -> Result<(), Error> {
    if raw.len() == expected {
        Ok(())
    } else {
        Err(Error::WrongLength {
            actual: raw.len(),
            expected,
        })
    }
}

fn original_scale(scales: &[u8], subgroup: usize) -> u8 {
    let low = if subgroup < 8 {
        scales[subgroup] & 15
    } else {
        scales[subgroup - 8] >> 4
    };
    let high = (scales[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
    low | high << 4
}

fn packed_scale(scales: &[u8], subgroup: usize) -> u8 {
    let position = subgroup * 6;
    let start = position / 8;
    let word =
        u16::from(scales[start]) | (u16::from(scales.get(start + 1).copied().unwrap_or(0)) << 8);
    ((word >> (position % 8)) & 63) as u8
}

fn put_scale(scales: &mut [u8], subgroup: usize, code: u8) {
    let position = subgroup * 6;
    let shift = position % 8;
    let start = position / 8;
    scales[start] |= code << shift;
    if shift > 2 {
        scales[start + 1] |= code >> (8 - shift);
    }
}

fn put_original_scale(scales: &mut [u8], subgroup: usize, code: u8) {
    if subgroup < 8 {
        scales[subgroup] |= code & 15;
    } else {
        scales[subgroup - 8] |= (code & 15) << 4;
    }
    scales[8 + subgroup % 4] |= ((code >> 4) & 3) << (2 * (subgroup / 4));
}

/// The decoder's validated directory, retaining one common source FP16.
fn validated_layout(wire: &[u8]) -> Result<(Layout, u16), Error> {
    let Some(&marker) = wire.first() else {
        return Err(Error::WrongLength {
            actual: 0,
            expected: 1,
        });
    };
    let layout = Layout::parse(marker).ok_or(Error::InvalidLayout { code: marker })?;
    strict_length(wire, layout.encoded_bytes())?;
    let mut cursor = 1;
    let mut fp16 = None;
    for &groups in layout.parts_in_groups() {
        cursor += (GROUP_PLANES + GROUP_SCALES) * groups;
        let bits = u16::from_le_bytes([wire[cursor], wire[cursor + 1]]);
        if let Some(expected) = fp16 {
            if expected != bits {
                return Err(Error::GlobalScaleMismatch {
                    expected,
                    actual: bits,
                });
            }
        }
        fp16 = Some(bits);
        cursor += 2;
    }
    debug_assert_eq!(cursor, wire.len());
    Ok((layout, fp16.expect("all layouts nonempty")))
}

/// Encode an original 110-byte BPB3-K block with a caller-chosen partition.
/// Layout selection is deliberately NOT part of this domain codec; ElasticXxx
/// may later choose only independently qualified layouts under its policy.
pub fn encode(raw: &[u8], layout: Layout) -> Result<Vec<u8>, Error> {
    strict_length(raw, SOURCE_BYTES)?;
    let mut result = Vec::with_capacity(layout.encoded_bytes());
    result.push(layout as u8);
    let mut first_group = 0;
    for &groups in layout.parts_in_groups() {
        let begin = first_group * GROUP_PLANES;
        let end = begin + groups * GROUP_PLANES;
        result.extend_from_slice(&raw[begin..end]);

        let mut packed = vec![0u8; groups * GROUP_SCALES];
        for local in 0..groups * 4 {
            let code = original_scale(
                &raw[SOURCE_PLANE_BYTES..SOURCE_PLANE_BYTES + SCALE_BYTES],
                first_group * 4 + local,
            );
            put_scale(&mut packed, local, code);
        }
        result.extend_from_slice(&packed);
        result.extend_from_slice(&raw[108..110]);
        first_group += groups;
    }
    debug_assert_eq!(first_group, 4);
    debug_assert_eq!(result.len(), layout.encoded_bytes());
    Ok(result)
}

/// Recreate the original BPB3-K bytes, including all 16 original Q3_K
/// subgroup scale codes and exact FP16 bits, for any legal partition.
pub fn decode(wire: &[u8]) -> Result<[u8; SOURCE_BYTES], Error> {
    let (layout, _) = validated_layout(wire)?;
    let mut result = [0u8; SOURCE_BYTES];
    let mut cursor = 1;
    let mut first_group = 0;
    for &groups in layout.parts_in_groups() {
        let count = groups * GROUP_PLANES;
        result[first_group * GROUP_PLANES..(first_group + groups) * GROUP_PLANES]
            .copy_from_slice(&wire[cursor..cursor + count]);
        cursor += count;
        let scales = &wire[cursor..cursor + groups * GROUP_SCALES];
        for local in 0..groups * 4 {
            let code = packed_scale(scales, local);
            put_original_scale(
                &mut result[SOURCE_PLANE_BYTES..SOURCE_PLANE_BYTES + SCALE_BYTES],
                first_group * 4 + local,
                code,
            );
        }
        cursor += groups * GROUP_SCALES;
        result[108..110].copy_from_slice(&wire[cursor..cursor + 2]);
        cursor += 2;
        first_group += groups;
    }
    Ok(result)
}

fn u64_le(raw: &[u8]) -> u64 {
    u64::from_le_bytes(raw.try_into().expect("validated 8-byte plane"))
}

/// Direct 256-coefficient bipolar dot on the variable-segment BPB3-X bytes.
/// Does NOT expand the block to BPB3-K or allocate a dense f32 array.
pub fn dot_bipolar(wire: &[u8], inputs_negative: &[u64; 4]) -> Result<DirectDot, Error> {
    let (layout, fp16) = validated_layout(wire)?;
    let mut cursor = 1;
    let mut first_group = 0;
    let mut total = 0i32;
    for &groups in layout.parts_in_groups() {
        let plane_bytes = &wire[cursor..cursor + groups * GROUP_PLANES];
        cursor += groups * GROUP_PLANES;
        let scales = &wire[cursor..cursor + groups * GROUP_SCALES];
        cursor += groups * GROUP_SCALES + 2;

        for group in 0..groups {
            let base = group * GROUP_PLANES;
            let negative = u64_le(&plane_bytes[base..base + 8]);
            let bit0 = u64_le(&plane_bytes[base + 8..base + 16]);
            let bit1 = u64_le(&plane_bytes[base + 16..base + 24]);
            let activation = inputs_negative[first_group + group];
            let equal_sign = !(negative ^ activation);
            for subgroup in 0..4 {
                let mask = 0xffffu64 << (16 * subgroup);
                let lo = bit0 & mask;
                let hi = bit1 & mask;
                let neg = negative & mask;
                let c0 = 2 * (equal_sign & lo).count_ones() as i32 - lo.count_ones() as i32;
                let c1 = 2 * (equal_sign & hi).count_ones() as i32 - hi.count_ones() as i32;
                let correction =
                    2 * (neg & activation).count_ones() as i32 - neg.count_ones() as i32;
                let signed_scale = i32::from(packed_scale(scales, group * 4 + subgroup)) - 32;
                total += (c0 + 2 * c1 + correction) * signed_scale;
            }
        }
        first_group += groups;
    }
    debug_assert_eq!(first_group, 4);
    debug_assert!(total.abs() <= 32768);
    Ok(DirectDot {
        weighted_integer: total,
        global_scale_f16_bits: fp16,
    })
}

#[cfg(test)]
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod bpb3k;

#[cfg(test)]
#[path = "bpb3k_direct.rs"]
#[allow(dead_code)]
mod bpb3k_direct;

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(case: usize, fp16: u16) -> [u8; SOURCE_BYTES] {
        let mut q3: [u8; SOURCE_BYTES] =
            std::array::from_fn(|i| (i * 73 + (i / 4) * 19 + case * 41) as u8);
        q3[108..110].copy_from_slice(&fp16.to_le_bytes());
        bpb3k::q3_to_bpb3k(&q3).unwrap()
    }

    #[test]
    fn enumerate_all_six_legal_partitions_and_exact_total_cost() {
        let costs = [111, 113, 115, 115, 115, 117];
        let mut unique_partitions = std::collections::BTreeSet::new();
        for (i, layout) in Layout::ALL.iter().enumerate() {
            assert_eq!(layout.encoded_bytes(), costs[i]);
            let lengths = layout.parts_in_groups();
            assert_eq!(lengths.iter().sum::<usize>(), 4);
            assert!(lengths.iter().all(|&n| n == 1 || n == 2 || n == 4));
            assert!(unique_partitions.insert(lengths.to_vec()));
        }
        assert_eq!(unique_partitions.len(), 6);
        assert_eq!(Layout::One256.encoded_bytes(), SOURCE_BYTES + 1);
    }

    #[test]
    fn all_partitions_are_exactly_reversible_for_64_sources() {
        for i in 0..64 {
            let scale_cases = [0x3c00, 0xbc00, 0x0001, 0, 0x8000, 0x7c00, 0x7e00, 0xfc00];
            let src = fixture(i, scale_cases[i % scale_cases.len()]);
            for layout in Layout::ALL {
                let wire = encode(&src, layout).unwrap();
                assert_eq!(wire.len(), layout.encoded_bytes());
                assert_eq!(decode(&wire).unwrap(), src, "case={i} layout={layout:?}");
            }
        }
    }

    #[test]
    fn direct_dot_matches_whole_bpb3k_for_every_partition() {
        for i in 0..32 {
            let src = fixture(i, 0x3c00);
            for pattern in 0..16 {
                let mask: [u64; 4] = std::array::from_fn(|j| match pattern {
                    0 => 0,
                    1 => u64::MAX,
                    2 => 0xaaaa_aaaa_aaaa_aaaa,
                    3 => 0x5555_5555_5555_5555,
                    _ => {
                        let k = (i + j + pattern) % 64;
                        if pattern % 2 == 0 {
                            1u64 << k
                        } else {
                            !(1u64 << k)
                        }
                    }
                });
                let oracle = bpb3k_direct::dot_bpb3k_bipolar(&src, &mask).unwrap();
                for layout in Layout::ALL {
                    let wire = encode(&src, layout).unwrap();
                    let got = dot_bipolar(&wire, &mask).unwrap();
                    assert_eq!(
                        got.weighted_integer, oracle.weighted_integer,
                        "case={i} pattern={pattern} layout={layout:?}"
                    );
                    assert_eq!(got.global_scale_f16_bits, oracle.global_scale_f16_bits);
                }
            }
        }
    }

    #[test]
    fn invalid_length_marker_and_mixed_global_scales_fail_closed() {
        assert_eq!(
            decode(&[]),
            Err(Error::WrongLength {
                actual: 0,
                expected: 1
            })
        );
        assert_eq!(decode(&[6]), Err(Error::InvalidLayout { code: 6 }));
        let source = fixture(7, 0x3c00);
        for layout in Layout::ALL {
            let wire = encode(&source, layout).unwrap();
            for &len in &[0, 1, wire.len() - 1] {
                let result = decode(&wire[..len]);
                assert!(result.is_err(), "layout={layout:?} len={len}");
            }
            let mut oversized = wire.clone();
            oversized.push(0);
            assert!(decode(&oversized).is_err());
        }

        let mut two = encode(&source, Layout::Two128).unwrap();
        // Layout 1: header, first 56B block, then second 56B block.
        // Original FP16 occupies positions [55,56] and [111,112].
        two[111] ^= 1;
        assert_eq!(
            decode(&two),
            Err(Error::GlobalScaleMismatch {
                expected: 0x3c00,
                actual: 0x3c01,
            })
        );
        assert!(dot_bipolar(&two, &[0; 4]).is_err());
    }
}
