//! SKY-M0: exact discrete octahedral-spin topological-charge oracle.
//!
//! This is NOT a micromagnetic simulation, not a binary skyrmion bit, and not
//! evidence of speed, energy savings, or adversarial robustness. It is a
//! controlled six-state restriction of the Berg-Luescher lattice O(3) charge.
//! Kept as a stand-alone research example until matched utility is measured.
//!
//! The spins are ±x, ±y, ±z. Every admissible spherical triangle has signed
//! area 0 or ±pi/2; eight oriented octants make one integer charge. We reject
//! antipodal pairs, for which the shortest geodesic is ambiguous.
//! A uniform boundary closes the grid topologically.

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Spin {
    Px,
    Nx,
    Py,
    Ny,
    Pz,
    Nz,
}

impl Spin {
    fn axis_sign(self) -> (u8, i8) {
        match self {
            Self::Px => (0, 1),
            Self::Nx => (0, -1),
            Self::Py => (1, 1),
            Self::Ny => (1, -1),
            Self::Pz => (2, 1),
            Self::Nz => (2, -1),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ChargeError {
    InvalidShape,
    NonUniformBoundary,
    AntipodalTriangle,
    NonIntegralCharge,
    ChargeOverflow,
    OutOfBounds,
    SectorChanged { before: i32, after: i32 },
}

/// A deliberately simple byte-per-spin reference, not a packed SBG layout.
#[derive(Clone)]
struct OctahedralField {
    width: usize,
    height: usize,
    spins: Vec<Spin>,
}

impl OctahedralField {
    fn new(width: usize, height: usize, spins: Vec<Spin>) -> Result<Self, ChargeError> {
        if width < 2 || height < 2 || width.checked_mul(height) != Some(spins.len()) {
            return Err(ChargeError::InvalidShape);
        }
        Ok(Self {
            width,
            height,
            spins,
        })
    }

    fn at(&self, x: usize, y: usize) -> Spin {
        self.spins[y * self.width + x]
    }

    fn signed_octants(&self) -> Result<i64, ChargeError> {
        let reference = self.at(0, 0);
        for x in 0..self.width {
            if self.at(x, 0) != reference || self.at(x, self.height - 1) != reference {
                return Err(ChargeError::NonUniformBoundary);
            }
        }
        for y in 0..self.height {
            if self.at(0, y) != reference || self.at(self.width - 1, y) != reference {
                return Err(ChargeError::NonUniformBoundary);
            }
        }

        let mut count = 0_i64;
        for y in 0..(self.height - 1) {
            for x in 0..(self.width - 1) {
                // Fixed orientation for both triangles of each square cell.
                let a = self.at(x, y);
                let b = self.at(x + 1, y);
                let c = self.at(x, y + 1);
                let d = self.at(x + 1, y + 1);
                for triangle in [(a, b, d), (a, d, c)] {
                    let octant = oriented_octant(triangle.0, triangle.1, triangle.2)?;
                    count = count
                        .checked_add(i64::from(octant))
                        .ok_or(ChargeError::ChargeOverflow)?;
                }
            }
        }
        Ok(count)
    }

    fn charge(&self) -> Result<i32, ChargeError> {
        let octants = self.signed_octants()?;
        if octants % 8 != 0 {
            return Err(ChargeError::NonIntegralCharge);
        }
        i32::try_from(octants / 8).map_err(|_| ChargeError::ChargeOverflow)
    }

    /// An explicit *software* admission rule, not physical topological protection.
    /// Reference-only O(width*height) full recomputation and one cloned grid.
    fn try_sector_preserving_write(
        &mut self,
        x: usize,
        y: usize,
        replacement: Spin,
    ) -> Result<i32, ChargeError> {
        if x >= self.width || y >= self.height {
            return Err(ChargeError::OutOfBounds);
        }
        let before = self.charge()?;
        let mut candidate = self.clone();
        candidate.spins[y * self.width + x] = replacement;
        let after = candidate.charge()?;
        if before != after {
            return Err(ChargeError::SectorChanged { before, after });
        }
        *self = candidate;
        Ok(after)
    }
}

/// O(1)-neighborhood, allocation-free update oracle with a cached global charge.
///
/// The cache is admitted only after one complete O(width*height) charge scan.
/// Subsequent individual interior-spin rewrites recompute at most four grid
/// squares (eight triangles before + eight after), independent of grid area.
/// This does NOT protect the microstate bit itself against silent corruption.
#[derive(Clone)]
struct CachedChargeField {
    field: OctahedralField,
    signed_octants: i64,
}

impl CachedChargeField {
    fn new(field: OctahedralField) -> Result<Self, ChargeError> {
        let signed_octants = field.signed_octants()?;
        if signed_octants % 8 != 0 {
            return Err(ChargeError::NonIntegralCharge);
        }
        i32::try_from(signed_octants / 8).map_err(|_| ChargeError::ChargeOverflow)?;
        Ok(Self {
            field,
            signed_octants,
        })
    }

    fn try_sector_preserving_write(
        &mut self,
        x: usize,
        y: usize,
        replacement: Spin,
    ) -> Result<i32, ChargeError> {
        if x >= self.field.width || y >= self.field.height {
            return Err(ChargeError::OutOfBounds);
        }
        let before =
            i32::try_from(self.signed_octants / 8).map_err(|_| ChargeError::ChargeOverflow)?;
        if self.field.at(x, y) == replacement {
            return Ok(before);
        }
        if x == 0 || y == 0 || x + 1 == self.field.width || y + 1 == self.field.height {
            // A single boundary spin cannot change without invalidating the
            // uniform reference boundary of the initial admissible field.
            return Err(ChargeError::NonUniformBoundary);
        }

        let mut delta = 0_i64;
        // Exactly the grid squares incident to the edited vertex.
        for cy in (y - 1)..=y {
            for cx in (x - 1)..=x {
                let locations = [(cx, cy), (cx + 1, cy), (cx, cy + 1), (cx + 1, cy + 1)];
                let before_corners = locations.map(|(px, py)| self.field.at(px, py));
                let mut after_corners = before_corners;
                for (corner, (px, py)) in locations.iter().copied().enumerate() {
                    if px == x && py == y {
                        after_corners[corner] = replacement;
                    }
                }
                for (a, b, c) in [(0, 1, 3), (0, 3, 2)] {
                    let before_octant =
                        oriented_octant(before_corners[a], before_corners[b], before_corners[c])?;
                    let after_octant =
                        oriented_octant(after_corners[a], after_corners[b], after_corners[c])?;
                    delta += i64::from(after_octant) - i64::from(before_octant);
                }
            }
        }

        let updated_octants = self
            .signed_octants
            .checked_add(delta)
            .ok_or(ChargeError::ChargeOverflow)?;
        if updated_octants % 8 != 0 {
            return Err(ChargeError::NonIntegralCharge);
        }
        let after = i32::try_from(updated_octants / 8).map_err(|_| ChargeError::ChargeOverflow)?;
        if before != after {
            return Err(ChargeError::SectorChanged { before, after });
        }
        // Atomic admission: neither field nor cache changed before here.
        self.field.spins[y * self.field.width + x] = replacement;
        self.signed_octants = updated_octants;
        Ok(after)
    }
}

/// Berg-Luescher oriented spherical-triangle area divided by pi/2.
///
/// For axis-aligned unit spins, non-antipodal triangles with three different
/// axes have dot products zero and chirality ±1, hence solid angle ±pi/2.
/// Repeated spins yield zero area. Any antipodal pair is rejected rather than
/// allowing a non-unique geodesic to produce a spurious 'integer charge'.
fn oriented_octant(a: Spin, b: Spin, c: Spin) -> Result<i8, ChargeError> {
    let (ax, asgn) = a.axis_sign();
    let (bx, bsgn) = b.axis_sign();
    let (cx, csgn) = c.axis_sign();

    for ((u, us), (v, vs)) in [
        ((ax, asgn), (bx, bsgn)),
        ((bx, bsgn), (cx, csgn)),
        ((ax, asgn), (cx, csgn)),
    ] {
        if u == v && us != vs {
            return Err(ChargeError::AntipodalTriangle);
        }
    }
    if ax == bx || bx == cx || ax == cx {
        return Ok(0);
    }
    let orientation = match (ax, bx, cx) {
        (0, 1, 2) | (1, 2, 0) | (2, 0, 1) => 1,
        _ => -1,
    };
    Ok(orientation * asgn * bsgn * csgn)
}

/// A 5x5 radial octahedral texture: fixed +z boundary, -z core and an
/// equatorial ring. Chirality reversal flips its charge, not the boundary.
fn reference_texture(reverse_chirality: bool) -> OctahedralField {
    use Spin::{Nx, Ny, Nz, Px, Py, Pz};
    let mut spins = vec![Pz; 25];
    let mut equator = [[Nx, Ny, Px], [Nx, Nz, Px], [Nx, Py, Px]];
    if reverse_chirality {
        equator[0][1] = Py;
        equator[2][1] = Ny;
    }
    for (y, row) in equator.iter().enumerate() {
        for (x, spin) in row.iter().enumerate() {
            spins[(y + 1) * 5 + (x + 1)] = *spin;
        }
    }
    OctahedralField::new(5, 5, spins).expect("known reference shape")
}

/// Hypothetical three-bit serialized representation of each six-state spin.
/// Reference Vec<u8> storage is NOT 75 actual packed bytes or a device model.
pub(crate) fn encoded_reference_texture(reverse_chirality: bool) -> Vec<u8> {
    reference_texture(reverse_chirality)
        .spins
        .iter()
        .flat_map(|spin| (0..3).map(move |bit| ((*spin as u8) >> bit) & 1))
        .collect()
}

/// Returns the useful payload (0 for Q=-1, 1 for Q=+1), or an erasure.
/// This is a policy decoder: it does not restore an altered lattice state.
pub(crate) fn decode_reference_texture(bits: &[u8]) -> Option<u8> {
    if bits.len() != 75 || bits.iter().any(|bit| *bit > 1) {
        return None;
    }
    let mut spins = Vec::with_capacity(25);
    for chunk in bits.chunks_exact(3) {
        let code = chunk[0] | (chunk[1] << 1) | (chunk[2] << 2);
        spins.push(match code {
            0 => Spin::Px,
            1 => Spin::Nx,
            2 => Spin::Py,
            3 => Spin::Ny,
            4 => Spin::Pz,
            5 => Spin::Nz,
            _ => return None,
        });
    }
    let field = OctahedralField::new(5, 5, spins).ok()?;
    match field.charge().ok()? {
        -1 => Some(0),
        1 => Some(1),
        _ => None,
    }
}

fn main() {
    let mut texture = reference_texture(false);
    let charge = texture.charge().expect("known reference configuration");
    println!(
        "SKY-M0 octahedral lattice: charge={charge}, octants={}",
        texture.signed_octants().expect("known texture")
    );
    let preserved = texture
        .try_sector_preserving_write(1, 1, Spin::Ny)
        .expect("admissible sector-preserving rewrite");
    assert_eq!(preserved, charge);
    assert_eq!(
        texture.try_sector_preserving_write(2, 2, Spin::Pz),
        Err(ChargeError::SectorChanged {
            before: -1,
            after: 0
        })
    );
    println!("SKY-M0 guarded rewrite: preserved sector and rejected forbidden change.");
    assert_eq!(
        decode_reference_texture(&encoded_reference_texture(false)),
        Some(0)
    );
    assert_eq!(
        decode_reference_texture(&encoded_reference_texture(true)),
        Some(1)
    );
    let mut cached =
        CachedChargeField::new(reference_texture(false)).expect("known initial cached texture");
    assert_eq!(cached.try_sector_preserving_write(1, 1, Spin::Ny), Ok(-1));
    assert_eq!(
        cached.try_sector_preserving_write(2, 2, Spin::Pz),
        Err(ChargeError::SectorChanged {
            before: -1,
            after: 0
        })
    );
    println!("SKY-M3 local cached-charge guard: same admission as full scan.");
    println!("Reference-only: not a skyrmion device, bit-encoding, or model benchmark.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_delta_matches_full_scan_exhaustively_across_spins_and_sites() {
        let choices = [Spin::Px, Spin::Nx, Spin::Py, Spin::Ny, Spin::Pz, Spin::Nz];
        for reverse in [false, true] {
            for y in 0..5 {
                for x in 0..5 {
                    for replacement in choices {
                        let mut original = reference_texture(reverse);
                        let mut cached = CachedChargeField::new(original.clone()).unwrap();
                        let expected = original.try_sector_preserving_write(x, y, replacement);
                        let actual = cached.try_sector_preserving_write(x, y, replacement);
                        assert_eq!(
                            actual, expected,
                            "reverse={reverse} x={x} y={y} spin={replacement:?}"
                        );
                        assert_eq!(cached.field.spins, original.spins);
                        assert_eq!(
                            cached.signed_octants,
                            cached.field.signed_octants().unwrap()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn cached_guard_is_constant_neighborhood_on_large_grids() {
        let large = OctahedralField::new(129, 129, vec![Spin::Pz; 129 * 129]).unwrap();
        let mut cached = CachedChargeField::new(large).unwrap();
        assert_eq!(cached.signed_octants, 0);
        assert_eq!(cached.try_sector_preserving_write(64, 64, Spin::Nx), Ok(0));
        assert_eq!(
            cached.signed_octants,
            cached.field.signed_octants().unwrap()
        );
        assert_eq!(
            cached.try_sector_preserving_write(64, 64, Spin::Nz),
            Err(ChargeError::AntipodalTriangle)
        );
        assert_eq!(
            cached.signed_octants,
            cached.field.signed_octants().unwrap()
        );
    }

    #[test]
    fn uniform_field_has_zero_charge() {
        let texture = OctahedralField::new(5, 5, vec![Spin::Pz; 25]).unwrap();
        assert_eq!(texture.signed_octants(), Ok(0));
        assert_eq!(texture.charge(), Ok(0));
    }

    #[test]
    fn oriented_triangles_have_exact_quarter_sphere_area() {
        assert_eq!(oriented_octant(Spin::Px, Spin::Py, Spin::Pz), Ok(1));
        assert_eq!(oriented_octant(Spin::Py, Spin::Px, Spin::Pz), Ok(-1));
        assert_eq!(oriented_octant(Spin::Px, Spin::Px, Spin::Pz), Ok(0));
        assert_eq!(
            oriented_octant(Spin::Px, Spin::Nx, Spin::Pz),
            Err(ChargeError::AntipodalTriangle)
        );
    }

    #[test]
    fn texture_and_chirality_reversal_have_opposite_integer_charges() {
        assert_eq!(reference_texture(false).signed_octants(), Ok(-8));
        assert_eq!(reference_texture(false).charge(), Ok(-1));
        assert_eq!(reference_texture(true).signed_octants(), Ok(8));
        assert_eq!(reference_texture(true).charge(), Ok(1));
    }

    #[test]
    fn admissible_distortion_preserves_sector() {
        let mut texture = reference_texture(false);
        assert_eq!(texture.try_sector_preserving_write(1, 1, Spin::Ny), Ok(-1));
        assert_eq!(texture.charge(), Ok(-1));
    }

    #[test]
    fn sector_changing_mutation_is_refused_atomically() {
        let mut texture = reference_texture(false);
        assert_eq!(
            texture.try_sector_preserving_write(2, 2, Spin::Pz),
            Err(ChargeError::SectorChanged {
                before: -1,
                after: 0
            })
        );
        assert_eq!(texture.charge(), Ok(-1));
        assert_eq!(texture.at(2, 2), Spin::Nz);
    }

    #[test]
    fn invalid_boundaries_shapes_and_antipodal_edges_fail_closed() {
        assert!(matches!(
            OctahedralField::new(5, 5, vec![Spin::Pz; 3]),
            Err(ChargeError::InvalidShape)
        ));
        let mut texture = reference_texture(false);
        assert_eq!(
            texture.try_sector_preserving_write(5, 0, Spin::Pz),
            Err(ChargeError::OutOfBounds)
        );
        assert_eq!(
            texture.try_sector_preserving_write(0, 0, Spin::Nx),
            Err(ChargeError::NonUniformBoundary)
        );
        assert_eq!(texture.charge(), Ok(-1));

        let mut uniform = OctahedralField::new(3, 3, vec![Spin::Pz; 9]).unwrap();
        uniform.spins[4] = Spin::Nz;
        assert_eq!(uniform.charge(), Err(ChargeError::AntipodalTriangle));
    }
}
