//! Experimental BPB3-K: bit-sliced, *lossless re-layout* of ggml Q3_K.
//!
//! This is NOT BPB-3 v0: it adopts Q3_K's asymmetric [-4,+3] integer
//! alphabet and all sixteen 6-bit subgroup scales and its fp16 global
//! scale. Hence its reconstruction error is identical to Q3_K, not better.
//! It has a distinct Boolean bitplane representation at the SAME 110 B/256.
//! References: ggml-org/llama.cpp commit
//! f7591a6a01f9273de03331732bdc0b39721168be.

#[path = "gguf_k.rs"]
#[allow(dead_code)]
mod gguf_k;

pub const VALUES: usize = 256;
pub const BYTES: usize = 110;
pub const PLANES_BYTES: usize = 96;
pub const SCALE_METADATA_BYTES: usize = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    WrongLength { actual: usize },
}

/// Byte order: four groups of three LE u64 planes (negative, bit0, bit1),
/// then the original Q3_K 12-byte scale field and FP16 d (14 bytes).
/// Signed q is 0..3 if sign=0; signed q is -(magnitude_bits+1) if sign=1.
pub fn q3_to_bpb3k(raw: &[u8]) -> Result<[u8; BYTES], Error> {
    if raw.len() != BYTES {
        return Err(Error::WrongLength { actual: raw.len() });
    }
    let mut output = [0u8; BYTES];
    for group in 0..4 {
        let mut neg = 0u64;
        let mut bit0 = 0u64;
        let mut bit1 = 0u64;
        for lane in 0..64 {
            let i = group * 64 + lane;
            let half = i / 128;
            let group_in_half = (i % 128) / 16;
            let local = (group_in_half % 2) * 16 + i % 16;
            let q_offset = half * 32 + local;
            let bitplane = group_in_half / 2;
            let high_bitplane = half * 4 + bitplane;
            let low = (raw[32 + q_offset] >> (2 * bitplane)) & 3;
            let is_negative = raw[local] & (1 << high_bitplane) == 0;
            let signed = i32::from(low) - if is_negative { 4 } else { 0 };
            debug_assert!((-4..=3).contains(&signed));
            let magnitude = if signed < 0 { -signed - 1 } else { signed } as u64;
            if signed < 0 {
                neg |= 1 << lane;
            }
            if magnitude & 1 != 0 {
                bit0 |= 1 << lane;
            }
            if magnitude & 2 != 0 {
                bit1 |= 1 << lane;
            }
        }
        for (p, word) in [neg, bit0, bit1].iter().enumerate() {
            let offset = group * 24 + p * 8;
            output[offset..offset + 8].copy_from_slice(&word.to_le_bytes());
        }
    }
    output[PLANES_BYTES..].copy_from_slice(&raw[96..110]);
    Ok(output)
}

fn decode_fp16(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exp = (bits >> 10) & 31;
    let frac = bits & 1023;
    match exp {
        0 => sign * f32::from(frac) * 2.0f32.powi(-24),
        31 if frac == 0 => sign * f32::INFINITY,
        31 => f32::NAN,
        _ => sign * (1.0 + f32::from(frac) / 1024.0) * 2.0f32.powi(i32::from(exp) - 15),
    }
}

pub fn decode_bpb3k(raw: &[u8]) -> Result<[f32; VALUES], Error> {
    if raw.len() != BYTES {
        return Err(Error::WrongLength { actual: raw.len() });
    }
    let metadata = &raw[96..];
    let scales = &metadata[..12];
    let global = decode_fp16(u16::from_le_bytes([metadata[12], metadata[13]]));
    Ok(std::array::from_fn(|i| {
        let group = i / 64;
        let lane = i % 64;
        let offset = group * 24;
        let read_word = |p: usize| {
            let start = offset + p * 8;
            u64::from_le_bytes(raw[start..start + 8].try_into().unwrap())
        };
        let is_negative = (read_word(0) >> lane) & 1 != 0;
        let magnitude = (((read_word(1) >> lane) & 1) | (((read_word(2) >> lane) & 1) << 1)) as i32;
        let q = if is_negative {
            -(magnitude + 1)
        } else {
            magnitude
        };
        let subgroup = i / 16;
        let low = if subgroup < 8 {
            scales[subgroup] & 15
        } else {
            scales[subgroup - 8] >> 4
        };
        let high = (scales[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
        let scale = i32::from(low | (high << 4)) - 32;
        global * (scale as f32) * (q as f32)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q3_fixture(case: usize) -> [u8; BYTES] {
        let mut v = std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8);
        v[108..110].copy_from_slice(&[0x00, 0x3c]); // d=1.0
        v
    }

    #[test]
    fn exact_ggml_superblock_budget_and_layout() {
        assert_eq!(PLANES_BYTES + SCALE_METADATA_BYTES, BYTES);
        assert_eq!(8.0 * BYTES as f64 / 256.0, 3.4375);
        assert_eq!(gguf_k::Q3_BYTES, BYTES);
    }

    #[test]
    fn reject_truncated_or_oversized_inputs() {
        for size in [0, 1, 109, 111, 144, 256] {
            assert_eq!(
                q3_to_bpb3k(&vec![0; size]),
                Err(Error::WrongLength { actual: size })
            );
            assert_eq!(
                decode_bpb3k(&vec![0; size]),
                Err(Error::WrongLength { actual: size })
            );
        }
    }

    #[test]
    fn bitwise_reconstruction_matches_official_q3_bytes() {
        for case in 0..25 {
            let raw = q3_fixture(case);
            let transcoded = q3_to_bpb3k(&raw).unwrap();
            assert_eq!(&transcoded[PLANES_BYTES..], &raw[96..]);
            let expected = gguf_k::decode_q3_k(&raw).unwrap();
            let actual = decode_bpb3k(&transcoded).unwrap();
            for i in 0..VALUES {
                assert_eq!(
                    actual[i].to_bits(),
                    expected[i].to_bits(),
                    "q3 BPB3K bitwise mismatch case={case} index={i}"
                );
            }
        }
    }

    #[test]
    fn q3_minus_four_and_plus_three_both_representable() {
        let mut all_negative = [0u8; BYTES];
        all_negative[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
        let converted = q3_to_bpb3k(&all_negative).unwrap();
        assert!(decode_bpb3k(&converted)
            .unwrap()
            .iter()
            .all(|&v| v == 128.0));
        for group in 0..4 {
            let sign =
                u64::from_le_bytes(converted[group * 24..group * 24 + 8].try_into().unwrap());
            assert_eq!(sign, u64::MAX);
        }
        let mut all_positive = all_negative;
        all_positive[..32].fill(0xff);
        all_positive[32..96].fill(0xff);
        let pos = q3_to_bpb3k(&all_positive).unwrap();
        assert!(decode_bpb3k(&pos).unwrap().iter().all(|&v| v == -96.0));
    }
}
