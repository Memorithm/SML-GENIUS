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

/// Exact arm used to compute one batch; every arm returns the same result.
///
/// `Selected` runs the policy architecture. `Scalar` forces one admitted scalar
/// arm and `Bitsliced` transposes up to 64 queries into `log2(BITS)` u64
/// bitplanes shared by every gate of the bank (GB-9). The returned selection
/// stays the policy label in all three cases; forcing an arm is a diagnostic
/// control, not a routing rule or a performance promotion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pvp6ExecutionPath {
    Selected,
    Scalar(Pvp6Architecture),
    Bitsliced,
}

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
    /// `queries` contains one prefix of `workload.query_count` addresses. The
    /// same prefix is evaluated `workload.reuse` times with unchanged gates;
    /// Pascal materialization happens once per gate, outside the reuse loop.
    /// Outputs are gate-major, with ceil(query_count * reuse / 64) words per
    /// gate. Query index `repeat * query_count + position` is least-bit first.
    /// The declared density must match every gate. Selection provenance labels
    /// policy coordinates, not evidence that these particular inputs were timed.
    pub fn evaluate(
        &self,
        workload: Pvp6Workload,
        queries: &[usize],
    ) -> Result<Pvp6BatchResult, Pvp6ExecutionError> {
        self.evaluate_on(Pvp6ExecutionPath::Selected, workload, queries)
    }

    /// Same contract and output as [`Self::evaluate`], computed by `path`.
    pub fn evaluate_on(
        &self,
        path: Pvp6ExecutionPath,
        workload: Pvp6Workload,
        queries: &[usize],
    ) -> Result<Pvp6BatchResult, Pvp6ExecutionError> {
        self.evaluate_checked(path, workload, queries, Pvp6DensityContract::Exact)
    }

    /// Opt-in bounded-density contract: `workload.density` is an upper bound,
    /// and every gate must have between 1 and that many monomials.
    ///
    /// Learned banks mix densities, so the exact contract would split one
    /// bank into several sub-banks, each with its own bit-slice transposition.
    /// Outputs are identical to evaluating each gate alone. The policy label
    /// is selected from the declared bound, so it describes coordinates rather
    /// than the bank's actual density mix.
    pub fn evaluate_bounded_on(
        &self,
        path: Pvp6ExecutionPath,
        workload: Pvp6Workload,
        queries: &[usize],
    ) -> Result<Pvp6BatchResult, Pvp6ExecutionError> {
        self.evaluate_checked(path, workload, queries, Pvp6DensityContract::AtMost)
    }

    fn evaluate_checked(
        &self,
        path: Pvp6ExecutionPath,
        workload: Pvp6Workload,
        queries: &[usize],
        contract: Pvp6DensityContract,
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

        for (gate_index, gate) in self.gates.iter().enumerate() {
            let actual = u32::try_from(gate.monomials.len()).ok();
            match contract {
                Pvp6DensityContract::Exact if actual != Some(workload.density) => {
                    return Err(Pvp6ExecutionError::DensityMismatch {
                        gate_index,
                        declared: workload.density,
                        actual: gate.monomials.len(),
                    });
                }
                Pvp6DensityContract::AtMost
                    if actual.is_none_or(|actual| actual > workload.density) =>
                {
                    return Err(Pvp6ExecutionError::DensityAboveBound {
                        gate_index,
                        bound: workload.density,
                        actual: gate.monomials.len(),
                    });
                }
                _ => {}
            }
        }
        let reuse = usize::try_from(workload.reuse).map_err(|_| Pvp6ExecutionError::Overflow)?;
        let query_count = queries
            .len()
            .checked_mul(reuse)
            .ok_or(Pvp6ExecutionError::Overflow)?;
        let words_per_gate = query_count.div_ceil(64);
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

        let architecture = match path {
            Pvp6ExecutionPath::Selected => selection.architecture,
            Pvp6ExecutionPath::Scalar(architecture) => architecture,
            Pvp6ExecutionPath::Bitsliced => {
                bitslice_into(
                    &self.gates,
                    queries,
                    query_count,
                    words_per_gate,
                    &mut packed,
                );
                return Ok(Pvp6BatchResult {
                    selection,
                    gate_count: self.gates.len(),
                    query_count,
                    words_per_gate,
                    packed,
                });
            }
        };
        for (gate_index, gate) in self.gates.iter().enumerate() {
            let base = gate_index * words_per_gate;
            let materialized = if architecture == Pvp6Architecture::PascalAnfMaterialize {
                let mut truth = zero_words(gate.coefficients.len())?;
                truth.copy_from_slice(&gate.coefficients);
                pascal_gf2_in_place(&mut truth).map_err(Pvp6ExecutionError::Pascal)?;
                Some(truth)
            } else {
                None
            };
            for repeat in 0..reuse {
                for (position, &query) in queries.iter().enumerate() {
                    let value = match architecture {
                        Pvp6Architecture::PascalAnfMaterialize => get_bit(
                            materialized.as_deref().expect("materialized Pascal arm"),
                            query,
                        ),
                        Pvp6Architecture::SparseAnfScan => sparse_eval(&gate.monomials, query),
                        Pvp6Architecture::SubmaskCoefficientQuery => {
                            submask_eval(&gate.coefficients, query)
                        }
                    };
                    if value {
                        set_bit(
                            &mut packed,
                            base,
                            words_per_gate,
                            repeat * queries.len() + position,
                        );
                    }
                }
            }
        }

        Ok(Pvp6BatchResult {
            selection,
            gate_count: self.gates.len(),
            query_count,
            words_per_gate,
            packed,
        })
    }
}

