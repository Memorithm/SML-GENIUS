//! CONTEXT-1b bounded streaming ingest.
//!
//! The ingest path keeps one raw segment buffer plus one serialized artifact at
//! a time. It never retains the complete stream or an in-memory vector of all
//! segment descriptors. Token accounting is delegated to a frozen caller-owned
//! segment counter; this module does not pretend to implement the future model
//! tokenizer.

use std::{
    fmt, fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use sml_core::context_ledger::{
    ContextLedgerError, ContextSegmentId, ContextSegmentRef, ContextStreamId, RawContextLedgerStats,
};

use crate::raw_context::{
    publish_raw_segment, sha256_raw_bytes, RawContextStoreError, RAW_CONTEXT_HEADER_BYTES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamingIngestPolicy {
    pub max_segment_bytes: usize,
    pub max_segments: u64,
    pub max_total_raw_bytes: u64,
}

impl StreamingIngestPolicy {
    pub fn validate(self) -> Result<Self, ContextIngestError> {
        if self.max_segment_bytes == 0 || self.max_segments == 0 || self.max_total_raw_bytes == 0 {
            return Err(ContextIngestError::InvalidPolicy);
        }
        Ok(self)
    }

    /// Conservative payload bound excluding allocator/filesystem overhead.
    ///
    /// The raw publisher currently serializes one artifact in addition to the
    /// live segment buffer. CONTEXT-1b therefore reports both explicitly rather
    /// than claiming a one-segment RSS ceiling.
    pub fn working_payload_bytes_bound(self) -> Result<u64, ContextIngestError> {
        let segment =
            u64::try_from(self.max_segment_bytes).map_err(|_| ContextIngestError::Overflow)?;
        segment
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(u64::from(RAW_CONTEXT_HEADER_BYTES)))
            .ok_or(ContextIngestError::Overflow)
    }
}

pub trait SegmentTokenCounter {
    /// Returns the exact token count for this immutable segment under the
    /// caller's frozen segment-tokenization contract.
    fn count_segment_tokens(&mut self, raw_segment: &[u8]) -> Result<u64, ContextIngestError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamingIngestStats {
    pub segments_published: u64,
    pub raw_bytes_published: u64,
    pub tokenizer_tokens_published: u64,
    pub peak_segment_bytes: u64,
    pub max_live_descriptors: u64,
    pub artifact_payload_bytes_bound: u64,
}

impl StreamingIngestStats {
    #[must_use]
    pub const fn ledger_stats(self) -> RawContextLedgerStats {
        RawContextLedgerStats {
            segments: self.segments_published,
            raw_bytes: self.raw_bytes_published,
            tokenizer_tokens: self.tokenizer_tokens_published,
            semantic_record_links: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamingIngestOutcome {
    pub stream_id: ContextStreamId,
    pub stats: StreamingIngestStats,
    pub last_segment: Option<ContextSegmentRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamingContextIngestor {
    stream_id: ContextStreamId,
    output_directory: PathBuf,
    policy: StreamingIngestPolicy,
}

impl StreamingContextIngestor {
    pub fn new(
        stream_id: ContextStreamId,
        output_directory: impl AsRef<Path>,
        policy: StreamingIngestPolicy,
    ) -> Result<Self, ContextIngestError> {
        let policy = policy.validate()?;
        let output_directory = output_directory.as_ref().to_path_buf();
        let metadata = fs::metadata(&output_directory)?;
        if !metadata.is_dir() {
            return Err(ContextIngestError::OutputNotDirectory);
        }
        Ok(Self {
            stream_id,
            output_directory,
            policy,
        })
    }

    #[must_use]
    pub const fn stream_id(&self) -> ContextStreamId {
        self.stream_id
    }

    #[must_use]
    pub const fn policy(&self) -> StreamingIngestPolicy {
        self.policy
    }

    pub fn ingest_reader<R: Read, T: SegmentTokenCounter>(
        &self,
        mut reader: R,
        token_counter: &mut T,
    ) -> Result<StreamingIngestOutcome, ContextIngestError> {
        let mut buffer = vec![0_u8; self.policy.max_segment_bytes];
        let mut filled = 0_usize;
        let mut byte_start = 0_u64;
        let mut token_start = 0_u64;
        let mut order_index = 0_u64;
        let mut next_segment_id = 1_u64;
        let mut last_segment = None;
        let mut stats = StreamingIngestStats {
            artifact_payload_bytes_bound: self.policy.working_payload_bytes_bound()?,
            ..StreamingIngestStats::default()
        };

        loop {
            let read = reader.read(&mut buffer[filled..])?;
            if read == 0 {
                if filled != 0 {
                    self.publish_buffer(
                        &buffer[..filled],
                        token_counter,
                        &mut byte_start,
                        &mut token_start,
                        &mut order_index,
                        &mut next_segment_id,
                        &mut stats,
                        &mut last_segment,
                    )?;
                }
                break;
            }
            filled = filled
                .checked_add(read)
                .ok_or(ContextIngestError::Overflow)?;
            if filled == buffer.len() {
                self.publish_buffer(
                    &buffer,
                    token_counter,
                    &mut byte_start,
                    &mut token_start,
                    &mut order_index,
                    &mut next_segment_id,
                    &mut stats,
                    &mut last_segment,
                )?;
                filled = 0;
            }
        }

        Ok(StreamingIngestOutcome {
            stream_id: self.stream_id,
            stats,
            last_segment,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_buffer<T: SegmentTokenCounter>(
        &self,
        raw: &[u8],
        token_counter: &mut T,
        byte_start: &mut u64,
        token_start: &mut u64,
        order_index: &mut u64,
        next_segment_id: &mut u64,
        stats: &mut StreamingIngestStats,
        last_segment: &mut Option<ContextSegmentRef>,
    ) -> Result<(), ContextIngestError> {
        if raw.is_empty() {
            return Err(ContextIngestError::InternalInvariant);
        }
        if stats.segments_published >= self.policy.max_segments {
            return Err(ContextIngestError::SegmentBudget);
        }

        let raw_len = u64::try_from(raw.len()).map_err(|_| ContextIngestError::Overflow)?;
        let next_total = stats
            .raw_bytes_published
            .checked_add(raw_len)
            .ok_or(ContextIngestError::Overflow)?;
        if next_total > self.policy.max_total_raw_bytes {
            return Err(ContextIngestError::RawByteBudget {
                actual: next_total,
                maximum: self.policy.max_total_raw_bytes,
            });
        }

        let token_len = token_counter.count_segment_tokens(raw)?;
        let content_hash = sha256_raw_bytes(raw)?;
        let identity = ContextSegmentId::new(self.stream_id, *next_segment_id)?;
        let reference = ContextSegmentRef::new(
            identity,
            *byte_start,
            raw_len,
            *token_start,
            token_len,
            *order_index,
            content_hash,
            Vec::new(),
        )?;

        if let Some(previous) = last_segment.as_ref() {
            let expected_byte_start = previous.byte_end()?;
            let expected_token_start = previous.token_end()?;
            let expected_order = previous
                .order_index
                .checked_add(1)
                .ok_or(ContextIngestError::Overflow)?;
            if reference.byte_start != expected_byte_start
                || reference.token_start != expected_token_start
                || reference.order_index != expected_order
                || reference.identity.segment_id <= previous.identity.segment_id
            {
                return Err(ContextIngestError::InternalInvariant);
            }
        } else if reference.byte_start != 0
            || reference.token_start != 0
            || reference.order_index != 0
        {
            return Err(ContextIngestError::InternalInvariant);
        }

        let path = self.segment_path(reference.identity.segment_id);
        publish_raw_segment(&path, &reference, raw)?;

        stats.segments_published = stats.segments_published.saturating_add(1);
        stats.raw_bytes_published = next_total;
        stats.tokenizer_tokens_published = stats
            .tokenizer_tokens_published
            .checked_add(token_len)
            .ok_or(ContextIngestError::Overflow)?;
        stats.peak_segment_bytes = stats.peak_segment_bytes.max(raw_len);
        stats.max_live_descriptors = 1;

        *byte_start = reference.byte_end()?;
        *token_start = reference.token_end()?;
        *order_index = order_index
            .checked_add(1)
            .ok_or(ContextIngestError::Overflow)?;
        *next_segment_id = next_segment_id
            .checked_add(1)
            .ok_or(ContextIngestError::Overflow)?;
        *last_segment = Some(reference);
        Ok(())
    }

    #[must_use]
    pub fn segment_path(&self, segment_id: u64) -> PathBuf {
        self.output_directory.join(format!(
            "stream-{:016x}-segment-{segment_id:016x}.smlctx",
            self.stream_id.get()
        ))
    }
}

#[derive(Debug)]
pub enum ContextIngestError {
    InvalidPolicy,
    OutputNotDirectory,
    SegmentBudget,
    RawByteBudget { actual: u64, maximum: u64 },
    TokenCounterRejected(&'static str),
    Overflow,
    InternalInvariant,
    Io(io::ErrorKind),
    Ledger(ContextLedgerError),
    Store(RawContextStoreError),
}

impl PartialEq for ContextIngestError {
    fn eq(&self, other: &Self) -> bool {
        use ContextIngestError as E;
        match (self, other) {
            (E::InvalidPolicy, E::InvalidPolicy)
            | (E::OutputNotDirectory, E::OutputNotDirectory)
            | (E::SegmentBudget, E::SegmentBudget)
            | (E::Overflow, E::Overflow)
            | (E::InternalInvariant, E::InternalInvariant) => true,
            (
                E::RawByteBudget {
                    actual: a1,
                    maximum: m1,
                },
                E::RawByteBudget {
                    actual: a2,
                    maximum: m2,
                },
            ) => a1 == a2 && m1 == m2,
            (E::TokenCounterRejected(a), E::TokenCounterRejected(b)) => a == b,
            (E::Io(a), E::Io(b)) => a == b,
            (E::Ledger(a), E::Ledger(b)) => a == b,
            (E::Store(a), E::Store(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ContextIngestError {}

impl From<io::Error> for ContextIngestError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

impl From<ContextLedgerError> for ContextIngestError {
    fn from(error: ContextLedgerError) -> Self {
        Self::Ledger(error)
    }
}

impl From<RawContextStoreError> for ContextIngestError {
    fn from(error: RawContextStoreError) -> Self {
        Self::Store(error)
    }
}

impl fmt::Display for ContextIngestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ContextIngestError {}
