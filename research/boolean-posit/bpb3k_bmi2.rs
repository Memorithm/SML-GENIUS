//! Experimental BPB3-K BMI2 PDEP + AVX2/FMA Q8_K vector dot.
//!
//! Directly expands three Boolean weight planes into signed i8 coefficients
//! in CPU registers, never a dense weight vector in memory. Its eight-lane
//! f32 FMA/reduction order mirrors the pinned upstream Q3_K AVX2 vector path.
//! Research-only: no default model switch, speedup or physical claim.

const WEIGHT_BLOCK: usize = 110;
const ACTIVATION_BLOCK: usize = 292;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bmi2Error {
    InvalidWeightLength(usize),
    InvalidActivationLength(usize),
    CpuFeatureUnavailable,
}

fn u64_le(raw: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(raw[offset..offset + 8].try_into().unwrap())
}

fn scale(raw: &[u8], group: usize) -> i32 {
    let sc = &raw[96..108];
    let lo = if group < 8 { sc[group] & 15 } else { sc[group - 8] >> 4 };
    let hi = (sc[8 + group % 4] >> (2 * (group / 4))) & 3;
    i32::from(lo | (hi << 4)) - 32
}

/// Exactly widen a binary16 value for the stock-vector scalar oracle and
/// AVX2 FMA path. Do not change accumulation order or assume fast-math.
fn fp16(raw: u16) -> f32 {
    let sign = ((raw as u32) & 0x8000) << 16;
    let exp = ((raw >> 10) & 31) as u32;
    let frac = u32::from(raw & 1023);
    if exp == 0 {
        if frac == 0 {
            return f32::from_bits(sign);
        }
        let val = (frac as f32) * (1.0f32 / 16_777_216.0f32);
        return if sign == 0 { val } else { -val };
    }
    if exp == 31 {
        return f32::from_bits(sign | 0x7f80_0000 | (frac << 13));
    }
    f32::from_bits(sign | ((exp + 112) << 23) | (frac << 13))
}

fn global_scale(w: &[u8], a: &[u8]) -> f32 {
    fp16(u16::from_le_bytes([w[108], w[109]]))
        * f32::from_le_bytes(a[..4].try_into().unwrap())
}

/// Public, dynamically guarded entry point; no BMI2 assumption on other
/// architectures and no exposure to malformed or partial superblocks.
pub fn dot_bpb3k_bmi2_stock_vector(w: &[u8], a: &[u8]) -> Result<f32, Bmi2Error> {
    if w.is_empty() || w.len() % WEIGHT_BLOCK != 0 {
        return Err(Bmi2Error::InvalidWeightLength(w.len()));
    }
    if a.len() != w.len() / WEIGHT_BLOCK * ACTIVATION_BLOCK {
        return Err(Bmi2Error::InvalidActivationLength(a.len()));
    }
    #[cfg(target_arch = "x86_64")]
    {
        if !(std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("bmi2"))
        {
            return Err(Bmi2Error::CpuFeatureUnavailable);
        }
        return Ok(unsafe { x86::dot(w, a) });
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        Err(Bmi2Error::CpuFeatureUnavailable)
    }
}

#[cfg(target_arch = "x86_64")]
mod x86 {
    use super::*;
    use std::arch::x86_64::*;

    // Each 8-bit source plane is deposited into the corresponding bit
    // position of eight output i8 coefficients. Two's-complement sign
    // extension uses 0xFC for all negative Q3_K coefficients [-4,-1].
    #[target_feature(enable = "bmi2")]
    unsafe fn decode8(sign: u64, low: u64, high: u64, chunk: usize) -> u64 {
        let shift = chunk * 8;
        let s = (sign >> shift) as u8;
        let l = (low >> shift) as u8 ^ s;
        let h = (high >> shift) as u8 ^ s;
        let first = _pdep_u64(u64::from(l), 0x0101_0101_0101_0101);
        let second = _pdep_u64(u64::from(h), 0x0202_0202_0202_0202);
        let negative = _pdep_u64(u64::from(s), 0x0101_0101_0101_0101)
            .wrapping_mul(0xfc);
        first | second | negative
    }

