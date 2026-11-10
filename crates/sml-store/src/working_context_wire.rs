//! CONTEXT-2 G4: versioned, bounded, canonical byte format for
//! [`WorkingContextSnapshot`].
//!
//! This module is the deterministic serialization / admission boundary of the
//! CONTEXT-2 working substrate. It owns **syntax, limits and canonical form**
//! only; semantic validity (register, stats, chronology, coverage, fates) stays
//! owned by [`WorkingContext::restore`], which [`decode_and_restore`] and
//! [`restore_into`] call only after a complete, successful wire decode.
//!
//! # Layout (wire version 1, all integers little-endian)
//!
//! Header — [`WIRE_HEADER_BYTES`] = 93 bytes:
//!
//! | Offset | Size | Field |
//! | --- | --- | --- |
//! | 0 | 4 | magic `b"SWC2"` ([`WORKING_CONTEXT_WIRE_MAGIC`]) |
//! | 4 | 2 | wire version, must equal [`WORKING_CONTEXT_WIRE_VERSION`] (1) |
//! | 6 | 2 | snapshot schema, must equal [`WORKING_CONTEXT_SCHEMA_VERSION`] (1) |
//! | 8 | 8 | stream id (non-zero) |
//! | 16 | 1 | policy tag: 1 Fixed, 2 Expanding, 3 CapacityBounded |
//! | 17 | 8 | policy capacity / hard_max (non-zero, fits `usize`) |
//! | 25 | 8 | step |
//! | 33 | 8 | recurrent register |
//! | 41 | 40 | stats: admitted, retained, evicted, lost, recurrent_folds |
//! | 81 | 4 | retained row count `R` (u32) |
//! | 85 | 4 | eviction-log row count `E` (u32) |
//! | 89 | 4 | seen-id count `S` (u32) |
//!
//! Then `R + E` provenance rows of [`WIRE_ROW_BYTES`] = 74 bytes each
//! (retained rows first, then eviction-log rows):
//!
//! | Offset | Size | Field |
//! | --- | --- | --- |
//! | 0 | 8 | segment id (non-zero; stream id is the header stream) |
//! | 8 | 8 | order index |
//! | 16 | 32 | content hash |
//! | 48 | 8 | fingerprint |
//! | 56 | 8 | admitted step |
//! | 64 | 1 | resolved flag: 0 = None, 1 = Some |
//! | 65 | 8 | resolved step (must be 0 when flag is 0) |
//! | 73 | 1 | fate: 0 Retained, 1 Evicted, 2 Lost |
//!
//! Then `S` seen segment ids of [`WIRE_SEEN_ID_BYTES`] = 8 bytes each.
//! Nothing may follow (trailing bytes are rejected).
//!
//! # Canonical form
//!
//! Exactly one byte string encodes a given snapshot, so for every accepted
//! input `encode_snapshot(&decode_snapshot(b)?)? == b`:
//!
//! - retained rows strictly increasing by `admitted_step` (FIFO oldest-first);
//! - eviction-log rows strictly increasing by `segment_id`;
//! - seen ids strictly increasing (hence unique) and non-zero;
//! - resolved flag in `{0, 1}` and zero padding when absent;
//! - every row's stream id equals the header stream id (the encoder refuses
//!   to emit otherwise because the wire stores only one stream id).
//!
//! These are exactly the orders produced by [`WorkingContext::snapshot`].
//!
//! # Limits (checked before any row allocation)
//!
//! - Input length is compared against [`SnapshotWireLimits::max_bytes`]
//!   (hard cap [`MAX_SNAPSHOT_BYTES`]) before reading any byte.
//! - `R + E` and `S` are each compared against
//!   [`SnapshotWireLimits::max_rows`] (hard cap [`MAX_SNAPSHOT_ROWS`]) and `R`
//!   against the declared policy limit, using checked arithmetic.
//! - The exact total length implied by the counts must equal the input length
//!   (shorter → [`SnapshotWireError::Truncated`], longer →
//!   [`SnapshotWireError::TrailingBytes`]) before any `Vec` is sized.
//!
//! A row vector is therefore never sized from an unchecked length prefix.
//!
//! # Non-claims
//!
//! No integrity/authenticity guarantee: this is not a checksum or a signature
//! and does not detect semantically-valid tampering of hashes or order
//! indices. It is not an effective context window and not CCOS recall; no
//! 1M-attention or quality claim is made (`NOT_MEASURED`).

