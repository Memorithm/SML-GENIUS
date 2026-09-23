use core::fmt;
use std::collections::BTreeSet;

use sml_core::tasks::{
    copy_after_marker, delayed_bit_recall, distractor_rejection, keyed_fact_recall,
    overwrite_bit_recall, two_fact_conjunction, TaskError, TaskInstance, TaskKind,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurriculumProposal {
    pub proposal_id: String,
    pub family: TaskKind,
    pub case_id_start: u64,
    pub case_count: u32,
    /// Family-specific bounded difficulty (delay, distractors or payload length).
    pub difficulty: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurriculumPolicy {
    pub max_proposals: usize,
    pub max_cases_per_proposal: u32,
    pub max_total_events: u64,
    /// Keeps RSI-generated development/training cases outside canonical low-ID fixtures.
    pub reserved_case_id_floor: u64,
    /// Exactly one residue class is development; all others are training.
    pub development_modulus: u64,
}

impl Default for CurriculumPolicy {
    fn default() -> Self {
        Self {
            max_proposals: 64,
            max_cases_per_proposal: 1024,
            max_total_events: 1_000_000,
            reserved_case_id_floor: 1_000_000,
            development_modulus: 5,
        }
    }
}

impl CurriculumPolicy {
    pub fn validate(self) -> Result<Self, CurriculumError> {
        if self.max_proposals == 0
            || self.max_cases_per_proposal == 0
            || self.max_total_events == 0
            || self.development_modulus < 2
        {
            return Err(CurriculumError::InvalidPolicy);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurriculumPartition {
    Training,
    Development,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCurriculumTask {
    pub proposal_id: String,
    pub partition: CurriculumPartition,
    pub task: TaskInstance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurriculumReport {
    pub tasks: Vec<VerifiedCurriculumTask>,
    pub proposal_count: usize,
    pub verified_tasks: u64,
    pub training_tasks: u64,
    pub development_tasks: u64,
    pub total_events: u64,
}

pub fn build_verified_curriculum(
    proposals: &[CurriculumProposal],
    policy: CurriculumPolicy,
) -> Result<CurriculumReport, CurriculumError> {
    let policy = policy.validate()?;
    if proposals.is_empty() {
        return Err(CurriculumError::EmptyProposals);
    }
    if proposals.len() > policy.max_proposals {
        return Err(CurriculumError::ProposalBudget);
    }

    let mut proposal_ids = BTreeSet::new();
    let mut task_keys = BTreeSet::new();
    let mut tasks = Vec::new();
    let mut total_events = 0_u64;
    let mut training_tasks = 0_u64;
    let mut development_tasks = 0_u64;

    for proposal in proposals {
        validate_proposal(proposal, policy)?;
        if !proposal_ids.insert(proposal.proposal_id.as_str()) {
            return Err(CurriculumError::DuplicateProposalId);
        }

        let end = proposal
            .case_id_start
            .checked_add(u64::from(proposal.case_count))
            .ok_or(CurriculumError::CaseIdOverflow)?;
        for case_id in proposal.case_id_start..end {
            let key = (family_tag(proposal.family), case_id);
            if !task_keys.insert(key) {
                return Err(CurriculumError::DuplicateTask);
            }

            let task = instantiate_task(proposal.family, case_id, proposal.difficulty)?;
            if !task.oracle_matches()? {
                return Err(CurriculumError::OracleMismatch);
            }

            total_events = total_events
                .checked_add(
                    u64::try_from(task.events.len()).map_err(|_| CurriculumError::Overflow)?,
                )
                .ok_or(CurriculumError::Overflow)?;
            if total_events > policy.max_total_events {
                return Err(CurriculumError::EventBudget);
            }

            let partition = partition_for(
                proposal.family,
                case_id,
                policy.development_modulus,
            );
            match partition {
                CurriculumPartition::Training => {
                    training_tasks = training_tasks.saturating_add(1);
                }
                CurriculumPartition::Development => {
                    development_tasks = development_tasks.saturating_add(1);
                }
            }

            tasks.push(VerifiedCurriculumTask {
                proposal_id: proposal.proposal_id.clone(),
                partition,
                task,
            });
        }
    }

    Ok(CurriculumReport {
        proposal_count: proposals.len(),
        verified_tasks: u64::try_from(tasks.len()).map_err(|_| CurriculumError::Overflow)?,
        tasks,
        training_tasks,
        development_tasks,
        total_events,
    })
}

fn validate_proposal(
    proposal: &CurriculumProposal,
    policy: CurriculumPolicy,
) -> Result<(), CurriculumError> {
    if proposal.proposal_id.trim().is_empty()
        || proposal.proposal_id.contains('\n')
        || proposal.proposal_id.contains('\r')
        || proposal.proposal_id.contains('\0')
    {
        return Err(CurriculumError::InvalidProposalId);
    }
    if proposal.case_count == 0 || proposal.case_count > policy.max_cases_per_proposal {
        return Err(CurriculumError::CaseBudget);
    }
    if proposal.case_id_start < policy.reserved_case_id_floor {
        return Err(CurriculumError::ReservedCaseRange);
    }
    if proposal.difficulty == 0 || proposal.difficulty > 16_383 {
        return Err(CurriculumError::InvalidDifficulty);
    }
    Ok(())
}

fn instantiate_task(
    family: TaskKind,
    case_id: u64,
    difficulty: u16,
) -> Result<TaskInstance, CurriculumError> {
    let difficulty = usize::from(difficulty);
    match family {
        TaskKind::DelayedBitRecall => Ok(delayed_bit_recall(case_id, difficulty)),
        TaskKind::OverwriteBitRecall => Ok(overwrite_bit_recall(case_id, difficulty)),
        TaskKind::KeyedFactRecall => Ok(keyed_fact_recall(case_id, difficulty)?),
        TaskKind::CopyAfterMarker => Ok(copy_after_marker(case_id, difficulty.min(4096))?),
        TaskKind::TwoFactConjunction => Ok(two_fact_conjunction(case_id, difficulty)?),
        TaskKind::DistractorRejection => Ok(distractor_rejection(case_id, difficulty)?),
    }
}

const fn family_tag(family: TaskKind) -> u8 {
    match family {
        TaskKind::DelayedBitRecall => 0,
        TaskKind::OverwriteBitRecall => 1,
        TaskKind::KeyedFactRecall => 2,
        TaskKind::CopyAfterMarker => 3,
        TaskKind::TwoFactConjunction => 4,
        TaskKind::DistractorRejection => 5,
    }
}

fn partition_for(
    family: TaskKind,
    case_id: u64,
    development_modulus: u64,
) -> CurriculumPartition {
    let mixed = mix64(case_id ^ u64::from(family_tag(family)).wrapping_mul(0x9e37_79b9));
    if mixed % development_modulus == 0 {
        CurriculumPartition::Development
    } else {
        CurriculumPartition::Training
    }
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurriculumError {
    InvalidPolicy,
    EmptyProposals,
    ProposalBudget,
    InvalidProposalId,
    DuplicateProposalId,
    CaseBudget,
    ReservedCaseRange,
    InvalidDifficulty,
    CaseIdOverflow,
    DuplicateTask,
    EventBudget,
    OracleMismatch,
    Overflow,
    Task(TaskError),
}

impl From<TaskError> for CurriculumError {
    fn from(error: TaskError) -> Self {
        Self::Task(error)
    }
}

impl fmt::Display for CurriculumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CurriculumError {}
