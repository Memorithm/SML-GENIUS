//! A6-E1: support-scoped input coverage, full-16-bit Hamming novelty and
//! degree-conditional GF(2) design rank. None establishes model correctness.
//!
//! Support comes from the *supplied/learned candidate bank*, never from a
//! hidden teacher. Rank only certifies identifiability CONDITIONAL ON both an
//! exactly fitting noiseless label oracle and correct grammar/support priors.

use crate::anf_wide_sparse::WideAnfGate;
use std::collections::VecDeque;
use std::mem::size_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedRiskError {
    Width,
    EmptySupport,
    DuplicateSupport,
    Budget,
    Allocation,
    EmptyTraining,
    Sealed,
    NotSealed,
}

impl std::fmt::Display for ScopedRiskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ScopedRiskError {}

/// Derive a real *candidate* support union, not a true-teacher or TEST support.
pub fn competing_support(
    gates: &[WideAnfGate],
    max_support: usize,
) -> Result<Vec<usize>, ScopedRiskError> {
    if gates.is_empty() || max_support == 0 {
        return Err(ScopedRiskError::EmptySupport);
    }
    let mut positions = Vec::new();
    for gate in gates {
        for &index in gate.input_positions() {
            if !positions.contains(&index) {
                if positions.len() == max_support {
                    return Err(ScopedRiskError::Budget);
                }
                positions
                    .try_reserve(1)
                    .map_err(|_| ScopedRiskError::Allocation)?;
                positions.push(index);
            }
        }
    }
    positions.sort_unstable();
    if positions.is_empty() {
        return Err(ScopedRiskError::EmptySupport);
    }
    Ok(positions)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScopeTuple {
    indexes: [usize; 3],
    seen: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeNovelty {
    pub missing_patterns: usize,
    pub completely_covered: bool,
}

/// Full strength-two/three coverage *within a frozen candidate union*.
/// Cost depends on C(s,t), not C(global_input_width,t), with s=|support|.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopedPatternGuard {
    width: usize,
    support: Vec<usize>,
    strength: usize,
    max_samples: usize,
    samples: usize,
    tuples: Vec<ScopeTuple>,
    sealed: bool,
}

impl ScopedPatternGuard {
    pub fn new(
        width: usize,
        active: &[usize],
        strength: usize,
        max_samples: usize,
        max_tuples: usize,
    ) -> Result<Self, ScopedRiskError> {
        if width == 0 {
            return Err(ScopedRiskError::Width);
        }
        if active.is_empty() {
            return Err(ScopedRiskError::EmptySupport);
        }
        if active.len() > 16
            || !(2..=3).contains(&strength)
            || active.len() < strength
            || max_samples == 0
            || max_tuples == 0
        {
            return Err(ScopedRiskError::Budget);
        }
        let mut support = active.to_vec();
        support.sort_unstable();
        if support.iter().any(|&position| position >= width) {
            return Err(ScopedRiskError::Width);
        }
        if support.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ScopedRiskError::DuplicateSupport);
        }
        let mut tuples = Vec::new();
        for a in 0..support.len() {
            for b in a + 1..support.len() {
                if strength == 2 {
                    if tuples.len() >= max_tuples {
                        return Err(ScopedRiskError::Budget);
                    }
                    tuples
                        .try_reserve(1)
                        .map_err(|_| ScopedRiskError::Allocation)?;
                    tuples.push(ScopeTuple {
                        indexes: [support[a], support[b], 0],
                        seen: 0,
                    });
                } else {
                    for c in b + 1..support.len() {
                        if tuples.len() >= max_tuples {
                            return Err(ScopedRiskError::Budget);
                        }
                        tuples
                            .try_reserve(1)
                            .map_err(|_| ScopedRiskError::Allocation)?;
                        tuples.push(ScopeTuple {
                            indexes: [support[a], support[b], support[c]],
                            seen: 0,
                        });
                    }
                }
            }
        }
        Ok(Self {
            width,
            support,
            strength,
            max_samples,
            samples: 0,
            tuples,
            sealed: false,
        })
    }

    #[must_use]
    pub fn support(&self) -> &[usize] {
        &self.support
    }

    #[must_use]
    pub fn tuple_count(&self) -> usize {
        self.tuples.len()
    }

    #[must_use]
    pub fn samples(&self) -> usize {
        self.samples
    }

    /// Only primitive support and tuple vector payload; excludes allocator.
    #[must_use]
    pub fn logical_payload_bytes(&self) -> usize {
        self.support.len() * size_of::<usize>() + self.tuples.len() * size_of::<ScopeTuple>()
    }

    fn pattern(input: &[bool], tuple: &ScopeTuple, strength: usize) -> usize {
        (0..strength).fold(0usize, |code, i| {
            (code << 1) | usize::from(input[tuple.indexes[i]])
        })
    }

    pub fn observe(&mut self, input: &[bool]) -> Result<(), ScopedRiskError> {
        if input.len() != self.width {
            return Err(ScopedRiskError::Width);
        }
        if self.sealed {
            return Err(ScopedRiskError::Sealed);
        }
        if self.samples >= self.max_samples {
            return Err(ScopedRiskError::Budget);
        }
        for tuple in &mut self.tuples {
            tuple.seen |= 1u8 << Self::pattern(input, tuple, self.strength);
        }
        self.samples += 1;
        Ok(())
    }

    pub fn seal(&mut self) -> Result<(), ScopedRiskError> {
        if self.sealed {
            return Err(ScopedRiskError::Sealed);
        }
        if self.samples == 0 {
            return Err(ScopedRiskError::EmptyTraining);
        }
        self.sealed = true;
        Ok(())
    }

    pub fn assess(&self, input: &[bool]) -> Result<ScopeNovelty, ScopedRiskError> {
        if input.len() != self.width {
            return Err(ScopedRiskError::Width);
        }
        if !self.sealed {
            return Err(ScopedRiskError::NotSealed);
        }
        let missing_patterns = self
            .tuples
            .iter()
            .filter(|tuple| {
                tuple.seen & (1u8 << Self::pattern(input, tuple, self.strength)) == 0
            })
            .count();
        Ok(ScopeNovelty {
            missing_patterns,
            completely_covered: missing_patterns == 0,
        })
    }
}

