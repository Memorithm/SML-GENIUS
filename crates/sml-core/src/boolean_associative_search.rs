//! BAF-SEARCH: alternative TRAIN-side search strategies for the BAF joint
//! focus fitter, compared against the k-flip escape of
//! [`fit_joint_focus`](crate::boolean_associative_joint::fit_joint_focus)
//! under the SAME candidate-evaluation counter, splits and admission rule.
//!
//! * [`fit_joint_focus_beam`]: deterministic beam search over single
//!   coefficient flips (no multi-flip escapes). Non-improving children may
//!   survive in the beam, which is the only way it can cross plateaus.
//! * [`fit_joint_focus_gf2`]: an exact GF(2) linear solve. An ANF predicate is
//!   linear in its monomial coefficients, so once a per-sample desired
//!   activation is known, fitting the predicate is solving `A x = y` over
//!   GF(2) on the degree <= `max_degree` feature bank. Desired activations are
//!   probed per page from TRAIN only (page forced on vs. off, other editable
//!   pages held at baseline). This assumes editable pages do not interact
//!   through priority/conflict. An inconsistent system is NOT patched up: the
//!   search returns the baseline (no change, not admitted).
//!
//! Both reuse `prepare`/`finish`, so validation is consulted exactly once after
//! the TRAIN candidate is frozen, and over-budget runs fail closed with
//! [`JointFocusError::CandidateBudget`] before any evaluation.

use std::collections::BTreeSet;

use crate::{
    boolean_associative_focus::{
        AnfMonomial, AssociativeFocusConfig, AssociativeFocusPage, BooleanAssociativeFocus,
    },
    boolean_associative_joint::{
        finish, prepare, JointFocusAcceptedFlip, JointFocusError, JointFocusPolicy,
        JointFocusResult, Search,
    },
    boolean_associative_train::{error_bits, FocusTransition},
    PageId,
};

type Key = Vec<(bool, Vec<(u64, u64)>)>;

fn key(pages: &[AssociativeFocusPage], indices: &[(PageId, usize)]) -> Key {
    indices
        .iter()
        .map(|&(_, i)| {
            let a = &pages[i].activation;
            let mut t: Vec<(u64, u64)> = a
                .terms
                .iter()
                .map(|m| (m.event_mask, m.state_mask))
                .collect();
            t.sort_unstable();
            (a.bias, t)
        })
        .collect()
}

/// Toggle one coefficient (feature 0 = bias). Returns None if adding the term
/// would exceed `max_terms`.
fn toggle(
    pages: &[AssociativeFocusPage],
    index: usize,
    feature: usize,
    basis: &[AnfMonomial],
    max_terms: usize,
) -> Option<(Vec<AssociativeFocusPage>, Option<AnfMonomial>)> {
    let mut trial = pages.to_vec();
    let a = &mut trial[index].activation;
    if feature == 0 {
        a.bias ^= true;
        return Some((trial, None));
    }
    let term = basis[feature - 1];
    if let Some(pos) = a.terms.iter().position(|x| *x == term) {
        a.terms.remove(pos);
    } else {
        if a.terms.len() >= max_terms {
            return None;
        }
        a.terms.push(term);
    }
    Some((trial, Some(term)))
}

struct Node {
    loss: u64,
    pages: Vec<AssociativeFocusPage>,
    path: Vec<JointFocusAcceptedFlip>,
}

