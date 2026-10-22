//! Exact nested rational angular partitions.
//!
//! Each level splits every wedge in two by inserting the sum of its two
//! oriented boundary rays. The refinement preserves sector predicates exactly;
//! beyond level zero the new sectors are NOT equally spaced in angle.
//! No floating-point trigonometry and no claim of probabilistic LSH behavior.

use std::cmp::Ordering;

use crate::geometric_boolean::{sector_of, GeometryError, Point, SectorMask};
use crate::pi_boolean::{PiBitStream, PiError};

/// Errors for bounded experimental fan geometry and its typed Boolean masks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanError {
    UnsupportedLevel(u8),
    InvalidMaskWidth(u8),
    OutOfDomainBits { width: u8 },
    WidthMismatch { expected: u8, actual: u8 },
    MaximumRefinement,
    Geometry(GeometryError),
    ReferenceInvariant,
}

/// Nested partition of the plane's nonzero directions.
/// Levels 0,1,2,3 contain 8,16,32,64 half-open sectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RationalFan {
    level: u8,
    rays: Vec<Point>,
}

const BASE_RAYS: [Point; 8] = [
    Point { x: 1, y: 0 },
    Point { x: 1, y: 1 },
    Point { x: 0, y: 1 },
    Point { x: -1, y: 1 },
    Point { x: -1, y: 0 },
    Point { x: -1, y: -1 },
    Point { x: 0, y: -1 },
    Point { x: 1, y: -1 },
];

fn upper_half(p: Point) -> bool {
    p.y > 0 || (p.y == 0 && p.x > 0)
}

fn cross(a: Point, b: Point) -> i128 {
    i128::from(a.x) * i128::from(b.y) - i128::from(a.y) * i128::from(b.x)
}

fn angle_order(a: Point, b: Point) -> Ordering {
    match (upper_half(a), upper_half(b)) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => match cross(a, b).cmp(&0) {
            Ordering::Greater => Ordering::Less,
            Ordering::Less => Ordering::Greater,
            Ordering::Equal => Ordering::Equal,
        },
    }
}

impl RationalFan {
    /// Create one of four supported nested partitions without approximations.
    pub fn new(level: u8) -> Result<Self, FanError> {
        if level > 3 {
            return Err(FanError::UnsupportedLevel(level));
        }
        let mut rays = BASE_RAYS.to_vec();
        for _ in 0..level {
            let mut next = Vec::with_capacity(rays.len() * 2);
            for index in 0..rays.len() {
                let left = rays[index];
                let right = rays[(index + 1) % rays.len()];
                next.push(left);
                // Both rays have small integral coordinates at the four
                // supported levels, so their sum is representable in i64.
                next.push(Point {
                    x: left.x + right.x,
                    y: left.y + right.y,
                });
            }
            rays = next;
        }
        Ok(Self { level, rays })
    }

    #[must_use]
    pub const fn level(&self) -> u8 {
        self.level
    }

    #[must_use]
    pub fn sector_count(&self) -> usize {
        self.rays.len()
    }

    #[must_use]
    pub fn rays(&self) -> &[Point] {
        &self.rays
    }

    /// Binary search over strictly angle-ordered rays: O(log(2N)).
    /// A direction coinciding with a ray starts its corresponding sector.
    pub fn classify(&self, point: Point) -> Result<usize, GeometryError> {
        if point.x == 0 && point.y == 0 {
            return Err(GeometryError::OriginUndefined);
        }
        let upper = self
            .rays
            .partition_point(|&ray| angle_order(ray, point) != Ordering::Greater);
        Ok(if upper == 0 {
            self.rays.len() - 1
        } else {
            upper - 1
        })
    }

    /// Independent linear wedge oracle; never uses angular sorting/search.
    pub fn classify_scan(&self, point: Point) -> Result<usize, FanError> {
        if point.x == 0 && point.y == 0 {
            return Err(FanError::Geometry(GeometryError::OriginUndefined));
        }
        for index in 0..self.rays.len() {
            let start = self.rays[index];
            let end = self.rays[(index + 1) % self.rays.len()];
            if cross(start, point) >= 0 && cross(point, end) > 0 {
                return Ok(index);
            }
        }
        Err(FanError::ReferenceInvariant)
    }

    /// Compare the binary classifier with the previously qualified GB-1
    /// classifier, only for the identical level-zero partition.
    pub fn gb1_sector(&self, point: Point) -> Result<u8, FanError> {
        if self.level != 0 {
            return Err(FanError::UnsupportedLevel(self.level));
        }
        sector_of(point).map_err(FanError::Geometry)
    }
}