use core::fmt;

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};

use crate::working_context::{
    RetentionFate, WorkingContext, WorkingContextError, WorkingContextSnapshot,
    WorkingContextStats, WorkingPolicy, WorkingSegmentProvenance, WORKING_CONTEXT_SCHEMA_VERSION,
};

/// Magic prefix of every v1 snapshot.
pub const WORKING_CONTEXT_WIRE_MAGIC: [u8; 4] = *b"SWC2";
/// Only wire version accepted by this reader/writer.
pub const WORKING_CONTEXT_WIRE_VERSION: u16 = 1;
/// Fixed header size in bytes.
pub const WIRE_HEADER_BYTES: usize = 93;
/// Fixed provenance-row size in bytes.
pub const WIRE_ROW_BYTES: usize = 74;
/// Size of one seen segment id in bytes.
pub const WIRE_SEEN_ID_BYTES: usize = 8;
/// Hard cap on provenance rows (`R + E`) and, separately, on seen ids (`S`).
pub const MAX_SNAPSHOT_ROWS: usize = 65_536;
/// Hard cap on encoded snapshot size (≈ 5.1 MiB).
pub const MAX_SNAPSHOT_BYTES: usize =
    WIRE_HEADER_BYTES + MAX_SNAPSHOT_ROWS * (WIRE_ROW_BYTES + WIRE_SEEN_ID_BYTES);

const OFF_COUNTS: usize = 81;

const TAG_FIXED: u8 = 1;
const TAG_EXPANDING: u8 = 2;
const TAG_CAPACITY_BOUNDED: u8 = 3;

const FATE_RETAINED: u8 = 0;
const FATE_EVICTED: u8 = 1;
const FATE_LOST: u8 = 2;

/// Caller-tightenable limits. Can never exceed the hard caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotWireLimits {
    max_bytes: usize,
    max_rows: usize,
}

impl SnapshotWireLimits {
    /// Hard caps: [`MAX_SNAPSHOT_BYTES`] / [`MAX_SNAPSHOT_ROWS`].
    pub const DEFAULT: Self = Self {
        max_bytes: MAX_SNAPSHOT_BYTES,
        max_rows: MAX_SNAPSHOT_ROWS,
    };

    /// Tighter limits. Zero or above-hard-cap values fail closed.
    pub fn new(max_bytes: usize, max_rows: usize) -> Result<Self, SnapshotWireError> {
        if max_bytes == 0
            || max_rows == 0
            || max_bytes > MAX_SNAPSHOT_BYTES
            || max_rows > MAX_SNAPSHOT_ROWS
        {
            return Err(SnapshotWireError::InvalidLimits);
        }
        Ok(Self {
            max_bytes,
            max_rows,
        })
    }

    #[must_use]
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }

    #[must_use]
    pub const fn max_rows(self) -> usize {
        self.max_rows
    }
}

impl Default for SnapshotWireLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Section of the wire body, for ordering diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireSection {
    Retained,
    EvictionLog,
    SeenIds,
}

