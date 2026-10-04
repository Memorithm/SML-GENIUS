//! Bounded downstream expert-applicability constraints.
//!
//! This module does not route, score, verify provenance, repair proposals or
//! authorize actions. It only preserves or suppresses a caller-owned proposal
//! according to an independently supplied bounded set of permitted expert pages.

use core::fmt;

use sml_core::PageId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpertApplicabilityLimits {
    pub expert_count: u32,
    pub max_allowed_experts: usize,
}

impl ExpertApplicabilityLimits {
    pub fn validate(self) -> Result<Self, ExpertApplicabilityError> {
        if self.expert_count == 0 {
            return Err(ExpertApplicabilityError::ZeroExpertCount);
        }
        if self.max_allowed_experts == 0 {
            return Err(ExpertApplicabilityError::ZeroAllowedBudget);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowedExpertSet {
    pages: Vec<PageId>,
}

impl AllowedExpertSet {
    pub fn new(
        mut pages: Vec<PageId>,
        limits: ExpertApplicabilityLimits,
    ) -> Result<Self, ExpertApplicabilityError> {
        let limits = limits.validate()?;
        if pages.is_empty() {
            return Err(ExpertApplicabilityError::EmptyAllowedSet);
        }
        if pages.len() > limits.max_allowed_experts {
            return Err(ExpertApplicabilityError::AllowedBudget {
                actual: pages.len(),
                maximum: limits.max_allowed_experts,
            });
        }
        for page in &pages {
            if page.0 >= limits.expert_count {
                return Err(ExpertApplicabilityError::ExpertOutOfRange {
                    page_id: *page,
                    expert_count: limits.expert_count,
                });
            }
        }

        pages.sort_by_key(|page| page.0);
        for pair in pages.windows(2) {
            if pair[0] == pair[1] {
                return Err(ExpertApplicabilityError::DuplicateExpert(pair[0]));
            }
        }

        Ok(Self { pages })
    }

    #[must_use]
    pub fn pages(&self) -> &[PageId] {
        &self.pages
    }

    #[must_use]
    pub fn allows(&self, page_id: PageId) -> bool {
        self.pages
            .binary_search_by_key(&page_id.0, |page| page.0)
            .is_ok()
    }

    /// Preserves the proposal unchanged when the page is independently
    /// permitted, otherwise returns a typed abstention. This never repairs or
    /// substitutes the caller's proposal.
    pub fn screen<T>(
        &self,
        page_id: PageId,
        proposal: T,
    ) -> ExpertApplicabilityOutcome<T> {
        if self.allows(page_id) {
            ExpertApplicabilityOutcome::Allowed(proposal)
        } else {
            ExpertApplicabilityOutcome::Abstain(
                ExpertApplicabilityAbstention::ExpertNotAllowed(page_id),
            )
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpertApplicabilityAbstention {
    ExpertNotAllowed(PageId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpertApplicabilityOutcome<T> {
    Allowed(T),
    Abstain(ExpertApplicabilityAbstention),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpertApplicabilityError {
    ZeroExpertCount,
    ZeroAllowedBudget,
    EmptyAllowedSet,
    AllowedBudget { actual: usize, maximum: usize },
    ExpertOutOfRange { page_id: PageId, expert_count: u32 },
    DuplicateExpert(PageId),
}

impl fmt::Display for ExpertApplicabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ExpertApplicabilityError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> ExpertApplicabilityLimits {
        ExpertApplicabilityLimits {
            expert_count: 4,
            max_allowed_experts: 3,
        }
    }

    #[test]
    fn allowed_pages_are_canonical_and_membership_is_exact() {
        let set = AllowedExpertSet::new(
            vec![PageId(3), PageId(1)],
            limits(),
        )
        .unwrap();
        assert_eq!(set.pages(), &[PageId(1), PageId(3)]);
        assert!(set.allows(PageId(1)));
        assert!(set.allows(PageId(3)));
        assert!(!set.allows(PageId(0)));
        assert!(!set.allows(PageId(2)));
    }

    #[test]
    fn invalid_sets_fail_closed() {
        assert_eq!(
            AllowedExpertSet::new(
                vec![PageId(0)],
                ExpertApplicabilityLimits {
                    expert_count: 0,
                    max_allowed_experts: 1,
                },
            ),
            Err(ExpertApplicabilityError::ZeroExpertCount)
        );
        assert_eq!(
            AllowedExpertSet::new(
                vec![PageId(0)],
                ExpertApplicabilityLimits {
                    expert_count: 1,
                    max_allowed_experts: 0,
                },
            ),
            Err(ExpertApplicabilityError::ZeroAllowedBudget)
        );
        assert_eq!(
            AllowedExpertSet::new(Vec::new(), limits()),
            Err(ExpertApplicabilityError::EmptyAllowedSet)
        );
        assert_eq!(
            AllowedExpertSet::new(
                vec![PageId(0), PageId(1), PageId(2), PageId(3)],
                limits(),
            ),
            Err(ExpertApplicabilityError::AllowedBudget {
                actual: 4,
                maximum: 3,
            })
        );
        assert_eq!(
            AllowedExpertSet::new(vec![PageId(4)], limits()),
            Err(ExpertApplicabilityError::ExpertOutOfRange {
                page_id: PageId(4),
                expert_count: 4,
            })
        );
        assert_eq!(
            AllowedExpertSet::new(vec![PageId(2), PageId(2)], limits()),
            Err(ExpertApplicabilityError::DuplicateExpert(PageId(2)))
        );
    }

    #[test]
    fn screening_only_preserves_or_suppresses() {
        let set = AllowedExpertSet::new(vec![PageId(1)], limits()).unwrap();

        assert_eq!(
            set.screen(PageId(1), false),
            ExpertApplicabilityOutcome::Allowed(false)
        );
        assert_eq!(
            set.screen(PageId(0), true),
            ExpertApplicabilityOutcome::Abstain(
                ExpertApplicabilityAbstention::ExpertNotAllowed(PageId(0))
            )
        );
    }
}
