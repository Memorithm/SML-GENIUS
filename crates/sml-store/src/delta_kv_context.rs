//! Bounded DKV-3a-0 source-evidence adapter, not a context-model qualification.
//!
//! Numeric Delta-KV semantics remain in sml-core. This layer owns raw SHA-256
//! verification, so an unchecked Boolean or a digest match cannot mint a receipt.
//! A receipt proves byte identity and provenance, NOT truth, entailment, calibrated
//! confidence or permission to execute an action. Task resolution is separate.
//!
//! Budgets count element payloads, not allocator overhead, RSS or page cache.
//! Writes are atomic for returned errors; process-crash durability is not claimed.

use core::{fmt, mem::size_of};

use sml_core::{
    context_ledger::{
        ContextSegmentId, ContextSegmentRef, ContextStreamId, RawContextLedger,
    },
    delta_kv::{DeltaKvCounters, DeltaKvError, DeltaKvMemory},
};

use crate::raw_context::{sha256_raw_bytes, RawContextStoreError};

pub const DKV_CONTEXT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextLimits {
    pub max_writes: usize,
    pub max_sources_per_write: usize,
    pub max_source_links: usize,
    pub max_ledger_segments: usize,
    pub max_ledger_semantic_links: usize,
    pub max_element_bytes: usize,
    pub max_staging_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationLimits {
    pub max_sources: usize,
    pub max_raw_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementPlan {
    pub matrix_coefficient_bytes: usize,
    pub max_retained_element_bytes: usize,
    pub max_write_staging_element_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContextCounters {
    pub ingest_prediction_macs: u64,
    pub query_prediction_macs: u64,
    pub matrix_coefficients_copied: u64,
    pub ledger_descriptors_copied: u64,
    pub index_entries_shifted: u64,
}

/// Immutable after admission. The full key, not its diagnostic fingerprint,
/// establishes equality. All admitted source descriptors are pinned separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteEvidence {
    write_id: u64,
    context_generation: u64,
    semantic_record_id: u64,
    key_q12: Vec<i16>,
    sources: Vec<ContextSegmentId>,
    target_fingerprint: u64,
    learning_rate_q12: u16,
    pre_update_counter: u64,
    post_update_counter: u64,
}

impl WriteEvidence {
    pub fn write_id(&self) -> u64 { self.write_id }
    pub fn context_generation(&self) -> u64 { self.context_generation }
    pub fn semantic_record_id(&self) -> u64 { self.semantic_record_id }
    pub fn key_q12(&self) -> &[i16] { &self.key_q12 }
    pub fn sources(&self) -> &[ContextSegmentId] { &self.sources }
    /// Diagnostic only; never used for key membership or source verification.
    pub fn target_fingerprint(&self) -> u64 { self.target_fingerprint }
    pub fn learning_rate_q12(&self) -> u16 { self.learning_rate_q12 }
    pub fn update_counters(&self) -> (u64, u64) {
        (self.pre_update_counter, self.post_update_counter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaKvContextMemory {
    memory: DeltaKvMemory,
    stream: ContextStreamId,
    limits: ContextLimits,
    plan: ElementPlan,
    ledger: Vec<ContextSegmentRef>,
    records: Vec<WriteEvidence>,
    // Sorted by full key, then write order. Query does not scan write history.
    index: Vec<usize>,
    source_links: usize,
    numeric: DeltaKvCounters,
    work: ContextCounters,
}

impl DeltaKvContextMemory {
    pub fn new(
        stream: ContextStreamId,
        key_dim: usize,
        value_dim: usize,
        limits: ContextLimits,
    ) -> Result<Self, ContextError> {
        if key_dim == 0 || value_dim == 0 || limits.max_writes == 0
            || limits.max_sources_per_write == 0 || limits.max_ledger_segments == 0
        {
            return Err(ContextError::InvalidLimits);
        }
        let matrix = mul(mul(key_dim, value_dim)?, size_of::<i16>())?;
        let records = mul(limits.max_writes,
            add(size_of::<WriteEvidence>() + size_of::<usize>(), mul(key_dim, 2)?)?)?;
        let sources = mul(limits.max_source_links, size_of::<ContextSegmentId>())?;
        let ledger = add(mul(limits.max_ledger_segments, size_of::<ContextSegmentRef>())?,
            mul(limits.max_ledger_semantic_links, size_of::<u64>())?)?;
        let retained = add(add(add(size_of::<Self>(), matrix)?, records)?, add(sources, ledger)?)?;
        // Temporary matrix, ledger snapshot, one prepared record and prediction.
        let staging = add(add(matrix, ledger)?, add(
            add(size_of::<WriteEvidence>(), mul(key_dim, 2)?)?,
            add(mul(limits.max_sources_per_write, size_of::<ContextSegmentId>())?,
                mul(value_dim, size_of::<i32>())?)?)?)?;
        if retained > limits.max_element_bytes || staging > limits.max_staging_bytes {
            return Err(ContextError::BudgetExceeded);
        }
        Ok(Self {
            memory: DeltaKvMemory::new(key_dim, value_dim)?, stream, limits,
            plan: ElementPlan { matrix_coefficient_bytes: matrix,
                max_retained_element_bytes: retained, max_write_staging_element_bytes: staging },
            ledger: Vec::new(), records: Vec::new(), index: Vec::new(), source_links: 0,
            numeric: DeltaKvCounters::default(), work: ContextCounters::default(),
        })
    }

    pub fn element_plan(&self) -> ElementPlan { self.plan }
    pub fn numeric_counters(&self) -> DeltaKvCounters { self.numeric }
    pub fn context_counters(&self) -> ContextCounters { self.work }
    pub fn matrix_semantic_bits(&self) -> u128 { self.memory.semantic_bits() }
    pub fn evidence(&self) -> &[WriteEvidence] { &self.records }

    /// Caller-owned ledger and verification buffers are outside the memory plan.
    /// Full descriptors/forward links are compared, not merely their count.
    fn check_ledger(&self, ledger: &RawContextLedger) -> Result<(), ContextError> {
        if ledger.segments().len() > self.limits.max_ledger_segments {
            return Err(ContextError::BudgetExceeded);
        }
        if ledger.stream_id() != self.stream || !ledger.segments().starts_with(&self.ledger) {
            return Err(ContextError::ContextMismatch);
        }
        let links = ledger.segments().iter().try_fold(0_usize,
            |n, s| add(n, s.semantic_record_ids().len()))?;
        if ledger.segments().len() > self.limits.max_ledger_segments
            || links > self.limits.max_ledger_semantic_links
        {
            return Err(ContextError::BudgetExceeded);
        }
        Ok(())
    }

    pub fn write(
        &mut self,
        ledger: &RawContextLedger,
        semantic_record_id: u64,
        key: &[i16],
        target: &[i16],
        rate_q12: u16,
        sources: &[ContextSegmentId],
    ) -> Result<&WriteEvidence, ContextError> {
        self.check_ledger(ledger)?;
        if key.len() != self.memory.key_dim() || target.len() != self.memory.value_dim() {
            return Err(ContextError::ShapeMismatch);
        }
        let links = add(self.source_links, sources.len())?;
        if self.records.len() >= self.limits.max_writes
            || sources.len() > self.limits.max_sources_per_write
            || links > self.limits.max_source_links
        {
            return Err(ContextError::BudgetExceeded);
        }
        let mut canonical = sources.to_vec();
        canonical.sort_unstable();
        if canonical.is_empty() || canonical.windows(2).any(|w| w[0] == w[1]) {
            return Err(ContextError::InvalidProvenance);
        }
        // Require the COMPLETE forward/reverse relation without an unbounded set.
        for id in &canonical {
            if id.stream_id != self.stream { return Err(ContextError::InvalidProvenance); }
            let i = ledger.segments().binary_search_by_key(&id.segment_id, |s| s.identity.segment_id)
                .map_err(|_| ContextError::InvalidProvenance)?;
            if ledger.segments()[i].semantic_record_ids().binary_search(&semantic_record_id).is_err() {
                return Err(ContextError::InvalidProvenance);
            }
        }
        for segment in ledger.segments() {
            if segment.semantic_record_ids().binary_search(&semantic_record_id).is_ok()
                && canonical.binary_search(&segment.identity).is_err()
            { return Err(ContextError::InvalidProvenance); }
        }
        let macs = u64::try_from(mul(key.len(), target.len())?).map_err(|_| ContextError::Overflow)?;
        let mut work = self.work;
        work.ingest_prediction_macs = sum64(work.ingest_prediction_macs, macs)?;
        work.matrix_coefficients_copied = sum64(work.matrix_coefficients_copied, macs)?;
        work.ledger_descriptors_copied = sum64(work.ledger_descriptors_copied,
            u64::try_from(ledger.segments().len()).map_err(|_| ContextError::Overflow)?)?;
        let position = self.index.partition_point(|&i| self.records[i].key_q12.as_slice() <= key);
        work.index_entries_shifted = sum64(work.index_entries_shifted,
            u64::try_from(self.index.len() - position).map_err(|_| ContextError::Overflow)?)?;
        sum64(self.numeric.updates, 1)?;
        sum64(self.numeric.read_macs, macs)?;
        sum64(self.numeric.weight_updates, macs)?;
        sum64(self.numeric.saturations, macs)?;
        let mut candidate = self.memory.clone();
        let mut numeric = self.numeric;
        candidate.update(key, target, rate_q12, &mut numeric)?;
        // All fallible preparation precedes the logical commit. Allocation aborts
        // and process crashes are explicitly outside the Result atomicity claim.
        let snapshot = ledger.segments().to_vec();
        let record = WriteEvidence {
            write_id: u64::try_from(self.records.len()).map_err(|_| ContextError::Overflow)? + 1,
            context_generation: u64::try_from(snapshot.len()).map_err(|_| ContextError::Overflow)?,
            semantic_record_id, key_q12: key.to_vec(), sources: canonical,
            target_fingerprint: fingerprint(target), learning_rate_q12: rate_q12,
            pre_update_counter: self.numeric.updates, post_update_counter: numeric.updates,
        };
        self.records.try_reserve_exact(1).map_err(|_| ContextError::Allocation)?;
        self.index.try_reserve_exact(1).map_err(|_| ContextError::Allocation)?;
        self.index.insert(position, self.records.len());
        self.records.push(record);
        self.memory = candidate;
        self.numeric = numeric;
        self.work = work;
        self.ledger = snapshot;
        self.source_links = links;
        Ok(self.records.last().expect("record committed"))
    }

    /// Derived Q12 values, never a receipt, confidence or proof of key presence.
    pub fn read(&mut self, key: &[i16]) -> Result<Vec<i32>, ContextError> {
        if key.len() != self.memory.key_dim() { return Err(ContextError::ShapeMismatch); }
        let macs = u64::try_from(mul(key.len(), self.memory.value_dim())?)
            .map_err(|_| ContextError::Overflow)?;
        sum64(self.numeric.reads, 1)?;
        sum64(self.numeric.read_macs, macs)?;
        let total = sum64(self.work.query_prediction_macs, macs)?;
        let mut numeric = self.numeric;
        let value = self.memory.read(key, &mut numeric)?;
        self.numeric = numeric;
        self.work.query_prediction_macs = total;
        Ok(value)
    }

    pub fn query(&self, key: &[i16]) -> Result<QueryEvidence<'_>, ContextError> {
        if key.len() != self.memory.key_dim() { return Err(ContextError::ShapeMismatch); }
        let first = self.index.partition_point(|&i| self.records[i].key_q12.as_slice() < key);
        let last = self.index.partition_point(|&i| self.records[i].key_q12.as_slice() <= key);
        if first == last { return Err(ContextError::NoWriteEvidence); }
        Ok(QueryEvidence { owner: self, indices: &self.index[first..last] })
    }
}

/// Nonempty exact-key result borrowed from one immutable memory generation.
#[derive(Debug)]
pub struct QueryEvidence<'a> {
    owner: &'a DeltaKvContextMemory,
    indices: &'a [usize],
}

impl<'a> QueryEvidence<'a> {
    pub fn key(&self) -> &'a [i16] { &self.owner.records[self.indices[0]].key_q12 }
    pub fn records(&self) -> impl ExactSizeIterator<Item = &'a WriteEvidence> + '_ {
        self.indices.iter().map(|&i| &self.owner.records[i])
    }

    /// Reads every unique source named by every matching write. The trusted
    /// reader must respect the supplied descriptor and its own I/O limits.
    /// This method checks lengths/hashes independently and binds the receipt to
    /// this query AND the exact ledger generation. No caller-supplied Verified flag.
    pub fn verify<F>(
        self,
        ledger: &'a RawContextLedger,
        limits: VerificationLimits,
        mut reader: F,
    ) -> Result<VerifiedContext<'a>, ContextError>
    where F: FnMut(&ContextSegmentRef) -> Result<Vec<u8>, RawContextStoreError> {
        if ledger.stream_id() != self.owner.stream || ledger.segments() != self.owner.ledger {
            return Err(ContextError::ContextMismatch);
        }
        if limits.max_sources == 0 || limits.max_raw_bytes == 0 {
            return Err(ContextError::InvalidLimits);
        }
        let mut ids: Vec<ContextSegmentId> = Vec::new();
        for record in self.records() {
            for &source in record.sources() {
                match ids.binary_search(&source) {
                    Ok(_) => {}
                    Err(pos) => {
                        if ids.len() >= limits.max_sources { return Err(ContextError::BudgetExceeded); }
                        ids.insert(pos, source);
                    }
                }
            }
        }
        if ids.is_empty() { return Err(ContextError::InvalidProvenance); }
        let mut total = 0_u64;
        // Admission is complete BEFORE the first read; no partial receipt escapes.
        for id in &ids {
            let i = ledger.segments().binary_search_by_key(&id.segment_id, |s| s.identity.segment_id)
                .map_err(|_| ContextError::InvalidProvenance)?;
            total = sum64(total, ledger.segments()[i].byte_len)?;
        }
        if total > limits.max_raw_bytes { return Err(ContextError::BudgetExceeded); }
        let mut sources = Vec::new();
        sources.try_reserve_exact(ids.len()).map_err(|_| ContextError::Allocation)?;
        for id in ids {
            let i = ledger.segments().binary_search_by_key(&id.segment_id, |s| s.identity.segment_id)
                .map_err(|_| ContextError::InvalidProvenance)?;
            let reference = &ledger.segments()[i];
            let raw = reader(reference)?;
            if u64::try_from(raw.len()).map_err(|_| ContextError::Overflow)? != reference.byte_len
                || sha256_raw_bytes(&raw)? != reference.content_hash
            {
                return Err(ContextError::SourceMismatch);
            }
            sources.push(VerifiedSource { reference, raw });
        }
        Ok(VerifiedContext { query: self, sources, raw_bytes: total })
    }
}

#[derive(Debug)]
pub struct VerifiedSource<'a> {
    reference: &'a ContextSegmentRef,
    raw: Vec<u8>,
}
impl VerifiedSource<'_> {
    pub fn reference(&self) -> &ContextSegmentRef { self.reference }
    pub fn bytes(&self) -> &[u8] { &self.raw }
}

/// Only QueryEvidence::verify can construct this receipt. Its borrows prevent
/// memory/ledger mutation while live; immutable bytes are retained until dropped.
/// It certifies source identity, never semantic correctness or action permission.
///
/// ```compile_fail
/// use sml_store::delta_kv_context::VerifiedContext;
/// let forged = VerifiedContext { sources: Vec::new(), raw_bytes: 0 };
/// ```
#[derive(Debug)]
pub struct VerifiedContext<'a> {
    query: QueryEvidence<'a>,
    sources: Vec<VerifiedSource<'a>>,
    raw_bytes: u64,
}
impl VerifiedContext<'_> {
    pub fn key(&self) -> &[i16] { self.query.key() }
    pub fn sources(&self) -> &[VerifiedSource<'_>] { &self.sources }
    pub fn raw_bytes(&self) -> u64 { self.raw_bytes }
    pub fn context_generation(&self) -> usize { self.query.owner.ledger.len() }
}

