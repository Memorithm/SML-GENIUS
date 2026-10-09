//! DKV-2b: bounded Delta-KV memory reads inside native SBG recurrence.
//!
//! The recurrent kernel consumes a frozen DKV-2a semantic key projection. It
//! does not own or replace direct associative memory, raw provenance stores, or
//! MOR. A later experiment may reproject keys between cycles; this slice keeps
//! the query key frozen so memory effects can be isolated.

use core::fmt;
use std::{collections::BTreeSet, sync::Arc};

use crate::{
    delta_kv::{DeltaKvCounters, DeltaKvError, DeltaKvMemory, DELTA_KV_ONE},
    delta_kv_semantic::DeltaKvSemanticKeyEvidence,
    semantic::{
        run_semantic_recurrent, SemanticBitFieldError, SemanticHaltReason,
        SemanticRecurrentOutcome, SemanticRecurrentRunError, SemanticRewriteStep, SemanticRewriter,
        SemanticState,
    },
    HaltingPolicy,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaKvRecurrentPolicy {
    pub ambiguity_threshold_q12: i32,
}

impl Default for DeltaKvRecurrentPolicy {
    fn default() -> Self {
        Self {
            ambiguity_threshold_q12: 128,
        }
    }
}

impl DeltaKvRecurrentPolicy {
    pub fn validate(self) -> Result<Self, DeltaKvRecurrentError> {
        if self.ambiguity_threshold_q12 < 0 {
            return Err(DeltaKvRecurrentError::InvalidPolicy);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvRecurrentCycleEvidence {
    pub depth: u32,
    pub read_values_q12: Vec<i32>,
    pub ambiguous_values: usize,
    /// Maximum absolute difference between the memory proposal and the current
    /// Boolean value encoded at ±Q12 one. This is compute evidence, not confidence.
    pub correction_abs_max_q12: u32,
    pub read_macs_delta: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvRecurrentKernel<const BITS: usize> {
    memory: DeltaKvMemory,
    key: DeltaKvSemanticKeyEvidence,
    value_coordinates: Vec<usize>,
    policy: DeltaKvRecurrentPolicy,
    counters: DeltaKvCounters,
    cycles: Arc<Vec<DeltaKvRecurrentCycleEvidence>>,
}

impl<const BITS: usize> DeltaKvRecurrentKernel<BITS> {
    pub fn new(
        memory: DeltaKvMemory,
        key: DeltaKvSemanticKeyEvidence,
        value_coordinates: Vec<usize>,
        policy: DeltaKvRecurrentPolicy,
    ) -> Result<Self, DeltaKvRecurrentError> {
        let policy = policy.validate()?;
        if key.key_q12.len() != memory.key_dim() {
            return Err(DeltaKvRecurrentError::KeyDimensionMismatch {
                expected: memory.key_dim(),
                actual: key.key_q12.len(),
            });
        }
        if value_coordinates.len() != memory.value_dim() {
            return Err(DeltaKvRecurrentError::ValueDimensionMismatch {
                expected: memory.value_dim(),
                actual: value_coordinates.len(),
            });
        }
        let unique = value_coordinates.iter().copied().collect::<BTreeSet<_>>();
        if unique.len() != value_coordinates.len() {
            return Err(DeltaKvRecurrentError::DuplicateValueCoordinate);
        }
        if value_coordinates
            .iter()
            .any(|coordinate| *coordinate >= BITS)
        {
            return Err(DeltaKvRecurrentError::ValueCoordinateOutOfRange);
        }

        Ok(Self {
            memory,
            key,
            value_coordinates,
            policy,
            counters: DeltaKvCounters::default(),
            cycles: Arc::new(Vec::new()),
        })
    }

    pub fn seed_target(
        &mut self,
        target_q12: &[i16],
        learning_rate_q12: u16,
    ) -> Result<(), DeltaKvRecurrentError> {
        self.memory.update(
            &self.key.key_q12,
            target_q12,
            learning_rate_q12,
            &mut self.counters,
        )?;
        Ok(())
    }

    #[must_use]
    pub fn key_evidence(&self) -> &DeltaKvSemanticKeyEvidence {
        &self.key
    }

    #[must_use]
    pub const fn counters(&self) -> DeltaKvCounters {
        self.counters
    }

    #[must_use]
    pub fn cycle_evidence(&self) -> &[DeltaKvRecurrentCycleEvidence] {
        &self.cycles
    }
}

impl<const BITS: usize> SemanticRewriter<BITS> for DeltaKvRecurrentKernel<BITS> {
    type Error = DeltaKvRecurrentError;

    fn rewrite(
        &mut self,
        state: &SemanticState<BITS>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<BITS>, Self::Error> {
        let before_macs = self.counters.read_macs;
        let values = self
            .memory
            .read(&self.key.key_q12, &mut self.counters)
            .map_err(DeltaKvRecurrentError::DeltaKv)?;
        if values.len() != self.value_coordinates.len() {
            return Err(DeltaKvRecurrentError::InternalInvariant);
        }

        let mut next = state.clone();
        let mut ambiguous_values = 0_usize;
        let mut correction_abs_max_q12 = 0_u32;
        for (value, coordinate) in values.iter().zip(&self.value_coordinates) {
            let current_q12 = if state.value.get(*coordinate)? {
                i32::from(DELTA_KV_ONE)
            } else {
                -i32::from(DELTA_KV_ONE)
            };
            correction_abs_max_q12 =
                correction_abs_max_q12.max(value.saturating_sub(current_q12).unsigned_abs());
            if value.unsigned_abs() <= self.policy.ambiguity_threshold_q12 as u32 {
                next.ambiguity.set(*coordinate, true)?;
                ambiguous_values = ambiguous_values.saturating_add(1);
            } else {
                next.value.set(*coordinate, *value > 0)?;
                next.ambiguity.set(*coordinate, false)?;
            }
        }

        Arc::make_mut(&mut self.cycles).push(DeltaKvRecurrentCycleEvidence {
            depth,
            read_values_q12: values,
            ambiguous_values,
            correction_abs_max_q12,
            read_macs_delta: self.counters.read_macs.saturating_sub(before_macs),
        });

        Ok(SemanticRewriteStep {
            state: next,
            stability_blocked: ambiguous_values != 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvRecurrentComparison<const BITS: usize> {
    pub adaptive: SemanticRecurrentOutcome<BITS>,
    pub fixed_depth: SemanticRecurrentOutcome<BITS>,
    pub no_delta: SemanticRecurrentOutcome<BITS>,
    pub adaptive_cycles: Vec<DeltaKvRecurrentCycleEvidence>,
    pub fixed_cycles: Vec<DeltaKvRecurrentCycleEvidence>,
}

pub fn compare_delta_kv_recurrent<const BITS: usize>(
    initial: SemanticState<BITS>,
    kernel: DeltaKvRecurrentKernel<BITS>,
    adaptive_policy: HaltingPolicy,
    fixed_depth: u32,
) -> Result<DeltaKvRecurrentComparison<BITS>, DeltaKvRecurrentComparisonError> {
    if fixed_depth == 0 {
        return Err(DeltaKvRecurrentComparisonError::InvalidFixedDepth);
    }

    let mut adaptive_kernel = kernel.clone();
    let adaptive = run_semantic_recurrent(initial.clone(), &mut adaptive_kernel, adaptive_policy)
        .map_err(DeltaKvRecurrentComparisonError::Adaptive)?;

    let mut fixed_kernel = kernel;
    let fixed = run_semantic_recurrent(
        initial.clone(),
        &mut fixed_kernel,
        HaltingPolicy {
            min_depth: fixed_depth,
            max_depth: fixed_depth,
            stable_hamming_threshold: adaptive_policy.stable_hamming_threshold,
        },
    )
    .map_err(DeltaKvRecurrentComparisonError::Fixed)?;

    let mut identity = IdentityRewriter;
    let no_delta = run_semantic_recurrent(initial, &mut identity, adaptive_policy)
        .map_err(DeltaKvRecurrentComparisonError::NoDelta)?;

    Ok(DeltaKvRecurrentComparison {
        adaptive,
        fixed_depth: fixed,
        no_delta,
        adaptive_cycles: Arc::unwrap_or_clone(adaptive_kernel.cycles),
        fixed_cycles: Arc::unwrap_or_clone(fixed_kernel.cycles),
    })
}

#[derive(Debug, Clone, Copy)]
struct IdentityRewriter;

impl<const BITS: usize> SemanticRewriter<BITS> for IdentityRewriter {
    type Error = ();

    fn rewrite(
        &mut self,
        state: &SemanticState<BITS>,
        _depth: u32,
    ) -> Result<SemanticRewriteStep<BITS>, Self::Error> {
        Ok(SemanticRewriteStep {
            state: state.clone(),
            stability_blocked: false,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaKvRecurrentError {
    InvalidPolicy,
    KeyDimensionMismatch { expected: usize, actual: usize },
    ValueDimensionMismatch { expected: usize, actual: usize },
    DuplicateValueCoordinate,
    ValueCoordinateOutOfRange,
    InternalInvariant,
    DeltaKv(DeltaKvError),
    BitField(SemanticBitFieldError),
}

impl From<DeltaKvError> for DeltaKvRecurrentError {
    fn from(error: DeltaKvError) -> Self {
        Self::DeltaKv(error)
    }
}

impl From<SemanticBitFieldError> for DeltaKvRecurrentError {
    fn from(error: SemanticBitFieldError) -> Self {
        Self::BitField(error)
    }
}

impl fmt::Display for DeltaKvRecurrentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DeltaKvRecurrentError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaKvRecurrentComparisonError {
    InvalidFixedDepth,
    Adaptive(SemanticRecurrentRunError<DeltaKvRecurrentError>),
    Fixed(SemanticRecurrentRunError<DeltaKvRecurrentError>),
    NoDelta(SemanticRecurrentRunError<()>),
}

impl fmt::Display for DeltaKvRecurrentComparisonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DeltaKvRecurrentComparisonError {}

#[must_use]
pub const fn stable_halt(reason: SemanticHaltReason) -> bool {
    matches!(reason, SemanticHaltReason::Stable)
}
