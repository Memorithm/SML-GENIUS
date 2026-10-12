//! A6-WL3: learn bounded positive sparse ANF with OVERLAPPING support sets
//! and unknown term count via active membership queries, GF(2) Möbius
//! coefficient certificates, and a probability-bounded residual zero test.
//!
//! Unlike A6-WL2, no disjointness or known cardinality is assumed.
//! This is NOT passive learning or a generic unrestricted ANF algorithm.
//! The degree<=5 and <=4 terms priors, Boolean membership interventions,
//! and independent uniform stopping proposals are central assumptions.

use std::fmt;

use crate::{
    anf_wide_sparse::{WideAnfError, WideAnfGate, WideAnfLimits},
    toy_tasks::SplitMix64,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlapLearnLimits {
    pub max_fan_in: usize,
    pub max_terms: usize,
    pub min_degree: usize,
    pub max_degree: usize,
    pub max_physical_queries: usize,
    pub max_witness_trials: usize,
}

impl OverlapLearnLimits {
    pub const fn new(
        max_fan_in: usize,
        max_terms: usize,
        min_degree: usize,
        max_degree: usize,
        max_physical_queries: usize,
        max_witness_trials: usize,
    ) -> Self {
        Self {
            max_fan_in,
            max_terms,
            min_degree,
            max_degree,
            max_physical_queries,
            max_witness_trials,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OverlapLearnError {
    Width,
    Limits,
    Repetitions,
    Budget,
    TooManyTerms,
    WitnessExhausted,
    Allocation,
    Gate(WideAnfError),
}
impl fmt::Display for OverlapLearnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OverlapLearnError {}
impl From<WideAnfError> for OverlapLearnError {
    fn from(e: WideAnfError) -> Self {
        Self::Gate(e)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlapFit {
    pub gate: WideAnfGate,
    /// Canonical, unordered full support sets, certified by GF(2) Möbius.
    pub learned_monomials: Vec<Vec<usize>>,
    pub logical_queries: usize,
    pub physical_queries: usize,
    pub attempted_witnesses: usize,
    pub candidate_rejections: usize,
    pub mobius_certificates: usize,
    /// Number of independent uniform residual queries used for final stop.
    pub zero_residual_probes: usize,
    /// Empirical stop is NOT a mathematical proof of absence of terms.
    pub stopped_probabilistically: bool,
}

struct Probe<F> {
    oracle: F,
    repetitions: usize,
    max_physical_queries: usize,
    logical_queries: usize,
    physical_queries: usize,
}

impl<F> Probe<F>
where
    F: FnMut(&[bool]) -> Result<bool, OverlapLearnError>,
{
    fn residual(
        &mut self,
        input: &[bool],
        learned: &[Vec<usize>],
    ) -> Result<bool, OverlapLearnError> {
        let due = self
            .physical_queries
            .checked_add(self.repetitions)
            .ok_or(OverlapLearnError::Budget)?;
        if due > self.max_physical_queries {
            return Err(OverlapLearnError::Budget);
        }
        self.logical_queries = self
            .logical_queries
            .checked_add(1)
            .ok_or(OverlapLearnError::Budget)?;
        let mut votes = 0usize;
        for _ in 0..self.repetitions {
            self.physical_queries += 1;
            if (self.oracle)(input)? {
                votes += 1;
            }
        }
        let mut value = votes > self.repetitions / 2;
        for term in learned {
            value ^= term.iter().all(|&i| input[i]);
        }
        Ok(value)
    }
}

/// Delete a group if the residual remains 1; otherwise restore and descend.
/// The temporary assignment is restored before ANY callback/budget error
/// propagates. Group deletion alone does NOT certify a true ANF coefficient.
fn minimize_group<F>(
    probe: &mut Probe<F>,
    assignment: &mut [bool],
    positions: &[usize],
    known: &[Vec<usize>],
) -> Result<(), OverlapLearnError>
where
    F: FnMut(&[bool]) -> Result<bool, OverlapLearnError>,
{
    let enabled = positions
        .iter()
        .copied()
        .filter(|&position| assignment[position])
        .collect::<Vec<_>>();
    if enabled.is_empty() {
        return Ok(());
    }
    for &position in &enabled {
        assignment[position] = false;
    }
    let result = probe.residual(assignment, known);
    if matches!(result, Ok(true)) {
        return Ok(());
    }
    for &position in &enabled {
        assignment[position] = true;
    }
    result?;
    if positions.len() > 1 {
        let half = positions.len() / 2;
        minimize_group(probe, assignment, &positions[..half], known)?;
        minimize_group(probe, assignment, &positions[half..], known)?;
    }
    Ok(())
}

/// Exact GF(2) subset Möbius inversion on a d<=5 candidate assignment.
/// The algebraic coefficient at mask U equals XOR_{V subseteq U} f(1_V).
/// Thus even when overlapping monomials trap group minimization in a
/// coalition, only GENUINE original residual coefficients are accepted.
///
/// All evaluations use the same TRAIN oracle (and repeat-majority policy).
fn certified_support<F>(
    probe: &mut Probe<F>,
    fan_in: usize,
    candidate: &[usize],
    known: &[Vec<usize>],
    min_degree: usize,
    max_degree: usize,
) -> Result<Option<Vec<usize>>, OverlapLearnError>
where
    F: FnMut(&[bool]) -> Result<bool, OverlapLearnError>,
{
    let degree = candidate.len();
    if degree > max_degree || degree < min_degree {
        return Ok(None);
    }
    let rows = 1usize
        .checked_shl(degree as u32)
        .ok_or(OverlapLearnError::Limits)?;
    let mut truth = Vec::new();
    truth
        .try_reserve_exact(rows)
        .map_err(|_| OverlapLearnError::Allocation)?;
    let mut query = vec![false; fan_in];
    for mask in 0..rows {
        for (bit, &position) in candidate.iter().enumerate() {
            query[position] = mask & (1 << bit) != 0;
        }
        truth.push(probe.residual(&query, known)?);
    }
    // The binary subset zeta/Möbius transform is involutive in GF(2).
    for bit in 0..degree {
        for mask in 0..rows {
            if mask & (1 << bit) != 0 {
                truth[mask] ^= truth[mask ^ (1 << bit)];
            }
        }
    }
    let mut exact_terms = Vec::new();
    for (mask, active) in truth.into_iter().enumerate() {
        if !active {
            continue;
        }
        let indices = candidate
            .iter()
            .enumerate()
            .filter_map(|(i, &position)| (mask & (1 << i) != 0).then_some(position))
            .collect::<Vec<_>>();
        if (min_degree..=max_degree).contains(&indices.len()) && !known.contains(&indices) {
            exact_terms.push(indices);
        }
    }
    exact_terms.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    Ok(exact_terms.into_iter().next())
}

/// Train-only fitting of unknown-order (<=4), degree-bounded GF(2) ANF.
/// No selector positions, support structure, or true term count is passed.
///
/// Final absence is *probabilistic*: if residual polynomial is nonzero with
/// degree<=d, Reed-Muller minimum relative weight >=2^-d gives a miss
/// probability of at most (1-2^-d)^trials on clean independent queries.
/// No such unconditional bound applies to noisy or misspecified oracles.
pub fn fit_overlap_sparse<F>(
    fan_in: usize,
    repetitions: usize,
    proposal_seed: u64,
    limits: OverlapLearnLimits,
    gate_limits: WideAnfLimits,
    oracle: F,
) -> Result<OverlapFit, OverlapLearnError>
where
    F: FnMut(&[bool]) -> Result<bool, OverlapLearnError>,
{
    if fan_in < 8 || fan_in > limits.max_fan_in {
        return Err(OverlapLearnError::Width);
    }
    if limits.max_terms != 4
        || limits.min_degree != 2
        || limits.max_degree != 5
        || limits.max_witness_trials != 512
        || limits.max_physical_queries == 0
    {
        return Err(OverlapLearnError::Limits);
    }
    if repetitions != 1 && repetitions != 21 {
        return Err(OverlapLearnError::Repetitions);
    }
    let mut probe = Probe {
        oracle,
        repetitions,
        max_physical_queries: limits.max_physical_queries,
        logical_queries: 0,
        physical_queries: 0,
    };
    let mut rng = SplitMix64::new(proposal_seed);
    let indices = (0..fan_in).collect::<Vec<_>>();
    let mut assignment = vec![false; fan_in];
    let mut learned = Vec::<Vec<usize>>::new();
    learned
        .try_reserve_exact(limits.max_terms)
        .map_err(|_| OverlapLearnError::Allocation)?;
    let mut attempted_witnesses = 0usize;
    let mut candidate_rejections = 0usize;
    let mut mobius_certificates = 0usize;
    let zero_residual_probes = loop {
        let mut found = None;
        let mut saw_positive = false;
        for _ in 0..limits.max_witness_trials {
            attempted_witnesses += 1;
            for bit in &mut assignment {
                *bit = rng.next_u64() & 1 != 0;
            }
            let positive = probe.residual(&assignment, &learned)?;
            if !positive {
                continue;
            }
            saw_positive = true;
            if learned.len() == limits.max_terms {
                return Err(OverlapLearnError::TooManyTerms);
            }
            // A positive Boolean witness is not a certified support.
            // Keep it positive during deterministic group reduction.
            for _ in 0..2 {
                minimize_group(&mut probe, &mut assignment, &indices, &learned)?;
            }
            let support = assignment
                .iter()
                .enumerate()
                .filter_map(|(i, &on)| on.then_some(i))
                .collect::<Vec<_>>();
            if !(limits.min_degree..=limits.max_degree).contains(&support.len()) {
                candidate_rejections += 1;
                continue;
            }
            mobius_certificates += 1;
            let certified = certified_support(
                &mut probe,
                fan_in,
                &support,
                &learned,
                limits.min_degree,
                limits.max_degree,
            )?;
            if let Some(term) = certified {
                found = Some(term);
                break;
            }
            candidate_rejections += 1;
        }
        if let Some(term) = found {
            learned.push(term);
            // The next iteration uses an independent residual search.
            continue;
        }
        if saw_positive {
            return Err(OverlapLearnError::WitnessExhausted);
        }
        break limits.max_witness_trials;
    };
    learned.sort_unstable();
    let gate =
        WideAnfGate::from_monomials((0..fan_in).collect(), learned.clone(), fan_in, gate_limits)?;
    Ok(OverlapFit {
        gate,
        learned_monomials: learned,
        logical_queries: probe.logical_queries,
        physical_queries: probe.physical_queries,
        attempted_witnesses,
        candidate_rejections,
        mobius_certificates,
        zero_residual_probes,
        stopped_probabilistically: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn caps() -> OverlapLearnLimits {
        OverlapLearnLimits::new(2048, 4, 2, 5, 160_000, 512)
    }
    fn gates() -> WideAnfLimits {
        WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
    }
    fn oracle(k: usize, monomials: &[Vec<usize>]) -> WideAnfGate {
        WideAnfGate::from_monomials((0..k).collect(), monomials.to_vec(), k, gates()).unwrap()
    }
    #[test]
    fn overlap_hub_and_chain_supports_can_be_identified_without_disjointness() {
        for (k, terms) in [
            (8, vec![vec![0, 1], vec![0, 2], vec![0, 3], vec![0, 4]]),
            (16, vec![vec![0, 1], vec![1, 2, 3], vec![3, 4, 5, 6]]),
        ] {
            let model = oracle(k, &terms);
            let fit = fit_overlap_sparse(k, 1, 0xd15e_1234 ^ k as u64, caps(), gates(), |x| {
                model.evaluate_scalar(x).map_err(Into::into)
            })
            .unwrap();
            let mut canonical = terms;
            canonical.sort_unstable();
            assert_eq!(fit.learned_monomials, canonical);
            assert_eq!(fit.zero_residual_probes, 512);
            assert!(fit.mobius_certificates >= canonical.len());
        }
    }
    #[test]
    fn nested_positive_monomials_are_not_erased_by_xor_cancellation() {
        let terms = vec![vec![1, 2], vec![1, 2, 3, 4, 5]];
        let model = oracle(16, &terms);
        let fit = fit_overlap_sparse(16, 1, 1551, caps(), gates(), |x| {
            model.evaluate_scalar(x).map_err(Into::into)
        })
        .unwrap();
        assert_eq!(fit.learned_monomials, terms);
    }
    #[test]
    fn mobius_certifies_true_terms_not_an_overlapping_three_way_coalition() {
        let k = 8;
        let terms = vec![vec![0, 1], vec![0, 2], vec![0, 3]];
        let model = oracle(k, &terms);
        let mut p = Probe {
            oracle: |x: &[bool]| model.evaluate_scalar(x).map_err(Into::into),
            repetitions: 1,
            max_physical_queries: 160_000,
            logical_queries: 0,
            physical_queries: 0,
        };
        let got = certified_support(&mut p, k, &[0, 1, 2, 3], &[], 2, 5)
            .unwrap()
            .unwrap();
        assert!(terms.contains(&got));
        assert_ne!(got, vec![0, 1, 2, 3]);
    }
    #[test]
    fn invalid_budget_width_and_repetitions_fail_closed() {
        let model = oracle(8, &[vec![0, 1], vec![2, 3]]);
        let observe = |x: &[bool]| model.evaluate_scalar(x).map_err(Into::into);
        assert_eq!(
            fit_overlap_sparse(0, 1, 0, caps(), gates(), observe),
            Err(OverlapLearnError::Width)
        );
        assert_eq!(
            fit_overlap_sparse(8, 2, 0, caps(), gates(), observe),
            Err(OverlapLearnError::Repetitions)
        );
        assert_eq!(
            fit_overlap_sparse(
                8,
                1,
                0,
                OverlapLearnLimits::new(2048, 4, 2, 5, 1, 512),
                gates(),
                observe,
            ),
            Err(OverlapLearnError::Budget)
        );
    }
    #[test]
    fn fifth_degree_six_monomial_is_not_silently_truncated() {
        let model = oracle(
            16,
            &[
                vec![0, 1],
                vec![0, 2, 3],
                vec![0, 4, 5, 6],
                vec![0, 7, 8, 9, 10],
                vec![1, 2, 3, 4, 5, 6],
            ],
        );
        let fit = fit_overlap_sparse(16, 1, 76, caps(), gates(), |x| {
            model.evaluate_scalar(x).map_err(Into::into)
        });
        assert!(fit.is_err());
    }
}
