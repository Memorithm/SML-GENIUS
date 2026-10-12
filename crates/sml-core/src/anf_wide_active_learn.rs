//! Active membership-query recovery of bounded, selector-anchored sparse ANF.
//!
//! This is NOT generic sparse ANF system identification. The public grammar is
//! XOR_{a in selectors}(x_a AND product_{j in hidden(a)} x_j), with exactly one
//! positive-literal conjunction per known selector and a nonempty hidden set.
//! The learner receives ONLY a Boolean answer to its chosen TRAIN queries,
//! never teacher monomials, source wiring, VAL or held-out TEST data.
//!
//! Group queries set all nonselector variables to 1 except for one subset,
//! which is switched off. With one selector active, this is a group test
//! detecting whether the unknown support intersects the switched-off subset.
//! This structural restriction makes adaptive O(s log k) discovery possible;
//! there is no analogous general guarantee for arbitrary GF(2) polynomials.

use crate::anf_wide_sparse::{WideAnfError, WideAnfGate, WideAnfLimits};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideAnchorLearnLimits {
    pub max_fan_in: usize,
    pub max_selectors: usize,
    pub max_physical_queries: usize,
    pub max_hidden_literals_per_term: usize,
}

impl WideAnchorLearnLimits {
    pub const fn new(
        max_fan_in: usize,
        max_selectors: usize,
        max_physical_queries: usize,
        max_hidden_literals_per_term: usize,
    ) -> Self {
        Self {
            max_fan_in,
            max_selectors,
            max_physical_queries,
            max_hidden_literals_per_term,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WideAnchorLearnError {
    InvalidWidth,
    InvalidSelectors,
    InvalidRepetitions,
    BudgetExceeded,
    Allocation,
    PriorViolated,
    EmptyHiddenSupport,
    HiddenDegreeExceeded,
    Gate(WideAnfError),
}

impl fmt::Display for WideAnchorLearnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WideAnchorLearnError {}
impl From<WideAnfError> for WideAnchorLearnError {
    fn from(e: WideAnfError) -> Self {
        Self::Gate(e)
    }
}

/// Frozen fitted native wide gate, containing only recovered support, never
/// teacher wiring. Output can be installed directly in a selected SBG page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WideAnchorFit {
    pub gate: WideAnfGate,
    pub learned_monomials: Vec<Vec<usize>>,
    pub logical_queries: usize,
    pub physical_queries: usize,
    pub recovered_hidden_literals: usize,
}

struct OracleProbe<F> {
    oracle: F,
    repetitions: usize,
    max_physical_queries: usize,
    logical_queries: usize,
    physical_queries: usize,
}

impl<F> OracleProbe<F>
where
    F: FnMut(&[bool]) -> Result<bool, WideAnchorLearnError>,
{
    fn ask(&mut self, query: &[bool]) -> Result<bool, WideAnchorLearnError> {
        let requested = self
            .physical_queries
            .checked_add(self.repetitions)
            .ok_or(WideAnchorLearnError::BudgetExceeded)?;
        if requested > self.max_physical_queries {
            return Err(WideAnchorLearnError::BudgetExceeded);
        }
        self.logical_queries = self
            .logical_queries
            .checked_add(1)
            .ok_or(WideAnchorLearnError::BudgetExceeded)?;
        let mut ones = 0usize;
        for _ in 0..self.repetitions {
            // Count even an oracle callback that returns an error; no trained
            // model is returned on failure and the caller retains ownership.
            self.physical_queries += 1;
            if (self.oracle)(query)? {
                ones += 1;
            }
        }
        Ok(ones > self.repetitions / 2)
    }
}

/// Recursively test one candidate subset. On the registered positive-AND
/// grammar, f(one_hot_anchor, data\group=0) is false iff the hidden
/// support intersects the switched-off group.
fn discover_support<F>(
    probe: &mut OracleProbe<F>,
    assignment: &mut [bool],
    group: &[usize],
    support: &mut Vec<usize>,
    max_hidden_literals: usize,
) -> Result<(), WideAnchorLearnError>
where
    F: FnMut(&[bool]) -> Result<bool, WideAnchorLearnError>,
{
    if group.is_empty() {
        return Ok(());
    }
    for &position in group {
        assignment[position] = false;
    }
    let answer = probe.ask(assignment);
    // Restore before propagating *any* error. Future probes can reuse the
    // same caller-owned buffer without inheriting a mutated query state.
    for &position in group {
        assignment[position] = true;
    }
    if answer? {
        return Ok(());
    }
    if group.len() == 1 {
        if support.len() >= max_hidden_literals {
            return Err(WideAnchorLearnError::HiddenDegreeExceeded);
        }
        support.push(group[0]);
    } else {
        let half = group.len() / 2;
        discover_support(
            probe,
            assignment,
            &group[..half],
            support,
            max_hidden_literals,
        )?;
        discover_support(
            probe,
            assignment,
            &group[half..],
            support,
            max_hidden_literals,
        )?;
    }
    Ok(())
}

/// Recover positive sparse conjunctions conditioned by a FIXED, PUBLIC
/// set of mutually exclusive query selectors. Each selector gates exactly
/// one nonconstant monomial; hidden nonselector variables are discovered.
///
/// `membership` can be a noisy TRAIN oracle; odd repetitions implement
/// strict-majority voting. There is no access to the generator's monomials
/// and no validation/test labels are passed into this method.
pub fn fit_selector_anchored_sparse<F>(
    fan_in: usize,
    selectors: &[usize],
    repetitions: usize,
    learn_limits: WideAnchorLearnLimits,
    gate_limits: WideAnfLimits,
    membership: F,
) -> Result<WideAnchorFit, WideAnchorLearnError>
where
    F: FnMut(&[bool]) -> Result<bool, WideAnchorLearnError>,
{
    if fan_in == 0
        || fan_in > learn_limits.max_fan_in
        || selectors.len() >= fan_in
    {
        return Err(WideAnchorLearnError::InvalidWidth);
    }
    if selectors.is_empty()
        || selectors.len() > learn_limits.max_selectors
        || learn_limits.max_hidden_literals_per_term == 0
    {
        return Err(WideAnchorLearnError::InvalidSelectors);
    }
    if repetitions == 0 || repetitions > 15 || repetitions % 2 == 0 {
        return Err(WideAnchorLearnError::InvalidRepetitions);
    }
    let mut unique = selectors.to_vec();
    unique.sort_unstable();
    if unique.iter().any(|&selector| selector >= fan_in)
        || unique.windows(2).any(|p| p[0] == p[1])
    {
        return Err(WideAnchorLearnError::InvalidSelectors);
    }
    let mut rest = Vec::new();
    rest.try_reserve_exact(fan_in - selectors.len())
        .map_err(|_| WideAnchorLearnError::Allocation)?;
    for index in 0..fan_in {
        if unique.binary_search(&index).is_err() {
            rest.push(index);
        }
    }
    let mut assignment = Vec::new();
    assignment
        .try_reserve_exact(fan_in)
        .map_err(|_| WideAnchorLearnError::Allocation)?;
    assignment.resize(fan_in, true);
    for &selector in selectors {
        assignment[selector] = false;
    }
    let mut probe = OracleProbe {
        oracle: membership,
        repetitions,
        max_physical_queries: learn_limits.max_physical_queries,
        logical_queries: 0,
        physical_queries: 0,
    };
    let mut monomials = Vec::new();
    monomials
        .try_reserve_exact(selectors.len())
        .map_err(|_| WideAnchorLearnError::Allocation)?;
    let mut recovered_hidden_literals = 0usize;
    for &selector in selectors {
        assignment[selector] = true;
        if !probe.ask(&assignment)? {
            return Err(WideAnchorLearnError::PriorViolated);
        }
        let mut hidden = Vec::new();
        hidden
            .try_reserve_exact(learn_limits.max_hidden_literals_per_term)
            .map_err(|_| WideAnchorLearnError::Allocation)?;
        discover_support(
            &mut probe,
            &mut assignment,
            &rest,
            &mut hidden,
            learn_limits.max_hidden_literals_per_term,
        )?;
        if hidden.is_empty() {
            return Err(WideAnchorLearnError::EmptyHiddenSupport);
        }
        hidden.sort_unstable();
        recovered_hidden_literals = recovered_hidden_literals
            .checked_add(hidden.len())
            .ok_or(WideAnchorLearnError::BudgetExceeded)?;
        let mut term = Vec::new();
        term.try_reserve_exact(hidden.len() + 1)
            .map_err(|_| WideAnchorLearnError::Allocation)?;
        term.push(selector);
        term.extend(hidden);
        monomials.push(term);
        assignment[selector] = false;
    }
    let gate = WideAnfGate::from_monomials(
        (0..fan_in).collect(),
        monomials.clone(),
        fan_in,
        gate_limits,
    )?;
    Ok(WideAnchorFit {
        gate,
        learned_monomials: monomials,
        logical_queries: probe.logical_queries,
        physical_queries: probe.physical_queries,
        recovered_hidden_literals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mor::MorDecision,
        pvp6_sbg::{SbgCoordinate, SbgField},
        semantic::SemanticState,
        wide_anf_selected_pages::{WideAnfPage, WideAnfPageAtlas},
        PageId,
    };
    use crate::toy_tasks::SplitMix64;

    const SELECTORS: [usize; 4] = [0, 1, 2, 3];

    fn learn_limits() -> WideAnchorLearnLimits {
        WideAnchorLearnLimits::new(2048, 4, 20_000, 8)
    }
    fn gate_limits() -> WideAnfLimits {
        WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
    }

    fn teacher(k: usize) -> (WideAnfGate, Vec<Vec<usize>>) {
        let terms = vec![
            vec![0, 7, k - 1],
            vec![1, 6, k - 2],
            vec![2, 4, k - 3],
            vec![3, 5, k - 4],
        ];
        // k=8 has some overlapping support ACROSS selectors but no
        // duplicate variables within a term.
        let gate = WideAnfGate::from_monomials(
            (0..k).collect(),
            terms.clone(),
            k,
            gate_limits(),
        )
        .unwrap();
        (gate, terms)
    }

    #[test]
    fn learns_large_unknown_sparse_support_only_from_its_active_labels() {
        let (oracle, terms) = teacher(1024);
        let fit = fit_selector_anchored_sparse(
            1024,
            &SELECTORS,
            1,
            learn_limits(),
            gate_limits(),
            |query| oracle.evaluate_scalar(query).map_err(Into::into),
        )
        .unwrap();
        let mut expected = terms;
        for term in &mut expected {
            term.sort_unstable();
        }
        assert_eq!(fit.learned_monomials, expected);
        assert!(fit.logical_queries < 200);
        let mut rng = SplitMix64::new(888);
        for _ in 0..256 {
            let q = (0..1024).map(|_| rng.next_u64() & 1 != 0).collect::<Vec<_>>();
            assert_eq!(fit.gate.evaluate_scalar(&q), oracle.evaluate_scalar(&q));
        }
    }

    #[test]
    fn majority_votes_stabilize_training_with_repeated_noisy_queries() {
        let (oracle, terms) = teacher(64);
        let mut rng = SplitMix64::new(0x9911);
        let fit = fit_selector_anchored_sparse(
            64,
            &SELECTORS,
            15,
            learn_limits(),
            gate_limits(),
            |query| {
                let clean = oracle.evaluate_scalar(query)?;
                Ok(clean ^ (rng.next_u64() & 15 == 0))
            },
        )
        .unwrap();
        let mut expected = terms;
        for term in &mut expected {
            term.sort_unstable();
        }
        assert_eq!(fit.learned_monomials, expected);
        assert_eq!(fit.physical_queries, fit.logical_queries * 15);
    }

    #[test]
    fn cross_selector_interaction_escapes_train_one_hot_but_fails_validation() {
        let (base, _) = teacher(32);
        let (teacher_original, _) = teacher(32);
        let mut invalid_terms = vec![
            vec![0, 7, 31],
            vec![1, 6, 30],
            vec![2, 4, 29],
            vec![3, 5, 28],
        ];
        invalid_terms.push(vec![0, 1]);
        let invalid = WideAnfGate::from_monomials(
            (0..32).collect(),
            invalid_terms,
            32,
            gate_limits(),
        )
        .unwrap();
        let fit = fit_selector_anchored_sparse(
            32, &SELECTORS, 1, learn_limits(), gate_limits(),
            |query| invalid.evaluate_scalar(query).map_err(Into::into),
        )
        .unwrap();
        let mut query = vec![true; 32];
        assert_eq!(fit.gate.evaluate_scalar(&query), base.evaluate_scalar(&query));
        assert_ne!(fit.gate.evaluate_scalar(&query), invalid.evaluate_scalar(&query));
        query[1] = false;
        assert_eq!(fit.gate.evaluate_scalar(&query), invalid.evaluate_scalar(&query));
        assert_eq!(fit.gate.evaluate_scalar(&query), teacher_original.evaluate_scalar(&query));
    }

    #[test]
    fn invalid_prior_and_query_resource_limits_fail_closed() {
        let (oracle, _) = teacher(16);
        let observe = |q: &[bool]| oracle.evaluate_scalar(q).map_err(Into::into);
        assert_eq!(
            fit_selector_anchored_sparse(0, &SELECTORS, 1, learn_limits(), gate_limits(), observe),
            Err(WideAnchorLearnError::InvalidWidth)
        );
        assert_eq!(
            fit_selector_anchored_sparse(16, &[0, 0], 1, learn_limits(), gate_limits(), observe),
            Err(WideAnchorLearnError::InvalidSelectors)
        );
        assert_eq!(
            fit_selector_anchored_sparse(16, &SELECTORS, 2, learn_limits(), gate_limits(), observe),
            Err(WideAnchorLearnError::InvalidRepetitions)
        );
        assert_eq!(
            fit_selector_anchored_sparse(16, &SELECTORS, 1,
                WideAnchorLearnLimits::new(2048, 4, 1, 8),
                gate_limits(), observe),
            Err(WideAnchorLearnError::BudgetExceeded)
        );
        assert_eq!(
            fit_selector_anchored_sparse(16, &SELECTORS, 1,
                WideAnchorLearnLimits::new(2048, 4, 20_000, 1),
                gate_limits(), observe),
            Err(WideAnchorLearnError::HiddenDegreeExceeded)
        );
    }

    #[test]
    fn truly_fitted_k1024_gate_executes_as_selected_sbg_page() {
        let (oracle, _) = teacher(1024);
        let fit = fit_selector_anchored_sparse(
            1024, &SELECTORS, 1, learn_limits(), gate_limits(),
            |q| oracle.evaluate_scalar(q).map_err(Into::into),
        )
        .unwrap();
        let atlas = WideAnfPageAtlas::<1024>::new(
            vec![WideAnfPage {
                id: PageId(7),
                gates: vec![fit.gate.clone()],
                inputs: (0..1024)
                    .map(|bit| SbgCoordinate { field: SbgField::Value, bit })
                    .collect(),
                outputs: vec![SbgCoordinate { field: SbgField::Meaning, bit: 0 }],
            }],
            1,
        )
        .unwrap();
        let decision = MorDecision {
            active_routers: vec![],
            ranked_experts: vec![],
            selected_experts: vec![PageId(7)],
            noise_ppm: 0,
            structural_disagreement_ppm: 0,
            expert_mass_threshold_ppm: 0,
            request_precision_escalation: false,
            hysteresis_retained_previous_primary: false,
        };
        for (selector, zero) in [(0, None), (1, Some(72)), (2, Some(712)), (3, None)] {
            let mut q = vec![true; 1024];
            for &a in &SELECTORS { q[a] = false; }
            q[selector] = true;
            if let Some(index) = zero { q[index] = false; }
            let mut state = SemanticState::<1024>::zeroed().unwrap();
            for (i, &value) in q.iter().enumerate() {
                if value { state.value.set(i, true).unwrap(); }
            }
            let executed = atlas.execute(&state, &decision, 1, 1).unwrap();
            assert_eq!(
                executed.state.meaning_mask.get(0).unwrap(),
                oracle.evaluate_scalar(&q).unwrap()
            );
            assert_eq!(executed.traces[0].id, PageId(7));
            assert_eq!(executed.traces[0].inputs, 1024);
        }
    }
}
