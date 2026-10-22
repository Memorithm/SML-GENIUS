//! Exact Möbius/ANF representation of GB-2 geometric sector predicates.
//!
//! Sector identifiers are Boolean addresses, not equiangular measures.
//! For 8/16/32/64 cells, each `FanMask` is a 3/4/5/6-variable truth
//! table respectively. This module introduces no model inference behavior
//! and does not claim that ANF beats a direct bit lookup.

use crate::geometric_boolean::{GeometryError, Point};
use crate::geometric_fan::{FanError, FanMask, RationalFan};

/// Validation error for ANF evaluation on bounded fan domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanAnfError {
    InvalidSector { sector: u8, width: u8 },
    Geometry(GeometryError),
    Fan(FanError),
}

/// A packed GF(2) polynomial corresponding exactly to a typed fan truth table.
/// The bit at index m indicates that the monomial for subset m is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanAnf {
    width: u8,
    coefficients: u64,
}

/// Apply the Boolean subset zeta/Möbius transform. This transform is its own
/// inverse over GF(2); input width must be 8, 16, 32 or 64 (invariant of FanMask).
fn mobius_transform(mut packed: u64, width: u8) -> u64 {
    let size = usize::from(width);
    let mut variable = 1_usize;
    while variable < size {
        for index in 0..size {
            if index & variable != 0 && (packed >> (index ^ variable)) & 1 != 0 {
                packed ^= 1_u64 << index;
            }
        }
        variable *= 2;
    }
    packed
}

impl FanAnf {
    /// Encode the entire sector truth table as GF(2) monomial coefficients.
    #[must_use]
    pub fn from_mask(mask: FanMask) -> Self {
        Self {
            width: mask.width(),
            coefficients: mobius_transform(mask.bits(), mask.width()),
        }
    }

    /// Number of Boolean assignments (sector addresses).
    #[must_use]
    pub const fn width(self) -> u8 {
        self.width
    }

    /// Bit-packed algebraic normal form coefficients.
    #[must_use]
    pub const fn coefficients(self) -> u64 {
        self.coefficients
    }

    /// Number of active monomials (not measured execution cost).
    #[must_use]
    pub const fn monomial_count(self) -> u32 {
        self.coefficients.count_ones()
    }

    /// Recover the original truth table via a second GF(2) transform.
    pub fn reconstruct_mask(self) -> Result<FanMask, FanError> {
        FanMask::new(self.width, mobius_transform(self.coefficients, self.width))
    }

    /// Evaluate using direct monomial inclusion, independent of the transform.
    ///
    /// Bit j of the sector address is Boolean input x_j. The empty subset is
    /// the constant term. This evaluation is O(M), not a speed claim.
    pub fn evaluate_sector(self, sector: u8) -> Result<bool, FanAnfError> {
        if sector >= self.width {
            return Err(FanAnfError::InvalidSector {
                sector,
                width: self.width,
            });
        }
        let mut parity = false;
        for monomial in 0..usize::from(self.width) {
            if (self.coefficients >> monomial) & 1 != 0
                && (monomial as u8 & sector) == monomial as u8
            {
                parity = !parity;
            }
        }
        Ok(parity)
    }

