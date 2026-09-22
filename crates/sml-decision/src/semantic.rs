//! SML-D1b: explicit observed SBG features feeding native typed decisions.
//!
//! M denotes relevance, not observation; A denotes ambiguity, not confidence.
//! Every field therefore has a separate caller-supplied observation mask.
//! Projections are fixed structural metadata, not learned semantic encoders.

use core::{fmt, mem::size_of};

use sml_core::semantic::{SemanticBitField, SemanticBitFieldError, SemanticState};

use crate::{
    training::{fit_boolean_expert, FitError, FitPolicy, FitReport, LabeledDecision},
    BatchBudget, BatchResult, DecisionError, DecisionProgram, InputRecord, TruthTable2,
};

/// Fixed coordinate selectors. No whole-state similarity scan is performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticFeature {
    /// V is usable only if V/M/A are observed, M is set and A is clear.
    ContextValue(usize),
    /// Observed metadata may be used as an explicit applicability input.
    Meaning(usize),
    Ambiguity(usize),
    Routing(usize),
}

impl SemanticFeature {
    fn coordinate(self) -> usize {
        match self {
            Self::ContextValue(i) | Self::Meaning(i) | Self::Ambiguity(i) | Self::Routing(i) => i,
        }
    }

    fn read_bound(self) -> usize {
        match self {
            Self::ContextValue(_) => 6,
            _ => 2,
        }
    }
}

/// Availability assertions supplied by the encoder/source, NOT authenticity proofs.
/// References may share one mask when all fields have identical availability.
#[derive(Debug, Clone, Copy)]
pub struct ObservedFields<'a, const BITS: usize> {
    pub value: &'a SemanticBitField<BITS>,
    pub meaning_mask: &'a SemanticBitField<BITS>,
    pub ambiguity: &'a SemanticBitField<BITS>,
    pub routing: &'a SemanticBitField<BITS>,
}

#[derive(Debug, Clone, Copy)]
pub struct SemanticInput<'a, const BITS: usize> {
    pub record_id: u64,
    pub schema_id: u64,
    pub projection_version: u64,
    pub state: &'a SemanticState<BITS>,
    pub observed: ObservedFields<'a, BITS>,
}

#[derive(Debug, Clone, Copy)]
pub struct LabeledSemanticInput<'a, const BITS: usize> {
    pub input: SemanticInput<'a, BITS>,
    pub target: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionBudget {
    pub max_records: usize,
    /// Internal projected/labeled element payload, not source fields or RSS.
    pub max_buffer_bytes: usize,
    pub max_bit_reads: u64,
}

