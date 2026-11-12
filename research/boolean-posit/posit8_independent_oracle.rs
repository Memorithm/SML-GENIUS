//! Independently structured posit8 arithmetic reference with exact integers.
//! The decoder iterates the regime/exponent/fraction stream explicitly;
//! rounding uses a binary search instead of the production oracle linear scan.
//! Same declared format contract, no external library or generated lookup table.
#![forbid(unsafe_code)]

#[path = "posit8_circuit_synth.rs"]
#[allow(dead_code)]
mod primary;

const UNIT_SHIFT: u32 = 24;
const EXP_BITS: usize = 2;
const MAX_POSITIVE: u8 = 127;
const SPECIAL_NAR: u8 = 0x80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndependentPosit8(pub u8);

impl IndependentPosit8 {
    /// Decode an 8-bit word as an exact integer measured in units of 2^-24.
    /// The bitstream advances from most significant encoded payload bit
    /// through the regime, up to two exponent bits, then fraction bits.
    pub fn decode_units(self) -> Option<i128> {
        if self.0 == SPECIAL_NAR {
            return None;
        }
        if self.0 == 0 {
            return Some(0);
        }
        let negative = self.0 & 0x80 != 0;
        let magnitude = if negative {
            self.0.wrapping_neg()
        } else {
            self.0
        };
        let mut position = 0usize;
        let encoded_bit = |offset: usize| -> u8 {
            (magnitude >> (6 - offset)) & 1
        };
        let first = encoded_bit(position);
        while position < 7 && encoded_bit(position) == first {
            position += 1;
        }
        let run_length = position as i32;
        let regime: i32 = if first == 1 {
            run_length - 1
        } else {
            -run_length
        };
        if position < 7 {
            // Consume the regime termination bit if one remains.
            position += 1;
        }
        let mut exponent = 0i32;
        for _ in 0..EXP_BITS {
            exponent <<= 1;
            if position < 7 {
                exponent |= i32::from(encoded_bit(position));
                position += 1;
            }
        }
        let available = 7 - position;
        let mut significand = 1i128 << available;
        for frac_bit in 0..available {
            let decoded_bit = i128::from(encoded_bit(position + frac_bit));
            significand |= decoded_bit << (available - frac_bit - 1);
        }
        let shift = UNIT_SHIFT as i32 + 4 * regime + exponent - available as i32;
        assert!((0..=48).contains(&shift), "invalid posit8 scale");
        let number = significand << shift;
        Some(if negative { -number } else { number })
    }

    fn nearest_from_scaled(numerator: i128, denominator_shift: u32) -> Self {
        if numerator == 0 {
            return Self(0);
        }
        let negative = numerator < 0;
        let target = numerator.abs();
        let positive_value = |encoded: u8| -> i128 {
            Self(encoded).decode_units().expect("finite positive") << denominator_shift
        };
        let smallest = positive_value(1);
        let largest = positive_value(MAX_POSITIVE);
        let chosen = if target <= smallest {
            1
        } else if target >= largest {
            MAX_POSITIVE
        } else {
            let mut lower = 1u8;
            let mut upper = MAX_POSITIVE;
            while upper - lower > 1 {
                let middle = lower + (upper - lower) / 2;
                if positive_value(middle) <= target {
                    lower = middle;
                } else {
                    upper = middle;
                }
            }
            let low_error = target - positive_value(lower);
            let high_error = positive_value(upper) - target;
            match low_error.cmp(&high_error) {
                std::cmp::Ordering::Less => lower,
                std::cmp::Ordering::Greater => upper,
                std::cmp::Ordering::Equal => {
                    if lower & 1 == 0 {
                        lower
                    } else {
                        upper
                    }
                }
            }
        };
        Self(if negative {
            chosen.wrapping_neg()
        } else {
            chosen
        })
    }
    pub fn add(self, other: Self) -> Self {
        match (self.decode_units(), other.decode_units()) {
            (Some(a), Some(b)) => Self::nearest_from_scaled(a + b, 0),
            _ => Self(SPECIAL_NAR),
        }
    }
    pub fn mul(self, other: Self) -> Self {
        match (self.decode_units(), other.decode_units()) {
            (Some(a), Some(b)) => Self::nearest_from_scaled(a * b, UNIT_SHIFT),
            _ => Self(SPECIAL_NAR),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use primary::Posit8;

    #[test]
    fn independent_decoder_agrees_on_all_256_encodings() {
        let mut last_positive = 0i128;
        for raw in 0..=255u8 {
            let independent = IndependentPosit8(raw).decode_units();
            let baseline = Posit8(raw).decode_units().map(i128::from);
            assert_eq!(independent, baseline, "code=0x{raw:02x}");
            if (1..=127).contains(&raw) {
                let value = independent.unwrap();
                assert!(value > last_positive, "positive order {raw}");
                last_positive = value;
            }
        }
    }

    #[test]
    fn independent_addition_matches_all_65536_pairs() {
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                let actual = IndependentPosit8(a).add(IndependentPosit8(b));
                let expected = Posit8(a).add(Posit8(b));
                assert_eq!(actual.0, expected.0, "ADD a={a} b={b}");
            }
        }
    }

    #[test]
    fn independent_multiplication_matches_all_65536_pairs() {
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                let actual = IndependentPosit8(a).mul(IndependentPosit8(b));
                let expected = Posit8(a).mul(Posit8(b));
                assert_eq!(actual.0, expected.0, "MUL a={a} b={b}");
            }
        }
    }

    #[test]
    fn zero_nar_and_extreme_encoding_invariants() {
        let positive_one = IndependentPosit8(0x40);
        let zero = IndependentPosit8(0);
        let nar = IndependentPosit8(SPECIAL_NAR);
        assert_eq!(positive_one.decode_units(), Some(1i128 << UNIT_SHIFT));
        assert_eq!(IndependentPosit8(1).decode_units(), Some(1));
        assert_eq!(
            IndependentPosit8(MAX_POSITIVE).decode_units(),
            Some(1i128 << 48)
        );
        assert_eq!(positive_one.add(zero), positive_one);
        assert_eq!(positive_one.mul(positive_one), positive_one);
        assert_eq!(nar.add(zero), nar);
        assert_eq!(nar.mul(positive_one), nar);
    }
}
