//! DKV-4a: conservative adaptive recurrent-compute controller.
//!
//! This module decides whether to spend another bounded refinement cycle. It
//! deliberately does not expose an answer-acceptance action. Provenance state is
//! external to Delta-KV numeric evidence, and key dimension is not an input.

use core::fmt;

use crate::{
    delta_kv_recurrent::{
        DeltaKvRecurrentCycleEvidence, DeltaKvRecurrentError, DeltaKvRecurrentKernel,
    },
    mor::{MorDecision, MOR_SCALE_PPM},
    semantic::{SemanticRewriteTrace, SemanticRewriter, SemanticState, SemanticStateDelta},
};

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

impl Dkv4Signals {
    /// Builds DKV-4 compute evidence from one executed DKV-2b/SBG cycle and the
    /// already-produced MOR decision. Provenance remains an explicit external gate.
    pub fn from_dkv2b(
        trace: &SemanticRewriteTrace,
        cycle: &DeltaKvRecurrentCycleEvidence,
        mor: &MorDecision,
        provenance: Dkv4Provenance,
    ) -> Result<Self, Dkv4Error> {
        if trace.depth != cycle.depth {
            return Err(Dkv4Error::EvidenceDepthMismatch {
                trace: trace.depth,
                cycle: cycle.depth,
            });
        }
        if mor.structural_disagreement_ppm > MOR_SCALE_PPM {
            return Err(Dkv4Error::InvalidPpm);
        }
        Ok(Self {
            depth: trace.depth,
            state_delta: trace.delta,
            ambiguous_values: cycle.ambiguous_values,
            correction_abs_max_q12: cycle.correction_abs_max_q12,
            mor_structural_disagreement_ppm: mor.structural_disagreement_ppm,
            mor_precision_escalation: mor.request_precision_escalation,
            provenance,
        })
    }
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
pub struct Dkv4SemanticPolicy {
    pub min_depth: u32,
    pub max_depth: u32,
    pub stable_hamming_threshold: u64,
    pub mor_disagreement_refine_ppm: u32,
}

impl Default for Dkv4SemanticPolicy {
    fn default() -> Self {
        Self {
            min_depth: 1,
            max_depth: 4,
            stable_hamming_threshold: 0,
            mor_disagreement_refine_ppm: 250_000,
        }
    }
}

impl Dkv4SemanticPolicy {
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

/// DKV-4c: semantic-impact controller.
///
/// Correction magnitude stays observable through Dkv4Signals but does not
/// independently request another cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dkv4SemanticController {
    policy: Dkv4SemanticPolicy,
}

impl Dkv4SemanticController {
    pub fn new(policy: Dkv4SemanticPolicy) -> Result<Self, Dkv4Error> {
        Ok(Self {
            policy: policy.validate()?,
        })
    }

    #[must_use]
    pub const fn policy(self) -> Dkv4SemanticPolicy {
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
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dkv4RefinementCapabilities {
    /// True only when another admitted recurrent cycle can in principle change
    /// the evidence from which the supplied MOR decision is derived.
    pub mor_evidence_can_change: bool,
}

impl Dkv4RefinementCapabilities {
    /// Current DKV-2b frozen-key composition: V/A may change, R does not.
    #[must_use]
    pub const fn dkv2b_fixed_mor() -> Self {
        Self {
            mor_evidence_can_change: false,
        }
    }

    /// Explicitly declares a composition where MOR evidence can change between
    /// admitted recurrent cycles.
    #[must_use]
    pub const fn dynamic_mor() -> Self {
        Self {
            mor_evidence_can_change: true,
        }
    }
}

/// DKV-4d: semantic-impact control with explicit signal actionability.
///
/// This keeps DKV-4c's fail-closed provenance and semantic-impact behavior, but
/// invariant MOR evidence is telemetry rather than an unconditional request for
/// another DKV-2b cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dkv4ActionableController {
    policy: Dkv4SemanticPolicy,
}

impl Dkv4ActionableController {
    pub fn new(policy: Dkv4SemanticPolicy) -> Result<Self, Dkv4Error> {
        Ok(Self {
            policy: policy.validate()?,
        })
    }

