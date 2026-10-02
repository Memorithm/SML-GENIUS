use core::fmt;

use sml_core::context_ledger::{ContextSegmentRef, ContextStreamId, RawContextLedger};

use super::{parse_event, HistoryError, HistoryLimits, HistoryQuery, ObservationLog, SubjectKey};
use super::{WireError, MAX_EVENT_BYTES};
use crate::raw_context::{sha256_raw_bytes, RawContextStoreError};

/// Raw-input bounds checked before the first reader call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryVerificationLimits {
    pub max_segments: usize,
    pub max_raw_bytes: u64,
    pub max_record_bytes: usize,
}

/// A complete parsed snapshot whose source bytes matched an admitted ledger.
///
/// This proves integrity at verification time, not authenticity or world truth.
/// Raw buffers are released after parsing; their immutable descriptors remain
/// borrowed. Absence is relative to this finite snapshot, not global absence.
/// Only verify_history constructs these private fields.
///
/// ```compile_fail
/// use sml_core::context_ledger::{ContextStreamId, RawContextLedger};
/// use sml_store::observed_history::{HistoryLimits, ObservationLog, VerifiedHistory};
/// let ledger = RawContextLedger::new(ContextStreamId::new(1).unwrap());
/// let log = ObservationLog::new(HistoryLimits {
///     max_events: 8, max_keys: 8, max_element_bytes: 4096,
/// }).unwrap();
/// let forged = VerifiedHistory { ledger: &ledger, log, raw_bytes: 0 };
/// ```
#[derive(Debug)]
pub struct VerifiedHistory<'a> {
    ledger: &'a RawContextLedger,
    log: ObservationLog,
    raw_bytes: u64,
}

impl VerifiedHistory<'_> {
    pub fn query(&self, key: SubjectKey) -> HistoryQuery<'_> {
        self.log.query(key)
    }

    pub fn stream_id(&self) -> ContextStreamId {
        self.ledger.stream_id()
    }

    pub fn generation(&self) -> usize {
        self.ledger.segments().len()
    }

    pub const fn raw_bytes(&self) -> u64 {
        self.raw_bytes
    }

    pub fn max_history_element_bytes(&self) -> usize {
        self.log.max_element_bytes()
    }
}

/// Verify every segment. Any failure returns no receipt, including a late error.
///
/// The reader is a trusted I/O boundary and must enforce its own allocation and
/// I/O limits. This adapter checks source/byte budgets before I/O, then validates
/// returned length, SHA-256, canonical grammar, exact forward record identity
/// and event-history constraints. It does not certify a model's numeric output.
/// Empty ledgers can certify only an empty finite snapshot, never a positive fact.
pub fn verify_history<F>(
    ledger: &RawContextLedger,
    history_limits: HistoryLimits,
    limits: HistoryVerificationLimits,
    mut reader: F,
) -> Result<VerifiedHistory<'_>, HistoryVerificationError>
where
    F: FnMut(&ContextSegmentRef) -> Result<Vec<u8>, RawContextStoreError>,
{
    if limits.max_segments == 0 || limits.max_raw_bytes == 0 || limits.max_record_bytes == 0 {
        return Err(HistoryVerificationError::Limits);
    }
    let segments = ledger.segments();
    if segments.len() > limits.max_segments || segments.len() > history_limits.max_events {
        return Err(HistoryVerificationError::Limits);
    }
    let mut log = ObservationLog::new(history_limits).map_err(HistoryVerificationError::History)?;
    let record_cap = limits.max_record_bytes.min(MAX_EVENT_BYTES) as u64;
    let mut total = 0_u64;
    let mut previous_id = 0_u64;
    for (order, segment) in segments.iter().enumerate() {
        if segment.identity.stream_id != ledger.stream_id()
            || segment.identity.segment_id <= previous_id
            || segment.order_index != order as u64
            || segment.byte_start != total
            || segment.byte_len == 0
            || segment.semantic_record_ids().len() != 1
        {
            return Err(HistoryVerificationError::SourceBinding);
        }
        if segment.byte_len > record_cap {
            return Err(HistoryVerificationError::Limits);
        }
        total = total
            .checked_add(segment.byte_len)
            .ok_or(HistoryVerificationError::Overflow)?;
        if total > limits.max_raw_bytes {
            return Err(HistoryVerificationError::Limits);
        }
        previous_id = segment.identity.segment_id;
    }
    // All source/byte admission is complete before reader invocation.
    for segment in segments {
        let raw = reader(segment).map_err(HistoryVerificationError::Raw)?;
        if raw.len() as u64 != segment.byte_len {
            return Err(HistoryVerificationError::SourceBinding);
        }
        let hash = sha256_raw_bytes(&raw).map_err(HistoryVerificationError::Raw)?;
        if hash != segment.content_hash {
            return Err(HistoryVerificationError::SourceBinding);
        }
        let event =
            parse_event(&raw, limits.max_record_bytes).map_err(HistoryVerificationError::Wire)?;
        if segment.semantic_record_ids() != [event.id()] {
            return Err(HistoryVerificationError::SourceBinding);
        }
        log.append(event)
            .map_err(HistoryVerificationError::History)?;
    }
    Ok(VerifiedHistory {
        ledger,
        log,
        raw_bytes: total,
    })
}

#[derive(Debug)]
pub enum HistoryVerificationError {
    Limits,
    Overflow,
    SourceBinding,
    History(HistoryError),
    Wire(WireError),
    Raw(RawContextStoreError),
}

