//! A compressed-key receipt identifies candidates, not the requested subject.
use super::{parse_event, HistoryQuery, ObservedState, SubjectKey, VerifiedHistory, WireError};
use crate::delta_kv_context::VerifiedContext;
use core::fmt;

#[derive(Debug, Clone, Copy)]
pub struct SubjectBindingLimits {
    pub max_candidate_sources: usize,
    pub max_record_bytes: usize,
}
#[derive(Debug)]
pub enum SubjectBindingError {
    Limits,
    SnapshotMismatch,
    MissingSubject,
    IncompleteSubject,
    SourceBinding,
    Wire(WireError),
}
impl fmt::Display for SubjectBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SubjectBindingError {}

/// Private, borrowed binding: no Accept flag, source-value replacement or action grant.
#[derive(Debug)]
pub struct BoundProjectedSubject<'a> {
    projected_key: &'a [i16],
    query: HistoryQuery<'a>,
    matched: usize,
    excluded: usize,
}
impl BoundProjectedSubject<'_> {
    pub fn subject(&self) -> SubjectKey {
        self.query.key()
    }
    pub fn projected_key(&self) -> &[i16] {
        self.projected_key
    }
    pub fn observed_state(&self) -> ObservedState {
        self.query.summary().state
    }
    pub const fn matched_sources(&self) -> usize {
        self.matched
    }
    pub const fn excluded_sources(&self) -> usize {
        self.excluded
    }
    pub fn generation(&self) -> usize {
        self.query.summary().generation
    }
}

/// Requires a complete verified history from the SAME full ledger snapshot.
/// The caller supplies an exact observed identity independently of the vector.
/// Every subject version must be among the candidate sources. Other subjects
/// are excluded, not interpreted as evidence of this subject's presence.
/// Work is bounded by candidate sources times matching history events; no new
/// raw I/O or retained allocation occurs here. parse_event has its own byte cap.
/// A successful binding proves source/subject identity, not latent correctness.
pub fn bind_projected_subject<'a>(
    receipt: &'a VerifiedContext<'_>,
    history: &'a VerifiedHistory<'_>,
    subject: SubjectKey,
    limits: SubjectBindingLimits,
) -> Result<BoundProjectedSubject<'a>, SubjectBindingError> {
    if limits.max_candidate_sources == 0
        || limits.max_record_bytes == 0
        || receipt.sources().len() > limits.max_candidate_sources
    {
        return Err(SubjectBindingError::Limits);
    }
    if receipt.ledger() != history.ledger().segments() {
        return Err(SubjectBindingError::SnapshotMismatch);
    }
    let query = history.query(subject);
    let expected = query.events().len();
    if expected == 0 {
        return Err(SubjectBindingError::MissingSubject);
    }
    let (mut matched, mut excluded) = (0_usize, 0_usize);
    for source in receipt.sources() {
        let event = parse_event(source.bytes(), limits.max_record_bytes)
            .map_err(SubjectBindingError::Wire)?;
        if source.reference().semantic_record_ids() != [event.id()] {
            return Err(SubjectBindingError::SourceBinding);
        }
        if event.key() == subject {
            if !query.events().any(|e| *e == event) {
                return Err(SubjectBindingError::SourceBinding);
            }
            matched += 1;
        } else {
            excluded += 1;
        }
    }
    if matched != expected {
        return Err(SubjectBindingError::IncompleteSubject);
    }
    Ok(BoundProjectedSubject {
        projected_key: receipt.key(),
        query,
        matched,
        excluded,
    })
}
