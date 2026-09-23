//! DKV-0b: preregistered deterministic interference/correction panel.
//!
//! The panel is fixed in source before comparative observation. It deliberately
//! includes favorable and unfavorable regimes and records all outcomes.

use core::fmt;
use std::collections::BTreeSet;

use crate::delta_kv::{DeltaKvCounters, DeltaKvError, DeltaKvMemory, DELTA_KV_ONE, DELTA_KV_SCALE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeltaKvPanelClass {
    Orthogonal,
    Overlap25,
    Overlap50,
    Overlap75,
    RepeatedOverwrite,
    ConflictingTarget,
    PartialRate25,
    PartialRate50,
    PartialRate75,
    PartialRate100,
    SaturationStress,
    Dimension2x1,
    Dimension4x2,
    Dimension8x4,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvPanelWrite {
    pub key_index: usize,
    pub target_q12: Vec<i16>,
    pub learning_rate_q12: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenDeltaKvCase {
    pub id: &'static str,
    pub class: DeltaKvPanelClass,
    pub key_dim: usize,
    pub value_dim: usize,
    pub keys_q12: Vec<Vec<i16>>,
    pub writes: Vec<DeltaKvPanelWrite>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaKvWriteEvidence {
    pub write_index: usize,
    pub key_index: usize,
    pub current_key_error_q12: u64,
    pub prior_keys_error_before_q12: u64,
    pub prior_keys_error_after_q12: u64,
    pub saturations_after: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvInterferenceCell {
    pub after_write: usize,
    pub key_index: usize,
    pub l1_error_q12: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvCaseReport {
    pub id: &'static str,
    pub class: DeltaKvPanelClass,
    pub key_dim: usize,
    pub value_dim: usize,
    pub semantic_bits: u128,
    pub writes: Vec<DeltaKvWriteEvidence>,
    pub interference_matrix: Vec<DeltaKvInterferenceCell>,
    pub counters: DeltaKvCounters,
    pub final_active_keys: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvPanelReport {
    pub cases: Vec<DeltaKvCaseReport>,
    pub total_cases: usize,
    pub total_updates: u64,
    pub total_reads: u64,
    pub total_read_macs: u64,
    pub total_weight_updates: u64,
    pub total_saturations: u64,
}

pub fn frozen_delta_kv_cases() -> Vec<FrozenDeltaKvCase> {
    let one = DELTA_KV_ONE;
    let q = |numerator: u16, denominator: u16| {
        u16::try_from(
            (u32::from(DELTA_KV_SCALE as u16) * u32::from(numerator)) / u32::from(denominator),
        )
        .expect("bounded Q12 learning rate")
    };

    let overlap_case = |id, class, overlap: usize| {
        let mut a = vec![0_i16; 8];
        let mut b = vec![0_i16; 8];
        for slot in a.iter_mut().take(4) {
            *slot = one;
        }
        for slot in b.iter_mut().take(overlap) {
            *slot = one;
        }
        for slot in b.iter_mut().skip(4).take(4 - overlap) {
            *slot = one;
        }
        FrozenDeltaKvCase {
            id,
            class,
            key_dim: 8,
            value_dim: 1,
            keys_q12: vec![a, b],
            writes: vec![
                DeltaKvPanelWrite {
                    key_index: 0,
                    target_q12: vec![1000],
                    learning_rate_q12: DELTA_KV_SCALE as u16,
                },
                DeltaKvPanelWrite {
                    key_index: 1,
                    target_q12: vec![2000],
                    learning_rate_q12: DELTA_KV_SCALE as u16,
                },
            ],
        }
    };

    let partial_case = |id, class, rate| FrozenDeltaKvCase {
        id,
        class,
        key_dim: 2,
        value_dim: 1,
        keys_q12: vec![vec![one, 0]],
        writes: vec![DeltaKvPanelWrite {
            key_index: 0,
            target_q12: vec![1600],
            learning_rate_q12: rate,
        }],
    };

    let dimension_case = |id, class, key_dim: usize, value_dim: usize| {
        let mut key = vec![0_i16; key_dim];
        key[0] = one;
        FrozenDeltaKvCase {
            id,
            class,
            key_dim,
            value_dim,
            keys_q12: vec![key],
            writes: vec![DeltaKvPanelWrite {
                key_index: 0,
                target_q12: (0..value_dim)
                    .map(|index| i16::try_from(300 + index * 100).expect("small target"))
                    .collect(),
                learning_rate_q12: DELTA_KV_SCALE as u16,
            }],
        }
    };

    vec![
        overlap_case("orthogonal-0", DeltaKvPanelClass::Orthogonal, 0),
        overlap_case("overlap-25", DeltaKvPanelClass::Overlap25, 1),
        overlap_case("overlap-50", DeltaKvPanelClass::Overlap50, 2),
        overlap_case("overlap-75", DeltaKvPanelClass::Overlap75, 3),
        FrozenDeltaKvCase {
            id: "repeated-overwrite",
            class: DeltaKvPanelClass::RepeatedOverwrite,
            key_dim: 2,
            value_dim: 1,
            keys_q12: vec![vec![one, 0]],
            writes: vec![
                DeltaKvPanelWrite {
                    key_index: 0,
                    target_q12: vec![1000],
                    learning_rate_q12: DELTA_KV_SCALE as u16,
                },
                DeltaKvPanelWrite {
                    key_index: 0,
                    target_q12: vec![-1000],
                    learning_rate_q12: DELTA_KV_SCALE as u16,
                },
                DeltaKvPanelWrite {
                    key_index: 0,
                    target_q12: vec![500],
                    learning_rate_q12: DELTA_KV_SCALE as u16,
                },
            ],
        },
        FrozenDeltaKvCase {
            id: "conflicting-target",
            class: DeltaKvPanelClass::ConflictingTarget,
            key_dim: 4,
            value_dim: 1,
            keys_q12: vec![vec![one, one, 0, 0], vec![one, one, 0, 0]],
            writes: vec![
                DeltaKvPanelWrite {
                    key_index: 0,
                    target_q12: vec![1800],
                    learning_rate_q12: DELTA_KV_SCALE as u16,
                },
                DeltaKvPanelWrite {
                    key_index: 1,
                    target_q12: vec![-1800],
                    learning_rate_q12: DELTA_KV_SCALE as u16,
                },
            ],
        },
        partial_case("partial-rate-25", DeltaKvPanelClass::PartialRate25, q(1, 4)),
        partial_case("partial-rate-50", DeltaKvPanelClass::PartialRate50, q(1, 2)),
        partial_case("partial-rate-75", DeltaKvPanelClass::PartialRate75, q(3, 4)),
        partial_case(
            "partial-rate-100",
            DeltaKvPanelClass::PartialRate100,
            DELTA_KV_SCALE as u16,
        ),
        FrozenDeltaKvCase {
            id: "saturation-stress",
            class: DeltaKvPanelClass::SaturationStress,
            key_dim: 1,
            value_dim: 1,
            keys_q12: vec![vec![256]],
            writes: vec![DeltaKvPanelWrite {
                key_index: 0,
                target_q12: vec![i16::MAX],
                learning_rate_q12: DELTA_KV_SCALE as u16,
            }],
        },
        dimension_case("dimension-2x1", DeltaKvPanelClass::Dimension2x1, 2, 1),
        dimension_case("dimension-4x2", DeltaKvPanelClass::Dimension4x2, 4, 2),
        dimension_case("dimension-8x4", DeltaKvPanelClass::Dimension8x4, 8, 4),
    ]
}

pub fn run_frozen_delta_kv_panel() -> Result<DeltaKvPanelReport, DeltaKvPanelError> {
    let cases = frozen_delta_kv_cases();
    validate_panel_definition(&cases)?;

    let mut reports = Vec::new();
    reports
        .try_reserve_exact(cases.len())
        .map_err(|_| DeltaKvPanelError::Allocation)?;

    let mut total_updates = 0_u64;
    let mut total_reads = 0_u64;
    let mut total_read_macs = 0_u64;
    let mut total_weight_updates = 0_u64;
    let mut total_saturations = 0_u64;

    for case in &cases {
        let report = run_case(case)?;
        total_updates = total_updates.saturating_add(report.counters.updates);
        total_reads = total_reads.saturating_add(report.counters.reads);
        total_read_macs = total_read_macs.saturating_add(report.counters.read_macs);
        total_weight_updates = total_weight_updates.saturating_add(report.counters.weight_updates);
        total_saturations = total_saturations.saturating_add(report.counters.saturations);
        reports.push(report);
    }

    Ok(DeltaKvPanelReport {
        total_cases: reports.len(),
        cases: reports,
        total_updates,
        total_reads,
        total_read_macs,
        total_weight_updates,
        total_saturations,
    })
}

fn run_case(case: &FrozenDeltaKvCase) -> Result<DeltaKvCaseReport, DeltaKvPanelError> {
    let mut memory = DeltaKvMemory::new(case.key_dim, case.value_dim)?;
    let semantic_bits = memory.semantic_bits();
    let mut counters = DeltaKvCounters::default();
    let mut active_targets = vec![None::<Vec<i16>>; case.keys_q12.len()];
    let mut writes = Vec::new();
    let mut interference_matrix = Vec::new();

    for (write_index, write) in case.writes.iter().enumerate() {
        let key = case
            .keys_q12
            .get(write.key_index)
            .ok_or(DeltaKvPanelError::InvalidKeyIndex {
                case_id: case.id,
                key_index: write.key_index,
            })?;
        if write.target_q12.len() != case.value_dim {
            return Err(DeltaKvPanelError::InvalidTargetWidth {
                case_id: case.id,
                expected: case.value_dim,
                actual: write.target_q12.len(),
            });
        }

        let prior_before = aggregate_prior_error(
            &memory,
            &case.keys_q12,
            &active_targets,
            write.key_index,
            &mut counters,
        )?;

        memory.update(
            key,
            &write.target_q12,
            write.learning_rate_q12,
            &mut counters,
        )?;
        active_targets[write.key_index] = Some(write.target_q12.clone());

        let current_prediction = memory.read(key, &mut counters)?;
        let current_error = l1_error(&current_prediction, &write.target_q12)?;

        let prior_after = aggregate_prior_error(
            &memory,
            &case.keys_q12,
            &active_targets,
            write.key_index,
            &mut counters,
        )?;

        writes.push(DeltaKvWriteEvidence {
            write_index,
            key_index: write.key_index,
            current_key_error_q12: current_error,
            prior_keys_error_before_q12: prior_before,
            prior_keys_error_after_q12: prior_after,
            saturations_after: counters.saturations,
        });

        for (key_index, target) in active_targets.iter().enumerate() {
            if let Some(target) = target {
                let prediction = memory.read(&case.keys_q12[key_index], &mut counters)?;
                interference_matrix.push(DeltaKvInterferenceCell {
                    after_write: write_index,
                    key_index,
                    l1_error_q12: l1_error(&prediction, target)?,
                });
            }
        }
    }

    Ok(DeltaKvCaseReport {
        id: case.id,
        class: case.class,
        key_dim: case.key_dim,
        value_dim: case.value_dim,
        semantic_bits,
        writes,
        interference_matrix,
        counters,
        final_active_keys: active_targets
            .iter()
            .filter(|target| target.is_some())
            .count(),
    })
}

fn aggregate_prior_error(
    memory: &DeltaKvMemory,
    keys: &[Vec<i16>],
    targets: &[Option<Vec<i16>>],
    current_key_index: usize,
    counters: &mut DeltaKvCounters,
) -> Result<u64, DeltaKvPanelError> {
    let mut total = 0_u64;
    for (key_index, target) in targets.iter().enumerate() {
        if key_index == current_key_index {
            continue;
        }
        if let Some(target) = target {
            let prediction = memory.read(&keys[key_index], counters)?;
            total = total
                .checked_add(l1_error(&prediction, target)?)
                .ok_or(DeltaKvPanelError::Overflow)?;
        }
    }
    Ok(total)
}

fn l1_error(prediction: &[i32], target: &[i16]) -> Result<u64, DeltaKvPanelError> {
    if prediction.len() != target.len() {
        return Err(DeltaKvPanelError::InternalShapeMismatch);
    }
    prediction
        .iter()
        .zip(target)
        .try_fold(0_u64, |sum, (prediction, target)| {
            let delta = i64::from(*prediction) - i64::from(*target);
            sum.checked_add(delta.unsigned_abs())
                .ok_or(DeltaKvPanelError::Overflow)
        })
}

fn validate_panel_definition(cases: &[FrozenDeltaKvCase]) -> Result<(), DeltaKvPanelError> {
    if cases.is_empty() {
        return Err(DeltaKvPanelError::EmptyPanel);
    }
    let mut ids = BTreeSet::new();
    let mut classes = BTreeSet::new();
    for case in cases {
        if !ids.insert(case.id) {
            return Err(DeltaKvPanelError::DuplicateCaseId(case.id));
        }
        if !classes.insert(case.class) {
            return Err(DeltaKvPanelError::DuplicateCaseClass(case.class));
        }
        if case.key_dim == 0
            || case.value_dim == 0
            || case.keys_q12.is_empty()
            || case.writes.is_empty()
        {
            return Err(DeltaKvPanelError::InvalidCase(case.id));
        }
        if case.keys_q12.iter().any(|key| key.len() != case.key_dim) {
            return Err(DeltaKvPanelError::InvalidCase(case.id));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaKvPanelError {
    EmptyPanel,
    DuplicateCaseId(&'static str),
    DuplicateCaseClass(DeltaKvPanelClass),
    InvalidCase(&'static str),
    InvalidKeyIndex {
        case_id: &'static str,
        key_index: usize,
    },
    InvalidTargetWidth {
        case_id: &'static str,
        expected: usize,
        actual: usize,
    },
    InternalShapeMismatch,
    Overflow,
    Allocation,
    DeltaKv(DeltaKvError),
}

impl From<DeltaKvError> for DeltaKvPanelError {
    fn from(error: DeltaKvError) -> Self {
        Self::DeltaKv(error)
    }
}

impl fmt::Display for DeltaKvPanelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DeltaKvPanelError {}