/// Exact packed outputs of one PVP6 batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pvp6BatchResult {
    selection: Pvp6Selection,
    gate_count: usize,
    query_count: usize,
    words_per_gate: usize,
    packed: Vec<u64>,
}

impl Pvp6BatchResult {
    #[must_use]
    pub fn selection(&self) -> Pvp6Selection {
        self.selection
    }

    #[must_use]
    pub fn gate_count(&self) -> usize {
        self.gate_count
    }

    /// Total queries, including all unchanged-prefix reuse rounds.
    #[must_use]
    pub fn query_count(&self) -> usize {
        self.query_count
    }

    /// Converts to query-major gate words without truncating banks above 64 gates.
    /// Each query owns ceil(gate_count / 64) words; unused high bits stay zero.
    /// With 16 gates this is the canonical PVP6 v1 task-output layout.
    pub fn to_query_major_words(&self) -> Result<Vec<u64>, Pvp6ExecutionError> {
        let words_per_query = self.gate_count.div_ceil(64);
        let len = self
            .query_count
            .checked_mul(words_per_query)
            .ok_or(Pvp6ExecutionError::Overflow)?;
        let mut output = zero_words(len)?;
        for gate_index in 0..self.gate_count {
            let row = &self.packed
                [gate_index * self.words_per_gate..(gate_index + 1) * self.words_per_gate];
            for (word_index, &word) in row.iter().enumerate() {
                let mut bits = word;
                while bits != 0 {
                    let query_index = word_index * 64 + bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    output[query_index * words_per_query + gate_index / 64] |=
                        1_u64 << (gate_index % 64);
                }
            }
        }
        Ok(output)
    }

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

fn zero_words(len: usize) -> Result<Vec<u64>, Pvp6ExecutionError> {
    let mut words = Vec::new();
    words
        .try_reserve_exact(len)
        .map_err(|_| Pvp6ExecutionError::Allocation)?;
    words.resize(len, 0);
    Ok(words)
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

/// Gate-major bit-sliced evaluation. Each chunk of 64 logical queries is
/// transposed once, then every gate reads the same bitplanes: a monomial is the
/// AND of its variable planes and a gate is the XOR of its monomials.
fn bitslice_into<const BITS: usize>(
    gates: &[Pvp6Gate<BITS>],
    queries: &[usize],
    query_count: usize,
    words_per_gate: usize,
    packed: &mut [u64],
) {
    let variables = BITS.trailing_zeros() as usize;
    let mut planes = [0_u64; usize::BITS as usize];
    for chunk in 0..words_per_gate {
        let start = chunk * 64;
        let lanes = (query_count - start).min(64);
        planes[..variables].fill(0);
        for lane in 0..lanes {
            let mut bits = queries[(start + lane) % queries.len()];
            while bits != 0 {
                planes[bits.trailing_zeros() as usize] |= 1_u64 << lane;
                bits &= bits - 1;
            }
        }
        let active = if lanes == 64 {
            u64::MAX
        } else {
            (1_u64 << lanes) - 1
        };
        for (gate_index, gate) in gates.iter().enumerate() {
            let mut output = 0_u64;
            for &mask in &gate.monomials {
                let mut product = active;
                let mut support = mask;
                while support != 0 {
                    product &= planes[support.trailing_zeros() as usize];
                    support &= support - 1;
                }
                output ^= product;
            }
            packed[gate_index * words_per_gate + chunk] = output;
        }
    }
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

/// How the declared workload density constrains each gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pvp6DensityContract {
    Exact,
    AtMost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pvp6ExecutionError {
    InvalidWidth {
        bits: usize,
    },
    EmptyMonomials,
    DuplicateMonomial(u16),
    MonomialOutOfRange {
        mask: u16,
        bits: usize,
    },
    EmptyBank,
    DensityMismatch {
        gate_index: usize,
        declared: u32,
        actual: usize,
    },
    DensityAboveBound {
        gate_index: usize,
        bound: u32,
        actual: usize,
    },
    Selection(Pvp6SelectionError),
    QueryCountMismatch {
        expected: usize,
        actual: usize,
    },
    QueryOutOfRange {
        query: usize,
        bits: usize,
    },
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
            Pvp6Gate::from_monomials(&[2, 5, 9, 11]).expect("gate"),
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
                    [2_u16, 5, 9, 11]
                        .iter()
                        .copied()
                        .fold(false, |value, mask| {
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
    fn bounded_density_mixed_bank_matches_per_gate_evaluation_on_every_arm() {
        let gate_sets: [&[u16]; 4] = [&[5], &[0, 1, 3, 7], &[2, 9], &[1, 6, 12, 33, 63]];
        let gates: Vec<Pvp6Gate<BITS>> = gate_sets
            .iter()
            .map(|m| Pvp6Gate::from_monomials(m).expect("gate"))
            .collect();
        let mixed = Pvp6Bank::new(gates.clone()).expect("bank");
        let qs: Vec<usize> = (0..65).map(|i| (i * 37 + 11) % BITS).collect();
        let bound = Pvp6Workload::new(5, 65, 2, Pvp6PopulationProfile::AffineUniform);
        let paths = [
            Pvp6ExecutionPath::Selected,
            Pvp6ExecutionPath::Scalar(Pvp6Architecture::PascalAnfMaterialize),
            Pvp6ExecutionPath::Scalar(Pvp6Architecture::SparseAnfScan),
            Pvp6ExecutionPath::Scalar(Pvp6Architecture::SubmaskCoefficientQuery),
            Pvp6ExecutionPath::Bitsliced,
        ];
        for path in paths {
            let result = mixed
                .evaluate_bounded_on(path, bound, &qs)
                .expect("bounded");
            assert_eq!(result.selection(), bound.select().expect("policy"));
            for (g, gate) in gates.iter().enumerate() {
                let alone = Pvp6Bank::new(vec![gate.clone()]).expect("bank");
                let exact = Pvp6Workload::new(
                    gate.monomials().len() as u32,
                    65,
                    2,
                    Pvp6PopulationProfile::AffineUniform,
                );
                let reference = alone.evaluate_on(path, exact, &qs).expect("exact");
                for q in 0..130 {
                    assert_eq!(
                        result.value(g, q),
                        reference.value(0, q),
                        "{path:?} {g} {q}"
                    );
                    let literal = gate.monomials().iter().fold(false, |v, &m| {
                        v ^ (usize::from(m) & qs[q % 65] == usize::from(m))
                    });
                    assert_eq!(result.value(g, q), Some(literal));
                }
            }
        }
        // The exact contract still rejects the same mixed bank.
        assert!(matches!(
            mixed.evaluate_on(Pvp6ExecutionPath::Bitsliced, bound, &qs),
            Err(Pvp6ExecutionError::DensityMismatch { gate_index: 0, .. })
        ));
        // A gate above the bound fails closed on every arm.
        let tight = Pvp6Workload::new(4, 65, 2, Pvp6PopulationProfile::AffineUniform);
        for path in paths {
            assert_eq!(
                mixed.evaluate_bounded_on(path, tight, &qs),
                Err(Pvp6ExecutionError::DensityAboveBound {
                    gate_index: 3,
                    bound: 4,
                    actual: 5
                })
            );
        }
        let zero = Pvp6Workload::new(0, 65, 2, Pvp6PopulationProfile::AffineUniform);
        assert_eq!(
            mixed.evaluate_bounded_on(Pvp6ExecutionPath::Bitsliced, zero, &qs),
            Err(Pvp6ExecutionError::Selection(
                Pvp6SelectionError::ZeroDensity
            ))
        );
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

    // Deliberately evaluates each input literal, without using subset masks,
    // coefficient lookup, sparse_eval, submask_eval or the Pascal transform.
    fn literal_oracle(monomials: &[u16], query: usize) -> bool {
        monomials.iter().fold(false, |parity, &mask| {
            let product = (0..u16::BITS)
                .all(|bit| (mask & (1_u16 << bit)) == 0 || (query & (1_usize << bit)) != 0);
            parity ^ product
        })
    }

    fn qualify_width<const WIDTH: usize>() {
        let monomials = [0, 1, (WIDTH / 2) as u16, (WIDTH - 1) as u16];
        let bank = Pvp6Bank::<WIDTH>::new(vec![
            Pvp6Gate::from_monomials(&monomials).unwrap(),
            Pvp6Gate::from_monomials(&[2, 5, 9, 11]).unwrap(),
        ])
        .unwrap();
        for count in [1, 31, 32, 63, 64, 65, 129] {
            let queries = (0..count).map(|i| (i * 37 + 3) % WIDTH).collect::<Vec<_>>();
            for reuse in [1, 4] {
                for profile in [
                    Pvp6PopulationProfile::AffineUniform,
                    Pvp6PopulationProfile::LowPopcount,
                    Pvp6PopulationProfile::HighPopcount,
                ] {
                    let workload = Pvp6Workload::new(4, count as u32, reuse, profile);
                    let before = bank.clone();
                    let result = bank.evaluate(workload, &queries).unwrap();
                    let total = count * reuse as usize;
                    assert_eq!(result.query_count(), total);
                    assert_eq!(result.gate_count(), 2);
                    assert_eq!(result.selection(), workload.select().unwrap());
                    let canonical = result.to_query_major_words().unwrap();
                    assert_eq!(canonical.len(), total);
                    let mut expected_packed = vec![0_u64; 2 * total.div_ceil(64)];
                    for (i, &word) in canonical.iter().enumerate() {
                        let query = queries[i % count];
                        let mut expected_word = 0;
                        for (gate, terms) in
                            [monomials.as_slice(), &[2, 5, 9, 11]].iter().enumerate()
                        {
                            let value = literal_oracle(terms, query);
                            assert_eq!(result.value(gate, i), Some(value));
                            if value {
                                expected_word |= 1_u64 << gate;
                                expected_packed[gate * total.div_ceil(64) + i / 64] |=
                                    1_u64 << (i % 64);
                            }
                        }
                        assert_eq!(word, expected_word);
                    }
                    assert_eq!(result.packed_words(), expected_packed);
                    assert_eq!(result.value(2, 0), None);
                    assert_eq!(result.value(0, total), None);
                    assert_eq!(result.value(usize::MAX, usize::MAX), None);
                    assert_eq!(bank, before);
                }
            }
        }
    }

    #[test]
    fn width_ladder_reuse_and_partial_words_match_literal_oracle() {
        qualify_width::<64>();
        qualify_width::<128>();
        qualify_width::<256>();
        qualify_width::<512>();
        qualify_width::<1024>();
        qualify_width::<2048>();
    }

    const ALL_PATHS: [Pvp6ExecutionPath; 5] = [
        Pvp6ExecutionPath::Selected,
        Pvp6ExecutionPath::Scalar(Pvp6Architecture::PascalAnfMaterialize),
        Pvp6ExecutionPath::Scalar(Pvp6Architecture::SparseAnfScan),
        Pvp6ExecutionPath::Scalar(Pvp6Architecture::SubmaskCoefficientQuery),
        Pvp6ExecutionPath::Bitsliced,
    ];

    fn lcg(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *state >> 11
    }

    fn random_bank<const WIDTH: usize>(seed: u64, gates: usize, density: usize) -> Pvp6Bank<WIDTH> {
        let mut state = seed;
        let gates = (0..gates)
            .map(|_| {
                let mut terms = Vec::new();
                while terms.len() < density {
                    let mask = (lcg(&mut state) % WIDTH as u64) as u16;
                    if !terms.contains(&mask) {
                        terms.push(mask);
                    }
                }
                Pvp6Gate::from_monomials(&terms).unwrap()
            })
            .collect();
        Pvp6Bank::new(gates).unwrap()
    }

    fn qualify_paths<const WIDTH: usize>() {
        let mut seed = WIDTH as u64;
        for gates in [1, 3, 64, 65] {
            for density in [1, 2, 7, WIDTH / 2] {
                let bank = random_bank::<WIDTH>(lcg(&mut seed), gates, density);
                for count in [1, 31, 63, 64, 65, 129] {
                    let queries = (0..count)
                        .map(|_| (lcg(&mut seed) % WIDTH as u64) as usize)
                        .collect::<Vec<_>>();
                    for reuse in [1, 3] {
                        let workload = Pvp6Workload::new(
                            density as u32,
                            count as u32,
                            reuse,
                            Pvp6PopulationProfile::AffineUniform,
                        );
                        let reference = bank.evaluate(workload, &queries).unwrap();
                        for path in ALL_PATHS {
                            assert_eq!(
                                bank.evaluate_on(path, workload, &queries).unwrap(),
                                reference,
                                "{path:?} WIDTH={WIDTH} gates={gates} density={density} count={count} reuse={reuse}"
                            );
                        }
                        for gate in 0..gates.min(3) {
                            for i in 0..reference.query_count() {
                                assert_eq!(
                                    reference.value(gate, i),
                                    Some(literal_oracle(
                                        bank.gates()[gate].monomials(),
                                        queries[i % count]
                                    ))
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn bitsliced_and_forced_scalar_arms_return_the_selected_result_exactly() {
        qualify_paths::<64>();
        qualify_paths::<128>();
        qualify_paths::<256>();
        qualify_paths::<1024>();
        qualify_paths::<4096>();
    }

    #[test]
    fn bitsliced_path_keeps_inactive_lanes_zero_for_constant_gates() {
        let bank = Pvp6Bank::<64>::new(vec![Pvp6Gate::from_monomials(&[0]).unwrap(); 2]).unwrap();
        let workload = Pvp6Workload::new(1, 5, 1, Pvp6PopulationProfile::AffineUniform);
        let result = bank
            .evaluate_on(Pvp6ExecutionPath::Bitsliced, workload, &[0, 1, 2, 3, 4])
            .unwrap();
        assert_eq!(result.packed_words(), &[0b1_1111, 0b1_1111]);
        assert_eq!(
            bank.evaluate_on(Pvp6ExecutionPath::Bitsliced, workload, &[0, 1, 2, 3]),
            Err(Pvp6ExecutionError::QueryCountMismatch {
                expected: 5,
                actual: 4
            })
        );
    }

    #[test]
    fn query_major_projection_keeps_gate_64_and_zero_padding() {
        let gate = Pvp6Gate::<128>::from_monomials(&[0]).unwrap();
        let bank = Pvp6Bank::new(vec![gate; 65]).unwrap();
        let result = bank
            .evaluate(
                Pvp6Workload::new(1, 65, 4, Pvp6PopulationProfile::AffineUniform),
                &(0..65).collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(result.query_count(), 260);
        assert_eq!(result.packed_words().len(), 65 * 5);
        for gate_words in result.packed_words().chunks_exact(5) {
            assert_eq!(gate_words, &[u64::MAX, u64::MAX, u64::MAX, u64::MAX, 15]);
        }
        let canonical = result.to_query_major_words().unwrap();
        assert_eq!(canonical.len(), 520);
        for query_words in canonical.chunks_exact(2) {
            assert_eq!(query_words, &[u64::MAX, 1]);
        }
    }

    #[test]
    fn false_density_and_empty_or_invalid_inputs_fail_closed() {
        assert_eq!(
            Pvp6Gate::<0>::from_monomials(&[0]),
            Err(Pvp6ExecutionError::InvalidWidth { bits: 0 })
        );
        assert_eq!(
            Pvp6Gate::<32>::from_monomials(&[0]),
            Err(Pvp6ExecutionError::InvalidWidth { bits: 32 })
        );
        assert_eq!(
            Pvp6Gate::<96>::from_monomials(&[0]),
            Err(Pvp6ExecutionError::InvalidWidth { bits: 96 })
        );
        assert_eq!(
            Pvp6Gate::<64>::from_monomials(&[]),
            Err(Pvp6ExecutionError::EmptyMonomials)
        );
        assert_eq!(
            Pvp6Bank::<64>::new(vec![]),
            Err(Pvp6ExecutionError::EmptyBank)
        );
        let mismatched = Pvp6Bank::<64>::new(vec![
            Pvp6Gate::from_monomials(&[0, 1, 3, 7]).unwrap(),
            Pvp6Gate::from_monomials(&[2, 5, 9]).unwrap(),
        ])
        .unwrap();
        assert_eq!(
            mismatched.evaluate(
                Pvp6Workload::new(4, 32, 1, Pvp6PopulationProfile::AffineUniform),
                &queries(32)
            ),
            Err(Pvp6ExecutionError::DensityMismatch {
                gate_index: 1,
                declared: 4,
                actual: 3
            })
        );
        for (workload, error) in [
            (
                Pvp6Workload::new(0, 32, 1, Pvp6PopulationProfile::AffineUniform),
                Pvp6SelectionError::ZeroDensity,
            ),
            (
                Pvp6Workload::new(4, 0, 1, Pvp6PopulationProfile::AffineUniform),
                Pvp6SelectionError::ZeroQueryCount,
            ),
            (
                Pvp6Workload::new(4, 32, 0, Pvp6PopulationProfile::AffineUniform),
                Pvp6SelectionError::ZeroReuse,
            ),
        ] {
            assert_eq!(
                bank().evaluate(workload, &queries(32)),
                Err(Pvp6ExecutionError::Selection(error))
            );
        }
    }

    #[test]
    fn coefficient_replacement_cannot_reuse_stale_materialization() {
        let make_bank = |terms: &[u16]| {
            Pvp6Bank::<64>::new(vec![Pvp6Gate::from_monomials(terms).unwrap()]).unwrap()
        };
        let old = make_bank(&[0, 1, 3, 7]);
        let new = make_bank(&[1, 2, 3, 7]);
        let workload = Pvp6Workload::new(4, 64, 4, Pvp6PopulationProfile::AffineUniform);
        let old_result = old.evaluate(workload, &queries(64)).unwrap();
        let new_result = new.evaluate(workload, &queries(64)).unwrap();
        assert_ne!(old_result.packed_words(), new_result.packed_words());
        for i in 0..256 {
            assert_eq!(
                new_result.value(0, i),
                Some(literal_oracle(&[1, 2, 3, 7], i % 64))
            );
        }
        assert_eq!(old.evaluate(workload, &queries(64)).unwrap(), old_result);
    }
}
