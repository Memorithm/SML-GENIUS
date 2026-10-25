//! BAF-H4 — causal train-only GF(2) gate for a frozen neural H-1d head.
//!
//! Native Boolean gate over *decoder-visible* P1 context and bank state.
//! All competing arms retain the same P1 counts, H-1d weights and coder.
//! Learning changes only ANF coefficients, never the neural weights.
//! Research-only: an additive router result cannot establish superiority of
//! a standalone Boolean language model over a neural network.
//!
//! Protocol: docs/BAF-H4-NEURAL-COMPRESSION-PREREGISTRATION.md.

use std::collections::BTreeSet;

use crate::{
    boolean_associative_focus::{AnfActivation, AnfMonomial},
    hyb_h1_residual::a0_cost,
    hyb_h1d_bank_head::{bank_word, BankHead, BankRecord},
};

const DOMAIN: usize = 256;
const SCALE: u64 = 1_000_000;
const MAX_EVALUATIONS: usize = 222;
const DEFAULT_VALIDATION_IMPROVEMENT_MICROBITS_PER_BYTE: u64 = 10_000;

/// The 8 frozen, decoder-visible bits: 4 context-word low bits and
/// one positive thermometer from each of the first 4 P1 banks, at depth 0.
/// In particular: never reads the current target byte or later bank depth.
#[must_use]
pub fn causal_features(record: &BankRecord) -> (u8, u8) {
    let event = (record.record.context & 15) as u8;
    let word = bank_word(&record.banks.stretch[0]);
    let state = (0..4).fold(0u8, |result, bank| {
        result | (((word >> (6 * bank)) & 1) as u8) << bank
    });
    (event, state)
}

/// An independently calculated scalar feature extractor for the H-3 oracle.
/// Deliberately does not call `causal_features` or `bank_word`.
#[must_use]
pub fn oracle_causal_features(record: &BankRecord) -> (u8, u8) {
    let event = record.record.context as u8 & 15;
    let mut state = 0u8;
    for (bank, &q) in record.banks.stretch[0].iter().take(4).enumerate() {
        if q >= 4 {
            state |= 1 << bank;
        }
    }
    (event, state)
}

#[must_use]
pub fn gate_is_active(gate: &AnfActivation, record: &BankRecord) -> bool {
    let (event, state) = causal_features(record);
    gate.is_active(u64::from(event), u64::from(state))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadGateObservation {
    pub doc_index: u32,
    pub position: u32,
    pub event: u8,
    pub state: u8,
    /// P1-only code length, integer microbits for all 8 current-byte bits.
    pub p1_microbits: u64,
    /// Same P1 plus the FROZEN H-1d head on all 8 current-byte bits.
    pub neural_microbits: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadGateError {
    InvalidScore,
    InputOutOfRange,
    NonMonotonicRow,
    TooManySamples,
    EmptySplit,
    TrainValidationDocOverlap(u32),
    InvalidPolicy,
    CandidateBudget,
}

impl core::fmt::Display for HeadGateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "BAF-H4 gate error: {self:?}")
    }
}

impl std::error::Error for HeadGateError {}

fn microbits(bits: f64) -> Result<u64, HeadGateError> {
    if !bits.is_finite() || bits < 0.0 || bits > u64::MAX as f64 / SCALE as f64 {
        return Err(HeadGateError::InvalidScore);
    }
    Ok((bits * SCALE as f64).round() as u64)
}

impl HeadGateObservation {
    /// Both true label costs are computed only for *TRAIN or CAL-SEL*
    /// evidence. The features themselves are causal and label-free.
    pub fn from_record(
        record: &BankRecord,
        head: &BankHead,
        eighths: i32,
    ) -> Result<Self, HeadGateError> {
        let (event, state) = causal_features(record);
        Ok(Self {
            doc_index: record.record.doc_index,
            position: record.record.position,
            event,
            state,
            p1_microbits: microbits(a0_cost(&record.record))?,
            neural_microbits: microbits(head.cost(record, eighths))?,
        })
    }

    const fn index(self) -> usize {
        (self.event as usize) | ((self.state as usize) << 4)
    }
}

