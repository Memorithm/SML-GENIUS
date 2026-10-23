//! GB-5: exact composable ANF algebra and portable u64 bit-sliced evaluation.
//!
//! A circuit is a GF(2) polynomial in the Boolean quotient ring
//! GF(2)[x0,..,xn-1]/(x_i*x_i - x_i), for 3 <= n <= 6.
//! Structural exactness is **not** a speed, GPU, SIMD or model-quality claim.

use crate::geometric_fan::FanMask;
use crate::geometric_fan_anf::FanAnf;

/// Invalid input/shape for bounded Boolean circuit and lane operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gb5Error {
    WidthMismatch { left: u8, right: u8 },
    InvalidVariable { variable: u8, variables: u32 },
    InvalidPermutation,
    InvalidSector { sector: u8, width: u8 },
    TooManyLanes { lanes: usize },
}

/// A typed, packed algebraic normal form with up to six variables.
/// The coefficient bit m encodes the monomial whose variable support is m.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnfCircuit64 {
    width: u8,
    coefficients: u64,
}

impl AnfCircuit64 {
    /// Convert a GB-2 truth table using the independently qualified GB-3
    /// Möbius transform. Both representation widths are part of the type.
    #[must_use]
    pub fn from_mask(mask: FanMask) -> Self {
        let anf = FanAnf::from_mask(mask);
        Self {
            width: anf.width(),
            coefficients: anf.coefficients(),
        }
    }

    #[must_use]
    pub const fn width(self) -> u8 {
        self.width
    }

    #[must_use]
    pub const fn coefficients(self) -> u64 {
        self.coefficients
    }

    #[must_use]
    pub const fn monomial_count(self) -> u32 {
        self.coefficients.count_ones()
    }

    #[must_use]
    pub fn variable_count(self) -> u32 {
        self.width.ilog2()
    }

    fn same_width(self, rhs: Self) -> Result<(), Gb5Error> {
        if self.width != rhs.width {
            return Err(Gb5Error::WidthMismatch {
                left: self.width,
                right: rhs.width,
            });
        }
        Ok(())
    }

    /// GF(2) polynomial addition is bitwise coefficient XOR.
    pub fn xor(self, rhs: Self) -> Result<Self, Gb5Error> {
        self.same_width(rhs)?;
        Ok(Self {
            width: self.width,
            coefficients: self.coefficients ^ rhs.coefficients,
        })
    }

    /// Boolean complement toggles exactly the constant monomial.
    #[must_use]
    pub fn complement(self) -> Self {
        Self {
            width: self.width,
            coefficients: self.coefficients ^ 1,
        }
    }

    /// Polynomial multiplication in the Boolean quotient ring: monomial
    /// supports multiply by union (bitwise OR); coefficients add in GF(2).
    ///
    /// Bounded O(popcount(lhs) * popcount(rhs)), not a runtime-speed claim.
    pub fn and(self, rhs: Self) -> Result<Self, Gb5Error> {
        self.same_width(rhs)?;
        let mut coefficients = 0_u64;
        let mut lhs = self.coefficients;
        while lhs != 0 {
            let a = lhs.trailing_zeros();
            lhs &= lhs - 1;
            let mut rhs_bits = rhs.coefficients;
            while rhs_bits != 0 {
                let b = rhs_bits.trailing_zeros();
                rhs_bits &= rhs_bits - 1;
                coefficients ^= 1_u64 << (a | b);
            }
        }
        Ok(Self {
            width: self.width,
            coefficients,
        })
    }

    /// Inclusive disjunction: f OR g = f XOR g XOR (f AND g).
    pub fn or(self, rhs: Self) -> Result<Self, Gb5Error> {
        let product = self.and(rhs)?;
        Ok(Self {
            width: self.width,
            coefficients: self.coefficients ^ rhs.coefficients ^ product.coefficients,
        })
    }

    /// Substitute variable x_v := value, retaining the original address width.
    ///
    /// This outputs a function independent of x_v, not a lowered-width fan.
    pub fn restrict_variable(self, variable: u8, value: bool) -> Result<Self, Gb5Error> {
        if u32::from(variable) >= self.variable_count() {
            return Err(Gb5Error::InvalidVariable {
                variable,
                variables: self.variable_count(),
            });
        }
        let variable_bit = 1_u32 << variable;
        let mut output = 0_u64;
        let mut terms = self.coefficients;
        while terms != 0 {
            let term = terms.trailing_zeros();
            terms &= terms - 1;
            if term & variable_bit == 0 {
                output ^= 1_u64 << term;
            } else if value {
                output ^= 1_u64 << (term ^ variable_bit);
            }
        }
        Ok(Self {
            width: self.width,
            coefficients: output,
        })
    }

