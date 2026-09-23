//! DKV-2a: bounded SBG/MOR projection into Delta-KV key space.
//!
//! This adapter does not create another router. It consumes the already-produced
//! native MOR decision and a SemanticState. Raw/source provenance is carried as
//! separate evidence and deliberately does not influence the compressed key.

use core::fmt;
use std::collections::BTreeSet;

use crate::{
    mor::MorDecision,
    semantic::{SemanticBitField, SemanticBitFieldError, SemanticState},
    PageId,
};

use crate::delta_kv::{DELTA_KV_ONE, DELTA_KV_SCALE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvSourceProvenance {
    pub semantic_record_id: u64,
    pub raw_segment_ids: Vec<u64>,
}

impl DeltaKvSourceProvenance {
    pub fn new(
        semantic_record_id: u64,
        raw_segment_ids: Vec<u64>,
    ) -> Result<Self, DeltaKvSemanticError> {
        if raw_segment_ids.is_empty() {
            return Err(DeltaKvSemanticError::EmptyRawProvenance);
        }
        let unique = raw_segment_ids.iter().copied().collect::<BTreeSet<_>>();
        if unique.len() != raw_segment_ids.len() {
            return Err(DeltaKvSemanticError::DuplicateRawSegment);
        }
        Ok(Self {
            semantic_record_id,
            raw_segment_ids,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaKvSemanticProjectionPolicy {
    pub semantic_weight: i32,
    pub mor_weight: i32,
}

impl Default for DeltaKvSemanticProjectionPolicy {
    fn default() -> Self {
        Self {
            semantic_weight: 1,
            mor_weight: 8,
        }
    }
}

impl DeltaKvSemanticProjectionPolicy {
    pub fn validate(self) -> Result<Self, DeltaKvSemanticError> {
        if self.semantic_weight <= 0 || self.mor_weight <= 0 {
            return Err(DeltaKvSemanticError::InvalidPolicy);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeltaKvSemanticProjectionCounters {
    pub semantic_coordinates_read: u64,
    pub semantic_fields_read: u64,
    pub mor_selected_experts_read: u64,
    pub bucket_accumulations: u64,
    pub quantized_key_coordinates: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvSemanticKeyEvidence {
    pub key_q12: Vec<i16>,
    pub selected_experts: Vec<PageId>,
    pub provenance: Option<DeltaKvSourceProvenance>,
    pub counters: DeltaKvSemanticProjectionCounters,
    pub transient_key_bits: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaKvSemanticProjector<const KEY_DIM: usize> {
    policy: DeltaKvSemanticProjectionPolicy,
    seed: u64,
}

impl<const KEY_DIM: usize> DeltaKvSemanticProjector<KEY_DIM> {
    pub fn new(
        policy: DeltaKvSemanticProjectionPolicy,
        seed: u64,
    ) -> Result<Self, DeltaKvSemanticError> {
        if KEY_DIM == 0 {
            return Err(DeltaKvSemanticError::ZeroKeyDimension);
        }
        Ok(Self {
            policy: policy.validate()?,
            seed,
        })
    }

    #[must_use]
    pub const fn key_dim(&self) -> usize {
        KEY_DIM
    }

    #[must_use]
    pub const fn transient_key_bits(&self) -> u128 {
        KEY_DIM as u128 * 16
    }

    pub fn project<const BITS: usize>(
        &self,
        state: &SemanticState<BITS>,
        mor: &MorDecision,
        provenance: Option<&DeltaKvSourceProvenance>,
    ) -> Result<DeltaKvSemanticKeyEvidence, DeltaKvSemanticError> {
        if mor.selected_experts.is_empty() {
            return Err(DeltaKvSemanticError::EmptyMorSelection);
        }
        let unique_pages = mor
            .selected_experts
            .iter()
            .map(|page| page.0)
            .collect::<BTreeSet<_>>();
        if unique_pages.len() != mor.selected_experts.len() {
            return Err(DeltaKvSemanticError::DuplicateMorSelection);
        }

        if let Some(provenance) = provenance {
            let unique_sources = provenance
                .raw_segment_ids
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            if provenance.raw_segment_ids.is_empty() {
                return Err(DeltaKvSemanticError::EmptyRawProvenance);
            }
            if unique_sources.len() != provenance.raw_segment_ids.len() {
                return Err(DeltaKvSemanticError::DuplicateRawSegment);
            }
        }

        let mut accumulators = vec![0_i64; KEY_DIM];
        let mut counters = DeltaKvSemanticProjectionCounters::default();

        self.fold_field(0, &state.value, &mut accumulators, &mut counters)?;
        self.fold_field(1, &state.meaning_mask, &mut accumulators, &mut counters)?;
        self.fold_field(2, &state.ambiguity, &mut accumulators, &mut counters)?;
        self.fold_field(3, &state.routing, &mut accumulators, &mut counters)?;

        for (rank, page) in mor.selected_experts.iter().enumerate() {
            let hash = mix64(
                self.seed
                    ^ 0x6d6f_725f_7061_6765
                    ^ u64::from(page.0)
                    ^ u64::try_from(rank).map_err(|_| DeltaKvSemanticError::Overflow)?,
            );
            let bucket = usize::try_from(hash % KEY_DIM as u64)
                .map_err(|_| DeltaKvSemanticError::Overflow)?;
            let sign = if hash & (1_u64 << 63) == 0 {
                1_i64
            } else {
                -1_i64
            };
            accumulators[bucket] = accumulators[bucket]
                .checked_add(sign * i64::from(self.policy.mor_weight))
                .ok_or(DeltaKvSemanticError::Overflow)?;
            counters.mor_selected_experts_read =
                counters.mor_selected_experts_read.saturating_add(1);
            counters.bucket_accumulations = counters.bucket_accumulations.saturating_add(1);
        }

        let mut key_q12 = Vec::new();
        key_q12
            .try_reserve_exact(KEY_DIM)
            .map_err(|_| DeltaKvSemanticError::Allocation)?;
        for accumulator in accumulators {
            let quantized = accumulator.signum();
            key_q12.push(match quantized {
                -1 => -DELTA_KV_ONE,
                0 => 0,
                1 => DELTA_KV_ONE,
                _ => return Err(DeltaKvSemanticError::InternalInvariant),
            });
            counters.quantized_key_coordinates =
                counters.quantized_key_coordinates.saturating_add(1);
        }
        if key_q12.iter().all(|component| *component == 0) {
            return Err(DeltaKvSemanticError::ZeroProjectedKey);
        }

        Ok(DeltaKvSemanticKeyEvidence {
            key_q12,
            selected_experts: mor.selected_experts.clone(),
            provenance: provenance.cloned(),
            counters,
            transient_key_bits: self.transient_key_bits(),
        })
    }

    fn fold_field<const BITS: usize>(
        &self,
        field_tag: u64,
        field: &SemanticBitField<BITS>,
        accumulators: &mut [i64],
        counters: &mut DeltaKvSemanticProjectionCounters,
    ) -> Result<(), DeltaKvSemanticError> {
        counters.semantic_fields_read = counters.semantic_fields_read.saturating_add(1);
        for index in 0..BITS {
            let bit = field.get(index)?;
            let hash = mix64(
                self.seed
                    ^ field_tag.wrapping_mul(0x9e37_79b9_7f4a_7c15)
                    ^ u64::try_from(index).map_err(|_| DeltaKvSemanticError::Overflow)?,
            );
            let bucket = usize::try_from(hash % KEY_DIM as u64)
                .map_err(|_| DeltaKvSemanticError::Overflow)?;
            let projection_sign = if hash & (1_u64 << 62) == 0 {
                1_i64
            } else {
                -1_i64
            };
            let semantic_sign = if bit { 1_i64 } else { -1_i64 };
            let delta = projection_sign
                .checked_mul(semantic_sign)
                .and_then(|value| value.checked_mul(i64::from(self.policy.semantic_weight)))
                .ok_or(DeltaKvSemanticError::Overflow)?;
            accumulators[bucket] = accumulators[bucket]
                .checked_add(delta)
                .ok_or(DeltaKvSemanticError::Overflow)?;
            counters.semantic_coordinates_read =
                counters.semantic_coordinates_read.saturating_add(1);
            counters.bucket_accumulations = counters.bucket_accumulations.saturating_add(1);
        }
        Ok(())
    }
}

#[must_use]
pub const fn delta_kv_q12_scale() -> i32 {
    DELTA_KV_SCALE
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaKvSemanticError {
    ZeroKeyDimension,
    InvalidPolicy,
    EmptyMorSelection,
    DuplicateMorSelection,
    EmptyRawProvenance,
    DuplicateRawSegment,
    ZeroProjectedKey,
    Overflow,
    Allocation,
    InternalInvariant,
    BitField(SemanticBitFieldError),
}

impl From<SemanticBitFieldError> for DeltaKvSemanticError {
    fn from(error: SemanticBitFieldError) -> Self {
        Self::BitField(error)
    }
}

impl fmt::Display for DeltaKvSemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DeltaKvSemanticError {}