/// Exact nearest Hamming distance to any TRAIN input with width exactly 16.
/// Full 2^16 Boolean hypercube BFS makes TEST lookups O(1) and prevents a
/// hidden O(samples * width) per-token scan. Not a generic wide vector index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hamming16Guard {
    seen: Vec<bool>,
    nearest: Option<Vec<u8>>,
    max_samples: usize,
    samples: usize,
}

impl Hamming16Guard {
    pub fn new(max_samples: usize) -> Result<Self, ScopedRiskError> {
        if max_samples == 0 {
            return Err(ScopedRiskError::Budget);
        }
        let mut seen = Vec::new();
        seen.try_reserve_exact(1 << 16)
            .map_err(|_| ScopedRiskError::Allocation)?;
        seen.resize(1 << 16, false);
        Ok(Self {
            seen,
            nearest: None,
            max_samples,
            samples: 0,
        })
    }

    pub fn packed(input: &[bool]) -> Result<usize, ScopedRiskError> {
        if input.len() != 16 {
            return Err(ScopedRiskError::Width);
        }
        Ok(input
            .iter()
            .enumerate()
            .fold(0usize, |acc, (i, bit)| acc | (usize::from(*bit) << i)))
    }

    pub fn observe(&mut self, input: &[bool]) -> Result<(), ScopedRiskError> {
        let pattern = Self::packed(input)?;
        if self.nearest.is_some() {
            return Err(ScopedRiskError::Sealed);
        }
        if self.samples >= self.max_samples {
            return Err(ScopedRiskError::Budget);
        }
        self.seen[pattern] = true;
        self.samples += 1;
        Ok(())
    }

    pub fn seal(&mut self) -> Result<(), ScopedRiskError> {
        if self.nearest.is_some() {
            return Err(ScopedRiskError::Sealed);
        }
        if self.samples == 0 {
            return Err(ScopedRiskError::EmptyTraining);
        }
        let mut nearest = Vec::new();
        nearest
            .try_reserve_exact(1 << 16)
            .map_err(|_| ScopedRiskError::Allocation)?;
        nearest.resize(1 << 16, u8::MAX);
        let mut queue = VecDeque::new();
        queue
            .try_reserve(1 << 16)
            .map_err(|_| ScopedRiskError::Allocation)?;
        for (i, &visited) in self.seen.iter().enumerate() {
            if visited {
                nearest[i] = 0;
                queue.push_back(i);
            }
        }
        while let Some(vertex) = queue.pop_front() {
            let next_distance = nearest[vertex] + 1;
            for bit in 0..16usize {
                let neighbor = vertex ^ (1 << bit);
                if next_distance < nearest[neighbor] {
                    nearest[neighbor] = next_distance;
                    queue.push_back(neighbor);
                }
            }
        }
        self.nearest = Some(nearest);
        Ok(())
    }

