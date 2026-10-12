//! A6-I1: epistemic predictions under *provable* observational ambiguity.
//!
//! A pair of real wide ANF candidates can agree on every observed context
//! while disagreeing elsewhere. A task-specific identity is then impossible
//! to identify from those observations. The only inductive signal used here
//! is a contextual PRIOR learned from *previous labeled tasks*; the current
//! task's hidden teacher ID is never consulted by predictions.
//!
//! Outputs keep apart hypothesis agreement, probability, forced prediction,
//! selective abstention, and an OPTIONAL active information-gain probe.
//! This is operational Bayesian-style inference, not human consciousness,
//! unrestricted ANF learning, or a guarantee under distribution shift.

use crate::anf_wide_sparse::{WideAnfError, WideAnfGate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpistemicAnfError {
    Context,
    EmptyMetaHistory,
    InvalidProbability,
    UnequalFanIn,
    InputWidth,
    NoDisagreement,
    MissingIntervention,
    Gate(WideAnfError),
}

impl From<WideAnfError> for EpistemicAnfError {
    fn from(error: WideAnfError) -> Self {
        Self::Gate(error)
    }
}
impl std::fmt::Display for EpistemicAnfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EpistemicAnfError {}

/// Only *historical, fully labeled tasks* may populate this meta-history.
/// Blind test-task labels are NOT an available input to the predictor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ContextualPrior {
    total: [u32; 2],
    alternative: [u32; 2],
}
impl ContextualPrior {
    pub fn observe_historical(
        &mut self,
        context: usize,
        teacher_is_alternative: bool,
    ) -> Result<(), EpistemicAnfError> {
        if context >= 2 {
            return Err(EpistemicAnfError::Context);
        }
        self.total[context] = self.total[context]
            .checked_add(1)
            .ok_or(EpistemicAnfError::InvalidProbability)?;
        if teacher_is_alternative {
            self.alternative[context] = self.alternative[context]
                .checked_add(1)
                .ok_or(EpistemicAnfError::InvalidProbability)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn total_historical(&self) -> u32 {
        self.total[0] + self.total[1]
    }

    pub fn probability_alternative(&self, context: usize) -> Result<f64, EpistemicAnfError> {
        if context >= 2 {
            return Err(EpistemicAnfError::Context);
        }
        if self.total[context] == 0 {
            return Err(EpistemicAnfError::EmptyMetaHistory);
        }
        // Beta(1,1) prior: report probability, not certainty.
        Ok((f64::from(self.alternative[context]) + 1.0)
            / (f64::from(self.total[context]) + 2.0))
    }

    pub fn pooled_probability_alternative(&self) -> Result<f64, EpistemicAnfError> {
        if self.total[0] == 0 || self.total[1] == 0 {
            return Err(EpistemicAnfError::EmptyMetaHistory);
        }
        Ok((f64::from(self.alternative[0] + self.alternative[1]) + 1.0)
            / (f64::from(self.total[0] + self.total[1]) + 2.0))
    }

    #[must_use]
    pub fn count_for(&self, context: usize) -> Option<(u32, u32)> {
        (context < 2).then_some((self.total[context], self.alternative[context]))
    }
}

/// A candidate set supplied by the ANF bank/structural discovery, not by
/// peeking at the currently hidden teacher. Both are actual native gates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpistemicAnfPair {
    reference: WideAnfGate,
    alternative: WideAnfGate,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EpistemicPrediction {
    pub reference: bool,
    pub alternative: bool,
    pub disagreement: bool,
    pub probability_true: f64,
    /// Forced 0/1 action; carries unquantified risk under wrong priors.
    pub forced: bool,
    /// The selective mode **does not make a prediction** on disagreement.
    pub consensus_only: Option<bool>,
    pub needs_intervention: bool,
}

impl EpistemicAnfPair {
    pub fn new(
        reference: WideAnfGate,
        alternative: WideAnfGate,
    ) -> Result<Self, EpistemicAnfError> {
        if reference.fan_in() != alternative.fan_in() {
            return Err(EpistemicAnfError::UnequalFanIn);
        }
        Ok(Self {
            reference,
            alternative,
        })
    }

    #[must_use]
    pub fn fan_in(&self) -> usize {
        self.reference.fan_in()
    }

    pub fn predict(
        &self,
        input: &[bool],
        probability_alternative: f64,
    ) -> Result<EpistemicPrediction, EpistemicAnfError> {
        if !probability_alternative.is_finite()
            || !(0.0..=1.0).contains(&probability_alternative)
        {
            return Err(EpistemicAnfError::InvalidProbability);
        }
        if input.len() != self.fan_in() {
            return Err(EpistemicAnfError::InputWidth);
        }
        let a = self.reference.evaluate_scalar(input)?;
        let b = self.alternative.evaluate_scalar(input)?;
        let disagreement = a != b;
        let p_true = if a == b {
            f64::from(u8::from(a))
        } else if b {
            probability_alternative
        } else {
            1.0 - probability_alternative
        };
        Ok(EpistemicPrediction {
            reference: a,
            alternative: b,
            disagreement,
            probability_true: p_true,
            forced: p_true >= 0.5,
            consensus_only: if disagreement { None } else { Some(a) },
            needs_intervention: disagreement,
        })
    }

    /// Binary truth about two candidate functions is revealed by a real
    /// intervention, deliberately kept OUTSIDE the passive inference path.
    /// An odd number of repeated noisy readings uses strict majority.
    pub fn classify_intervention(
        &self,
        input: &[bool],
        repeated_labels: &[bool],
    ) -> Result<bool, EpistemicAnfError> {
        if repeated_labels.is_empty()
            || repeated_labels.len() % 2 == 0
            || repeated_labels.len() > 21
        {
            return Err(EpistemicAnfError::MissingIntervention);
        }
        let pair = self.predict(input, 0.5)?;
        if !pair.disagreement {
            return Err(EpistemicAnfError::NoDisagreement);
        }
        let yes = repeated_labels.iter().filter(|&&value| value).count();
        let majority = yes > repeated_labels.len() / 2;
        Ok(majority == pair.alternative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anf_wide_sparse::WideAnfLimits;

    fn budget() -> WideAnfLimits {
        WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
    }
    fn pair() -> EpistemicAnfPair {
        let a = WideAnfGate::from_monomials(
            (0..16).collect(),
            vec![vec![2, 3], vec![4, 5, 6]],
            16,
            budget(),
        )
        .unwrap();
        let b = WideAnfGate::from_monomials(
            (0..16).collect(),
            vec![vec![0, 1], vec![2, 3], vec![4, 5, 6]],
            16,
            budget(),
        )
        .unwrap();
        EpistemicAnfPair::new(a, b).unwrap()
    }

    #[test]
    fn two_distinct_anf_gates_are_observationally_identical_on_censored_support() {
        let model = pair();
        for i in 0..128usize {
            let mut x = (0..16)
                .map(|bit| i & (1 << (bit % 7)) != 0)
                .collect::<Vec<_>>();
            x[0] = (i & 1) != 0;
            x[1] = !x[0];
            let pred = model.predict(&x, 0.88).unwrap();
            assert!(!pred.disagreement);
            assert_eq!(pred.consensus_only, Some(pred.reference));
        }
        let mut adversary = vec![false; 16];
        adversary[0] = true;
        adversary[1] = true;
        let divergent = model.predict(&adversary, 0.88).unwrap();
        assert!(divergent.disagreement);
        assert_eq!(divergent.consensus_only, None);
        assert!(divergent.needs_intervention);
        assert!(divergent.forced);
        assert!((divergent.probability_true - 0.88).abs() < 1e-12);
    }

    #[test]
    fn prior_never_mistakes_task_context_for_ground_truth() {
        let mut table = ContextualPrior::default();
        assert_eq!(
            table.probability_alternative(0),
            Err(EpistemicAnfError::EmptyMetaHistory)
        );
        for _ in 0..7 {
            table.observe_historical(0, false).unwrap();
            table.observe_historical(1, true).unwrap();
        }
        table.observe_historical(0, true).unwrap();
        table.observe_historical(1, false).unwrap();
        assert!((table.probability_alternative(0).unwrap() - 0.2).abs() < 1e-12);
        assert!((table.probability_alternative(1).unwrap() - 0.8).abs() < 1e-12);
        assert!((table.pooled_probability_alternative().unwrap() - 0.5).abs() < 1e-12);
        assert_eq!(
            table.observe_historical(2, false),
            Err(EpistemicAnfError::Context)
        );
    }

    #[test]
    fn an_intervention_changes_epistemic_status_but_is_not_passive_inference() {
        let model = pair();
        let mut x = vec![false; 16];
        x[0] = true;
        x[1] = true;
        assert_eq!(model.classify_intervention(&x, &[true]), Ok(true));
        assert_eq!(model.classify_intervention(&x, &[false]), Ok(false));
        assert_eq!(model.classify_intervention(&x, &[true; 21]), Ok(true));
        assert_eq!(
            model.classify_intervention(&x, &[true, false]),
            Err(EpistemicAnfError::MissingIntervention)
        );
        x[1] = false;
        assert_eq!(
            model.classify_intervention(&x, &[true]),
            Err(EpistemicAnfError::NoDisagreement)
        );
        assert_eq!(
            model.predict(&x[..12], 0.5),
            Err(EpistemicAnfError::InputWidth)
        );
    }
}
