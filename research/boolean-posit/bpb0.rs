//! BPB-0: experimental, NOT a standards-compliant posit format.
//! Exact bit-plane dot products for already-quantized 64-lane signed values.
//! No floating-point or general multiplication in the dot-product kernel.
//!
//! On-wire layout (25 bytes, little endian):
//! [scale exponent: i8][sign: u64][magnitude bit 0: u64][magnitude bit 1: u64]

pub const LANES: usize = 64;
pub const WIRE_BYTES: usize = 25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bpb0 {
    pub exponent: i8,
    sign: u64,
    magnitude_0: u64,
    magnitude_1: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DyadicDot {
    /// Signed integer numerator of mantissa * 2^exponent.
    pub mantissa: i32,
    /// Exponent retained instead of rounding, shifting, or overflowing.
    pub exponent: i16,
}

impl Bpb0 {
    /// Construct a block from 64 signed integers in [-3, 3].
    pub fn encode(values: &[i8; LANES], exponent: i8) -> Result<Self, &'static str> {
        let (mut sign, mut magnitude_0, mut magnitude_1) = (0_u64, 0_u64, 0_u64);
        for (i, &v) in values.iter().enumerate() {
            if !(-3..=3).contains(&v) {
                return Err("coefficient must be between -3 and 3");
            }
            let mask = 1_u64 << i;
            if v < 0 {
                sign |= mask;
            }
            let mag = v.unsigned_abs();
            if mag & 1 != 0 {
                magnitude_0 |= mask;
            }
            if mag & 2 != 0 {
                magnitude_1 |= mask;
            }
        }
        Ok(Self { exponent, sign, magnitude_0, magnitude_1 })
    }

    pub fn decode(&self) -> [i8; LANES] {
        std::array::from_fn(|i| {
            let magnitude = (((self.magnitude_0 >> i) & 1)
                | (((self.magnitude_1 >> i) & 1) << 1)) as i8;
            if (self.sign >> i) & 1 == 1 { -magnitude } else { magnitude }
        })
    }

    /// Canonical serialized representation independent of struct padding.
    pub fn to_wire(&self) -> [u8; WIRE_BYTES] {
        let mut bytes = [0_u8; WIRE_BYTES];
        bytes[0] = self.exponent as u8;
        bytes[1..9].copy_from_slice(&self.sign.to_le_bytes());
        bytes[9..17].copy_from_slice(&self.magnitude_0.to_le_bytes());
        bytes[17..25].copy_from_slice(&self.magnitude_1.to_le_bytes());
        bytes
    }

    /// Reject negative zero; each accepted wire value is canonical.
    pub fn from_wire(bytes: &[u8; WIRE_BYTES]) -> Result<Self, &'static str> {
        let sign = u64::from_le_bytes(bytes[1..9].try_into().expect("8 bytes"));
        let magnitude_0 = u64::from_le_bytes(bytes[9..17].try_into().expect("8 bytes"));
        let magnitude_1 = u64::from_le_bytes(bytes[17..25].try_into().expect("8 bytes"));
        if sign & !(magnitude_0 | magnitude_1) != 0 {
            return Err("negative-zero lane is not canonical");
        }
        Ok(Self { exponent: bytes[0] as i8, sign, magnitude_0, magnitude_1 })
    }

    /// Exact dot product of already-quantized coefficients.
    /// Four bit-plane intersections, eight POPCOUNTs, no general multiplication.
    pub fn dot(&self, other: &Self) -> DyadicDot {
        let opposite_signs = self.sign ^ other.sign;
        fn signed_count(plane: u64, opposite_signs: u64) -> i32 {
            let all = plane.count_ones() as i32;
            let negative = (plane & opposite_signs).count_ones() as i32;
            all - (negative << 1)
        }
        let c00 = signed_count(self.magnitude_0 & other.magnitude_0, opposite_signs);
        let c01 = signed_count(self.magnitude_0 & other.magnitude_1, opposite_signs);
        let c10 = signed_count(self.magnitude_1 & other.magnitude_0, opposite_signs);
        let c11 = signed_count(self.magnitude_1 & other.magnitude_1, opposite_signs);
        DyadicDot {
            mantissa: c00 + ((c01 + c10) << 1) + (c11 << 2),
            exponent: i16::from(self.exponent) + i16::from(other.exponent),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhaustive_constant_lanes() {
        for a in -3_i8..=3 {
            for b in -3_i8..=3 {
                let lhs = Bpb0::encode(&[a; LANES], -7).unwrap();
                let rhs = Bpb0::encode(&[b; LANES], 5).unwrap();
                assert_eq!(lhs.dot(&rhs), DyadicDot {
                    mantissa: 64 * i32::from(a) * i32::from(b),
                    exponent: -2,
                });
            }
        }
    }

    #[test]
    fn deterministic_nonuniform_vectors() {
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        for trial in 0..500_i16 {
            let mut lhs = [0_i8; LANES];
            let mut rhs = [0_i8; LANES];
            for i in 0..LANES {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                lhs[i] = (seed % 7) as i8 - 3;
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                rhs[i] = (seed % 7) as i8 - 3;
            }
            let left = Bpb0::encode(&lhs, (trial % 64 - 32) as i8).unwrap();
            let right = Bpb0::encode(&rhs, (31 - trial % 64) as i8).unwrap();
            let scalar: i32 = lhs.iter().zip(&rhs).map(|(&a, &b)| i32::from(a) * i32::from(b)).sum();
            assert_eq!(left.dot(&right).mantissa, scalar);
            assert_eq!(left.dot(&right).exponent, i16::from(left.exponent) + i16::from(right.exponent));
            assert_eq!(left.decode(), lhs);
            assert_eq!(right.decode(), rhs);
            assert_eq!(Bpb0::from_wire(&left.to_wire()).unwrap(), left);
        }
    }

    #[test]
    fn wire_size_and_canonical_zero() {
        let all_zero = Bpb0::encode(&[0; LANES], -128).unwrap();
        assert_eq!(all_zero.to_wire().len(), WIRE_BYTES);
        assert_eq!(all_zero.dot(&all_zero), DyadicDot { mantissa: 0, exponent: -256 });
        let mut invalid = all_zero.to_wire();
        invalid[1] = 1; // Sign bit on a zero magnitude is forbidden.
        assert_eq!(Bpb0::from_wire(&invalid), Err("negative-zero lane is not canonical"));
        assert_eq!(Bpb0::encode(&[4; LANES], 0), Err("coefficient must be between -3 and 3"));
    }

    #[test]
    fn maximum_possible_dot() {
        let max = Bpb0::encode(&[3; LANES], 127).unwrap();
        let min = Bpb0::encode(&[-3; LANES], -128).unwrap();
        assert_eq!(max.dot(&max), DyadicDot { mantissa: 576, exponent: 254 });
        assert_eq!(max.dot(&min), DyadicDot { mantissa: -576, exponent: -1 });
    }
}
