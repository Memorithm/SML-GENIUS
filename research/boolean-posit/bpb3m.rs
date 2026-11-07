//! BPB3-M/v0: reversible 128-weight microblock view of BPB3-K.
//!
//! Research-only; 56 bytes per 128 coefficients (3.5 bits/weight).
//! Two 56-byte microblocks preserve exactly one 110-byte Q3_K-compatible
//! BPB3-K superblock, at 2 bytes extra for the duplicated global FP16 scale.
//! This is NOT a new quantization method and makes no speed claim.
//!
//! Wire layout: two 64-lane groups of [negative:u64, bit0:u64, bit1:u64],
//! eight 6-bit signed Q3_K subgroup scales densely packed LSB-first into
//! six bytes, then the original FP16 global scale in little-endian order.
//! Input masks use one bit per bipolar activation; set bit means -1.
//!
//! See BPB3_M_128.md for scope, parity contract and budget.
#![forbid(unsafe_code)]

pub const VALUES: usize = 128;
pub const BYTES: usize = 56;
pub const PLANE_BYTES: usize = 48;
pub const LOCAL_SCALE_BYTES: usize = 6;
pub const GROUPS: usize = 2;
pub const SUBGROUPS: usize = 8;
const Q3_K_BYTES: usize = 110;
const Q3_K_PLANE_BYTES: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    WrongLength { actual: usize, expected: usize },
    GlobalScaleMismatch { first: u16, second: u16 },
}

/// Exact integer dot and unchanged original FP16 global scale bits.
/// Mathematical represented result is integer_dot multiplied by global scale.
/// No f32 accumulation order or NaN handling policy is imposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectDot {
    pub integer_dot: i32,
    pub global_scale_f16_bits: u16,
}

fn check_len(raw: &[u8], expected: usize) -> Result<(), Error> {
    if raw.len() != expected {
        return Err(Error::WrongLength {
            actual: raw.len(),
            expected,
        });
    }
    Ok(())
}

fn read_word(raw: &[u8], start: usize) -> u64 {
    u64::from_le_bytes(raw[start..start + 8].try_into().expect("checked fixed layout"))
}

