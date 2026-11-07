//! Matched deterministic correctness panel: original Q3_K direct dot vs
//! BPB3-K direct bitplane dot, plus an independent original-wire decoder.
//!
//! This is a correctness experiment only. No elapsed-time, throughput,
//! physical traffic, memory or model-quality claim is supported.

#![forbid(unsafe_code)]

#[path = "q3k_direct.rs"]
#[allow(dead_code)]
mod q3k_direct;

#[path = "bpb3k_direct.rs"]
#[allow(dead_code)]
mod bpb3k_direct;

#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod bpb3k;

#[path = "gguf_k.rs"]
#[allow(dead_code)]
mod gguf_k;

#[cfg(test)]
mod tests {
    use super::*;

    fn q3_wire(case: usize, scale_bits: u16) -> [u8; 110] {
        let mut raw: [u8; 110] =
            std::array::from_fn(|i| (i * 73 + (i / 4) * 19 + case * 41) as u8);
        raw[108..110].copy_from_slice(&scale_bits.to_le_bytes());
        raw
    }

    fn masks(case: usize, pattern: usize) -> [u64; 4] {
        std::array::from_fn(|group| match pattern {
            0 => 0,
            1 => u64::MAX,
            2 => 0xaaaa_aaaa_aaaa_aaaa,
            3 => 0x5555_5555_5555_5555,
            4 => 1u64 << ((case + group) % 64),
            5 => !(1u64 << ((case + group) % 64)),
            6 => 0xffff_0000_ffff_0000,
            7 => 0x0000_ffff_0000_ffff,
            _ => {
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

    fn independent_scalar_dot(raw: &[u8; 110], x: &[u64; 4]) -> f64 {
        let w = gguf_k::decode_q3_k(raw).unwrap();
        w.iter()
            .enumerate()
            .map(|(i, &v)| {
                f64::from(v) * if (x[i / 64] >> (i % 64)) & 1 != 0 { -1.0 } else { 1.0 }
            })
            .sum()
    }

    fn assert_matched(raw: &[u8; 110], x: &[u64; 4]) {
        let bpb = bpb3k::q3_to_bpb3k(raw).unwrap();
        let direct_q3 = q3k_direct::dot_q3k_bipolar(raw, x).unwrap();
        let direct_bpb = bpb3k_direct::dot_bpb3k_bipolar(&bpb, x).unwrap();
        assert_eq!(direct_q3.weighted_integer, direct_bpb.weighted_integer);
        assert_eq!(direct_q3.global_scale_f16_bits, direct_bpb.global_scale_f16_bits);
        assert_eq!(direct_q3.global_scale_f16_bits, u16::from_le_bytes([raw[108], raw[109]]));
    }

    #[test]
    fn paired_64_fixtures_times_64_bipolar_masks() {
        const GLOBAL_SCALES: [u16; 8] = [
            0x3c00, // 1.0
            0xbc00, // -1.0
            0x3800, // 0.5
            0x0001, // positive subnormal
            0x0000, // positive zero
            0x8000, // negative zero
            0x7c00, // positive infinity (metadata preserved, no float usage)
            0x7e00, // NaN (metadata preserved, no float usage)
        ];
        let mut checked = 0usize;
        for case in 0..64 {
            let scale_bits = GLOBAL_SCALES[case % GLOBAL_SCALES.len()];
            let raw = q3_wire(case, scale_bits);
            for pattern in 0..64 {
                let x = masks(case, pattern);
                assert_matched(&raw, &x);
                if scale_bits == 0x3c00 || scale_bits == 0xbc00 || scale_bits == 0x3800 {
                    let result = q3k_direct::dot_q3k_bipolar(&raw, &x).unwrap();
                    let s = match scale_bits {
                        0x3c00 => 1.0,
                        0xbc00 => -1.0,
                        _ => 0.5,
                    };
                    assert_eq!(
                        f64::from(result.weighted_integer) * s,
                        independent_scalar_dot(&raw, &x),
                        "case={case} pattern={pattern}"
                    );
                }
                checked += 1;
            }
        }
        assert_eq!(checked, 4_096);
    }

    fn uniform_six_bit_scales(raw: &mut [u8; 110], code: u8) {
        raw[96..108].fill(0);
        for subgroup in 0..16 {
            if subgroup < 8 {
                raw[96 + subgroup] |= code & 15;
            } else {
                raw[96 + subgroup - 8] |= (code & 15) << 4;
            }
            raw[96 + 8 + subgroup % 4] |= ((code >> 4) & 3) << (2 * (subgroup / 4));
        }
    }

    #[test]
    fn full_six_bit_subgroup_scale_range_and_sign_boundaries() {
        // All 64 possible scale codes, including -32, 0 and +31.
        for code in 0..64u8 {
            let mut raw = q3_wire(31 + usize::from(code), 0x3c00);
            uniform_six_bit_scales(&mut raw, code);
            for pattern in 0..8 {
                let x = masks(usize::from(code), pattern);
                assert_matched(&raw, &x);
                let expected = independent_scalar_dot(&raw, &x);
                let got = q3k_direct::dot_q3k_bipolar(&raw, &x).unwrap();
                assert_eq!(f64::from(got.weighted_integer), expected);
            }
        }
    }

    #[test]
    fn preserve_q3_minus_four_and_plus_three_cases() {
        for positive in [false, true] {
            let mut raw = [0u8; 110];
            raw[108..110].copy_from_slice(&0x3c00u16.to_le_bytes());
            if positive {
                // All Q3 integer coefficients +3; high sign mask set,
                // both low code bits set.
                raw[..32].fill(0xff);
                raw[32..96].fill(0xff);
            }
            let direct = q3k_direct::dot_q3k_bipolar(&raw, &[0; 4]).unwrap();
            assert_eq!(direct.weighted_integer, if positive { -96 * 256 } else { 128 * 256 });
            assert_matched(&raw, &[0; 4]);
            assert_matched(&raw, &[u64::MAX; 4]);
        }
    }

    #[test]
    fn reject_wrong_length_in_both_wire_paths() {
        for len in [0usize, 1, 31, 32, 64, 96, 109, 111, 144, 256, 1024] {
            let data = vec![0u8; len];
            let q3 = q3k_direct::dot_q3k_bipolar(&data, &[0; 4]);
            let bpb = bpb3k_direct::dot_bpb3k_bipolar(&data, &[0; 4]);
            assert_eq!(q3, Err(q3k_direct::Error::WrongLength { actual: len }));
            assert_eq!(bpb, Err(bpb3k::Error::WrongLength { actual: len }));
        }
    }
}
