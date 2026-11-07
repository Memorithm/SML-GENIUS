//! OCP MXFP4 / E2M1 element and E8M0 scale *research reference*.
//! Logical wire record: two consecutive [scale: u8, packed nibbles: [u8; 16]]
//! for each 64-value block (34 bytes total). This does not specify tensor
//! memory alignment, vendor-specific packing, or a GPU execution kernel.
//!
//! Based on OCP Microscaling Formats MX v1.0, sections 5.2, 5.3.3 and 5.4.1.
//! The *encoder* searches scales in [-24,27] to match the BPB-3 panel's
//! experimental search range; that selection policy is not an OCP requirement.
//! The *decoder* accepts the complete finite E8M0 scale range (0..=254).
//! FP4 narrowing uses nearest with ties to even; saturation is to +/-6.
pub const BLOCK: usize = 32;
pub const WIRE_BYTES: usize = 17;
pub const PAIR_BYTES: usize = 34;
pub const PANEL_MIN_EXPONENT: i32 = -24;
pub const PANEL_MAX_EXPONENT: i32 = 27;

const LEVELS: [f64; 8] = [0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    NanScale,
}

pub fn decode_element(code: u8) -> f64 {
    let magnitude = LEVELS[usize::from(code & 7)];
    if code & 8 != 0 {
        -magnitude
    } else {
        magnitude
    }
}

/// E8M0 exponent bias 127; 255 is a NaN scale, not a finite scale.
pub fn decode_scale(code: u8) -> Result<f64, DecodeError> {
    if code == u8::MAX {
        return Err(DecodeError::NanScale);
    }
    Ok(2.0f64.powi(i32::from(code) - 127))
}

/// Implement roundTiesToEven by choosing the code with an even low bit on
/// equidistant neighbors. This preserves the sign bit for negative zero.
pub fn encode_element(value: f64) -> u8 {
    assert!(value.is_finite(), "STOP_NONFINITE_MXFP4_INPUT");
    let magnitude = value.abs();
    let mut chosen = 0usize;
    let mut error = f64::INFINITY;
    for (index, &level) in LEVELS.iter().enumerate() {
        let candidate_error = (magnitude - level).abs();
        if candidate_error < error
            || (candidate_error == error && index % 2 == 0 && chosen % 2 != 0)
        {
            error = candidate_error;
            chosen = index;
        }
    }
    (chosen as u8) | if value.is_sign_negative() { 8 } else { 0 }
}

pub fn decode32(wire: &[u8; WIRE_BYTES]) -> Result<[f64; BLOCK], DecodeError> {
    let scale = decode_scale(wire[0])?;
    Ok(std::array::from_fn(|i| {
        let byte = wire[1 + i / 2];
        let code = if i % 2 == 0 { byte & 15 } else { byte >> 4 };
        scale * decode_element(code)
    }))
}

/// This is an explicit, reproducible *experimental* per-block SSE search.
/// It is not a claim that all deployable MXFP4 quantizers use this rule.
pub fn encode32(values: &[f64; BLOCK]) -> [u8; WIRE_BYTES] {
    assert!(
        values.iter().all(|v| v.is_finite()),
        "STOP_NONFINITE_MXFP4_INPUT"
    );
    let mut best = [0u8; WIRE_BYTES];
    let mut best_error = f64::INFINITY;
    for exponent in PANEL_MIN_EXPONENT..=PANEL_MAX_EXPONENT {
        let scale = 2.0f64.powi(exponent);
        let mut candidate = [0u8; WIRE_BYTES];
        candidate[0] = (exponent + 127) as u8;
        let mut error = 0.0;
        for (index, &value) in values.iter().enumerate() {
            let code = encode_element(value / scale);
            if index % 2 == 0 {
                candidate[1 + index / 2] |= code;
            } else {
                candidate[1 + index / 2] |= code << 4;
            }
            let delta = value - scale * decode_element(code);
            error += delta * delta;
        }
        if error < best_error {
            best_error = error;
            best = candidate;
        }
    }
    best
}

