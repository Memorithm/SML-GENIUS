use core::fmt;

use crate::{
    curriculum::CurriculumReport,
    manifest::RsiCandidateManifest,
    promotion::{
        evaluate_promotion_eligibility, PromotionEligibilityReport, PromotionPolicy,
        PromotionReviewContext, PromotionStatus,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CyclePhase {
    ChampionFrozen,
    CurriculumVerified,
    CandidateFrozen,
    EvidenceBound,
    ReviewedEligible,
    ReviewedIneligible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurriculumEvidence {
    pub verified_tasks: u64,
    pub training_tasks: u64,
    pub development_tasks: u64,
    pub total_events: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsiCycle {
    cycle_id: String,
    objective: String,
    champion_sha: String,
    candidate_head_sha: Option<String>,
    phase: CyclePhase,
    curriculum: Option<CurriculumEvidence>,
    manifest: Option<RsiCandidateManifest>,
    review: Option<PromotionEligibilityReport>,
}

impl RsiCycle {
    pub fn begin(
        cycle_id: impl Into<String>,
        objective: impl Into<String>,
        champion_sha: impl Into<String>,
    ) -> Result<Self, CycleError> {
        let cycle_id = cycle_id.into();
        let objective = objective.into();
        let champion_sha = champion_sha.into();
        validate_text(&cycle_id)?;
        validate_text(&objective)?;
        validate_git_sha(&champion_sha)?;
        Ok(Self {
            cycle_id,
            objective,
            champion_sha,
            candidate_head_sha: None,
            phase: CyclePhase::ChampionFrozen,
            curriculum: None,
            manifest: None,
            review: None,
        })
    }

    #[must_use]
    pub fn phase(&self) -> CyclePhase {
        self.phase
    }

    #[must_use]
    pub fn champion_sha(&self) -> &str {
        &self.champion_sha
    }

    #[must_use]
    pub fn candidate_head_sha(&self) -> Option<&str> {
        self.candidate_head_sha.as_deref()
    }

    #[must_use]
    pub const fn human_merge_gate_required(&self) -> bool {
        true
    }

    #[must_use]
    pub const fn self_promotion_authorized(&self) -> bool {
        false
    }

    pub fn attach_curriculum(&mut self, report: &CurriculumReport) -> Result<(), CycleError> {
        self.require_phase(CyclePhase::ChampionFrozen)?;
        if report.verified_tasks == 0
            || report.verified_tasks
                != report
                    .training_tasks
                    .checked_add(report.development_tasks)
                    .ok_or(CycleError::Overflow)?
        {
            return Err(CycleError::InvalidCurriculum);
        }
        self.curriculum = Some(CurriculumEvidence {
            verified_tasks: report.verified_tasks,
            training_tasks: report.training_tasks,
            development_tasks: report.development_tasks,
            total_events: report.total_events,
        });
        self.phase = CyclePhase::CurriculumVerified;
        Ok(())
    }

    pub fn freeze_candidate(
        &mut self,
        candidate_head_sha: impl Into<String>,
    ) -> Result<(), CycleError> {
        self.require_phase(CyclePhase::CurriculumVerified)?;
        let candidate_head_sha = candidate_head_sha.into();
        validate_git_sha(&candidate_head_sha)?;
        if candidate_head_sha == self.champion_sha {
            return Err(CycleError::CandidateEqualsChampion);
        }
        self.candidate_head_sha = Some(candidate_head_sha);
        self.phase = CyclePhase::CandidateFrozen;
        Ok(())
    }

    pub fn bind_manifest(&mut self, manifest: RsiCandidateManifest) -> Result<(), CycleError> {
        self.require_phase(CyclePhase::CandidateFrozen)?;
        manifest
            .validate()
            .map_err(|_| CycleError::InvalidManifest)?;
        let candidate = self
            .candidate_head_sha
            .as_deref()
            .ok_or(CycleError::MissingCandidate)?;
        if manifest.parent_main_sha != self.champion_sha {
            return Err(CycleError::ManifestParentMismatch);
        }
        if manifest.candidate_head_sha != candidate {
            return Err(CycleError::ManifestHeadMismatch);
        }
        self.manifest = Some(manifest);
        self.phase = CyclePhase::EvidenceBound;
        Ok(())
    }

    pub fn review(
        &mut self,
        context: &PromotionReviewContext,
        policy: &PromotionPolicy,
    ) -> Result<&PromotionEligibilityReport, CycleError> {
        self.require_phase(CyclePhase::EvidenceBound)?;
        if context.current_main_sha != self.champion_sha {
            return Err(CycleError::ChampionMoved);
        }
        let candidate = self
            .candidate_head_sha
            .as_deref()
            .ok_or(CycleError::MissingCandidate)?;
        if context.pr_head_sha != candidate {
            return Err(CycleError::ReviewHeadMismatch);
        }
        let manifest = self.manifest.as_ref().ok_or(CycleError::InvalidManifest)?;
        let report = evaluate_promotion_eligibility(manifest, context, policy);
        self.phase = if report.status == PromotionStatus::EligibleForHumanReview {
            CyclePhase::ReviewedEligible
        } else {
            CyclePhase::ReviewedIneligible
        };
        self.review = Some(report);
        self.review.as_ref().ok_or(CycleError::InternalInvariant)
    }

    #[must_use]
    pub fn review_report(&self) -> Option<&PromotionEligibilityReport> {
        self.review.as_ref()
    }

    pub fn pr_summary(&self) -> Result<String, CycleError> {
        if self.phase != CyclePhase::EvidenceBound
            && self.phase != CyclePhase::ReviewedEligible
            && self.phase != CyclePhase::ReviewedIneligible
        {
            return Err(CycleError::InvalidPhase {
                expected: CyclePhase::EvidenceBound,
                actual: self.phase,
            });
        }
        let curriculum = self
            .curriculum
            .as_ref()
            .ok_or(CycleError::InvalidCurriculum)?;
        let candidate = self
            .candidate_head_sha
            .as_deref()
            .ok_or(CycleError::MissingCandidate)?;
        let manifest = self.manifest.as_ref().ok_or(CycleError::InvalidManifest)?;

        Ok(format!(
            "SML-RSI cycle {}\nobjective={}\nparent={}\nhead={}\nclass={:?}\nverified_tasks={}\ntraining_tasks={}\ndevelopment_tasks={}\ntotal_events={}\nhuman_merge_gate=true\nself_promotion=false\n",
            self.cycle_id,
            self.objective,
            self.champion_sha,
            candidate,
            manifest.class,
            curriculum.verified_tasks,
            curriculum.training_tasks,
            curriculum.development_tasks,
            curriculum.total_events,
        ))
    }

    fn require_phase(&self, expected: CyclePhase) -> Result<(), CycleError> {
        if self.phase != expected {
            return Err(CycleError::InvalidPhase {
                expected,
                actual: self.phase,
            });
        }
        Ok(())
    }
}

fn validate_git_sha(value: &str) -> Result<(), CycleError> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CycleError::InvalidGitSha);
    }
    Ok(())
}

fn validate_text(value: &str) -> Result<(), CycleError> {
    if value.trim().is_empty()
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(CycleError::InvalidText);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleError {
    InvalidGitSha,
    InvalidText,
    InvalidPhase {
        expected: CyclePhase,
        actual: CyclePhase,
    },
    InvalidCurriculum,
    CandidateEqualsChampion,
    MissingCandidate,
    InvalidManifest,
    ManifestParentMismatch,
    ManifestHeadMismatch,
    ChampionMoved,
    ReviewHeadMismatch,
    Overflow,
    InternalInvariant,
}

impl fmt::Display for CycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CycleError {}