/// Deterministic beam search over single coefficient flips.
///
/// Each of `max_rounds` layers expands every beam member by every single flip
/// (pages ascending; bias, then the feature bank in order), skipping states
/// already evaluated. The `beam_width` lowest-loss children (ties: generation
/// order) form the next layer. The best TRAIN state ever seen (strict
/// improvement, first wins) is the frozen candidate. Preflight worst case is
/// `beam_width * (basis + 1) * pages * max_rounds`; above
/// `max_candidate_evaluations` the fit fails closed.
pub fn fit_joint_focus_beam(
    config: AssociativeFocusConfig,
    pages: &[AssociativeFocusPage],
    editable: &[PageId],
    train: &[FocusTransition],
    validation: &[FocusTransition],
    policy: JointFocusPolicy,
    beam_width: usize,
) -> Result<JointFocusResult, JointFocusError> {
    if beam_width == 0 {
        return Err(JointFocusError::InvalidPolicy("beam width must be > 0"));
    }
    let local = policy.local;
    let prep = prepare(config, pages, editable, train, validation, policy)?;
    let worst_case = (prep.basis.len() as u64 + 1)
        .checked_mul(prep.ids.len() as u64)
        .and_then(|x| x.checked_mul(local.max_rounds as u64))
        .and_then(|x| x.checked_mul(beam_width as u64))
        .ok_or(JointFocusError::CandidateBudget)?;
    if worst_case > policy.max_candidate_evaluations {
        return Err(JointFocusError::CandidateBudget);
    }

    let baseline_train = error_bits(&prep.baseline, train).map_err(JointFocusError::Split)?;
    let mut seen: BTreeSet<Key> = BTreeSet::new();
    seen.insert(key(pages, &prep.indices));
    let mut beam = vec![Node {
        loss: baseline_train,
        pages: pages.to_vec(),
        path: Vec::new(),
    }];
    let mut best: Option<Node> = None;
    let mut best_loss = baseline_train;
    let mut evaluated = 0u64;

    for _ in 0..local.max_rounds {
        if best_loss == 0 {
            break;
        }
        let mut children: Vec<Node> = Vec::new();
        for parent in &beam {
            for &(id, index) in &prep.indices {
                for feature in 0..=prep.basis.len() {
                    let Some((trial, term)) =
                        toggle(&parent.pages, index, feature, &prep.basis, local.max_terms)
                    else {
                        continue;
                    };
                    if !seen.insert(key(&trial, &prep.indices)) {
                        continue;
                    }
                    let engine = BooleanAssociativeFocus::new(config, trial.clone())
                        .map_err(JointFocusError::Engine)?;
                    let loss = error_bits(&engine, train).map_err(JointFocusError::Split)?;
                    evaluated += 1;
                    let mut path = parent.path.clone();
                    path.push(JointFocusAcceptedFlip {
                        page: id,
                        term,
                        train_errors_after: loss,
                    });
                    children.push(Node {
                        loss,
                        pages: trial,
                        path,
                    });
                }
            }
        }
        if children.is_empty() {
            break;
        }
        // Stable sort keeps generation order among equal losses.
        children.sort_by_key(|n| n.loss);
        children.truncate(beam_width);
        if children[0].loss < best_loss {
            best_loss = children[0].loss;
            best = Some(Node {
                loss: children[0].loss,
                pages: children[0].pages.clone(),
                path: children[0].path.clone(),
            });
        }
        beam = children;
    }

    let (train_error, candidate_pages, accepted) = match best {
        Some(n) => (n.loss, n.pages, n.path),
        None => (baseline_train, pages.to_vec(), Vec::new()),
    };
    finish(
        config,
        pages,
        &prep.indices,
        prep.baseline,
        candidate_pages,
        validation,
        Search {
            baseline_train,
            train_error,
            evaluated,
            accepted,
        },
    )
}

fn monomial_active(m: AnfMonomial, event: u64, state: u64) -> bool {
    event & m.event_mask == m.event_mask && state & m.state_mask == m.state_mask
}

/// Solve `rows * x = rhs` over GF(2); each row is a bitset over `cols`
/// columns. Pivots are chosen left to right (bias, then low-degree monomials
/// first) and free variables are 0, so a solution supported on independent
/// low-degree columns is preferred. None if inconsistent.
fn solve_gf2(mut rows: Vec<(Vec<u64>, bool)>, cols: usize) -> Option<Vec<bool>> {
    let mut pivots: Vec<(usize, usize)> = Vec::new(); // (row, col)
    let mut r = 0;
    for c in 0..cols {
        let (w, b) = (c / 64, 1u64 << (c % 64));
        let Some(p) = (r..rows.len()).find(|&i| rows[i].0[w] & b != 0) else {
            continue;
        };
        rows.swap(r, p);
        let (pivot_bits, pivot_rhs) = rows[r].clone();
        for (i, row) in rows.iter_mut().enumerate() {
            if i != r && row.0[w] & b != 0 {
                for (x, y) in row.0.iter_mut().zip(&pivot_bits) {
                    *x ^= *y;
                }
                row.1 ^= pivot_rhs;
            }
        }
        pivots.push((r, c));
        r += 1;
        if r == rows.len() {
            break;
        }
    }
    if rows[r..].iter().any(|(_, rhs)| *rhs) {
        return None;
    }
    let mut x = vec![false; cols];
    for (row, col) in pivots {
        x[col] = rows[row].1;
    }
    Some(x)
}