/// Streaming loss aggregation: no 40M-byte trace or per-position labels in
/// memory after observation. Fits use 256 event/state loss-difference cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadGateEvidence {
    docs: BTreeSet<u32>,
    last: Option<(u32, u32)>,
    pub samples: u64,
    pub p1_microbits: i128,
    pub neural_microbits: i128,
    /// Difference of P1 minus neural cost for rows of each 8-bit signature.
    delta: [i128; DOMAIN],
    /// Number of rows of each signature; useful for activity reporting.
    count: [u64; DOMAIN],
}

impl Default for HeadGateEvidence {
    fn default() -> Self {
        Self {
            docs: BTreeSet::new(),
            last: None,
            samples: 0,
            p1_microbits: 0,
            neural_microbits: 0,
            delta: [0; DOMAIN],
            count: [0; DOMAIN],
        }
    }
}

impl HeadGateEvidence {
    pub fn observe(&mut self, row: HeadGateObservation) -> Result<(), HeadGateError> {
        if row.event > 15 || row.state > 15 {
            return Err(HeadGateError::InputOutOfRange);
        }
        let key = (row.doc_index, row.position);
        if self.last.is_some_and(|old| key <= old) {
            return Err(HeadGateError::NonMonotonicRow);
        }
        if self.samples == 100_000_000 {
            return Err(HeadGateError::TooManySamples);
        }
        let index = row.index();
        self.delta[index] += i128::from(row.p1_microbits) - i128::from(row.neural_microbits);
        self.count[index] += 1;
        self.p1_microbits += i128::from(row.p1_microbits);
        self.neural_microbits += i128::from(row.neural_microbits);
        self.samples += 1;
        self.last = Some(key);
        self.docs.insert(row.doc_index);
        Ok(())
    }

    #[must_use]
    pub fn selected_count(&self, gate: &AnfActivation) -> u64 {
        let mut selected = 0u64;
        for event in 0..16 {
            for state in 0..16 {
                if gate.is_active(event, state) {
                    selected += self.count[event as usize | ((state as usize) << 4)];
                }
            }
        }
        selected
    }

    /// Total applied-byte code length in fixed-point integer microbits.
    #[must_use]
    pub fn routed_microbits(&self, gate: &AnfActivation) -> i128 {
        let mut loss = self.neural_microbits;
        for event in 0..16 {
            for state in 0..16 {
                if !gate.is_active(event, state) {
                    loss += self.delta[event as usize | ((state as usize) << 4)];
                }
            }
        }
        loss
    }
}

/// Non-learned activity-matched control: fixed, predeclared hash of the
/// same eight causal inputs. Only the threshold is matched to TRAIN gate
/// activity; never fit this control on CAL-SEL or on target labels.
#[must_use]
pub fn hash_gate_is_active(record: &BankRecord, threshold: u16) -> bool {
    let (event, state) = causal_features(record);
    hash_signature(event, state) < threshold
}

fn hash_signature(event: u8, state: u8) -> u16 {
    let mut x = u64::from(event) | (u64::from(state) << 4);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    ((x ^ (x >> 31)) & 255) as u16
}

impl HeadGateEvidence {
    #[must_use]
    pub fn hashed_microbits_and_selected(&self, threshold: u16) -> (i128, u64) {
        let mut cost = self.neural_microbits;
        let mut selected = 0u64;
        for event in 0..16 {
            for state in 0..16 {
                let index = event | (state << 4);
                if hash_signature(event as u8, state as u8) < threshold {
                    selected += self.count[index];
                } else {
                    cost += self.delta[index];
                }
            }
        }
        (cost, selected)
    }

