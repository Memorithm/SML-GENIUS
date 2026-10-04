//! DKV-4a: conservative adaptive recurrent-compute controller.
//!
//! This module decides whether to spend another bounded refinement cycle. It
//! deliberately does not expose an answer-acceptance action. Provenance state is
//! external to Delta-KV numeric evidence, and key dimension is not an input.

use core::fmt;

use crate::{mor::MOR_SCALE_PPM, semantic::SemanticStateDelta};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dkv4Provenance {
    VerifiedPresent,
    Missing,
    Conflict,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dkv4Signals {
    pub depth: u32,
    pub state_delta: SemanticStateDelta,
    pub ambiguous_values: usize,
    /// Maximum absolute correction signal. This may request more work but is
    /// never interpreted as presence, correctness, confidence, or provenance.
    pub correction_abs_max_q12: u32,
    pub mor_structural_disagreement_ppm: u32,
    pub mor_precision_escalation: bool,
    pub provenance: Dkv4Provenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dkv4Policy {
    pub min_depth: u32,
    pub max_depth: u32,
    pub stable_hamming_threshold: u64,
    pub correction_refine_threshold_q12: u32,
    pub mor_disagreement_refine_ppm: u32,
}

impl Default for Dkv4Policy {
    fn default() -> Self {
        Self {
            min_depth: 1,
            max_depth: 4,
            stable_hamming_threshold: 0,
            correction_refine_threshold_q12: 128,
            mor_disagreement_refine_ppm: 250_000,
        }
    }
}

impl Dkv4Policy {
    pub fn validate(self) -> Result<Self, Dkv4Error> {
        if self.min_depth == 0 || self.max_depth < self.min_depth {
            return Err(Dkv4Error::InvalidDepthRange {
                min: self.min_depth,
                max: self.max_depth,
            });
        }
        if self.mor_disagreement_refine_ppm > MOR_SCALE_PPM {
            return Err(Dkv4Error::InvalidPpm);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dkv4Reason {
    MissingEvidence,
    ConflictingEvidence,
    UnverifiedEvidence,
    MinimumDepth,
    AmbiguousMemory,
    CorrectionActive,
    MorDisagreement,
    MorPrecisionEscalation,
    StateChanging,
    Stable,
    BudgetExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dkv4Decision {
    Refine(Dkv4Reason),
    /// Computation may stop. This is explicitly not answer acceptance.
    StopCandidate(Dkv4Reason),
    Abstain(Dkv4Reason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dkv4Controller {
    policy: Dkv4Policy,
}

impl Dkv4Controller {
    pub fn new(policy: Dkv4Policy) -> Result<Self, Dkv4Error> {
        Ok(Self {
            policy: policy.validate()?,
        })
    }

    #[must_use]
    pub const fn policy(self) -> Dkv4Policy {
        self.policy
    }

    pub fn decide(self, signals: Dkv4Signals) -> Result<Dkv4Decision, Dkv4Error> {
        if signals.depth == 0 || signals.depth > self.policy.max_depth {
            return Err(Dkv4Error::InvalidDepth {
                depth: signals.depth,
                max: self.policy.max_depth,
            });
        }
        if signals.mor_structural_disagreement_ppm > MOR_SCALE_PPM {
            return Err(Dkv4Error::InvalidPpm);
        }

        match signals.provenance {
            Dkv4Provenance::Missing => {
                return Ok(Dkv4Decision::Abstain(Dkv4Reason::MissingEvidence))
            }
            Dkv4Provenance::Conflict => {
                return Ok(Dkv4Decision::Abstain(Dkv4Reason::ConflictingEvidence))
            }
            Dkv4Provenance::Unverified => {
                return Ok(if signals.depth < self.policy.max_depth {
                    Dkv4Decision::Refine(Dkv4Reason::UnverifiedEvidence)
                } else {
                    Dkv4Decision::Abstain(Dkv4Reason::UnverifiedEvidence)
                })
            }
            Dkv4Provenance::VerifiedPresent => {}
        }

        if signals.depth < self.policy.min_depth {
            return Ok(Dkv4Decision::Refine(Dkv4Reason::MinimumDepth));
        }

        let unresolved = self.unresolved_reason(signals);
        match unresolved {
            Some(reason) if signals.depth < self.policy.max_depth => {
                Ok(Dkv4Decision::Refine(reason))
            }
            Some(_) => Ok(Dkv4Decision::Abstain(Dkv4Reason::BudgetExhausted)),
            None => Ok(Dkv4Decision::StopCandidate(Dkv4Reason::Stable)),
        }
    }

    fn unresolved_reason(self, signals: Dkv4Signals) -> Option<Dkv4Reason> {
        if signals.ambiguous_values != 0 {
            return Some(Dkv4Reason::AmbiguousMemory);
        }
        if signals.mor_precision_escalation {
            return Some(Dkv4Reason::MorPrecisionEscalation);
        }
        if signals.mor_structural_disagreement_ppm > self.policy.mor_disagreement_refine_ppm {
            return Some(Dkv4Reason::MorDisagreement);
        }
        if signals.state_delta.total() > self.policy.stable_hamming_threshold as u128 {
            return Some(Dkv4Reason::StateChanging);
        }
        if signals.correction_abs_max_q12 > self.policy.correction_refine_threshold_q12 {
            return Some(Dkv4Reason::CorrectionActive);
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dkv4Error {
    InvalidDepthRange { min: u32, max: u32 },
    InvalidDepth { depth: u32, max: u32 },
    InvalidPpm,
}

impl fmt::Display for Dkv4Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Dkv4Error {}

#[cfg(test)]
mod tests {
    use super::*;

    fn stable(depth: u32, provenance: Dkv4Provenance) -> Dkv4Signals {
        Dkv4Signals {
            depth,
            state_delta: SemanticStateDelta::default(),
            ambiguous_values: 0,
            correction_abs_max_q12: 0,
            mor_structural_disagreement_ppm: 0,
            mor_precision_escalation: false,
            provenance,
        }
    }

    #[test]
    fn policy_and_signal_ranges_fail_closed() {
        assert!(matches!(
            Dkv4Controller::new(Dkv4Policy {
                min_depth: 0,
                ..Dkv4Policy::default()
            }),
            Err(Dkv4Error::InvalidDepthRange { .. })
        ));
        assert!(matches!(
            Dkv4Controller::new(Dkv4Policy {
                mor_disagreement_refine_ppm: MOR_SCALE_PPM + 1,
                ..Dkv4Policy::default()
            }),
            Err(Dkv4Error::InvalidPpm)
        ));
        let c = Dkv4Controller::new(Dkv4Policy::default()).unwrap();
        assert!(matches!(
            c.decide(stable(0, Dkv4Provenance::VerifiedPresent)),
            Err(Dkv4Error::InvalidDepth { .. })
        ));
    }

    #[test]
    fn missing_and_conflict_abstain_at_every_valid_depth() {
        let c = Dkv4Controller::new(Dkv4Policy::default()).unwrap();
        for depth in 1..=c.policy().max_depth {
            assert_eq!(
                c.decide(stable(depth, Dkv4Provenance::Missing)).unwrap(),
                Dkv4Decision::Abstain(Dkv4Reason::MissingEvidence)
            );
            assert_eq!(
                c.decide(stable(depth, Dkv4Provenance::Conflict)).unwrap(),
                Dkv4Decision::Abstain(Dkv4Reason::ConflictingEvidence)
            );
        }
    }

    #[test]
    fn unverified_never_stops_a_candidate() {
        let c = Dkv4Controller::new(Dkv4Policy::default()).unwrap();
        for depth in 1..=c.policy().max_depth {
            let d = c
                .decide(stable(depth, Dkv4Provenance::Unverified))
                .unwrap();
            assert!(!matches!(d, Dkv4Decision::StopCandidate(_)));
        }
    }

    #[test]
    fn verified_stable_can_stop() {
        let c = Dkv4Controller::new(Dkv4Policy::default()).unwrap();
        assert_eq!(
            c.decide(stable(1, Dkv4Provenance::VerifiedPresent))
                .unwrap(),
            Dkv4Decision::StopCandidate(Dkv4Reason::Stable)
        );
    }

    #[test]
    fn each_unresolved_signal_refines_then_abstains_at_budget() {
        let c = Dkv4Controller::new(Dkv4Policy::default()).unwrap();
        let mut cases = Vec::new();

        let mut s = stable(1, Dkv4Provenance::VerifiedPresent);
        s.ambiguous_values = 1;
        cases.push((s, Dkv4Reason::AmbiguousMemory));

        let mut s = stable(1, Dkv4Provenance::VerifiedPresent);
        s.mor_precision_escalation = true;
        cases.push((s, Dkv4Reason::MorPrecisionEscalation));

        let mut s = stable(1, Dkv4Provenance::VerifiedPresent);
        s.mor_structural_disagreement_ppm = 250_001;
        cases.push((s, Dkv4Reason::MorDisagreement));

        let mut s = stable(1, Dkv4Provenance::VerifiedPresent);
        s.state_delta.value = 1;
        cases.push((s, Dkv4Reason::StateChanging));

        let mut s = stable(1, Dkv4Provenance::VerifiedPresent);
        s.correction_abs_max_q12 = 129;
        cases.push((s, Dkv4Reason::CorrectionActive));

        for (signals, reason) in cases {
            assert_eq!(
                c.decide(signals).unwrap(),
                Dkv4Decision::Refine(reason)
            );
            let mut exhausted = signals;
            exhausted.depth = c.policy().max_depth;
            assert_eq!(
                c.decide(exhausted).unwrap(),
                Dkv4Decision::Abstain(Dkv4Reason::BudgetExhausted)
            );
        }
    }

    #[test]
    fn minimum_depth_precedes_stable_stop() {
        let c = Dkv4Controller::new(Dkv4Policy {
            min_depth: 2,
            ..Dkv4Policy::default()
        })
        .unwrap();
        assert_eq!(
            c.decide(stable(1, Dkv4Provenance::VerifiedPresent))
                .unwrap(),
            Dkv4Decision::Refine(Dkv4Reason::MinimumDepth)
        );
        assert_eq!(
            c.decide(stable(2, Dkv4Provenance::VerifiedPresent))
                .unwrap(),
            Dkv4Decision::StopCandidate(Dkv4Reason::Stable)
        );
    }

    #[test]
    fn correction_threshold_is_inclusive_for_stop() {
        let p = Dkv4Policy::default();
        let c = Dkv4Controller::new(p).unwrap();
        let mut s = stable(1, Dkv4Provenance::VerifiedPresent);
        s.correction_abs_max_q12 = p.correction_refine_threshold_q12;
        assert_eq!(
            c.decide(s).unwrap(),
            Dkv4Decision::StopCandidate(Dkv4Reason::Stable)
        );
    }
}
