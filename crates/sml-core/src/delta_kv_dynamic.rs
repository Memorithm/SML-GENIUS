//! DKV-2c: dynamic Delta-KV query re-projection during SBG recurrence.
//!
//! This kernel changes exactly one DKV-2b mechanism: before each memory read it
//! re-runs the already-qualified DKV-2a projector on the current SemanticState.
//! MOR selection is frozen at construction and is never recomputed here.

use core::fmt;
use std::collections::BTreeSet;

use crate::{
    delta_kv::{DeltaKvCounters, DeltaKvError, DeltaKvMemory},
    delta_kv_recurrent::DeltaKvRecurrentPolicy,
    delta_kv_semantic::{
        DeltaKvSemanticError, DeltaKvSemanticProjectionCounters, DeltaKvSemanticProjector,
        DeltaKvSourceProvenance,
    },
    mor::MorDecision,
    semantic::{SemanticBitFieldError, SemanticRewriteStep, SemanticRewriter, SemanticState},
};

/// Per-cycle evidence from one dynamic re-key/read/rewrite step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvDynamicCycleEvidence {
    pub depth: u32,
    pub key_q12: Vec<i16>,
    pub key_changed: bool,
    pub read_values_q12: Vec<i32>,
    pub ambiguous_values: usize,
    pub read_macs_delta: u64,
    pub projection: DeltaKvSemanticProjectionCounters,
}

/// Dynamic DKV-2c recurrent kernel with frozen MOR selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvDynamicRekeyKernel<const BITS: usize, const KEY_DIM: usize> {
    memory: DeltaKvMemory,
    projector: DeltaKvSemanticProjector<KEY_DIM>,
    mor: MorDecision,
    provenance: Option<DeltaKvSourceProvenance>,
    value_coordinates: Vec<usize>,
    policy: DeltaKvRecurrentPolicy,
    counters: DeltaKvCounters,
    projection_counters: DeltaKvSemanticProjectionCounters,
    cycles: Vec<DeltaKvDynamicCycleEvidence>,
    distinct_keys: Vec<Vec<i16>>,
}

impl<const BITS: usize, const KEY_DIM: usize> DeltaKvDynamicRekeyKernel<BITS, KEY_DIM> {
    pub fn new(
        memory: DeltaKvMemory,
        projector: DeltaKvSemanticProjector<KEY_DIM>,
        mor: MorDecision,
        provenance: Option<DeltaKvSourceProvenance>,
        value_coordinates: Vec<usize>,
        policy: DeltaKvRecurrentPolicy,
    ) -> Result<Self, DeltaKvDynamicRekeyError> {
        if policy.ambiguity_threshold_q12 < 0 {
            return Err(DeltaKvDynamicRekeyError::InvalidPolicy);
        }
        if memory.key_dim() != KEY_DIM {
            return Err(DeltaKvDynamicRekeyError::KeyDimensionMismatch {
                expected: memory.key_dim(),
                actual: KEY_DIM,
            });
        }
        if value_coordinates.len() != memory.value_dim() {
            return Err(DeltaKvDynamicRekeyError::ValueDimensionMismatch {
                expected: memory.value_dim(),
                actual: value_coordinates.len(),
            });
        }
        let unique_coordinates = value_coordinates.iter().copied().collect::<BTreeSet<_>>();
        if unique_coordinates.len() != value_coordinates.len() {
            return Err(DeltaKvDynamicRekeyError::DuplicateValueCoordinate);
        }
        if value_coordinates.iter().any(|coordinate| *coordinate >= BITS) {
            return Err(DeltaKvDynamicRekeyError::ValueCoordinateOutOfRange);
        }
        if mor.selected_experts.is_empty() {
            return Err(DeltaKvDynamicRekeyError::EmptyMorSelection);
        }
        let unique_experts = mor
            .selected_experts
            .iter()
            .map(|page| page.0)
            .collect::<BTreeSet<_>>();
        if unique_experts.len() != mor.selected_experts.len() {
            return Err(DeltaKvDynamicRekeyError::DuplicateMorSelection);
        }

        Ok(Self {
            memory,
            projector,
            mor,
            provenance,
            value_coordinates,
            policy,
            counters: DeltaKvCounters::default(),
            projection_counters: DeltaKvSemanticProjectionCounters::default(),
            cycles: Vec::new(),
            distinct_keys: Vec::new(),
        })
    }

