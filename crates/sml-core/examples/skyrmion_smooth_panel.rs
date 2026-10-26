//! SKY-M2: floating-point, research-only spherical lattice charge oracle.
//!
//! This is a mathematical continuous-spin *software* control, not a magnetic
//! device, Langevin/thermal simulation, or SBG runtime implementation.
//! The experiment distinguishes smooth local spin perturbations from
//! hypothetical three-bit-per-spin nearest-axis quantization.

use std::f64::consts::PI;

#[allow(dead_code)]
#[path = "skyrmion_octahedron.rs"]
mod reference;

#[derive(Clone, Copy, Debug)]
struct Vector3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vector3 {
    const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }

    fn normalize(self) -> Result<Self, GeometryError> {
        let norm = self.norm();
        if !norm.is_finite() || norm < 1e-12 {
            return Err(GeometryError::InvalidSpin);
        }
        Ok(Self::new(self.x / norm, self.y / norm, self.z / norm))
    }

    fn scale_add(self, scale: f64, perturbation: Self) -> Self {
        Self::new(
            self.x + scale * perturbation.x,
            self.y + scale * perturbation.y,
            self.z + scale * perturbation.z,
        )
    }
}

const PX: Vector3 = Vector3::new(1.0, 0.0, 0.0);
const NX: Vector3 = Vector3::new(-1.0, 0.0, 0.0);
const PY: Vector3 = Vector3::new(0.0, 1.0, 0.0);
const NY: Vector3 = Vector3::new(0.0, -1.0, 0.0);
const PZ: Vector3 = Vector3::new(0.0, 0.0, 1.0);
const NZ: Vector3 = Vector3::new(0.0, 0.0, -1.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GeometryError {
    InvalidSpin,
    NonUniformBoundary,
    AntipodalEdge,
    AmbiguousTriangle,
    NonIntegralCharge,
}

#[derive(Clone, Copy, Debug)]
struct Charge {
    integer: i32,
    residual: f64,
}

#[derive(Clone, Debug)]
struct Field {
    cells: [Vector3; 25],
}

fn reference_texture(reverse: bool) -> Field {
    let mut cells = [PZ; 25];
    let mut ring = [[NX, NY, PX], [NX, NZ, PX], [NX, PY, PX]];
    if reverse {
        ring[0][1] = PY;
        ring[2][1] = NY;
    }
    for (y, row) in ring.iter().enumerate() {
        for (x, spin) in row.iter().enumerate() {
            cells[(y + 1) * 5 + x + 1] = *spin;
        }
    }
    Field { cells }
}

/// A bounded, deterministic, spatially varying perturbation.
/// Not noise calibrated to measured physical diffusion or temperature.
fn perturb(field: &Field, amplitude: f64) -> Result<Field, GeometryError> {
    if !amplitude.is_finite() || amplitude < 0.0 {
        return Err(GeometryError::InvalidSpin);
    }
    let mut changed = field.clone();
    for y in 1..4 {
        for x in 1..4 {
            let xf = x as f64;
            let yf = y as f64;
            let direction = Vector3::new(
                ((xf + 1.0) * 1.71 + yf * 0.31).sin(),
                ((yf + 1.0) * 2.17 + xf * 0.23).cos(),
                ((xf + yf + 2.0) * 0.73).sin(),
            );
            let index = y * 5 + x;
            changed.cells[index] = field.cells[index]
                .scale_add(amplitude, direction)
                .normalize()?;
        }
    }
    Ok(changed)
}

/// A spherical triangular solid angle: Berg-Luescher.
fn oriented_solid_angle(a: Vector3, b: Vector3, c: Vector3) -> Result<f64, GeometryError> {
    for (u, v) in [(a, b), (b, c), (a, c)] {
        if u.dot(v) < -1.0 + 1e-12 {
            return Err(GeometryError::AntipodalEdge);
        }
    }
    let determinant = a.dot(b.cross(c));
    let denominator = 1.0 + a.dot(b) + b.dot(c) + c.dot(a);
    if determinant.abs() < 1e-12 && denominator.abs() < 1e-12 {
        return Err(GeometryError::AmbiguousTriangle);
    }
    Ok(2.0 * determinant.atan2(denominator))
}

impl Field {
    fn spin(&self, x: usize, y: usize) -> Vector3 {
        self.cells[5 * y + x]
    }

    fn charge(&self) -> Result<Charge, GeometryError> {
        for spin in self.cells {
            if !spin.norm().is_finite() || (spin.norm() - 1.0).abs() > 1e-10 {
                return Err(GeometryError::InvalidSpin);
            }
        }

        for index in 0..5 {
            for spin in [
                self.spin(index, 0),
                self.spin(index, 4),
                self.spin(0, index),
                self.spin(4, index),
            ] {
                let boundary_difference = Vector3::new(spin.x - PZ.x, spin.y - PZ.y, spin.z - PZ.z);
                if boundary_difference.norm() > 1e-10 {
                    return Err(GeometryError::NonUniformBoundary);
                }
            }
        }

        let mut signed_area = 0.0;
        for y in 0..4 {
            for x in 0..4 {
                let a = self.spin(x, y);
                let b = self.spin(x + 1, y);
                let c = self.spin(x, y + 1);
                let d = self.spin(x + 1, y + 1);
                signed_area += oriented_solid_angle(a, b, d)?;
                signed_area += oriented_solid_angle(a, d, c)?;
            }
        }
        let raw = signed_area / (4.0 * PI);
        let nearest_integer = raw.round();
        if !raw.is_finite() || (raw - nearest_integer).abs() > 1e-9 {
            return Err(GeometryError::NonIntegralCharge);
        }
        Ok(Charge {
            integer: nearest_integer as i32,
            residual: raw - nearest_integer,
        })
    }

    /// Quantize each vector independently to the nearest axial direction.
    /// This is an intentionally lossy *separate* control, not topological
    /// evolution and not a physical skyrmion readout mechanism.
    fn quantized_payload(&self) -> Option<u8> {
        let mut bits = Vec::with_capacity(75);
        for spin in self.cells {
            let ax = spin.x.abs();
            let ay = spin.y.abs();
            let az = spin.z.abs();
            let code = if ax >= ay && ax >= az {
                if spin.x >= 0.0 {
                    0_u8
                } else {
                    1_u8
                }
            } else if ay >= ax && ay >= az {
                if spin.y >= 0.0 {
                    2_u8
                } else {
                    3_u8
                }
            } else if spin.z >= 0.0 {
                4_u8
            } else {
                5_u8
            };
            for bit in 0..3 {
                bits.push((code >> bit) & 1);
            }
        }
        reference::decode_reference_texture(&bits)
    }
}

fn main() {
    println!("SKY-M2: deterministic local continuous-spin perturbations; not hardware noise");
    println!("payload,amplitude,charge,residual,nearest_axis_payload");
    for payload in 0..=1_u8 {
        let original = reference_texture(payload == 1);
        for amplitude in [0.0, 0.1, 0.25, 0.5, 0.6, 0.75, 1.0] {
            let moved = perturb(&original, amplitude).expect("bounded finite perturbation");
            let charge = moved.charge().expect("frozen lattice fixture");
            let quantized = moved
                .quantized_payload()
                .map_or_else(|| "ERASURE".to_string(), |decoded| decoded.to_string());
            println!(
                "{payload},{amplitude:.2},{},{:.6e},{}",
                charge.integer, charge.residual, quantized
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_octahedral_baselines_have_opposite_charges() {
        let minus = reference_texture(false);
        let plus = reference_texture(true);
        assert_eq!(minus.charge().unwrap().integer, -1);
        assert_eq!(plus.charge().unwrap().integer, 1);
        assert_eq!(minus.quantized_payload(), Some(0));
        assert_eq!(plus.quantized_payload(), Some(1));
    }

    #[test]
    fn bounded_local_perturbations_conserve_degree() {
        for payload in 0..=1_u8 {
            let base = reference_texture(payload == 1);
            let expected = if payload == 0 { -1 } else { 1 };
            for amplitude in [0.0, 0.1, 0.25, 0.5, 0.6, 0.75] {
                let changed = perturb(&base, amplitude).unwrap();
                let charge = changed.charge().unwrap();
                assert_eq!(
                    charge.integer, expected,
                    "payload={payload}, amplitude={amplitude}"
                );
                assert!(charge.residual.abs() < 1e-9);
            }
        }
    }

    #[test]
    fn quantization_can_erase_even_when_continuous_degree_is_preserved() {
        let changed = perturb(&reference_texture(false), 0.6).unwrap();
        assert_eq!(changed.charge().unwrap().integer, -1);
        assert_eq!(changed.quantized_payload(), None);
    }

    #[test]
    fn high_amplitude_does_not_guarantee_sector_preservation() {
        let changed = perturb(&reference_texture(false), 1.0).unwrap();
        assert_eq!(changed.charge().unwrap().integer, 0);
        // No claim that a continuous physical homotopy crosses sectors
        // without singularities: finite triangulation/geodesic conditions
        // can fail between the sampled amplitudes.
    }

    #[test]
    fn malformed_boundary_antipodal_edges_and_nonfinite_input_fail_closed() {
        let mut invalid = reference_texture(false);
        invalid.cells[0] = PX;
        assert_eq!(
            invalid.charge().unwrap_err(),
            GeometryError::NonUniformBoundary
        );

        let mut opposite = Field { cells: [PZ; 25] };
        opposite.cells[12] = NZ;
        assert_eq!(opposite.charge().unwrap_err(), GeometryError::AntipodalEdge);

        assert_eq!(
            perturb(&reference_texture(false), f64::NAN).unwrap_err(),
            GeometryError::InvalidSpin
        );
    }
}
