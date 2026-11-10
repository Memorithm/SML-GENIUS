//! Research-only alternative BPB3-K weight encodings against signed Q8_K.
//! 146 B: 128 B of signed Q3 coefficients encoded as unsigned (q+4)
//! nibbles, 16 signed i8 subgroup scales, 2 raw FP16 bytes.
//! 160 B: identical 146 B semantics with padding and a 160-byte stride.
//! 228 B: TWO consecutive lossless 114-byte bitplane blocks, 512 weights.
//!
//! Every candidate is benchmarked against unchanged optimized original
//! Q3_K C AVX2. These new payloads are NOT GGUF Q3_K wire formats.
//! No changes to inference or model defaults.
#![deny(unsafe_op_in_unsafe_fn)]

#[path = "bpb3k.rs"]
#[allow(dead_code)]
mod bpb3k;
#[path = "bpb3k_preencoded114_avx2.rs"]
#[allow(dead_code)]
mod previous;

pub const Q3_BYTES: usize = 110;
pub const Q4_BYTES: usize = 146;
pub const Q4_PADDED_BYTES: usize = 160;
pub const SCALE114_BYTES: usize = 114;
pub const PAIR228_BYTES: usize = 228;
pub const Q8_BYTES: usize = 292;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Error {
    BadSourceLen(usize),
    BadWeightsLen(usize),
    BadActivationsLen(usize),
    UnsupportedCpu,
}

fn signed_scale(raw: &[u8], subgroup: usize) -> i8 {
    let scales = &raw[96..108];
    let lo = if subgroup < 8 {
        scales[subgroup] & 15
    } else {
        scales[subgroup - 8] >> 4
    };
    let hi = (scales[8 + subgroup % 4] >> (2 * (subgroup / 4))) & 3;
    (i16::from(lo | (hi << 4)) - 32) as i8
}

/// Completely independent read of original ggml Q3_K wire format.
pub fn original_q3_code(raw: &[u8], i: usize) -> i8 {
    let half = i / 128;
    let quadrant = (i % 128) / 32;
    let lo = (raw[32 + half * 32 + i % 32] >> (2 * quadrant)) & 3;
    let sign = (raw[i % 32] >> (4 * half + quadrant)) & 1;
    lo as i8 - if sign == 0 { 4 } else { 0 }
}

pub fn original_integer_dot(raw: &[u8], q8: &[u8]) -> i32 {
    (0..256)
        .map(|i| {
            i32::from(original_q3_code(raw, i))
                * i32::from(signed_scale(raw, i / 16))
                * i32::from(q8[4 + i] as i8)
        })
        .sum()
}

/// Encode 256 exact Q3_K signed integer codes in 128 nibble bytes.
/// Each 32-code group encodes the lower 16 Q3 coefficients in its
/// low nibble, the upper 16 in its high nibble. This permits one
/// 16-byte load and one AVX2 cross-128-lane permutation per 32 codes.
pub fn transcode_q4<const N: usize>(source_q3: &[u8]) -> Result<[u8; N], Error> {
    if source_q3.len() != Q3_BYTES {
        return Err(Error::BadSourceLen(source_q3.len()));
    }
    if N != Q4_BYTES && N != Q4_PADDED_BYTES {
        return Err(Error::BadWeightsLen(N));
    }
    let mut out = [0u8; N];
    for group in 0..8 {
        for lane in 0..16 {
            let lo = (original_q3_code(source_q3, group * 32 + lane) + 4) as u8;
            let hi = (original_q3_code(source_q3, group * 32 + lane + 16) + 4) as u8;
            out[group * 16 + lane] = lo | (hi << 4);
        }
    }
    for subgroup in 0..16 {
        out[128 + subgroup] = signed_scale(source_q3, subgroup) as u8;
    }
    out[N - 2..N].copy_from_slice(&source_q3[108..110]);
    // N=160 has 14 zero padding bytes at 144..158, tested explicitly.
    Ok(out)
}

pub fn transcode_pair228(q3_a: &[u8], q3_b: &[u8]) -> Result<[u8; PAIR228_BYTES], Error> {
    if q3_a.len() != Q3_BYTES {
        return Err(Error::BadSourceLen(q3_a.len()));
    }
    if q3_b.len() != Q3_BYTES {
        return Err(Error::BadSourceLen(q3_b.len()));
    }
    let a = bpb3k::q3_to_bpb3k(q3_a).unwrap();
    let b = bpb3k::q3_to_bpb3k(q3_b).unwrap();
    let pa = previous::preencode_scales(&a).unwrap();
    let pb = previous::preencode_scales(&b).unwrap();
    let mut out = [0u8; PAIR228_BYTES];
    out[..114].copy_from_slice(&pa);
    out[114..228].copy_from_slice(&pb);
    Ok(out)
}

