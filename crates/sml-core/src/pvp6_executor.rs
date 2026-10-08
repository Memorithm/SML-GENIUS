//! Model-owned PVP6 execution over packed ANF gate banks.
//!
//! This module internalizes the minimum exact execution contract needed by the
//! SML model. It deliberately contains no SciRust, FLAT-ATTENTION, NNIS, CUDA,
//! or vendor-runtime dependency. The selector remains diagnostic-only; this
//! executor does not promote a production performance claim.

use core::fmt;

use crate::{
    pascal_gf2::{pascal_gf2_in_place, PascalGf2Error},
    pascal_vector_projection::{Pvp6Architecture, Pvp6Selection, Pvp6SelectionError, Pvp6Workload},
};

/// One canonical ANF gate for a packed Boolean domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pvp6Gate<const BITS: usize> {
    coefficients: Vec<u64>,
    monomials: Vec<u16>,
}

impl<const BITS: usize> Pvp6Gate<BITS> {
    /// Builds a gate from the non-zero ANF monomial masks.
    ///
    /// The constructor sorts and validates the masks, then materializes the
    /// coefficient bitplane once. Duplicate monomials are rejected instead of
    /// being silently cancelled.
    pub fn from_monomials(monomials: &[u16]) -> Result<Self, Pvp6ExecutionError> {
        validate_width::<BITS>()?;
        if monomials.is_empty() {
            return Err(Pvp6ExecutionError::EmptyMonomials);
        }

        let mut sorted = monomials.to_vec();
        sorted.sort_unstable();
        for pair in sorted.windows(2) {
            if pair[0] == pair[1] {
                return Err(Pvp6ExecutionError::DuplicateMonomial(pair[0]));
            }
        }

        let mut coefficients = vec![0_u64; BITS / 64];
        for mask in sorted.iter().copied() {
            let index = usize::from(mask);
            if index >= BITS {
                return Err(Pvp6ExecutionError::MonomialOutOfRange { mask, bits: BITS });
            }
            coefficients[index / 64] |= 1_u64 << (index % 64);
        }

        Ok(Self {
            coefficients,
            monomials: sorted,
        })
    }

    #[must_use]
    pub fn coefficients(&self) -> &[u64] {
        &self.coefficients
    }

    #[must_use]
    pub fn monomials(&self) -> &[u16] {
        &self.monomials
    }
}

/// A non-empty model-owned bank of ANF gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pvp6Bank<const BITS: usize> {
    gates: Vec<Pvp6Gate<BITS>>,
}

impl<const BITS: usize> Pvp6Bank<BITS> {
    pub fn new(gates: Vec<Pvp6Gate<BITS>>) -> Result<Self, Pvp6ExecutionError> {
        validate_width::<BITS>()?;
        if gates.is_empty() {
            return Err(Pvp6ExecutionError::EmptyBank);
        }
        Ok(Self { gates })
    }

    #[must_use]
    pub fn gates(&self) -> &[Pvp6Gate<BITS>] {
        &self.gates
    }