fn original_scale_code(scales: &[u8], subgroup: usize) -> u8 {
    let low = if subgroup < 8 {
        scales[subgroup] & 15
    } else {
        scales[subgroup - 8] >> 4
    };
    let high = (scales[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
    low | (high << 4)
}

fn local_scale_bits(raw: &[u8]) -> u64 {
    let mut bytes = [0u8; 8];
    bytes[..LOCAL_SCALE_BYTES].copy_from_slice(&raw[PLANE_BYTES..PLANE_BYTES + LOCAL_SCALE_BYTES]);
    u64::from_le_bytes(bytes)
}

fn fp16_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = (bits >> 10) & 31;
    let fraction = bits & 1023;
    match exponent {
        0 => sign * f32::from(fraction) * 2.0f32.powi(-24),
        31 if fraction == 0 => sign * f32::INFINITY,
        31 => f32::NAN,
        _ => sign * (1.0 + f32::from(fraction) / 1024.0)
            * 2.0f32.powi(i32::from(exponent) - 15),
    }
}

/// Split a BPB3-K block (110 bytes/256 weights) into two 56-byte BPB3-M
/// microblocks, without modifying any coefficient or its 6-bit scale.
/// The single original FP16 scale is deliberately duplicated.
pub fn split_bpb3k(raw: &[u8]) -> Result<[[u8; BYTES]; 2], Error> {
    check_len(raw, Q3_K_BYTES)?;
    let mut out = [[0u8; BYTES]; 2];
    for half in 0..2 {
        out[half][..PLANE_BYTES]
            .copy_from_slice(&raw[half * PLANE_BYTES..(half + 1) * PLANE_BYTES]);
        let mut packed = 0u64;
        for subgroup in 0..SUBGROUPS {
            let code = u64::from(original_scale_code(
                &raw[Q3_K_PLANE_BYTES..Q3_K_PLANE_BYTES + 12],
                half * SUBGROUPS + subgroup,
            ));
            packed |= code << (6 * subgroup);
        }
        out[half][PLANE_BYTES..PLANE_BYTES + LOCAL_SCALE_BYTES]
            .copy_from_slice(&packed.to_le_bytes()[..LOCAL_SCALE_BYTES]);
        out[half][54..56].copy_from_slice(&raw[108..110]);
    }
    Ok(out)
}

/// Recombine two BPB3-M microblocks back into the *exact original* BPB3-K
/// wire bytes. Their global FP16 scale bits must match; this prevents
/// pretending two independently quantized halves share a scale.
pub fn join_bpb3m(first: &[u8], second: &[u8]) -> Result<[u8; Q3_K_BYTES], Error> {
    check_len(first, BYTES)?;
    check_len(second, BYTES)?;
    let left_scale = u16::from_le_bytes([first[54], first[55]]);
    let right_scale = u16::from_le_bytes([second[54], second[55]]);
    if left_scale != right_scale {
        return Err(Error::GlobalScaleMismatch {
            first: left_scale,
            second: right_scale,
        });
    }
    let mut out = [0u8; Q3_K_BYTES];
    out[..PLANE_BYTES].copy_from_slice(&first[..PLANE_BYTES]);
    out[PLANE_BYTES..Q3_K_PLANE_BYTES].copy_from_slice(&second[..PLANE_BYTES]);
    for (half, block) in [first, second].iter().enumerate() {
        let packed = local_scale_bits(block);
        for subgroup in 0..SUBGROUPS {
            let global_subgroup = half * SUBGROUPS + subgroup;
            let code = ((packed >> (6 * subgroup)) & 63) as u8;
            if global_subgroup < 8 {
                out[96 + global_subgroup] |= code & 15;
            } else {
                out[96 + global_subgroup - 8] |= (code & 15) << 4;
            }
            out[104 + global_subgroup % 4] |=
                ((code >> 4) & 3) << (2 * (global_subgroup / 4));
        }
    }
    out[108..110].copy_from_slice(&first[54..56]);
    Ok(out)
}

/// Full 128-value decode oracle (not part of the direct compute hot path).
/// Reconstruction order intentionally mirrors Q3_K: global * subgroup * q.
pub fn decode_bpb3m(raw: &[u8]) -> Result<[f32; VALUES], Error> {
    check_len(raw, BYTES)?;
    let global = fp16_to_f32(u16::from_le_bytes([raw[54], raw[55]]));
    let packed_scales = local_scale_bits(raw);
    Ok(std::array::from_fn(|i| {
        let group = i / 64;
        let lane = i % 64;
        let offset = group * 24;
        let sign = (read_word(raw, offset) >> lane) & 1 != 0;
        let magnitude = (((read_word(raw, offset + 8) >> lane) & 1)
            | (((read_word(raw, offset + 16) >> lane) & 1) << 1)) as i32;
        let q = if sign { -(magnitude + 1) } else { magnitude };
        let subgroup_scale = ((packed_scales >> (6 * (i / 16))) & 63) as i32 - 32;
        global * subgroup_scale as f32 * q as f32
    }))
}

/// Direct bipolar dot in a *single* 128-weight microblock: 2 groups of 64
/// with 3 bitplanes each and 8 independently signed 6-bit subgroup scales.
/// No heap allocation, f32 weight expansion, or shared global state.
pub fn dot_bpb3m_bipolar(raw: &[u8], input_negative: &[u64; GROUPS]) -> Result<DirectDot, Error> {
    check_len(raw, BYTES)?;
    let packed_scales = local_scale_bits(raw);
    let mut integer_dot = 0i32;

    for (group, &x_negative) in input_negative.iter().enumerate() {
        let offset = group * 24;
        let negative = read_word(raw, offset);
        let bit0 = read_word(raw, offset + 8);
        let bit1 = read_word(raw, offset + 16);
        let equal_sign = !(negative ^ x_negative);
        for local in 0..4 {
            let subgroup = group * 4 + local;
            let mask = 0xffffu64 << (16 * local);
            let lo = bit0 & mask;
            let hi = bit1 & mask;
            let neg = negative & mask;
            let c0 = 2 * (equal_sign & lo).count_ones() as i32 - lo.count_ones() as i32;
            let c1 = 2 * (equal_sign & hi).count_ones() as i32 - hi.count_ones() as i32;
            let correction = 2 * (neg & x_negative).count_ones() as i32 - neg.count_ones() as i32;
            let signed_scale = ((packed_scales >> (6 * subgroup)) & 63) as i32 - 32;
            integer_dot += (c0 + 2 * c1 + correction) * signed_scale;
        }
    }
    debug_assert!(integer_dot.abs() <= 16_384);
    Ok(DirectDot {
        integer_dot,
        global_scale_f16_bits: u16::from_le_bytes([raw[54], raw[55]]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[path = "gguf_k.rs"]
    #[allow(dead_code)]
    mod gguf_k;

    #[path = "bpb3k.rs"]
    #[allow(dead_code)]
    mod bpb3k;

    #[path = "bpb3k_direct.rs"]
    #[allow(dead_code)]
    mod bpb3k_direct;

    fn fixture(case: usize, scale: u16) -> [u8; Q3_K_BYTES] {
        let mut raw: [u8; Q3_K_BYTES] =
            std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8);
        raw[108..110].copy_from_slice(&scale.to_le_bytes());
        raw
    }

    #[test]
    fn size_and_metadata_budget() {
        assert_eq!(3 * 2 * 8 + 8 * 6 / 8 + 2, BYTES);
        assert_eq!(8.0 * BYTES as f64 / VALUES as f64, 3.5);
        assert_eq!(BYTES * 2, Q3_K_BYTES + 2);
    }

    #[test]
    fn round_trip_is_bit_exact_for_original_bpb3k_wire() {
        // Including arbitrary Q3_K packed scale bytes, signed global FP16,
        // subnormal / zero / infinite metadata (only bits compared here).
        let f16 = [0x3c00, 0xbc00, 0x0001, 0, 0x8000, 0x7c00, 0x7e00, 0xfc00];
        for case in 0..64 {
            let q3 = fixture(case, f16[case % f16.len()]);
            let k = bpb3k::q3_to_bpb3k(&q3).unwrap();
            let [a, b] = split_bpb3k(&k).unwrap();
            assert_eq!(join_bpb3m(&a, &b).unwrap(), k);
        }
    }

    #[test]
    fn decode_parity_with_original_q3_k() {
        for case in 0..48 {
            let scale = [0x3c00, 0xbc00, 0x3800, 0x0001][case % 4];
            let q3 = fixture(case, scale);
            let k = bpb3k::q3_to_bpb3k(&q3).unwrap();
            let halves = split_bpb3k(&k).unwrap();
            let original = gguf_k::decode_q3_k(&q3).unwrap();
            for (half, block) in halves.iter().enumerate() {
                let weights = decode_bpb3m(block).unwrap();
                for i in 0..VALUES {
                    assert_eq!(
                        weights[i].to_bits(),
                        original[half * VALUES + i].to_bits(),
                        "case={case} half={half} lane={i}"
                    );
                }
            }
        }
    }

    #[test]
    fn direct_bipolar_dot_adds_to_full_bpb3k_dot() {
        for case in 0..48 {
            let q3 = fixture(case, 0x3c00);
            let k = bpb3k::q3_to_bpb3k(&q3).unwrap();
            let [a, b] = split_bpb3k(&k).unwrap();
            for pattern in 0..16 {
                let masks: [u64; 4] = std::array::from_fn(|i| match pattern {
                    0 => 0,
                    1 => u64::MAX,
                    2 => 0xaaaa_aaaa_aaaa_aaaa,
                    3 => 0x5555_5555_5555_5555,
                    _ => {
                        let shift = (case + pattern + i) % 64;
                        if pattern % 2 == 0 {
                            1u64 << shift
                        } else {
                            !(1u64 << shift)
                        }
                    }
                });
                let d0 = dot_bpb3m_bipolar(&a, &[masks[0], masks[1]]).unwrap();
                let d1 = dot_bpb3m_bipolar(&b, &[masks[2], masks[3]]).unwrap();
                let dk = bpb3k_direct::dot_bpb3k_bipolar(&k, &masks).unwrap();
                assert_eq!(d0.integer_dot + d1.integer_dot, dk.weighted_integer);
                assert_eq!(d0.global_scale_f16_bits, dk.global_scale_f16_bits);
                assert_eq!(d1.global_scale_f16_bits, dk.global_scale_f16_bits);
            }
        }
    }

    #[test]
    fn malformed_inputs_and_global_scale_mismatch_fail_closed() {
        for len in [0, 1, 55, 57, 96, 109, 111] {
            if len != Q3_K_BYTES {
                assert_eq!(
                    split_bpb3k(&vec![0; len]),
                    Err(Error::WrongLength {
                        actual: len,
                        expected: Q3_K_BYTES
                    })
                );
            }
            if len != BYTES {
                assert_eq!(
                    decode_bpb3m(&vec![0; len]),
                    Err(Error::WrongLength {
                        actual: len,
                        expected: BYTES
                    })
                );
                assert_eq!(
                    dot_bpb3m_bipolar(&vec![0; len], &[0; GROUPS]),
                    Err(Error::WrongLength {
                        actual: len,
                        expected: BYTES
                    })
                );
            }
        }
        let q3 = fixture(0, 0x3c00);
        let k = bpb3k::q3_to_bpb3k(&q3).unwrap();
        let [a, mut b] = split_bpb3k(&k).unwrap();
        b[54] ^= 1;
        assert_eq!(
            join_bpb3m(&a, &b),
            Err(Error::GlobalScaleMismatch {
                first: 0x3c00,
                second: 0x3c01
            })
        );
    }

    #[test]
    fn represent_negative_four_and_positive_three() {
        let mut all_neg = [0u8; 110];
        all_neg[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
        let k = bpb3k::q3_to_bpb3k(&all_neg).unwrap();
        let blocks = split_bpb3k(&k).unwrap();
        for block in blocks.iter() {
            assert_eq!(dot_bpb3m_bipolar(block, &[0; 2]).unwrap().integer_dot, 128 * 128);
        }
        let mut all_pos = all_neg;
        all_pos[..32].fill(0xff);
        all_pos[32..96].fill(0xff);
        let kp = bpb3k::q3_to_bpb3k(&all_pos).unwrap();
        for block in split_bpb3k(&kp).unwrap() {
            assert_eq!(dot_bpb3m_bipolar(&block, &[0; 2]).unwrap().integer_dot, -96 * 128);
        }
    }
}