/// Integer result plus bit-preserved scale metadata. Only single-block
/// integer and unit-scale FP32 parity are qualified by this research.
/// Arbitrary-scale multiblock FP32 summation order is not claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dot {
    pub integer: i32,
    pub global_scale_f16_bits: u16,
    pub q8_scale_f32_bits: u32,
}

#[cfg(target_arch = "x86_64")]
mod simd {
    use super::*;
    use core::arch::x86_64::*;

    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn q4<const N: usize>(weights: &[u8], activation: &[u8]) -> i32 {
        unsafe {
            let mut acc = _mm256_setzero_si256();
            let nibble_mask = _mm256_set1_epi8(15);
            let four = _mm256_set1_epi8(4);
            for group in 0..8 {
                // Broadcast 16 input bytes across two 128-bit lanes.
                let packed = _mm_loadu_si128(weights.as_ptr().add(16 * group).cast::<__m128i>());
                let broadcast = _mm256_broadcastsi128_si256(packed);
                let low = _mm256_and_si256(broadcast, nibble_mask);
                let high = _mm256_and_si256(_mm256_srli_epi16(broadcast, 4), nibble_mask);
                // The 0x20 permutation selects lower lanes from the
                // two inputs to produce 32 consecutive unsigned q+4.
                let unsigned_q = _mm256_permute2x128_si256(low, high, 0x20);
                let q8 = _mm256_loadu_si256(activation.as_ptr().add(4 + 32 * group).cast());
                let paired = _mm256_sub_epi16(
                    _mm256_maddubs_epi16(unsigned_q, q8),
                    _mm256_maddubs_epi16(four, q8),
                );
                let s0 = i16::from(weights[128 + 2 * group] as i8);
                let s1 = i16::from(weights[128 + 2 * group + 1] as i8);
                let scales = _mm256_inserti128_si256(
                    _mm256_castsi128_si256(_mm_set1_epi16(s0)),
                    _mm_set1_epi16(s1),
                    1,
                );
                acc = _mm256_add_epi32(acc, _mm256_madd_epi16(paired, scales));
            }
            let mut lanes = [0i32; 8];
            _mm256_storeu_si256(lanes.as_mut_ptr().cast(), acc);
            lanes.iter().sum()
        }
    }

    #[target_feature(enable = "avx2")]
    #[inline]
    unsafe fn bits32(
        bits: u32,
        selector: __m256i,
        bitmask: __m256i,
        zero: __m256i,
        one: __m256i,
    ) -> __m256i {
        let v = _mm256_set1_epi32(bits as i32);
        let x = _mm256_and_si256(_mm256_shuffle_epi8(v, selector), bitmask);
        _mm256_andnot_si256(_mm256_cmpeq_epi8(x, zero), one)
    }

    /// Pipeline two different, consecutive 256-element blocks in one AVX2
    /// 512-element call. Two independent vector accumulators and staggered
    /// activation loads in the same loop; no 8-row aliasing or GGML
    /// nrc=4 claim. Each 114 B subblock contains exact 16 signed scales.
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn paired228(weights: &[u8], activation: &[u8]) -> [i32; 2] {
        unsafe {
            let selector = _mm256_setr_epi8(
                0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3,
                3, 3, 3, 3,
            );
            let mask = _mm256_setr_epi8(
                1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64,
                -128, 1, 2, 4, 8, 16, 32, 64, -128,
            );
            let one = _mm256_set1_epi8(1);
            let four = _mm256_set1_epi8(4);
            let zero = _mm256_setzero_si256();
            let mut acc = [zero; 2];
            for group in 0..4usize {
                for pair in 0..2usize {
                    // Both Q8 vectors are consumed before either source
                    // block is advanced to its next 32-coefficient pair.
                    let offset = group * 64 + pair * 32;
                    let in0 = _mm256_loadu_si256(
                        activation.as_ptr().add(4 + offset).cast::<__m256i>(),
                    );
                    let in1 = _mm256_loadu_si256(
                        activation.as_ptr().add(Q8_BYTES + 4 + offset).cast::<__m256i>(),
                    );
                    for (block, input) in [(0usize, in0), (1usize, in1)] {
                        let w = &weights[block * SCALE114_BYTES..(block + 1) * SCALE114_BYTES];
                        let p = group * 24 + pair * 4;
                        let raw = |at: usize| {
                            u32::from_le_bytes(w[at..at + 4].try_into().unwrap())
                        };
                        let sign = bits32(raw(p), selector, mask, zero, one);
                        let b0 = bits32(raw(p + 8), selector, mask, zero, one);
                        let b1 = bits32(raw(p + 16), selector, mask, zero, one);
                        let bit0 = _mm256_xor_si256(b0, sign);
                        let bit1 = _mm256_slli_epi16(_mm256_xor_si256(b1, sign), 1);
                        let plus4_top = _mm256_slli_epi16(_mm256_xor_si256(sign, one), 2);
                        let code = _mm256_or_si256(bit0, _mm256_or_si256(bit1, plus4_top));
                        let paired = _mm256_sub_epi16(
                            _mm256_maddubs_epi16(code, input),
                            _mm256_maddubs_epi16(four, input),
                        );
                        let subgroup = group * 4 + pair * 2;
                        let s0 = i16::from(w[96 + subgroup] as i8);
                        let s1 = i16::from(w[96 + subgroup + 1] as i8);
                        let scales = _mm256_inserti128_si256(
                            _mm256_castsi128_si256(_mm_set1_epi16(s0)),
                            _mm_set1_epi16(s1),
                            1,
                        );
                        acc[block] = _mm256_add_epi32(
                            acc[block],
                            _mm256_madd_epi16(paired, scales),
                        );
                    }
                }
            }
            let mut out = [0i32; 2];
            for block in 0..2 {
                let mut lanes = [0i32; 8];
                _mm256_storeu_si256(lanes.as_mut_ptr().cast(), acc[block]);
                out[block] = lanes.iter().sum();
            }
            out
        }
    }
}

