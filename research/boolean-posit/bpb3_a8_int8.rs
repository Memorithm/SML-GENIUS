//! Exact signed INT8 activation dot/matvec over original BPB-3-A8 Boolean wires.
//! Uses eight bipolar POPCOUNT evaluations plus one all-positive correction.
//! This is a correctness-first research kernel, not a speed or model claim.
#![forbid(unsafe_code)]

#[path = "bpb3_a8_superblock.rs"]
#[allow(dead_code)]
mod a8;
#[path = "bpb3_a8_tensor.rs"]
#[allow(dead_code)]
mod tensor;

const LANES: usize = 256;
const BITS: usize = 8;
const OFFSET: i128 = 1;

/// Result integer is in exact 2^-24 units (not rounded floating point).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidWire,
    InvalidTensor,
    WrongOutputLength,
    ArithmeticOverflow,
    NonIntegralResult,
}

fn signed_activation_mask(input: &[i8; LANES], bit: usize) -> [u64; 4] {
    let mut mask = [0u64; 4];
    for (index, &value) in input.iter().enumerate() {
        // A8's bipolar mask uses 1=-1. For an ordinary input bit b,
        // 2*b-1 maps bit=0 to -1 and bit=1 to +1.
        if ((value as u8 >> bit) & 1) == 0 {
            mask[index / 64] |= 1u64 << (index % 64);
        }
    }
    mask
}

/// Exact signed-INT8 dot with a 256-weight BPB-3-A8 row, with no dense
/// coefficient reconstruction or floating-point multiplication.
///
/// Two's-complement x = Σ_{b=0..6}2^b xb - 128*x7, where xb ∈ {0,1}.
/// For bipolar sb=2xb-1, Σw*xb=(Db+S)/2, with
/// Db=Σw*sb and S=Σw; hence
/// 2*Σw*x = Σ_{b=0..6}2^b Db - 128*D7 - S.
/// The original a8::dot_exact provides every Db and S in 2^-24 units.
/// Nine fixed bitplane invocations trade compute for numeric generality.
pub fn dot_i8_exact(wire: &[u8], input: &[i8; LANES]) -> Result<i128, Error> {
    let mut numerator = 0i128;
    for bit in 0..BITS {
        let mask = signed_activation_mask(input, bit);
        let db = a8::dot_exact(wire, &mask).map_err(|_| Error::InvalidWire)?;
        let coefficient = if bit == 7 { -128i128 } else { 1i128 << bit };
        numerator = numerator
            .checked_add(
                db.checked_mul(coefficient)
                    .ok_or(Error::ArithmeticOverflow)?,
            )
            .ok_or(Error::ArithmeticOverflow)?;
    }
    let s = a8::dot_exact(wire, &[0u64; 4]).map_err(|_| Error::InvalidWire)?;
    numerator = numerator
        .checked_sub(s.checked_mul(OFFSET).ok_or(Error::ArithmeticOverflow)?)
        .ok_or(Error::ArithmeticOverflow)?;
    if numerator % 2 != 0 {
        return Err(Error::NonIntegralResult);
    }
    Ok(numerator / 2)
}

/// Reuses an existing caller-owned output slice, one value per tensor row.
/// A size error is detected before touching any output element.
pub fn matvec_i8_into(
    view: &tensor::TensorView<'_>,
    input: &[i8; LANES],
    output: &mut [i128],
) -> Result<(), Error> {
    if output.len() != view.len() {
        return Err(Error::WrongOutputLength);
    }
    for (index, slot) in output.iter_mut().enumerate() {
        let wire = view.row(index).map_err(|_| Error::InvalidTensor)?;
        *slot = dot_i8_exact(wire, input)?;
    }
    Ok(())
}

