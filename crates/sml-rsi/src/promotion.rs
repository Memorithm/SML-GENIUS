use core::fmt;
use std::collections::BTreeSet;

use crate::manifest::{
    CandidateManifestError, EvidenceKind, RegressionSeverity, RsiCandidateManifest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionPolicy {
    pub required_evidence: BTreeSet<EvidenceKind>,
    pub reject_critical_known_regressions: bool,
    pub require_protected_evaluation: bool,
}

impl PromotionPolicy {
    #[must_use]
    pub fn baseline() -> Self {
        Self {
            required_evidence: [
                EvidenceKind::Build,
                EvidenceKind::RequiredTests,
                EvidenceKind::Determinism,
            ]
            .into_iter()
            .collect(),
            reject_critical_known_regressions: true,
            require_protected_evaluation: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionReviewContext {
    /// Current repository champion SHA observed by the trusted GitHub adapter.
    pub current_main_sha: String,
    /// Exact PR head SHA observed by the trusted GitHub adapter.
    pub pr_head_sha: String,
    /// Evidence digests used during candidate selection/tuning.
    pub selection_evidence_sha256: BTreeSet<String>,
    /// Evidence digests held back until after the candidate was frozen.
    pub protected_evidence_sha256: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionStatus {
    EligibleForHumanReview,
    Ineligible,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PromotionIneligibility {
    InvalidManifest,
    StaleParent,
    HeadMismatch,
    MissingRequiredEvidence(EvidenceKind),
    HardGateRegression,
    CriticalKnownRegression,
    MissingProtectedEvaluation,
    UnknownSelectionEvidence,
    UnknownProtectedEvidence,
    ProtectedEvidenceUsedForSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionEligibilityReport {
    pub status: PromotionStatus,
    pub reasons: Vec<PromotionIneligibility>,
    pub current_main_sha: String,
    pub candidate_head_sha: String,
    pub checked_evidence: usize,
    pub protected_evidence: usize,
}

impl PromotionEligibilityReport {
    #[must_use]
    pub fn eligible_for_human_review(&self) -> bool {
        self.status == PromotionStatus::EligibleForHumanReview
    }

    /// Deliberately no merge/promote operation exists here.
    #[must_use]
    pub const fn is_merge_authorization(&self) -> bool {
        false
    }
}

pub fn evaluate_promotion_eligibility(
    manifest: &RsiCandidateManifest,
    context: &PromotionReviewContext,
    policy: &PromotionPolicy,
) -> PromotionEligibilityReport {
    let mut reasons = BTreeSet::new();

    if manifest.validate().is_err() {
        reasons.insert(PromotionIneligibility::InvalidManifest);
    }

    if manifest.parent_main_sha != context.current_main_sha {
        reasons.insert(PromotionIneligibility::StaleParent);
    }
    if manifest.candidate_head_sha != context.pr_head_sha {
        reasons.insert(PromotionIneligibility::HeadMismatch);
    }

    let manifest_digests = manifest
        .evidence
        .iter()
        .map(|evidence| evidence.sha256.as_str())
        .collect::<BTreeSet<_>>();
    let manifest_kinds = manifest
        .evidence
        .iter()
        .map(|evidence| evidence.kind)
        .collect::<BTreeSet<_>>();

    for required in &policy.required_evidence {
        if !manifest_kinds.contains(required) {
            reasons.insert(PromotionIneligibility::MissingRequiredEvidence(*required));
        }
    }

    if manifest.hard_gate_regressions() != 0 {
        reasons.insert(PromotionIneligibility::HardGateRegression);
    }

    if policy.reject_critical_known_regressions
        && manifest
            .known_regressions
            .iter()
            .any(|item| item.severity == RegressionSeverity::Critical)
    {
        reasons.insert(PromotionIneligibility::CriticalKnownRegression);
    }

    if policy.require_protected_evaluation && context.protected_evidence_sha256.is_empty() {
        reasons.insert(PromotionIneligibility::MissingProtectedEvaluation);
    }

    for digest in &context.selection_evidence_sha256 {
        if !manifest_digests.contains(digest.as_str()) {
            reasons.insert(PromotionIneligibility::UnknownSelectionEvidence);
        }
    }
    for digest in &context.protected_evidence_sha256 {
        if !manifest_digests.contains(digest.as_str()) {
            reasons.insert(PromotionIneligibility::UnknownProtectedEvidence);
        }
        if context.selection_evidence_sha256.contains(digest) {
            reasons.insert(PromotionIneligibility::ProtectedEvidenceUsedForSelection);
        }
    }

    let reasons = reasons.into_iter().collect::<Vec<_>>();
    PromotionEligibilityReport {
        status: if reasons.is_empty() {
            PromotionStatus::EligibleForHumanReview
        } else {
            PromotionStatus::Ineligible
        },
        reasons,
        current_main_sha: context.current_main_sha.clone(),
        candidate_head_sha: context.pr_head_sha.clone(),
        checked_evidence: manifest.evidence.len(),
        protected_evidence: context.protected_evidence_sha256.len(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromotionVerifierError {
    InvalidManifest(CandidateManifestError),
    InvalidContext(&'static str),
}

impl fmt::Display for PromotionVerifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for PromotionVerifierError {}
