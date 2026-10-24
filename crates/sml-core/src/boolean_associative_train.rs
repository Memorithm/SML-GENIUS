//! BAF-1: train-only, task-loss-driven discovery of Boolean memory activation.
//!
//! No query/key/value projections, similarity weights, softmax or numerical
//! attention matrix. Monomials are generated from raw event/state bit positions
//! and selected by deterministic greedy GF(2) coefficient flips.
//!
//! This is intentionally limited: only one page's activation is fitted, with
//! fixed write mask/value, fixed page priority and all other pages frozen.
//! Validation can veto a frozen train winner, but never chooses its terms.
//! Test/OOD data must remain sealed and is not accepted by this interface.

use std::collections::BTreeSet;

use crate::{
    boolean_associative_focus::{
        AnfMonomial, AssociativeFocusConfig, AssociativeFocusError, AssociativeFocusPage,
        BooleanAssociativeFocus,
    },
    PageId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusTransition {
    pub case_id: u64,
    pub event: u64,
    pub state: u64,
    pub expected_next_state: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusFitPolicy {
    /// Maximum supported degree is 2, to keep the hypothesis class explicit.
    pub max_degree: usize,
    /// Maximum number of train-only greedy coefficient-flip iterations.
    pub max_rounds: usize,
    /// Upper bound on simultaneously active ANF monomials.
    pub max_terms: usize,
    /// Fail closed instead of silently truncating the candidate feature bank.
    pub max_basis_terms: usize,
    /// Bound total TRAIN + VALIDATION examples; never includes sealed TEST.
    pub max_samples: usize,
}

impl Default for FocusFitPolicy {
    fn default() -> Self {
        Self {
            max_degree: 2,
            max_rounds: 8,
            max_terms: 8,
            max_basis_terms: 136,
            max_samples: 8192,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusFitError {
    Policy(&'static str),
    MissingEditablePage(PageId),
    NonemptyInitialPredicate(PageId),
    EmptySplit,
    TooManySamples,
    DuplicateCaseId(u64),
    SplitInputOverlap { event: u64, state: u64 },
    OutOfRangeSample(u64),
    Engine(AssociativeFocusError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusFitReport {
    pub train_baseline_errors: u64,
    pub validation_baseline_errors: u64,
    pub train_candidate_errors: u64,
    pub validation_candidate_errors: u64,
    pub evaluated_train_candidates: u64,
    pub candidate_terms: Vec<AnfMonomial>,
    /// True only if TRAIN strictly improves and VALIDATION does not regress.
    pub admitted: bool,
}

/// An independent, immutable returned model and reproducible fit evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusFitResult {
    pub model: BooleanAssociativeFocus,
    pub report: FocusFitReport,
}

/// SML-native one-page Boolean attention-rule learner.
///
/// The train-only algorithm starts from a zero-term ANF predicate, then flips
/// either bias or one degree-1/2 monomial coefficient per round. At every
/// round, the first strict loss improvement in deterministic feature order
/// breaks ties. Training loss is total Hamming error on full next-state words.
///
/// The validation set is read **once** after search has frozen its candidate:
/// it is an admission/rollback guard, never an optimization signal. Sealed
/// independent TEST is outside this API and must be run separately.
pub fn fit_associative_focus(
    config: AssociativeFocusConfig,
    pages: &[AssociativeFocusPage],
    editable_id: PageId,
    train: &[FocusTransition],
    validation: &[FocusTransition],
    policy: FocusFitPolicy,
) -> Result<FocusFitResult, FocusFitError> {
    if !(1..=2).contains(&policy.max_degree)
        || policy.max_rounds == 0
        || policy.max_terms == 0
        || policy.max_basis_terms == 0
        || policy.max_samples == 0
    {
        return Err(FocusFitError::Policy("invalid search limits or degree"));
    }
    // The first experiment caps total input dimensions independently of u64
    // event/state representability, avoiding accidental quadratic growth.
    if config.event_bits.saturating_add(config.state_bits) > 16 {
        return Err(FocusFitError::Policy(
            "BAF-1 supports at most 16 total input bits",
        ));
    }
    if train.is_empty() || validation.is_empty() {
        return Err(FocusFitError::EmptySplit);
    }
    if train.len().saturating_add(validation.len()) > policy.max_samples {
        return Err(FocusFitError::TooManySamples);
    }

    let baseline =
        BooleanAssociativeFocus::new(config, pages.to_vec()).map_err(FocusFitError::Engine)?;
    let index = pages
        .iter()
        .position(|p| p.id == editable_id)
        .ok_or(FocusFitError::MissingEditablePage(editable_id))?;
    if !pages[index].activation.terms.is_empty() {
        return Err(FocusFitError::NonemptyInitialPredicate(editable_id));
    }
    validate_splits(config, train, validation)?;
    let basis = feature_bank(config, policy.max_degree);
    if basis.len() > policy.max_basis_terms {
        return Err(FocusFitError::Policy(
            "monomial bank exceeds declared budget",
        ));
    }

    let baseline_train = error_bits(&baseline, train)?;
    let baseline_val = error_bits(&baseline, validation)?;

    let mut best_pages = pages.to_vec();
    let mut current_train = baseline_train;
    let mut evaluated = 0u64;
    for _ in 0..policy.max_rounds {
        let mut round_best_train = current_train;
        let mut round_winner = None;

        // Candidate zero toggles the ANF constant; following candidates
        // toggle individual terms in a stable, documented basis order.
        for candidate_id in 0..=basis.len() {
            let mut trial = best_pages.clone();
            let predicate = &mut trial[index].activation;
            if candidate_id == 0 {
                predicate.bias = !predicate.bias;
            } else {
                let monomial = basis[candidate_id - 1];
                if let Some(pos) = predicate.terms.iter().position(|t| *t == monomial) {
                    predicate.terms.remove(pos);
                } else {
                    if predicate.terms.len() >= policy.max_terms {
                        continue;
                    }
                    predicate.terms.push(monomial);
                }
            }
            let engine = BooleanAssociativeFocus::new(config, trial.clone())
                .map_err(FocusFitError::Engine)?;
            let loss = error_bits(&engine, train)?;
            evaluated += 1;
            if loss < round_best_train {
                round_best_train = loss;
                round_winner = Some(trial);
            }
        }
        let Some(next_pages) = round_winner else {
            break;
        };
        best_pages = next_pages;
        current_train = round_best_train;
    }

    let candidate =
        BooleanAssociativeFocus::new(config, best_pages.clone()).map_err(FocusFitError::Engine)?;
    let candidate_val = error_bits(&candidate, validation)?;
    let admitted = current_train < baseline_train && candidate_val <= baseline_val;
    Ok(FocusFitResult {
        model: if admitted { candidate } else { baseline },
        report: FocusFitReport {
            train_baseline_errors: baseline_train,
            validation_baseline_errors: baseline_val,
            train_candidate_errors: current_train,
            validation_candidate_errors: candidate_val,
            evaluated_train_candidates: evaluated,
            candidate_terms: best_pages[index].activation.terms.clone(),
            admitted,
        },
    })
}

fn bit_mask(bits: usize) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

fn validate_splits(
    config: AssociativeFocusConfig,
    train: &[FocusTransition],
    validation: &[FocusTransition],
) -> Result<(), FocusFitError> {
    let mut ids = BTreeSet::new();
    let mut train_inputs = BTreeSet::new();
    let event_mask = bit_mask(config.event_bits);
    let state_mask = bit_mask(config.state_bits);
    for sample in train.iter().chain(validation) {
        if !ids.insert(sample.case_id) {
            return Err(FocusFitError::DuplicateCaseId(sample.case_id));
        }
        if sample.event & !event_mask != 0
            || sample.state & !state_mask != 0
            || sample.expected_next_state & !state_mask != 0
        {
            return Err(FocusFitError::OutOfRangeSample(sample.case_id));
        }
    }
    for sample in train {
        train_inputs.insert((sample.event, sample.state));
    }
    for sample in validation {
        if train_inputs.contains(&(sample.event, sample.state)) {
            return Err(FocusFitError::SplitInputOverlap {
                event: sample.event,
                state: sample.state,
            });
        }
    }
    Ok(())
}

fn feature_bank(config: AssociativeFocusConfig, max_degree: usize) -> Vec<AnfMonomial> {
    let total = config.event_bits + config.state_bits;
    let atom = |position: usize| -> AnfMonomial {
        if position < config.event_bits {
            AnfMonomial {
                event_mask: 1u64 << position,
                state_mask: 0,
            }
        } else {
            AnfMonomial {
                event_mask: 0,
                state_mask: 1u64 << (position - config.event_bits),
            }
        }
    };
    let atoms: Vec<_> = (0..total).map(atom).collect();
    let mut basis = atoms.clone();
    if max_degree == 2 {
        for i in 0..atoms.len() {
            for j in (i + 1)..atoms.len() {
                basis.push(AnfMonomial {
                    event_mask: atoms[i].event_mask | atoms[j].event_mask,
                    state_mask: atoms[i].state_mask | atoms[j].state_mask,
                });
            }
        }
    }
    basis
}

fn error_bits(
    engine: &BooleanAssociativeFocus,
    samples: &[FocusTransition],
) -> Result<u64, FocusFitError> {
    let mut sum = 0u64;
    for s in samples {
        let got = engine
            .step(s.event, s.state)
            .map_err(FocusFitError::Engine)?
            .next_state;
        sum += (got ^ s.expected_next_state).count_ones() as u64;
    }
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean_associative_focus::AnfActivation;

    fn cfg(event_bits: usize, state_bits: usize) -> AssociativeFocusConfig {
        AssociativeFocusConfig {
            event_bits,
            state_bits,
            max_selected_pages: 1,
        }
    }

    fn blank_page(mask: u64) -> AssociativeFocusPage {
        AssociativeFocusPage {
            id: PageId(7),
            priority: 10,
            activation: AnfActivation {
                bias: false,
                terms: Vec::new(),
            },
            write_mask: mask,
            write_value: mask,
        }
    }

    fn sample(case_id: u64, event: u64, state: u64, target: u64) -> FocusTransition {
        FocusTransition {
            case_id,
            event,
            state,
            expected_next_state: target,
        }
    }

    #[test]
    fn learns_a_cross_event_state_interaction_from_only_final_transitions() {
        // Hidden generator is *not* supplied to fit: it sees only final state.
        let mut train = Vec::new();
        let mut validation = Vec::new();
        for event in 0..4u64 {
            for state in 0..4u64 {
                let target = if event & 1 != 0 && state & 1 != 0 {
                    state | 0b100
                } else {
                    state
                };
                let s = sample(event * 4 + state, event, state, target);
                if (event ^ state) & 2 == 0 {
                    train.push(s);
                } else {
                    validation.push(s);
                }
            }
        }
        let out = fit_associative_focus(
            cfg(2, 3),
            &[blank_page(0b100)],
            PageId(7),
            &train,
            &validation,
            FocusFitPolicy::default(),
        )
        .unwrap();
        assert!(out.report.admitted);
        assert_eq!(out.report.train_candidate_errors, 0);
        assert_eq!(out.report.validation_candidate_errors, 0);
        assert_eq!(
            out.report.candidate_terms,
            vec![AnfMonomial {
                event_mask: 1,
                state_mask: 1,
            }]
        );
        assert_eq!(out.model.step(1, 1).unwrap().next_state, 0b101);
        assert_eq!(out.model.step(2, 1).unwrap().next_state, 1);
    }

    #[test]
    fn validation_vetoes_a_train_only_spurious_rule_without_changing_baseline() {
        let train = [sample(0, 0, 0, 0), sample(1, 1, 0, 1)];
        let validation = [sample(2, 2, 0, 0), sample(3, 3, 0, 0)];
        let policy = FocusFitPolicy {
            max_degree: 1,
            max_rounds: 1,
            ..FocusFitPolicy::default()
        };
        let out = fit_associative_focus(
            cfg(2, 1),
            &[blank_page(1)],
            PageId(7),
            &train,
            &validation,
            policy,
        )
        .unwrap();
        assert_eq!(out.report.train_baseline_errors, 1);
        assert_eq!(out.report.train_candidate_errors, 0);
        assert_eq!(out.report.validation_baseline_errors, 0);
        assert_eq!(out.report.validation_candidate_errors, 1);
        assert!(!out.report.admitted);
        assert_eq!(out.model.step(1, 0).unwrap().next_state, 0);
    }

    #[test]
    fn degree_one_control_cannot_fit_a_genuine_conjunction() {
        let train: Vec<_> = (0..2u64)
            .flat_map(|event| {
                (0..2u64).map(move |state| {
                    sample(
                        2 * event + state,
                        event,
                        state,
                        if event == 1 && state == 1 {
                            state | 0b10
                        } else {
                            state
                        },
                    )
                })
            })
            .collect();
        let validation = [
            sample(4, 2, 0, 0),
            sample(5, 2, 1, 1),
            sample(6, 3, 0, 0),
            sample(7, 3, 1, 0b11),
        ];
        let out = fit_associative_focus(
            cfg(2, 2),
            &[blank_page(0b10)],
            PageId(7),
            &train,
            &validation,
            FocusFitPolicy {
                max_degree: 1,
                ..FocusFitPolicy::default()
            },
        )
        .unwrap();
        assert!(!out.report.admitted);
        assert_eq!(out.report.train_baseline_errors, 1);
        assert_eq!(out.report.train_candidate_errors, 1);
    }

    #[test]
    fn duplicate_ids_and_leaked_inputs_are_rejected() {
        let base = [blank_page(1)];
        let policy = FocusFitPolicy::default();
        assert_eq!(
            fit_associative_focus(
                cfg(2, 1),
                &base,
                PageId(7),
                &[sample(1, 0, 0, 0)],
                &[sample(1, 1, 0, 0)],
                policy
            ),
            Err(FocusFitError::DuplicateCaseId(1))
        );
        assert_eq!(
            fit_associative_focus(
                cfg(2, 1),
                &base,
                PageId(7),
                &[sample(1, 0, 0, 0)],
                &[sample(2, 0, 0, 0)],
                policy
            ),
            Err(FocusFitError::SplitInputOverlap { event: 0, state: 0 })
        );
    }

    #[test]
    fn invalid_width_and_candidate_budget_fail_closed() {
        let base = [blank_page(1)];
        assert_eq!(
            fit_associative_focus(
                cfg(2, 1),
                &base,
                PageId(7),
                &[sample(0, 4, 0, 0)],
                &[sample(1, 0, 0, 0)],
                FocusFitPolicy::default()
            ),
            Err(FocusFitError::OutOfRangeSample(0))
        );
        assert_eq!(
            fit_associative_focus(
                cfg(2, 1),
                &base,
                PageId(7),
                &[sample(0, 1, 0, 1)],
                &[sample(1, 2, 0, 0)],
                FocusFitPolicy {
                    max_basis_terms: 1,
                    ..FocusFitPolicy::default()
                }
            ),
            Err(FocusFitError::Policy(
                "monomial bank exceeds declared budget"
            ))
        );
    }
}
