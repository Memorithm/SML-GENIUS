//! A6-WL2: unanchored active sparse ANF identification on a restricted
//! disjoint positive-monomial grammar. No public selector indices.
//!
//! An arbitrary positive AND monomial has no privileged selector: each
//! factor is symmetric. The learner identifies the unordered full support
//! of four disjoint positive monomials, using only TRAIN membership queries.
//! This is NOT passive learning, generic ANF factoring or parity-with-noise.

use std::fmt;

use crate::{
    anf_wide_sparse::{WideAnfError, WideAnfGate, WideAnfLimits},
    toy_tasks::SplitMix64,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnanchoredLimits {
    pub max_fan_in: usize,
    pub expected_terms: usize,
    pub max_degree: usize,
    pub max_physical_queries: usize,
    pub max_witness_attempts_per_term: usize,
}

impl UnanchoredLimits {
    pub const fn new(
        max_fan_in: usize,
        expected_terms: usize,
        max_degree: usize,
        max_physical_queries: usize,
        max_witness_attempts_per_term: usize,
    ) -> Self {
        Self {
            max_fan_in,
            expected_terms,
            max_degree,
            max_physical_queries,
            max_witness_attempts_per_term,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnanchoredError {
    Width,
    Terms,
    Repetitions,
    Budget,
    WitnessExhausted,
    Allocation,
    Gate(WideAnfError),
}
impl fmt::Display for UnanchoredError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for UnanchoredError {}
impl From<WideAnfError> for UnanchoredError {
    fn from(e: WideAnfError) -> Self {
        Self::Gate(e)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnanchoredFit {
    pub gate: WideAnfGate,
    /// Canonically sorted FULL monomial supports; there are no selectors.
    pub learned_monomials: Vec<Vec<usize>>,
    pub logical_queries: usize,
    pub physical_queries: usize,
    pub witness_attempts: usize,
    pub rejected_witnesses: usize,
}

struct Probe<F> {
    membership: F,
    repetitions: usize,
    query_limit: usize,
    logical_queries: usize,
    physical_queries: usize,
}

impl<F> Probe<F>
where
    F: FnMut(&[bool]) -> Result<bool, UnanchoredError>,
{
    /// Strict-majority Boolean oracle answer, then XOR locally with learned
    /// terms. No teacher coefficient, support, source or TEST information
    /// can enter the residual evaluator.
    fn residual(
        &mut self,
        input: &[bool],
        learned: &[Vec<usize>],
    ) -> Result<bool, UnanchoredError> {
        let next = self
            .physical_queries
            .checked_add(self.repetitions)
            .ok_or(UnanchoredError::Budget)?;
        if next > self.query_limit {
            return Err(UnanchoredError::Budget);
        }
        self.logical_queries = self
            .logical_queries
            .checked_add(1)
            .ok_or(UnanchoredError::Budget)?;
        let mut true_votes = 0usize;
        for _ in 0..self.repetitions {
            self.physical_queries += 1;
            true_votes += usize::from((self.membership)(input)?);
        }
        let mut value = true_votes > self.repetitions / 2;
        for term in learned {
            value ^= term.iter().all(|&variable| input[variable]);
        }
        Ok(value)
    }
}

/// Input maintains residual(input)=1. Try deleting all currently enabled
/// bits of a candidate group; if that loses the positive witness, undo and
/// recursively split it. Exactly preserve any bits already disabled.
fn minimize_group<F>(
    probe: &mut Probe<F>,
    input: &mut [bool],
    group: &[usize],
    learned: &[Vec<usize>],
) -> Result<(), UnanchoredError>
where
    F: FnMut(&[bool]) -> Result<bool, UnanchoredError>,
{
    let active = group
        .iter()
        .copied()
        .filter(|&index| input[index])
        .collect::<Vec<_>>();
    if active.is_empty() {
        return Ok(());
    }
    for &index in &active {
        input[index] = false;
    }
    let attempt = probe.residual(input, learned);
    if matches!(attempt, Ok(true)) {
        return Ok(());
    }
    // Restore before propagating a genuine callback/budget error. Never
    // convert an oracle error into a fabricated membership observation.
    for &index in &active {
        input[index] = true;
    }
    attempt?;
    if group.len() > 1 {
        let midpoint = group.len() / 2;
        minimize_group(probe, input, &group[..midpoint], learned)?;
        minimize_group(probe, input, &group[midpoint..], learned)?;
    }
    Ok(())
}

/// Extract four unordered, pairwise-disjoint square-free positive monomials.
/// Only TRAIN Boolean labels are available through `membership`; the PRNG
/// chooses interventions and is independent of the teacher's hidden RNG.
///
/// For this grammar, any positive residual witness has an odd number of
/// active terms. If three disjoint degree>=2 terms remain active, its support
/// contains at least six bits and is rejected by the degree<=5 cap.
pub fn fit_unanchored_disjoint_sparse<F>(
    fan_in: usize,
    repetitions: usize,
    seed: u64,
    limits: UnanchoredLimits,
    gate_limits: WideAnfLimits,
    membership: F,
) -> Result<UnanchoredFit, UnanchoredError>
where
    F: FnMut(&[bool]) -> Result<bool, UnanchoredError>,
{
    if fan_in < 8 || fan_in > limits.max_fan_in {
        return Err(UnanchoredError::Width);
    }
    if limits.expected_terms != 4
        || limits.max_degree != 5
        || limits.max_witness_attempts_per_term == 0
        || limits.max_physical_queries == 0
    {
        return Err(UnanchoredError::Terms);
    }
    if repetitions == 0 || repetitions > 21 || repetitions % 2 == 0 {
        return Err(UnanchoredError::Repetitions);
    }
    let mut probe = Probe {
        membership,
        repetitions,
        query_limit: limits.max_physical_queries,
        logical_queries: 0,
        physical_queries: 0,
    };
    let mut rng = SplitMix64::new(seed);
    let indices = (0..fan_in).collect::<Vec<_>>();
    let mut input = vec![false; fan_in];
    let mut learned = Vec::<Vec<usize>>::new();
    learned
        .try_reserve_exact(limits.expected_terms)
        .map_err(|_| UnanchoredError::Allocation)?;
    let mut witness_attempts = 0usize;
    let mut rejected_witnesses = 0usize;
    for _ in 0..limits.expected_terms {
        let mut found = false;
        for _ in 0..limits.max_witness_attempts_per_term {
            witness_attempts = witness_attempts
                .checked_add(1)
                .ok_or(UnanchoredError::Budget)?;
            for bit in &mut input {
                *bit = rng.next_u64() & 1 != 0;
            }
            if !probe.residual(&input, &learned)? {
                continue;
            }
            for _ in 0..fan_in {
                let before = input.iter().filter(|&&on| on).count();
                minimize_group(&mut probe, &mut input, &indices, &learned)?;
                if input.iter().filter(|&&on| on).count() == before {
                    break;
                }
            }
            let support = input
                .iter()
                .enumerate()
                .filter_map(|(index, &bit)| bit.then_some(index))
                .collect::<Vec<_>>();
            if !(2..=limits.max_degree).contains(&support.len())
                || learned.contains(&support)
                || learned
                    .iter()
                    .any(|known| known.iter().any(|i| support.contains(i)))
            {
                rejected_witnesses += 1;
                continue;
            }
            learned.push(support);
            found = true;
            break;
        }
        if !found {
            return Err(UnanchoredError::WitnessExhausted);
        }
    }
    learned.sort_unstable();
    let gate =
        WideAnfGate::from_monomials((0..fan_in).collect(), learned.clone(), fan_in, gate_limits)?;
    Ok(UnanchoredFit {
        gate,
        learned_monomials: learned,
        logical_queries: probe.logical_queries,
        physical_queries: probe.physical_queries,
        witness_attempts,
        rejected_witnesses,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn learn_limits() -> UnanchoredLimits {
        UnanchoredLimits::new(2048, 4, 5, 25_000, 512)
    }
    fn gate_limits() -> WideAnfLimits {
        WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
    }
    fn teacher(k: usize) -> (WideAnfGate, Vec<Vec<usize>>) {
        let mut terms = if k == 8 {
            vec![vec![0, 1], vec![2, 3], vec![4, 5], vec![6, 7]]
        } else {
            vec![
                vec![0, 9],
                vec![1, 5, 11],
                vec![2, 6, 10, 12],
                vec![3, 4, 7, 8, 13],
            ]
        };
        terms.sort_unstable();
        let gate =
            WideAnfGate::from_monomials((0..k).collect(), terms.clone(), k, gate_limits()).unwrap();
        (gate, terms)
    }

    #[test]
    fn truly_unanchored_supports_are_recovered_from_only_four_bitwise_outputs() {
        for k in [8, 16, 32, 64, 128, 256, 1024] {
            let (oracle, terms) = teacher(k);
            let fit = fit_unanchored_disjoint_sparse(
                k,
                1,
                k as u64 * 929 + 31,
                learn_limits(),
                gate_limits(),
                |q| oracle.evaluate_scalar(q).map_err(Into::into),
            )
            .unwrap();
            assert_eq!(fit.learned_monomials, terms, "k={k}");
            assert!(fit.logical_queries < 1000);
        }
    }

    #[test]
    fn noisy_majority_does_not_receive_any_selector_or_teacher_support() {
        let (oracle, terms) = teacher(128);
        let mut rng = SplitMix64::new(0x9911);
        let fit =
            fit_unanchored_disjoint_sparse(128, 21, 373, learn_limits(), gate_limits(), |q| {
                let clean = oracle.evaluate_scalar(q)?;
                Ok(clean ^ (rng.next_u64() & 15 == 0))
            })
            .unwrap();
        assert_eq!(fit.learned_monomials, terms);
        assert_eq!(fit.physical_queries, fit.logical_queries * 21);
    }

    #[test]
    fn illegal_width_query_budget_and_repetitions_are_rejected() {
        let (oracle, _) = teacher(16);
        let observe = |q: &[bool]| oracle.evaluate_scalar(q).map_err(Into::into);
        assert_eq!(
            fit_unanchored_disjoint_sparse(0, 1, 2, learn_limits(), gate_limits(), observe),
            Err(UnanchoredError::Width)
        );
        assert_eq!(
            fit_unanchored_disjoint_sparse(16, 2, 2, learn_limits(), gate_limits(), observe),
            Err(UnanchoredError::Repetitions)
        );
        assert_eq!(
            fit_unanchored_disjoint_sparse(
                16,
                1,
                2,
                UnanchoredLimits::new(2048, 4, 5, 1, 512),
                gate_limits(),
                observe,
            ),
            Err(UnanchoredError::Budget)
        );
    }

    #[test]
    fn selector_label_is_not_identifiable_from_commutative_and() {
        // Both factorizations describe EXACTLY the same ANF monomial:
        // no observation on inputs/outputs can tell which factor was a
        // privileged "selector".
        let expression_a = [2usize, 9, 12];
        let expression_b = [12usize, 2, 9];
        let gate_a = WideAnfGate::from_monomials(
            (0..16).collect(),
            vec![expression_a.to_vec()],
            16,
            gate_limits(),
        )
        .unwrap();
        let gate_b = WideAnfGate::from_monomials(
            (0..16).collect(),
            vec![expression_b.to_vec()],
            16,
            gate_limits(),
        )
        .unwrap();
        for mask in 0..512usize {
            let q = (0..16)
                .map(|i| mask & (1 << (i % 9)) != 0)
                .collect::<Vec<_>>();
            assert_eq!(gate_a.evaluate_scalar(&q), gate_b.evaluate_scalar(&q));
        }
    }
}