impl fmt::Display for HistoryVerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for HistoryVerificationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observed_history::{encode_event, ObservationEvent, ObservedState};
    use sml_core::context_ledger::ContextSegmentId;

    fn history_limits() -> HistoryLimits {
        HistoryLimits {
            max_events: 16,
            max_keys: 8,
            max_element_bytes: 8192,
        }
    }

    fn verification_limits() -> HistoryVerificationLimits {
        HistoryVerificationLimits {
            max_segments: 16,
            max_raw_bytes: 4096,
            max_record_bytes: MAX_EVENT_BYTES,
        }
    }

    fn world(events: &[ObservationEvent]) -> (RawContextLedger, Vec<Vec<u8>>) {
        let stream = ContextStreamId::new(7).unwrap();
        let mut ledger = RawContextLedger::new(stream);
        let mut sources = Vec::new();
        for (index, event) in events.iter().enumerate() {
            let bytes = encode_event(*event);
            let id = ContextSegmentId::new(stream, index as u64 + 1).unwrap();
            let reference = ContextSegmentRef::new(
                id,
                ledger.stats().raw_bytes,
                bytes.len() as u64,
                0,
                0,
                index as u64,
                sha256_raw_bytes(&bytes).unwrap(),
                vec![event.id()],
            )
            .unwrap();
            ledger.append(reference).unwrap();
            sources.push(bytes);
        }
        (ledger, sources)
    }

    fn event(id: u64, bits: [bool; 2], prior: Option<u64>) -> ObservationEvent {
        let key = SubjectKey::new(1, 0).unwrap();
        ObservationEvent::new(id, key, bits, prior).unwrap()
    }

    #[test]
    fn complete_snapshot_retains_conflicting_history_and_identity() {
        let (ledger, bytes) = world(&[event(1, [false; 2], None), event(2, [true; 2], None)]);
        let mut calls = 0;
        let verified = verify_history(&ledger, history_limits(), verification_limits(), |r| {
            calls += 1;
            Ok(bytes[(r.identity.segment_id - 1) as usize].clone())
        })
        .unwrap();
        let key = SubjectKey::new(1, 0).unwrap();
        assert_eq!(verified.query(key).summary().state, ObservedState::Conflict);
        assert_eq!(verified.generation(), 2);
        assert_eq!(calls, 2);
        assert_eq!(verified.stream_id(), ledger.stream_id());
        assert_eq!(verified.raw_bytes(), ledger.stats().raw_bytes);
        assert!(verified.max_history_element_bytes() > 0);
    }

    #[test]
    fn all_byte_and_segment_limits_reject_before_any_reader_call() {
        let (ledger, bytes) = world(&[event(1, [false; 2], None), event(2, [true; 2], None)]);
        for limits in [
            HistoryVerificationLimits {
                max_segments: 1,
                ..verification_limits()
            },
            HistoryVerificationLimits {
                max_raw_bytes: ledger.stats().raw_bytes - 1,
                ..verification_limits()
            },
            HistoryVerificationLimits {
                max_record_bytes: bytes[0].len() - 1,
                ..verification_limits()
            },
        ] {
            let mut calls = 0;
            let result = verify_history(&ledger, history_limits(), limits, |_| {
                calls += 1;
                Ok(Vec::new())
            });
            assert!(matches!(result, Err(HistoryVerificationError::Limits)));
            assert_eq!(calls, 0);
        }
    }

    #[test]
    fn same_length_corruption_source_substitution_and_truncation_reject() {
        let (ledger, bytes) = world(&[event(1, [false; 2], None), event(2, [true; 2], None)]);
        for variant in 0..3 {
            let result = verify_history(&ledger, history_limits(), verification_limits(), |r| {
                let mut raw = bytes[(r.identity.segment_id - 1) as usize].clone();
                match variant {
                    0 => raw[0] = b'9',
                    1 => raw = bytes[1].clone(),
                    _ => {
                        raw.pop();
                    }
                }
                Ok(raw)
            });
            assert!(matches!(
                result,
                Err(HistoryVerificationError::SourceBinding)
            ));
        }
    }

    #[test]
    fn invalid_late_history_returns_no_partial_receipt() {
        // Both records are canonical, but their identifiers are decreasing.
        let (ledger, bytes) = world(&[event(2, [false; 2], None), event(1, [true; 2], None)]);
        let result = verify_history(&ledger, history_limits(), verification_limits(), |r| {
            Ok(bytes[(r.identity.segment_id - 1) as usize].clone())
        });
        assert!(matches!(result, Err(HistoryVerificationError::History(_))));
    }

    #[test]
    fn semantic_forward_id_must_match_the_parsed_record() {
        let (ledger, bytes) = world(&[event(1, [false; 2], None)]);
        let original = &ledger.segments()[0];
        let mut other = RawContextLedger::new(ledger.stream_id());
        let mismatched = ContextSegmentRef::new(
            original.identity,
            0,
            original.byte_len,
            0,
            0,
            0,
            original.content_hash,
            vec![99],
        )
        .unwrap();
        other.append(mismatched).unwrap();
        let result = verify_history(&other, history_limits(), verification_limits(), |_| {
            Ok(bytes[0].clone())
        });
        assert!(matches!(
            result,
            Err(HistoryVerificationError::SourceBinding)
        ));
    }

    #[test]
    fn empty_snapshot_only_supports_snapshot_relative_absence() {
        let (ledger, _) = world(&[]);
        let verified = verify_history(&ledger, history_limits(), verification_limits(), |_| {
            panic!("empty snapshot must not invoke reader")
        })
        .unwrap();
        let key = SubjectKey::new(1, 0).unwrap();
        assert_eq!(verified.query(key).summary().state, ObservedState::Missing);
        assert_eq!(verified.raw_bytes(), 0);
        assert_eq!(verified.generation(), 0);
    }
}