pub fn matvec_i8(view: &tensor::TensorView<'_>, input: &[i8; LANES]) -> Result<Vec<i128>, Error> {
    let mut output = vec![0i128; view.len()];
    matvec_i8_into(view, input, &mut output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(index: usize) -> Vec<u8> {
        let base = (index % 52) as i8 - 24;
        let groups = std::array::from_fn(|g| {
            let mode = 1 + ((index + g) % 3) as u8;
            let max = (1i8 << mode) - 1;
            let codes = std::array::from_fn(|lane| {
                let magnitude = ((index + g + lane) % (usize::from(max as u8) + 1)) as i8;
                if (index + g + lane) % 2 == 0 {
                    magnitude
                } else {
                    -magnitude
                }
            });
            a8::Group {
                mode,
                exponent: base,
                codes,
                sse: 0.0,
            }
        });
        a8::pack(&a8::Superblock {
            base,
            groups,
            sse: 0.0,
        })
        .unwrap()
    }

    fn scalar_oracle(wire: &[u8], input: &[i8; LANES]) -> i128 {
        let (weights, scales) = a8::unpack(wire).unwrap();
        weights.iter().enumerate().fold(0i128, |sum, (i, &weight)| {
            let factor = 1i128 << (i32::from(scales[i / 16]) + 24);
            sum + i128::from(weight) * i128::from(input[i]) * factor
        })
    }

    fn fixture(kind: usize) -> [i8; LANES] {
        std::array::from_fn(|i| match kind {
            0 => 0,
            1 => 1,
            2 => -1,
            3 => 127,
            4 => -128,
            5 => {
                if i % 2 == 0 {
                    127
                } else {
                    -128
                }
            }
            6 => (i as u8).wrapping_mul(47) as i8,
            7 => (i as u8).wrapping_mul(181).wrapping_add(13) as i8,
            _ => unreachable!(),
        })
    }

    #[test]
    fn exact_i8_dots_agree_with_independent_dense_decoder() {
        for index in 0..64 {
            let row = wire(index);
            for kind in 0..8 {
                let input = fixture(kind);
                assert_eq!(
                    dot_i8_exact(&row, &input).unwrap(),
                    scalar_oracle(&row, &input),
                    "row={index} kind={kind}"
                );
            }
        }
    }

    #[test]
    fn two_page_i8_matvec_matches_scalar_oracle_for_signed_extremes() {
        let wires: Vec<Vec<u8>> = (0..257).map(wire).collect();
        let serialized = tensor::encode_tensor(&wires).unwrap();
        let view = tensor::TensorView::new(&serialized).unwrap();
        for kind in [0, 3, 4, 5, 6, 7] {
            let input = fixture(kind);
            let actual = matvec_i8(&view, &input).unwrap();
            let expected: Vec<i128> = wires
                .iter()
                .map(|block| scalar_oracle(block, &input))
                .collect();
            assert_eq!(actual, expected, "fixture={kind}");
            let mut reusable = vec![i128::MIN; wires.len()];
            matvec_i8_into(&view, &input, &mut reusable).unwrap();
            assert_eq!(reusable, expected);
        }
    }

    #[test]
    fn output_length_rejection_precedes_mutation() {
        let rows = vec![wire(1), wire(2)];
        let serialized = tensor::encode_tensor(&rows).unwrap();
        let view = tensor::TensorView::new(&serialized).unwrap();
        let mut undersized = vec![123i128; 1];
        assert_eq!(
            matvec_i8_into(&view, &fixture(5), &mut undersized),
            Err(Error::WrongOutputLength)
        );
        assert_eq!(undersized, [123]);
    }

    #[test]
    fn corrupt_a8_wire_rejected_without_silent_scalar_fallback() {
        let mut bad = wire(10);
        bad[1] |= 3;
        assert_eq!(dot_i8_exact(&bad, &fixture(5)), Err(Error::InvalidWire));
        let mut truncated = wire(3);
        truncated.pop();
        assert_eq!(
            dot_i8_exact(&truncated, &fixture(5)),
            Err(Error::InvalidWire)
        );
    }
}
