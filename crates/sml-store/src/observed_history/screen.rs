use super::{HistoryQuery, ObservedState, SubjectKey, VerifiedHistory};

/// Reasons describe screening only, never semantic entailment or action grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreeningReason {
    MissingObservation,
    ConflictingObservations,
    CandidateUnknown,
    ObservedAgreement,
}

/// An owner-bound diagnostic result; there is deliberately no Accept variant.
/// The candidate is never replaced with an observed source value.
#[derive(Debug)]
pub struct ScreenedCandidate<'a> {
    query: HistoryQuery<'a>,
    original: Option<[bool; 2]>,
    screened: Option<[bool; 2]>,
    reason: ScreeningReason,
}

impl ScreenedCandidate<'_> {
    pub fn key(&self) -> SubjectKey {
        self.query.key()
    }

    pub fn generation(&self) -> usize {
        self.query.summary().generation
    }

    pub const fn original(&self) -> Option<[bool; 2]> {
        self.original
    }

    pub const fn value(&self) -> Option<[bool; 2]> {
        self.screened
    }

    pub const fn reason(&self) -> ScreeningReason {
        self.reason
    }
}

/// Requires a complete verified history, not caller-minted summary/Boolean flags.
///
/// A missing or conflicting observation suppresses a candidate. Agreement only
/// preserves it, even when wrong; observing a value does not verify the model's
/// downstream computation. Original output remains separately inspectable.
/// No gold label, task family or source-answer replacement enters this function.
pub fn screen_candidate<'a>(
    history: &'a VerifiedHistory<'_>,
    key: SubjectKey,
    candidate: Option<[bool; 2]>,
) -> ScreenedCandidate<'a> {
    screen_verified_query(history.query(key), candidate)
}

// Internal only: callers must have established the complete verified snapshot.
// Both public paths preserve this precondition; no public summary-based gate.
pub(super) fn screen_verified_query(
    query: HistoryQuery<'_>,
    candidate: Option<[bool; 2]>,
) -> ScreenedCandidate<'_> {
    let (screened, reason) = match query.summary().state {
        ObservedState::Missing => (None, ScreeningReason::MissingObservation),
        ObservedState::Conflict => (None, ScreeningReason::ConflictingObservations),
        ObservedState::Agreement => match candidate {
            None => (None, ScreeningReason::CandidateUnknown),
            Some(value) => (Some(value), ScreeningReason::ObservedAgreement),
        },
    };
    ScreenedCandidate {
        query,
        original: candidate,
        screened,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observed_history::{
        encode_event, verify_history, HistoryLimits, HistoryVerificationLimits, ObservationEvent,
        MAX_EVENT_BYTES,
    };
    use crate::raw_context::sha256_raw_bytes;
    use sml_core::context_ledger::{
        ContextSegmentId, ContextSegmentRef, ContextStreamId, RawContextLedger,
    };

    fn world(values: &[[bool; 2]], replace: bool) -> (RawContextLedger, Vec<Vec<u8>>) {
        let stream = ContextStreamId::new(1).unwrap();
        let key = SubjectKey::new(1, 0).unwrap();
        let mut ledger = RawContextLedger::new(stream);
        let mut sources = Vec::new();
        for (i, &bits) in values.iter().enumerate() {
            let prior = if replace && i != 0 {
                Some(i as u64)
            } else {
                None
            };
            let event = ObservationEvent::new(i as u64 + 1, key, bits, prior).unwrap();
            let bytes = encode_event(event);
            let id = ContextSegmentId::new(stream, event.id()).unwrap();
            let reference = ContextSegmentRef::new(
                id,
                ledger.stats().raw_bytes,
                bytes.len() as u64,
                0,
                0,
                i as u64,
                sha256_raw_bytes(&bytes).unwrap(),
                vec![event.id()],
            )
            .unwrap();
            ledger.append(reference).unwrap();
            sources.push(bytes);
        }
        (ledger, sources)
    }

    fn verified<'a>(ledger: &'a RawContextLedger, raw: &[Vec<u8>]) -> VerifiedHistory<'a> {
        verify_history(
            ledger,
            HistoryLimits {
                max_events: 16,
                max_keys: 8,
                max_element_bytes: 8192,
            },
            HistoryVerificationLimits {
                max_segments: 16,
                max_raw_bytes: 4096,
                max_record_bytes: MAX_EVENT_BYTES,
            },
            |r| Ok(raw[(r.identity.segment_id - 1) as usize].clone()),
        )
        .unwrap()
    }

    #[test]
    fn missing_suppresses_every_nonzero_or_false_candidate() {
        let (ledger, raw) = world(&[], false);
        let history = verified(&ledger, &raw);
        let key = SubjectKey::new(1, 0).unwrap();
        for bits in [[false, false], [false, true], [true, false], [true, true]] {
            let screened = screen_candidate(&history, key, Some(bits));
            assert_eq!(screened.original(), Some(bits));
            assert_eq!(screened.value(), None);
            assert_eq!(screened.reason(), ScreeningReason::MissingObservation);
            assert_eq!(screened.key(), key);
            assert_eq!(screened.generation(), 0);
        }
    }

    #[test]
    fn disagreement_suppresses_even_downstream_equivalent_values() {
        let (ledger, raw) = world(&[[false, true], [true, false]], false);
        let history = verified(&ledger, &raw);
        let key = SubjectKey::new(1, 0).unwrap();
        let screened = screen_candidate(&history, key, Some([true, false]));
        assert_eq!(screened.value(), None);
        assert_eq!(screened.reason(), ScreeningReason::ConflictingObservations);
        assert_eq!(screened.generation(), 2);
    }

    #[test]
    fn agreeing_sources_do_not_repair_any_wrong_candidate() {
        let (ledger, raw) = world(&[[true, true], [true, true]], false);
        let history = verified(&ledger, &raw);
        let key = SubjectKey::new(1, 0).unwrap();
        for bits in [[false, false], [false, true], [true, false], [true, true]] {
            let screened = screen_candidate(&history, key, Some(bits));
            assert_eq!(screened.value(), Some(bits));
            assert_eq!(screened.original(), Some(bits));
            assert_eq!(screened.reason(), ScreeningReason::ObservedAgreement);
        }
    }

    #[test]
    fn candidate_unknown_stays_unknown_despite_observed_agreement() {
        let (ledger, raw) = world(&[[true; 2]], false);
        let history = verified(&ledger, &raw);
        let key = SubjectKey::new(1, 0).unwrap();
        let screened = screen_candidate(&history, key, None);
        assert_eq!(screened.value(), None);
        assert_eq!(screened.original(), None);
        assert_eq!(screened.reason(), ScreeningReason::CandidateUnknown);
    }

    #[test]
    fn declared_supersession_and_exact_query_group_are_respected() {
        let (ledger, raw) = world(&[[false; 2], [true; 2]], true);
        let history = verified(&ledger, &raw);
        let candidate = Some([false; 2]);
        let key = SubjectKey::new(1, 0).unwrap();
        let screened = screen_candidate(&history, key, candidate);
        assert_eq!(screened.value(), candidate);
        assert_eq!(screened.reason(), ScreeningReason::ObservedAgreement);
        let other = SubjectKey::new(1, 1).unwrap();
        let screened = screen_candidate(&history, other, candidate);
        assert_eq!(screened.value(), None);
        assert_eq!(screened.reason(), ScreeningReason::MissingObservation);
    }
}