    /// Couple exact geometric classification and exact GF(2) evaluation.
    ///
    /// A width mismatch fails closed; geometry must belong to this fan.
    pub fn evaluate_point(self, fan: &RationalFan, point: Point) -> Result<bool, FanAnfError> {
        if usize::from(self.width) != fan.sector_count() {
            return Err(FanAnfError::Fan(FanError::WidthMismatch {
                expected: self.width,
                actual: fan.sector_count() as u8,
            }));
        }
        let sector = fan.classify(point).map_err(FanAnfError::Geometry)?;
        self.evaluate_sector(sector as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometric_boolean::{evaluate_anf, SectorMask};
    use crate::geometric_fan::pi_fan_mask;
    use crate::pi_boolean::PiError;

    fn reference_coefficient(truth: u64, monomial: u8) -> bool {
        // Independent subset enumeration; does not call Möbius stages.
        let mut parity = false;
        for assignment in 0..=monomial {
            if assignment & !monomial == 0 && truth & (1_u64 << assignment) != 0 {
                parity = !parity;
            }
        }
        parity
    }

    fn sample_points(fan: &RationalFan) -> Vec<Point> {
        let mut points = Vec::new();
        for x in -8_i64..=8 {
            for y in -8_i64..=8 {
                if x != 0 || y != 0 {
                    points.push(Point { x, y });
                }
            }
        }
        points.extend([
            Point {
                x: i64::MIN,
                y: i64::MAX,
            },
            Point {
                x: i64::MAX,
                y: i64::MIN,
            },
            Point {
                x: i64::MIN,
                y: i64::MIN,
            },
            Point {
                x: i64::MAX,
                y: i64::MAX,
            },
        ]);
        for (index, &start) in fan.rays().iter().enumerate() {
            let end = fan.rays()[(index + 1) % fan.sector_count()];
            points.push(start);
            points.push(Point {
                x: start.x + end.x,
                y: start.y + end.y,
            });
        }
        points
    }

    fn assert_paired_paths(fan: &RationalFan, mask: FanMask, points: &[Point]) {
        let anf = FanAnf::from_mask(mask);
        assert_eq!(anf.reconstruct_mask(), Ok(mask));
        for sector in 0..mask.width() {
            assert_eq!(
                anf.evaluate_sector(sector),
                Ok(mask.bits() & (1_u64 << sector) != 0),
                "width={}, sector={}, mask={:016x}",
                mask.width(),
                sector,
                mask.bits()
            );
            assert_eq!(
                anf.coefficients() & (1_u64 << sector) != 0,
                reference_coefficient(mask.bits(), sector),
                "independent coefficient oracle: width={}, monomial={}",
                mask.width(),
                sector
            );
        }
        for &point in points {
            let direct = mask.contains_point(fan, point).unwrap();
            let sector_linear = fan.classify_scan(point).unwrap();
            let linear = mask.bits() & (1_u64 << sector_linear) != 0;
            assert_eq!(direct, linear, "scan oracle: {point:?}");
            assert_eq!(
                anf.evaluate_point(fan, point),
                Ok(direct),
                "ANF: {point:?}, width={}",
                mask.width()
            );
        }
    }

    #[test]
    fn all_256_gb1_masks_match_original_anf_and_fan_geometry() {
        let fan = RationalFan::new(0).unwrap();
        let points = sample_points(&fan);
        for bits in 0_u8..=u8::MAX {
            let original = SectorMask(bits).anf_coefficients();
            let mask = FanMask::from_gb1(SectorMask(bits));
            let anf = FanAnf::from_mask(mask);
            for (monomial, expected) in original.iter().copied().enumerate() {
                assert_eq!(anf.coefficients() & (1_u64 << monomial) != 0, expected);
                assert_eq!(
                    anf.evaluate_sector(monomial as u8),
                    evaluate_anf(&original, monomial as u8).map_err(FanAnfError::Geometry)
                );
            }
            assert_paired_paths(&fan, mask, &points);
        }
    }

    #[test]
    fn all_singletons_and_structured_masks_are_exact_across_levels() {
        for level in 1..=3 {
            let fan = RationalFan::new(level).unwrap();
            let width = fan.sector_count() as u8;
            let points = sample_points(&fan);
            let upper_mask = if width == 64 {
                u64::MAX
            } else {
                (1_u64 << width) - 1
            };
            for value in [
                0,
                upper_mask,
                0xAAAA_AAAA_AAAA_AAAA & upper_mask,
                0xCCCC_CCCC_CCCC_CCCC & upper_mask,
                0xF0F0_F0F0_F0F0_F0F0 & upper_mask,
                0xFFFF_0000_FFFF_0000 & upper_mask,
                1,
                1_u64 << (width - 1),
            ] {
                assert_paired_paths(&fan, FanMask::new(width, value).unwrap(), &points);
            }
            for singleton in 0..width {
                let mask = FanMask::new(width, 1_u64 << singleton).unwrap();
                assert_paired_paths(&fan, mask, &points);
            }
            for offset in [0, usize::from(width)] {
                let mask = pi_fan_mask(&fan, offset).unwrap();
                assert_paired_paths(&fan, mask, &points);
            }
        }
    }

    #[test]
    fn literal_coefficients_and_monomial_counts_are_exact() {
        let mask = FanMask::new(64, 0).unwrap();
        let zero = FanAnf::from_mask(mask);
        assert_eq!(zero.coefficients(), 0);
        assert_eq!(zero.monomial_count(), 0);

        let full = FanAnf::from_mask(FanMask::new(64, u64::MAX).unwrap());
        assert_eq!(full.coefficients(), 1);
        assert_eq!(full.monomial_count(), 1);

        let x0 = FanAnf::from_mask(FanMask::new(64, 0xAAAA_AAAA_AAAA_AAAA).unwrap());
        assert_eq!(x0.coefficients(), 1_u64 << 1);
        let x1 = FanAnf::from_mask(FanMask::new(64, 0xCCCC_CCCC_CCCC_CCCC).unwrap());
        assert_eq!(x1.coefficients(), 1_u64 << 2);
        let x5 = FanAnf::from_mask(FanMask::new(64, 0xFFFF_FFFF_0000_0000).unwrap());
        assert_eq!(x5.coefficients(), 1_u64 << 32);

        let only_zero = FanAnf::from_mask(FanMask::new(64, 1).unwrap());
        assert_eq!(only_zero.coefficients(), u64::MAX);
        assert_eq!(only_zero.monomial_count(), 64);

        let only_last = FanAnf::from_mask(FanMask::new(64, 1_u64 << 63).unwrap());
        assert_eq!(only_last.coefficients(), 1_u64 << 63);
        assert_eq!(only_last.monomial_count(), 1);
    }

    #[test]
    fn all_three_refinements_preserve_anf_membership() {
        let fans: Vec<_> = (0..=3)
            .map(|level| RationalFan::new(level).unwrap())
            .collect();
        let points = sample_points(&fans[3]);
        for bits in 0_u8..=u8::MAX {
            let mut mask = FanMask::from_gb1(SectorMask(bits));
            for level in 0..3 {
                let child = mask.refine_once().unwrap();
                let coarse = FanAnf::from_mask(mask);
                let fine = FanAnf::from_mask(child);
                for &point in &points {
                    assert_eq!(
                        coarse.evaluate_point(&fans[level], point),
                        fine.evaluate_point(&fans[level + 1], point)
                    );
                }
                mask = child;
            }
        }
    }

    #[test]
    fn incompatible_widths_sectors_and_origin_fail_explicitly() {
        let fan64 = RationalFan::new(3).unwrap();
        let fan8 = RationalFan::new(0).unwrap();
        let anf = FanAnf::from_mask(FanMask::new(64, 0b1010).unwrap());
        assert_eq!(
            anf.evaluate_sector(64),
            Err(FanAnfError::InvalidSector {
                sector: 64,
                width: 64,
            })
        );
        assert_eq!(
            anf.evaluate_point(&fan8, Point { x: 1, y: 1 }),
            Err(FanAnfError::Fan(FanError::WidthMismatch {
                expected: 64,
                actual: 8,
            }))
        );
        assert_eq!(
            anf.evaluate_point(&fan64, Point { x: 0, y: 0 }),
            Err(FanAnfError::Geometry(GeometryError::OriginUndefined))
        );
    }

    #[test]
    fn finite_pi_exhaustion_is_preserved() {
        let fan = RationalFan::new(3).unwrap();
        assert!(matches!(
            pi_fan_mask(&fan, 4096 - 32),
            Err(PiError::Exhausted { .. })
        ));
    }
}