/// Fail-closed wire errors. `row` indices count records globally in wire
/// order: retained rows, then eviction-log rows, then seen ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotWireError {
    /// Limits constructor received zero or above-hard-cap values.
    InvalidLimits,
    /// Input (or would-be output) exceeds the byte limit.
    InputTooLarge {
        len: usize,
        max: usize,
    },
    /// Input shorter than the header or than the length implied by counts.
    Truncated {
        needed: usize,
        got: usize,
    },
    /// Bytes remain after the last declared record.
    TrailingBytes {
        extra: usize,
    },
    BadMagic,
    UnknownWireVersion {
        got: u16,
    },
    UnsupportedSchema {
        got: u16,
    },
    InvalidStreamId,
    UnknownPolicyTag {
        got: u8,
    },
    ZeroPolicyLimit,
    /// Policy limit does not fit `usize` on this target.
    PolicyLimitOverflow,
    /// A row count (or `R + E`) exceeds the row limit.
    RowCountLimit {
        count: u64,
        max: usize,
    },
    /// Declared retained rows exceed the policy capacity / hard_max.
    RetainedExceedsPolicyLimit {
        retained: u64,
        limit: usize,
    },
    InvalidSegmentId {
        row: usize,
    },
    NoncanonicalResolvedStep {
        row: usize,
    },
    UnknownFateTag {
        row: usize,
        got: u8,
    },
    NoncanonicalOrder {
        section: WireSection,
        index: usize,
    },
    /// Encoder: a row's stream id differs from the snapshot stream id.
    RowStreamMismatch {
        row: usize,
    },
    /// Wire decode succeeded but semantic restore failed closed.
    Restore(WorkingContextError),
}

impl fmt::Display for SnapshotWireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SnapshotWireError {}

/// Encode with the default (hard-cap) limits.
pub fn encode_snapshot(snapshot: &WorkingContextSnapshot) -> Result<Vec<u8>, SnapshotWireError> {
    encode_snapshot_with_limits(snapshot, SnapshotWireLimits::DEFAULT)
}

/// Encode a live context's snapshot with the default limits.
pub fn encode_context(ctx: &WorkingContext) -> Result<Vec<u8>, SnapshotWireError> {
    encode_snapshot(&ctx.snapshot())
}

/// Encode into canonical v1 bytes. Refuses (never repairs) noncanonical or
/// over-limit snapshots so that decode → encode is the identity.
pub fn encode_snapshot_with_limits(
    snapshot: &WorkingContextSnapshot,
    limits: SnapshotWireLimits,
) -> Result<Vec<u8>, SnapshotWireError> {
    if snapshot.schema_version != WORKING_CONTEXT_SCHEMA_VERSION {
        return Err(SnapshotWireError::UnsupportedSchema {
            got: snapshot.schema_version,
        });
    }
    let (tag, limit) = policy_parts(snapshot.policy);
    if limit == 0 {
        return Err(SnapshotWireError::ZeroPolicyLimit);
    }
    let r = snapshot.retained.len();
    let e = snapshot.eviction_log.len();
    let s = snapshot.seen_segment_ids.len();
    let rows = r.checked_add(e).ok_or(SnapshotWireError::RowCountLimit {
        count: u64::MAX,
        max: limits.max_rows,
    })?;
    check_count(rows as u64, limits.max_rows)?;
    check_count(s as u64, limits.max_rows)?;
    if r > limit {
        return Err(SnapshotWireError::RetainedExceedsPolicyLimit {
            retained: r as u64,
            limit,
        });
    }
    let total = body_len(rows, s).ok_or(SnapshotWireError::InputTooLarge {
        len: usize::MAX,
        max: limits.max_bytes,
    })?;
    if total > limits.max_bytes {
        return Err(SnapshotWireError::InputTooLarge {
            len: total,
            max: limits.max_bytes,
        });
    }

    for (i, row) in snapshot
        .retained
        .iter()
        .chain(snapshot.eviction_log.iter())
        .enumerate()
    {
        if row.identity.stream_id != snapshot.stream_id {
            return Err(SnapshotWireError::RowStreamMismatch { row: i });
        }
        if row.identity.segment_id == 0 {
            return Err(SnapshotWireError::InvalidSegmentId { row: i });
        }
    }
    check_order(
        snapshot.retained.iter().map(|row| row.admitted_step),
        WireSection::Retained,
    )?;
    check_order(
        snapshot
            .eviction_log
            .iter()
            .map(|row| row.identity.segment_id),
        WireSection::EvictionLog,
    )?;
    for (i, id) in snapshot.seen_segment_ids.iter().enumerate() {
        if *id == 0 {
            return Err(SnapshotWireError::InvalidSegmentId { row: rows + i });
        }
    }
    check_order(
        snapshot.seen_segment_ids.iter().copied(),
        WireSection::SeenIds,
    )?;

    // Sizes are bounded by `limits` above; allocation is exact.
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&WORKING_CONTEXT_WIRE_MAGIC);
    out.extend_from_slice(&WORKING_CONTEXT_WIRE_VERSION.to_le_bytes());
    out.extend_from_slice(&snapshot.schema_version.to_le_bytes());
    out.extend_from_slice(&snapshot.stream_id.get().to_le_bytes());
    out.push(tag);
    out.extend_from_slice(&(limit as u64).to_le_bytes());
    out.extend_from_slice(&snapshot.step.to_le_bytes());
    out.extend_from_slice(&snapshot.recurrent_register.to_le_bytes());
    let st = snapshot.stats;
    for v in [
        st.admitted,
        st.retained,
        st.evicted,
        st.lost,
        st.recurrent_folds,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for count in [r, e, s] {
        // Bounded by MAX_SNAPSHOT_ROWS (< u32::MAX) above.
        let c = u32::try_from(count).map_err(|_| SnapshotWireError::RowCountLimit {
            count: count as u64,
            max: limits.max_rows,
        })?;
        out.extend_from_slice(&c.to_le_bytes());
    }
    for row in snapshot.retained.iter().chain(snapshot.eviction_log.iter()) {
        out.extend_from_slice(&row.identity.segment_id.to_le_bytes());
        out.extend_from_slice(&row.order_index.to_le_bytes());
        out.extend_from_slice(row.content_hash.bytes());
        out.extend_from_slice(&row.fingerprint.to_le_bytes());
        out.extend_from_slice(&row.admitted_step.to_le_bytes());
        match row.resolved_step {
            None => {
                out.push(0);
                out.extend_from_slice(&0_u64.to_le_bytes());
            }
            Some(step) => {
                out.push(1);
                out.extend_from_slice(&step.to_le_bytes());
            }
        }
        out.push(match row.fate {
            RetentionFate::Retained => FATE_RETAINED,
            RetentionFate::Evicted => FATE_EVICTED,
            RetentionFate::Lost => FATE_LOST,
        });
    }
    for id in &snapshot.seen_segment_ids {
        out.extend_from_slice(&id.to_le_bytes());
    }
    if out.len() != total {
        return Err(SnapshotWireError::Restore(
            WorkingContextError::InternalInvariant,
        ));
    }
    Ok(out)
}

