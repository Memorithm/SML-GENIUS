//! CONTEXT-2 bounded recurrent working-context substrate.
//!
//! Deterministic reference comparing three matched policies on identical
//! synthetic segment streams:
//!
//! - [`WorkingPolicy::Fixed`]: constant-capacity ring; overwrite of a resident
//!   slot marks the prior segment **Lost** (irreversible from working state).
//! - [`WorkingPolicy::Expanding`]: retains every admitted segment until a
//!   declared hard ceiling; never evicts below that ceiling.
//! - [`WorkingPolicy::CapacityBounded`]: FIFO capacity with explicit **Evicted**
//!   accounting and retained raw segment-ID provenance.
//!
//! This is not an effective context window and not a quality measurement. No
//! silent retention: every segment fate is [`RetentionFate`]. Restart restores
//! only from a complete, validated snapshot; truncated or unknown provenance
//! fails closed. Capacity zero and duplicate segment IDs fail closed.
//!
//! Dual-track note: CCOS recall is not a 1M attention claim. CONTEXT-2 owns a
//! bounded working substrate only.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};

/// Schema version frozen by this CONTEXT-2 substrate.
pub const WORKING_CONTEXT_SCHEMA_VERSION: u16 = 1;

/// Matched working-context policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingPolicy {
    /// Fixed ring. Overwrites mark prior residents Lost.
    Fixed { capacity: usize },
    /// Grows with admissions until `hard_max` (inclusive ceiling of retained).
    Expanding { hard_max: usize },
    /// FIFO capacity with explicit Evicted accounting.
    CapacityBounded { capacity: usize },
}

impl WorkingPolicy {
    /// Returns the binding capacity/ceiling, or an error when zero/invalid.
    pub fn validate(self) -> Result<Self, WorkingContextError> {
        let limit = match self {
            Self::Fixed { capacity } | Self::CapacityBounded { capacity } => capacity,
            Self::Expanding { hard_max } => hard_max,
        };
        if limit == 0 {
            return Err(WorkingContextError::InvalidCapacity);
        }
        Ok(self)
    }

    #[must_use]
    pub const fn kind_name(self) -> &'static str {
        match self {
            Self::Fixed { .. } => "fixed",
            Self::Expanding { .. } => "expanding",
            Self::CapacityBounded { .. } => "capacity_bounded",
        }
    }

    #[must_use]
    pub const fn limit(self) -> usize {
        match self {
            Self::Fixed { capacity } | Self::CapacityBounded { capacity } => capacity,
            Self::Expanding { hard_max } => hard_max,
        }
    }
}

/// One synthetic (or caller-supplied) segment offered to the working substrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingSegment {
    pub identity: ContextSegmentId,
    pub order_index: u64,
    pub content_hash: ContextContentHash,
    /// Deterministic fingerprint folded into the recurrent register.
    pub fingerprint: u64,
}

impl WorkingSegment {
    pub fn new(
        identity: ContextSegmentId,
        order_index: u64,
        content_hash: ContextContentHash,
        fingerprint: u64,
    ) -> Result<Self, WorkingContextError> {
        if fingerprint == 0 {
            return Err(WorkingContextError::ZeroFingerprint);
        }
        Ok(Self {
            identity,
            order_index,
            content_hash,
            fingerprint,
        })
    }
}

/// Explicit fate of one segment relative to working state. No silent retention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionFate {
    /// Present in the current working ring/list.
    Retained,
    /// Removed by CapacityBounded FIFO; provenance kept in the eviction log.
    Evicted,
    /// Overwritten under Fixed policy; not recoverable from working state.
    Lost,
}

/// Per-segment accounting row with raw segment-ID provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentAccounting {
    pub identity: ContextSegmentId,
    pub order_index: u64,
    pub content_hash: ContextContentHash,
    pub fingerprint: u64,
    pub fate: RetentionFate,
    /// Monotonic admission step when the segment first entered working state.
    pub admitted_step: u64,
    /// Step at which fate became Evicted or Lost; None while Retained.
    pub resolved_step: Option<u64>,
}