pub fn dot_q4<const N: usize>(weights: &[u8], activation: &[u8]) -> Result<Dot, Error> {
    if N != Q4_BYTES && N != Q4_PADDED_BYTES {
        return Err(Error::BadWeightsLen(N));
    }
    if weights.len() != N {
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
        let integer = unsafe { simd::q4::<N>(weights, activation) };
        Ok(Dot {
            integer,
            global_scale_f16_bits: u16::from_le_bytes(weights[N - 2..N].try_into().unwrap()),
            q8_scale_f32_bits: u32::from_le_bytes(activation[..4].try_into().unwrap()),
        })
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        Err(Error::UnsupportedCpu)
    }
}

pub fn dot_pair228(weights: &[u8], activation: &[u8]) -> Result<[Dot; 2], Error> {
    if weights.len() != PAIR228_BYTES {
        return Err(Error::BadWeightsLen(weights.len()));
    }
    if activation.len() != Q8_BYTES * 2 {
        return Err(Error::BadActivationsLen(activation.len()));
    }
    #[cfg(target_arch = "x86_64")]
    {
        if !std::is_x86_feature_detected!("avx2") {
            return Err(Error::UnsupportedCpu);
        }
        let ints = unsafe { simd::paired228(weights, activation) };
        Ok(std::array::from_fn(|block| {
            let at = block * SCALE114_BYTES;
            let input_at = block * Q8_BYTES;
            Dot {
                integer: ints[block],
                global_scale_f16_bits: u16::from_le_bytes(
                    weights[at + 112..at + 114].try_into().unwrap(),
                ),
                q8_scale_f32_bits: u32::from_le_bytes(
                    activation[input_at..input_at + 4].try_into().unwrap(),
                ),
            }
        }))
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        Err(Error::UnsupportedCpu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q3(seed: usize, scale: u16) -> [u8; Q3_BYTES] {
        let mut w: [u8; Q3_BYTES] =
            std::array::from_fn(|i| (i * 73 + i / 4 * 19 + seed * 41) as u8);
        w[108..110].copy_from_slice(&scale.to_le_bytes());
        w
    }

    fn q8(seed: usize, pattern: usize, scale: u32) -> [u8; Q8_BYTES] {
        let mut a = [0u8; Q8_BYTES];
        a[..4].copy_from_slice(&scale.to_le_bytes());
        for i in 0..256 {
            a[4 + i] = match pattern % 8 {
                0 => 0,
                1 => 127,
                2 => 128,
                3 => 255,
                4 => if i % 2 == 0 { 127 } else { 128 },
                5 => if i % 2 == 0 { 1 } else { 255 },
                6 => i as u8,
                _ => {
                    let mut x = (seed as u32) ^ (i as u32).wrapping_mul(0x9e3779b9);
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    x as u8
                }
            }
        }
        a
    }

    #[test]
    fn original_q3_oracle_vs_q4_146_160_and_pair228() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        for seed in 0..96 {
            let w0 = q3(seed, [0x3c00, 0xbc00, 0x7e01, 0x8000][seed % 4]);
            let w1 = q3(seed + 1, 0x7e01);
            let a0 = q8(seed, seed, [0x3f800000, 0x7fc12345][seed % 2]);
            let a1 = q8(seed + 1, seed + 3, 0x7fcabcde);
            let q4_146 = transcode_q4::<Q4_BYTES>(&w0).unwrap();
            let q4_160 = transcode_q4::<Q4_PADDED_BYTES>(&w0).unwrap();
            assert!(q4_160[144..158].iter().all(|&b| b == 0));
            assert_eq!(&q4_146[..144], &q4_160[..144]);
            assert_eq!(&q4_146[144..146], &q4_160[158..160]);
            for i in 0..256 {
                let byte = q4_146[(i / 32) * 16 + (i % 16)];
                let nibble = if i % 32 < 16 { byte & 15 } else { byte >> 4 };
                assert_eq!(nibble as i8 - 4, original_q3_code(&w0, i));
            }
            let o0 = original_integer_dot(&w0, &a0);
            let o1 = original_integer_dot(&w1, &a1);
            let result146 = dot_q4::<Q4_BYTES>(&q4_146, &a0).unwrap();
            let result160 = dot_q4::<Q4_PADDED_BYTES>(&q4_160, &a0).unwrap();
            assert_eq!(result146.integer, o0, "seed={seed}");
            assert_eq!(result160.integer, o0, "seed={seed}");
            assert_eq!(result146.global_scale_f16_bits, [0x3c00, 0xbc00, 0x7e01, 0x8000][seed % 4]);
            assert_eq!(result146.q8_scale_f32_bits, [0x3f800000, 0x7fc12345][seed % 2]);
            let pair = transcode_pair228(&w0, &w1).unwrap();
            let mut both_a = [0u8; 2 * Q8_BYTES];
            both_a[..Q8_BYTES].copy_from_slice(&a0);
            both_a[Q8_BYTES..].copy_from_slice(&a1);
            let got = dot_pair228(&pair, &both_a).unwrap();
            assert_eq!(got[0].integer, o0, "pair0 seed={seed}");
            assert_eq!(got[1].integer, o1, "pair1 seed={seed}");
            assert_eq!(got[1].global_scale_f16_bits, 0x7e01);
            assert_eq!(got[1].q8_scale_f32_bits, 0x7fcabcde);
        }
    }

    #[test]
    fn malformed_sizes_and_unaligned_sources() {
        if !std::is_x86_feature_detected!("avx2") {
            return;
        }
        let q3_0 = q3(13, 0x3c00);
        let q3_1 = q3(14, 0x3c00);
        let a = q8(14, 6, 0x3f800000);
        for n in [0, 1, 109, 111, 228] {
            assert_eq!(transcode_q4::<Q4_BYTES>(&vec![0u8; n]), Err(Error::BadSourceLen(n)));
        }
        assert_eq!(transcode_q4::<228>(&q3_0), Err(Error::BadWeightsLen(228)));
        let v146 = transcode_q4::<Q4_BYTES>(&q3_0).unwrap();
        let v160 = transcode_q4::<Q4_PADDED_BYTES>(&q3_0).unwrap();
        let pair = transcode_pair228(&q3_0, &q3_1).unwrap();
        for offset in 0..32 {
            let mut wb = vec![0xa5u8; Q4_PADDED_BYTES + 64];
            let mut ab = vec![0x5au8; Q8_BYTES + 64];
            wb[offset..offset + Q4_BYTES].copy_from_slice(&v146);
            ab[31 - offset..31 - offset + Q8_BYTES].copy_from_slice(&a);
            let wcopy = wb.clone();
            let acopy = ab.clone();
            assert_eq!(
                dot_q4::<Q4_BYTES>(
                    &wb[offset..offset + Q4_BYTES],
                    &ab[31 - offset..31 - offset + Q8_BYTES]
                ).unwrap().integer,
                original_integer_dot(&q3_0, &a)
            );
            assert_eq!(wb, wcopy);
            assert_eq!(ab, acopy);
            assert_eq!(dot_q4::<Q4_PADDED_BYTES>(&v160[..159], &a), Err(Error::BadWeightsLen(159)));
            assert_eq!(dot_q4::<Q4_BYTES>(&v146, &a[..291]), Err(Error::BadActivationsLen(291)));
        }
        assert_eq!(dot_pair228(&pair[..227], &vec![0; Q8_BYTES * 2]), Err(Error::BadWeightsLen(227)));
        assert_eq!(dot_pair228(&pair, &vec![0; Q8_BYTES * 2 - 1]), Err(Error::BadActivationsLen(583)));
    }
}
