//! Research-only deterministic five-bit quantization from **source-precision**
//! FP32, FP16 and BF16 tensors. Produces BPB3-K Q5-178 blocks.
//!
//! This is not a GGUF or ggml production quantizer. It performs a bounded
//! exhaustive per-subgroup signed scale search against source weights,
//! minimizing squared reconstruction error at a fixed shared FP16 scale.
//! The shared scale is selected deterministically from exact powers of two.
//! It neither uses pretrained calibration data nor claims model quality.
#![deny(unsafe_op_in_unsafe_fn)]

#[path = "bpb3k_q5_178_tiled_avx2.rs"]
#[allow(dead_code)]
mod q5;

pub const BLOCK_WEIGHTS: usize = 256;
pub const SUBGROUP_WEIGHTS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuantizeError {
    NonFiniteSource(usize),
    InvalidGlobalF16(u16),
    RangeNotRepresentable,
    PackError,
}

#[derive(Clone, Debug)]
pub struct Quantized {
    pub bytes: [u8; q5::Q5_BYTES],
    /// Sum of squared reconstruction residuals against input FP32 values.
    /// Measured *before* any Q8_K activation quantization or model inference.
    pub squared_error: f64,
    pub global_scale_f16_bits: u16,
    pub signed_scales: [i8; 16],
    pub signed_codes: [i8; 256],
}

/// Exact conversion of all finite IEEE 754 binary16 values to binary32.
/// Nonfinite payloads remain nonfinite and are rejected at the API boundary.
pub fn f16_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = u32::from((bits >> 10) & 31);
    let fraction = u32::from(bits & 0x03ff);
    if exponent == 0 {
        if fraction == 0 {
            return f32::from_bits(sign);
        }
        let magnitude = (fraction as f32) * f32::from_bits(0x3380_0000);
        return if sign != 0 { -magnitude } else { magnitude };
    }
    if exponent == 31 {
        return f32::from_bits(sign | 0x7f80_0000 | (fraction << 13));
    }
    f32::from_bits(sign | ((exponent + 112) << 23) | (fraction << 13))
}

/// Encodes an *exact* positive binary16 power of two. Powers below -14
/// are subnormal, while e=-14..15 are normal. No rounding performed.
fn f16_power_of_two(exponent: i32) -> u16 {
    debug_assert!((-24..=15).contains(&exponent));
    if exponent < -14 {
        1u16 << (exponent + 24)
    } else {
        ((exponent + 15) as u16) << 10
    }
}

/// Choose the finest supported global binary16 power-of-two whose
/// maximum reachable positive magnitude (127*15*g) covers input.
/// When all weights are zero, +1.0 is used to avoid subnormal noise.
fn choose_global(source: &[f32; 256]) -> Result<u16, QuantizeError> {
    let mut maximum = 0.0f64;
    for (i, &value) in source.iter().enumerate() {
        if !value.is_finite() {
            return Err(QuantizeError::NonFiniteSource(i));
        }
        maximum = maximum.max(f64::from(value).abs());
    }
    if maximum == 0.0 {
        return Ok(0x3c00);
    }
    for exponent in -24..=15 {
        // 15 positive codes times the highest *positive* i8 scale.
        // Negative scales remain available as an additional option.
        if maximum <= 1905.0 * (2.0f64).powi(exponent) {
            return Ok(f16_power_of_two(exponent));
        }
    }
    Err(QuantizeError::RangeNotRepresentable)
}