    /// Substitute each old variable x_i by x_{mapping[i]}, bijectively.
    /// The mapping must contain exactly n distinct indices in [0,n).
    pub fn permute_variables(self, mapping: &[u8]) -> Result<Self, Gb5Error> {
        if mapping.len() != self.variable_count() as usize {
            return Err(Gb5Error::InvalidPermutation);
        }
        let mut seen = 0_u8;
        for &dest in mapping {
            if u32::from(dest) >= self.variable_count() || seen & (1_u8 << dest) != 0 {
                return Err(Gb5Error::InvalidPermutation);
            }
            seen |= 1_u8 << dest;
        }
        let mut output = 0_u64;
        let mut terms = self.coefficients;
        while terms != 0 {
            let term = terms.trailing_zeros();
            terms &= terms - 1;
            let mut renamed = 0_u32;
            for (source, &dest) in mapping.iter().enumerate() {
                if term & (1_u32 << source) != 0 {
                    renamed |= 1_u32 << dest;
                }
            }
            output ^= 1_u64 << renamed;
        }
        Ok(Self {
            width: self.width,
            coefficients: output,
        })
    }

    /// Independent monomial-inclusion evaluation at one sector address.
    pub fn evaluate_sector(self, sector: u8) -> Result<bool, Gb5Error> {
        if sector >= self.width {
            return Err(Gb5Error::InvalidSector {
                sector,
                width: self.width,
            });
        }
        let mut result = false;
        let mut terms = self.coefficients;
        while terms != 0 {
            let monomial = terms.trailing_zeros() as u8;
            terms &= terms - 1;
            result ^= monomial & sector == monomial;
        }
        Ok(result)
    }

    /// Reconstruct a truth table via independently evaluated assignments.
    /// This is a correctness witness, not a high-performance materializer.
    #[must_use]
    pub fn truth_mask(self) -> FanMask {
        let mut bits = 0_u64;
        for sector in 0..self.width {
            if self.evaluate_sector(sector).expect("sector < width") {
                bits |= 1_u64 << sector;
            }
        }
        FanMask::new(self.width, bits).expect("bounded valid ANF domain")
    }

    /// Evaluate up to 64 independent addresses simultaneously through
    /// u64 bitplanes. `active_lanes` masks inactive lanes in the result.
    ///
    /// No vector intrinsics or vendor SDK required; no SIMD-speed claim.
    #[must_use]
    pub fn evaluate_bitplanes(self, planes: &[u64; 6], active_lanes: u64) -> u64 {
        let mut output = 0_u64;
        let mut terms = self.coefficients;
        while terms != 0 {
            let monomial = terms.trailing_zeros();
            terms &= terms - 1;
            let mut product = active_lanes;
            for variable in 0..self.variable_count() {
                if monomial & (1_u32 << variable) != 0 {
                    product &= planes[variable as usize];
                }
            }
            output ^= product;
        }
        output & active_lanes
    }
}

