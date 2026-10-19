//! Bounded exact Fourier–Walsh reference for Boolean truth tables and GF(2)
//! Möbius coefficients. This is a synthetic oracle, not a scalable language
//! model or a generic Learning Parity with Noise (LPN) solver.
//!
//! Input indices encode variables little-endian: bit i of x is variable i.
//! Walsh characters are chi_s(x) = (-1)^popcount(x & s).
#![forbid(unsafe_code)]

use core::fmt;

pub const MAX_EXACT_VARIABLES: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanSpectralError {
    EmptyTruthTable,
    LengthNotPowerOfTwo { len: usize },
    TooManyVariables { variables: u32 },
    NonFiniteInput,
    MaskOutOfRange { mask: usize, len: usize },
}

impl fmt::Display for BooleanSpectralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BooleanSpectralError {}

/// Validate the COMPLETE domain size before mutating or allocating. n=0
/// (one truth-table entry) is mathematically meaningful.
pub fn variables_for_table_len(len: usize) -> Result<u32, BooleanSpectralError> {
    if len == 0 {
        return Err(BooleanSpectralError::EmptyTruthTable);
    }
    if !len.is_power_of_two() {
        return Err(BooleanSpectralError::LengthNotPowerOfTwo { len });
    }
    let n = len.ilog2();
    if n > MAX_EXACT_VARIABLES {
        return Err(BooleanSpectralError::TooManyVariables { variables: n });
    }
    Ok(n)
}

/// Unnormalized fast Walsh–Hadamard transform (WHT) in O(n * 2^n).
/// It is NOT a correlation under an empirical, nonuniform data distribution.
/// Rejects bad shape or any NaN/Inf BEFORE changing the caller buffer.
pub fn fwht_in_place(values: &mut [f64]) -> Result<(), BooleanSpectralError> {
    variables_for_table_len(values.len())?;
    if values.iter().any(|x| !x.is_finite()) {
        return Err(BooleanSpectralError::NonFiniteInput);
    }
    let mut span = 1usize;
    while span < values.len() {
        for block in (0..values.len()).step_by(span * 2) {
            for offset in 0..span {
                let a = values[block + offset];
                let b = values[block + span + offset];
                values[block + offset] = a + b;
                values[block + span + offset] = a - b;
            }
        }
        span *= 2;
    }
    // The maximum n=16 bound does not imply arbitrary f64 magnitudes are safe.
    // Refuse overflow rather than returning invalid Fourier coordinates.
    if values.iter().any(|x| !x.is_finite()) {
        return Err(BooleanSpectralError::NonFiniteInput);
    }
    Ok(())
}

/// Exact uniform Boolean Fourier coefficients for g(x)=(-1)^f(x).
/// The output is indexed by parity mask S, 0 <= S < 2^n.
/// Any nonuniform sampling must be analyzed separately.
pub fn uniform_walsh_spectrum(truth: &[bool]) -> Result<Vec<f64>, BooleanSpectralError> {
    variables_for_table_len(truth.len())?;
    let mut values: Vec<f64> = truth
        .iter()
        .map(|bit| if *bit { -1.0 } else { 1.0 })
        .collect();
    fwht_in_place(&mut values)?;
    let normalization = 1.0 / truth.len() as f64;
    for value in &mut values {
        *value *= normalization;
    }
    Ok(values)
}

/// Independent O(2^n) direct-definition reference, not a FWHT.
pub fn uniform_walsh_coefficient_direct(
    truth: &[bool],
    mask: usize,
) -> Result<f64, BooleanSpectralError> {
    variables_for_table_len(truth.len())?;
    if mask >= truth.len() {
        return Err(BooleanSpectralError::MaskOutOfRange {
            mask,
            len: truth.len(),
        });
    }
    let mut total: i64 = 0;
    for (x, bit) in truth.iter().enumerate() {
        let sign_negative = *bit ^ (((x & mask).count_ones() & 1) != 0);
        total += if sign_negative { -1 } else { 1 };
    }
    Ok(total as f64 / truth.len() as f64)
}

