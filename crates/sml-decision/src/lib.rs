#![forbid(unsafe_code)]

//! Native typed decisions over structured Boolean features, without text decoding.
//!
//! This is SML-D0 reference execution, not a natural-language decision model.
//! All heads share one acyclic circuit. Bit planes evaluate up to 64 records per
//! group. Unknown features remain unknown; no score is presented as a calibrated
//! probability. Proposals never authorize or execute external actions.

use core::{fmt, mem::size_of};

pub use sml_core::{circuit::SignalRef, train::TruthTable2};

pub mod training;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateSpec {
    pub left: SignalRef,
    pub right: SignalRef,
    pub table: TruthTable2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub id: u32,
    pub eligible: SignalRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadKind {
    Binary(SignalRef),
    /// Exactly one candidate must be known eligible. No hidden tie breaking.
    Choice(Vec<Candidate>),
    /// Little-endian output bits with an inclusive, explicitly declared maximum.
    Ordinal { bits: Vec<SignalRef>, maximum: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionHead {
    pub question_id: u64,
    /// Declared hard applicability constraint, not a learned confidence signal.
    pub applicable: SignalRef,
    pub kind: HeadKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputRecord {
    pub record_id: u64,
    pub schema_id: u64,
    pub values: u64,
    /// A clear bit means missing, never implicitly false.
    pub known: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionValue {
    Binary(bool),
    Choice(u32),
    Ordinal(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Abstention {
    MissingInformation,
    Inapplicable,
    NoEligibleCandidate,
    AmbiguousCandidates,
    OrdinalOutOfRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionOutcome {
    /// A proposal only. A separate execution policy must authorize any action.
    Proposed(DecisionValue),
    Abstain(Abstention),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uncertainty {
    /// Logical determinacy is not statistical calibration.
    Uncalibrated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionRecord {
    pub record_id: u64,
    pub question_id: u64,
    pub schema_id: u64,
    pub model_version: u64,
    pub outcome: DecisionOutcome,
    pub uncertainty: Uncertainty,
}

/// Constructor admission limits. Caller-owned input vectors are not allocated here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramLimits {
    pub max_gates: usize,
    pub max_heads: usize,
    pub max_head_signals: usize,
}

impl Default for ProgramLimits {
    fn default() -> Self {
        Self { max_gates: 4096, max_heads: 256, max_head_signals: 4096 }
    }
}

/// Per-call bounds, checked before scratch/result allocation or gate execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchBudget {
    pub max_records: usize,
    pub max_results: usize,
    pub max_result_bytes: usize,
    pub max_scratch_bytes: usize,
    pub max_logical_gate_evaluations: u64,
}

impl Default for BatchBudget {
    fn default() -> Self {
        Self {
            max_records: 4096,
            max_results: 65_536,
            max_result_bytes: 8 * 1024 * 1024,
            max_scratch_bytes: 1024 * 1024,
            max_logical_gate_evaluations: 16_777_216,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchPlan {
    pub records: usize,
    pub results: usize,
    /// Rust element payload only; not allocator overhead, RSS or caller input.
    pub result_bytes: usize,
    pub scratch_bytes: usize,
    pub logical_gate_evaluations: u64,
    pub word_gate_evaluations: u64,
    pub feature_extractions: u64,
    pub head_signal_read_bound: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchResult {
    /// Input-record order, then declared head order.
    pub decisions: Vec<DecisionRecord>,
    pub plan: BatchPlan,
    pub abstentions: u64,
    pub head_signal_reads: u64,
    pub text_tokens_generated: u64,
}

#[derive(Debug, Clone)]
pub struct DecisionProgram {
    schema_id: u64,
    model_version: u64,
    inputs: usize,
    gates: Vec<GateSpec>,
    heads: Vec<DecisionHead>,
    head_signals: usize,
}

impl DecisionProgram {
    pub fn compile(
        schema_id: u64,
        model_version: u64,
        inputs: usize,
        gates: Vec<GateSpec>,
        heads: Vec<DecisionHead>,
        limits: ProgramLimits,
    ) -> Result<Self, DecisionError> {
        if !(1..=64).contains(&inputs) || heads.is_empty() {
            return Err(DecisionError::InvalidShape);
        }
        if gates.len() > limits.max_gates || gates.len() > u16::MAX as usize
            || heads.len() > limits.max_heads || heads.len() > u16::MAX as usize
        {
            return Err(DecisionError::ProgramBudget);
        }
        for (index, gate) in gates.iter().enumerate() {
            validate_signal(gate.left, inputs, index)?;
            validate_signal(gate.right, inputs, index)?;
        }
        let mut head_signals = 0_usize;
        for (index, head) in heads.iter().enumerate() {
            if heads[..index].iter().any(|prior| prior.question_id == head.question_id) {
                return Err(DecisionError::DuplicateQuestion);
            }
            validate_signal(head.applicable, inputs, gates.len())?;
            let count = match &head.kind {
                HeadKind::Binary(signal) => {
                    validate_signal(*signal, inputs, gates.len())?;
                    1
                }
                HeadKind::Choice(candidates) => {
                    if candidates.is_empty() || candidates.len() > limits.max_head_signals {
                        return Err(DecisionError::InvalidShape);
                    }
                    for (position, candidate) in candidates.iter().enumerate() {
                        validate_signal(candidate.eligible, inputs, gates.len())?;
                        if candidates[..position].iter().any(|prior| prior.id == candidate.id) {
                            return Err(DecisionError::DuplicateCandidate);
                        }
                    }
                    candidates.len()
                }
                HeadKind::Ordinal { bits, maximum } => {
                    if bits.is_empty() || bits.len() > 16 || u32::from(*maximum) >= (1_u32 << bits.len()) {
                        return Err(DecisionError::InvalidShape);
                    }
                    for bit in bits { validate_signal(*bit, inputs, gates.len())?; }
                    bits.len()
                }
            };
            head_signals = head_signals.checked_add(count + 1).ok_or(DecisionError::Overflow)?;
            if head_signals > limits.max_head_signals { return Err(DecisionError::ProgramBudget); }
        }
        Ok(Self { schema_id, model_version, inputs, gates, heads, head_signals })
    }

    /// Stored truth-table bits are not automatically counted as learned parameters.
    pub fn table_storage_bits(&self) -> u64 { self.gates.len() as u64 * 4 }

    pub fn plan(&self, records: usize, budget: BatchBudget) -> Result<BatchPlan, DecisionError> {
        let results = records.checked_mul(self.heads.len()).ok_or(DecisionError::Overflow)?;
        let result_bytes = results.checked_mul(size_of::<DecisionRecord>()).ok_or(DecisionError::Overflow)?;
        let scratch_bytes = if records == 0 { 0 } else {
            (self.inputs + self.gates.len()).checked_mul(size_of::<Plane>()).ok_or(DecisionError::Overflow)?
        };
        let logical_gate_evaluations = product(records, self.gates.len())?;
        let word_gate_evaluations = product(records.div_ceil(64), self.gates.len())?;
        let plan = BatchPlan {
            records, results, result_bytes, scratch_bytes, logical_gate_evaluations,
            word_gate_evaluations, feature_extractions: product(records, self.inputs)?,
            head_signal_read_bound: product(records, self.head_signals)?,
        };
        if records > budget.max_records || results > budget.max_results
            || result_bytes > budget.max_result_bytes || scratch_bytes > budget.max_scratch_bytes
            || logical_gate_evaluations > budget.max_logical_gate_evaluations
        { return Err(DecisionError::BatchBudget); }
        Ok(plan)
    }

    pub fn evaluate(&self, records: &[InputRecord], budget: BatchBudget) -> Result<BatchResult, DecisionError> {
        let plan = self.plan(records.len(), budget)?;
        let input_mask = lane_mask(self.inputs);
        // Reject the whole batch before any gate/output work. Unknown payload bits
        // and undeclared feature bits are not silently interpreted as evidence.
        for record in records {
            if record.schema_id != self.schema_id { return Err(DecisionError::SchemaMismatch); }
            if record.known & !input_mask != 0 || record.values & !record.known != 0 {
                return Err(DecisionError::InvalidRecord);
            }
        }
        let mut decisions = Vec::new();
        decisions.try_reserve_exact(plan.results).map_err(|_| DecisionError::Allocation)?;
        let mut planes = Vec::new();
        if !records.is_empty() {
            planes.try_reserve_exact(self.inputs + self.gates.len()).map_err(|_| DecisionError::Allocation)?;
            planes.resize(self.inputs + self.gates.len(), Plane::default());
        }
        let mut abstentions = 0;
        let mut head_signal_reads = 0;
        for group in records.chunks(64) {
            let live = lane_mask(group.len());
            planes.fill(Plane::default());
            for (lane, record) in group.iter().enumerate() {
                for (feature, plane) in planes[..self.inputs].iter_mut().enumerate() {
                    plane.known |= ((record.known >> feature) & 1) << lane;
                    plane.values |= ((record.values >> feature) & 1) << lane;
                }
            }
            for (index, gate) in self.gates.iter().enumerate() {
                let left = planes[signal_index(gate.left, self.inputs)];
                let right = planes[signal_index(gate.right, self.inputs)];
                planes[self.inputs + index] = gate_plane(*gate, left, right, live);
            }
            for (lane, record) in group.iter().enumerate() {
                for head in &self.heads {
                    let outcome = resolve_head(head, |signal| {
                        head_signal_reads += 1;
                        let plane = planes[signal_index(signal, self.inputs)];
                        if plane.known & (1_u64 << lane) == 0 { None }
                        else { Some(plane.values & (1_u64 << lane) != 0) }
                    });
                    if matches!(outcome, DecisionOutcome::Abstain(_)) { abstentions += 1; }
                    decisions.push(DecisionRecord {
                        record_id: record.record_id, question_id: head.question_id,
                        schema_id: self.schema_id, model_version: self.model_version,
                        outcome, uncertainty: Uncertainty::Uncalibrated,
                    });
                }
            }
        }
        Ok(BatchResult { decisions, plan, abstentions, head_signal_reads, text_tokens_generated: 0 })
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Plane { values: u64, known: u64 }

fn lane_mask(lanes: usize) -> u64 {
    if lanes == 64 { u64::MAX } else { (1_u64 << lanes) - 1 }
}

fn product(left: usize, right: usize) -> Result<u64, DecisionError> {
    let result = (left as u128).checked_mul(right as u128).ok_or(DecisionError::Overflow)?;
    u64::try_from(result).map_err(|_| DecisionError::Overflow)
}

fn validate_signal(signal: SignalRef, inputs: usize, earlier_gates: usize) -> Result<(), DecisionError> {
    match signal {
        SignalRef::Input(index) if (index as usize) < inputs => Ok(()),
        SignalRef::Gate(index) if (index as usize) < earlier_gates => Ok(()),
        _ => Err(DecisionError::InvalidSignal),
    }
}

fn signal_index(signal: SignalRef, inputs: usize) -> usize {
    match signal { SignalRef::Input(index) => index as usize, SignalRef::Gate(index) => inputs + index as usize }
}

fn gate_plane(gate: GateSpec, left: Plane, right: Plane, live: u64) -> Plane {
    let mut yes = 0_u64;
    let mut no = 0_u64;
    for row in 0..4 {
        // Preserve immediate aliasing (e.g. XOR(x,x)). Wider reconvergent
        // correlations are conservative: extra abstention, not invented evidence.
        if gate.left == gate.right && (row == 1 || row == 2) { continue; }
        let l = if row & 1 != 0 { left.values | !left.known } else { !left.values };
        let r = if row & 2 != 0 { right.values | !right.known } else { !right.values };
        let possible = l & r & live;
        if gate.table.code() & (1_u8 << row) != 0 { yes |= possible; } else { no |= possible; }
    }
    let known = live & !(yes & no);
    Plane { values: yes & known, known }
}

fn resolve_head(head: &DecisionHead, mut read: impl FnMut(SignalRef) -> Option<bool>) -> DecisionOutcome {
    use Abstention as A;
    use DecisionOutcome::{Abstain, Proposed};
    match read(head.applicable) {
        Some(true) => {},
        Some(false) => return Abstain(A::Inapplicable),
        None => return Abstain(A::MissingInformation),
    }
    match &head.kind {
        HeadKind::Binary(signal) => match read(*signal) {
            Some(value) => Proposed(DecisionValue::Binary(value)),
            None => Abstain(A::MissingInformation),
        },
        HeadKind::Choice(candidates) => {
            let mut selected = None;
            let mut missing = false;
            let mut multiple = false;
            for candidate in candidates {
                match read(candidate.eligible) {
                    Some(true) => {
                        if selected.is_some() { multiple = true; }
                        selected = Some(candidate.id);
                    }
                    Some(false) => {},
                    None => missing = true,
                }
            }
            if multiple { Abstain(A::AmbiguousCandidates) }
            else if missing { Abstain(A::MissingInformation) }
            else if let Some(id) = selected { Proposed(DecisionValue::Choice(id)) }
            else { Abstain(A::NoEligibleCandidate) }
        }
        HeadKind::Ordinal { bits, maximum } => {
            let mut value = 0_u16;
            for (index, signal) in bits.iter().enumerate() {
                match read(*signal) {
                    Some(true) => value |= 1_u16 << index,
                    Some(false) => {},
                    None => return Abstain(A::MissingInformation),
                }
            }
            if value > *maximum { Abstain(A::OrdinalOutOfRange) }
            else { Proposed(DecisionValue::Ordinal(value)) }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionError {
    InvalidShape, ProgramBudget, DuplicateQuestion, DuplicateCandidate, InvalidSignal,
    Overflow, BatchBudget, SchemaMismatch, InvalidRecord, Allocation,
}

impl fmt::Display for DecisionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{self:?}") }
}
impl std::error::Error for DecisionError {}
