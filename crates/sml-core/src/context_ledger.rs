//! CONTEXT-0 immutable raw-segment identity and provenance contract.
//!
//! This module is dependency-free and owns identities/accounting only. It does
//! not hash bytes itself, persist files, tokenize text, or claim an effective
//! context length. A caller supplies the SHA-256 digest produced by its admitted
//! ingest path.

use core::fmt;
use std::collections::BTreeSet;

/// Version of the CONTEXT-0 segment/provenance contract.
pub const CONTEXT_SEGMENT_SCHEMA_VERSION: u16 = 1;

/// Cryptographic content-hash algorithm frozen by CONTEXT-0.
pub const CONTEXT_CONTENT_HASH_ALGORITHM: &str = "sha256";

/// Stable stream identifier. Zero is reserved as an invalid/uninitialized value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContextStreamId(u64);

impl ContextStreamId {
    pub fn new(value: u64) -> Result<Self, ContextLedgerError> {
        if value == 0 {
            return Err(ContextLedgerError::ZeroStreamId);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Stable segment identity inside one stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContextSegmentId {
    pub stream_id: ContextStreamId,
    pub segment_id: u64,
}

impl ContextSegmentId {
    pub fn new(stream_id: ContextStreamId, segment_id: u64) -> Result<Self, ContextLedgerError> {
        if segment_id == 0 {
            return Err(ContextLedgerError::ZeroSegmentId);
        }
        Ok(Self {
            stream_id,
            segment_id,
        })
    }
}

/// Caller-supplied SHA-256 digest of the exact immutable raw segment bytes.
///
/// CONTEXT-0 deliberately separates cryptographic source identity from CRC-style
/// corruption detection used elsewhere in the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContextContentHash([u8; 32]);

impl ContextContentHash {
    #[must_use]
    pub const fn from_sha256(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn hex(&self) -> String {
        let mut output = String::with_capacity(64);
        for byte in self.0 {
            use core::fmt::Write as _;
            let _ = write!(output, "{byte:02x}");
        }
        output
    }
}

/// Immutable raw-context segment descriptor.
///
/// Byte and token spans are half-open. Semantic record IDs are canonical
/// ascending/unique metadata only; raw bytes remain authoritative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSegmentRef {
    pub identity: ContextSegmentId,
    pub byte_start: u64,
    pub byte_len: u64,
    pub token_start: u64,
    pub token_len: u64,
    pub order_index: u64,
    pub content_hash: ContextContentHash,
    semantic_record_ids: Vec<u64>,
}

impl ContextSegmentRef {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        identity: ContextSegmentId,
        byte_start: u64,
        byte_len: u64,
        token_start: u64,
        token_len: u64,
        order_index: u64,
        content_hash: ContextContentHash,
        mut semantic_record_ids: Vec<u64>,
    ) -> Result<Self, ContextLedgerError> {
        if byte_len == 0 {
            return Err(ContextLedgerError::ZeroByteLength);
        }
        byte_start
            .checked_add(byte_len)
            .ok_or(ContextLedgerError::SpanOverflow)?;
        token_start
            .checked_add(token_len)
            .ok_or(ContextLedgerError::SpanOverflow)?;

        semantic_record_ids.sort_unstable();
        for pair in semantic_record_ids.windows(2) {
            if pair[0] == pair[1] {
                return Err(ContextLedgerError::DuplicateSemanticRecordId(pair[0]));
            }
        }

        Ok(Self {
            identity,
            byte_start,
            byte_len,
            token_start,
            token_len,
            order_index,
            content_hash,
            semantic_record_ids,
        })
    }

    #[must_use]
    pub fn semantic_record_ids(&self) -> &[u64] {
        &self.semantic_record_ids
    }

    pub fn byte_end(&self) -> Result<u64, ContextLedgerError> {
        self.byte_start
            .checked_add(self.byte_len)
            .ok_or(ContextLedgerError::SpanOverflow)
    }

    pub fn token_end(&self) -> Result<u64, ContextLedgerError> {
        self.token_start
            .checked_add(self.token_len)
            .ok_or(ContextLedgerError::SpanOverflow)
    }
}

/// Reverse provenance from one semantic record to one or more immutable sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticRecordProvenance {
    pub semantic_record_id: u64,
    source_segments: Vec<ContextSegmentId>,
}

impl SemanticRecordProvenance {
    pub fn new(
        semantic_record_id: u64,
        mut source_segments: Vec<ContextSegmentId>,
    ) -> Result<Self, ContextLedgerError> {
        if source_segments.is_empty() {
            return Err(ContextLedgerError::EmptySemanticProvenance);
        }
        source_segments.sort_unstable();
        for pair in source_segments.windows(2) {
            if pair[0] == pair[1] {
                return Err(ContextLedgerError::DuplicateSourceSegment(pair[0]));
            }
        }
        Ok(Self {
            semantic_record_id,
            source_segments,
        })
    }