    pub fn distance(&self, input: &[bool]) -> Result<u8, ScopedRiskError> {
        let pattern = Self::packed(input)?;
        Ok(*self
            .nearest
            .as_ref()
            .ok_or(ScopedRiskError::NotSealed)?
            .get(pattern)
            .ok_or(ScopedRiskError::Width)?)
    }

    #[must_use]
    pub fn samples(&self) -> usize {
        self.samples
    }

    #[must_use]
    pub fn logical_payload_bytes(&self) -> usize {
        self.seen.len() * size_of::<bool>()
            + self.nearest.as_ref().map_or(0, |values| values.len() * size_of::<u8>())
    }
}

/// GF(2) evaluation-matrix rank for square-free ANF monomials of degree <=d.
/// Full rank is **not** a teacher membership or no-noise proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DegreeDesignRank {
    width: usize,
    support: Vec<usize>,
    terms: Vec<u16>,
    pivots: [u64; 64],
    rank: usize,
    samples: usize,
}

impl DegreeDesignRank {
    pub fn new(
        width: usize,
        support: &[usize],
        max_degree: usize,
    ) -> Result<Self, ScopedRiskError> {
        if width == 0 || support.is_empty() || support.len() > 16 {
            return Err(ScopedRiskError::Width);
        }
        if !(1..=2).contains(&max_degree) {
            return Err(ScopedRiskError::Budget);
        }
        let mut ordered = support.to_vec();
        ordered.sort_unstable();
        if ordered.iter().any(|&x| x >= width) {
            return Err(ScopedRiskError::Width);
        }
        if ordered.windows(2).any(|x| x[0] == x[1]) {
            return Err(ScopedRiskError::DuplicateSupport);
        }
        let mut terms = Vec::new();
        for bits in 0u32..(1u32 << ordered.len()) {
            if bits.count_ones() as usize <= max_degree {
                if terms.len() == 64 {
                    return Err(ScopedRiskError::Budget);
                }
                terms.push(bits as u16);
            }
        }
        Ok(Self {
            width,
            support: ordered,
            terms,
            pivots: [0u64; 64],
            rank: 0,
            samples: 0,
        })
    }

    pub fn observe(&mut self, input: &[bool]) -> Result<(), ScopedRiskError> {
        if input.len() != self.width {
            return Err(ScopedRiskError::Width);
        }
        let active = self.support.iter().enumerate().fold(0u16, |v, (i, &p)| {
            v | (u16::from(input[p]) << i)
        });
        let mut row = self.terms.iter().enumerate().fold(0u64, |v, (i, &term)| {
            if active & term == term {
                v | (1u64 << i)
            } else {
                v
            }
        });
        while row != 0 {
            let pivot = 63 - row.leading_zeros() as usize;
            if self.pivots[pivot] != 0 {
                row ^= self.pivots[pivot];
            } else {
                self.pivots[pivot] = row;
                self.rank += 1;
                break;
            }
        }
        self.samples += 1;
        Ok(())
    }

    #[must_use]
    pub fn rank(&self) -> usize {
        self.rank
    }

    #[must_use]
    pub fn monomial_count(&self) -> usize {
        self.terms.len()
    }

    #[must_use]
    pub fn full_rank(&self) -> bool {
        self.rank == self.terms.len()
    }