/// Möbius transform on the subset lattice, over GF(2): from the truth
/// table to ANF coefficients. Applying it twice recovers the truth table.
/// This is NOT the real-valued Walsh transform and uses XOR, not addition.
pub fn mobius_gf2_in_place(values: &mut [bool]) -> Result<(), BooleanSpectralError> {
    variables_for_table_len(values.len())?;
    let mut bit = 1usize;
    while bit < values.len() {
        for x in 0..values.len() {
            if x & bit != 0 {
                values[x] ^= values[x ^ bit];
            }
        }
        bit *= 2;
    }
    Ok(())
}

/// Evaluate a GF(2) ANF: XOR all coefficients whose supports are subsets
/// of the requested assignment. No hidden Walsh normalization.
pub fn evaluate_anf(
    coefficients: &[bool],
    assignment: usize,
) -> Result<bool, BooleanSpectralError> {
    variables_for_table_len(coefficients.len())?;
    if assignment >= coefficients.len() {
        return Err(BooleanSpectralError::MaskOutOfRange {
            mask: assignment,
            len: coefficients.len(),
        });
    }
    let mut subset = assignment;
    let mut value = false;
    loop {
        value ^= coefficients[subset];
        if subset == 0 {
            break;
        }
        subset = (subset - 1) & assignment;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() <= 1e-12, "{a} != {b}");
    }

    fn parity_truth(n: u32, mask: usize) -> Vec<bool> {
        (0..(1usize << n))
            .map(|x| ((x & mask).count_ones() & 1) != 0)
            .collect()
    }

    #[test]
    fn n_zero_and_constant_functions_have_only_dc() {
        assert_eq!(variables_for_table_len(1).unwrap(), 0);
        assert_eq!(uniform_walsh_spectrum(&[false]).unwrap(), vec![1.0]);
        assert_eq!(uniform_walsh_spectrum(&[true]).unwrap(), vec![-1.0]);
        assert_eq!(uniform_walsh_spectrum(&[true; 8]).unwrap()[0], -1.0);
        assert!(uniform_walsh_spectrum(&[true; 8]).unwrap()[1..]
            .iter()
            .all(|v| *v == 0.0));
        let mut a = vec![true];
        mobius_gf2_in_place(&mut a).unwrap();
        assert!(evaluate_anf(&a, 0).unwrap());
    }

    #[test]
    fn xor_four_has_one_exact_walsh_coefficient() {
        let truth = parity_truth(4, 0b1101);
        let spectrum = uniform_walsh_spectrum(&truth).unwrap();
        for (mask, value) in spectrum.iter().enumerate() {
            near(*value, if mask == 0b1101 { 1.0 } else { 0.0 });
            near(
                *value,
                uniform_walsh_coefficient_direct(&truth, mask).unwrap(),
            );
        }
        let mut anf = truth.clone();
        mobius_gf2_in_place(&mut anf).unwrap();
        for (x, &expected) in truth.iter().enumerate() {
            assert_eq!(evaluate_anf(&anf, x).unwrap(), expected);
        }
        assert_eq!(anf.iter().filter(|b| **b).count(), 3);
    }

    #[test]
    fn and_is_single_anf_monomial_but_walsh_is_dense() {
        let truth: Vec<_> = (0..8).map(|x| x == 7).collect();
        let spectrum = uniform_walsh_spectrum(&truth).unwrap();
        near(spectrum[0], 0.75);
        for mask in 1..8 {
            near(spectrum[mask].abs(), 0.25);
        }
        let mut anf = truth.clone();
        mobius_gf2_in_place(&mut anf).unwrap();
        assert_eq!(anf.iter().filter(|b| **b).count(), 1);
        assert!(anf[7]);
        mobius_gf2_in_place(&mut anf).unwrap();
        assert_eq!(anf, truth);
    }

    #[test]
    fn majority_three_and_arbitrary_tables_satisfy_parseval() {
        let mut samples = vec![
            (0..8)
                .map(|x: usize| x.count_ones() >= 2)
                .collect::<Vec<_>>(),
            (0..32)
                .map(|x: usize| (((x * 31 + 7) ^ (x >> 1)) % 7) < 3)
                .collect::<Vec<_>>(),
        ];
        samples.push(parity_truth(1, 1));
        for truth in samples {
            let fast = uniform_walsh_spectrum(&truth).unwrap();
            near(fast.iter().map(|c| c * c).sum(), 1.0);
            let mut anf = truth.clone();
            mobius_gf2_in_place(&mut anf).unwrap();
            for (x, &expected) in truth.iter().enumerate() {
                assert_eq!(evaluate_anf(&anf, x).unwrap(), expected);
            }
            mobius_gf2_in_place(&mut anf).unwrap();
            assert_eq!(truth, anf);
            for mask in 0..truth.len() {
                near(
                    fast[mask],
                    uniform_walsh_coefficient_direct(&truth, mask).unwrap(),
                );
            }
        }
    }

    #[test]
    fn raw_wht_is_self_inverse_up_to_length() {
        let mut data: Vec<_> = (0..16).map(|i| i as f64 - 3.0).collect();
        let original = data.clone();
        fwht_in_place(&mut data).unwrap();
        fwht_in_place(&mut data).unwrap();
        for (a, b) in data.iter().zip(original) {
            near(*a, b * 16.0);
        }
    }

    #[test]
    fn n_sixteen_full_table_is_bounded_and_exact() {
        let truth = parity_truth(16, 0x8421);
        let spectrum = uniform_walsh_spectrum(&truth).unwrap();
        assert_eq!(spectrum.len(), 65_536);
        near(spectrum[0x8421], 1.0);
        near(spectrum.iter().map(|v| v * v).sum(), 1.0);
        assert!(spectrum
            .iter()
            .enumerate()
            .all(|(i, v)| i == 0x8421 || *v == 0.0));
        let mut anf = truth.clone();
        mobius_gf2_in_place(&mut anf).unwrap();
        for &x in &[0, 1, 0x8888, 0xffff] {
            assert_eq!(evaluate_anf(&anf, x).unwrap(), truth[x]);
        }
        mobius_gf2_in_place(&mut anf).unwrap();
        assert_eq!(anf, truth);
    }

    #[test]
    fn failures_are_bounded_and_prevalidate_before_mutation() {
        assert_eq!(
            variables_for_table_len(0),
            Err(BooleanSpectralError::EmptyTruthTable)
        );
        assert_eq!(
            variables_for_table_len(3),
            Err(BooleanSpectralError::LengthNotPowerOfTwo { len: 3 })
        );
        assert_eq!(
            variables_for_table_len(131_072),
            Err(BooleanSpectralError::TooManyVariables { variables: 17 })
        );
        let mut values = [1.0, f64::NAN, 1.0, -1.0];
        let original = values;
        assert_eq!(
            fwht_in_place(&mut values),
            Err(BooleanSpectralError::NonFiniteInput)
        );
        assert!(values[0] == original[0] && values[2] == original[2]);
        assert!(values[1].is_nan());
        let mut wrong_shape = [true, false, true];
        assert!(mobius_gf2_in_place(&mut wrong_shape).is_err());
        assert_eq!(wrong_shape, [true, false, true]);
        let mut too_wide = vec![false; 131_072];
        assert!(mobius_gf2_in_place(&mut too_wide).is_err());
        assert_eq!(
            evaluate_anf(&[false, true], 2),
            Err(BooleanSpectralError::MaskOutOfRange { mask: 2, len: 2 })
        );
        assert!(uniform_walsh_coefficient_direct(&[false, true], 2).is_err());
    }
}