/// Decode with the default (hard-cap) limits.
pub fn decode_snapshot(bytes: &[u8]) -> Result<WorkingContextSnapshot, SnapshotWireError> {
    decode_snapshot_with_limits(bytes, SnapshotWireLimits::DEFAULT)
}

/// Decode canonical v1 bytes. Wire-level only: the result still has to pass
/// [`WorkingContext::restore`] (see [`decode_and_restore`]).
pub fn decode_snapshot_with_limits(
    bytes: &[u8],
    limits: SnapshotWireLimits,
) -> Result<WorkingContextSnapshot, SnapshotWireError> {
    // 1. Byte limit before reading anything.
    if bytes.len() > limits.max_bytes {
        return Err(SnapshotWireError::InputTooLarge {
            len: bytes.len(),
            max: limits.max_bytes,
        });
    }
    if bytes.len() < WIRE_HEADER_BYTES {
        return Err(SnapshotWireError::Truncated {
            needed: WIRE_HEADER_BYTES,
            got: bytes.len(),
        });
    }
    let mut cur = Cursor { bytes, pos: 0 };

    // 2. Fixed header.
    if cur.array::<4>()? != WORKING_CONTEXT_WIRE_MAGIC {
        return Err(SnapshotWireError::BadMagic);
    }
    let wire_version = cur.u16()?;
    if wire_version != WORKING_CONTEXT_WIRE_VERSION {
        return Err(SnapshotWireError::UnknownWireVersion { got: wire_version });
    }
    let schema_version = cur.u16()?;
    if schema_version != WORKING_CONTEXT_SCHEMA_VERSION {
        return Err(SnapshotWireError::UnsupportedSchema {
            got: schema_version,
        });
    }
    let stream_id =
        ContextStreamId::new(cur.u64()?).map_err(|_| SnapshotWireError::InvalidStreamId)?;
    let tag = cur.u8()?;
    let raw_limit = cur.u64()?;
    if !matches!(tag, TAG_FIXED | TAG_EXPANDING | TAG_CAPACITY_BOUNDED) {
        return Err(SnapshotWireError::UnknownPolicyTag { got: tag });
    }
    if raw_limit == 0 {
        return Err(SnapshotWireError::ZeroPolicyLimit);
    }
    let limit = usize::try_from(raw_limit).map_err(|_| SnapshotWireError::PolicyLimitOverflow)?;
    let policy = match tag {
        TAG_FIXED => WorkingPolicy::Fixed { capacity: limit },
        TAG_EXPANDING => WorkingPolicy::Expanding { hard_max: limit },
        _ => WorkingPolicy::CapacityBounded { capacity: limit },
    };
    let step = cur.u64()?;
    let recurrent_register = cur.u64()?;
    let stats = WorkingContextStats {
        admitted: cur.u64()?,
        retained: cur.u64()?,
        evicted: cur.u64()?,
        lost: cur.u64()?,
        recurrent_folds: cur.u64()?,
    };
    debug_assert_eq!(cur.pos, OFF_COUNTS);
    let r = u64::from(cur.u32()?);
    let e = u64::from(cur.u32()?);
    let s = u64::from(cur.u32()?);

    // 3. Count limits (checked arithmetic) before any allocation.
    check_count(r, limits.max_rows)?;
    check_count(e, limits.max_rows)?;
    check_count(r + e, limits.max_rows)?; // u32 + u32 cannot overflow u64
    check_count(s, limits.max_rows)?;
    if r > limit as u64 {
        return Err(SnapshotWireError::RetainedExceedsPolicyLimit { retained: r, limit });
    }
    // Counts are now <= MAX_SNAPSHOT_ROWS, so these conversions are lossless.
    let r = r as usize;
    let e = e as usize;
    let s = s as usize;
    let rows = r + e;
    let needed = body_len(rows, s).ok_or(SnapshotWireError::InputTooLarge {
        len: usize::MAX,
        max: limits.max_bytes,
    })?;
    if bytes.len() < needed {
        return Err(SnapshotWireError::Truncated {
            needed,
            got: bytes.len(),
        });
    }
    if bytes.len() > needed {
        return Err(SnapshotWireError::TrailingBytes {
            extra: bytes.len() - needed,
        });
    }

    // 4. Exact length proven; allocation is bounded by checked counts.
    let mut retained = Vec::with_capacity(r);
    for i in 0..r {
        retained.push(cur.row(stream_id, i)?);
    }
    let mut eviction_log = Vec::with_capacity(e);
    for i in 0..e {
        eviction_log.push(cur.row(stream_id, r + i)?);
    }
    let mut seen_segment_ids = Vec::with_capacity(s);
    for i in 0..s {
        let id = cur.u64()?;
        if id == 0 {
            return Err(SnapshotWireError::InvalidSegmentId { row: rows + i });
        }
        seen_segment_ids.push(id);
    }
    if cur.pos != bytes.len() {
        return Err(SnapshotWireError::TrailingBytes {
            extra: bytes.len() - cur.pos,
        });
    }

    // 5. Canonical ordering.
    check_order(
        retained.iter().map(|row| row.admitted_step),
        WireSection::Retained,
    )?;
    check_order(
        eviction_log.iter().map(|row| row.identity.segment_id),
        WireSection::EvictionLog,
    )?;
    check_order(seen_segment_ids.iter().copied(), WireSection::SeenIds)?;

    Ok(WorkingContextSnapshot {
        schema_version,
        stream_id,
        policy,
        step,
        recurrent_register,
        retained,
        eviction_log,
        stats,
        seen_segment_ids,
    })
}

