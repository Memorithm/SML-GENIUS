//! MOR-gated Boolean learning across packed SML pages.
//!
//! This module binds gradient-free Boolean learning to SML's sparse routing
//! decision. Only pages present in MorDecision::selected_experts are eligible
//! for training in a step. Inventory and selection invariants are validated
//! before any page mutation.
//!
//! Commits remain page-local and atomic. The batch deliberately does not promise
//! all-or-nothing rollback across multiple expert pages: copying large packed
//! pages merely to emulate a global transaction would defeat the sparse design.

use core::fmt;

use crate::{
    boolean_learning::{
        BooleanLearningError, BooleanLearningOutcome, BooleanLearningPolicy, BooleanLearningReport,
        BooleanLearningTransaction,
    },
    boolean_page::{BooleanPageRead, PackedBooleanPage},
    mor::MorDecision,
    PageId,
};

/// Hard limits for one MOR-gated learning step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorBooleanLearningPolicy {
    pub page_learning: BooleanLearningPolicy,
    /// Maximum routed pages that may be trained in one step.
    pub max_pages_per_step: usize,
    /// Maximum pages allowed to commit hard flips in one step.
    pub max_commits_per_step: usize,
}

impl Default for MorBooleanLearningPolicy {
    fn default() -> Self {
        Self {
            page_learning: BooleanLearningPolicy::default(),
            max_pages_per_step: 4,
            max_commits_per_step: 2,
        }
    }
}

impl MorBooleanLearningPolicy {
    pub fn validate(self) -> Result<Self, MorBooleanLearningPolicyError> {
        self.page_learning
            .validate()
            .map_err(MorBooleanLearningPolicyError::PageLearning)?;
        if self.max_pages_per_step == 0 {
            return Err(MorBooleanLearningPolicyError::ZeroPageBudget);
        }
        if self.max_commits_per_step == 0 {
            return Err(MorBooleanLearningPolicyError::ZeroCommitBudget);
        }
        if self.max_commits_per_step > self.max_pages_per_step {
            return Err(
                MorBooleanLearningPolicyError::CommitBudgetExceedsPageBudget {
                    commits: self.max_commits_per_step,
                    pages: self.max_pages_per_step,
                },
            );
        }
        Ok(self)
    }
}

/// Result for one routed expert page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorBooleanPageLearningReport {
    pub page_id: PageId,
    pub report: BooleanLearningReport,
}

/// Aggregate evidence for one sparse MOR learning step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorBooleanLearningReport {
    /// Exact MOR-selected page order used as the training eligibility boundary.
    pub selected_pages: Vec<PageId>,
    /// Prefix of selected pages actually trained under the page/commit budgets.
    pub trained_pages: Vec<PageId>,
    pub page_reports: Vec<MorBooleanPageLearningReport>,
    pub committed_pages: u64,
    pub committed_flips: u64,
    pub group_evaluations: u64,
    pub probe_evaluations: u64,
    pub rescued_groups: u64,
    pub skipped_selected_pages: u64,
}

/// Sparse Boolean trainer driven directly by a committed MOR decision.
pub struct MorBooleanLearning;