/// Exact GF(2) solve of each editable page's ANF on the degree <=
/// `max_degree` feature bank (see module docs). Evaluation count: two TRAIN
/// probes per editable page plus one frozen-candidate pass, preflighted
/// against `max_candidate_evaluations`. A solution with more than
/// `max_terms` monomials fails closed (CandidateBudget). If any page's system
/// is inconsistent on TRAIN, the baseline is returned (honest failure).
pub fn fit_joint_focus_gf2(
    config: AssociativeFocusConfig,
    pages: &[AssociativeFocusPage],
    editable: &[PageId],
    train: &[FocusTransition],
    validation: &[FocusTransition],
    policy: JointFocusPolicy,
) -> Result<JointFocusResult, JointFocusError> {
    let local = policy.local;
    let prep = prepare(config, pages, editable, train, validation, policy)?;
    let worst_case = (prep.ids.len() as u64)
        .checked_mul(2)
        .and_then(|x| x.checked_add(1))
        .ok_or(JointFocusError::CandidateBudget)?;
    if worst_case > policy.max_candidate_evaluations {
        return Err(JointFocusError::CandidateBudget);
    }
    let baseline_train = error_bits(&prep.baseline, train).map_err(JointFocusError::Split)?;
    let cols = prep.basis.len() + 1;
    let words = cols.div_ceil(64);
    let mut evaluated = 0u64;
    let mut candidate_pages = pages.to_vec();
    let mut accepted = Vec::new();
    let mut consistent = true;

    for &(id, index) in &prep.indices {
        let per_sample = |forced: bool| -> Result<Vec<u64>, JointFocusError> {
            let mut trial = pages.to_vec();
            trial[index].activation.bias = forced;
            trial[index].activation.terms.clear();
            let engine =
                BooleanAssociativeFocus::new(config, trial).map_err(JointFocusError::Engine)?;
            train
                .iter()
                .map(|s| {
                    engine
                        .step(s.event, s.state)
                        .map(|st| (st.next_state ^ s.expected_next_state).count_ones() as u64)
                        .map_err(JointFocusError::Engine)
                })
                .collect()
        };
        let off = per_sample(false)?;
        let on = per_sample(true)?;
        evaluated += 2;
        let mut rows = Vec::new();
        for (k, s) in train.iter().enumerate() {
            if on[k] == off[k] {
                continue; // don't care
            }
            let mut bits = vec![0u64; words];
            bits[0] |= 1; // bias column
            for (j, m) in prep.basis.iter().enumerate() {
                if monomial_active(*m, s.event, s.state) {
                    bits[(j + 1) / 64] |= 1u64 << ((j + 1) % 64);
                }
            }
            rows.push((bits, on[k] < off[k]));
        }
        if rows.is_empty() {
            continue;
        }
        let Some(x) = solve_gf2(rows, cols) else {
            consistent = false;
            break;
        };
        let terms: Vec<AnfMonomial> = (1..cols)
            .filter(|&c| x[c])
            .map(|c| prep.basis[c - 1])
            .collect();
        if terms.len() > local.max_terms {
            return Err(JointFocusError::CandidateBudget);
        }
        let a = &mut candidate_pages[index].activation;
        a.bias = x[0];
        if x[0] {
            accepted.push(JointFocusAcceptedFlip {
                page: id,
                term: None,
                train_errors_after: 0,
            });
        }
        for t in &terms {
            accepted.push(JointFocusAcceptedFlip {
                page: id,
                term: Some(*t),
                train_errors_after: 0,
            });
        }
        a.terms = terms;
    }

    let (train_error, candidate_pages, accepted) = if consistent {
        let engine = BooleanAssociativeFocus::new(config, candidate_pages.clone())
            .map_err(JointFocusError::Engine)?;
        let loss = error_bits(&engine, train).map_err(JointFocusError::Split)?;
        evaluated += 1;
        let accepted = accepted
            .into_iter()
            .map(|f| JointFocusAcceptedFlip {
                train_errors_after: loss,
                ..f
            })
            .collect();
        (loss, candidate_pages, accepted)
    } else {
        (baseline_train, pages.to_vec(), Vec::new())
    };
    finish(
        config,
        pages,
        &prep.indices,
        prep.baseline,
        candidate_pages,
        validation,
        Search {
            baseline_train,
            train_error,
            evaluated,
            accepted,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean_associative_focus::AnfActivation;
    use crate::boolean_associative_train::FocusFitPolicy;

    fn cfg() -> AssociativeFocusConfig {
        AssociativeFocusConfig {
            event_bits: 3,
            state_bits: 4,
            max_selected_pages: 2,
        }
    }

    fn atlas() -> Vec<AssociativeFocusPage> {
        [(10u32, 4u64), (20, 8)]
            .into_iter()
            .map(|(id, mask)| AssociativeFocusPage {
                id: PageId(id),
                priority: 100 - id,
                activation: AnfActivation {
                    bias: false,
                    terms: vec![],
                },
                write_mask: mask,
                write_value: mask,
            })
            .collect()
    }

    /// Page 10 writes bit 2 = e0 ^ e1 ^ s0 (parity3, degree 1); page 20
    /// writes bit 3 = e2 & s1 (degree 2). Inputs with state bits 2,3 clear.
    fn splits(f: fn(u64, u64) -> (bool, bool)) -> (Vec<FocusTransition>, Vec<FocusTransition>) {
        let (mut tr, mut va) = (Vec::new(), Vec::new());
        for e in 0u64..8 {
            for s in 0u64..4 {
                let (a, b) = f(e, s);
                let t = FocusTransition {
                    case_id: e * 4 + s,
                    event: e,
                    state: s,
                    expected_next_state: s | if a { 4 } else { 0 } | if b { 8 } else { 0 },
                };
                if (e + s) % 4 == 0 {
                    va.push(FocusTransition {
                        case_id: 100 + t.case_id,
                        ..t
                    });
                } else {
                    tr.push(t);
                }
            }
        }
        (tr, va)
    }

    fn policy(degree: usize) -> JointFocusPolicy {
        JointFocusPolicy {
            local: FocusFitPolicy {
                max_degree: degree,
                ..FocusFitPolicy::default()
            },
            ..JointFocusPolicy::default()
        }
    }

    fn mixed(e: u64, s: u64) -> (bool, bool) {
        (
            (e ^ (e >> 1) ^ s) & 1 == 1,
            (e >> 2) & 1 == 1 && (s >> 1) & 1 == 1,
        )
    }

    #[test]
    fn gf2_is_exact_when_target_is_in_span() {
        let (tr, va) = splits(mixed);
        let ids = [PageId(10), PageId(20)];
        let r = fit_joint_focus_gf2(cfg(), &atlas(), &ids, &tr, &va, policy(2)).unwrap();
        assert_eq!(r.report.train_candidate_errors, 0);
        assert_eq!(r.report.validation_candidate_errors, 0);
        assert!(r.report.admitted);
        assert_eq!(r.report.examined_train_candidates, 5);
    }

    #[test]
    fn gf2_fails_honestly_when_out_of_span() {
        // Degree-2 target, degree-1 bank: inconsistent -> baseline.
        let (tr, va) = splits(mixed);
        let ids = [PageId(10), PageId(20)];
        let r = fit_joint_focus_gf2(cfg(), &atlas(), &ids, &tr, &va, policy(1)).unwrap();
        assert!(!r.report.admitted);
        assert_eq!(
            r.report.train_candidate_errors,
            r.report.train_baseline_errors
        );
        assert!(r.report.accepted_flips.is_empty());
    }

    #[test]
    fn gf2_budget_fails_closed() {
        let (tr, va) = splits(mixed);
        let ids = [PageId(10), PageId(20)];
        let p = JointFocusPolicy {
            max_candidate_evaluations: 4,
            ..policy(2)
        };
        assert_eq!(
            fit_joint_focus_gf2(cfg(), &atlas(), &ids, &tr, &va, p).unwrap_err(),
            JointFocusError::CandidateBudget
        );
    }

    #[test]
    fn beam_budget_fails_closed_and_is_counted() {
        let (tr, va) = splits(mixed);
        let ids = [PageId(10), PageId(20)];
        let p = policy(2);
        // basis(2) at 7 bits = 7 + 21 = 28; (28+1)*2*8*4 = 1856 <= 5000.
        let r = fit_joint_focus_beam(cfg(), &atlas(), &ids, &tr, &va, p, 4).unwrap();
        assert!(r.report.examined_train_candidates <= 1856);
        assert!(r.report.train_candidate_errors < r.report.train_baseline_errors);
        assert_eq!(
            fit_joint_focus_beam(cfg(), &atlas(), &ids, &tr, &va, p, 3_000).unwrap_err(),
            JointFocusError::CandidateBudget
        );
        assert!(matches!(
            fit_joint_focus_beam(cfg(), &atlas(), &ids, &tr, &va, p, 0),
            Err(JointFocusError::InvalidPolicy(_))
        ));
    }

    #[test]
    fn beam_is_deterministic() {
        let (tr, va) = splits(mixed);
        let ids = [PageId(10), PageId(20)];
        let a = fit_joint_focus_beam(cfg(), &atlas(), &ids, &tr, &va, policy(2), 4).unwrap();
        let b = fit_joint_focus_beam(cfg(), &atlas(), &ids, &tr, &va, policy(2), 4).unwrap();
        assert_eq!(a.report, b.report);
    }

    #[test]
    fn solver_prefers_low_columns_and_detects_inconsistency() {
        // x0 ^ x1 = 1, x1 = 1 -> x0 = 0, x1 = 1.
        let rows = vec![(vec![0b11u64], true), (vec![0b10u64], true)];
        assert_eq!(solve_gf2(rows, 3), Some(vec![false, true, false]));
        let bad = vec![(vec![0b1u64], true), (vec![0b1u64], false)];
        assert_eq!(solve_gf2(bad, 1), None);
    }
}