/// Cumulative counters. Invariant: admitted == retained + evicted + lost
/// after every successful admission (expanding never increments evicted/lost
/// until hard_max refusal — refusal is an error, not a fate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WorkingContextStats {
    pub admitted: u64,
    pub retained: u64,
    pub evicted: u64,
    pub lost: u64,
    pub recurrent_folds: u64,
}

/// One resident slot in working state.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Resident {
    identity: ContextSegmentId,
    order_index: u64,
    content_hash: ContextContentHash,
    fingerprint: u64,
    admitted_step: u64,
}

/// Restartable snapshot. Must carry complete retained provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingContextSnapshot {
    pub schema_version: u16,
    pub stream_id: ContextStreamId,
    pub policy: WorkingPolicy,
    pub step: u64,
    pub recurrent_register: u64,
    pub retained: Vec<WorkingSegmentProvenance>,
    pub eviction_log: Vec<WorkingSegmentProvenance>,
    pub stats: WorkingContextStats,
    /// Every segment_id ever admitted (collision detection across restart).
    pub seen_segment_ids: Vec<u64>,
}

/// Immutable provenance row for restart / oracle comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingSegmentProvenance {
    pub identity: ContextSegmentId,
    pub order_index: u64,
    pub content_hash: ContextContentHash,
    pub fingerprint: u64,
    pub admitted_step: u64,
    pub resolved_step: Option<u64>,
    pub fate: RetentionFate,
}

/// Bounded recurrent working context for one stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingContext {
    stream_id: ContextStreamId,
    policy: WorkingPolicy,
    step: u64,
    recurrent_register: u64,
    residents: VecDeque<Resident>,
    /// segment_id -> accounting (complete history for this run).
    ledger: BTreeMap<u64, SegmentAccounting>,
    seen: BTreeSet<u64>,
    stats: WorkingContextStats,
}

impl WorkingContext {
    pub fn new(
        stream_id: ContextStreamId,
        policy: WorkingPolicy,
    ) -> Result<Self, WorkingContextError> {
        let policy = policy.validate()?;
        Ok(Self {
            stream_id,
            policy,
            step: 0,
            recurrent_register: 0,
            residents: VecDeque::new(),
            ledger: BTreeMap::new(),
            seen: BTreeSet::new(),
            stats: WorkingContextStats::default(),
        })
    }

    #[must_use]
    pub const fn stream_id(&self) -> ContextStreamId {
        self.stream_id
    }

    #[must_use]
    pub const fn policy(&self) -> WorkingPolicy {
        self.policy
    }

    #[must_use]
    pub const fn step(&self) -> u64 {
        self.step
    }

    #[must_use]
    pub const fn recurrent_register(&self) -> u64 {
        self.recurrent_register
    }

    #[must_use]
    pub const fn stats(&self) -> WorkingContextStats {
        self.stats
    }

    #[must_use]
    pub fn retained_count(&self) -> usize {
        self.residents.len()
    }

    /// Current retained segment identities in FIFO order (oldest first).
    #[must_use]
    pub fn retained_identities(&self) -> Vec<ContextSegmentId> {
        self.residents.iter().map(|r| r.identity).collect()
    }

    /// Complete per-segment accounting, ordered by segment_id.
    #[must_use]
    pub fn accounting(&self) -> Vec<SegmentAccounting> {
        self.ledger.values().cloned().collect()
    }