    #[must_use]
    pub const fn policy(self) -> Dkv4SemanticPolicy {
        self.policy
    }

    pub fn decide(
        self,
        signals: Dkv4Signals,
        capabilities: Dkv4RefinementCapabilities,
    ) -> Result<Dkv4Decision, Dkv4Error> {
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

        let unresolved = self.unresolved_reason(signals, capabilities);
        match unresolved {
            Some(reason) if signals.depth < self.policy.max_depth => {
                Ok(Dkv4Decision::Refine(reason))
            }
            Some(_) => Ok(Dkv4Decision::Abstain(Dkv4Reason::BudgetExhausted)),
            None => Ok(Dkv4Decision::StopCandidate(Dkv4Reason::Stable)),
        }
    }

    fn unresolved_reason(
        self,
        signals: Dkv4Signals,
        capabilities: Dkv4RefinementCapabilities,
    ) -> Option<Dkv4Reason> {
        if signals.ambiguous_values != 0 {
            return Some(Dkv4Reason::AmbiguousMemory);
        }
        if signals.state_delta.total() > self.policy.stable_hamming_threshold as u128 {
            return Some(Dkv4Reason::StateChanging);
        }
        if capabilities.mor_evidence_can_change {
            if signals.mor_precision_escalation {
                return Some(Dkv4Reason::MorPrecisionEscalation);
            }
            if signals.mor_structural_disagreement_ppm > self.policy.mor_disagreement_refine_ppm {
                return Some(Dkv4Reason::MorDisagreement);
            }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dkv4RunOutcome<const BITS: usize> {
    pub state: SemanticState<BITS>,
    pub depth: u32,
    pub decision: Dkv4Decision,
    pub trace: Vec<SemanticRewriteTrace>,
    pub cycles: Vec<DeltaKvRecurrentCycleEvidence>,
}

/// Executes DKV-2b under DKV-4 compute control. Missing/conflicting provenance
/// abstains before the first matrix read. StopCandidate remains non-authoritative.
pub fn run_dkv4_recurrent<const BITS: usize>(
    initial: SemanticState<BITS>,
    kernel: &mut DeltaKvRecurrentKernel<BITS>,
    mor: &MorDecision,
    provenance: Dkv4Provenance,
    controller: Dkv4Controller,
) -> Result<Dkv4RunOutcome<BITS>, Dkv4RunError> {
    let preflight = match provenance {
        Dkv4Provenance::Missing => Some(Dkv4Decision::Abstain(Dkv4Reason::MissingEvidence)),
        Dkv4Provenance::Conflict => Some(Dkv4Decision::Abstain(Dkv4Reason::ConflictingEvidence)),
        Dkv4Provenance::VerifiedPresent | Dkv4Provenance::Unverified => None,
    };
    if let Some(decision) = preflight {
        return Ok(Dkv4RunOutcome {
            state: initial,
            depth: 0,
            decision,
            trace: Vec::new(),
            cycles: Vec::new(),
        });
    }

    let mut state = initial;
    let mut trace = Vec::with_capacity(controller.policy().max_depth as usize);
    let cycle_start = kernel.cycle_evidence().len();

    for depth in 1..=controller.policy().max_depth {
        let before = kernel.cycle_evidence().len();
        let step = kernel
            .rewrite(&state, depth)
            .map_err(Dkv4RunError::Kernel)?;
        if kernel.cycle_evidence().len() != before.saturating_add(1) {
            return Err(Dkv4RunError::InternalInvariant);
        }
        let entry = SemanticRewriteTrace {
            depth,
            delta: SemanticStateDelta::between(&state, &step.state),
            stability_blocked: step.stability_blocked,
        };
        let cycle = kernel
            .cycle_evidence()
            .last()
            .ok_or(Dkv4RunError::InternalInvariant)?;
        let signals = Dkv4Signals::from_dkv2b(&entry, cycle, mor, provenance)
            .map_err(Dkv4RunError::Controller)?;
        let decision = controller
            .decide(signals)
            .map_err(Dkv4RunError::Controller)?;
        trace.push(entry);
        state = step.state;

        if !matches!(decision, Dkv4Decision::Refine(_)) {
            return Ok(Dkv4RunOutcome {
                state,
                depth,
                decision,
                trace,
                cycles: kernel.cycle_evidence()[cycle_start..].to_vec(),
            });
        }
    }

    Err(Dkv4RunError::InternalInvariant)
}

/// Executes DKV-2b under DKV-4c semantic-impact control.
///
/// Missing/conflicting provenance still abstains before the first matrix read.
/// Correction magnitude is retained in cycle evidence but never independently
/// forces another recurrent cycle.
pub fn run_dkv4_semantic_recurrent<const BITS: usize>(
    initial: SemanticState<BITS>,
    kernel: &mut DeltaKvRecurrentKernel<BITS>,
    mor: &MorDecision,
    provenance: Dkv4Provenance,
    controller: Dkv4SemanticController,
) -> Result<Dkv4RunOutcome<BITS>, Dkv4RunError> {
    let preflight = match provenance {
        Dkv4Provenance::Missing => Some(Dkv4Decision::Abstain(Dkv4Reason::MissingEvidence)),
        Dkv4Provenance::Conflict => Some(Dkv4Decision::Abstain(Dkv4Reason::ConflictingEvidence)),
        Dkv4Provenance::VerifiedPresent | Dkv4Provenance::Unverified => None,
    };
    if let Some(decision) = preflight {
        return Ok(Dkv4RunOutcome {
            state: initial,
            depth: 0,
            decision,
            trace: Vec::new(),
            cycles: Vec::new(),
        });
    }

    let mut state = initial;
    let mut trace = Vec::with_capacity(controller.policy().max_depth as usize);
    let cycle_start = kernel.cycle_evidence().len();

    for depth in 1..=controller.policy().max_depth {
        let before = kernel.cycle_evidence().len();
        let step = kernel
            .rewrite(&state, depth)
            .map_err(Dkv4RunError::Kernel)?;
        if kernel.cycle_evidence().len() != before.saturating_add(1) {
            return Err(Dkv4RunError::InternalInvariant);
        }
        let entry = SemanticRewriteTrace {
            depth,
            delta: SemanticStateDelta::between(&state, &step.state),
            stability_blocked: step.stability_blocked,
        };
        let cycle = kernel
            .cycle_evidence()
            .last()
            .ok_or(Dkv4RunError::InternalInvariant)?;
        let signals = Dkv4Signals::from_dkv2b(&entry, cycle, mor, provenance)
            .map_err(Dkv4RunError::Controller)?;
        let decision = controller
            .decide(signals)
            .map_err(Dkv4RunError::Controller)?;
        trace.push(entry);
        state = step.state;

        if !matches!(decision, Dkv4Decision::Refine(_)) {
            return Ok(Dkv4RunOutcome {
                state,
                depth,
                decision,
                trace,
                cycles: kernel.cycle_evidence()[cycle_start..].to_vec(),
            });
        }
    }

    Err(Dkv4RunError::InternalInvariant)
}

/// Executes DKV-2b under DKV-4d actionability-aware control.
///
/// The supplied capability declaration is part of the composition contract.
/// It is not inferred from task outcomes or answer correctness.
pub fn run_dkv4_actionable_recurrent<const BITS: usize>(
    initial: SemanticState<BITS>,
    kernel: &mut DeltaKvRecurrentKernel<BITS>,
    mor: &MorDecision,
    provenance: Dkv4Provenance,
    controller: Dkv4ActionableController,
    capabilities: Dkv4RefinementCapabilities,
) -> Result<Dkv4RunOutcome<BITS>, Dkv4RunError> {
    let preflight = match provenance {
        Dkv4Provenance::Missing => Some(Dkv4Decision::Abstain(Dkv4Reason::MissingEvidence)),
        Dkv4Provenance::Conflict => Some(Dkv4Decision::Abstain(Dkv4Reason::ConflictingEvidence)),
        Dkv4Provenance::VerifiedPresent | Dkv4Provenance::Unverified => None,
    };
    if let Some(decision) = preflight {
        return Ok(Dkv4RunOutcome {
            state: initial,
            depth: 0,
            decision,
            trace: Vec::new(),
            cycles: Vec::new(),
        });
    }

    let mut state = initial;
    let mut trace = Vec::with_capacity(controller.policy().max_depth as usize);
    let cycle_start = kernel.cycle_evidence().len();

    for depth in 1..=controller.policy().max_depth {
        let before = kernel.cycle_evidence().len();
        let step = kernel
            .rewrite(&state, depth)
            .map_err(Dkv4RunError::Kernel)?;
        if kernel.cycle_evidence().len() != before.saturating_add(1) {
            return Err(Dkv4RunError::InternalInvariant);
        }
        let entry = SemanticRewriteTrace {
            depth,
            delta: SemanticStateDelta::between(&state, &step.state),
            stability_blocked: step.stability_blocked,
        };
        let cycle = kernel
            .cycle_evidence()
            .last()
            .ok_or(Dkv4RunError::InternalInvariant)?;
        let signals = Dkv4Signals::from_dkv2b(&entry, cycle, mor, provenance)
            .map_err(Dkv4RunError::Controller)?;
        let decision = controller
            .decide(signals, capabilities)
            .map_err(Dkv4RunError::Controller)?;
        trace.push(entry);
        state = step.state;

        if !matches!(decision, Dkv4Decision::Refine(_)) {
            return Ok(Dkv4RunOutcome {
                state,
                depth,
                decision,
                trace,
                cycles: kernel.cycle_evidence()[cycle_start..].to_vec(),
            });
        }
    }

    Err(Dkv4RunError::InternalInvariant)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dkv4RunError {
    Controller(Dkv4Error),
    Kernel(DeltaKvRecurrentError),
    InternalInvariant,
}

impl fmt::Display for Dkv4RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Dkv4RunError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dkv4Error {
    InvalidDepthRange { min: u32, max: u32 },
    InvalidDepth { depth: u32, max: u32 },
    InvalidPpm,
    EvidenceDepthMismatch { trace: u32, cycle: u32 },
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
    use crate::{
        delta_kv::{DeltaKvMemory, DELTA_KV_ONE},
        delta_kv_recurrent::DeltaKvRecurrentPolicy,
        delta_kv_semantic::{DeltaKvSemanticKeyEvidence, DeltaKvSemanticProjectionCounters},
        PageId,
    };

    fn mor() -> MorDecision {
        MorDecision {
            active_routers: Vec::new(),
            ranked_experts: Vec::new(),
            selected_experts: vec![PageId(1)],
            noise_ppm: 0,
            structural_disagreement_ppm: 0,
            expert_mass_threshold_ppm: 0,
            request_precision_escalation: false,
            hysteresis_retained_previous_primary: false,
        }
    }

    fn kernel() -> DeltaKvRecurrentKernel<8> {
        let memory = DeltaKvMemory::new(1, 1).unwrap();
        let key = DeltaKvSemanticKeyEvidence {
            key_q12: vec![DELTA_KV_ONE],
            selected_experts: vec![PageId(1)],
            provenance: None,
            counters: DeltaKvSemanticProjectionCounters::default(),
            transient_key_bits: 16,
        };
        let mut kernel =
            DeltaKvRecurrentKernel::new(memory, key, vec![0], DeltaKvRecurrentPolicy::default())
                .unwrap();
        kernel.seed_target(&[DELTA_KV_ONE], 4096).unwrap();
        kernel
    }

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
            let d = c.decide(stable(depth, Dkv4Provenance::Unverified)).unwrap();
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
            assert_eq!(c.decide(signals).unwrap(), Dkv4Decision::Refine(reason));
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
    fn dkv2b_adapter_preserves_executed_cycle_signals() {
        let trace = SemanticRewriteTrace {
            depth: 2,
            delta: SemanticStateDelta {
                value: 1,
                meaning_mask: 2,
                ambiguity: 3,
                routing: 4,
            },
            stability_blocked: true,
        };
        let cycle = DeltaKvRecurrentCycleEvidence {
            depth: 2,
            read_values_q12: vec![4096, -4096],
            ambiguous_values: 1,
            correction_abs_max_q12: 8192,
            read_macs_delta: 16,
        };
        let mor = MorDecision {
            active_routers: Vec::new(),
            ranked_experts: Vec::new(),
            selected_experts: vec![crate::PageId(1)],
            noise_ppm: 0,
            structural_disagreement_ppm: 42,
            expert_mass_threshold_ppm: 0,
            request_precision_escalation: true,
            hysteresis_retained_previous_primary: false,
        };
        let signals =
            Dkv4Signals::from_dkv2b(&trace, &cycle, &mor, Dkv4Provenance::VerifiedPresent).unwrap();
        assert_eq!(signals.depth, 2);
        assert_eq!(signals.state_delta.total(), 10);
        assert_eq!(signals.ambiguous_values, 1);
        assert_eq!(signals.correction_abs_max_q12, 8192);
        assert_eq!(signals.mor_structural_disagreement_ppm, 42);
        assert!(signals.mor_precision_escalation);
    }

    #[test]
    fn dkv2b_adapter_rejects_misaligned_cycle_evidence() {
        let trace = SemanticRewriteTrace {
            depth: 1,
            delta: SemanticStateDelta::default(),
            stability_blocked: false,
        };
        let cycle = DeltaKvRecurrentCycleEvidence {
            depth: 2,
            read_values_q12: vec![],
            ambiguous_values: 0,
            correction_abs_max_q12: 0,
            read_macs_delta: 0,
        };
        let mor = MorDecision {
            active_routers: Vec::new(),
            ranked_experts: Vec::new(),
            selected_experts: vec![crate::PageId(1)],
            noise_ppm: 0,
            structural_disagreement_ppm: 0,
            expert_mass_threshold_ppm: 0,
            request_precision_escalation: false,
            hysteresis_retained_previous_primary: false,
        };
        assert!(matches!(
            Dkv4Signals::from_dkv2b(&trace, &cycle, &mor, Dkv4Provenance::VerifiedPresent),
            Err(Dkv4Error::EvidenceDepthMismatch { trace: 1, cycle: 2 })
        ));
    }

    #[test]
    fn missing_provenance_abstains_before_any_recurrent_read() {
        let mut k = kernel();
        let before = k.cycle_evidence().len();
        let outcome = run_dkv4_recurrent(
            SemanticState::<8>::zeroed().unwrap(),
            &mut k,
            &mor(),
            Dkv4Provenance::Missing,
            Dkv4Controller::new(Dkv4Policy::default()).unwrap(),
        )
        .unwrap();
        assert_eq!(outcome.depth, 0);
        assert_eq!(
            outcome.decision,
            Dkv4Decision::Abstain(Dkv4Reason::MissingEvidence)
        );
        assert!(outcome.trace.is_empty());
        assert!(outcome.cycles.is_empty());
        assert_eq!(k.cycle_evidence().len(), before);
    }

    #[test]
    fn verified_memory_correction_refines_once_then_stops() {
        let mut k = kernel();
        let initial = SemanticState::<8>::zeroed().unwrap();
        let outcome = run_dkv4_recurrent(
            initial,
            &mut k,
            &mor(),
            Dkv4Provenance::VerifiedPresent,
            Dkv4Controller::new(Dkv4Policy::default()).unwrap(),
        )
        .unwrap();
        assert_eq!(outcome.depth, 2);
        assert_eq!(
            outcome.decision,
            Dkv4Decision::StopCandidate(Dkv4Reason::Stable)
        );
        assert_eq!(outcome.trace.len(), 2);
        assert_eq!(outcome.trace[0].delta.value, 1);
        assert!(outcome.trace[1].delta.is_zero());
        assert_eq!(outcome.cycles[0].correction_abs_max_q12, 8192);
        assert_eq!(outcome.cycles[1].correction_abs_max_q12, 0);
        assert!(outcome.state.value.get(0).unwrap());
    }

    #[test]
    fn verified_already_matching_state_stops_after_one_cycle() {
        let mut k = kernel();
        let mut initial = SemanticState::<8>::zeroed().unwrap();
        initial.value.set(0, true).unwrap();
        let outcome = run_dkv4_recurrent(
            initial,
            &mut k,
            &mor(),
            Dkv4Provenance::VerifiedPresent,
            Dkv4Controller::new(Dkv4Policy::default()).unwrap(),
        )
        .unwrap();
        assert_eq!(outcome.depth, 1);
        assert_eq!(
            outcome.decision,
            Dkv4Decision::StopCandidate(Dkv4Reason::Stable)
        );
        assert_eq!(outcome.cycles[0].correction_abs_max_q12, 0);
    }

    #[test]
    fn unverified_path_exhausts_budget_without_stopping_candidate() {
        let mut k = kernel();
        let mut initial = SemanticState::<8>::zeroed().unwrap();
        initial.value.set(0, true).unwrap();
        let policy = Dkv4Policy {
            max_depth: 3,
            ..Dkv4Policy::default()
        };
        let outcome = run_dkv4_recurrent(
            initial,
            &mut k,
            &mor(),
            Dkv4Provenance::Unverified,
            Dkv4Controller::new(policy).unwrap(),
        )
        .unwrap();
        assert_eq!(outcome.depth, 3);
        assert_eq!(
            outcome.decision,
            Dkv4Decision::Abstain(Dkv4Reason::UnverifiedEvidence)
        );
        assert_eq!(outcome.cycles.len(), 3);
    }

    fn kernel_with_positive_target(target_q12: i16) -> DeltaKvRecurrentKernel<8> {
        let memory = DeltaKvMemory::new(1, 1).unwrap();
        let key = DeltaKvSemanticKeyEvidence {
            key_q12: vec![DELTA_KV_ONE],
            selected_experts: vec![PageId(1)],
            provenance: None,
            counters: DeltaKvSemanticProjectionCounters::default(),
            transient_key_bits: 16,
        };
        let mut kernel =
            DeltaKvRecurrentKernel::new(memory, key, vec![0], DeltaKvRecurrentPolicy::default())
                .unwrap();
        kernel.seed_target(&[target_q12], 4096).unwrap();
        kernel
    }

    #[test]
    fn semantic_controller_ignores_analog_residual_when_state_is_stable() {
        let c = Dkv4SemanticController::new(Dkv4SemanticPolicy::default()).unwrap();
        let mut signals = stable(1, Dkv4Provenance::VerifiedPresent);
        signals.correction_abs_max_q12 = 4095;
        assert_eq!(
            c.decide(signals).unwrap(),
            Dkv4Decision::StopCandidate(Dkv4Reason::Stable)
        );
    }

    #[test]
    fn semantic_controller_preserves_ambiguity_and_mor_refinement_signals() {
        let c = Dkv4SemanticController::new(Dkv4SemanticPolicy::default()).unwrap();

        let mut ambiguous = stable(1, Dkv4Provenance::VerifiedPresent);
        ambiguous.ambiguous_values = 1;
        assert_eq!(
            c.decide(ambiguous).unwrap(),
            Dkv4Decision::Refine(Dkv4Reason::AmbiguousMemory)
        );

        let mut mor_escalation = stable(1, Dkv4Provenance::VerifiedPresent);
        mor_escalation.mor_precision_escalation = true;
        assert_eq!(
            c.decide(mor_escalation).unwrap(),
            Dkv4Decision::Refine(Dkv4Reason::MorPrecisionEscalation)
        );

        let mut disagreement = stable(1, Dkv4Provenance::VerifiedPresent);
        disagreement.mor_structural_disagreement_ppm = 250_001;
        assert_eq!(
            c.decide(disagreement).unwrap(),
            Dkv4Decision::Refine(Dkv4Reason::MorDisagreement)
        );
    }

    #[test]
    fn semantic_runner_stops_on_stable_boolean_value_despite_analog_residual() {
        let mut initial = SemanticState::<8>::zeroed().unwrap();
        initial.value.set(0, true).unwrap();

        let mut old_kernel = kernel_with_positive_target(3000);
        let old = run_dkv4_recurrent(
            initial.clone(),
            &mut old_kernel,
            &mor(),
            Dkv4Provenance::VerifiedPresent,
            Dkv4Controller::new(Dkv4Policy::default()).unwrap(),
        )
        .unwrap();
        assert_eq!(old.depth, 4);
        assert_eq!(
            old.decision,
            Dkv4Decision::Abstain(Dkv4Reason::BudgetExhausted)
        );
        assert!(old
            .cycles
            .iter()
            .all(|cycle| cycle.correction_abs_max_q12 > 128));

        let mut semantic_kernel = kernel_with_positive_target(3000);
        let semantic = run_dkv4_semantic_recurrent(
            initial,
            &mut semantic_kernel,
            &mor(),
            Dkv4Provenance::VerifiedPresent,
            Dkv4SemanticController::new(Dkv4SemanticPolicy::default()).unwrap(),
        )
        .unwrap();
        assert_eq!(semantic.depth, 1);
        assert_eq!(
            semantic.decision,
            Dkv4Decision::StopCandidate(Dkv4Reason::Stable)
        );
        assert!(semantic.cycles[0].correction_abs_max_q12 > 128);
        assert!(semantic.state.value.get(0).unwrap());
    }

    #[test]
    fn semantic_runner_missing_provenance_still_performs_zero_reads() {
        let mut k = kernel();
        let before = k.counters().reads;
        let outcome = run_dkv4_semantic_recurrent(
            SemanticState::<8>::zeroed().unwrap(),
            &mut k,
            &mor(),
            Dkv4Provenance::Missing,
            Dkv4SemanticController::new(Dkv4SemanticPolicy::default()).unwrap(),
        )
        .unwrap();
        assert_eq!(outcome.depth, 0);
        assert_eq!(k.counters().reads, before);
        assert_eq!(
            outcome.decision,
            Dkv4Decision::Abstain(Dkv4Reason::MissingEvidence)
        );
    }

    #[test]
    fn semantic_runner_unverified_never_stops_candidate() {
        let mut k = kernel();
        let mut initial = SemanticState::<8>::zeroed().unwrap();
        initial.value.set(0, true).unwrap();
        let outcome = run_dkv4_semantic_recurrent(
            initial,
            &mut k,
            &mor(),
            Dkv4Provenance::Unverified,
            Dkv4SemanticController::new(Dkv4SemanticPolicy {
                max_depth: 3,
                ..Dkv4SemanticPolicy::default()
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(outcome.depth, 3);
        assert_eq!(
            outcome.decision,
            Dkv4Decision::Abstain(Dkv4Reason::UnverifiedEvidence)
        );
    }

    #[test]
    fn actionable_controller_ignores_invariant_mor_signal() {
        let controller = Dkv4ActionableController::new(Dkv4SemanticPolicy::default()).unwrap();
        let mut signals = stable(1, Dkv4Provenance::VerifiedPresent);
        signals.mor_precision_escalation = true;
        signals.mor_structural_disagreement_ppm = 500_000;

        assert_eq!(
            controller
                .decide(signals, Dkv4RefinementCapabilities::dkv2b_fixed_mor())
                .unwrap(),
            Dkv4Decision::StopCandidate(Dkv4Reason::Stable)
        );
        assert_eq!(
            controller
                .decide(signals, Dkv4RefinementCapabilities::dynamic_mor())
                .unwrap(),
            Dkv4Decision::Refine(Dkv4Reason::MorPrecisionEscalation)
        );
    }

    #[test]
    fn actionable_controller_keeps_semantic_change_and_ambiguity_active() {
        let controller = Dkv4ActionableController::new(Dkv4SemanticPolicy::default()).unwrap();
        let caps = Dkv4RefinementCapabilities::dkv2b_fixed_mor();

        let mut changing = stable(1, Dkv4Provenance::VerifiedPresent);
        changing.state_delta.value = 1;
        changing.mor_precision_escalation = true;
        assert_eq!(
            controller.decide(changing, caps).unwrap(),
            Dkv4Decision::Refine(Dkv4Reason::StateChanging)
        );

        let mut ambiguous = stable(1, Dkv4Provenance::VerifiedPresent);
        ambiguous.ambiguous_values = 1;
        assert_eq!(
            controller.decide(ambiguous, caps).unwrap(),
            Dkv4Decision::Refine(Dkv4Reason::AmbiguousMemory)
        );
    }

    #[test]
    fn actionable_runner_fixed_mor_stops_when_semantic_state_stabilizes() {
        let mut initial = SemanticState::<8>::zeroed().unwrap();
        initial.value.set(0, true).unwrap();
        let mut disagreement = mor();
        disagreement.structural_disagreement_ppm = 500_000;
        disagreement.request_precision_escalation = true;

        let mut semantic_kernel = kernel();
        let semantic = run_dkv4_semantic_recurrent(
            initial.clone(),
            &mut semantic_kernel,
            &disagreement,
            Dkv4Provenance::VerifiedPresent,
            Dkv4SemanticController::new(Dkv4SemanticPolicy::default()).unwrap(),
        )
        .unwrap();
        assert_eq!(semantic.depth, 4);
        assert_eq!(
            semantic.decision,
            Dkv4Decision::Abstain(Dkv4Reason::BudgetExhausted)
        );

        let mut actionable_kernel = kernel();
        let actionable = run_dkv4_actionable_recurrent(
            initial,
            &mut actionable_kernel,
            &disagreement,
            Dkv4Provenance::VerifiedPresent,
            Dkv4ActionableController::new(Dkv4SemanticPolicy::default()).unwrap(),
            Dkv4RefinementCapabilities::dkv2b_fixed_mor(),
        )
        .unwrap();
        assert_eq!(actionable.depth, 1);
        assert_eq!(
            actionable.decision,
            Dkv4Decision::StopCandidate(Dkv4Reason::Stable)
        );
    }

    #[test]
    fn actionable_runner_dynamic_mor_reproduces_semantic_controller_depth() {
        let mut initial = SemanticState::<8>::zeroed().unwrap();
        initial.value.set(0, true).unwrap();
        let mut disagreement = mor();
        disagreement.structural_disagreement_ppm = 500_000;
        disagreement.request_precision_escalation = true;

        let mut semantic_kernel = kernel();
        let semantic = run_dkv4_semantic_recurrent(
            initial.clone(),
            &mut semantic_kernel,
            &disagreement,
            Dkv4Provenance::VerifiedPresent,
            Dkv4SemanticController::new(Dkv4SemanticPolicy::default()).unwrap(),
        )
        .unwrap();

        let mut actionable_kernel = kernel();
        let actionable = run_dkv4_actionable_recurrent(
            initial,
            &mut actionable_kernel,
            &disagreement,
            Dkv4Provenance::VerifiedPresent,
            Dkv4ActionableController::new(Dkv4SemanticPolicy::default()).unwrap(),
            Dkv4RefinementCapabilities::dynamic_mor(),
        )
        .unwrap();

        assert_eq!(actionable.depth, semantic.depth);
        assert_eq!(actionable.decision, semantic.decision);
        assert_eq!(actionable.state, semantic.state);
    }

    #[test]
    fn actionable_runner_preserves_fail_closed_provenance() {
        let controller = Dkv4ActionableController::new(Dkv4SemanticPolicy::default()).unwrap();
        let caps = Dkv4RefinementCapabilities::dkv2b_fixed_mor();

        for provenance in [Dkv4Provenance::Missing, Dkv4Provenance::Conflict] {
            let mut k = kernel();
            let before = k.counters().reads;
            let outcome = run_dkv4_actionable_recurrent(
                SemanticState::<8>::zeroed().unwrap(),
                &mut k,
                &mor(),
                provenance,
                controller,
                caps,
            )
            .unwrap();
            assert_eq!(outcome.depth, 0);
            assert_eq!(k.counters().reads, before);
            assert!(matches!(outcome.decision, Dkv4Decision::Abstain(_)));
        }

        let mut k = kernel();
        let outcome = run_dkv4_actionable_recurrent(
            SemanticState::<8>::zeroed().unwrap(),
            &mut k,
            &mor(),
            Dkv4Provenance::Unverified,
            controller,
            caps,
        )
        .unwrap();
        assert!(!matches!(
            outcome.decision,
            Dkv4Decision::StopCandidate(_)
        ));
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
