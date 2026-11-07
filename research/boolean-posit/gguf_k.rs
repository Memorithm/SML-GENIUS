//! Strict, dependency-free Rust decode oracle for the GGUF Q3_K / Q4_K
//! 256-value superblock layouts, derived from ggml-quants.c and gguf-py.
//! Not a GGUF container parser, quantizer, training pipeline or GPU kernel.
//! Reference commit: ggml-org/llama.cpp f7591a6a01f9273de03331732bdc0b39721168be

pub const SUPERBLOCK: usize = 256;
pub const Q3_BYTES: usize = 110;
pub const Q4_BYTES: usize = 144;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    WrongLength { actual: usize, expected: usize },
}

fn fp16_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = (bits >> 10) & 0x1f;
    let fraction = bits & 0x3ff;
    match exponent {
        0 => sign * (f32::from(fraction) * 2.0f32.powi(-24)),
        31 if fraction == 0 => sign * f32::INFINITY,
        31 => f32::NAN,
        _ => sign * (1.0 + f32::from(fraction) / 1024.0) * 2.0f32.powi(i32::from(exponent) - 15),
    }
}

fn f16_le(bytes: &[u8]) -> f32 {
    fp16_to_f32(u16::from_le_bytes([bytes[0], bytes[1]]))
}

/// Actual ggml block_q3_K layout:
/// hmask[32], qs[64], scales[12], superblock FP16 d[2].
///
/// 16 signed six-bit scale values (s - 32), then quant bits:
/// 2-bit low value from four 32-byte bitplane groups,
/// 1-bit hmask determining whether to subtract 4.
pub fn decode_q3_k(wire: &[u8]) -> Result<[f32; SUPERBLOCK], DecodeError> {
    if wire.len() != Q3_BYTES {
        return Err(DecodeError::WrongLength {
            actual: wire.len(),
            expected: Q3_BYTES,
        });
    }
    let hmask = &wire[..32];
    let qs = &wire[32..96];
    let scales = &wire[96..108];
    let d = f16_le(&wire[108..110]);
    Ok(std::array::from_fn(|i| {
        let half = i / 128;
        let group_in_half = (i % 128) / 16;
        let scale_index = half * 8 + group_in_half;
        let low_nibble = if scale_index < 8 {
            scales[scale_index] & 15
        } else {
            scales[scale_index - 8] >> 4
        };
        let high_pair = (scales[8 + scale_index % 4] >> (2 * (scale_index / 4))) & 3;
        let signed_scale = i32::from(low_nibble | (high_pair << 4)) - 32;

        let offset = half * 32 + (group_in_half % 2) * 16 + (i % 16);
        let bitplane = group_in_half / 2;
        let low = (qs[offset] >> (2 * bitplane)) & 3;
        let hbit_set = hmask[offset] & (1 << bitplane) != 0;
        let signed_quant = i32::from(low) - if hbit_set { 0 } else { 4 };
        d * (signed_scale as f32) * (signed_quant as f32)
    }))
}

/// Actual ggml block_q4_K layout:
/// FP16 d[2], FP16 dmin[2], 12 bytes of 6-bit scales and minima,
/// followed by 128 densely packed nibbles. Eight 32-value groups.
pub fn decode_q4_k(wire: &[u8]) -> Result<[f32; SUPERBLOCK], DecodeError> {
    if wire.len() != Q4_BYTES {
        return Err(DecodeError::WrongLength {
            actual: wire.len(),
            expected: Q4_BYTES,
        });
    }
    let d = f16_le(&wire[0..2]);
    let dmin = f16_le(&wire[2..4]);
    let scales = &wire[4..16];
    let qs = &wire[16..144];
    Ok(std::array::from_fn(|i| {
        let group = i / 32;
        let (sc, mn) = if group < 4 {
            (scales[group] & 63, scales[group + 4] & 63)
        } else {
            let ds = (scales[group + 4] & 15) | ((scales[group - 4] >> 6) << 4);
            let ms = (scales[group + 4] >> 4) | ((scales[group] >> 6) << 4);
            (ds, ms)
        };
        let packed = qs[(group / 2) * 32 + (i % 32)];
        let q = if group % 2 == 0 {
            packed & 15
        } else {
            packed >> 4
        };
        (d * f32::from(sc)) * f32::from(q) - (dmin * f32::from(mn))
    }))
}