    #[must_use]
    pub const fn counters(&self) -> DeltaKvCounters {
        self.counters
    }

    #[must_use]
    pub const fn projection_counters(&self) -> DeltaKvSemanticProjectionCounters {
        self.projection_counters
    }

    #[must_use]
    pub fn cycle_evidence(&self) -> &[DeltaKvDynamicCycleEvidence] {
        &self.cycles
    }

    #[must_use]
    pub fn distinct_keys(&self) -> &[Vec<i16>] {
        &self.distinct_keys
    }

    #[must_use]
    pub fn key_change_count(&self) -> usize {
        self.cycles.iter().filter(|cycle| cycle.key_changed).count()
    }
}

impl<const BITS: usize, const KEY_DIM: usize> SemanticRewriter<BITS>
    for DeltaKvDynamicRekeyKernel<BITS, KEY_DIM>
{
    type Error = DeltaKvDynamicRekeyError;

    fn rewrite(
        &mut self,
        state: &SemanticState<BITS>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<BITS>, Self::Error> {
        let projection = self
            .projector
            .project(state, &self.mor, self.provenance.as_ref())
            .map_err(DeltaKvDynamicRekeyError::Projection)?;
        add_projection_counters(&mut self.projection_counters, projection.counters);

        let key_changed = self
            .cycles
            .last()
            .map(|previous| previous.key_q12 != projection.key_q12)
            .unwrap_or(false);
        if !self.distinct_keys.contains(&projection.key_q12) {
            self.distinct_keys.push(projection.key_q12.clone());
        }

        let before_macs = self.counters.read_macs;
        let values = self
            .memory
            .read(&projection.key_q12, &mut self.counters)
            .map_err(DeltaKvDynamicRekeyError::DeltaKv)?;
        if values.len() != self.value_coordinates.len() {
            return Err(DeltaKvDynamicRekeyError::InternalInvariant);
        }

        let mut next = state.clone();
        let mut ambiguous_values = 0_usize;
        for (value, coordinate) in values.iter().zip(&self.value_coordinates) {
            if value.unsigned_abs() <= self.policy.ambiguity_threshold_q12 as u32 {
                next.ambiguity.set(*coordinate, true)?;
                ambiguous_values = ambiguous_values.saturating_add(1);
            } else {
                next.value.set(*coordinate, *value > 0)?;
                next.ambiguity.set(*coordinate, false)?;
            }
        }

        self.cycles.push(DeltaKvDynamicCycleEvidence {
            depth,
            key_q12: projection.key_q12,
            key_changed,
            read_values_q12: values,
            ambiguous_values,
            read_macs_delta: self.counters.read_macs.saturating_sub(before_macs),
            projection: projection.counters,
        });

        Ok(SemanticRewriteStep {
            state: next,
            stability_blocked: ambiguous_values != 0,
        })
    }
}

fn add_projection_counters(
    total: &mut DeltaKvSemanticProjectionCounters,
    delta: DeltaKvSemanticProjectionCounters,
) {
    total.semantic_coordinates_read = total
        .semantic_coordinates_read
        .saturating_add(delta.semantic_coordinates_read);
    total.semantic_fields_read = total
        .semantic_fields_read
        .saturating_add(delta.semantic_fields_read);
    total.mor_selected_experts_read = total
        .mor_selected_experts_read
        .saturating_add(delta.mor_selected_experts_read);
    total.bucket_accumulations = total
        .bucket_accumulations
        .saturating_add(delta.bucket_accumulations);
    total.quantized_key_coordinates = total
        .quantized_key_coordinates
        .saturating_add(delta.quantized_key_coordinates);
}

/// DKV-2c validation/execution errors.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeltaKvDynamicRekeyError {
    InvalidPolicy,
    KeyDimensionMismatch { expected: usize, actual: usize },
    ValueDimensionMismatch { expected: usize, actual: usize },
    DuplicateValueCoordinate,
    ValueCoordinateOutOfRange,
    EmptyMorSelection,
    DuplicateMorSelection,
    InternalInvariant,
    DeltaKv(DeltaKvError),
    Projection(DeltaKvSemanticError),
    BitField(SemanticBitFieldError),
}

