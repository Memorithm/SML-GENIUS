//! Discrete Boolean evidence learning for Semantic Boolean Geometry.
//!
//! SBG reuses the native Boolean-integral accumulator for credit assignment and
//! the native train/holdout commit gate for mutation acceptance. Evidence is
//! indexed over the four semantic fields V/M/A/R. Candidate mutations remain
//! immutable until an independent train/holdout evaluation authorizes an atomic
//! semantic-state replacement.

use core::fmt;

use crate::{
    boolean_hierarchical::{
        BooleanCommitDecision, BooleanCommitEvidence, BooleanCommitPolicy, HierarchicalBooleanError,
    },
    boolean_integral::{
        BooleanCreditCell, BooleanIntegralAccumulator, BooleanIntegralDecision,
        BooleanIntegralError, BooleanIntegralPolicy, BooleanIntervention,
        BOOLEAN_INTEGRAL_SCALE_PPM,
    },
};

use super::{SemanticBitField, SemanticBitFieldError, SemanticState};

const SEMANTIC_FIELD_COUNT: usize = 4;

/// Addressable field in the native SBG state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticLearningField {
    Value,
    MeaningMask,
    Ambiguity,
    Routing,
}

impl SemanticLearningField {
    #[must_use]
    const fn ordinal(self) -> usize {
        match self {
            Self::Value => 0,
            Self::MeaningMask => 1,
            Self::Ambiguity => 2,
            Self::Routing => 3,
        }
    }

    fn from_ordinal(value: usize) -> Option<Self> {
        match value {
            0 => Some(Self::Value),
            1 => Some(Self::MeaningMask),
            2 => Some(Self::Ambiguity),
            3 => Some(Self::Routing),
            _ => None,
        }
    }
}

/// One exact Boolean coordinate in V, M, A or R.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemanticCoordinate {
    pub field: SemanticLearningField,
    pub index: usize,
}

/// Human-readable semantic interpretation of an underlying Boolean flip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticCoordinateAction {
    SetValue,
    ClearValue,
    Mask,
    Unmask,
    MarkAmbiguous,
    ResolveAmbiguity,
    RouteOn,
    RouteOff,
}

/// One measured XOR intervention over semantic coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEvidenceObservation {
    pub coordinates: Vec<SemanticCoordinate>,
    pub baseline_loss_ppm: u32,
    pub perturbed_loss_ppm: u32,
}

/// Candidate emitted only after integrated Boolean evidence recommends a flip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticLearningCandidate {
    pub coordinate: SemanticCoordinate,
    pub from_value: bool,
    pub to_value: bool,
    pub action: SemanticCoordinateAction,
    pub observations: u32,
    pub confidence_ppm: i32,
}

/// Persistent discrete evidence ledger for the four SBG fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEvidenceLedger<const BITS: usize> {
    accumulator: BooleanIntegralAccumulator,
    policy: BooleanIntegralPolicy,
}

impl<const BITS: usize> SemanticEvidenceLedger<BITS> {
    pub fn new(policy: BooleanIntegralPolicy) -> Result<Self, SemanticEvidenceError<()>> {
        if BITS == 0 {
            return Err(SemanticEvidenceError::ZeroSemanticWidth);
        }
        let parameter_count = BITS
            .checked_mul(SEMANTIC_FIELD_COUNT)
            .ok_or(SemanticEvidenceError::ArithmeticOverflow)?;
        let policy = policy.validate().map_err(SemanticEvidenceError::Integral)?;
        let accumulator = BooleanIntegralAccumulator::new(parameter_count, policy)
            .map_err(SemanticEvidenceError::Integral)?;
        Ok(Self {
            accumulator,
            policy,
        })
    }

    #[must_use]
    pub const fn policy(&self) -> BooleanIntegralPolicy {
        self.policy
    }

    #[must_use]
    pub fn interventions(&self) -> u64 {
        self.accumulator.interventions()
    }

