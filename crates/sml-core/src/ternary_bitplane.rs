//! Exact integer ternary layer over packed bitplanes (hybrid engine H-0).
//!
//! A neural page expert for the shared SBG bit bus: ternary weights are stored
//! as two disjoint `u64` bitplanes `w+` and `w-`, unsigned activations as
//! `1..=8` bitplanes, and one output is
//! `sum_b 2^b * (popcount(x_b & w+) - popcount(x_b & w-))` in `i32`.
//! No floating point, no vendor runtime, no SIMD intrinsics: outputs are
//! identical on every host. This is a kernel contract with a scalar oracle,
//! not a trained expert, a routing rule or a performance claim.

use core::fmt;

/// Largest accepted input width; keeps `255 * MAX_INPUTS` inside `i32`.
pub const MAX_INPUTS: usize = 1 << 20;
/// Largest accepted activation precision, in bitplanes.
pub const MAX_ACTIVATION_BITS: u8 = 8;

/// Unsigned activations packed as `bits` planes of `ceil(inputs / 64)` words.
///
/// Plane `b` holds bit `b` of every input, least-significant input first.
/// Padding bits past `inputs` are always zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitplaneActivation {
    inputs: usize,
    bits: u8,
    planes: Vec<u64>,
}

impl BitplaneActivation {
    /// Packs `values`, each strictly below `2^bits`.
    pub fn from_values(values: &[u8], bits: u8) -> Result<Self, TernaryBitplaneError> {
        validate_inputs(values.len())?;
        if bits == 0 || bits > MAX_ACTIVATION_BITS {
            return Err(TernaryBitplaneError::InvalidActivationBits(bits));
        }
        let words = words_for(values.len());
        let mut planes = vec![0_u64; usize::from(bits) * words];
        for (index, value) in values.iter().copied().enumerate() {
            if u16::from(value) >> bits != 0 {
                return Err(TernaryBitplaneError::ActivationOutOfRange { index, value, bits });
            }
            for plane in 0..usize::from(bits) {
                if (value >> plane) & 1 == 1 {
                    planes[plane * words + index / 64] |= 1_u64 << (index % 64);
                }
            }
        }
        Ok(Self {
            inputs: values.len(),
            bits,
            planes,
        })
    }

    /// Wraps one already packed Boolean plane, e.g. SBG state words.
    pub fn from_bits(words: &[u64], inputs: usize) -> Result<Self, TernaryBitplaneError> {
        validate_inputs(inputs)?;
        if words.len() != words_for(inputs) {
            return Err(TernaryBitplaneError::LengthMismatch {
                expected: words_for(inputs),
                actual: words.len(),
            });
        }
        let mut planes = words.to_vec();
        if let Some(last) = planes.last_mut() {
            *last &= tail_mask(inputs);
        }
        Ok(Self {
            inputs,
            bits: 1,
            planes,
        })
    }

    #[must_use]
    pub fn inputs(&self) -> usize {
        self.inputs
    }

    #[must_use]
    pub fn bits(&self) -> u8 {
        self.bits
    }

    #[must_use]
    pub fn planes(&self) -> &[u64] {
        &self.planes
    }

    fn plane(&self, index: usize) -> &[u64] {
        let words = words_for(self.inputs);
        &self.planes[index * words..(index + 1) * words]
    }
}

/// A dense ternary layer, output-major, each row as disjoint `w+`/`w-` planes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TernaryLayer {
    inputs: usize,
    outputs: usize,
    plus: Vec<u64>,
    minus: Vec<u64>,
}

impl TernaryLayer {
    /// Builds the layer from row-major weights in `{-1, 0, 1}`.
    pub fn from_weights(
        weights: &[i8],
        inputs: usize,
        outputs: usize,
    ) -> Result<Self, TernaryBitplaneError> {
        validate_inputs(inputs)?;
        if outputs == 0 {
            return Err(TernaryBitplaneError::EmptyOutputs);
        }
        let expected = inputs
            .checked_mul(outputs)
            .ok_or(TernaryBitplaneError::Overflow)?;
        if weights.len() != expected {
            return Err(TernaryBitplaneError::LengthMismatch {
                expected,
                actual: weights.len(),
            });
        }
        let words = words_for(inputs);
        let mut plus = vec![0_u64; outputs * words];
        let mut minus = vec![0_u64; outputs * words];
        for (index, weight) in weights.iter().copied().enumerate() {
            let (row, column) = (index / inputs, index % inputs);
            let slot = row * words + column / 64;
            let bit = 1_u64 << (column % 64);
            match weight {
                1 => plus[slot] |= bit,
                -1 => minus[slot] |= bit,
                0 => {}
                _ => return Err(TernaryBitplaneError::InvalidWeight { index, weight }),
            }
        }
        Ok(Self {
            inputs,
            outputs,
            plus,
            minus,
        })
    }

