//! Bounded exact diagnostics, not a similarity policy or an authority signal.
use core::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticError {
    Shape,
    ZeroNorm,
    Limit,
    Overflow,
}
impl fmt::Display for DiagnosticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DiagnosticError {}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyGeometry {
    pub dot: i128,
    pub left_norm_sq: u128,
    pub right_norm_sq: u128,
}
impl KeyGeometry {
    pub fn measure(
        left: &[i16],
        right: &[i16],
        max_dimension: usize,
    ) -> Result<Self, DiagnosticError> {
        if left.is_empty() || left.len() != right.len() {
            return Err(DiagnosticError::Shape);
        }
        if left.len() > max_dimension {
            return Err(DiagnosticError::Limit);
        }
        let (mut dot, mut a, mut b) = (0_i128, 0_u128, 0_u128);
        for (&x, &y) in left.iter().zip(right) {
            let (x, y) = (i128::from(x), i128::from(y));
            dot = dot.checked_add(x * y).ok_or(DiagnosticError::Overflow)?;
            a = a
                .checked_add((x * x) as u128)
                .ok_or(DiagnosticError::Overflow)?;
            b = b
                .checked_add((y * y) as u128)
                .ok_or(DiagnosticError::Overflow)?;
        }
        if a == 0 || b == 0 {
            return Err(DiagnosticError::ZeroNorm);
        }
        Ok(Self {
            dot,
            left_norm_sq: a,
            right_norm_sq: b,
        })
    }
    /// Exact numerator/denominator for cos^2, not cos; sign remains in dot.
    pub fn squared_cosine_ratio(self) -> Result<(u128, u128), DiagnosticError> {
        if self.left_norm_sq == 0 || self.right_norm_sq == 0 {
            return Err(DiagnosticError::ZeroNorm);
        }
        let n = self
            .dot
            .unsigned_abs()
            .checked_mul(self.dot.unsigned_abs())
            .ok_or(DiagnosticError::Overflow)?;
        let d = self
            .left_norm_sq
            .checked_mul(self.right_norm_sq)
            .ok_or(DiagnosticError::Overflow)?;
        if n > d {
            return Err(DiagnosticError::Shape);
        }
        Ok((n, d))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rounded_denominator_is_not_exact_cosine() {
        let g = KeyGeometry::measure(&[4096, 0], &[4096, 4096], 2).unwrap();
        let (n, d) = g.squared_cosine_ratio().unwrap();
        assert_eq!(2 * n, d);
        assert_eq!(1000 / (2_u64.isqrt()), 1000);
    }
    #[test]
    fn shape_norm_limits_and_signed_dot_are_checked() {
        assert_eq!(
            KeyGeometry::measure(&[], &[], 4),
            Err(DiagnosticError::Shape)
        );
        assert_eq!(
            KeyGeometry::measure(&[1], &[1, 2], 4),
            Err(DiagnosticError::Shape)
        );
        assert_eq!(
            KeyGeometry::measure(&[0], &[1], 4),
            Err(DiagnosticError::ZeroNorm)
        );
        assert_eq!(
            KeyGeometry::measure(&[1], &[1], 0),
            Err(DiagnosticError::Limit)
        );
        let g = KeyGeometry::measure(&[i16::MIN, 7], &[i16::MAX, -7], 2).unwrap();
        assert!(g.dot < 0);
        let (n, d) = g.squared_cosine_ratio().unwrap();
        assert!(n <= d);
    }
    #[test]
    fn zero_dot_and_extreme_products_remain_exact() {
        assert_eq!(KeyGeometry::measure(&[1, 1], &[1, -1], 2).unwrap().dot, 0);
        let invalid = KeyGeometry {
            dot: i128::MAX,
            left_norm_sq: u128::MAX,
            right_norm_sq: 2,
        };
        assert_eq!(
            invalid.squared_cosine_ratio(),
            Err(DiagnosticError::Overflow)
        );
    }
}