    /// Admit one segment. Fail-closed on stream mismatch, ID collision,
    /// expanding hard-max overflow, or invalid segment.
    ///
    /// `WorkingSegment` fields are public, so a caller can bypass
    /// [`WorkingSegment::new`]; the zero-fingerprint contract is therefore
    /// re-checked here **before any mutation** (seen set, step, residents).
    pub fn admit(&mut self, segment: WorkingSegment) -> Result<RetentionFate, WorkingContextError> {
        if segment.fingerprint == 0 {
            return Err(WorkingContextError::ZeroFingerprint);
        }
        if segment.identity.stream_id != self.stream_id {
            return Err(WorkingContextError::StreamMismatch {
                expected: self.stream_id,
                actual: segment.identity.stream_id,
            });
        }
        if !self.seen.insert(segment.identity.segment_id) {
            return Err(WorkingContextError::SegmentIdCollision(segment.identity));
        }

        self.step = self
            .step
            .checked_add(1)
            .ok_or(WorkingContextError::StepOverflow)?;
        let admitted_step = self.step;

        match self.policy {
            WorkingPolicy::Fixed { capacity } => self.admit_fixed(segment, capacity, admitted_step),
            WorkingPolicy::Expanding { hard_max } => {
                self.admit_expanding(segment, hard_max, admitted_step)
            }
            WorkingPolicy::CapacityBounded { capacity } => {
                self.admit_capacity_bounded(segment, capacity, admitted_step)
            }
        }
    }

    fn admit_fixed(
        &mut self,
        segment: WorkingSegment,
        capacity: usize,
        admitted_step: u64,
    ) -> Result<RetentionFate, WorkingContextError> {
        if self.residents.len() == capacity {
            let victim = self
                .residents
                .pop_front()
                .ok_or(WorkingContextError::InternalInvariant)?;
            self.mark_fate(
                victim.identity.segment_id,
                RetentionFate::Lost,
                admitted_step,
            )?;
            self.stats.lost = self.stats.lost.saturating_add(1);
            self.stats.retained = self.stats.retained.saturating_sub(1);
            // Remove victim contribution from the recurrent register.
            self.recurrent_register ^= victim.fingerprint;
        }
        self.push_resident(segment, admitted_step)?;
        Ok(RetentionFate::Retained)
    }

    fn admit_expanding(
        &mut self,
        segment: WorkingSegment,
        hard_max: usize,
        admitted_step: u64,
    ) -> Result<RetentionFate, WorkingContextError> {
        if self.residents.len() >= hard_max {
            // Roll back the seen insert and step so the caller can retry after
            // a policy change; fail closed rather than silent drop.
            self.seen.remove(&segment.identity.segment_id);
            self.step = self.step.saturating_sub(1);
            return Err(WorkingContextError::ExpandingCeiling {
                hard_max,
                retained: self.residents.len(),
            });
        }
        self.push_resident(segment, admitted_step)?;
        Ok(RetentionFate::Retained)
    }

    fn admit_capacity_bounded(
        &mut self,
        segment: WorkingSegment,
        capacity: usize,
        admitted_step: u64,
    ) -> Result<RetentionFate, WorkingContextError> {
        if self.residents.len() == capacity {
            let victim = self
                .residents
                .pop_front()
                .ok_or(WorkingContextError::InternalInvariant)?;
            self.mark_fate(
                victim.identity.segment_id,
                RetentionFate::Evicted,
                admitted_step,
            )?;
            self.stats.evicted = self.stats.evicted.saturating_add(1);
            self.stats.retained = self.stats.retained.saturating_sub(1);
            self.recurrent_register ^= victim.fingerprint;
        }
        self.push_resident(segment, admitted_step)?;
        Ok(RetentionFate::Retained)
    }

    fn push_resident(
        &mut self,
        segment: WorkingSegment,
        admitted_step: u64,
    ) -> Result<(), WorkingContextError> {
        self.residents.push_back(Resident {
            identity: segment.identity,
            order_index: segment.order_index,
            content_hash: segment.content_hash,
            fingerprint: segment.fingerprint,
            admitted_step,
        });
        self.recurrent_register ^= segment.fingerprint;
        self.stats.recurrent_folds = self.stats.recurrent_folds.saturating_add(1);
        self.stats.admitted = self.stats.admitted.saturating_add(1);
        self.stats.retained = self.stats.retained.saturating_add(1);
        self.ledger.insert(
            segment.identity.segment_id,
            SegmentAccounting {
                identity: segment.identity,
                order_index: segment.order_index,
                content_hash: segment.content_hash,
                fingerprint: segment.fingerprint,
                fate: RetentionFate::Retained,
                admitted_step,
                resolved_step: None,
            },
        );
        self.check_stats_invariant()?;
        Ok(())
    }