    /// Evaluates all gates for the declared query workload.
    ///
    /// Outputs are packed gate-major: each gate owns ceil(query_count / 64)
    /// words, with query zero in the least-significant bit. The selected arm is
    /// returned with the output so callers cannot lose the provenance decision.
    pub fn evaluate(
        &self,
        workload: Pvp6Workload,
        queries: &[usize],
    ) -> Result<Pvp6BatchResult, Pvp6ExecutionError> {
        let selection = workload.select().map_err(Pvp6ExecutionError::Selection)?;
        let expected_queries =
            usize::try_from(workload.query_count).map_err(|_| Pvp6ExecutionError::Overflow)?;
        if expected_queries != queries.len() {
            return Err(Pvp6ExecutionError::QueryCountMismatch {
                expected: expected_queries,
                actual: queries.len(),
            });
        }
        for &query in queries {
            if query >= BITS {
                return Err(Pvp6ExecutionError::QueryOutOfRange { query, bits: BITS });
            }
        }

        let words_per_gate = queries.len().div_ceil(64);
        let output_words = self
            .gates
            .len()
            .checked_mul(words_per_gate)
            .ok_or(Pvp6ExecutionError::Overflow)?;
        let mut packed = Vec::new();
        packed
            .try_reserve_exact(output_words)
            .map_err(|_| Pvp6ExecutionError::Allocation)?;
        packed.resize(output_words, 0);

        for (gate_index, gate) in self.gates.iter().enumerate() {
            let base = gate_index * words_per_gate;
            match selection.architecture {
                Pvp6Architecture::PascalAnfMaterialize => {
                    let mut truth = gate.coefficients.clone();
                    pascal_gf2_in_place(&mut truth).map_err(Pvp6ExecutionError::Pascal)?;
                    for (query_index, &query) in queries.iter().enumerate() {
                        if get_bit(&truth, query) {
                            set_bit(&mut packed, base, words_per_gate, query_index);
                        }
                    }
                }
                Pvp6Architecture::SparseAnfScan => {
                    for (query_index, &query) in queries.iter().enumerate() {
                        if sparse_eval(&gate.monomials, query) {
                            set_bit(&mut packed, base, words_per_gate, query_index);
                        }
                    }
                }
                Pvp6Architecture::SubmaskCoefficientQuery => {
                    for (query_index, &query) in queries.iter().enumerate() {
                        if submask_eval(&gate.coefficients, query) {
                            set_bit(&mut packed, base, words_per_gate, query_index);
                        }
                    }
                }
            }
        }

        Ok(Pvp6BatchResult {
            selection,
            gate_count: self.gates.len(),
            query_count: queries.len(),
            words_per_gate,
            packed,
        })
    }
}

/// Exact packed outputs of one PVP6 batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pvp6BatchResult {
    pub selection: Pvp6Selection,
    pub gate_count: usize,
    pub query_count: usize,
    words_per_gate: usize,
    packed: Vec<u64>,
}

impl Pvp6BatchResult {
    #[must_use]
    pub fn value(&self, gate_index: usize, query_index: usize) -> Option<bool> {
        if gate_index >= self.gate_count || query_index >= self.query_count {
            return None;
        }
        Some(
            self.packed[gate_index * self.words_per_gate + query_index / 64]
                & (1_u64 << (query_index % 64))
                != 0,
        )
    }

    #[must_use]
    pub fn packed_words(&self) -> &[u64] {
        &self.packed
    }
}

fn validate_width<const BITS: usize>() -> Result<(), Pvp6ExecutionError> {
    if BITS < 64 || !BITS.is_power_of_two() {
        return Err(Pvp6ExecutionError::InvalidWidth { bits: BITS });
    }
    Ok(())
}

fn get_bit(words: &[u64], bit: usize) -> bool {
    words[bit / 64] & (1_u64 << (bit % 64)) != 0
}

fn set_bit(words: &mut [u64], base: usize, words_per_gate: usize, bit: usize) {
    words[base + bit / 64] |= 1_u64 << (bit % 64);
    debug_assert!(base / words_per_gate < words.len() / words_per_gate);
}

fn sparse_eval(monomials: &[u16], query: usize) -> bool {
    monomials.iter().copied().fold(false, |value, mask| {
        value ^ (usize::from(mask) & query == usize::from(mask))
    })
}

