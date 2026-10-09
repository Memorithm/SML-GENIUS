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
        if value_coordinates
            .iter()
            .any(|coordinate| *coordinate >= BITS)
        {
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

    /// Opt-in dynamic routing: project the current SBG state AND supplied MOR
    /// decision. Failed projection/read leaves all owned memory/evidence intact.
    /// The legacy SemanticRewriter path remains frozen-route when used alone.
    pub fn rewrite_with_mor(
        &mut self,
        state: &SemanticState<BITS>,
        depth: u32,
        mor: &MorDecision,
    ) -> Result<SemanticRewriteStep<BITS>, DeltaKvDynamicRekeyError> {
        if usize::try_from(depth).ok() != self.cycles.len().checked_add(1) {
            return Err(DeltaKvDynamicRekeyError::InvalidDepth);
        }
        let mut candidate = self.clone();
        candidate.mor = mor.clone();
        let result = candidate.rewrite(state, depth)?;
        *self = candidate;
        Ok(result)
    }

    #[must_use]
    pub fn mor_decision(&self) -> &MorDecision {
        &self.mor
    }

    #[must_use]
    pub fn provenance(&self) -> Option<&DeltaKvSourceProvenance> {
        self.provenance.as_ref()
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
    InvalidDepth,
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
        delta_kv_semantic::DeltaKvSemanticProjectionPolicy, semantic::SemanticState, PageId,
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

#[cfg(test)]
mod current_mor_tests {
    use super::*;
    use crate::{
        delta_kv::DELTA_KV_SCALE, delta_kv_semantic::DeltaKvSemanticProjectionPolicy, PageId,
    };
    fn mor(page: u32) -> MorDecision {
        MorDecision {
            active_routers: vec![],
            ranked_experts: vec![],
            selected_experts: vec![PageId(page)],
            noise_ppm: 0,
            structural_disagreement_ppm: 0,
            expert_mass_threshold_ppm: 0,
            request_precision_escalation: false,
            hysteresis_retained_previous_primary: false,
        }
    }
    fn projector() -> DeltaKvSemanticProjector<8> {
        DeltaKvSemanticProjector::new(
            DeltaKvSemanticProjectionPolicy {
                semantic_weight: 1,
                mor_weight: 32,
            },
            17,
        )
        .unwrap()
    }
    fn kernel() -> DeltaKvDynamicRekeyKernel<8, 8> {
        let state = SemanticState::<8>::zeroed().unwrap();
        let key = projector().project(&state, &mor(0), None).unwrap();
        let mut memory = DeltaKvMemory::new(8, 1).unwrap();
        memory
            .update(
                &key.key_q12,
                &[2000],
                DELTA_KV_SCALE as u16,
                &mut DeltaKvCounters::default(),
            )
            .unwrap();
        DeltaKvDynamicRekeyKernel::new(
            memory,
            projector(),
            mor(0),
            Some(DeltaKvSourceProvenance::new(7, vec![11, 13]).unwrap()),
            vec![0],
            DeltaKvRecurrentPolicy::default(),
        )
        .unwrap()
    }
    #[test]
    fn new_route_and_state_are_used_without_changing_legacy_frozen_route_control() {
        let mut current = kernel();
        let mut legacy = current.clone();
        let state = SemanticState::<8>::zeroed().unwrap();
        current.rewrite_with_mor(&state, 1, &mor(0)).unwrap();
        legacy.rewrite(&state, 1).unwrap();
        assert_eq!(current, legacy);
        let mut changed = state.clone();
        changed.value.set(1, true).unwrap();
        changed.routing.set(7, true).unwrap();
        current.rewrite_with_mor(&changed, 2, &mor(1)).unwrap();
        legacy.rewrite(&changed, 2).unwrap();
        let expected = projector().project(&changed, &mor(1), None).unwrap();
        let frozen = projector().project(&changed, &mor(0), None).unwrap();
        assert_ne!(expected.key_q12, frozen.key_q12);
        assert_eq!(current.cycle_evidence()[1].key_q12, expected.key_q12);
        assert_eq!(legacy.cycle_evidence()[1].key_q12, frozen.key_q12);
        assert_eq!(current.mor_decision(), &mor(1));
        assert_eq!(legacy.mor_decision(), &mor(0));
        assert_eq!(current.counters().reads, 2);
        assert_eq!(current.provenance().unwrap().raw_segment_ids, vec![11, 13]);
    }
    #[test]
    fn invalid_route_and_depth_cannot_mutate_owned_memory_or_evidence() {
        let mut k = kernel();
        let state = SemanticState::<8>::zeroed().unwrap();
        k.rewrite_with_mor(&state, 1, &mor(0)).unwrap();
        let before = k.clone();
        for depth in [0, 1, 3] {
            assert_eq!(
                k.rewrite_with_mor(&state, depth, &mor(1)),
                Err(DeltaKvDynamicRekeyError::InvalidDepth)
            );
            assert_eq!(k, before);
        }
        let mut empty = mor(0);
        empty.selected_experts.clear();
        assert!(k.rewrite_with_mor(&state, 2, &empty).is_err());
        assert_eq!(k, before);
        let mut dup = mor(0);
        dup.selected_experts.push(PageId(0));
        assert!(k.rewrite_with_mor(&state, 2, &dup).is_err());
        assert_eq!(k, before);
    }
}