    pub fn observe(
        &mut self,
        observation: &SemanticEvidenceObservation,
    ) -> Result<(), SemanticEvidenceError<()>> {
        if observation.coordinates.is_empty() {
            return Err(SemanticEvidenceError::EmptyCoordinates);
        }

        let mut mask = Vec::with_capacity(observation.coordinates.len());
        for coordinate in &observation.coordinates {
            let flat = flat_index::<BITS>(*coordinate)?;
            mask.push(
                u32::try_from(flat)
                    .map_err(|_| SemanticEvidenceError::FlatIndexOutOfRange(flat))?,
            );
        }

        self.accumulator
            .observe(&BooleanIntervention {
                mask,
                baseline_loss_ppm: observation.baseline_loss_ppm,
                perturbed_loss_ppm: observation.perturbed_loss_ppm,
            })
            .map_err(SemanticEvidenceError::Integral)
    }

    pub fn cell(
        &self,
        coordinate: SemanticCoordinate,
    ) -> Result<BooleanCreditCell, SemanticEvidenceError<()>> {
        let flat = flat_index::<BITS>(coordinate)?;
        self.accumulator
            .cell(flat)
            .map_err(SemanticEvidenceError::Integral)
    }

    pub fn decision(
        &self,
        coordinate: SemanticCoordinate,
    ) -> Result<BooleanIntegralDecision, SemanticEvidenceError<()>> {
        Ok(self.cell(coordinate)?.decision(self.policy))
    }

    /// Returns the strongest integrated flip recommendations for the current
    /// semantic state. The underlying evidence decision remains XOR/Flip; the
    /// action label expresses what that flip means for V/M/A/R.
    pub fn recommended_candidates(
        &self,
        state: &SemanticState<BITS>,
        max_candidates: usize,
    ) -> Result<Vec<SemanticLearningCandidate>, SemanticEvidenceError<()>> {
        if max_candidates == 0 {
            return Err(SemanticEvidenceError::ZeroCandidateBudget);
        }

        let mut candidates = Vec::new();
        for flat in self.accumulator.recommended_flips() {
            let coordinate = coordinate_from_flat::<BITS>(flat)?;
            let from_value = read_coordinate(state, coordinate)?;
            let to_value = !from_value;
            let cell = self
                .accumulator
                .cell(flat)
                .map_err(SemanticEvidenceError::Integral)?;
            candidates.push(SemanticLearningCandidate {
                coordinate,
                from_value,
                to_value,
                action: semantic_action(coordinate.field, from_value),
                observations: cell.observations,
                confidence_ppm: cell.signed_confidence_ppm(),
            });
        }

        candidates.sort_by(|left, right| {
            right
                .confidence_ppm
                .cmp(&left.confidence_ppm)
                .then_with(|| right.observations.cmp(&left.observations))
                .then_with(|| left.coordinate.cmp(&right.coordinate))
        });
        candidates.truncate(max_candidates);
        Ok(candidates)
    }
}

/// Terminal result of an SBG evidence-learning commit attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticEvidenceLearningOutcome {
    NoCandidate,
    CommitGate(BooleanCommitDecision),
}

/// Exact evidence emitted by one atomic semantic mutation attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticEvidenceLearningReport {
    pub train_baseline_loss_ppm: u32,
    pub holdout_baseline_loss_ppm: u32,
    pub train_candidate_loss_ppm: Option<u32>,
    pub holdout_candidate_loss_ppm: Option<u32>,
    pub proposed: Vec<SemanticLearningCandidate>,
    pub committed_mutations: u64,
    pub outcome: SemanticEvidenceLearningOutcome,
}