fn submask_eval(coefficients: &[u64], query: usize) -> bool {
    let mut value = false;
    let mut submask = query;
    loop {
        value ^= get_bit(coefficients, submask);
        if submask == 0 {
            break;
        }
        submask = (submask - 1) & query;
    }
    value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pvp6ExecutionError {
    InvalidWidth { bits: usize },
    EmptyMonomials,
    DuplicateMonomial(u16),
    MonomialOutOfRange { mask: u16, bits: usize },
    EmptyBank,
    Selection(Pvp6SelectionError),
    QueryCountMismatch { expected: usize, actual: usize },
    QueryOutOfRange { query: usize, bits: usize },
    Overflow,
    Allocation,
    Pascal(PascalGf2Error),
}

impl From<PascalGf2Error> for Pvp6ExecutionError {
    fn from(error: PascalGf2Error) -> Self {
        Self::Pascal(error)
    }
}

impl fmt::Display for Pvp6ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Pvp6ExecutionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pascal_vector_projection::Pvp6PopulationProfile;

    const BITS: usize = 64;
    const MONOMIALS: [u16; 4] = [0, 1, 3, 7];

    fn oracle(query: usize) -> bool {
        MONOMIALS.iter().copied().fold(false, |value, mask| {
            value ^ (usize::from(mask) & query == usize::from(mask))
        })
    }

    fn bank() -> Pvp6Bank<BITS> {
        Pvp6Bank::new(vec![
            Pvp6Gate::from_monomials(&MONOMIALS).expect("gate"),
            Pvp6Gate::from_monomials(&[2, 5, 9]).expect("gate"),
        ])
        .expect("bank")
    }

    fn queries(count: usize) -> Vec<usize> {
        (0..count).collect()
    }

    fn assert_exact(profile: Pvp6PopulationProfile, query_count: u32) {
        let result = bank()
            .evaluate(
                Pvp6Workload::new(4, query_count, 1, profile),
                &queries(query_count as usize),
            )
            .expect("exact PVP6 evaluation");

        for gate in 0..result.gate_count {
            for query in 0..result.query_count {
                let expected = if gate == 0 {
                    oracle(query)
                } else {
                    [2_u16, 5, 9].iter().copied().fold(false, |value, mask| {
                        value ^ (usize::from(mask) & query == usize::from(mask))
                    })
                };
                assert_eq!(result.value(gate, query), Some(expected));
            }
        }
        assert!(result.selection.diagnostic_only);
    }

    #[test]
    fn every_admitted_arm_matches_the_independent_oracle() {
        assert_exact(Pvp6PopulationProfile::AffineUniform, 32);
        assert_exact(Pvp6PopulationProfile::LowPopcount, 32);
        assert_exact(Pvp6PopulationProfile::HighPopcount, 32);
        assert_exact(Pvp6PopulationProfile::AffineUniform, 64);
    }

    #[test]
    fn malformed_gates_and_batches_fail_closed() {
        assert_eq!(
            Pvp6Gate::<BITS>::from_monomials(&[1, 1]),
            Err(Pvp6ExecutionError::DuplicateMonomial(1))
        );
        assert_eq!(
            Pvp6Gate::<BITS>::from_monomials(&[64]),
            Err(Pvp6ExecutionError::MonomialOutOfRange {
                mask: 64,
                bits: BITS
            })
        );
        assert_eq!(
            bank().evaluate(
                Pvp6Workload::new(4, 31, 1, Pvp6PopulationProfile::AffineUniform),
                &queries(32),
            ),
            Err(Pvp6ExecutionError::QueryCountMismatch {
                expected: 31,
                actual: 32
            })
        );
        assert_eq!(
            bank().evaluate(
                Pvp6Workload::new(4, 1, 1, Pvp6PopulationProfile::AffineUniform),
                &[64],
            ),
            Err(Pvp6ExecutionError::QueryOutOfRange {
                query: 64,
                bits: BITS
            })
        );
    }

    #[test]
    fn output_is_gate_major_and_packed() {
        let result = bank()
            .evaluate(
                Pvp6Workload::new(4, 64, 1, Pvp6PopulationProfile::AffineUniform),
                &queries(64),
            )
            .expect("evaluation");

        assert_eq!(result.words_per_gate, 1);
        assert_eq!(result.packed_words().len(), 2);
        assert_eq!(result.value(0, 0), Some(true));
        // At x=1, the constant and x0 monomials both fire and cancel in GF(2).
        assert_eq!(result.value(0, 1), Some(false));
        assert_eq!(result.value(1, 0), Some(false));
    }
}