    fn mark_fate(
        &mut self,
        segment_id: u64,
        fate: RetentionFate,
        resolved_step: u64,
    ) -> Result<(), WorkingContextError> {
        let row = self
            .ledger
            .get_mut(&segment_id)
            .ok_or(WorkingContextError::UnknownProvenance { segment_id })?;
        if row.fate != RetentionFate::Retained {
            return Err(WorkingContextError::InternalInvariant);
        }
        row.fate = fate;
        row.resolved_step = Some(resolved_step);
        Ok(())
    }

    fn check_stats_invariant(&self) -> Result<(), WorkingContextError> {
        let sum = self
            .stats
            .retained
            .checked_add(self.stats.evicted)
            .and_then(|v| v.checked_add(self.stats.lost))
            .ok_or(WorkingContextError::InternalInvariant)?;
        if sum != self.stats.admitted {
            return Err(WorkingContextError::InternalInvariant);
        }
        if self.stats.retained as usize != self.residents.len() {
            return Err(WorkingContextError::InternalInvariant);
        }
        Ok(())
    }

    /// Export a complete restart snapshot.
    #[must_use]
    pub fn snapshot(&self) -> WorkingContextSnapshot {
        let mut retained = Vec::with_capacity(self.residents.len());
        for r in &self.residents {
            let resolved_step = self
                .ledger
                .get(&r.identity.segment_id)
                .map(|row| row.resolved_step)
                .unwrap_or(None);
            retained.push(WorkingSegmentProvenance {
                identity: r.identity,
                order_index: r.order_index,
                content_hash: r.content_hash,
                fingerprint: r.fingerprint,
                admitted_step: r.admitted_step,
                resolved_step,
                fate: RetentionFate::Retained,
            });
        }
        let eviction_log: Vec<WorkingSegmentProvenance> = self
            .ledger
            .values()
            .filter(|row| matches!(row.fate, RetentionFate::Evicted | RetentionFate::Lost))
            .map(|row| WorkingSegmentProvenance {
                identity: row.identity,
                order_index: row.order_index,
                content_hash: row.content_hash,
                fingerprint: row.fingerprint,
                admitted_step: row.admitted_step,
                resolved_step: row.resolved_step,
                fate: row.fate,
            })
            .collect();
        WorkingContextSnapshot {
            schema_version: WORKING_CONTEXT_SCHEMA_VERSION,
            stream_id: self.stream_id,
            policy: self.policy,
            step: self.step,
            recurrent_register: self.recurrent_register,
            retained,
            eviction_log,
            stats: self.stats,
            seen_segment_ids: self.seen.iter().copied().collect(),
        }
    }