impl Default for ProjectionBudget {
    fn default() -> Self {
        Self {
            max_records: 4096,
            max_buffer_bytes: 1024 * 1024,
            max_bit_reads: 2_097_152,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionPlan {
    pub records: usize,
    pub buffer_bytes: usize,
    pub feature_queries: u64,
    pub bit_reads_bound: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProjectionCounters {
    pub records: u64,
    pub feature_queries: u64,
    pub observation_mask_reads: u64,
    pub semantic_bit_reads: u64,
    pub known_features: u64,
}

impl ProjectionCounters {
    fn add(&mut self, other: Self) {
        // Each total is bounded by the checked batch projection plan.
        self.records += other.records;
        self.feature_queries += other.feature_queries;
        self.observation_mask_reads += other.observation_mask_reads;
        self.semantic_bit_reads += other.semantic_bit_reads;
        self.known_features += other.known_features;
    }
}

/// Immutable projection, bound to an explicit schema and caller-managed version.
#[derive(Debug, Clone)]
pub struct SemanticProjection<const BITS: usize> {
    schema_id: u64,
    version: u64,
    features: Vec<SemanticFeature>,
    reads_per_record: usize,
}

impl<const BITS: usize> SemanticProjection<BITS> {
    pub fn new(
        schema_id: u64,
        version: u64,
        features: Vec<SemanticFeature>,
    ) -> Result<Self, SemanticDecisionError> {
        if BITS == 0 || version == 0 || !(1..=64).contains(&features.len()) {
            return Err(SemanticDecisionError::InvalidProjection);
        }
        for (position, feature) in features.iter().enumerate() {
            if feature.coordinate() >= BITS {
                return Err(SemanticDecisionError::InvalidCoordinate(
                    feature.coordinate(),
                ));
            }
            if features[..position].contains(feature) {
                return Err(SemanticDecisionError::DuplicateFeature);
            }
        }
        let reads_per_record = features.iter().map(|feature| feature.read_bound()).sum();
        Ok(Self {
            schema_id,
            version,
            features,
            reads_per_record,
        })
    }

    pub fn plan(
        &self,
        records: usize,
        budget: ProjectionBudget,
    ) -> Result<ProjectionPlan, SemanticDecisionError> {
        self.plan_buffer(records, size_of::<InputRecord>(), budget)
    }

    fn plan_buffer(
        &self,
        records: usize,
        element_bytes: usize,
        budget: ProjectionBudget,
    ) -> Result<ProjectionPlan, SemanticDecisionError> {
        let buffer_bytes = records
            .checked_mul(element_bytes)
            .ok_or(SemanticDecisionError::Overflow)?;
        let feature_queries = crate::product(records, self.features.len())?;
        let bit_reads_bound = crate::product(records, self.reads_per_record)?;
        if records > budget.max_records
            || buffer_bytes > budget.max_buffer_bytes
            || bit_reads_bound > budget.max_bit_reads
        {
            return Err(SemanticDecisionError::ProjectionBudget);
        }
        Ok(ProjectionPlan {
            records,
            buffer_bytes,
            feature_queries,
            bit_reads_bound,
        })
    }

    fn validate_input(&self, input: &SemanticInput<'_, BITS>) -> Result<(), SemanticDecisionError> {
        if input.schema_id != self.schema_id || input.projection_version != self.version {
            return Err(SemanticDecisionError::BindingMismatch);
        }
        Ok(())
    }

    /// Reads only declared coordinates; never clones the SBG fields.
    pub fn project(
        &self,
        input: SemanticInput<'_, BITS>,
    ) -> Result<(InputRecord, ProjectionCounters), SemanticDecisionError> {
        self.validate_input(&input)?;
        let mut record = InputRecord {
            record_id: input.record_id,
            schema_id: self.schema_id,
            values: 0,
            known: 0,
        };
        let mut counters = ProjectionCounters {
            records: 1,
            feature_queries: self.features.len() as u64,
            ..ProjectionCounters::default()
        };
        for (slot, feature) in self.features.iter().copied().enumerate() {
            let i = feature.coordinate();
            let value = match feature {
                SemanticFeature::ContextValue(_) => {
                    counters.observation_mask_reads += 3;
                    let v = input.observed.value.get(i)?;
                    let m = input.observed.meaning_mask.get(i)?;
                    let a = input.observed.ambiguity.get(i)?;
                    if v && m && a {
                        counters.semantic_bit_reads += 2;
                        let relevant = input.state.meaning_mask.get(i)?;
                        let ambiguous = input.state.ambiguity.get(i)?;
                        if relevant && !ambiguous {
                            counters.semantic_bit_reads += 1;
                            Some(input.state.value.get(i)?)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => {
                    counters.observation_mask_reads += 1;
                    let (observed, field) = match feature {
                        SemanticFeature::Meaning(_) => {
                            (input.observed.meaning_mask, &input.state.meaning_mask)
                        }
                        SemanticFeature::Ambiguity(_) => {
                            (input.observed.ambiguity, &input.state.ambiguity)
                        }
                        SemanticFeature::Routing(_) => {
                            (input.observed.routing, &input.state.routing)
                        }
                        SemanticFeature::ContextValue(_) => unreachable!(),
                    };
                    if observed.get(i)? {
                        counters.semantic_bit_reads += 1;
                        Some(field.get(i)?)
                    } else {
                        None
                    }
                }
            };
            if let Some(value) = value {
                record.known |= 1_u64 << slot;
                record.values |= u64::from(value) << slot;
                counters.known_features += 1;
            }
        }
        Ok((record, counters))
    }

    /// Fits one finite two-input expert from SBG projections. Coordinates remain
    /// fixed; training selects a table and validation may only reject it.
    pub fn fit_expert(
        &self,
        training: &[LabeledSemanticInput<'_, BITS>],
        validation: &[LabeledSemanticInput<'_, BITS>],
        feature_slots: [usize; 2],
        incumbent: TruthTable2,
        policy: FitPolicy,
        budget: ProjectionBudget,
    ) -> Result<SemanticFitReport, SemanticDecisionError> {
        if feature_slots[0] == feature_slots[1]
            || feature_slots
                .iter()
                .any(|slot| *slot >= self.features.len())
        {
            return Err(SemanticDecisionError::InvalidFeaturePair);
        }
        if training.is_empty() || validation.is_empty() {
            return Err(SemanticDecisionError::Fit(FitError::EmptySplit));
        }
        let total = training
            .len()
            .checked_add(validation.len())
            .ok_or(SemanticDecisionError::Overflow)?;
        if total > policy.max_samples {
            return Err(SemanticDecisionError::Fit(FitError::SampleBudget));
        }
        let plan = self.plan_buffer(total, size_of::<LabeledDecision>(), budget)?;
        for sample in training.iter().chain(validation) {
            self.validate_input(&sample.input)?;
        }
        let mut projected = Vec::new();
        projected
            .try_reserve_exact(total)
            .map_err(|_| SemanticDecisionError::Allocation)?;
        let mut counters = ProjectionCounters::default();
        let required = (1_u64 << feature_slots[0]) | (1_u64 << feature_slots[1]);
        for sample in training.iter().chain(validation) {
            let (record, work) = self.project(sample.input)?;
            if record.known & required != required {
                return Err(SemanticDecisionError::UnknownTrainingFeature(
                    record.record_id,
                ));
            }
            counters.add(work);
            projected.push(LabeledDecision {
                case_id: record.record_id,
                left: record.values & (1_u64 << feature_slots[0]) != 0,
                right: record.values & (1_u64 << feature_slots[1]) != 0,
                target: sample.target,
            });
        }
        let (train, valid) = projected.split_at(training.len());
        let fit = fit_boolean_expert(train, valid, incumbent, policy)?;
        Ok(SemanticFitReport {
            fit,
            projection: counters,
            plan,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticFitReport {
    pub fit: FitReport,
    pub projection: ProjectionCounters,
    pub plan: ProjectionPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticBatchResult {
    pub decisions: BatchResult,
    pub projection_version: u64,
    pub projection: ProjectionCounters,
    pub projection_plan: ProjectionPlan,
    /// Projected input + decision scratch + result element payload, not RSS.
    pub working_payload_bytes_bound: usize,
}

#[derive(Debug, Clone)]
pub struct SemanticDecisionProgram<const BITS: usize> {
    projection: SemanticProjection<BITS>,
    decision: DecisionProgram,
}

impl<const BITS: usize> SemanticDecisionProgram<BITS> {
    pub fn new(
        projection: SemanticProjection<BITS>,
        decision: DecisionProgram,
    ) -> Result<Self, SemanticDecisionError> {
        if projection.schema_id != decision.schema_id
            || projection.features.len() != decision.inputs
        {
            return Err(SemanticDecisionError::BindingMismatch);
        }
        Ok(Self {
            projection,
            decision,
        })
    }

    pub fn evaluate(
        &self,
        inputs: &[SemanticInput<'_, BITS>],
        decision_budget: BatchBudget,
        projection_budget: ProjectionBudget,
    ) -> Result<SemanticBatchResult, SemanticDecisionError> {
        // Both admission plans run before projection buffers or field reads.
        let plan = self.decision.plan(inputs.len(), decision_budget)?;
        let projection_plan = self.projection.plan(inputs.len(), projection_budget)?;
        let working_payload_bytes_bound = projection_plan
            .buffer_bytes
            .checked_add(plan.scratch_bytes)
            .and_then(|bytes| bytes.checked_add(plan.result_bytes))
            .ok_or(SemanticDecisionError::Overflow)?;
        for input in inputs {
            self.projection.validate_input(input)?;
        }
        let mut projected = Vec::new();
        projected
            .try_reserve_exact(inputs.len())
            .map_err(|_| SemanticDecisionError::Allocation)?;
        let mut projection = ProjectionCounters::default();
        for input in inputs {
            let (record, work) = self.projection.project(*input)?;
            projected.push(record);
            projection.add(work);
        }
        let decisions = self.decision.evaluate(&projected, decision_budget)?;
        Ok(SemanticBatchResult {
            decisions,
            projection_version: self.projection.version,
            projection,
            projection_plan,
            working_payload_bytes_bound,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticDecisionError {
    InvalidProjection,
    InvalidCoordinate(usize),
    DuplicateFeature,
    BindingMismatch,
    InvalidFeaturePair,
    UnknownTrainingFeature(u64),
    ProjectionBudget,
    Overflow,
    Allocation,
    Field(SemanticBitFieldError),
    Decision(DecisionError),
    Fit(FitError),
}

impl From<SemanticBitFieldError> for SemanticDecisionError {
    fn from(error: SemanticBitFieldError) -> Self {
        Self::Field(error)
    }
}

impl From<DecisionError> for SemanticDecisionError {
    fn from(error: DecisionError) -> Self {
        Self::Decision(error)
    }
}

impl From<FitError> for SemanticDecisionError {
    fn from(error: FitError) -> Self {
        Self::Fit(error)
    }
}

impl fmt::Display for SemanticDecisionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticDecisionError {}