impl MorBooleanLearning {
    pub fn train_selected<E, TrainObjective, HoldoutObjective>(
        decision: &MorDecision,
        pages: &mut [PackedBooleanPage],
        policy: MorBooleanLearningPolicy,
        mut train_objective: TrainObjective,
        mut holdout_objective: HoldoutObjective,
    ) -> Result<MorBooleanLearningReport, MorBooleanLearningError<E>>
    where
        TrainObjective: FnMut(PageId, &dyn BooleanPageRead) -> Result<u32, E>,
        HoldoutObjective: FnMut(PageId, &dyn BooleanPageRead) -> Result<u32, E>,
    {
        let policy = policy.validate().map_err(MorBooleanLearningError::Policy)?;

        validate_inventory(pages)?;
        validate_selection(&decision.selected_experts, pages)?;

        let selected_pages = decision.selected_experts.clone();
        let mut trained_pages = Vec::new();
        let mut page_reports = Vec::new();
        let mut committed_pages = 0_u64;
        let mut committed_flips = 0_u64;
        let mut group_evaluations = 0_u64;
        let mut probe_evaluations = 0_u64;
        let mut rescued_groups = 0_u64;

        for page_id in selected_pages.iter().copied() {
            if trained_pages.len() >= policy.max_pages_per_step
                || committed_pages as usize >= policy.max_commits_per_step
            {
                break;
            }

            let page_index = pages
                .iter()
                .position(|page| page.page_id() == page_id)
                .ok_or(MorBooleanLearningError::MissingSelectedPage(page_id))?;
            let page = &mut pages[page_index];

            let report = BooleanLearningTransaction::train_page(
                page,
                policy.page_learning,
                |read| train_objective(page_id, read),
                |read| holdout_objective(page_id, read),
            )
            .map_err(MorBooleanLearningError::PageLearning)?;

            if matches!(
                report.outcome,
                BooleanLearningOutcome::CommitGate(
                    crate::boolean_hierarchical::BooleanCommitDecision::Commit
                )
            ) && report.committed_flips > 0
            {
                committed_pages = committed_pages.saturating_add(1);
            }

            committed_flips = committed_flips.saturating_add(report.committed_flips);
            group_evaluations = group_evaluations.saturating_add(report.group_evaluations);
            probe_evaluations = probe_evaluations.saturating_add(report.probe_evaluations);
            rescued_groups = rescued_groups.saturating_add(report.rescued_groups);
            trained_pages.push(page_id);
            page_reports.push(MorBooleanPageLearningReport { page_id, report });
        }

        let skipped_selected_pages =
            selected_pages.len().saturating_sub(trained_pages.len()) as u64;

        Ok(MorBooleanLearningReport {
            selected_pages,
            trained_pages,
            page_reports,
            committed_pages,
            committed_flips,
            group_evaluations,
            probe_evaluations,
            rescued_groups,
            skipped_selected_pages,
        })
    }
}

fn validate_inventory<E>(pages: &[PackedBooleanPage]) -> Result<(), MorBooleanLearningError<E>> {
    for (index, page) in pages.iter().enumerate() {
        if pages[..index]
            .iter()
            .any(|other| other.page_id() == page.page_id())
        {
            return Err(MorBooleanLearningError::DuplicatePage(page.page_id()));
        }
    }
    Ok(())
}

fn validate_selection<E>(
    selected: &[PageId],
    pages: &[PackedBooleanPage],
) -> Result<(), MorBooleanLearningError<E>> {
    if selected.is_empty() {
        return Err(MorBooleanLearningError::EmptyMorSelection);
    }

    for (index, page_id) in selected.iter().copied().enumerate() {
        if selected[..index].contains(&page_id) {
            return Err(MorBooleanLearningError::DuplicateSelectedPage(page_id));
        }
        if !pages.iter().any(|page| page.page_id() == page_id) {
            return Err(MorBooleanLearningError::MissingSelectedPage(page_id));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorBooleanLearningPolicyError {
    ZeroPageBudget,
    ZeroCommitBudget,
    CommitBudgetExceedsPageBudget { commits: usize, pages: usize },
    PageLearning(crate::boolean_learning::BooleanLearningPolicyError),
}

impl fmt::Display for MorBooleanLearningPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorBooleanLearningPolicyError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MorBooleanLearningError<E> {
    Policy(MorBooleanLearningPolicyError),
    EmptyMorSelection,
    DuplicateSelectedPage(PageId),
    DuplicatePage(PageId),
    MissingSelectedPage(PageId),
    PageLearning(BooleanLearningError<E>),
}

impl<E: fmt::Debug> fmt::Display for MorBooleanLearningError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<E: fmt::Debug> std::error::Error for MorBooleanLearningError<E> {}
