//! BAF-2: bounded **joint** rule learning for native Boolean memory pages.
//!
//! Candidate search uses only TRAIN final-state transition labels. Every
//! candidate is executed through the unmodified BAF-0 priority/conflict oracle.
//! Validation is consulted *once* after the entire TRAIN candidate freezes.
//! No Q/K/V embeddings, pairwise similarities, softmax, or attention matrices.
//!
//! Only page activation ANFs are learned. Page payloads, masks, priorities,
//! candidate IDs and state widths remain immutable. Greedy search cannot be
//! assumed to discover synergistic rules whose individual flips do not help.

use std::collections::BTreeSet;

use crate::{
    boolean_associative_focus::{
        AnfMonomial, AssociativeFocusConfig, AssociativeFocusError, AssociativeFocusPage,
        BooleanAssociativeFocus,
    },
    boolean_associative_train::{
        error_bits, feature_bank, validate_splits, FocusFitError, FocusFitPolicy, FocusTransition,
    },
    PageId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JointFocusPolicy {
    pub local: FocusFitPolicy,
    pub max_editable_pages: usize,
    /// Preflight bound on the entire worst-case TRAIN candidate evaluation
    /// count; refused before computing any search result.
    pub max_candidate_evaluations: u64,
}

impl Default for JointFocusPolicy {
    fn default() -> Self {
        Self {
            local: FocusFitPolicy::default(),
            max_editable_pages: 4,
            max_candidate_evaluations: 5_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JointFocusError {
    InvalidPolicy(&'static str),
    NoEditablePages,
    EditablePageBudget,
    DuplicateEditablePage(PageId),
    MissingEditablePage(PageId),
    NonemptyInitialPredicate(PageId),
    CandidateBudget,
    Split(FocusFitError),
    Engine(AssociativeFocusError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JointFocusAcceptedFlip {
    pub page: PageId,
    /// None means the predicate's constant ANF coefficient was flipped.
    pub term: Option<AnfMonomial>,
    pub train_errors_after: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JointFocusReport {
    pub train_baseline_errors: u64,
    pub validation_baseline_errors: u64,
    pub train_candidate_errors: u64,
    pub validation_candidate_errors: u64,
    pub examined_train_candidates: u64,
    /// Ordered strictly by accepted train-only objective improvement.
    pub accepted_flips: Vec<JointFocusAcceptedFlip>,
    /// Number of pages whose final candidate activation differs from input.
    pub changed_pages: usize,
    /// A frozen TRAIN winner is accepted only if it strictly improves TRAIN
    /// and does not worsen the untouched VALIDATION baseline.
    pub admitted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JointFocusResult {
    pub model: BooleanAssociativeFocus,
    pub report: JointFocusReport,
}

/// Deterministic global coordinate descent across selected page ANF rules.
///
/// The first candidate with the smallest *strictly improved* TRAIN loss wins
/// the round (page IDs ascending; bias, degree-1, then degree-2 monomials).
/// Later rounds reconsider all pages from the previously accepted model. The
/// final model is validated only after all train-side choices are complete.
/// A regression rolls back all candidate changes atomically.
pub fn fit_joint_focus(
    config: AssociativeFocusConfig,
    pages: &[AssociativeFocusPage],
    editable: &[PageId],
    train: &[FocusTransition],
    validation: &[FocusTransition],
    policy: JointFocusPolicy,
) -> Result<JointFocusResult, JointFocusError> {
    let local = policy.local;
    if !(1..=2).contains(&local.max_degree)
        || local.max_rounds == 0
        || local.max_terms == 0
        || local.max_basis_terms == 0
        || local.max_samples == 0
        || policy.max_editable_pages == 0
        || policy.max_candidate_evaluations == 0
    {
        return Err(JointFocusError::InvalidPolicy(
            "zero budget or unsupported ANF degree",
        ));
    }
    if config.event_bits.saturating_add(config.state_bits) > 16 {
        return Err(JointFocusError::InvalidPolicy(
            "at most 16 event+state input bits",
        ));
    }
    if editable.is_empty() {
        return Err(JointFocusError::NoEditablePages);
    }
    if editable.len() > policy.max_editable_pages {
        return Err(JointFocusError::EditablePageBudget);
    }
    if train.is_empty() || validation.is_empty() {
        return Err(JointFocusError::Split(FocusFitError::EmptySplit));
    }
    if train.len().saturating_add(validation.len()) > local.max_samples {
        return Err(JointFocusError::Split(FocusFitError::TooManySamples));
    }

    let baseline =
        BooleanAssociativeFocus::new(config, pages.to_vec()).map_err(JointFocusError::Engine)?;
    validate_splits(config, train, validation).map_err(JointFocusError::Split)?;

    let mut ids = editable.to_vec();
    ids.sort_unstable_by_key(|p| p.0);
    if let Some(duplicate) = ids.windows(2).find(|w| w[0] == w[1]) {
        return Err(JointFocusError::DuplicateEditablePage(duplicate[0]));
    }
    let mut indices = Vec::with_capacity(ids.len());
    let mut present = BTreeSet::new();
    for (index, page) in pages.iter().enumerate() {
        present.insert(page.id.0);
        if ids.binary_search_by_key(&page.id.0, |id| id.0).is_ok() {
            if !page.activation.terms.is_empty() {
                return Err(JointFocusError::NonemptyInitialPredicate(page.id));
            }
            indices.push((page.id, index));
        }
    }
    for id in &ids {
        if !present.contains(&id.0) {
            return Err(JointFocusError::MissingEditablePage(*id));
        }
    }
    indices.sort_unstable_by_key(|(id, _)| id.0);

    let basis = feature_bank(config, local.max_degree);
    if basis.len() > local.max_basis_terms {
        return Err(JointFocusError::InvalidPolicy(
            "basis exceeds declared bound",
        ));
    }
    let worst_case = (basis.len() as u64)
        .checked_add(1)
        .and_then(|features| features.checked_mul(ids.len() as u64))
        .and_then(|features| features.checked_mul(local.max_rounds as u64))
        .ok_or(JointFocusError::CandidateBudget)?;
    if worst_case > policy.max_candidate_evaluations {
        return Err(JointFocusError::CandidateBudget);
    }

    let baseline_train = error_bits(&baseline, train).map_err(JointFocusError::Split)?;
    let mut candidate_pages = pages.to_vec();
    let mut train_error = baseline_train;
    let mut evaluated = 0u64;
    let mut accepted = Vec::new();

    for _ in 0..local.max_rounds {
        let mut best_error = train_error;
        let mut best_pages = None;
        let mut best_flip = None;

        for (id, index) in indices.iter().copied() {
            for feature in 0..=basis.len() {
                let mut trial = candidate_pages.clone();
                let activation = &mut trial[index].activation;
                let toggled_term = if feature == 0 {
                    activation.bias ^= true;
                    None
                } else {
                    let term = basis[feature - 1];
                    if let Some(position) = activation.terms.iter().position(|x| *x == term) {
                        activation.terms.remove(position);
                    } else {
                        if activation.terms.len() >= local.max_terms {
                            continue;
                        }
                        activation.terms.push(term);
                    }
                    Some(term)
                };
                let engine = BooleanAssociativeFocus::new(config, trial.clone())
                    .map_err(JointFocusError::Engine)?;
                let loss = error_bits(&engine, train).map_err(JointFocusError::Split)?;
                evaluated += 1;
                if loss < best_error {
                    best_error = loss;
                    best_flip = Some((id, toggled_term));
                    best_pages = Some(trial);
                }
            }
        }
        let Some(winner) = best_pages else {
            break;
        };
        candidate_pages = winner;
        train_error = best_error;
        let (id, term) = best_flip.expect("a strictly improving candidate has a flip");
        accepted.push(JointFocusAcceptedFlip {
            page: id,
            term,
            train_errors_after: best_error,
        });
    }

    let candidate = BooleanAssociativeFocus::new(config, candidate_pages.clone())
        .map_err(JointFocusError::Engine)?;
    // Consult validation targets only after the entire TRAIN search is frozen.
    let baseline_validation = error_bits(&baseline, validation).map_err(JointFocusError::Split)?;
    let validation_error = error_bits(&candidate, validation).map_err(JointFocusError::Split)?;
    let admitted = train_error < baseline_train && validation_error <= baseline_validation;
    let changed_pages = indices
        .iter()
        .filter(|(_, index)| pages[*index].activation != candidate_pages[*index].activation)
        .count();
    Ok(JointFocusResult {
        model: if admitted { candidate } else { baseline },
        report: JointFocusReport {
            train_baseline_errors: baseline_train,
            validation_baseline_errors: baseline_validation,
            train_candidate_errors: train_error,
            validation_candidate_errors: validation_error,
            examined_train_candidates: evaluated,
            accepted_flips: accepted,
            changed_pages,
            admitted,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean_associative_focus::AnfActivation;

    fn cfg() -> AssociativeFocusConfig {
        AssociativeFocusConfig {
            event_bits: 3,
            state_bits: 4,
            max_selected_pages: 2,
        }
    }

    fn page(id: u32, priority: u32, write_mask: u64, write_value: u64) -> AssociativeFocusPage {
        AssociativeFocusPage {
            id: PageId(id),
            priority,
            activation: AnfActivation {
                bias: false,
                terms: vec![],
            },
            write_mask,
            write_value,
        }
    }

    fn splits() -> (Vec<FocusTransition>, Vec<FocusTransition>) {
        // Two independent non-linear gates, one per memory page. Event bit 2
        // is an unused nuisance variable held out across TRAIN/VAL. No oracle
        // route labels or hidden ANF monomials enter the fitting interface.
        let mut train = Vec::new();
        let mut validation = Vec::new();
        for e in 0u64..4 {
            for s in 0u64..4 {
                let target = s
                    | if e & 1 != 0 && s & 1 != 0 { 0b0100 } else { 0 }
                    | if e & 2 != 0 && s & 2 != 0 { 0b1000 } else { 0 };
                train.push(FocusTransition {
                    case_id: e * 4 + s,
                    event: e,
                    state: s,
                    expected_next_state: target,
                });
                validation.push(FocusTransition {
                    case_id: 100 + e * 4 + s,
                    event: e | 0b100,
                    state: s,
                    expected_next_state: target,
                });
            }
        }
        (train, validation)
    }

    #[test]
    fn learns_two_independent_memory_focus_rules_without_route_labels() {
        let (train, val) = splits();
        // Reverse page order and editable order: deterministic result anyway.
        let pages = [page(20, 5, 0b1000, 0b1000), page(10, 5, 0b0100, 0b0100)];
        let result = fit_joint_focus(
            cfg(),
            &pages,
            &[PageId(20), PageId(10)],
            &train,
            &val,
            JointFocusPolicy::default(),
        )
        .unwrap();
        assert!(result.report.admitted);
        assert_eq!(result.report.train_baseline_errors, 8);
        assert_eq!(result.report.train_candidate_errors, 0);
        assert_eq!(result.report.validation_candidate_errors, 0);
        assert_eq!(result.report.changed_pages, 2);
        assert_eq!(result.report.accepted_flips.len(), 2);
        assert_eq!(result.model.step(3, 3).unwrap().next_state, 15);
        assert_eq!(result.model.step(0, 3).unwrap().next_state, 3);
    }

    #[test]
    fn validation_veto_rolls_back_entire_joint_model() {
        let (train, mut val) = splits();
        // On the sealed validation input panel the observed transition is
        // identity. Train-optimal activations must therefore be vetoed.
        for sample in &mut val {
            sample.expected_next_state = sample.state;
        }
        let pages = [page(20, 5, 0b1000, 0b1000), page(10, 5, 0b0100, 0b0100)];
        let result = fit_joint_focus(
            cfg(),
            &pages,
            &[PageId(10), PageId(20)],
            &train,
            &val,
            JointFocusPolicy::default(),
        )
        .unwrap();
        assert!(!result.report.admitted);
        assert_eq!(result.report.validation_baseline_errors, 0);
        assert_eq!(result.report.validation_candidate_errors, 8);
        assert_eq!(result.model.step(3, 3).unwrap().next_state, 3);
    }

    #[test]
    fn candidate_budget_rejects_before_learning() {
        let (train, val) = splits();
        let pages = [page(20, 5, 8, 8), page(10, 5, 4, 4)];
        let result = fit_joint_focus(
            cfg(),
            &pages,
            &[PageId(20), PageId(10)],
            &train,
            &val,
            JointFocusPolicy {
                max_candidate_evaluations: 1,
                ..JointFocusPolicy::default()
            },
        );
        assert_eq!(result, Err(JointFocusError::CandidateBudget));
    }

    #[test]
    fn duplicate_missing_and_cross_split_alias_fail_closed() {
        let (train, mut val) = splits();
        let pages = [page(20, 5, 8, 8), page(10, 5, 4, 4)];
        assert_eq!(
            fit_joint_focus(
                cfg(),
                &pages,
                &[PageId(10), PageId(10)],
                &train,
                &val,
                JointFocusPolicy::default()
            ),
            Err(JointFocusError::DuplicateEditablePage(PageId(10)))
        );
        assert_eq!(
            fit_joint_focus(
                cfg(),
                &pages,
                &[PageId(10), PageId(99)],
                &train,
                &val,
                JointFocusPolicy::default()
            ),
            Err(JointFocusError::MissingEditablePage(PageId(99)))
        );
        val[0].event = train[0].event;
        val[0].state = train[0].state;
        assert_eq!(
            fit_joint_focus(
                cfg(),
                &pages,
                &[PageId(10), PageId(20)],
                &train,
                &val,
                JointFocusPolicy::default()
            ),
            Err(JointFocusError::Split(FocusFitError::SplitInputOverlap {
                event: train[0].event,
                state: train[0].state
            }))
        );
    }

    #[test]
    fn priority_conflicts_are_evaluated_by_original_baf0_oracle() {
        // Competing pages write the same state bit. Page 10 has priority 2
        // and writes 1. Page 20 has priority 1 and writes 0. When both are
        // active, priority 2 must win, even if page 20 is learned first.
        let cfg = AssociativeFocusConfig {
            event_bits: 2,
            state_bits: 2,
            max_selected_pages: 2,
        };
        let mut high = page(10, 2, 0b10, 0b10);
        high.activation.terms = vec![AnfMonomial {
            event_mask: 1,
            state_mask: 0,
        }];
        let low = page(20, 1, 0b10, 0);
        let pages = [high, low];
        let mut train = Vec::new();
        let mut val = Vec::new();
        for event in 0u64..4 {
            // Lower priority clears bit 1 only if event bit 1 is present
            // and no high-priority writer is active; high always wins ties.
            let target = if event & 1 != 0 {
                0b11
            } else if event & 2 != 0 {
                0b01
            } else {
                0b11
            };
            train.push(FocusTransition {
                case_id: event,
                event,
                state: 0b11,
                expected_next_state: target,
            });
            val.push(FocusTransition {
                case_id: 10 + event,
                event,
                state: 0b10,
                expected_next_state: target & 0b10,
            });
        }
        // Learn only low activation, while the high rule stays frozen.
        let result = fit_joint_focus(
            cfg,
            &pages,
            &[PageId(20)],
            &train,
            &val,
            JointFocusPolicy::default(),
        )
        .unwrap();
        assert!(result.report.admitted);
        let both = result.model.step(3, 0b11).unwrap();
        assert_eq!(both.selected, vec![PageId(10), PageId(20)]);
        assert_eq!(both.next_state, 0b11);
        assert_eq!(result.model.step(2, 0b11).unwrap().next_state, 0b01);
    }
}