/// Transpose 0..64 sector identifiers to six u64 bitplanes.
/// The returned lane mask is 0 for an empty batch and u64::MAX for 64 lanes.
pub fn pack_sectors(
    sectors: &[u8],
    width: u8,
) -> Result<([u64; 6], u64), Gb5Error> {
    if sectors.len() > 64 {
        return Err(Gb5Error::TooManyLanes {
            lanes: sectors.len(),
        });
    }
    // This public utility is tied to the exact GB-2 domain.
    let variables = match width {
        8 => 3,
        16 => 4,
        32 => 5,
        64 => 6,
        _ => return Err(Gb5Error::InvalidSector { sector: 0, width }),
    };
    let mut planes = [0_u64; 6];
    for (lane, &sector) in sectors.iter().enumerate() {
        if sector >= width {
            return Err(Gb5Error::InvalidSector { sector, width });
        }
        for (variable, plane) in planes.iter_mut().enumerate().take(variables) {
            if sector & (1_u8 << variable) != 0 {
                *plane |= 1_u64 << lane;
            }
        }
    }
    let active = match sectors.len() {
        64 => u64::MAX,
        count => (1_u64 << count) - 1,
    };
    Ok((planes, active))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometric_boolean::Point;
    use crate::geometric_fan::{pi_fan_mask, RationalFan};

    fn frozen_masks(width: u8) -> Vec<FanMask> {
        let fan = RationalFan::new(width.ilog2() as u8 - 3).unwrap();
        let domain = if width == 64 {
            u64::MAX
        } else {
            (1_u64 << width) - 1
        };
        let mut masks = vec![
            FanMask::new(width, 0).unwrap(),
            FanMask::new(width, domain).unwrap(),
            FanMask::new(width, 0xaaaa_aaaa_aaaa_aaaa & domain).unwrap(),
            FanMask::new(width, 0xcccc_cccc_cccc_cccc & domain).unwrap(),
            pi_fan_mask(&fan, 0).unwrap(),
            pi_fan_mask(&fan, 128).unwrap(),
        ];
        for sector in 0..width {
            masks.push(FanMask::new(width, 1_u64 << sector).unwrap());
        }
        masks
    }

    fn assert_agrees_with_lut(circuit: AnfCircuit64, expected: FanMask) {
        assert_eq!(circuit.width(), expected.width());
        assert_eq!(circuit.truth_mask(), expected);
        for sector in 0..expected.width() {
            assert_eq!(
                circuit.evaluate_sector(sector),
                Ok(expected.bits() & (1_u64 << sector) != 0)
            );
        }
    }

    #[test]
    fn all_gb1_pairs_compose_exactly_against_truth_tables() {
        for a in 0_u8..=u8::MAX {
            let am = FanMask::new(8, u64::from(a)).unwrap();
            let ac = AnfCircuit64::from_mask(am);
            assert_eq!(ac.truth_mask(), am);
            assert_eq!(ac.complement().truth_mask(), am.complement());
            for b in 0_u8..=u8::MAX {
                let bm = FanMask::new(8, u64::from(b)).unwrap();
                let bc = AnfCircuit64::from_mask(bm);
                assert_eq!(ac.xor(bc).unwrap().truth_mask(), am.xor(bm).unwrap());
                assert_eq!(ac.and(bc).unwrap().truth_mask(), am.intersection(bm).unwrap());
                assert_eq!(ac.or(bc).unwrap().truth_mask(), am.union(bm).unwrap());
            }
        }
    }

    #[test]
    fn composition_and_de_morgan_hold_across_all_domain_widths() {
        for width in [16, 32, 64] {
            let masks = frozen_masks(width);
            for &am in &masks {
                let a = AnfCircuit64::from_mask(am);
                assert_eq!(a.complement().complement(), a);
                assert_eq!(a.and(a).unwrap(), a);
                for &bm in &masks {
                    let b = AnfCircuit64::from_mask(bm);
                    assert_eq!(a.xor(b).unwrap().truth_mask(), am.xor(bm).unwrap());
                    assert_eq!(
                        a.and(b).unwrap().truth_mask(),
                        am.intersection(bm).unwrap()
                    );
                    assert_eq!(a.or(b).unwrap().truth_mask(), am.union(bm).unwrap());
                    let left = a.and(b).unwrap().complement();
                    let right = a.complement().or(b.complement()).unwrap();
                    assert_eq!(left, right);
                }
            }
        }
    }

    #[test]
    fn restrictions_are_exact_for_every_gb1_function_and_variable() {
        for bits in 0_u8..=u8::MAX {
            let mask = FanMask::new(8, u64::from(bits)).unwrap();
            let circuit = AnfCircuit64::from_mask(mask);
            for variable in 0..3_u8 {
                for value in [false, true] {
                    let restricted = circuit.restrict_variable(variable, value).unwrap();
                    for sector in 0..8_u8 {
                        let forced = (sector & !(1_u8 << variable))
                            | (u8::from(value) << variable);
                        assert_eq!(
                            restricted.evaluate_sector(sector),
                            circuit.evaluate_sector(forced)
                        );
                    }
                    assert_eq!(
                        restricted.coefficients() & coeffs_with_var(variable, 8),
                        0
                    );
                }
            }
        }
    }

    fn coeffs_with_var(variable: u8, width: u8) -> u64 {
        let mut terms = 0_u64;
        for i in 0..width {
            if i & (1_u8 << variable) != 0 {
                terms |= 1_u64 << i;
            }
        }
        terms
    }

    #[test]
    fn restrictions_and_permutations_preserve_64_sector_truth() {
        for width in [16, 32, 64] {
            let variables = width.ilog2() as usize;
            let forward: Vec<u8> = (0..variables as u8).collect();
            let reverse: Vec<u8> = forward.iter().copied().rev().collect();
            for mask in frozen_masks(width) {
                let f = AnfCircuit64::from_mask(mask);
                assert_eq!(f.permute_variables(&forward), Ok(f));
                let permuted = f.permute_variables(&reverse).unwrap();
                assert_eq!(permuted.permute_variables(&reverse), Ok(f));
                for x in 0..width {
                    let mut old_address = 0_u8;
                    for (old_var, &new_var) in reverse.iter().enumerate() {
                        if x & (1_u8 << new_var) != 0 {
                            old_address |= 1_u8 << old_var;
                        }
                    }
                    assert_eq!(
                        permuted.evaluate_sector(x),
                        f.evaluate_sector(old_address)
                    );
                    for variable in 0..variables as u8 {
                        for value in [false, true] {
                            let selected = (x & !(1_u8 << variable))
                                | (u8::from(value) << variable);
                            assert_eq!(
                                f.restrict_variable(variable, value)
                                    .unwrap()
                                    .evaluate_sector(x),
                                f.evaluate_sector(selected)
                            );
                        }
                    }
                }
            }
            let f = AnfCircuit64::from_mask(FanMask::new(width, 1).unwrap());
            assert_eq!(f.permute_variables(&[]), Err(Gb5Error::InvalidPermutation));
            let wrong = vec![0; variables];
            assert_eq!(f.permute_variables(&wrong), Err(Gb5Error::InvalidPermutation));
            assert_eq!(
                f.restrict_variable(variables as u8, true),
                Err(Gb5Error::InvalidVariable {
                    variable: variables as u8,
                    variables: variables as u32,
                })
            );
        }
    }

    fn assert_lane_parity(mask: FanMask, sectors: &[u8]) {
        let circuit = AnfCircuit64::from_mask(mask);
        let (planes, active) = pack_sectors(sectors, mask.width()).unwrap();
        let output = circuit.evaluate_bitplanes(&planes, active);
        assert_eq!(output & !active, 0);
        for (lane, &sector) in sectors.iter().enumerate() {
            let expected = mask.bits() & (1_u64 << sector) != 0;
            assert_eq!((output >> lane) & 1 != 0, expected);
            assert_eq!(circuit.evaluate_sector(sector), Ok(expected));
        }
    }

    #[test]
    fn bitsliced_64_lane_equivalence_with_scalar_and_lut() {
        for width in [8_u8, 16, 32, 64] {
            let cases: Vec<u8> = (0..64).map(|i| (i % usize::from(width)) as u8).collect();
            let families = if width == 8 {
                (0_u8..=u8::MAX)
                    .map(|bits| FanMask::new(8, u64::from(bits)).unwrap())
                    .collect()
            } else {
                frozen_masks(width)
            };
            for mask in families {
                for &len in &[0, 1, 7, 32, 63, 64] {
                    assert_lane_parity(mask, &cases[..len]);
                }
                assert_lane_parity(mask, &cases);
            }
        }
    }

    #[test]
    fn lane_and_width_errors_are_explicit() {
        assert_eq!(
            pack_sectors(&vec![0; 65], 64),
            Err(Gb5Error::TooManyLanes { lanes: 65 })
        );
        assert_eq!(
            pack_sectors(&[7, 8], 8),
            Err(Gb5Error::InvalidSector {
                sector: 8,
                width: 8,
            })
        );
        assert!(pack_sectors(&[], 9).is_err());
        assert_eq!(
            AnfCircuit64::from_mask(FanMask::new(8, 1).unwrap())
                .xor(AnfCircuit64::from_mask(FanMask::new(64, 1).unwrap())),
            Err(Gb5Error::WidthMismatch { left: 8, right: 64 })
        );
    }

    #[test]
    fn real_sector_geometry_drives_both_lut_and_bitsliced_circuit() {
        let fan = RationalFan::new(3).unwrap();
        let mask = pi_fan_mask(&fan, 0).unwrap();
        let points = [
            Point { x: 1, y: 0 },
            Point { x: 1, y: 2 },
            Point { x: 0, y: 1 },
            Point { x: -9, y: 2 },
            Point { x: i64::MIN, y: i64::MAX },
            Point { x: -4, y: -5 },
            Point { x: i64::MAX, y: i64::MIN },
        ];
        let sectors: Vec<u8> = points.iter().map(|&p| fan.classify(p).unwrap() as u8).collect();
        assert_lane_parity(mask, &sectors);
        for (&p, &s) in points.iter().zip(sectors.iter()) {
            assert_eq!(fan.classify_scan(p), Ok(usize::from(s)));
        }
    }
}