    #[must_use]
    pub fn inputs(&self) -> usize {
        self.inputs
    }

    #[must_use]
    pub fn outputs(&self) -> usize {
        self.outputs
    }

    /// Compute a one-bit SBG input against every ternary output row without
    /// allocating or copying the caller's packed input.
    ///
    /// The input is one little-endian u64 bitplane of exactly
    /// `ceil(inputs / 64)` words. Padding is ignored: constructed weight
    /// planes always have zero bits beyond the logical input width.
    /// Validation occurs before writing any output.
    ///
    /// This is a portable u64 reference/adapter, not a measured speedup.
    pub fn forward_binary_into(
        &self,
        input_words: &[u64],
        output: &mut [i32],
    ) -> Result<(), TernaryBitplaneError> {
        let words = words_for(self.inputs);
        if input_words.len() != words {
            return Err(TernaryBitplaneError::LengthMismatch {
                expected: words,
                actual: input_words.len(),
            });
        }
        if output.len() != self.outputs {
            return Err(TernaryBitplaneError::LengthMismatch {
                expected: self.outputs,
                actual: output.len(),
            });
        }
        for (row, slot) in output.iter_mut().enumerate() {
            let offset = row * words;
            let mut sum = 0_i32;
            for (index, &input) in input_words.iter().enumerate() {
                sum += (input & self.plus[offset + index]).count_ones() as i32
                    - (input & self.minus[offset + index]).count_ones() as i32;
            }
            *slot = sum;
        }
        Ok(())
    }

    /// Exact integer pre-activations, one per output row.
    pub fn forward(
        &self,
        activation: &BitplaneActivation,
    ) -> Result<Vec<i32>, TernaryBitplaneError> {
        if activation.inputs != self.inputs {
            return Err(TernaryBitplaneError::LengthMismatch {
                expected: self.inputs,
                actual: activation.inputs,
            });
        }
        let words = words_for(self.inputs);
        let mut out = Vec::with_capacity(self.outputs);
        for row in 0..self.outputs {
            let plus = &self.plus[row * words..(row + 1) * words];
            let minus = &self.minus[row * words..(row + 1) * words];
            let mut acc = 0_i32;
            for plane in 0..usize::from(activation.bits) {
                let x = activation.plane(plane);
                let mut signed = 0_i32;
                for ((&xw, &pw), &mw) in x.iter().zip(plus).zip(minus) {
                    // Each popcount is at most 64; the row total stays below
                    // MAX_INPUTS, and the weighted sum below 255 * MAX_INPUTS.
                    signed += (xw & pw).count_ones() as i32 - (xw & mw).count_ones() as i32;
                }
                acc += signed << plane;
            }
            out.push(acc);
        }
        Ok(out)
    }

    /// Writes `pre_activation > threshold` back as one packed Boolean plane.
    pub fn forward_bits(
        &self,
        activation: &BitplaneActivation,
        thresholds: &[i32],
    ) -> Result<Vec<u64>, TernaryBitplaneError> {
        if thresholds.len() != self.outputs {
            return Err(TernaryBitplaneError::LengthMismatch {
                expected: self.outputs,
                actual: thresholds.len(),
            });
        }
        let values = self.forward(activation)?;
        let mut bits = vec![0_u64; words_for(self.outputs)];
        for (index, (&value, &threshold)) in values.iter().zip(thresholds).enumerate() {
            if value > threshold {
                bits[index / 64] |= 1_u64 << (index % 64);
            }
        }
        Ok(bits)
    }
}