    /// Choose hash activity closest to BAF's TRAIN-only selected count.
    #[must_use]
    pub fn match_hash_activity(&self, desired: u64) -> u16 {
        let mut best = 0u16;
        let mut min_delta = u64::MAX;
        for threshold in 0..=256 {
            let selected = self.hashed_microbits_and_selected(threshold).1;
            let delta = selected.abs_diff(desired);
            if delta < min_delta {
                min_delta = delta;
                best = threshold;
            }
        }
        best
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadGatePolicy {
    pub max_rounds: usize,
    pub max_terms: usize,
    pub max_candidate_evaluations: usize,
    pub min_validation_improvement_microbits_per_byte: u64,
}

impl Default for HeadGatePolicy {
    fn default() -> Self {
        Self {
            max_rounds: 6,
            max_terms: 8,
            max_candidate_evaluations: MAX_EVALUATIONS,
            min_validation_improvement_microbits_per_byte:
                DEFAULT_VALIDATION_IMPROVEMENT_MICROBITS_PER_BYTE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadGateFit {
    /// On any failed holdout gate, always-on neural baseline is returned.
    pub gate: AnfActivation,
    pub candidate: AnfActivation,
    pub admitted: bool,
    pub evaluations: usize,
    pub train_baseline_microbits: i128,
    pub train_candidate_microbits: i128,
    pub validation_baseline_microbits: i128,
    pub validation_candidate_microbits: i128,
    pub train_selected: u64,
    pub validation_selected: u64,
}

fn feature_bank() -> Vec<AnfMonomial> {
    let mut terms = Vec::with_capacity(36);
    for first in 0..8 {
        let make = |second: Option<usize>| {
            let bits = (1u16 << first) | second.map_or(0, |index| 1u16 << index);
            AnfMonomial {
                event_mask: u64::from(bits & 15),
                state_mask: u64::from(bits >> 4),
            }
        };
        terms.push(make(None));
    }
    for first in 0..8 {
        for second in (first + 1)..8 {
            let bits = (1u16 << first) | (1u16 << second);
            terms.push(AnfMonomial {
                event_mask: u64::from(bits & 15),
                state_mask: u64::from(bits >> 4),
            });
        }
    }
    terms
}

/// Deterministic train-only ANF search; CAL-SEL is read exactly once after
/// choosing a frozen candidate. No test panel or gate-teacher labels are read.
pub fn fit_head_gate(
    train: &HeadGateEvidence,
    validation: &HeadGateEvidence,
    policy: HeadGatePolicy,
) -> Result<HeadGateFit, HeadGateError> {
    if train.samples == 0 || validation.samples == 0 {
        return Err(HeadGateError::EmptySplit);
    }
    if let Some(overlap) = train.docs.intersection(&validation.docs).next() {
        return Err(HeadGateError::TrainValidationDocOverlap(*overlap));
    }
    if policy.max_rounds == 0 || policy.max_terms == 0 || policy.max_terms > 8 {
        return Err(HeadGateError::InvalidPolicy);
    }
    let basis = feature_bank();
    let worst_case = policy
        .max_rounds
        .checked_mul(basis.len() + 1)
        .ok_or(HeadGateError::CandidateBudget)?;
    if worst_case > policy.max_candidate_evaluations {
        return Err(HeadGateError::CandidateBudget);
    }

    let baseline = AnfActivation {
        bias: true,
        terms: Vec::new(),
    };
    let mut candidate = baseline.clone();
    let mut current_loss = train.routed_microbits(&candidate);
    let train_baseline = current_loss;
    let mut evaluations = 0;
    for _ in 0..policy.max_rounds {
        let mut winner = None;
        let mut best_loss = current_loss;
        for feature in 0..=basis.len() {
            let mut trial = candidate.clone();
            if feature == 0 {
                trial.bias = !trial.bias;
            } else {
                let term = basis[feature - 1];
                if let Some(index) = trial.terms.iter().position(|x| *x == term) {
                    trial.terms.remove(index);
                } else {
                    if trial.terms.len() >= policy.max_terms {
                        continue;
                    }
                    trial.terms.push(term);
                }
            }
            let loss = train.routed_microbits(&trial);
            evaluations += 1;
            if loss < best_loss {
                best_loss = loss;
                winner = Some(trial);
            }
        }
        let Some(next) = winner else { break };
        candidate = next;
        current_loss = best_loss;
    }

    // Holdout is deliberately *not* read until train search is frozen.
    let validation_baseline = validation.routed_microbits(&baseline);
    let validation_candidate = validation.routed_microbits(&candidate);
    let required = i128::from(policy.min_validation_improvement_microbits_per_byte)
        * i128::from(validation.samples);
    let admitted =
        current_loss < train_baseline && validation_candidate <= validation_baseline - required;
    let gate = if admitted {
        candidate.clone()
    } else {
        baseline
    };
    Ok(HeadGateFit {
        train_selected: train.selected_count(&gate),
        validation_selected: validation.selected_count(&gate),
        gate,
        candidate,
        admitted,
        evaluations,
        train_baseline_microbits: train_baseline,
        train_candidate_microbits: current_loss,
        validation_baseline_microbits: validation_baseline,
        validation_candidate_microbits: validation_candidate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyb_h1d_bank_head::replay_bank_records;

    fn make_evidence(doc: u32, harmed_when_both: bool) -> HeadGateEvidence {
        let mut result = HeadGateEvidence::default();
        let mut position = 0;
        for event in 0..16u8 {
            for state in 0..16u8 {
                let harmed = event & 1 != 0 && state & 1 != 0;
                let (p1, neural) = if harmed == harmed_when_both {
                    (2_000_000, 7_000_000)
                } else {
                    (7_000_000, 2_000_000)
                };
                result
                    .observe(HeadGateObservation {
                        doc_index: doc,
                        position,
                        event,
                        state,
                        p1_microbits: p1,
                        neural_microbits: neural,
                    })
                    .unwrap();
                position += 1;
            }
        }
        result
    }

    #[test]
    fn learns_non_linear_decoder_visible_gate_from_only_true_task_loss() {
        let tr = make_evidence(1, true);
        let val = make_evidence(2, true);
        let fit = fit_head_gate(&tr, &val, HeadGatePolicy::default()).unwrap();
        assert!(fit.admitted);
        assert!(fit.train_candidate_microbits < fit.train_baseline_microbits);
        assert!(fit.validation_candidate_microbits < fit.validation_baseline_microbits);
        assert!(!fit.gate.is_active(1, 1));
        assert!(fit.gate.is_active(0, 1));
        assert_eq!(fit.gate.terms.len(), 1);
        assert_eq!(fit.train_selected, 192);
    }

    #[test]
    fn holdout_veto_is_atomic_and_does_not_change_neural_weights() {
        let tr = make_evidence(1, true);
        let val = make_evidence(2, false);
        let fit = fit_head_gate(&tr, &val, HeadGatePolicy::default()).unwrap();
        assert!(!fit.admitted);
        assert!(fit.gate.bias);
        assert!(fit.gate.terms.is_empty());
        assert_eq!(fit.validation_selected, val.samples);
    }

    #[test]
    fn rejects_same_document_even_with_disjoint_positions() {
        let tr = make_evidence(1, true);
        let val = make_evidence(1, false);
        assert_eq!(
            fit_head_gate(&tr, &val, HeadGatePolicy::default()),
            Err(HeadGateError::TrainValidationDocOverlap(1))
        );
    }

    #[test]
    fn null_task_leaves_neural_head_always_on() {
        let mut tr = make_evidence(1, true);
        for item in &mut tr.delta {
            *item = -1;
        }
        let val = make_evidence(2, true);
        let fit = fit_head_gate(&tr, &val, HeadGatePolicy::default()).unwrap();
        assert!(!fit.admitted);
    }

    #[test]
    fn causal_feature_oracles_agree_across_synthetic_records() {
        for row in replay_bank_records(256) {
            assert_eq!(causal_features(&row), oracle_causal_features(&row));
        }
    }

    #[test]
    fn rejects_over_budget_and_out_of_order_evidence() {
        let tr = make_evidence(1, true);
        let val = make_evidence(2, true);
        assert_eq!(
            fit_head_gate(
                &tr,
                &val,
                HeadGatePolicy {
                    max_candidate_evaluations: 1,
                    ..HeadGatePolicy::default()
                }
            ),
            Err(HeadGateError::CandidateBudget)
        );
        let mut e = HeadGateEvidence::default();
        let row = HeadGateObservation {
            doc_index: 9,
            position: 1,
            event: 0,
            state: 0,
            p1_microbits: 1,
            neural_microbits: 1,
        };
        e.observe(row).unwrap();
        assert_eq!(e.observe(row), Err(HeadGateError::NonMonotonicRow));
    }
}