pub fn encode64(values: &[f64; 64]) -> [u8; PAIR_BYTES] {
    let mut first = [0.0; BLOCK];
    let mut second = [0.0; BLOCK];
    first.copy_from_slice(&values[..BLOCK]);
    second.copy_from_slice(&values[BLOCK..]);
    let a = encode32(&first);
    let b = encode32(&second);
    let mut wire = [0u8; PAIR_BYTES];
    wire[..WIRE_BYTES].copy_from_slice(&a);
    wire[WIRE_BYTES..].copy_from_slice(&b);
    wire
}

pub fn decode64(wire: &[u8; PAIR_BYTES]) -> Result<[f64; 64], DecodeError> {
    let a: &[u8; WIRE_BYTES] = wire[..WIRE_BYTES].try_into().expect("MXFP4 half block");
    let b: &[u8; WIRE_BYTES] = wire[WIRE_BYTES..].try_into().expect("MXFP4 half block");
    let first = decode32(a)?;
    let second = decode32(b)?;
    let mut result = [0.0; 64];
    result[..BLOCK].copy_from_slice(&first);
    result[BLOCK..].copy_from_slice(&second);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_e2m1_codebook_and_negative_zero() {
        let expected = [0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0];
        for (code, &level) in expected.iter().enumerate() {
            assert_eq!(decode_element(code as u8), level);
            assert_eq!(decode_element((code + 8) as u8), -level);
            assert_eq!(encode_element(level), code as u8);
            assert_eq!(encode_element(-level), (code + 8) as u8);
        }
        assert!(decode_element(8).is_sign_negative());
        assert!(decode_element(0).is_sign_positive());
    }

    #[test]
    fn round_to_nearest_ties_even_and_saturate() {
        let ties = [
            (0.25, 0),
            (0.75, 2),
            (1.25, 2),
            (1.75, 4),
            (2.5, 4),
            (3.5, 6),
            (5.0, 6),
        ];
        for (value, code) in ties {
            assert_eq!(encode_element(value), code);
            assert_eq!(encode_element(-value), code + 8);
        }
        assert_eq!(encode_element(100.0), 7);
        assert_eq!(encode_element(-100.0), 15);
        assert_eq!(encode_element(-0.0), 8);
    }

    #[test]
    fn full_finite_scale_range_and_nan_sentinel() {
        assert_eq!(decode_scale(127), Ok(1.0));
        assert_eq!(decode_scale(126), Ok(0.5));
        assert_eq!(decode_scale(128), Ok(2.0));
        assert_eq!(decode_scale(0), Ok(2.0f64.powi(-127)));
        assert_eq!(decode_scale(254), Ok(2.0f64.powi(127)));
        assert_eq!(decode_scale(255), Err(DecodeError::NanScale));
    }

    #[test]
    fn two_blocks_are_exactly_34_bytes_with_independent_scales() {
        let values = std::array::from_fn(|i| {
            if i < 32 {
                if i % 2 == 0 {
                    1.5
                } else {
                    -3.0
                }
            } else if i % 2 == 0 {
                64.0
            } else {
                -128.0
            }
        });
        let wire = encode64(&values);
        assert_eq!(wire.len(), 34);
        assert_ne!(wire[0], wire[17], "separate 32-value scales");
        assert_eq!(decode64(&wire).unwrap(), values);
    }

    #[test]
    fn nan_scale_rejected_in_either_half_and_packing_is_stable() {
        let mut wire = encode64(&[0.0; 64]);
        let copy = wire;
        assert_eq!(wire, copy);
        wire[0] = 255;
        assert_eq!(decode64(&wire), Err(DecodeError::NanScale));
        wire[0] = 127;
        wire[17] = 255;
        assert_eq!(decode64(&wire), Err(DecodeError::NanScale));
    }

    #[test]
    fn two_values_per_byte_low_nibble_first() {
        let mut wire = [0u8; WIRE_BYTES];
        wire[0] = 127;
        wire[1] = 0xE3; // +1.5 in lane 0, -4 in lane 1
        let actual = decode32(&wire).unwrap();
        assert_eq!(actual[0], 1.5);
        assert_eq!(actual[1], -4.0);
        assert_eq!(actual[2], 0.0);
    }
}