/// Deterministic byte-pattern fixture with deliberate FP16 scales.
/// This fixture is independently recreated by gguf_k_reference.py in CI.
fn fixture(format: &str, case: usize) -> Vec<u8> {
    let len = match format {
        "Q3_K" => Q3_BYTES,
        "Q4_K" => Q4_BYTES,
        _ => unreachable!("fixed test formats"),
    };
    let mut wire: Vec<u8> = (0..len)
        .map(|i| ((i * 73 + (i / 4) * 19 + case * 41) % 256) as u8)
        .collect();
    let scales: (u16, u16) = match case {
        0 => (0x3c00, 0x3800), // d=1, dmin=0.5
        1 => (0xbc00, 0x4000), // d=-1, dmin=2
        2 => (0x0001, 0x3c00), // subnormal d, dmin=1
        3 => (0x0000, 0x0000), // zero scales
        4 => (0x3e00, 0x3a00), // d=1.5, dmin=0.75
        _ => unreachable!("fixed cases"),
    };
    if format == "Q3_K" {
        wire[108..110].copy_from_slice(&scales.0.to_le_bytes());
    } else {
        wire[0..2].copy_from_slice(&scales.0.to_le_bytes());
        wire[2..4].copy_from_slice(&scales.1.to_le_bytes());
    }
    wire
}

/// Machine-readable exact f32 bits for independent official gguf-py parity.
/// Output is never a performance or reconstruction-quality claim.
pub fn emit_reference_vectors() {
    for format in ["Q3_K", "Q4_K"] {
        for case in 0..5 {
            let data = fixture(format, case);
            let values = if format == "Q3_K" {
                decode_q3_k(&data).unwrap()
            } else {
                decode_q4_k(&data).unwrap()
            };
            for (index, value) in values.iter().enumerate() {
                println!("{format},{case},{index},{:08x}", value.to_bits());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_gguf_superblock_bytes_and_bits_per_weight() {
        assert_eq!(Q3_BYTES, 32 + 64 + 12 + 2);
        assert_eq!(Q4_BYTES, 2 + 2 + 12 + 128);
        assert_eq!(8.0 * Q3_BYTES as f64 / 256.0, 3.4375);
        assert_eq!(8.0 * Q4_BYTES as f64 / 256.0, 4.5);
    }

    #[test]
    fn f16_le_handles_signed_zero_subnormal_normal_and_infinity() {
        assert_eq!(fp16_to_f32(0x3c00), 1.0);
        assert_eq!(fp16_to_f32(0xbc00), -1.0);
        assert_eq!(fp16_to_f32(0x3800), 0.5);
        assert_eq!(fp16_to_f32(1), 2.0f32.powi(-24));
        assert!(fp16_to_f32(0x8000).is_sign_negative());
        assert!(fp16_to_f32(0x7c00).is_infinite());
        assert!(fp16_to_f32(0x7e00).is_nan());
    }

    #[test]
    fn reject_partial_extra_or_wrong_superblocks() {
        for len in [0, 32, 109, 111, 143, 145, 256] {
            assert_eq!(
                decode_q3_k(&vec![0u8; len]),
                Err(DecodeError::WrongLength { actual: len, expected: 110 })
            );
            assert_eq!(
                decode_q4_k(&vec![0u8; len]),
                Err(DecodeError::WrongLength { actual: len, expected: 144 })
            );
        }
    }

    #[test]
    fn zero_data_with_unit_scales_has_reference_offsets() {
        let mut q3 = [0u8; Q3_BYTES];
        q3[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
        // scale = -32, quant = -4 => +128, all lanes.
        assert!(decode_q3_k(&q3).unwrap().iter().all(|x| *x == 128.0));
        let mut q4 = [0u8; Q4_BYTES];
        q4[0..2].copy_from_slice(&0x3c00u16.to_le_bytes());
        // zero dmin and zero scales => every value = 0.
        assert!(decode_q4_k(&q4).unwrap().iter().all(|x| *x == 0.0));
    }

    #[test]
    fn fully_randomized_bitfields_and_f16_scales_remain_finite() {
        for format in ["Q3_K", "Q4_K"] {
            for case in 0..5 {
                let data = fixture(format, case);
                let output = if format == "Q3_K" {
                    decode_q3_k(&data).unwrap()
                } else {
                    decode_q4_k(&data).unwrap()
                };
                assert_eq!(output.len(), SUPERBLOCK);
                assert!(output.iter().all(|v| v.is_finite()));
            }
        }
    }
}
