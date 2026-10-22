//! Exact geometric predicates for the eight angular cells cut out by four lines.
//!
//! Lines: x = 0, y = 0, y = x, y = -x.
//! Rays on cell boundaries belong to the counter-clockwise cell that starts
//! at the ray. The origin has no direction and is explicitly rejected.
//! This experiment uses exact integer comparisons; it is not a floating-point
//! angular classifier, a probabilistic LSH family or a Dirac approximation.

use crate::pi_boolean::{PiBitStream, PiError};

/// Nonzero integral vector representing a direction in the plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryError {
    /// (0,0) has no well-defined direction.
    OriginUndefined,
    /// A direction code must be one of 0..=7.
    SectorOutOfRange(u8),
}

/// Eight exact 45-degree sectors, counter-clockwise from (1,0).
///
/// A boundary ray belongs to the sector whose lower angular boundary it is.
pub fn sector_of(point: Point) -> Result<u8, GeometryError> {
    let x = i128::from(point.x);
    let y = i128::from(point.y);
    if x == 0 && y == 0 {
        return Err(GeometryError::OriginUndefined);
    }
    Ok(if x > 0 && y >= 0 {
        if y < x { 0 } else { 1 }
    } else if x <= 0 && y > 0 {
        if -x < y { 2 } else { 3 }
    } else if x < 0 && y <= 0 {
        if -y < -x { 4 } else { 5 }
    } else if x < -y {
        6
    } else {
        7
    })
}

/// Four half-plane literals, in the order x>=0, y>=0, y>=x, y>=-x.
///
/// Sign comparisons use i128 intermediates, preserving the full i64 range.
pub fn halfplane_literals(point: Point) -> Result<[bool; 4], GeometryError> {
    let x = i128::from(point.x);
    let y = i128::from(point.y);
    if x == 0 && y == 0 {
        return Err(GeometryError::OriginUndefined);
    }
    Ok([x >= 0, y >= 0, y >= x, y >= -x])
}

/// A finite Boolean algebra over all unions of the eight disjoint sectors.
/// Bit `i` denotes membership in sector `i`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SectorMask(pub u8);

impl SectorMask {
    pub const EMPTY: Self = Self(0);
    pub const FULL: Self = Self(0xFF);

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    #[must_use]
    pub const fn complement(self) -> Self {
        Self(!self.0)
    }

    #[must_use]
    pub const fn symmetric_difference(self, other: Self) -> Self {
        Self(self.0 ^ other.0)
    }

    /// Angular coverage in eighths of a full turn, under uniform orientation.
    #[must_use]
    pub const fn angular_eighths(self) -> u32 {
        self.0.count_ones()
    }

    /// Membership of an already classified sector.
    pub fn contains_sector(self, sector: u8) -> Result<bool, GeometryError> {
        if sector >= 8 {
            return Err(GeometryError::SectorOutOfRange(sector));
        }
        Ok(self.0 & (1_u8 << sector) != 0)
    }

    /// Predicate evaluation on one geometric direction.
    pub fn contains_point(self, point: Point) -> Result<bool, GeometryError> {
        self.contains_sector(sector_of(point)?)
    }

    /// Exact 3-input truth table over the bits of the sector identifier.
    #[must_use]
    pub fn truth_table(self) -> [bool; 8] {
        std::array::from_fn(|index| self.0 & (1_u8 << index) != 0)
    }

    /// Truth-table to ANF coefficients, using the GF(2) Möbius transform.
    /// The coefficient at index `m` multiplies the variables whose bits
    /// are set in `m`, so the empty product at index 0 is the constant.
    #[must_use]
    pub fn anf_coefficients(self) -> [bool; 8] {
        let mut values = self.truth_table();
        for bit in 0..3 {
            for index in 0..8 {
                if index & (1 << bit) != 0 {
                    values[index] ^= values[index ^ (1 << bit)];
                }
            }
        }
        values
    }
}

/// Independently evaluate a 3-input ANF with explicit monomial inclusion.
pub fn evaluate_anf(coefficients: &[bool; 8], sector: u8) -> Result<bool, GeometryError> {
    if sector >= 8 {
        return Err(GeometryError::SectorOutOfRange(sector));
    }
    let mut value = false;
    for (monomial, &enabled) in coefficients.iter().enumerate() {
        if enabled && (monomial as u8 & !sector) == 0 {
            value = !value;
        }
    }
    Ok(value)
}

/// One π bit per angular sector, consumed in sector order (0..7).
/// This is a reproducible selection experiment, not a stochastic sampler.
pub fn pi_sector_mask(bit_offset: usize) -> Result<SectorMask, PiError> {
    let mut stream = PiBitStream::new(bit_offset)?;
    let mut bits = 0_u8;
    for sector in 0..8 {
        if stream.read_bit()? {
            bits |= 1_u8 << sector;
        }
    }
    Ok(SectorMask(bits))
}