/// CI/conformance success is not scientific promotion. Missing evidence blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionDecision { Blocked, EligibleForIndependentReview }

#[derive(Debug, Clone, Copy)]
pub struct PromotionEvidence {
    pub bound_source_contract: bool,
    pub no_unverified_source_answers: bool,
    pub no_false_sourced_misses: bool,
    pub raw_provenance_complete: bool,
    pub matched_correctness: Option<bool>,
    pub resource_evidence_complete: bool,
    pub sbg_mor_projected_recurrence_qualified: bool,
}
impl PromotionEvidence {
    pub fn decision(self) -> PromotionDecision {
        if self.bound_source_contract && self.no_unverified_source_answers
            && self.no_false_sourced_misses && self.raw_provenance_complete
            && self.matched_correctness == Some(true) && self.resource_evidence_complete
            && self.sbg_mor_projected_recurrence_qualified
        { PromotionDecision::EligibleForIndependentReview } else { PromotionDecision::Blocked }
    }
}

fn fingerprint(values: &[i16]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for v in values { for b in v.to_le_bytes() {
        hash = (hash ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }}
    hash
}
fn add(a: usize, b: usize) -> Result<usize, ContextError> { a.checked_add(b).ok_or(ContextError::Overflow) }
fn mul(a: usize, b: usize) -> Result<usize, ContextError> { a.checked_mul(b).ok_or(ContextError::Overflow) }
fn sum64(a: u64, b: u64) -> Result<u64, ContextError> { a.checked_add(b).ok_or(ContextError::Overflow) }

#[derive(Debug)]
pub enum ContextError {
    InvalidLimits, BudgetExceeded, Overflow, Allocation, InvalidProvenance,
    ContextMismatch, ShapeMismatch, NoWriteEvidence, SourceMismatch,
    Numeric(DeltaKvError), Raw(RawContextStoreError),
}
impl From<DeltaKvError> for ContextError { fn from(e: DeltaKvError) -> Self { Self::Numeric(e) } }
impl From<RawContextStoreError> for ContextError { fn from(e: RawContextStoreError) -> Self { Self::Raw(e) } }
impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{self:?}") }
}
impl std::error::Error for ContextError {}