    #[must_use]
    pub fn source_segments(&self) -> &[ContextSegmentId] {
        &self.source_segments
    }
}

/// Exact logical accounting for one immutable stream ledger.
///
/// These counters are not RSS, mapped bytes, disk I/O, or allocator evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RawContextLedgerStats {
    pub segments: u64,
    pub raw_bytes: u64,
    pub tokenizer_tokens: u64,
    pub semantic_record_links: u64,
}

/// Append-only in-memory reference ledger for one raw context stream.
///
/// CONTEXT-0 requires contiguous byte/token spans and monotonic order. The
/// persistence layer introduced later must preserve these semantics exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawContextLedger {
    stream_id: ContextStreamId,
    segments: Vec<ContextSegmentRef>,
    stats: RawContextLedgerStats,
}

impl RawContextLedger {
    #[must_use]
    pub const fn new(stream_id: ContextStreamId) -> Self {
        Self {
            stream_id,
            segments: Vec::new(),
            stats: RawContextLedgerStats {
                segments: 0,
                raw_bytes: 0,
                tokenizer_tokens: 0,
                semantic_record_links: 0,
            },
        }
    }

    #[must_use]
    pub const fn stream_id(&self) -> ContextStreamId {
        self.stream_id
    }

    #[must_use]
    pub fn segments(&self) -> &[ContextSegmentRef] {
        &self.segments
    }

    #[must_use]
    pub const fn stats(&self) -> RawContextLedgerStats {
        self.stats
    }

    pub fn append(&mut self, segment: ContextSegmentRef) -> Result<(), ContextLedgerError> {
        if segment.identity.stream_id != self.stream_id {
            return Err(ContextLedgerError::StreamMismatch {
                expected: self.stream_id,
                actual: segment.identity.stream_id,
            });
        }

        match self.segments.last() {
            None => {
                if segment.byte_start != 0 || segment.token_start != 0 || segment.order_index != 0 {
                    return Err(ContextLedgerError::InvalidFirstSegmentOrigin {
                        byte_start: segment.byte_start,
                        token_start: segment.token_start,
                        order_index: segment.order_index,
                    });
                }
            }
            Some(previous) => {
                let expected_byte_start = previous.byte_end()?;
                let expected_token_start = previous.token_end()?;
                let expected_order = previous
                    .order_index
                    .checked_add(1)
                    .ok_or(ContextLedgerError::SpanOverflow)?;
                if segment.identity.segment_id <= previous.identity.segment_id {
                    return Err(ContextLedgerError::NonIncreasingSegmentId {
                        previous: previous.identity.segment_id,
                        current: segment.identity.segment_id,
                    });
                }
                if segment.byte_start != expected_byte_start
                    || segment.token_start != expected_token_start
                    || segment.order_index != expected_order
                {
                    return Err(ContextLedgerError::NonContiguousSegment {
                        expected_byte_start,
                        actual_byte_start: segment.byte_start,
                        expected_token_start,
                        actual_token_start: segment.token_start,
                        expected_order,
                        actual_order: segment.order_index,
                    });
                }
            }
        }

        let link_count = u64::try_from(segment.semantic_record_ids.len())
            .map_err(|_| ContextLedgerError::AccountingOverflow)?;
        let next_stats = RawContextLedgerStats {
            segments: self
                .stats
                .segments
                .checked_add(1)
                .ok_or(ContextLedgerError::AccountingOverflow)?,
            raw_bytes: self
                .stats
                .raw_bytes
                .checked_add(segment.byte_len)
                .ok_or(ContextLedgerError::AccountingOverflow)?,
            tokenizer_tokens: self
                .stats
                .tokenizer_tokens
                .checked_add(segment.token_len)
                .ok_or(ContextLedgerError::AccountingOverflow)?,
            semantic_record_links: self
                .stats
                .semantic_record_links
                .checked_add(link_count)
                .ok_or(ContextLedgerError::AccountingOverflow)?,
        };
        self.segments.push(segment);
        self.stats = next_stats;
        Ok(())
    }