/// Fixed-width Boolean mask of cells in a bounded angular partition.
/// Bit i selects sector i; widths may be 8,16,32,64.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanMask {
    width: u8,
    bits: u64,
}

fn domain_mask(width: u8) -> u64 {
    if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    }
}

impl FanMask {
    pub fn new(width: u8, bits: u64) -> Result<Self, FanError> {
        if !matches!(width, 8 | 16 | 32 | 64) {
            return Err(FanError::InvalidMaskWidth(width));
        }
        if bits & !domain_mask(width) != 0 {
            return Err(FanError::OutOfDomainBits { width });
        }
        Ok(Self { width, bits })
    }

    #[must_use]
    pub const fn width(self) -> u8 {
        self.width
    }

    #[must_use]
    pub const fn bits(self) -> u64 {
        self.bits
    }

    #[must_use]
    pub const fn selected_cells(self) -> u32 {
        self.bits.count_ones()
    }

    fn verify_same_width(self, other: Self) -> Result<(), FanError> {
        if self.width != other.width {
            return Err(FanError::WidthMismatch {
                expected: self.width,
                actual: other.width,
            });
        }
        Ok(())
    }

    pub fn union(self, other: Self) -> Result<Self, FanError> {
        self.verify_same_width(other)?;
        Ok(Self {
            width: self.width,
            bits: self.bits | other.bits,
        })
    }

    pub fn intersection(self, other: Self) -> Result<Self, FanError> {
        self.verify_same_width(other)?;
        Ok(Self {
            width: self.width,
            bits: self.bits & other.bits,
        })
    }

    pub fn xor(self, other: Self) -> Result<Self, FanError> {
        self.verify_same_width(other)?;
        Ok(Self {
            width: self.width,
            bits: self.bits ^ other.bits,
        })
    }

    #[must_use]
    pub fn complement(self) -> Self {
        Self {
            width: self.width,
            bits: !self.bits & domain_mask(self.width),
        }
    }

    /// The parent sector i is split into children 2i and 2i+1.
    /// A selected parent selects both children, preserving the point predicate.
    pub fn refine_once(self) -> Result<Self, FanError> {
        if self.width == 64 {
            return Err(FanError::MaximumRefinement);
        }
        let mut bits = 0_u64;
        for i in 0..self.width {
            if self.bits & (1_u64 << i) != 0 {
                bits |= 3_u64 << (2 * u32::from(i));
            }
        }
        Ok(Self {
            width: self.width * 2,
            bits,
        })
    }

    /// Evaluate membership for a matching fan partition.
    pub fn contains_point(self, fan: &RationalFan, point: Point) -> Result<bool, FanError> {
        if usize::from(self.width) != fan.sector_count() {
            return Err(FanError::WidthMismatch {
                expected: self.width,
                actual: fan.sector_count() as u8,
            });
        }
        let sector = fan.classify(point).map_err(FanError::Geometry)?;
        Ok(self.bits & (1_u64 << sector) != 0)
    }

    /// Convert GB-1's exact eight-sector union to the typed GB-2 mask.
    #[must_use]
    pub const fn from_gb1(mask: SectorMask) -> Self {
        Self {
            width: 8,
            bits: mask.0 as u64,
        }
    }
}