/// Applies candidate flips only after the native train/holdout commit gate.
///
/// Candidate evaluation occurs on an immutable clone. The caller's semantic
/// state changes only when the gate returns `Commit`.
pub fn commit_semantic_candidates<const BITS: usize, E, TrainObjective, HoldoutObjective>(
    state: &mut SemanticState<BITS>,
    candidates: &[SemanticLearningCandidate],
    commit_policy: BooleanCommitPolicy,
    mut train_objective: TrainObjective,
    mut holdout_objective: HoldoutObjective,
) -> Result<SemanticEvidenceLearningReport, SemanticEvidenceError<E>>
where
    TrainObjective: FnMut(&SemanticState<BITS>) -> Result<u32, E>,
    HoldoutObjective: FnMut(&SemanticState<BITS>) -> Result<u32, E>,
{
    let train_baseline_loss_ppm = evaluate_objective(state, &mut train_objective)?;
    let holdout_baseline_loss_ppm = evaluate_objective(state, &mut holdout_objective)?;

    if candidates.is_empty() {
        return Ok(SemanticEvidenceLearningReport {
            train_baseline_loss_ppm,
            holdout_baseline_loss_ppm,
            train_candidate_loss_ppm: None,
            holdout_candidate_loss_ppm: None,
            proposed: Vec::new(),
            committed_mutations: 0,
            outcome: SemanticEvidenceLearningOutcome::NoCandidate,
        });
    }

    validate_candidates(state, candidates)?;
    let mut candidate_state = state.clone();
    for candidate in candidates {
        write_coordinate(
            &mut candidate_state,
            candidate.coordinate,
            candidate.to_value,
        )?;
    }

    let train_candidate_loss_ppm = evaluate_objective(&candidate_state, &mut train_objective)?;
    let holdout_candidate_loss_ppm = evaluate_objective(&candidate_state, &mut holdout_objective)?;

    let decision = BooleanCommitEvidence {
        train_baseline_loss_ppm,
        train_candidate_loss_ppm,
        holdout_baseline_loss_ppm,
        holdout_candidate_loss_ppm,
    }
    .decide(commit_policy)
    .map_err(SemanticEvidenceError::Commit)?;

    let committed_mutations = if decision == BooleanCommitDecision::Commit {
        *state = candidate_state;
        candidates.len() as u64
    } else {
        0
    };

    Ok(SemanticEvidenceLearningReport {
        train_baseline_loss_ppm,
        holdout_baseline_loss_ppm,
        train_candidate_loss_ppm: Some(train_candidate_loss_ppm),
        holdout_candidate_loss_ppm: Some(holdout_candidate_loss_ppm),
        proposed: candidates.to_vec(),
        committed_mutations,
        outcome: SemanticEvidenceLearningOutcome::CommitGate(decision),
    })
}

fn validate_candidates<const BITS: usize, E>(
    state: &SemanticState<BITS>,
    candidates: &[SemanticLearningCandidate],
) -> Result<(), SemanticEvidenceError<E>> {
    let mut seen = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        flat_index::<BITS>(candidate.coordinate).map_err(convert_unit_error)?;
        if seen.contains(&candidate.coordinate) {
            return Err(SemanticEvidenceError::DuplicateCandidate(
                candidate.coordinate,
            ));
        }
        seen.push(candidate.coordinate);

        let current = read_coordinate(state, candidate.coordinate).map_err(convert_unit_error)?;
        if current != candidate.from_value {
            return Err(SemanticEvidenceError::StaleCandidate {
                coordinate: candidate.coordinate,
                expected: candidate.from_value,
                actual: current,
            });
        }
        if candidate.to_value == candidate.from_value
            || candidate.action != semantic_action(candidate.coordinate.field, candidate.from_value)
        {
            return Err(SemanticEvidenceError::InvalidCandidateTransition(
                candidate.coordinate,
            ));
        }
    }
    Ok(())
}

fn evaluate_objective<const BITS: usize, E, Objective>(
    state: &SemanticState<BITS>,
    objective: &mut Objective,
) -> Result<u32, SemanticEvidenceError<E>>
where
    Objective: FnMut(&SemanticState<BITS>) -> Result<u32, E>,
{
    let loss = objective(state).map_err(SemanticEvidenceError::Objective)?;
    if loss > BOOLEAN_INTEGRAL_SCALE_PPM {
        return Err(SemanticEvidenceError::LossOutOfRange(loss));
    }
    Ok(loss)
}

fn flat_index<const BITS: usize>(
    coordinate: SemanticCoordinate,
) -> Result<usize, SemanticEvidenceError<()>> {
    if BITS == 0 {
        return Err(SemanticEvidenceError::ZeroSemanticWidth);
    }
    if coordinate.index >= BITS {
        return Err(SemanticEvidenceError::CoordinateOutOfRange {
            coordinate,
            width: BITS,
        });
    }
    coordinate
        .field
        .ordinal()
        .checked_mul(BITS)
        .and_then(|base| base.checked_add(coordinate.index))
        .ok_or(SemanticEvidenceError::ArithmeticOverflow)
}

fn coordinate_from_flat<const BITS: usize>(
    flat: usize,
) -> Result<SemanticCoordinate, SemanticEvidenceError<()>> {
    if BITS == 0 {
        return Err(SemanticEvidenceError::ZeroSemanticWidth);
    }
    let field_ordinal = flat / BITS;
    let index = flat % BITS;
    let field = SemanticLearningField::from_ordinal(field_ordinal)
        .ok_or(SemanticEvidenceError::FlatIndexOutOfRange(flat))?;
    Ok(SemanticCoordinate { field, index })
}