    /// Restore from a snapshot. Fail-closed on schema, capacity, truncated
    /// retained provenance, unknown fate rows, zero segment id, register
    /// mismatch, or stats mismatch (including counter overflow and
    /// `recurrent_folds != admitted`).
    ///
    /// Pure: builds a new context or returns `Err`; no caller state is touched,
    /// so a rejected restore can never leave partial live state.
    pub fn restore(snapshot: &WorkingContextSnapshot) -> Result<Self, WorkingContextError> {
        if snapshot.schema_version != WORKING_CONTEXT_SCHEMA_VERSION {
            return Err(WorkingContextError::UnsupportedSchema {
                got: snapshot.schema_version,
            });
        }
        let policy = snapshot.policy.validate()?;
        if snapshot.retained.len() > policy.limit() {
            return Err(WorkingContextError::TruncatedProvenance {
                reason: "retained exceeds policy limit",
            });
        }
        // G3: ContextSegmentId fields are public, so forged rows may carry the
        // zero segment id that ContextSegmentId::new refuses.
        for row in snapshot.retained.iter().chain(snapshot.eviction_log.iter()) {
            if row.identity.segment_id == 0 {
                return Err(WorkingContextError::InvalidSegmentIdentity);
            }
        }
        // Every retained row must declare Retained fate and complete fields.
        for row in &snapshot.retained {
            if row.fate != RetentionFate::Retained {
                return Err(WorkingContextError::TruncatedProvenance {
                    reason: "retained row has non-retained fate",
                });
            }
            if row.identity.stream_id != snapshot.stream_id {
                return Err(WorkingContextError::StreamMismatch {
                    expected: snapshot.stream_id,
                    actual: row.identity.stream_id,
                });
            }
            if row.fingerprint == 0 {
                return Err(WorkingContextError::ZeroFingerprint);
            }
            if row.resolved_step.is_some() {
                return Err(WorkingContextError::TruncatedProvenance {
                    reason: "retained row has resolved_step",
                });
            }
        }
        for row in &snapshot.eviction_log {
            if row.fingerprint == 0 {
                return Err(WorkingContextError::ZeroFingerprint);
            }
            if !matches!(row.fate, RetentionFate::Evicted | RetentionFate::Lost) {
                return Err(WorkingContextError::TruncatedProvenance {
                    reason: "eviction_log row not Evicted/Lost",
                });
            }
            if row.resolved_step.is_none() {
                return Err(WorkingContextError::TruncatedProvenance {
                    reason: "eviction_log missing resolved_step",
                });
            }
            if row.identity.stream_id != snapshot.stream_id {
                return Err(WorkingContextError::StreamMismatch {
                    expected: snapshot.stream_id,
                    actual: row.identity.stream_id,
                });
            }
        }

        let mut seen = BTreeSet::new();
        for id in &snapshot.seen_segment_ids {
            if !seen.insert(*id) {
                return Err(WorkingContextError::SegmentIdCollision(ContextSegmentId {
                    stream_id: snapshot.stream_id,
                    segment_id: *id,
                }));
            }
        }
        // Provenance must be a subset of seen IDs.
        for row in snapshot.retained.iter().chain(snapshot.eviction_log.iter()) {
            if !seen.contains(&row.identity.segment_id) {
                return Err(WorkingContextError::UnknownProvenance {
                    segment_id: row.identity.segment_id,
                });
            }
        }
        // Every seen ID must appear in retained or eviction_log (no silent drop).
        let mut covered: BTreeSet<u64> = BTreeSet::new();
        for row in snapshot.retained.iter().chain(snapshot.eviction_log.iter()) {
            if !covered.insert(row.identity.segment_id) {
                return Err(WorkingContextError::SegmentIdCollision(row.identity));
            }
        }
        if covered != seen {
            return Err(WorkingContextError::TruncatedProvenance {
                reason: "seen_segment_ids not fully covered by retained+eviction_log",
            });
        }

        validate_chronology(snapshot, policy)?;
        validate_snapshot_stats(snapshot)?;

        let mut register = 0_u64;
        let mut residents = VecDeque::new();
        let mut ledger = BTreeMap::new();
        for row in &snapshot.retained {
            register ^= row.fingerprint;
            residents.push_back(Resident {
                identity: row.identity,
                order_index: row.order_index,
                content_hash: row.content_hash,
                fingerprint: row.fingerprint,
                admitted_step: row.admitted_step,
            });
            ledger.insert(
                row.identity.segment_id,
                SegmentAccounting {
                    identity: row.identity,
                    order_index: row.order_index,
                    content_hash: row.content_hash,
                    fingerprint: row.fingerprint,
                    fate: RetentionFate::Retained,
                    admitted_step: row.admitted_step,
                    resolved_step: None,
                },
            );
        }
        for row in &snapshot.eviction_log {
            ledger.insert(
                row.identity.segment_id,
                SegmentAccounting {
                    identity: row.identity,
                    order_index: row.order_index,
                    content_hash: row.content_hash,
                    fingerprint: row.fingerprint,
                    fate: row.fate,
                    admitted_step: row.admitted_step,
                    resolved_step: row.resolved_step,
                },
            );
        }
        if register != snapshot.recurrent_register {
            return Err(WorkingContextError::RegisterMismatch {
                expected: snapshot.recurrent_register,
                actual: register,
            });
        }

        let ctx = Self {
            stream_id: snapshot.stream_id,
            policy,
            step: snapshot.step,
            recurrent_register: register,
            residents,
            ledger,
            seen,
            stats: snapshot.stats,
        };
        ctx.check_stats_invariant()?;
        // Stats retained/evicted/lost must match ledger fates.
        let mut retained = 0_u64;
        let mut evicted = 0_u64;
        let mut lost = 0_u64;
        for row in ctx.ledger.values() {
            match row.fate {
                RetentionFate::Retained => retained += 1,
                RetentionFate::Evicted => evicted += 1,
                RetentionFate::Lost => lost += 1,
            }
        }
        if retained != ctx.stats.retained
            || evicted != ctx.stats.evicted
            || lost != ctx.stats.lost
            || retained + evicted + lost != ctx.stats.admitted
        {
            return Err(WorkingContextError::StatsMismatch);
        }
        Ok(ctx)
    }
}