/// Independently optimize 16 integer coefficients at every signed
/// subgroup scale. Error is measured in f64, with a deterministic
/// tie-break preferring scale 0, then small positive scales, then
/// increasingly negative scales.
///
/// This exact discrete search cannot be worse than a signed four-bit
/// candidate search using the SAME global scale: [-8,7] is a subset
/// of [-16,15]. Floating-point model logits remain unqualified.
fn optimize(source: &[f32; 256], global: f64, lower: i8, upper: i8) -> ([i8; 256], [i8; 16], f64) {
    let mut codes = [0i8; 256];
    let mut scales = [0i8; 16];
    let mut total_sse = 0.0f64;
    for subgroup in 0..16 {
        let mut best_sse = f64::INFINITY;
        let mut best_scale = 0i8;
        let mut best_codes = [0i8; 16];
        for choice in 0..256usize {
            let scale = if choice <= 127 {
                choice as i16
            } else {
                127 - choice as i16
            };
            let multiplier = global * f64::from(scale);
            let mut trial = [0i8; 16];
            let mut err = 0.0f64;
            for (local, code) in trial.iter_mut().enumerate() {
                let value = f64::from(source[subgroup * 16 + local]);
                // A zero-scale candidate represents precisely zero.
                let q = if scale == 0 {
                    0.0
                } else {
                    (value / multiplier)
                        .round()
                        .clamp(f64::from(lower), f64::from(upper))
                };
                *code = q as i8;
                let residual = value - q * multiplier;
                err += residual * residual;
            }
            if err < best_sse {
                best_sse = err;
                best_codes = trial;
                best_scale = scale as i8;
            }
        }
        codes[subgroup * 16..subgroup * 16 + 16].copy_from_slice(&best_codes);
        scales[subgroup] = best_scale;
        total_sse += best_sse;
    }
    (codes, scales, total_sse)
}

pub fn quantize_f32_fixed_global(
    source: &[f32; BLOCK_WEIGHTS],
    global_scale_f16_bits: u16,
) -> Result<Quantized, QuantizeError> {
    let global = f16_to_f32(global_scale_f16_bits);
    if !global.is_finite() || global <= 0.0 {
        return Err(QuantizeError::InvalidGlobalF16(global_scale_f16_bits));
    }
    for (index, &value) in source.iter().enumerate() {
        if !value.is_finite() {
            return Err(QuantizeError::NonFiniteSource(index));
        }
    }
    let (signed_codes, signed_scales, squared_error) = optimize(source, f64::from(global), -16, 15);
    let bytes = q5::pack_signed(&signed_codes, &signed_scales, global_scale_f16_bits)
        .map_err(|_| QuantizeError::PackError)?;
    Ok(Quantized {
        bytes,
        squared_error,
        global_scale_f16_bits,
        signed_scales,
        signed_codes,
    })
}

pub fn quantize_f32(source: &[f32; BLOCK_WEIGHTS]) -> Result<Quantized, QuantizeError> {
    let global = choose_global(source)?;
    quantize_f32_fixed_global(source, global)
}

/// FP16 source input is read at its original precision; no Q3_K/Q4
/// surrogate may enter this path without explicit caller conversion.
pub fn quantize_fp16(source: &[u16; BLOCK_WEIGHTS]) -> Result<Quantized, QuantizeError> {
    let values = source.map(f16_to_f32);
    quantize_f32(&values)
}

/// BF16 source input uses the exact IEEE bit-position widening.
pub fn quantize_bf16(source: &[u16; BLOCK_WEIGHTS]) -> Result<Quantized, QuantizeError> {
    let values = source.map(|bits| f32::from_bits(u32::from(bits) << 16));
    quantize_f32(&values)
}

/// Independent public reconstruction oracle for source-level error checks.
/// This intentionally does not use the SIMD coefficient decoder.
pub fn dequantize(q: &Quantized) -> [f32; BLOCK_WEIGHTS] {
    let global = f16_to_f32(q.global_scale_f16_bits);
    std::array::from_fn(|i| {
        f32::from(q.signed_codes[i]) * f32::from(q.signed_scales[i / SUBGROUP_WEIGHTS]) * global
    })
}

