use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use sml_core::context_ledger::ContextStreamId;
use sml_store::{
    context_ingest::{
        ContextIngestError, SegmentTokenCounter, StreamingContextIngestor, StreamingIngestPolicy,
    },
    raw_context::{load_raw_segment, RawContextStoreLimits},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "sml-context-ingest-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct ByteTokens;

impl SegmentTokenCounter for ByteTokens {
    fn count_segment_tokens(&mut self, raw_segment: &[u8]) -> Result<u64, ContextIngestError> {
        u64::try_from(raw_segment.len()).map_err(|_| ContextIngestError::Overflow)
    }
}

struct RejectTokens;

impl SegmentTokenCounter for RejectTokens {
    fn count_segment_tokens(&mut self, _raw_segment: &[u8]) -> Result<u64, ContextIngestError> {
        Err(ContextIngestError::TokenCounterRejected("fixture"))
    }
}

struct ChunkedReader {
    bytes: Vec<u8>,
    cursor: usize,
    max_read: usize,
}

impl ChunkedReader {
    fn new(bytes: &[u8], max_read: usize) -> Self {
        Self {
            bytes: bytes.to_vec(),
            cursor: 0,
            max_read,
        }
    }
}

impl Read for ChunkedReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.cursor == self.bytes.len() {
            return Ok(0);
        }
        let remaining = self.bytes.len() - self.cursor;
        let count = remaining.min(self.max_read).min(output.len());
        output[..count].copy_from_slice(&self.bytes[self.cursor..self.cursor + count]);
        self.cursor += count;
        Ok(count)
    }
}

fn policy(segment_bytes: usize) -> StreamingIngestPolicy {
    StreamingIngestPolicy {
        max_segment_bytes: segment_bytes,
        max_segments: 1024,
        max_total_raw_bytes: 1 << 20,
    }
}

fn limits() -> RawContextStoreLimits {
    RawContextStoreLimits {
        max_file_bytes: 1 << 20,
        max_raw_bytes: 1 << 20,
        max_semantic_links: 0,
    }
}

#[test]
fn arbitrary_reader_chunking_produces_fixed_segment_boundaries() {
    let temp = TempDir::new();
    let stream = ContextStreamId::new(7).unwrap();
    let ingestor = StreamingContextIngestor::new(stream, &temp.path, policy(4)).unwrap();
    let bytes = b"abcdefghij";
    let mut tokens = ByteTokens;

    let outcome = ingestor
        .ingest_reader(ChunkedReader::new(bytes, 3), &mut tokens)
        .unwrap();

    assert_eq!(outcome.stats.segments_published, 3);
    assert_eq!(outcome.stats.raw_bytes_published, 10);
    assert_eq!(outcome.stats.tokenizer_tokens_published, 10);
    assert_eq!(outcome.stats.peak_segment_bytes, 4);
    assert_eq!(outcome.stats.max_live_descriptors, 1);
    assert_eq!(outcome.stats.artifact_payload_bytes_bound, 152);
    assert_eq!(outcome.stats.ledger_stats().raw_bytes, 10);

    let first = load_raw_segment(ingestor.segment_path(1), limits()).unwrap();
    let second = load_raw_segment(ingestor.segment_path(2), limits()).unwrap();
    let third = load_raw_segment(ingestor.segment_path(3), limits()).unwrap();

    assert_eq!(first.raw_bytes, b"abcd");
    assert_eq!(second.raw_bytes, b"efgh");
    assert_eq!(third.raw_bytes, b"ij");
    assert_eq!((first.reference.byte_start, first.reference.byte_len), (0, 4));
    assert_eq!((second.reference.byte_start, second.reference.byte_len), (4, 4));
    assert_eq!((third.reference.byte_start, third.reference.byte_len), (8, 2));
    assert_eq!((third.reference.token_start, third.reference.token_len), (8, 2));
    assert_eq!(third.reference.order_index, 2);
    assert_eq!(outcome.last_segment, Some(third.reference));
}

#[test]
fn reader_granularity_does_not_change_artifact_bytes() {
    let left = TempDir::new();
    let right = TempDir::new();
    let stream = ContextStreamId::new(8).unwrap();
    let a = StreamingContextIngestor::new(stream, &left.path, policy(5)).unwrap();
    let b = StreamingContextIngestor::new(stream, &right.path, policy(5)).unwrap();
    let bytes = b"0123456789abcdefgh";
    let mut tokens_a = ByteTokens;
    let mut tokens_b = ByteTokens;

    let out_a = a
        .ingest_reader(ChunkedReader::new(bytes, 1), &mut tokens_a)
        .unwrap();
    let out_b = b
        .ingest_reader(ChunkedReader::new(bytes, 17), &mut tokens_b)
        .unwrap();

    assert_eq!(out_a.stats, out_b.stats);
    for segment_id in 1..=out_a.stats.segments_published {
        assert_eq!(
            fs::read(a.segment_path(segment_id)).unwrap(),
            fs::read(b.segment_path(segment_id)).unwrap()
        );
    }
}