/// Validate snapshot counters before they reach the live invariant checker.
///
/// Corrupt counters (including values whose sum overflows `u64`) are snapshot
/// corruption and map to [`WorkingContextError::StatsMismatch`], never to
/// [`WorkingContextError::InternalInvariant`]. Every successful admission
/// folds exactly once, so `recurrent_folds == admitted` on any honest
/// snapshot (G3, RemoteOps#1208).
fn validate_snapshot_stats(snapshot: &WorkingContextSnapshot) -> Result<(), WorkingContextError> {
    let stats = snapshot.stats;
    let sum = stats
        .retained
        .checked_add(stats.evicted)
        .and_then(|v| v.checked_add(stats.lost))
        .ok_or(WorkingContextError::StatsMismatch)?;
    if sum != stats.admitted || stats.recurrent_folds != stats.admitted {
        return Err(WorkingContextError::StatsMismatch);
    }
    if u64::try_from(snapshot.retained.len()).ok() != Some(stats.retained) {
        return Err(WorkingContextError::StatsMismatch);
    }
    Ok(())
}

/// Validate snapshot step / admission chronology and retained FIFO order.
///
/// Every successful admission increments `step` by exactly one and assigns
/// that step as `admitted_step`; a refused Expanding admission rolls the step
/// back. Therefore a consistent snapshot satisfies:
///
/// - `step == stats.admitted`;
/// - the admission steps of retained ∪ eviction_log are exactly `1..=step`
///   (no duplicates, no gaps);
/// - retained rows are listed in strictly increasing `admitted_step` (FIFO,
///   oldest first) and are the most recent admissions;
/// - every resolved row has `admitted_step < resolved_step <= step`; under
///   FIFO policies the victim admitted at `a` is resolved at exactly
///   `a + capacity` with the policy's fate (Fixed → Lost, CapacityBounded →
///   Evicted); Expanding never resolves a row.
///
/// The retained order is **rejected** (not reconstructed) when inconsistent so
/// the supplied FIFO order can never silently drive the next eviction.
fn validate_chronology(
    snapshot: &WorkingContextSnapshot,
    policy: WorkingPolicy,
) -> Result<(), WorkingContextError> {
    let step = snapshot.step;
    if step != snapshot.stats.admitted {
        return Err(WorkingContextError::InconsistentStepChronology {
            reason: "step != stats.admitted",
        });
    }
    let total = snapshot.retained.len() + snapshot.eviction_log.len();
    if u64::try_from(total).ok() != Some(step) {
        return Err(WorkingContextError::InconsistentStepChronology {
            reason: "provenance row count != step",
        });
    }

    // Retained FIFO order: strictly increasing admitted_step.
    for pair in snapshot.retained.windows(2) {
        if pair[0].admitted_step >= pair[1].admitted_step {
            return Err(WorkingContextError::InconsistentFifoOrder);
        }
    }

    let mut steps: BTreeSet<u64> = BTreeSet::new();
    for row in snapshot.retained.iter().chain(snapshot.eviction_log.iter()) {
        if row.admitted_step == 0 || row.admitted_step > step {
            return Err(WorkingContextError::InconsistentStepChronology {
                reason: "admitted_step outside 1..=step",
            });
        }
        if !steps.insert(row.admitted_step) {
            return Err(WorkingContextError::InconsistentStepChronology {
                reason: "duplicate admitted_step",
            });
        }
    }
    // Distinct values in 1..=step with count == step => exactly 1..=step.

    // Retained rows must be the most recent admissions.
    let retained_len = snapshot.retained.len() as u64;
    if let Some(first) = snapshot.retained.first() {
        if first.admitted_step != step - retained_len + 1 {
            return Err(WorkingContextError::InconsistentStepChronology {
                reason: "retained rows are not the most recent admissions",
            });
        }
    }

    for row in &snapshot.eviction_log {
        let resolved = row
            .resolved_step
            .ok_or(WorkingContextError::TruncatedProvenance {
                reason: "eviction_log missing resolved_step",
            })?;
        if resolved <= row.admitted_step || resolved > step {
            return Err(WorkingContextError::InconsistentStepChronology {
                reason: "resolved_step outside admitted_step+1..=step",
            });
        }
        let (capacity, expected_fate) = match policy {
            WorkingPolicy::Fixed { capacity } => (capacity, RetentionFate::Lost),
            WorkingPolicy::CapacityBounded { capacity } => (capacity, RetentionFate::Evicted),
            WorkingPolicy::Expanding { .. } => {
                return Err(WorkingContextError::InconsistentStepChronology {
                    reason: "expanding policy never resolves a row",
                });
            }
        };
        if row.fate != expected_fate {
            return Err(WorkingContextError::InconsistentStepChronology {
                reason: "eviction fate does not match policy",
            });
        }
        let expected_resolved = row
            .admitted_step
            .checked_add(capacity as u64)
            .ok_or(WorkingContextError::StepOverflow)?;
        if resolved != expected_resolved {
            return Err(WorkingContextError::InconsistentStepChronology {
                reason: "resolved_step != admitted_step + capacity",
            });
        }
    }
    Ok(())
}

