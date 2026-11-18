//! P0/P1/P2 native MOR research qualification and fail-closed shadow dispatch.
//!
//! Each label comes from a real SBG→MOR execution, not a guessed page or
//! numerical-attention score. This panel compares counterfactual RAE choices
//! against the model's exact native reference. It cannot prove task quality,
//! physical speedup, safe MOR elision, or FLAT's distinct Q/K mass bounds.

use core::fmt;
use std::collections::BTreeSet;

use crate::PageId;

use super::{
    rae_learned_route::{
        RaeHoldoutCounts, RaeLearnError, RaeReplayPolicy, RaeTrainPolicy, RaeTrainedEventRouter,
    },
    rae_native_trace::SbgRaeNativeFrame,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaeQualificationError {
    InvalidIdentity,
    InvalidRevision,
    DuplicateCaseId,
    TrainHoldoutCaseOverlap,
    NoHoldoutCases,
    EmptyCase,
    EmptyNativeReference,
    InvalidModelGeometry,
    Learned(RaeLearnError),
    MismatchedModelRevision,
}

impl fmt::Display for RaeQualificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RaeQualificationError {}

#[derive(Debug, Clone, Copy)]
pub struct RaeNativeCase<'a> {
    pub case_id: &'a str,
    pub frames: &'a [SbgRaeNativeFrame],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaeShadowBudgets {
    pub min_holdout_cases: usize,
    pub max_missed_pages: u128,
    pub max_extra_pages: u128,
    /// An independent task-quality result must be supplied by model tests.
    /// Merely matching native selected pages is insufficient.
    pub model_task_quality_confirmed: bool,
    /// Physical *end-to-end* savings must have been measured and accepted.
    pub physical_savings_confirmed: bool,
    /// Necessary for any future route-suppression promotion.
    pub complete_dependency_certificate: bool,
}