    #[target_feature(enable = "avx2,fma,bmi2")]
    pub unsafe fn dot(weights: &[u8], activations: &[u8]) -> f32 {
        let mut accum = _mm256_setzero_ps();
        for (w, a) in weights
            .chunks_exact(WEIGHT_BLOCK)
            .zip(activations.chunks_exact(ACTIVATION_BLOCK))
        {
            let mut lanes = _mm256_setzero_si256();
            for bank in 0..4 {
                let off = bank * 24;
                let sign = u64_le(w, off);
                let low = u64_le(w, off + 8);
                let high = u64_le(w, off + 16);
                for part in 0..2 {
                    let group = bank * 4 + part * 2;
                    let chunk = part * 4;
                    let lo_q = _mm_set_epi64x(
                        decode8(sign, low, high, chunk + 1) as i64,
                        decode8(sign, low, high, chunk) as i64,
                    );
                    let hi_q = _mm_set_epi64x(
                        decode8(sign, low, high, chunk + 3) as i64,
                        decode8(sign, low, high, chunk + 2) as i64,
                    );
                    let lo_a = _mm_loadu_si128(a.as_ptr().add(4 + group * 16).cast());
                    let hi_a = _mm_loadu_si128(a.as_ptr().add(4 + (group + 1) * 16).cast());
                    let lo_products = _mm256_madd_epi16(
                        _mm256_mullo_epi16(
                            _mm256_cvtepi8_epi16(lo_q),
                            _mm256_set1_epi16(scale(w, group) as i16),
                        ),
                        _mm256_cvtepi8_epi16(lo_a),
                    );
                    let hi_products = _mm256_madd_epi16(
                        _mm256_mullo_epi16(
                            _mm256_cvtepi8_epi16(hi_q),
                            _mm256_set1_epi16(scale(w, group + 1) as i16),
                        ),
                        _mm256_cvtepi8_epi16(hi_a),
                    );
                    // Same four-coefficient stock AVX2 integer lanes.
                    let joined = _mm256_hadd_epi32(lo_products, hi_products);
                    let joined = _mm256_permutevar8x32_epi32(
                        joined,
                        _mm256_setr_epi32(0, 1, 4, 5, 2, 3, 6, 7),
                    );
                    lanes = _mm256_add_epi32(lanes, joined);
                }
            }
            accum = _mm256_fmadd_ps(
                _mm256_set1_ps(global_scale(w, a)),
                _mm256_cvtepi32_ps(lanes),
                accum,
            );
        }
        // Exact pinned Q3_K AVX2 grouping and balanced horizontal reduction.
        let sum = _mm_add_ps(_mm256_extractf128_ps(accum, 1), _mm256_castps256_ps128(accum));
        let sum = _mm_add_ps(sum, _mm_movehl_ps(sum, sum));
        _mm_cvtss_f32(_mm_add_ss(sum, _mm_movehdup_ps(sum)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[path = "bpb3k.rs"]
    #[allow(dead_code)]
    mod wire;

    fn scalar_stock_vector(weights: &[u8], acts: &[u8]) -> f32 {
        let mut lanes = [0f32; 8];
        for (w, a) in weights
            .chunks_exact(WEIGHT_BLOCK)
            .zip(acts.chunks_exact(ACTIVATION_BLOCK))
        {
            let mut ints = [0i32; 8];
            for i in 0..256 {
                let bank = i / 64;
                let bit = i % 64;
                let off = bank * 24;
                let neg = (u64_le(w, off) >> bit) & 1 != 0;
                let lo = ((u64_le(w, off + 8) >> bit) & 1) as i32;
                let hi = ((u64_le(w, off + 16) >> bit) & 1) as i32;
                let q = if neg { -1 - lo - 2 * hi } else { lo + 2 * hi };
                ints[(i % 32) / 4] +=
                    q * scale(w, i / 16) * i32::from(a[4 + i] as i8);
            }
            let d = global_scale(w, a);
            for lane in 0..8 {
                lanes[lane] = d.mul_add(ints[lane] as f32, lanes[lane]);
            }
        }
        let s0 = lanes[4] + lanes[0];
        let s1 = lanes[5] + lanes[1];
        let s2 = lanes[6] + lanes[2];
        let s3 = lanes[7] + lanes[3];
        (s0 + s2) + (s1 + s3)
    }

    #[test]
    fn bmi2_rejects_malformed_or_partial_blocks() {
        assert_eq!(dot_bpb3k_bmi2_stock_vector(&[], &[]),
                   Err(Bmi2Error::InvalidWeightLength(0)));
        assert_eq!(dot_bpb3k_bmi2_stock_vector(&[0u8; 109], &[0u8; 292]),
                   Err(Bmi2Error::InvalidWeightLength(109)));
        assert_eq!(dot_bpb3k_bmi2_stock_vector(&[0u8; 110], &[0u8; 291]),
                   Err(Bmi2Error::InvalidActivationLength(291)));
    }

    #[test]
    fn bmi2_matches_independent_scalar_stock_vector_order() {
        if !cfg!(target_arch = "x86_64") {
            return;
        }
        if std::is_x86_feature_detected!("bmi2")
            && std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
        {
            for blocks in [1usize, 2, 3, 9, 33, 257] {
                for case in 0..32 {
                    let mut original = vec![0u8; blocks * WEIGHT_BLOCK];
                    let mut acts = vec![0u8; blocks * ACTIVATION_BLOCK];
                    let mut weights = Vec::with_capacity(original.len());
                    for block in 0..blocks {
                        for (i, b) in original[block*110..(block+1)*110].iter_mut().enumerate() {
                            *b = (i * 71 + block * 49 + case * 37) as u8;
                        }
                        let fp16 = [0x3c00u16,0xbc00,0x3555,0x0001,0x8000,0x7bff]
                            [(case + block) % 6];
                        original[block*110+108..(block+1)*110].copy_from_slice(&fp16.to_le_bytes());
                        weights.extend_from_slice(
                            &wire::q3_to_bpb3k(&original[block*110..(block+1)*110]).unwrap()
                        );
                        let a = &mut acts[block*292..(block+1)*292];
                        let d = [1.0f32, -0.125, 0.1, 1e-20, 1e20]
                            [(case + block) % 5];
                        a[..4].copy_from_slice(&d.to_le_bytes());
                        for (i, x) in a[4..260].iter_mut().enumerate() {
                            *x = (i*33+case*11+block*7) as u8;
                        }
                    }
                    let expected = scalar_stock_vector(&weights, &acts);
                    let actual = dot_bpb3k_bmi2_stock_vector(&weights, &acts).unwrap();
                    assert_eq!(actual.to_bits(), expected.to_bits(),
                        "blocks={blocks} case={case} actual={actual} expected={expected}");
                }
            }
        }
    }

    #[test]
    fn bmi2_bit_sign_extension_covers_every_q3_value() {
        #[cfg(target_arch = "x86_64")]
        {
            if !std::is_x86_feature_detected!("bmi2") { return; }
            for q in -4i32..=3 {
                let sign = u64::from(q < 0);
                let magnitude = if q < 0 { -q - 1 } else { q };
                let low = u64::from(magnitude & 1 != 0);
                let high = u64::from(magnitude & 2 != 0);
                let result = unsafe { x86::decode8(sign, low, high, 0) };
                assert_eq!(result as u8 as i8 as i32, q, "q={q}");
            }
        }
    }
}