/// Exact oracle for a synthetic stream under one policy.
///
/// Recomputes expected retained IDs, fates and the XOR recurrent register
/// independently of [`WorkingContext`] so tests can detect drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingContextOracle {
    pub policy: WorkingPolicy,
    pub retained_ids: Vec<u64>,
    pub fates: BTreeMap<u64, RetentionFate>,
    pub recurrent_register: u64,
    pub stats: WorkingContextStats,
}

impl WorkingContextOracle {
    /// Evaluate the oracle over an ordered segment stream (already validated).
    pub fn evaluate(
        policy: WorkingPolicy,
        segments: &[WorkingSegment],
    ) -> Result<Self, WorkingContextError> {
        let policy = policy.validate()?;
        let mut retained: VecDeque<&WorkingSegment> = VecDeque::new();
        let mut fates: BTreeMap<u64, RetentionFate> = BTreeMap::new();
        let mut seen: BTreeSet<u64> = BTreeSet::new();
        let mut stats = WorkingContextStats::default();
        let mut register = 0_u64;

        for segment in segments {
            if !seen.insert(segment.identity.segment_id) {
                return Err(WorkingContextError::SegmentIdCollision(segment.identity));
            }
            match policy {
                WorkingPolicy::Fixed { capacity } => {
                    if retained.len() == capacity {
                        let victim = retained
                            .pop_front()
                            .ok_or(WorkingContextError::InternalInvariant)?;
                        fates.insert(victim.identity.segment_id, RetentionFate::Lost);
                        stats.lost = stats.lost.saturating_add(1);
                        stats.retained = stats.retained.saturating_sub(1);
                        register ^= victim.fingerprint;
                    }
                }
                WorkingPolicy::Expanding { hard_max } => {
                    if retained.len() >= hard_max {
                        return Err(WorkingContextError::ExpandingCeiling {
                            hard_max,
                            retained: retained.len(),
                        });
                    }
                }
                WorkingPolicy::CapacityBounded { capacity } => {
                    if retained.len() == capacity {
                        let victim = retained
                            .pop_front()
                            .ok_or(WorkingContextError::InternalInvariant)?;
                        fates.insert(victim.identity.segment_id, RetentionFate::Evicted);
                        stats.evicted = stats.evicted.saturating_add(1);
                        stats.retained = stats.retained.saturating_sub(1);
                        register ^= victim.fingerprint;
                    }
                }
            }
            retained.push_back(segment);
            register ^= segment.fingerprint;
            stats.recurrent_folds = stats.recurrent_folds.saturating_add(1);
            stats.admitted = stats.admitted.saturating_add(1);
            stats.retained = stats.retained.saturating_add(1);
            fates.insert(segment.identity.segment_id, RetentionFate::Retained);
        }

        // Final fates: retained override any prior? No — victims already set.
        // Re-mark currently retained.
        for seg in &retained {
            fates.insert(seg.identity.segment_id, RetentionFate::Retained);
        }

        Ok(Self {
            policy,
            retained_ids: retained.iter().map(|s| s.identity.segment_id).collect(),
            fates,
            recurrent_register: register,
            stats,
        })
    }
}