/// Independent scalar reference: plain `i8 * u8` row sums.
#[must_use]
pub fn ternary_forward_oracle(weights: &[i8], values: &[u8], outputs: usize) -> Vec<i32> {
    let inputs = values.len();
    (0..outputs)
        .map(|row| {
            weights[row * inputs..(row + 1) * inputs]
                .iter()
                .zip(values)
                .map(|(&w, &x)| i32::from(w) * i32::from(x))
                .sum()
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TernaryBitplaneError {
    InvalidInputs(usize),
    EmptyOutputs,
    InvalidActivationBits(u8),
    ActivationOutOfRange { index: usize, value: u8, bits: u8 },
    InvalidWeight { index: usize, weight: i8 },
    LengthMismatch { expected: usize, actual: usize },
    Overflow,
}

impl fmt::Display for TernaryBitplaneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for TernaryBitplaneError {}

fn validate_inputs(inputs: usize) -> Result<(), TernaryBitplaneError> {
    if inputs == 0 || inputs > MAX_INPUTS {
        return Err(TernaryBitplaneError::InvalidInputs(inputs));
    }
    Ok(())
}

fn words_for(inputs: usize) -> usize {
    inputs.div_ceil(64)
}

fn tail_mask(inputs: usize) -> u64 {
    match inputs % 64 {
        0 => u64::MAX,
        used => (1_u64 << used) - 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }
    }

    fn random_case(seed: u64, inputs: usize, outputs: usize, bits: u8) -> (Vec<i8>, Vec<u8>) {
        let mut lcg = Lcg(seed);
        let weights = (0..inputs * outputs)
            .map(|_| (lcg.next() % 3) as i8 - 1)
            .collect();
        let values = (0..inputs)
            .map(|_| (lcg.next() % (1_u64 << bits)) as u8)
            .collect();
        (weights, values)
    }

    #[test]
    fn bitplane_forward_matches_scalar_oracle() {
        let mut seed = 11;
        for inputs in [1, 2, 63, 64, 65, 127, 128, 1024, 1030] {
            for outputs in [1, 3, 17, 64, 65] {
                for bits in [1, 2, 4, 8] {
                    seed += 1;
                    let (weights, values) = random_case(seed, inputs, outputs, bits);
                    let layer =
                        TernaryLayer::from_weights(&weights, inputs, outputs).expect("layer");
                    let activation = BitplaneActivation::from_values(&values, bits).expect("act");
                    assert_eq!(
                        layer.forward(&activation).expect("forward"),
                        ternary_forward_oracle(&weights, &values, outputs),
                        "inputs={inputs} outputs={outputs} bits={bits}"
                    );
                }
            }
        }
    }

    #[test]
    fn borrowed_binary_plane_matches_independent_oracle_with_dirty_padding() {
        let mut seed = 101;
        for inputs in [1, 2, 63, 64, 65, 124, 127, 128, 130, 1024, 1030] {
            for outputs in [1, 3, 17, 64, 65] {
                seed += 1;
                let (weights, values) = random_case(seed, inputs, outputs, 1);
                let layer = TernaryLayer::from_weights(&weights, inputs, outputs).unwrap();
                let packed = BitplaneActivation::from_values(&values, 1).unwrap();
                let mut raw = packed.planes().to_vec();
                let last = raw.len() - 1;
                raw[last] |= !tail_mask(inputs);
                let mut actual = vec![i32::MIN; outputs];
                layer.forward_binary_into(&raw, &mut actual).unwrap();
                assert_eq!(
                    actual,
                    ternary_forward_oracle(&weights, &values, outputs),
                    "inputs={inputs} outputs={outputs}"
                );
                assert_eq!(
                    actual,
                    layer.forward(&packed).unwrap(),
                    "packed reference inputs={inputs} outputs={outputs}"
                );
            }
        }
    }

    #[test]
    fn borrowed_binary_plane_invalid_shapes_leave_output_unchanged() {
        let layer = TernaryLayer::from_weights(&[1, -1, 0, 1], 2, 2).unwrap();
        let mut out = [77_i32; 2];
        assert_eq!(
            layer.forward_binary_into(&[], &mut out),
            Err(TernaryBitplaneError::LengthMismatch {
                expected: 1,
                actual: 0
            })
        );
        assert_eq!(out, [77, 77]);
        assert_eq!(
            layer.forward_binary_into(&[3], &mut out[..1]),
            Err(TernaryBitplaneError::LengthMismatch {
                expected: 2,
                actual: 1
            })
        );
        assert_eq!(out, [77, 77]);
        layer.forward_binary_into(&[3], &mut out).unwrap();
        assert_eq!(out, [0, 1]);
    }

    #[test]
    fn extreme_values_do_not_overflow() {
        let inputs = 4096;
        let layer =
            TernaryLayer::from_weights(&[vec![1; inputs], vec![-1; inputs]].concat(), inputs, 2)
                .expect("layer");
        let activation = BitplaneActivation::from_values(&vec![255; inputs], 8).expect("act");
        assert_eq!(
            layer.forward(&activation).expect("forward"),
            vec![255 * 4096, -255 * 4096]
        );
    }

    #[test]
    fn planes_are_disjoint_and_padding_is_zero() {
        let (weights, values) = random_case(7, 70, 5, 3);
        let layer = TernaryLayer::from_weights(&weights, 70, 5).expect("layer");
        for (plus, minus) in layer.plus.iter().zip(&layer.minus) {
            assert_eq!(plus & minus, 0);
        }
        for row in 0..5 {
            assert_eq!(layer.plus[row * 2 + 1] >> 6, 0);
            assert_eq!(layer.minus[row * 2 + 1] >> 6, 0);
        }
        let activation = BitplaneActivation::from_values(&values, 3).expect("act");
        for plane in 0..3 {
            assert_eq!(activation.plane(plane)[1] >> 6, 0);
        }
    }

    #[test]
    fn packed_boolean_state_matches_single_plane_values() {
        let (weights, values) = random_case(23, 130, 9, 1);
        let layer = TernaryLayer::from_weights(&weights, 130, 9).expect("layer");
        let packed = BitplaneActivation::from_values(&values, 1).expect("act");
        let mut dirty = packed.planes().to_vec();
        dirty[2] |= !tail_mask(130);
        let wrapped = BitplaneActivation::from_bits(&dirty, 130).expect("wrap");
        assert_eq!(wrapped, packed);
        assert_eq!(
            layer.forward(&wrapped).expect("forward"),
            ternary_forward_oracle(&weights, &values, 9)
        );
    }

    #[test]
    fn forward_bits_thresholds_each_output() {
        let (weights, values) = random_case(31, 200, 70, 4);
        let layer = TernaryLayer::from_weights(&weights, 200, 70).expect("layer");
        let activation = BitplaneActivation::from_values(&values, 4).expect("act");
        let thresholds: Vec<i32> = (0..70).map(|i| i - 35).collect();
        let bits = layer.forward_bits(&activation, &thresholds).expect("bits");
        let oracle = ternary_forward_oracle(&weights, &values, 70);
        for (index, (&value, &threshold)) in oracle.iter().zip(&thresholds).enumerate() {
            assert_eq!(
                (bits[index / 64] >> (index % 64)) & 1 == 1,
                value > threshold
            );
        }
        assert_eq!(bits[1] >> 6, 0);
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        assert_eq!(
            TernaryLayer::from_weights(&[0, 2], 2, 1),
            Err(TernaryBitplaneError::InvalidWeight {
                index: 1,
                weight: 2
            })
        );
        assert_eq!(
            TernaryLayer::from_weights(&[0; 3], 2, 1),
            Err(TernaryBitplaneError::LengthMismatch {
                expected: 2,
                actual: 3
            })
        );
        assert_eq!(
            TernaryLayer::from_weights(&[], 0, 1),
            Err(TernaryBitplaneError::InvalidInputs(0))
        );
        assert_eq!(
            TernaryLayer::from_weights(&[], 2, 0),
            Err(TernaryBitplaneError::EmptyOutputs)
        );
        assert_eq!(
            BitplaneActivation::from_values(&[1, 4], 2),
            Err(TernaryBitplaneError::ActivationOutOfRange {
                index: 1,
                value: 4,
                bits: 2
            })
        );
        assert_eq!(
            BitplaneActivation::from_values(&[1], 9),
            Err(TernaryBitplaneError::InvalidActivationBits(9))
        );
        assert_eq!(
            BitplaneActivation::from_bits(&[0, 0], 64),
            Err(TernaryBitplaneError::LengthMismatch {
                expected: 1,
                actual: 2
            })
        );
        let layer = TernaryLayer::from_weights(&[1, -1], 2, 1).expect("layer");
        let wrong = BitplaneActivation::from_values(&[1, 1, 1], 1).expect("act");
        assert!(matches!(
            layer.forward(&wrong),
            Err(TernaryBitplaneError::LengthMismatch {
                expected: 2,
                actual: 3
            })
        ));
        let right = BitplaneActivation::from_values(&[1, 1], 1).expect("act");
        assert!(matches!(
            layer.forward_bits(&right, &[]),
            Err(TernaryBitplaneError::LengthMismatch {
                expected: 1,
                actual: 0
            })
        ));
    }
}