fn semantic_action(field: SemanticLearningField, from_value: bool) -> SemanticCoordinateAction {
    match (field, from_value) {
        (SemanticLearningField::Value, false) => SemanticCoordinateAction::SetValue,
        (SemanticLearningField::Value, true) => SemanticCoordinateAction::ClearValue,
        (SemanticLearningField::MeaningMask, false) => SemanticCoordinateAction::Unmask,
        (SemanticLearningField::MeaningMask, true) => SemanticCoordinateAction::Mask,
        (SemanticLearningField::Ambiguity, false) => SemanticCoordinateAction::MarkAmbiguous,
        (SemanticLearningField::Ambiguity, true) => SemanticCoordinateAction::ResolveAmbiguity,
        (SemanticLearningField::Routing, false) => SemanticCoordinateAction::RouteOn,
        (SemanticLearningField::Routing, true) => SemanticCoordinateAction::RouteOff,
    }
}

fn field_ref<const BITS: usize>(
    state: &SemanticState<BITS>,
    field: SemanticLearningField,
) -> &SemanticBitField<BITS> {
    match field {
        SemanticLearningField::Value => &state.value,
        SemanticLearningField::MeaningMask => &state.meaning_mask,
        SemanticLearningField::Ambiguity => &state.ambiguity,
        SemanticLearningField::Routing => &state.routing,
    }
}

fn field_mut<const BITS: usize>(
    state: &mut SemanticState<BITS>,
    field: SemanticLearningField,
) -> &mut SemanticBitField<BITS> {
    match field {
        SemanticLearningField::Value => &mut state.value,
        SemanticLearningField::MeaningMask => &mut state.meaning_mask,
        SemanticLearningField::Ambiguity => &mut state.ambiguity,
        SemanticLearningField::Routing => &mut state.routing,
    }
}

fn read_coordinate<const BITS: usize>(
    state: &SemanticState<BITS>,
    coordinate: SemanticCoordinate,
) -> Result<bool, SemanticEvidenceError<()>> {
    field_ref(state, coordinate.field)
        .get(coordinate.index)
        .map_err(SemanticEvidenceError::BitField)
}

fn write_coordinate<const BITS: usize, E>(
    state: &mut SemanticState<BITS>,
    coordinate: SemanticCoordinate,
    value: bool,
) -> Result<(), SemanticEvidenceError<E>> {
    field_mut(state, coordinate.field)
        .set(coordinate.index, value)
        .map_err(SemanticEvidenceError::BitField)
}

fn convert_unit_error<E>(error: SemanticEvidenceError<()>) -> SemanticEvidenceError<E> {
    match error {
        SemanticEvidenceError::ZeroSemanticWidth => SemanticEvidenceError::ZeroSemanticWidth,
        SemanticEvidenceError::ArithmeticOverflow => SemanticEvidenceError::ArithmeticOverflow,
        SemanticEvidenceError::FlatIndexOutOfRange(index) => {
            SemanticEvidenceError::FlatIndexOutOfRange(index)
        }
        SemanticEvidenceError::CoordinateOutOfRange { coordinate, width } => {
            SemanticEvidenceError::CoordinateOutOfRange { coordinate, width }
        }
        SemanticEvidenceError::EmptyCoordinates => SemanticEvidenceError::EmptyCoordinates,
        SemanticEvidenceError::ZeroCandidateBudget => SemanticEvidenceError::ZeroCandidateBudget,
        SemanticEvidenceError::DuplicateCandidate(coordinate) => {
            SemanticEvidenceError::DuplicateCandidate(coordinate)
        }
        SemanticEvidenceError::StaleCandidate {
            coordinate,
            expected,
            actual,
        } => SemanticEvidenceError::StaleCandidate {
            coordinate,
            expected,
            actual,
        },
        SemanticEvidenceError::InvalidCandidateTransition(coordinate) => {
            SemanticEvidenceError::InvalidCandidateTransition(coordinate)
        }
        SemanticEvidenceError::LossOutOfRange(value) => {
            SemanticEvidenceError::LossOutOfRange(value)
        }
        SemanticEvidenceError::Integral(error) => SemanticEvidenceError::Integral(error),
        SemanticEvidenceError::Commit(error) => SemanticEvidenceError::Commit(error),
        SemanticEvidenceError::BitField(error) => SemanticEvidenceError::BitField(error),
        SemanticEvidenceError::Objective(()) => {
            unreachable!("unit objective error is never converted")
        }
    }
}