/// Full admission: wire decode (default limits), then semantic
/// [`WorkingContext::restore`]. Nothing is restored unless both succeed.
pub fn decode_and_restore(bytes: &[u8]) -> Result<WorkingContext, SnapshotWireError> {
    decode_and_restore_with_limits(bytes, SnapshotWireLimits::DEFAULT)
}

/// [`decode_and_restore`] with caller limits.
pub fn decode_and_restore_with_limits(
    bytes: &[u8],
    limits: SnapshotWireLimits,
) -> Result<WorkingContext, SnapshotWireError> {
    let snapshot = decode_snapshot_with_limits(bytes, limits)?;
    WorkingContext::restore(&snapshot).map_err(SnapshotWireError::Restore)
}

/// Replace `target` with the context encoded in `bytes` only after full
/// decode + restore success. On any error `target` is left untouched.
pub fn restore_into(target: &mut WorkingContext, bytes: &[u8]) -> Result<(), SnapshotWireError> {
    let restored = decode_and_restore(bytes)?;
    *target = restored;
    Ok(())
}

fn policy_parts(policy: WorkingPolicy) -> (u8, usize) {
    match policy {
        WorkingPolicy::Fixed { capacity } => (TAG_FIXED, capacity),
        WorkingPolicy::Expanding { hard_max } => (TAG_EXPANDING, hard_max),
        WorkingPolicy::CapacityBounded { capacity } => (TAG_CAPACITY_BOUNDED, capacity),
    }
}