impl From<DeltaKvError> for DeltaKvDynamicRekeyError {
    fn from(error: DeltaKvError) -> Self {
        Self::DeltaKv(error)
    }
}

impl From<DeltaKvSemanticError> for DeltaKvDynamicRekeyError {
    fn from(error: DeltaKvSemanticError) -> Self {
        Self::Projection(error)
    }
}

impl From<SemanticBitFieldError> for DeltaKvDynamicRekeyError {
    fn from(error: SemanticBitFieldError) -> Self {
        Self::BitField(error)
    }
}

impl fmt::Display for DeltaKvDynamicRekeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for DeltaKvDynamicRekeyError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        delta_kv_semantic::DeltaKvSemanticProjectionPolicy,
        semantic::SemanticState,
        PageId,
    };

    fn mor() -> MorDecision {
        MorDecision {
            active_routers: Vec::new(),
            ranked_experts: Vec::new(),
            selected_experts: vec![PageId(5)],
            noise_ppm: 0,
            structural_disagreement_ppm: 0,
            expert_mass_threshold_ppm: 0,
            request_precision_escalation: false,
            hysteresis_retained_previous_primary: false,
        }
    }

    #[test]
    fn constructor_rejects_geometry_and_mor_drift() {
        let projector = DeltaKvSemanticProjector::<8>::new(
            DeltaKvSemanticProjectionPolicy {
                semantic_weight: 1,
                mor_weight: 32,
            },
            17,
        )
        .unwrap();
        let mut duplicate_mor = mor();
        duplicate_mor.selected_experts.push(PageId(5));

        assert!(matches!(
            DeltaKvDynamicRekeyKernel::<8, 8>::new(
                DeltaKvMemory::new(8, 2).unwrap(),
                projector,
                duplicate_mor,
                None,
                vec![0, 1],
                DeltaKvRecurrentPolicy::default(),
            ),
            Err(DeltaKvDynamicRekeyError::DuplicateMorSelection)
        ));

        assert!(matches!(
            DeltaKvDynamicRekeyKernel::<8, 8>::new(
                DeltaKvMemory::new(8, 2).unwrap(),
                projector,
                mor(),
                None,
                vec![0],
                DeltaKvRecurrentPolicy::default(),
            ),
            Err(DeltaKvDynamicRekeyError::ValueDimensionMismatch {
                expected: 2,
                actual: 1
            })
        ));
    }

    #[test]
    fn dynamic_kernel_projects_before_read() {
        let projector = DeltaKvSemanticProjector::<8>::new(
            DeltaKvSemanticProjectionPolicy {
                semantic_weight: 1,
                mor_weight: 32,
            },
            17,
        )
        .unwrap();
        let state = SemanticState::<8>::zeroed().unwrap();
        let mut kernel = DeltaKvDynamicRekeyKernel::<8, 8>::new(
            DeltaKvMemory::new(8, 2).unwrap(),
            projector,
            mor(),
            None,
            vec![0, 1],
            DeltaKvRecurrentPolicy::default(),
        )
        .unwrap();

        let _ = kernel.rewrite(&state, 1).unwrap();
        assert_eq!(kernel.cycle_evidence().len(), 1);
        assert_eq!(kernel.counters().reads, 1);
        assert_eq!(kernel.projection_counters().semantic_fields_read, 4);
        assert_eq!(kernel.projection_counters().semantic_coordinates_read, 32);
        assert_eq!(kernel.distinct_keys().len(), 1);
    }
}