/// Invalid semantic evidence, coordinate, candidate or objective state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticEvidenceError<E> {
    ZeroSemanticWidth,
    ArithmeticOverflow,
    FlatIndexOutOfRange(usize),
    CoordinateOutOfRange {
        coordinate: SemanticCoordinate,
        width: usize,
    },
    EmptyCoordinates,
    ZeroCandidateBudget,
    DuplicateCandidate(SemanticCoordinate),
    StaleCandidate {
        coordinate: SemanticCoordinate,
        expected: bool,
        actual: bool,
    },
    InvalidCandidateTransition(SemanticCoordinate),
    LossOutOfRange(u32),
    Integral(BooleanIntegralError),
    Commit(HierarchicalBooleanError),
    BitField(SemanticBitFieldError),
    Objective(E),
}

impl<E: fmt::Debug> fmt::Display for SemanticEvidenceError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<E: fmt::Debug> std::error::Error for SemanticEvidenceError<E> {}

#[cfg(test)]
mod tests {
    use super::{
        commit_semantic_candidates, SemanticCoordinate, SemanticCoordinateAction,
        SemanticEvidenceError, SemanticEvidenceLearningOutcome, SemanticEvidenceLedger,
        SemanticEvidenceObservation, SemanticLearningField,
    };
    use crate::{
        boolean_hierarchical::{BooleanCommitDecision, BooleanCommitPolicy},
        boolean_integral::BooleanIntegralPolicy,
        semantic::SemanticState,
    };

    fn policy(min_observations: u32) -> BooleanIntegralPolicy {
        BooleanIntegralPolicy {
            min_observations,
            min_confidence_ppm: 500_000,
            max_mask_width: 8,
        }
    }

    #[test]
    fn integrated_evidence_maps_flip_to_native_field_action() {
        let mut ledger = SemanticEvidenceLedger::<8>::new(policy(1)).expect("ledger");
        let coordinates = [
            SemanticCoordinate {
                field: SemanticLearningField::Value,
                index: 0,
            },
            SemanticCoordinate {
                field: SemanticLearningField::MeaningMask,
                index: 1,
            },
            SemanticCoordinate {
                field: SemanticLearningField::Ambiguity,
                index: 2,
            },
            SemanticCoordinate {
                field: SemanticLearningField::Routing,
                index: 3,
            },
        ];

        for coordinate in coordinates {
            ledger
                .observe(&SemanticEvidenceObservation {
                    coordinates: vec![coordinate],
                    baseline_loss_ppm: 800_000,
                    perturbed_loss_ppm: 200_000,
                })
                .expect("observation");
        }

        let mut state = SemanticState::<8>::zeroed().expect("state");
        state.meaning_mask.set(1, true).expect("mask bit");
        let candidates = ledger
            .recommended_candidates(&state, 8)
            .expect("candidates");

        assert_eq!(candidates.len(), 4);
        assert!(candidates.iter().any(|candidate| {
            candidate.coordinate == coordinates[0]
                && candidate.action == SemanticCoordinateAction::SetValue
        }));
        assert!(candidates.iter().any(|candidate| {
            candidate.coordinate == coordinates[1]
                && candidate.action == SemanticCoordinateAction::Mask
        }));
        assert!(candidates.iter().any(|candidate| {
            candidate.coordinate == coordinates[2]
                && candidate.action == SemanticCoordinateAction::MarkAmbiguous
        }));
        assert!(candidates.iter().any(|candidate| {
            candidate.coordinate == coordinates[3]
                && candidate.action == SemanticCoordinateAction::RouteOn
        }));
    }

    #[test]
    fn evidence_respects_minimum_observation_count() {
        let coordinate = SemanticCoordinate {
            field: SemanticLearningField::Value,
            index: 0,
        };
        let mut ledger = SemanticEvidenceLedger::<8>::new(policy(2)).expect("ledger");
        let observation = SemanticEvidenceObservation {
            coordinates: vec![coordinate],
            baseline_loss_ppm: 900_000,
            perturbed_loss_ppm: 100_000,
        };

        ledger.observe(&observation).expect("first");
        assert!(ledger
            .recommended_candidates(&SemanticState::zeroed().expect("state"), 1)
            .expect("candidates")
            .is_empty());

        ledger.observe(&observation).expect("second");
        assert_eq!(
            ledger.decision(coordinate).expect("decision"),
            crate::boolean_integral::BooleanIntegralDecision::Flip
        );
    }

