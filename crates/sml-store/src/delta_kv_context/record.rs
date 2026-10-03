//! A view of one record version's verified sources, not subject-level authority.
use super::{VerifiedContext, VerifiedSource};
use core::fmt;
use sml_core::context_ledger::ContextStreamId;

/// An explicit input identity claim. Obtain it from the semantic index, not a
/// compressed vector. Construction itself authenticates neither caller nor ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordRequest {
    stream: ContextStreamId,
    record: u64,
}
impl RecordRequest {
    pub const fn new(stream: ContextStreamId, record: u64) -> Self {
        Self { stream, record }
    }
    pub const fn stream_id(self) -> ContextStreamId {
        self.stream
    }
    pub const fn record_id(self) -> u64 {
        self.record
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordBindingLimits {
    pub max_write_records: usize,
    pub max_sources: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordBindingError {
    Limits,
    ForeignStream,
    NoMatchingWrite,
    InvalidProvenance,
}
impl fmt::Display for RecordBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RecordBindingError {}

/// Cannot mint an Accept/screening result or replace the complete subject history.
/// Borrows an existing nonempty SHA-verified receipt. No new raw I/O/allocation.
///
/// ```compile_fail
/// use sml_store::delta_kv_context::VerifiedRecordContext;
/// fn not_a_subject_answer(view: VerifiedRecordContext<'_, '_>) {
///     let _ = view.screen(Some([true,true]));
/// }
/// ```
#[derive(Debug)]
pub struct VerifiedRecordContext<'receipt, 'context> {
    receipt: &'receipt VerifiedContext<'context>,
    request: RecordRequest,
    writes: usize,
    sources: usize,
}
impl VerifiedContext<'_> {
    pub fn bind_record(
        &self,
        request: RecordRequest,
        limits: RecordBindingLimits,
    ) -> Result<VerifiedRecordContext<'_, '_>, RecordBindingError> {
        if limits.max_write_records == 0
            || limits.max_sources == 0
            || self.query.records().len() > limits.max_write_records
            || self.sources.len() > limits.max_sources
        {
            return Err(RecordBindingError::Limits);
        }
        if request.stream != self.query.owner.stream {
            return Err(RecordBindingError::ForeignStream);
        }
        let writes = self
            .query
            .records()
            .filter(|r| r.semantic_record_id == request.record)
            .count();
        if writes == 0 {
            return Err(RecordBindingError::NoMatchingWrite);
        }
        let sources = self
            .sources
            .iter()
            .filter(|s| {
                s.reference
                    .semantic_record_ids()
                    .binary_search(&request.record)
                    .is_ok()
            })
            .count();
        if sources == 0 {
            return Err(RecordBindingError::InvalidProvenance);
        }
        Ok(VerifiedRecordContext {
            receipt: self,
            request,
            writes,
            sources,
        })
    }
}
impl<'context> VerifiedRecordContext<'_, 'context> {
    pub const fn request(&self) -> RecordRequest {
        self.request
    }
    pub fn key(&self) -> &[i16] {
        self.receipt.key()
    }
    pub fn generation(&self) -> usize {
        self.receipt.context_generation()
    }
    pub const fn matching_writes(&self) -> usize {
        self.writes
    }
    pub const fn source_count(&self) -> usize {
        self.sources
    }
    /// Work includes all parent key-candidate records/sources, not just matches.
    pub fn inspected_counts(&self) -> (usize, usize) {
        (
            self.receipt.query.records().len(),
            self.receipt.sources.len(),
        )
    }
    pub fn sources(&self) -> impl Iterator<Item = &VerifiedSource<'context>> + '_ {
        self.receipt.sources.iter().filter(|s| {
            s.reference
                .semantic_record_ids()
                .binary_search(&self.request.record)
                .is_ok()
        })
    }
}