/// Build a deterministic synthetic stream for fixtures.
pub fn synthetic_stream(
    stream_id: ContextStreamId,
    count: u64,
) -> Result<Vec<WorkingSegment>, WorkingContextError> {
    let mut out = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for i in 1..=count {
        let identity = ContextSegmentId::new(stream_id, i)
            .map_err(|_| WorkingContextError::InvalidSegmentIdentity)?;
        let fingerprint = splitmix64(i.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        // Non-zero guarantee: splitmix of non-zero seed is never zero in practice,
        // but enforce the WorkingSegment contract explicitly.
        let fingerprint = if fingerprint == 0 { 1 } else { fingerprint };
        let mut hash_bytes = [0_u8; 32];
        hash_bytes[..8].copy_from_slice(&i.to_le_bytes());
        hash_bytes[8..16].copy_from_slice(&fingerprint.to_le_bytes());
        let content_hash = ContextContentHash::from_sha256(hash_bytes);
        out.push(WorkingSegment::new(
            identity,
            i - 1,
            content_hash,
            fingerprint,
        )?);
    }
    Ok(out)
}

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkingContextError {
    InvalidCapacity,
    ZeroFingerprint,
    InvalidSegmentIdentity,
    StreamMismatch {
        expected: ContextStreamId,
        actual: ContextStreamId,
    },
    SegmentIdCollision(ContextSegmentId),
    ExpandingCeiling {
        hard_max: usize,
        retained: usize,
    },
    StepOverflow,
    UnsupportedSchema {
        got: u16,
    },
    TruncatedProvenance {
        reason: &'static str,
    },
    UnknownProvenance {
        segment_id: u64,
    },
    RegisterMismatch {
        expected: u64,
        actual: u64,
    },
    StatsMismatch,
    /// Snapshot retained rows are not in strictly increasing admission order.
    InconsistentFifoOrder,
    /// Snapshot step / admission / resolution chronology is inconsistent.
    InconsistentStepChronology {
        reason: &'static str,
    },
    InternalInvariant,
}

impl fmt::Display for WorkingContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for WorkingContextError {}

#[cfg(test)]
mod unit_smoke {
    use super::*;

    #[test]
    fn zero_capacity_fails_closed() {
        let stream = ContextStreamId::new(1).unwrap();
        assert!(matches!(
            WorkingContext::new(stream, WorkingPolicy::Fixed { capacity: 0 }),
            Err(WorkingContextError::InvalidCapacity)
        ));
        assert!(matches!(
            WorkingContext::new(stream, WorkingPolicy::Expanding { hard_max: 0 }),
            Err(WorkingContextError::InvalidCapacity)
        ));
        assert!(matches!(
            WorkingContext::new(stream, WorkingPolicy::CapacityBounded { capacity: 0 }),
            Err(WorkingContextError::InvalidCapacity)
        ));
    }
}