    /// Validate one reverse provenance record against currently admitted segments.
    pub fn validate_provenance(
        &self,
        provenance: &SemanticRecordProvenance,
    ) -> Result<(), ContextLedgerError> {
        let admitted = self
            .segments
            .iter()
            .map(|segment| segment.identity)
            .collect::<BTreeSet<_>>();
        for source in provenance.source_segments() {
            if source.stream_id != self.stream_id || !admitted.contains(source) {
                return Err(ContextLedgerError::UnknownSourceSegment(*source));
            }
        }

        let forward_sources = self
            .segments
            .iter()
            .filter(|segment| {
                segment
                    .semantic_record_ids()
                    .binary_search(&provenance.semantic_record_id)
                    .is_ok()
            })
            .map(|segment| segment.identity)
            .collect::<BTreeSet<_>>();
        let reverse_sources = provenance
            .source_segments()
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if forward_sources != reverse_sources {
            return Err(ContextLedgerError::ProvenanceMismatch {
                semantic_record_id: provenance.semantic_record_id,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContextLedgerError {
    ZeroStreamId,
    ZeroSegmentId,
    ZeroByteLength,
    SpanOverflow,
    DuplicateSemanticRecordId(u64),
    EmptySemanticProvenance,
    DuplicateSourceSegment(ContextSegmentId),
    StreamMismatch {
        expected: ContextStreamId,
        actual: ContextStreamId,
    },
    InvalidFirstSegmentOrigin {
        byte_start: u64,
        token_start: u64,
        order_index: u64,
    },
    NonIncreasingSegmentId {
        previous: u64,
        current: u64,
    },
    NonContiguousSegment {
        expected_byte_start: u64,
        actual_byte_start: u64,
        expected_token_start: u64,
        actual_token_start: u64,
        expected_order: u64,
        actual_order: u64,
    },
    AccountingOverflow,
    UnknownSourceSegment(ContextSegmentId),
    ProvenanceMismatch {
        semantic_record_id: u64,
    },
}

impl fmt::Display for ContextLedgerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ContextLedgerError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: u8) -> ContextContentHash {
        ContextContentHash::from_sha256([byte; 32])
    }

    fn segment(
        stream: ContextStreamId,
        segment_id: u64,
        byte_span: (u64, u64),
        token_span: (u64, u64),
        order_index: u64,
        semantic_ids: &[u64],
    ) -> ContextSegmentRef {
        ContextSegmentRef::new(
            ContextSegmentId::new(stream, segment_id).unwrap(),
            byte_span.0,
            byte_span.1,
            token_span.0,
            token_span.1,
            order_index,
            digest(segment_id as u8),
            semantic_ids.to_vec(),
        )
        .unwrap()
    }

    #[test]
    fn append_tracks_exact_contiguous_spans() {
        let stream = ContextStreamId::new(9).unwrap();
        let mut ledger = RawContextLedger::new(stream);
        ledger
            .append(segment(stream, 10, (0, 11), (0, 3), 0, &[7]))
            .unwrap();
        ledger
            .append(segment(stream, 20, (11, 5), (3, 2), 1, &[8, 9]))
            .unwrap();

        assert_eq!(
            ledger.stats(),
            RawContextLedgerStats {
                segments: 2,
                raw_bytes: 16,
                tokenizer_tokens: 5,
                semantic_record_links: 3,
            }
        );
        assert_eq!(ledger.segments()[1].byte_end().unwrap(), 16);
        assert_eq!(ledger.segments()[1].token_end().unwrap(), 5);
    }

    #[test]
    fn gaps_overlaps_and_reordering_fail_closed() {
        let stream = ContextStreamId::new(1).unwrap();
        let first = segment(stream, 10, (0, 4), (0, 2), 0, &[]);
        let mut ledger = RawContextLedger::new(stream);
        ledger.append(first).unwrap();

        assert!(matches!(
            ledger.append(segment(stream, 11, (5, 2), (2, 1), 1, &[])),
            Err(ContextLedgerError::NonContiguousSegment { .. })
        ));
        assert!(matches!(
            ledger.append(segment(stream, 9, (4, 2), (2, 1), 1, &[])),
            Err(ContextLedgerError::NonIncreasingSegmentId {
                previous: 10,
                current: 9
            })
        ));
    }

    #[test]
    fn first_segment_must_start_at_stream_origin() {
        let stream = ContextStreamId::new(2).unwrap();
        let mut ledger = RawContextLedger::new(stream);
        assert!(matches!(
            ledger.append(segment(stream, 1, (1, 2), (0, 1), 0, &[])),
            Err(ContextLedgerError::InvalidFirstSegmentOrigin { .. })
        ));
    }

    #[test]
    fn reverse_provenance_must_exactly_match_forward_links() {
        let stream = ContextStreamId::new(4).unwrap();
        let mut ledger = RawContextLedger::new(stream);
        let first_id = ContextSegmentId::new(stream, 1).unwrap();
        let second_id = ContextSegmentId::new(stream, 2).unwrap();
        ledger
            .append(segment(stream, 1, (0, 3), (0, 1), 0, &[77]))
            .unwrap();
        ledger
            .append(segment(stream, 2, (3, 3), (1, 1), 1, &[77, 88]))
            .unwrap();

        let correct = SemanticRecordProvenance::new(77, vec![first_id, second_id]).unwrap();
        ledger.validate_provenance(&correct).unwrap();

        let incomplete = SemanticRecordProvenance::new(77, vec![first_id]).unwrap();
        assert_eq!(
            ledger.validate_provenance(&incomplete),
            Err(ContextLedgerError::ProvenanceMismatch {
                semantic_record_id: 77
            })
        );
    }

    #[test]
    fn segment_semantic_ids_are_canonical_and_duplicate_safe() {
        let stream = ContextStreamId::new(5).unwrap();
        let reference = segment(stream, 1, (0, 1), (0, 0), 0, &[9, 3, 7]);
        assert_eq!(reference.semantic_record_ids(), &[3, 7, 9]);

        assert_eq!(
            ContextSegmentRef::new(
                ContextSegmentId::new(stream, 2).unwrap(),
                1,
                1,
                0,
                0,
                1,
                digest(2),
                vec![3, 3],
            ),
            Err(ContextLedgerError::DuplicateSemanticRecordId(3))
        );
    }

    #[test]
    fn failed_append_leaves_ledger_unchanged() {
        let stream = ContextStreamId::new(6).unwrap();
        let mut ledger = RawContextLedger::new(stream);
        ledger
            .append(segment(stream, 1, (0, 3), (0, 1), 0, &[1]))
            .unwrap();
        let before = ledger.clone();

        assert!(ledger
            .append(segment(stream, 2, (4, 1), (1, 1), 1, &[2]))
            .is_err());
        assert_eq!(ledger, before);
    }

    #[test]
    fn content_hash_hex_is_stable_and_separate_from_crc() {
        let hash = ContextContentHash::from_sha256([0xab; 32]);
        assert_eq!(hash.hex(), "ab".repeat(32));
        assert_eq!(CONTEXT_CONTENT_HASH_ALGORITHM, "sha256");
    }
}