/// Construct a selected-cell mask from consecutive bits of π, with no wrap.
pub fn pi_fan_mask(fan: &RationalFan, bit_offset: usize) -> Result<FanMask, PiError> {
    let mut stream = PiBitStream::new(bit_offset)?;
    let mut bits = 0_u64;
    for sector in 0..fan.sector_count() {
        if stream.read_bit()? {
            bits |= 1_u64 << sector;
        }
    }
    Ok(FanMask {
        width: fan.sector_count() as u8,
        bits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometric_boolean::SectorMask;

    fn sample_points() -> Vec<Point> {
        let mut points = Vec::new();
        for x in -16_i64..=16 {
            for y in -16_i64..=16 {
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
        points
    }

    #[test]
    fn supported_geometries_have_nested_rational_rays() {
        for level in 0..=3 {
            let fan = RationalFan::new(level).unwrap();
            assert_eq!(fan.level(), level);
            assert_eq!(fan.sector_count(), 8 << level);
            assert_eq!(fan.rays()[0], Point { x: 1, y: 0 });
            for i in 0..fan.sector_count() {
                let start = fan.rays()[i];
                let end = fan.rays()[(i + 1) % fan.sector_count()];
                assert!(cross(start, end) > 0);
                assert_eq!(fan.classify(start), Ok(i));
                assert_eq!(fan.classify_scan(start), Ok(i));
                assert_eq!(
                    fan.classify_scan(Point {
                        x: start.x + end.x,
                        y: start.y + end.y,
                    }),
                    fan.classify(Point {
                        x: start.x + end.x,
                        y: start.y + end.y,
                    })
                    .map_err(FanError::Geometry)
                );
            }
        }
        assert_eq!(RationalFan::new(4), Err(FanError::UnsupportedLevel(4)));
    }

    #[test]
    fn independent_wedge_scan_matches_binary_search_for_all_grid_points() {
        let samples = sample_points();
        for level in 0..=3 {
            let fan = RationalFan::new(level).unwrap();
            for &point in &samples {
                assert_eq!(
                    fan.classify_scan(point),
                    fan.classify(point).map_err(FanError::Geometry),
                    "level={level}, point={point:?}"
                );
            }
        }
        let fan = RationalFan::new(3).unwrap();
        assert_eq!(
            fan.classify(Point { x: 0, y: 0 }),
            Err(GeometryError::OriginUndefined)
        );
        assert_eq!(
            fan.classify_scan(Point { x: 0, y: 0 }),
            Err(FanError::Geometry(GeometryError::OriginUndefined))
        );
    }

    #[test]
    fn gb1_level_zero_is_identical_on_grid_and_boundary_rays() {
        let fan = RationalFan::new(0).unwrap();
        for point in sample_points() {
            assert_eq!(
                fan.classify(point).unwrap(),
                usize::from(fan.gb1_sector(point).unwrap())
            );
        }
        assert_eq!(
            RationalFan::new(1)
                .unwrap()
                .gb1_sector(Point { x: 1, y: 0 }),
            Err(FanError::UnsupportedLevel(1))
        );
    }

    #[test]
    fn child_indices_and_all_gb1_masks_preserve_membership() {
        let samples = sample_points();
        let fans: Vec<_> = (0..=3)
            .map(|level| RationalFan::new(level).unwrap())
            .collect();
        for &point in &samples {
            for level in 0..3 {
                assert_eq!(
                    fans[level + 1].classify(point).unwrap() / 2,
                    fans[level].classify(point).unwrap()
                );
            }
        }
        for bits in 0_u8..=u8::MAX {
            let mut mask = FanMask::from_gb1(SectorMask(bits));
            for level in 0..3 {
                let child = mask.refine_once().unwrap();
                for &point in &samples {
                    assert_eq!(
                        mask.contains_point(&fans[level], point),
                        child.contains_point(&fans[level + 1], point)
                    );
                }
                mask = child;
            }
            assert_eq!(mask.width(), 64);
            assert_eq!(mask.refine_once(), Err(FanError::MaximumRefinement));
        }
    }

    #[test]
    fn typed_masks_enforce_widths_and_64_bit_complement() {
        for width in [8, 16, 32, 64] {
            let full = FanMask::new(width, domain_mask(width)).unwrap();
            let empty = FanMask::new(width, 0).unwrap();
            let high = FanMask::new(width, 1_u64 << (width - 1)).unwrap();
            assert_eq!(full.selected_cells(), u32::from(width));
            assert_eq!(full.complement(), empty);
            assert_eq!(empty.complement(), full);
            assert_eq!(high.union(high.complement()), Ok(full));
            assert_eq!(high.intersection(high.complement()), Ok(empty));
            assert_eq!(high.xor(high), Ok(empty));
        }
        assert_eq!(FanMask::new(4, 0), Err(FanError::InvalidMaskWidth(4)));
        assert_eq!(
            FanMask::new(8, 256),
            Err(FanError::OutOfDomainBits { width: 8 })
        );
        assert_eq!(
            FanMask::new(8, 1)
                .unwrap()
                .union(FanMask::new(16, 1).unwrap()),
            Err(FanError::WidthMismatch {
                expected: 8,
                actual: 16,
            })
        );
        assert_eq!(
            FanMask::new(8, 1)
                .unwrap()
                .contains_point(&RationalFan::new(1).unwrap(), Point { x: 1, y: 0 }),
            Err(FanError::WidthMismatch {
                expected: 8,
                actual: 16,
            })
        );
    }

    #[test]
    fn pi_bits_round_trip_to_gb1_and_are_strictly_finite() {
        let fan0 = RationalFan::new(0).unwrap();
        let pi0 = pi_fan_mask(&fan0, 0).unwrap();
        assert_eq!(pi0, FanMask::from_gb1(SectorMask(0b0010_0100)));
        assert_eq!(pi0.bits(), 0b0010_0100);
        assert!(matches!(
            pi_fan_mask(&RationalFan::new(3).unwrap(), 4090),
            Err(PiError::Exhausted { .. })
        ));
    }
}
