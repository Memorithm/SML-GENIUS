//! Safe Rust x86-64 boundary for hand-written AVX2/PSHUFB Q3-equivalent dot.
//! The external .S is a controlled, research-only System V ABI kernel.
#![deny(unsafe_op_in_unsafe_fn)]
#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod codec;
#[path = "bpb3k_pshufb_avx2.rs"]
#[allow(dead_code)]
mod pshufb;

pub use pshufb::{PshufbError, SimdQ8Dot, Q8_K_BYTES};

#[cfg(target_arch = "x86_64")]
unsafe extern "C" {
    fn mem_bpb3k_dot_avx2_asm(weights: *const u8, activation: *const u8) -> i32;
}

/// The input buffers may be unaligned. Exactly 110 and 292 bytes must exist.
/// Returns raw FP16/FP32 block-scale bits without applying floating-point
/// reductions, and a fully signed, exact integer accumulation.
pub fn dot_bpb3k_q8_hand_asm(
    weights: &[u8],
    activation: &[u8],
) -> Result<SimdQ8Dot, PshufbError> {
    if weights.len() != 110 {
        return Err(PshufbError::WrongWeightLength {
            actual: weights.len(),
        });
    }
    if activation.len() != Q8_K_BYTES {
        return Err(PshufbError::WrongActivationLength {
            actual: activation.len(),
        });
    }
    #[cfg(target_arch = "x86_64")]
    {
        if !std::is_x86_feature_detected!("avx2") {
            return Err(PshufbError::UnsupportedCpu);
        }
        // SAFETY: exact lengths and CPU support were checked. Assembly uses
        // only unaligned loads for user buffers, read-only aligned constants,
        // caller-saved registers and no writes to either buffer.
        let weighted_integer =
            unsafe { mem_bpb3k_dot_avx2_asm(weights.as_ptr(), activation.as_ptr()) };
        debug_assert!(weighted_integer.abs() <= 4_194_304);
        Ok(SimdQ8Dot {
            weighted_integer,
            global_scale_f16_bits: u16::from_le_bytes([weights[108], weights[109]]),
            activation_scale_f32_bits: u32::from_le_bytes(activation[..4].try_into().unwrap()),
        })
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        Err(PshufbError::UnsupportedCpu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q3(case: usize, fp16: u16) -> [u8; 110] {
        let mut original: [u8; 110] =
            std::array::from_fn(|i| (i * 73 + i / 4 * 19 + case * 41) as u8);
        original[108..110].copy_from_slice(&fp16.to_le_bytes());
        original
    }

    fn q8(case: usize, pattern: usize, bits: u32) -> [u8; Q8_K_BYTES] {
        let mut v = [0u8; Q8_K_BYTES];
        v[..4].copy_from_slice(&bits.to_le_bytes());
        for i in 0..256 {
            v[4 + i] = match pattern % 8 {
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
        v
    }

    // Oracle reads ORIGINAL Q3_K fields, not BPB3-K or the handwritten asm.
    fn original_q3_integer_oracle(q3: &[u8; 110], q8: &[u8; Q8_K_BYTES]) -> i32 {
        let mut result = 0i32;
        for i in 0..256 {
            let half = i / 128;
            let plane = (i % 128) / 32;
            let local = i % 32;
            let magnitude = (q3[32 + half * 32 + local] >> (plane * 2)) & 3;
            let negative = (q3[local] >> (half * 4 + plane)) & 1 == 0;
            let signed = i32::from(magnitude) - if negative { 4 } else { 0 };
            let subgroup = i / 16;
            let scales = &q3[96..108];
            let low = if subgroup < 8 {
                scales[subgroup] & 15
            } else {
                scales[subgroup - 8] >> 4
            };
            let high = (scales[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
            let signed_scale = i32::from(low | (high << 4)) - 32;
            result += signed * signed_scale * i32::from(q8[4 + i] as i8);
        }
        result
    }

    #[test]
    fn compare_original_q3_scalar_rust_intrinsics_and_hand_asm() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        for case in 0..96 {
            let fp16 = [0x3c00, 0xbc00, 0x7e01, 0x8000][case % 4];
            let original = q3(case, fp16);
            let converted = codec::q3_to_bpb3k(&original).unwrap();
            for pattern in 0..16 {
                let activation = q8(case, pattern, [0x3f800000, 0x7fc12345][pattern % 2]);
                let original_dot = original_q3_integer_oracle(&original, &activation);
                let instrinsics =
                    pshufb::dot_bpb3k_q8_pshufb(&converted, &activation).unwrap();
                let assembly = dot_bpb3k_q8_hand_asm(&converted, &activation).unwrap();
                assert_eq!(assembly.weighted_integer, original_dot, "case={case} pattern={pattern}");
                assert_eq!(assembly, instrinsics, "case={case} pattern={pattern}");
            }
        }
    }

    #[test]
    fn all_signed_q8_codes_and_unaligned_input_addresses() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let mut activation = q8(33, 6, 0x7fc12345);
        for (i, element) in activation[4..260].iter_mut().enumerate() {
            *element = i as u8;
        }
        for case in 0..32 {
            let original = q3(case, (case as u16) ^ 0x8001);
            let weights = codec::q3_to_bpb3k(&original).unwrap();
            let oracle = original_q3_integer_oracle(&original, &activation);
            let mut padded_w = vec![0x5au8; 110 + 32 + 32];
            let mut padded_a = vec![0xa5u8; Q8_K_BYTES + 32 + 32];
            for offset in 0..32 {
                padded_w[offset..offset + 110].copy_from_slice(&weights);
                padded_a[31 - offset..31 - offset + Q8_K_BYTES].copy_from_slice(&activation);
                let result = dot_bpb3k_q8_hand_asm(
                    &padded_w[offset..offset + 110],
                    &padded_a[31 - offset..31 - offset + Q8_K_BYTES],
                )
                .unwrap();
                assert_eq!(result.weighted_integer, oracle, "case={case} offset={offset}");
                assert_eq!(result.global_scale_f16_bits, (case as u16) ^ 0x8001);
                assert_eq!(result.activation_scale_f32_bits, 0x7fc12345);
            }
        }
    }

    #[test]
    fn extreme_codes_and_invalid_lengths() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let mut original = [0u8; 110];
        original[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
        let weights = codec::q3_to_bpb3k(&original).unwrap();
        let mut activation = q8(0, 0, 0x3f800000);
        for (code, expected) in [(128u8, -4_194_304i32), (127u8, 4_161_536), (0u8, 0)] {
            activation[4..260].fill(code);
            assert_eq!(
                dot_bpb3k_q8_hand_asm(&weights, &activation)
                    .unwrap()
                    .weighted_integer,
                expected
            );
        }
        for len in [0usize, 1, 96, 109, 111] {
            assert_eq!(
                dot_bpb3k_q8_hand_asm(&vec![0u8; len], &activation),
                Err(PshufbError::WrongWeightLength { actual: len })
            );
        }
        for len in [0usize, 1, 260, 291, 293] {
            assert_eq!(
                dot_bpb3k_q8_hand_asm(&weights, &vec![0u8; len]),
                Err(PshufbError::WrongActivationLength { actual: len })
            );
        }
    }
}