impl Default for RaeShadowBudgets {
    fn default() -> Self {
        Self {
            min_holdout_cases: 2,
            max_missed_pages: 0,
            max_extra_pages: 0,
            model_task_quality_confirmed: false,
            physical_savings_confirmed: false,
            complete_dependency_certificate: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RaeShadowPolicyTotals {
    pub frames: usize,
    pub native_selected_pages: u128,
    pub predicted_pages: u128,
    pub missed_native_pages: u128,
    pub extra_predicted_pages: u128,
    pub frames_with_misses: usize,
    pub frames_with_no_rising_front: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaeShadowQualification {
    pub pinned_code_revision: String,
    pub train_case_id: String,
    pub holdout_case_ids: Vec<String>,
    pub train_frames: usize,
    pub learned_links: usize,
    pub rising: RaeShadowPolicyTotals,
    pub persistent: RaeShadowPolicyTotals,
    pub contextual: RaeShadowPolicyTotals,
    /// Meets only the declared *page agreement* thresholds, not task quality.
    pub page_agreement_passed: bool,
    /// P2 is intentionally never authorizing native MOR suppression here.
    pub production_skip_authorized: bool,
    /// Structured list of independent gates missing for production promotion.
    pub promotion_blockers: Vec<&'static str>,
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value == value.trim()
}

fn update(totals: &mut RaeShadowPolicyTotals, frame: &SbgRaeNativeFrame, counts: RaeHoldoutCounts) {
    totals.frames += 1;
    totals.native_selected_pages += counts.required_pages;
    totals.predicted_pages += counts.selected_pages;
    totals.missed_native_pages += counts.missed_required_pages;
    totals.extra_predicted_pages += counts.extra_selected_pages;
    totals.frames_with_misses += usize::from(counts.missed_required_pages != 0);
    totals.frames_with_no_rising_front += usize::from(frame.rising_routing_bits == 0);
}

/// TRAIN on one independent native model trajectory, then evaluate frozen
/// links over multiple separately identified HOLDOUT trajectories.
/// Transitions are checked *within* each case by the existing validated
/// learner, not across independent session resets.
///
/// The native MOR expert selection has already run for every frame. The
/// counters are evidence of page agreement, NOT model output quality.
/// Even zero misses cannot authorize bypassing original MOR in this API.
pub fn qualify_native_rae<const BITS: usize>(
    pinned_code_revision: &str,
    train: RaeNativeCase<'_>,
    holdout: &[RaeNativeCase<'_>],
    expert_count: usize,
    policy: RaeTrainPolicy,
    fanout: usize,
    budgets: RaeShadowBudgets,
) -> Result<(RaeTrainedEventRouter, RaeShadowQualification), RaeQualificationError> {
    if !valid_identity(pinned_code_revision) {
        return Err(RaeQualificationError::InvalidRevision);
    }
    if !valid_identity(train.case_id) {
        return Err(RaeQualificationError::InvalidIdentity);
    }
    if train.frames.is_empty() {
        return Err(RaeQualificationError::EmptyCase);
    }
    if holdout.is_empty() {
        return Err(RaeQualificationError::NoHoldoutCases);
    }
    let mut ids = BTreeSet::new();
    ids.insert(train.case_id);
    for case in holdout {
        if !valid_identity(case.case_id) {
            return Err(RaeQualificationError::InvalidIdentity);
        }
        if case.case_id == train.case_id {
            return Err(RaeQualificationError::TrainHoldoutCaseOverlap);
        }
        if !ids.insert(case.case_id) {
            return Err(RaeQualificationError::DuplicateCaseId);
        }
        if case.frames.is_empty() {
            return Err(RaeQualificationError::EmptyCase);
        }
    }
    let learned = RaeTrainedEventRouter::train_with_fanout::<BITS>(
        train.case_id,
        expert_count,
        policy,
        fanout,
        train.frames,
    )
    .map_err(RaeQualificationError::Learned)?;

    let mut rising = RaeShadowPolicyTotals::default();
    let mut persistent = RaeShadowPolicyTotals::default();
    let mut contextual = RaeShadowPolicyTotals::default();
    for case in holdout {
        for (mode, accumulator) in [
            (RaeReplayPolicy::RisingOnly, &mut rising),
            (RaeReplayPolicy::PersistentActive, &mut persistent),
            (RaeReplayPolicy::RisingWithContextRecheck, &mut contextual),
        ] {
            let report = learned
                .evaluate_with_policy(case.case_id, case.frames, mode)
                .map_err(RaeQualificationError::Learned)?;
            for (frame, counts) in case.frames.iter().zip(report.per_frame) {
                update(accumulator, frame, counts);
            }
        }
    }

    let pages_pass = holdout.len() >= budgets.min_holdout_cases
        && rising.missed_native_pages <= budgets.max_missed_pages
        && rising.extra_predicted_pages <= budgets.max_extra_pages;
    let mut blockers = Vec::new();
    if !pages_pass {
        blockers.push("frozen-page-agreement");
    }
    if !budgets.model_task_quality_confirmed {
        blockers.push("independent-model-task-quality");
    }
    if !budgets.physical_savings_confirmed {
        blockers.push("paired-physical-cost");
    }
    if !budgets.complete_dependency_certificate {
        blockers.push("full-model-dependency-certificate");
    }
    // No skip in this API, even if all caller-supplied booleans are true:
    // on-the-fly model dependency and numerical validity cannot be proven
    // by attestations supplied alongside a research experiment.
    blockers.push("runtime-fail-closed-admission-not-implemented");
    let qualification = RaeShadowQualification {
        pinned_code_revision: pinned_code_revision.to_owned(),
        train_case_id: train.case_id.to_owned(),
        holdout_case_ids: holdout.iter().map(|case| case.case_id.to_owned()).collect(),
        train_frames: train.frames.len(),
        learned_links: learned.learned_association_count(),
        rising,
        persistent,
        contextual,
        page_agreement_passed: pages_pass,
        production_skip_authorized: false,
        promotion_blockers: blockers,
    };
    Ok((learned, qualification))
}

/// Every public dispatch mode executes the original MOR decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaeDispatchMode {
    Disabled,
    Shadow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaeNativeDispatch {
    pub executed_native_pages: Vec<PageId>,
    pub counterfactual_pages: Vec<PageId>,
    pub fallback_applied: bool,
    pub native_mor_bypassed: bool,
}

/// Model-owned P2 shadow dispatcher: always returns the actual native MOR
/// pages as the *executed* route. Candidate pages are diagnostic only.
/// A model revision mismatch aborts without any dispatch suggestion.
pub fn dispatch_native_shadow(
    expected_revision: &str,
    qualification: &RaeShadowQualification,
    trained: &RaeTrainedEventRouter,
    frame: &SbgRaeNativeFrame,
    mode: RaeDispatchMode,
    replay: RaeReplayPolicy,
) -> Result<RaeNativeDispatch, RaeQualificationError> {
    if expected_revision != qualification.pinned_code_revision {
        return Err(RaeQualificationError::MismatchedModelRevision);
    }
    if frame.selected_experts.is_empty() {
        return Err(RaeQualificationError::EmptyNativeReference);
    }
    if trained.expert_count().div_ceil(64) != frame.required_pages_words.len() {
        return Err(RaeQualificationError::InvalidModelGeometry);
    }
    let (counterfactual_pages, fallback_applied) = if mode == RaeDispatchMode::Shadow {
        let mask = trained
            .predict_frame(frame, replay)
            .map_err(RaeQualificationError::Learned)?;
        // MOR returns pages in score order; a diagnostic bitset has
        // original page order. Compare masks, never ordered Vec positions.
        let mismatch = mask != frame.required_pages_words;
        let candidate = mask
            .iter()
            .enumerate()
            .flat_map(|(word, bits)| {
                let mut rest = *bits;
                std::iter::from_fn(move || {
                    if rest == 0 {
                        return None;
                    }
                    let bit = rest.trailing_zeros() as usize;
                    rest &= rest - 1;
                    Some(PageId((word * 64 + bit) as u32))
                })
            })
            .collect();
        (candidate, mismatch)
    } else {
        (Vec::new(), false)
    };
    Ok(RaeNativeDispatch {
        executed_native_pages: frame.selected_experts.clone(),
        counterfactual_pages,
        fallback_applied,
        native_mor_bypassed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(cycle: u64, routing: bool, rise: bool, page: u32) -> SbgRaeNativeFrame {
        let bit = u64::from(routing);
        SbgRaeNativeFrame {
            cycle,
            routing_words: vec![bit],
            rising_words: vec![u64::from(rise)],
            falling_words: vec![0],
            required_pages_words: vec![1_u64 << page],
            selected_experts: vec![PageId(page)],
            rising_routing_bits: u64::from(rise),
            falling_routing_bits: 0,
            active_routing_bits: bit,
            context_invalidated: false,
        }
    }

    #[test]
    fn independent_cases_report_native_misses_and_keep_original_mor() {
        let train = [frame(1, true, true, 1), frame(2, true, false, 1)];
        let first = [frame(1, true, true, 1), frame(2, true, false, 1)];
        let second = [frame(1, true, true, 1), frame(2, true, false, 1)];
        let (learned, report) = qualify_native_rae::<64>(
            "repo-sha-pinned",
            RaeNativeCase {
                case_id: "TRAIN-1",
                frames: &train,
            },
            &[
                RaeNativeCase {
                    case_id: "HOLDOUT-2",
                    frames: &first,
                },
                RaeNativeCase {
                    case_id: "HOLDOUT-3",
                    frames: &second,
                },
            ],
            2,
            RaeTrainPolicy {
                minimum_exposures: 1,
                minimum_precision_ppm: 1_000_000,
                max_associations: 2,
            },
            1,
            RaeShadowBudgets::default(),
        )
        .unwrap();
        assert_eq!(report.rising.frames, 4);
        assert_eq!(report.rising.missed_native_pages, 2);
        assert_eq!(report.persistent.missed_native_pages, 0);
        assert!(!report.page_agreement_passed);
        assert!(!report.production_skip_authorized);
        let dispatch = dispatch_native_shadow(
            "repo-sha-pinned",
            &report,
            &learned,
            &second[1],
            RaeDispatchMode::Shadow,
            RaeReplayPolicy::RisingOnly,
        )
        .unwrap();
        assert_eq!(dispatch.executed_native_pages, vec![PageId(1)]);
        assert!(dispatch.counterfactual_pages.is_empty());
        assert!(dispatch.fallback_applied);
        assert!(!dispatch.native_mor_bypassed);
        let disabled = dispatch_native_shadow(
            "repo-sha-pinned",
            &report,
            &learned,
            &second[1],
            RaeDispatchMode::Disabled,
            RaeReplayPolicy::RisingOnly,
        )
        .unwrap();
        assert_eq!(disabled.executed_native_pages, vec![PageId(1)]);
        assert!(disabled.counterfactual_pages.is_empty());
        assert!(!disabled.fallback_applied);
        assert!(!disabled.native_mor_bypassed);
        assert_eq!(
            dispatch_native_shadow(
                "wrong-sha",
                &report,
                &learned,
                &second[1],
                RaeDispatchMode::Shadow,
                RaeReplayPolicy::RisingOnly,
            ),
            Err(RaeQualificationError::MismatchedModelRevision)
        );
    }

    #[test]
    fn overlapping_cases_are_rejected_without_training() {
        let rows = [frame(1, true, true, 1)];
        let policy = RaeTrainPolicy {
            minimum_exposures: 1,
            minimum_precision_ppm: 0,
            max_associations: 2,
        };
        let err = qualify_native_rae::<64>(
            "rev",
            RaeNativeCase {
                case_id: "case",
                frames: &rows,
            },
            &[RaeNativeCase {
                case_id: "case",
                frames: &rows,
            }],
            2,
            policy,
            1,
            RaeShadowBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(err, RaeQualificationError::TrainHoldoutCaseOverlap);
    }
}