/// Optimized 4-bit integer quantizer under the *same* 16 signed i8
/// subgroup scales and FP16 global scale, used as an analytical baseline.
/// It is not the existing Q4-146 lossless Q3_K representation, and must
/// never be reported as an actual Q4-146 ggml model result.
pub fn q4_reference_sse(
    source: &[f32; 256],
    global_scale_f16_bits: u16,
) -> Result<f64, QuantizeError> {
    let global = f16_to_f32(global_scale_f16_bits);
    if !global.is_finite() || global <= 0.0 {
        return Err(QuantizeError::InvalidGlobalF16(global_scale_f16_bits));
    }
    for (i, &v) in source.iter().enumerate() {
        if !v.is_finite() {
            return Err(QuantizeError::NonFiniteSource(i));
        }
    }
    Ok(optimize(source, f64::from(global), -8, 7).2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deterministic_source(seed: usize) -> [f32; 256] {
        std::array::from_fn(|index| {
            let mut x = (index as u32)
                .wrapping_mul(0x9e3779b9)
                .wrapping_add((seed as u32).wrapping_mul(0xa511e9b3));
            x ^= x >> 15;
            x = x.wrapping_mul(0x85ebca6b);
            x ^= x >> 13;
            let uniform = (x % 60001) as f32 / 30000.0 - 1.0;
            let scale = match seed % 5 {
                0 => 0.25,
                1 => 1.0,
                2 => 30.0,
                3 => 0.00001,
                _ => 300.0,
            };
            uniform * scale
        })
    }

    #[test]
    fn f16_widening_preserves_signed_zero_subnormal_and_nonfinite() {
        assert_eq!(f16_to_f32(0x0000).to_bits(), 0u32);
        assert_eq!(f16_to_f32(0x8000).to_bits(), 0x80000000);
        assert_eq!(f16_to_f32(0x3c00), 1.0);
        assert_eq!(f16_to_f32(0x0001), 2.0f32.powi(-24));
        assert_eq!(f16_to_f32(0x0400), 2.0f32.powi(-14));
        assert!(f16_to_f32(0x7c00).is_infinite());
        assert!(f16_to_f32(0x7e00).is_nan());
        for exponent in -24..=15 {
            assert_eq!(
                f16_to_f32(f16_power_of_two(exponent)),
                2.0f32.powi(exponent)
            );
        }
    }

    #[test]
    fn exact_source_precision_and_zero_block() {
        let source = std::array::from_fn(|i| {
            // All 32 distinct signed codes occur repeatedly; global=1.
            ((i % 32) as i8 - 16) as f32
        });
        let result = quantize_f32_fixed_global(&source, 0x3c00).unwrap();
        assert_eq!(result.squared_error, 0.0);
        assert_eq!(dequantize(&result), source);
        assert_eq!(result.bytes.len(), 178);
        let zero = quantize_f32(&[0.0f32; 256]).unwrap();
        assert_eq!(zero.global_scale_f16_bits, 0x3c00);
        assert_eq!(zero.squared_error, 0.0);
        assert_eq!(zero.signed_scales, [0; 16]);
        assert_eq!(dequantize(&zero), [0.0; 256]);
    }

    #[test]
    fn q5_reconstruction_error_never_worse_than_same_scale_q4_oracle() {
        for seed in 0..32 {
            let original = deterministic_source(seed);
            let result = quantize_f32(&original).unwrap();
            let baseline = q4_reference_sse(&original, result.global_scale_f16_bits).unwrap();
            assert!(
                result.squared_error <= baseline + 1e-11,
                "seed {seed}: {} > {baseline}",
                result.squared_error
            );
            let reconstruction = dequantize(&result);
            let actual_sse: f64 = original
                .iter()
                .zip(reconstruction.iter())
                .map(|(&a, &b)| (f64::from(a) - f64::from(b)).powi(2))
                .sum();
            // The reconstruction is rounded to f32, while our search
            // evaluates exact f64 products; allow only FP32 roundoff.
            assert!(
                (actual_sse - result.squared_error).abs()
                    <= (1e-4 * actual_sse.max(result.squared_error)).max(1e-12),
                "seed={seed} actual={actual_sse} predicted={}",
                result.squared_error
            );
            assert!(result.signed_codes.iter().all(|&x| (-16..=15).contains(&x)));
        }
    }

    #[test]
    fn fp16_and_bf16_are_real_source_precision_and_reject_nonfinite() {
        let f16: [u16; 256] = std::array::from_fn(|i| match i % 4 {
            0 => 0x3c00,
            1 => 0xbc00,
            2 => 0x3800,
            _ => 0x3400,
        });
        let fp16_q = quantize_fp16(&f16).unwrap();
        assert!(fp16_q.squared_error.is_finite());
        let fp16_values = f16.map(f16_to_f32);
        let fp16_exact = quantize_f32_fixed_global(&fp16_values, 0x3400).unwrap();
        assert_eq!(fp16_exact.squared_error, 0.0);
        assert_eq!(dequantize(&fp16_exact), fp16_values);
        let bf16: [u16; 256] = std::array::from_fn(|i| match i % 4 {
            0 => 0x3f80,
            1 => 0xbf80,
            2 => 0x3f00,
            _ => 0x3e80,
        });
        let bf16_q = quantize_bf16(&bf16).unwrap();
        assert!(bf16_q.squared_error.is_finite());
        let bf16_values = bf16.map(|x| f32::from_bits(u32::from(x) << 16));
        let bf16_exact = quantize_f32_fixed_global(&bf16_values, 0x3400).unwrap();
        assert_eq!(bf16_exact.squared_error, 0.0);
        assert_eq!(dequantize(&bf16_exact), bf16_values);
        let mut invalid = f16;
        invalid[17] = 0x7c00;
        assert_eq!(
            quantize_fp16(&invalid).unwrap_err(),
            QuantizeError::NonFiniteSource(17)
        );
        let mut invalid_bf16 = bf16;
        invalid_bf16[42] = 0x7fc1;
        assert_eq!(
            quantize_bf16(&invalid_bf16).unwrap_err(),
            QuantizeError::NonFiniteSource(42)
        );
    }

    #[test]
    fn strict_invalid_values_and_out_of_range() {
        let mut source = [0.0f32; 256];
        source[13] = f32::NAN;
        assert_eq!(
            quantize_f32(&source).unwrap_err(),
            QuantizeError::NonFiniteSource(13)
        );
        source[13] = f32::INFINITY;
        assert_eq!(
            quantize_f32(&source).unwrap_err(),
            QuantizeError::NonFiniteSource(13)
        );
        source[13] = f32::MAX;
        assert_eq!(
            quantize_f32(&source).unwrap_err(),
            QuantizeError::RangeNotRepresentable
        );
        source[13] = -0.4;
        assert_eq!(
            quantize_f32_fixed_global(&source, 0).unwrap_err(),
            QuantizeError::InvalidGlobalF16(0)
        );
        assert_eq!(
            quantize_f32_fixed_global(&source, 0x7c00).unwrap_err(),
            QuantizeError::InvalidGlobalF16(0x7c00)
        );
        assert_eq!(
            quantize_f32_fixed_global(&source, 0xbc00).unwrap_err(),
            QuantizeError::InvalidGlobalF16(0xbc00)
        );
    }

    #[test]
    fn deterministic_repeated_output_and_q5_simd_dot_parity() {
        let original = deterministic_source(14);
        let a = quantize_f32(&original).unwrap();
        let b = quantize_f32(&original).unwrap();
        assert_eq!(a.bytes, b.bytes);
        assert_eq!(a.squared_error.to_bits(), b.squared_error.to_bits());
        let mut q8 = [0u8; 292];
        q8[..4].copy_from_slice(&0x3f800000u32.to_le_bytes());
        for i in 0..256 {
            q8[4 + i] = (i as i32 - 128) as i8 as u8;
        }
        let scalar = q5::scalar_lanes(&a.bytes, &q8).unwrap();
        if std::is_x86_feature_detected!("avx2") {
            let tiled = q5::dot_tiled::<1>(&a.bytes, &q8).unwrap();
            assert_eq!(tiled[0].lanes, scalar.lanes);
        }
    }
}