fn check_count(count: u64, max: usize) -> Result<(), SnapshotWireError> {
    if count > max as u64 {
        return Err(SnapshotWireError::RowCountLimit { count, max });
    }
    Ok(())
}

fn body_len(rows: usize, seen: usize) -> Option<usize> {
    rows.checked_mul(WIRE_ROW_BYTES)?
        .checked_add(seen.checked_mul(WIRE_SEEN_ID_BYTES)?)?
        .checked_add(WIRE_HEADER_BYTES)
}

fn check_order(
    keys: impl Iterator<Item = u64>,
    section: WireSection,
) -> Result<(), SnapshotWireError> {
    let mut prev: Option<u64> = None;
    for (index, key) in keys.enumerate() {
        if let Some(p) = prev {
            if key <= p {
                return Err(SnapshotWireError::NoncanonicalOrder { section, index });
            }
        }
        prev = Some(key);
    }
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn array<const N: usize>(&mut self) -> Result<[u8; N], SnapshotWireError> {
        let end = self
            .pos
            .checked_add(N)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(SnapshotWireError::Truncated {
                needed: self.pos.saturating_add(N),
                got: self.bytes.len(),
            })?;
        let mut out = [0_u8; N];
        out.copy_from_slice(&self.bytes[self.pos..end]);
        self.pos = end;
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, SnapshotWireError> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, SnapshotWireError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, SnapshotWireError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, SnapshotWireError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn row(
        &mut self,
        stream_id: ContextStreamId,
        row: usize,
    ) -> Result<WorkingSegmentProvenance, SnapshotWireError> {
        let segment_id = self.u64()?;
        let identity = ContextSegmentId::new(stream_id, segment_id)
            .map_err(|_| SnapshotWireError::InvalidSegmentId { row })?;
        let order_index = self.u64()?;
        let content_hash = ContextContentHash::from_sha256(self.array::<32>()?);
        let fingerprint = self.u64()?;
        let admitted_step = self.u64()?;
        let flag = self.u8()?;
        let raw_resolved = self.u64()?;
        let resolved_step = match (flag, raw_resolved) {
            (0, 0) => None,
            (1, step) => Some(step),
            _ => return Err(SnapshotWireError::NoncanonicalResolvedStep { row }),
        };
        let fate = match self.u8()? {
            FATE_RETAINED => RetentionFate::Retained,
            FATE_EVICTED => RetentionFate::Evicted,
            FATE_LOST => RetentionFate::Lost,
            got => return Err(SnapshotWireError::UnknownFateTag { row, got }),
        };
        Ok(WorkingSegmentProvenance {
            identity,
            order_index,
            content_hash,
            fingerprint,
            admitted_step,
            resolved_step,
            fate,
        })
    }
}