#[test]
fn empty_stream_publishes_no_artifact_and_reports_zero_live_descriptors() {
    let temp = TempDir::new();
    let ingestor =
        StreamingContextIngestor::new(ContextStreamId::new(9).unwrap(), &temp.path, policy(8))
            .unwrap();
    let mut tokens = ByteTokens;
    let outcome = ingestor.ingest_reader(&b""[..], &mut tokens).unwrap();

    assert_eq!(outcome.stats.segments_published, 0);
    assert_eq!(outcome.stats.raw_bytes_published, 0);
    assert_eq!(outcome.stats.peak_segment_bytes, 0);
    assert_eq!(outcome.stats.max_live_descriptors, 0);
    assert!(outcome.last_segment.is_none());
    assert_eq!(fs::read_dir(&temp.path).unwrap().count(), 0);
}

#[test]
fn segment_budget_fails_before_publishing_the_excess_segment() {
    let temp = TempDir::new();
    let policy = StreamingIngestPolicy {
        max_segment_bytes: 4,
        max_segments: 2,
        max_total_raw_bytes: 100,
    };
    let ingestor =
        StreamingContextIngestor::new(ContextStreamId::new(10).unwrap(), &temp.path, policy)
            .unwrap();
    let mut tokens = ByteTokens;

    assert_eq!(
        ingestor.ingest_reader(&b"abcdefghij"[..], &mut tokens),
        Err(ContextIngestError::SegmentBudget)
    );
    assert!(ingestor.segment_path(1).exists());
    assert!(ingestor.segment_path(2).exists());
    assert!(!ingestor.segment_path(3).exists());
}

#[test]
fn total_raw_byte_budget_fails_before_publishing_excess_bytes() {
    let temp = TempDir::new();
    let policy = StreamingIngestPolicy {
        max_segment_bytes: 4,
        max_segments: 10,
        max_total_raw_bytes: 6,
    };
    let ingestor =
        StreamingContextIngestor::new(ContextStreamId::new(11).unwrap(), &temp.path, policy)
            .unwrap();
    let mut tokens = ByteTokens;

    assert_eq!(
        ingestor.ingest_reader(&b"abcdefgh"[..], &mut tokens),
        Err(ContextIngestError::RawByteBudget {
            actual: 8,
            maximum: 6,
        })
    );
    assert!(ingestor.segment_path(1).exists());
    assert!(!ingestor.segment_path(2).exists());
}

#[test]
fn token_counter_rejection_occurs_before_artifact_publication() {
    let temp = TempDir::new();
    let ingestor =
        StreamingContextIngestor::new(ContextStreamId::new(12).unwrap(), &temp.path, policy(4))
            .unwrap();
    let mut tokens = RejectTokens;

    assert_eq!(
        ingestor.ingest_reader(&b"abcd"[..], &mut tokens),
        Err(ContextIngestError::TokenCounterRejected("fixture"))
    );
    assert_eq!(fs::read_dir(&temp.path).unwrap().count(), 0);
}

#[test]
fn immutable_target_names_prevent_silent_stream_reingest_overwrite() {
    let temp = TempDir::new();
    let ingestor =
        StreamingContextIngestor::new(ContextStreamId::new(13).unwrap(), &temp.path, policy(4))
            .unwrap();
    let mut first = ByteTokens;
    ingestor.ingest_reader(&b"abcd"[..], &mut first).unwrap();
    let before = fs::read(ingestor.segment_path(1)).unwrap();

    let mut second = ByteTokens;
    assert!(matches!(
        ingestor.ingest_reader(&b"WXYZ"[..], &mut second),
        Err(ContextIngestError::Store(_))
    ));
    assert_eq!(fs::read(ingestor.segment_path(1)).unwrap(), before);
}

#[test]
fn working_payload_bound_is_constant_in_total_stream_length() {
    let small = policy(4096);
    let huge_total = StreamingIngestPolicy {
        max_segment_bytes: 4096,
        max_segments: 1_000_000,
        max_total_raw_bytes: u64::MAX / 2,
    };

    assert_eq!(
        small.working_payload_bytes_bound().unwrap(),
        huge_total.working_payload_bytes_bound().unwrap()
    );
    assert_eq!(small.working_payload_bytes_bound().unwrap(), 8336);
}

#[test]
fn invalid_policy_and_non_directory_output_fail_before_reading() {
    let temp = TempDir::new();
    assert_eq!(
        StreamingContextIngestor::new(
            ContextStreamId::new(14).unwrap(),
            &temp.path,
            StreamingIngestPolicy {
                max_segment_bytes: 0,
                max_segments: 1,
                max_total_raw_bytes: 1,
            },
        ),
        Err(ContextIngestError::InvalidPolicy)
    );

    let file = temp.path.join("not-a-dir");
    fs::write(&file, b"x").unwrap();
    assert_eq!(
        StreamingContextIngestor::new(ContextStreamId::new(14).unwrap(), &file, policy(4)),
        Err(ContextIngestError::OutputNotDirectory)
    );
}
