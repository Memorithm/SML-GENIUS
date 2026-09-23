//! DKV-1: matched declared-storage memory controls.
//!
//! The benchmark runs one frozen write/read script against Delta-KV,
//! BoundedAssociativeMemory under both collision policies, and a bounded
//! append-only exact-record ledger. It reports each backend independently and
//! deliberately computes no overall winner.

use core::fmt;

use crate::{
    delta_kv::{DeltaKvCounters, DeltaKvError, DeltaKvMemory, DELTA_KV_ONE, DELTA_KV_SCALE},
    memory::{BoundedAssociativeMemory, MemoryError, ReadOutcome, ReplacementPolicy, WriteOutcome},
    BooleanWorkspace, ExecutionCounters,
};

pub const DKV_CONTROL_KEY_DIM: usize = 16;
pub const DKV_CONTROL_VALUE_DIM: usize = 2;
pub const DKV_CONTROL_VALUE_BITS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DkvControlOperation {
    Write { key: u64, value_q12: [i16; 2] },
    Read { key: u64, expected_q12: [i16; 2] },
}

pub fn frozen_dkv_control_script() -> Vec<DkvControlOperation> {
    const A: u64 = 0x11;
    const B: u64 = 0x22;
    const C: u64 = 0x33;
    const D: u64 = 0x44;

    vec![
        DkvControlOperation::Write {
            key: A,
            value_q12: [1000, -1000],
        },
        DkvControlOperation::Read {
            key: A,
            expected_q12: [1000, -1000],
        },
        DkvControlOperation::Write {
            key: B,
            value_q12: [800, 400],
        },
        DkvControlOperation::Read {
            key: A,
            expected_q12: [1000, -1000],
        },
        DkvControlOperation::Write {
            key: A,
            value_q12: [1200, -500],
        },
        DkvControlOperation::Read {
            key: A,
            expected_q12: [1200, -500],
        },
        DkvControlOperation::Write {
            key: C,
            value_q12: [-700, 900],
        },
        DkvControlOperation::Read {
            key: B,
            expected_q12: [800, 400],
        },
        DkvControlOperation::Write {
            key: D,
            value_q12: [1500, -1500],
        },
        DkvControlOperation::Read {
            key: C,
            expected_q12: [-700, 900],
        },
        DkvControlOperation::Read {
            key: D,
            expected_q12: [1500, -1500],
        },
        DkvControlOperation::Write {
            key: B,
            value_q12: [-800, -400],
        },
        DkvControlOperation::Read {
            key: B,
            expected_q12: [-800, -400],
        },
        DkvControlOperation::Read {
            key: A,
            expected_q12: [1200, -500],
        },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DkvControlBackend {
    DeltaKv,
    DirectRejectCollision,
    DirectReplaceExisting,
    AppendLedger,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DkvControlBackendReport {
    pub backend: DkvControlBackend,
    pub semantic_bits: u128,
    pub storage_budget_bits: u128,
    pub unused_budget_bits: u128,
    pub writes_attempted: u64,
    pub writes_committed: u64,
    pub write_rejections: u64,
    pub reads: u64,
    pub exact_reads: u64,
    pub inexact_reads: u64,
    pub misses: u64,
    pub l1_error_q12: u64,
    pub memory_collisions: u64,
    pub memory_replacements: u64,
    pub key_comparisons: u64,
    pub read_macs: u64,
    pub weight_updates: u64,
    pub saturations: u64,
    pub ledger_scan_comparisons: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DkvMatchedControlReport {
    pub storage_budget_bits: u128,
    pub direct_capacity: usize,
    pub append_capacity: usize,
    pub operations: usize,
    pub writes: u64,
    pub reads: u64,
    pub backends: Vec<DkvControlBackendReport>,
}

pub fn run_dkv_matched_controls() -> Result<DkvMatchedControlReport, DkvControlError> {
    let script = frozen_dkv_control_script();
    let delta = DeltaKvMemory::new(DKV_CONTROL_KEY_DIM, DKV_CONTROL_VALUE_DIM)?;
    let budget = delta.semantic_bits();

    let direct_capacity = maximum_direct_capacity(budget, DKV_CONTROL_VALUE_BITS)?;
    let direct_probe = BoundedAssociativeMemory::new(
        direct_capacity,
        DKV_CONTROL_VALUE_BITS,
        ReplacementPolicy::RejectCollision,
    )?;
    let append_capacity = maximum_append_capacity(budget, DKV_CONTROL_VALUE_BITS)?;
    let append_probe = AppendLedger::new(append_capacity, DKV_CONTROL_VALUE_BITS)?;

    if direct_probe.semantic_bits() > budget || append_probe.semantic_bits() > budget {
        return Err(DkvControlError::StorageBudgetInvariant);
    }

    let backends = vec![
        run_delta_backend(&script, budget)?,
        run_direct_backend(
            &script,
            budget,
            direct_capacity,
            ReplacementPolicy::RejectCollision,
            DkvControlBackend::DirectRejectCollision,
        )?,
        run_direct_backend(
            &script,
            budget,
            direct_capacity,
            ReplacementPolicy::ReplaceExisting,
            DkvControlBackend::DirectReplaceExisting,
        )?,
        run_append_backend(&script, budget, append_capacity)?,
    ];

    let writes = script
        .iter()
        .filter(|op| matches!(op, DkvControlOperation::Write { .. }))
        .count();
    let reads = script.len().saturating_sub(writes);

    Ok(DkvMatchedControlReport {
        storage_budget_bits: budget,
        direct_capacity,
        append_capacity,
        operations: script.len(),
        writes: u64::try_from(writes).map_err(|_| DkvControlError::Overflow)?,
        reads: u64::try_from(reads).map_err(|_| DkvControlError::Overflow)?,
        backends,
    })
}

fn run_delta_backend(
    script: &[DkvControlOperation],
    budget: u128,
) -> Result<DkvControlBackendReport, DkvControlError> {
    let mut memory = DeltaKvMemory::new(DKV_CONTROL_KEY_DIM, DKV_CONTROL_VALUE_DIM)?;
    let mut counters = DeltaKvCounters::default();
    let mut report =
        empty_backend_report(DkvControlBackend::DeltaKv, memory.semantic_bits(), budget)?;

    for operation in script {
        match *operation {
            DkvControlOperation::Write { key, value_q12 } => {
                report.writes_attempted = report.writes_attempted.saturating_add(1);
                memory.update(
                    &project_key_q12(key),
                    &value_q12,
                    DELTA_KV_SCALE as u16,
                    &mut counters,
                )?;
                report.writes_committed = report.writes_committed.saturating_add(1);
            }
            DkvControlOperation::Read { key, expected_q12 } => {
                report.reads = report.reads.saturating_add(1);
                let value = memory.read(&project_key_q12(key), &mut counters)?;
                let error = l1_error_i32(&value, &expected_q12)?;
                record_read_error(&mut report, error);
            }
        }
    }

    report.read_macs = counters.read_macs;
    report.weight_updates = counters.weight_updates;
    report.saturations = counters.saturations;
    Ok(report)
}

fn run_direct_backend(
    script: &[DkvControlOperation],
    budget: u128,
    capacity: usize,
    replacement: ReplacementPolicy,
    backend: DkvControlBackend,
) -> Result<DkvControlBackendReport, DkvControlError> {
    let mut memory = BoundedAssociativeMemory::new(capacity, DKV_CONTROL_VALUE_BITS, replacement)?;
    let mut counters = ExecutionCounters::default();
    let mut report = empty_backend_report(backend, memory.semantic_bits(), budget)?;

    for operation in script {
        match *operation {
            DkvControlOperation::Write { key, value_q12 } => {
                report.writes_attempted = report.writes_attempted.saturating_add(1);
                let outcome = memory.write(key, encode_value(value_q12), &mut counters)?;
                match outcome {
                    WriteOutcome::Inserted { .. }
                    | WriteOutcome::Updated { .. }
                    | WriteOutcome::Replaced { .. } => {
                        report.writes_committed = report.writes_committed.saturating_add(1);
                    }
                    WriteOutcome::CollisionRejected { .. } => {
                        report.write_rejections = report.write_rejections.saturating_add(1);
                    }
                }
            }
            DkvControlOperation::Read { key, expected_q12 } => {
                report.reads = report.reads.saturating_add(1);
                match memory.read(key, &mut counters) {
                    ReadOutcome::Hit { value, .. } => {
                        let decoded = decode_value(&value)?;
                        let prediction = [i32::from(decoded[0]), i32::from(decoded[1])];
                        let error = l1_error_i32(&prediction, &expected_q12)?;
                        record_read_error(&mut report, error);
                    }
                    ReadOutcome::EmptyMiss { .. } | ReadOutcome::KeyMismatch { .. } => {
                        report.misses = report.misses.saturating_add(1);
                    }
                }
            }
        }
    }

    report.memory_collisions = counters.memory_collisions;
    report.memory_replacements = counters.memory_replacements;
    report.key_comparisons = counters.memory_key_comparisons;
    Ok(report)
}

fn run_append_backend(
    script: &[DkvControlOperation],
    budget: u128,
    capacity: usize,
) -> Result<DkvControlBackendReport, DkvControlError> {
    let mut ledger = AppendLedger::new(capacity, DKV_CONTROL_VALUE_BITS)?;
    let mut report = empty_backend_report(
        DkvControlBackend::AppendLedger,
        ledger.semantic_bits(),
        budget,
    )?;

    for operation in script {
        match *operation {
            DkvControlOperation::Write { key, value_q12 } => {
                report.writes_attempted = report.writes_attempted.saturating_add(1);
                if ledger.append(key, value_q12)? {
                    report.writes_committed = report.writes_committed.saturating_add(1);
                } else {
                    report.write_rejections = report.write_rejections.saturating_add(1);
                }
            }
            DkvControlOperation::Read { key, expected_q12 } => {
                report.reads = report.reads.saturating_add(1);
                match ledger.read_latest(key) {
                    Some((value, comparisons, _epoch)) => {
                        report.ledger_scan_comparisons =
                            report.ledger_scan_comparisons.saturating_add(comparisons);
                        let prediction = [i32::from(value[0]), i32::from(value[1])];
                        let error = l1_error_i32(&prediction, &expected_q12)?;
                        record_read_error(&mut report, error);
                    }
                    None => {
                        report.ledger_scan_comparisons =
                            report.ledger_scan_comparisons.saturating_add(
                                u64::try_from(ledger.len())
                                    .map_err(|_| DkvControlError::Overflow)?,
                            );
                        report.misses = report.misses.saturating_add(1);
                    }
                }
            }
        }
    }
    Ok(report)
}

fn empty_backend_report(
    backend: DkvControlBackend,
    semantic_bits: u128,
    budget: u128,
) -> Result<DkvControlBackendReport, DkvControlError> {
    if semantic_bits > budget {
        return Err(DkvControlError::StorageBudgetInvariant);
    }
    Ok(DkvControlBackendReport {
        backend,
        semantic_bits,
        storage_budget_bits: budget,
        unused_budget_bits: budget - semantic_bits,
        writes_attempted: 0,
        writes_committed: 0,
        write_rejections: 0,
        reads: 0,
        exact_reads: 0,
        inexact_reads: 0,
        misses: 0,
        l1_error_q12: 0,
        memory_collisions: 0,
        memory_replacements: 0,
        key_comparisons: 0,
        read_macs: 0,
        weight_updates: 0,
        saturations: 0,
        ledger_scan_comparisons: 0,
    })
}

fn record_read_error(report: &mut DkvControlBackendReport, error: u64) {
    report.l1_error_q12 = report.l1_error_q12.saturating_add(error);
    if error == 0 {
        report.exact_reads = report.exact_reads.saturating_add(1);
    } else {
        report.inexact_reads = report.inexact_reads.saturating_add(1);
    }
}

fn project_key_q12(key: u64) -> [i16; DKV_CONTROL_KEY_DIM] {
    let mut output = [0_i16; DKV_CONTROL_KEY_DIM];
    let mut state = mix64(key);
    for (index, component) in output.iter_mut().enumerate() {
        state = mix64(state ^ u64::try_from(index).expect("small fixed index"));
        *component = if state & 1 == 0 {
            DELTA_KV_ONE
        } else {
            -DELTA_KV_ONE
        };
    }
    output
}

fn maximum_direct_capacity(budget: u128, value_bits: usize) -> Result<usize, DkvControlError> {
    let mut capacity = 1_usize;
    loop {
        let next = capacity.checked_add(1).ok_or(DkvControlError::Overflow)?;
        let memory =
            BoundedAssociativeMemory::new(next, value_bits, ReplacementPolicy::RejectCollision)?;
        if memory.semantic_bits() > budget {
            break;
        }
        capacity = next;
    }
    Ok(capacity)
}

fn maximum_append_capacity(budget: u128, value_bits: usize) -> Result<usize, DkvControlError> {
    let mut capacity = 1_usize;
    loop {
        let next = capacity.checked_add(1).ok_or(DkvControlError::Overflow)?;
        let ledger = AppendLedger::new(next, value_bits)?;
        if ledger.semantic_bits() > budget {
            break;
        }
        capacity = next;
    }
    Ok(capacity)
}

fn encode_value(value: [i16; 2]) -> BooleanWorkspace {
    let mut bits = Vec::with_capacity(DKV_CONTROL_VALUE_BITS);
    for scalar in value {
        let raw = u16::from_le_bytes(scalar.to_le_bytes());
        for bit in 0..16 {
            bits.push(raw & (1_u16 << bit) != 0);
        }
    }
    BooleanWorkspace::from_bits(&bits)
}

fn decode_value(value: &BooleanWorkspace) -> Result<[i16; 2], DkvControlError> {
    if value.len() != DKV_CONTROL_VALUE_BITS {
        return Err(DkvControlError::ValueWidth);
    }
    let mut output = [0_i16; 2];
    for (scalar_index, scalar) in output.iter_mut().enumerate() {
        let mut raw = 0_u16;
        for bit in 0..16 {
            if value.get(scalar_index * 16 + bit).unwrap_or(false) {
                raw |= 1_u16 << bit;
            }
        }
        *scalar = i16::from_le_bytes(raw.to_le_bytes());
    }
    Ok(output)
}

fn l1_error_i32(prediction: &[i32], target: &[i16; 2]) -> Result<u64, DkvControlError> {
    if prediction.len() != 2 {
        return Err(DkvControlError::ValueWidth);
    }
    prediction
        .iter()
        .zip(target)
        .try_fold(0_u64, |sum, (prediction, target)| {
            let delta = i64::from(*prediction) - i64::from(*target);
            sum.checked_add(delta.unsigned_abs())
                .ok_or(DkvControlError::Overflow)
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LedgerRecord {
    key: u64,
    value_q12: [i16; 2],
    epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppendLedger {
    records: Vec<LedgerRecord>,
    capacity: usize,
    value_bits: usize,
    next_epoch: u64,
}

impl AppendLedger {
    fn new(capacity: usize, value_bits: usize) -> Result<Self, DkvControlError> {
        if capacity == 0 || value_bits != DKV_CONTROL_VALUE_BITS {
            return Err(DkvControlError::ValueWidth);
        }
        Ok(Self {
            records: Vec::new(),
            capacity,
            value_bits,
            next_epoch: 0,
        })
    }

    fn semantic_bits(&self) -> u128 {
        const GLOBAL_BITS: u128 = 64 + 64 + 32 + 64;
        let record_bits = 64_u128 + self.value_bits as u128 + 64;
        GLOBAL_BITS + self.capacity as u128 * record_bits
    }

    fn len(&self) -> usize {
        self.records.len()
    }

    fn append(&mut self, key: u64, value_q12: [i16; 2]) -> Result<bool, DkvControlError> {
        if self.records.len() == self.capacity {
            return Ok(false);
        }
        let epoch = self.next_epoch;
        self.next_epoch = self
            .next_epoch
            .checked_add(1)
            .ok_or(DkvControlError::Overflow)?;
        self.records.push(LedgerRecord {
            key,
            value_q12,
            epoch,
        });
        Ok(true)
    }

    fn read_latest(&self, key: u64) -> Option<([i16; 2], u64, u64)> {
        let mut comparisons = 0_u64;
        for record in self.records.iter().rev() {
            comparisons = comparisons.saturating_add(1);
            if record.key == key {
                return Some((record.value_q12, comparisons, record.epoch));
            }
        }
        None
    }
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DkvControlError {
    StorageBudgetInvariant,
    ValueWidth,
    Overflow,
    DeltaKv(DeltaKvError),
    Memory(MemoryError),
}

impl From<DeltaKvError> for DkvControlError {
    fn from(error: DeltaKvError) -> Self {
        Self::DeltaKv(error)
    }
}

impl From<MemoryError> for DkvControlError {
    fn from(error: MemoryError) -> Self {
        Self::Memory(error)
    }
}

impl fmt::Display for DkvControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DkvControlError {}