/// Prepare an opt-in Boolean state for an existing Kauffman network.
/// The number of points must separately match that network's node count.
pub fn initial_boolean_state(
    directions: &[Point],
    selection: SectorMask,
) -> Result<Vec<bool>, GeometryError> {
    directions
        .iter()
        .copied()
        .map(|point| selection.contains_point(point))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pi_boolean::KauffmanPiNetwork;

    const RAYS: [Point; 8] = [
        Point { x: 1, y: 0 },
        Point { x: 1, y: 1 },
        Point { x: 0, y: 1 },
        Point { x: -1, y: 1 },
        Point { x: -1, y: 0 },
        Point { x: -1, y: -1 },
        Point { x: 0, y: -1 },
        Point { x: 1, y: -1 },
    ];

    const INSIDE: [Point; 8] = [
        Point { x: 3, y: 1 },
        Point { x: 1, y: 3 },
        Point { x: -1, y: 3 },
        Point { x: -3, y: 1 },
        Point { x: -3, y: -1 },
        Point { x: -1, y: -3 },
        Point { x: 1, y: -3 },
        Point { x: 3, y: -1 },
    ];

    #[test]
    fn rays_belong_to_counterclockwise_half_open_cells() {
        for (index, point) in RAYS.iter().copied().enumerate() {
            assert_eq!(sector_of(point), Ok(index as u8));
        }
        for (index, point) in INSIDE.iter().copied().enumerate() {
            assert_eq!(sector_of(point), Ok(index as u8));
        }
        assert_eq!(
            sector_of(Point { x: 0, y: 0 }),
            Err(GeometryError::OriginUndefined)
        );
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let cases = [
            (Point { x: i64::MIN, y: i64::MAX }, 3),
            (Point { x: i64::MIN, y: i64::MIN }, 5),
            (Point { x: i64::MAX, y: i64::MIN }, 7),
            (Point { x: i64::MAX, y: i64::MAX }, 1),
        ];
        for (point, expected) in cases {
            assert_eq!(sector_of(point), Ok(expected));
            assert!(halfplane_literals(point).is_ok());
        }
    }

    #[test]
    fn full_and_empty_algebra_and_angular_measure() {
        for mask in 0_u8..=u8::MAX {
            let a = SectorMask(mask);
            let b = SectorMask(mask.rotate_left(3));
            assert_eq!(a.union(a.complement()), SectorMask::FULL);
            assert_eq!(a.intersection(a.complement()), SectorMask::EMPTY);
            assert_eq!(a.symmetric_difference(a), SectorMask::EMPTY);
            assert_eq!(a.union(b), b.union(a));
            assert_eq!(a.intersection(b), b.intersection(a));
            assert_eq!(a.union(SectorMask::EMPTY), a);
            assert_eq!(a.intersection(SectorMask::FULL), a);
            assert_eq!(
                a.angular_eighths() + a.complement().angular_eighths(),
                8
            );
        }
        assert_eq!(SectorMask(0b0010_0100).angular_eighths(), 2);
        assert_eq!(
            SectorMask::FULL.contains_sector(8),
            Err(GeometryError::SectorOutOfRange(8))
        );
    }

    #[test]
    fn exact_halfplane_and_sector_predicates() {
        let inside = Point { x: 3, y: 1 };
        let outside = Point { x: 1, y: 3 };
        let literals = halfplane_literals(inside).unwrap();
        assert_eq!(literals, [true, true, false, true]);
        assert!(literals[0] && literals[1] && !literals[2]);
        assert!(SectorMask(1).contains_point(inside).unwrap());
        assert!(!SectorMask(1).contains_point(outside).unwrap());
        assert_eq!(
            SectorMask::FULL.contains_point(Point { x: 0, y: 0 }),
            Err(GeometryError::OriginUndefined)
        );
    }

    #[test]
    fn all_256_truth_tables_match_exact_anf() {
        for mask in 0_u8..=u8::MAX {
            let predicate = SectorMask(mask);
            let coefficients = predicate.anf_coefficients();
            let values = predicate.truth_table();
            for sector in 0_u8..8 {
                assert_eq!(
                    evaluate_anf(&coefficients, sector),
                    Ok(values[sector as usize]),
                    "mask={mask}, sector={sector}"
                );
                assert_eq!(
                    predicate.contains_sector(sector),
                    Ok(values[sector as usize])
                );
            }
        }
        assert_eq!(
            evaluate_anf(&[false; 8], 9),
            Err(GeometryError::SectorOutOfRange(9))
        );
    }

    #[test]
    fn pi_selects_sectors_two_and_five_and_rejects_exhaustion() {
        // Binary expansion starts with the hexadecimal digits 24 = 00100100.
        assert_eq!(pi_sector_mask(0), Ok(SectorMask(0b0010_0100)));
        assert!(matches!(
            pi_sector_mask(4_090),
            Err(PiError::Exhausted { .. })
        ));
    }

    #[test]
    fn the_geometric_predicate_really_seeds_kauffman_state() {
        let state = initial_boolean_state(&RAYS, SectorMask(0b0010_0100)).unwrap();
        assert_eq!(
            state,
            vec![false, false, true, false, false, true, false, false]
        );
        let network = KauffmanPiNetwork::from_pi(8, 1, 0).unwrap();
        assert_eq!(network.step(&state), network.step(&state));
        assert_ne!(
            state,
            initial_boolean_state(&RAYS, SectorMask(1)).unwrap()
        );
    }
}