    #[test]
    fn train_and_holdout_gate_commits_atomically() {
        let coordinate = SemanticCoordinate {
            field: SemanticLearningField::Value,
            index: 0,
        };
        let mut ledger = SemanticEvidenceLedger::<8>::new(policy(1)).expect("ledger");
        ledger
            .observe(&SemanticEvidenceObservation {
                coordinates: vec![coordinate],
                baseline_loss_ppm: 1_000_000,
                perturbed_loss_ppm: 0,
            })
            .expect("observation");

        let mut state = SemanticState::<8>::zeroed().expect("state");
        let candidates = ledger
            .recommended_candidates(&state, 1)
            .expect("candidates");
        let report = commit_semantic_candidates(
            &mut state,
            &candidates,
            BooleanCommitPolicy::default(),
            |candidate| {
                Ok::<u32, ()>(if candidate.value.get(0).expect("bit") {
                    0
                } else {
                    1_000_000
                })
            },
            |candidate| {
                Ok::<u32, ()>(if candidate.value.get(0).expect("bit") {
                    0
                } else {
                    1_000_000
                })
            },
        )
        .expect("commit");

        assert_eq!(
            report.outcome,
            SemanticEvidenceLearningOutcome::CommitGate(BooleanCommitDecision::Commit)
        );
        assert_eq!(report.committed_mutations, 1);
        assert!(state.value.get(0).expect("bit"));
    }

    #[test]
    fn holdout_rejection_leaves_state_unchanged() {
        let coordinate = SemanticCoordinate {
            field: SemanticLearningField::Routing,
            index: 2,
        };
        let mut ledger = SemanticEvidenceLedger::<8>::new(policy(1)).expect("ledger");
        ledger
            .observe(&SemanticEvidenceObservation {
                coordinates: vec![coordinate],
                baseline_loss_ppm: 800_000,
                perturbed_loss_ppm: 100_000,
            })
            .expect("observation");

        let mut state = SemanticState::<8>::zeroed().expect("state");
        let before = state.clone();
        let candidates = ledger
            .recommended_candidates(&state, 1)
            .expect("candidates");
        let report = commit_semantic_candidates(
            &mut state,
            &candidates,
            BooleanCommitPolicy::default(),
            |candidate| {
                Ok::<u32, ()>(if candidate.routing.get(2).expect("bit") {
                    100_000
                } else {
                    800_000
                })
            },
            |candidate| {
                Ok::<u32, ()>(if candidate.routing.get(2).expect("bit") {
                    900_000
                } else {
                    200_000
                })
            },
        )
        .expect("gate");

        assert_eq!(
            report.outcome,
            SemanticEvidenceLearningOutcome::CommitGate(BooleanCommitDecision::RejectHoldout)
        );
        assert_eq!(report.committed_mutations, 0);
        assert_eq!(state, before);
    }

    #[test]
    fn stale_candidate_fails_closed() {
        let coordinate = SemanticCoordinate {
            field: SemanticLearningField::Value,
            index: 0,
        };
        let mut ledger = SemanticEvidenceLedger::<8>::new(policy(1)).expect("ledger");
        ledger
            .observe(&SemanticEvidenceObservation {
                coordinates: vec![coordinate],
                baseline_loss_ppm: 900_000,
                perturbed_loss_ppm: 100_000,
            })
            .expect("observation");

        let state = SemanticState::<8>::zeroed().expect("state");
        let candidates = ledger
            .recommended_candidates(&state, 1)
            .expect("candidates");
        let mut changed = state;
        changed.value.set(0, true).expect("change");

        let result = commit_semantic_candidates(
            &mut changed,
            &candidates,
            BooleanCommitPolicy::default(),
            |_| Ok::<u32, ()>(0),
            |_| Ok::<u32, ()>(0),
        );

        assert_eq!(
            result,
            Err(SemanticEvidenceError::StaleCandidate {
                coordinate,
                expected: false,
                actual: true,
            })
        );
    }
}