    #[must_use]
    pub fn samples(&self) -> usize {
        self.samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anf_wide_sparse::WideAnfLimits;

    fn vec_from_code(width: usize, value: usize) -> Vec<bool> {
        (0..width).map(|i| value & (1 << i) != 0).collect()
    }

    #[test]
    fn all_pair_patterns_can_be_covered_but_linear_function_unidentifiable() {
        let mut pairs = ScopedPatternGuard::new(3, &[0, 1, 2], 2, 4, 10).unwrap();
        let mut rank = DegreeDesignRank::new(3, &[0, 1, 2], 1).unwrap();
        for x in [0usize, 3, 5, 6] {
            let row = vec_from_code(3, x);
            pairs.observe(&row).unwrap();
            rank.observe(&row).unwrap();
            let f = false;
            let g = row.iter().fold(false, |a, &b| a ^ b);
            assert_eq!(f, g, "even parity training points indistinguishable");
        }
        pairs.seal().unwrap();
        assert_eq!(pairs.assess(&vec_from_code(3, 1)).unwrap().missing_patterns, 0);
        assert_eq!(rank.monomial_count(), 4);
        assert!(rank.rank() < rank.monomial_count());
        assert!(vec_from_code(3, 1).iter().fold(false, |a, &b| a ^ b));
    }

    #[test]
    fn scoped_triples_catch_hidden_triple_not_visible_to_scoped_pairs() {
        let mut pair = ScopedPatternGuard::new(8, &[5, 6, 7], 2, 7, 10).unwrap();
        let mut triple = ScopedPatternGuard::new(8, &[5, 6, 7], 3, 7, 10).unwrap();
        for code in 0..7 {
            let mut input = vec![false; 8];
            for bit in 0..3 {
                input[5 + bit] = code & (1 << bit) != 0;
            }
            pair.observe(&input).unwrap();
            triple.observe(&input).unwrap();
        }
        pair.seal().unwrap();
        triple.seal().unwrap();
        let mut novel = vec![false; 8];
        novel[5] = true;
        novel[6] = true;
        novel[7] = true;
        assert!(pair.assess(&novel).unwrap().completely_covered);
        assert_eq!(triple.assess(&novel).unwrap().missing_patterns, 1);
        assert_eq!(triple.tuple_count(), 1);
    }

    #[test]
    fn candidate_union_is_bounded_and_does_not_inspect_true_teacher() {
        let limits = WideAnfLimits::new(32, 256, 8192, 8192, 8192);
        let g1 = WideAnfGate::from_monomials(vec![1, 8], vec![vec![0, 1]], 16, limits).unwrap();
        let g2 = WideAnfGate::from_monomials(vec![3, 8], vec![vec![0], vec![1]], 16, limits).unwrap();
        assert_eq!(competing_support(&[g1.clone(), g2.clone()], 4).unwrap(), vec![1, 3, 8]);
        assert_eq!(
            competing_support(&[g1, g2], 2).unwrap_err(),
            ScopedRiskError::Budget
        );
    }

    #[test]
    fn hamming16_uses_exact_full_input_minimum_not_selected_support() {
        let mut index = Hamming16Guard::new(2).unwrap();
        index.observe(&vec![false; 16]).unwrap();
        let mut second = vec![true; 16];
        second[3] = false;
        index.observe(&second).unwrap();
        index.seal().unwrap();
        assert_eq!(index.distance(&vec![false; 16]).unwrap(), 0);
        assert_eq!(index.distance(&vec![true; 16]).unwrap(), 1);
        let mut query = vec![false; 16];
        query[2] = true;
        assert_eq!(index.distance(&query).unwrap(), 1);
        assert_eq!(index.observe(&query), Err(ScopedRiskError::Sealed));
        assert_eq!(index.logical_payload_bytes(), 131_072);
    }

    #[test]
    fn invalid_inputs_and_quotas_fail_before_mutating() {
        let mut g = ScopedPatternGuard::new(16, &[1, 3, 5], 3, 1, 10).unwrap();
        assert_eq!(g.seal(), Err(ScopedRiskError::EmptyTraining));
        let old = g.clone();
        assert_eq!(g.observe(&[false; 8]), Err(ScopedRiskError::Width));
        assert_eq!(g, old);
        g.observe(&[false; 16]).unwrap();
        assert_eq!(g.observe(&[true; 16]), Err(ScopedRiskError::Budget));
        assert_eq!(g.assess(&[false; 16]), Err(ScopedRiskError::NotSealed));
        g.seal().unwrap();
        assert_eq!(g.observe(&[false; 16]), Err(ScopedRiskError::Sealed));
        assert_eq!(
            ScopedPatternGuard::new(16, &[1, 1, 2], 2, 10, 10).unwrap_err(),
            ScopedRiskError::DuplicateSupport
        );
        assert_eq!(
            ScopedPatternGuard::new(16, &[1, 2, 3, 4, 5], 3, 10, 1).unwrap_err(),
            ScopedRiskError::Budget
        );
        assert_eq!(
            DegreeDesignRank::new(16, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 2).unwrap_err(),
            ScopedRiskError::Budget
        );
        let mut complete = DegreeDesignRank::new(3, &[0, 1, 2], 1).unwrap();
        for code in 0..8 {
            complete.observe(&vec_from_code(3, code)).unwrap();
        }
        assert!(complete.full_rank());
    }
}
