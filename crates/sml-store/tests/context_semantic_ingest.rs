use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use sml_core::{
    context_ledger::ContextStreamId,
    semantic::{SemanticState, SemanticStoreView},
};
use sml_store::{
    context_ingest::{ContextIngestError, SegmentTokenCounter, StreamingIngestPolicy},
    context_semantic_ingest::{
        parse_provenance_artifact, ContextSemanticIngestError, SemanticIngestPolicy,
        SemanticSegmentEmitter, SemanticStreamingContextIngestor, SegmentEmissionContext,
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
            "sml-context-semantic-{}-{id}",
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

struct DeterministicEmitter;

impl SemanticSegmentEmitter<8> for DeterministicEmitter {
    fn emit_segment(
        &mut self,
        raw_segment: &[u8],
        context: SegmentEmissionContext,
    ) -> Result<Vec<(u64, SemanticState<8>)>, ContextSemanticIngestError> {
        let mut state = SemanticState::<8>::zeroed().unwrap();
        state.value.set(0, raw_segment[0] & 1 != 0).unwrap();
        state.value.set(1, raw_segment[raw_segment.len() - 1] & 1 != 0).unwrap();
        state.meaning_mask.set(0, true).unwrap();
        state.meaning_mask.set(1, true).unwrap();
        state.routing
            .set((context.order_index as usize) % 8, true)
            .unwrap();
        Ok(vec![(1000 + context.segment_id, state)])
    }
}

struct DuplicateEmitter;

impl SemanticSegmentEmitter<8> for DuplicateEmitter {
    fn emit_segment(
        &mut self,
        _raw_segment: &[u8],
        _context: SegmentEmissionContext,
    ) -> Result<Vec<(u64, SemanticState<8>)>, ContextSemanticIngestError> {
        Ok(vec![
            (7, SemanticState::<8>::zeroed().unwrap()),
            (7, SemanticState::<8>::zeroed().unwrap()),
        ])
    }
}

struct EmptyEmitter;

impl SemanticSegmentEmitter<8> for EmptyEmitter {
    fn emit_segment(
        &mut self,
        _raw_segment: &[u8],
        _context: SegmentEmissionContext,
    ) -> Result<Vec<(u64, SemanticState<8>)>, ContextSemanticIngestError> {
        Ok(Vec::new())
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
        let count = (self.bytes.len() - self.cursor)
            .min(self.max_read)
            .min(output.len());
        output[..count].copy_from_slice(&self.bytes[self.cursor..self.cursor + count]);
        self.cursor += count;
        Ok(count)
    }
}

fn raw_policy() -> StreamingIngestPolicy {
    StreamingIngestPolicy {
        max_segment_bytes: 4,
        max_segments: 32,
        max_total_raw_bytes: 1024,
    }
}

fn semantic_policy() -> SemanticIngestPolicy {
    SemanticIngestPolicy {
        max_records_per_segment: 4,
        max_semantic_store_bytes: 1 << 20,
        max_provenance_bytes: 4096,
    }
}

fn raw_limits() -> RawContextStoreLimits {
    RawContextStoreLimits {
        max_file_bytes: 1 << 20,
        max_raw_bytes: 1024,
        max_semantic_links: 4,
    }
}

#[test]
fn raw_semantic_and_provenance_artifacts_share_the_same_semantic_identity() {
    let temp = TempDir::new();
    let stream = ContextStreamId::new(21).unwrap();
    let ingestor = SemanticStreamingContextIngestor::<8>::new(
        stream,
        &temp.path,
        raw_policy(),
        semantic_policy(),
    )
    .unwrap();
    let mut tokens = ByteTokens;
    let mut emitter = DeterministicEmitter;

    let outcome = ingestor
        .ingest_reader(&b"abcdefghij"[..], &mut tokens, &mut emitter)
        .unwrap();

    assert_eq!(outcome.stats.segments_published, 3);
    assert_eq!(outcome.stats.semantic_records_published, 3);
    assert_eq!(outcome.stats.raw_bytes_published, 10);
    assert_eq!(outcome.stats.tokenizer_tokens_published, 10);

    for segment_id in 1..=3 {
        let raw = load_raw_segment(ingestor.raw_path(segment_id), raw_limits()).unwrap();
        let expected_id = 1000 + segment_id;
        assert_eq!(raw.reference.semantic_record_ids(), &[expected_id]);

        let semantic_bytes = fs::read(ingestor.semantic_path(segment_id)).unwrap();
        let view = SemanticStoreView::<8>::parse(&semantic_bytes).unwrap();
        assert_eq!(view.record_count(), 1);
        assert!(view.get(expected_id).unwrap().is_some());

        let provenance_bytes = fs::read(ingestor.provenance_path(segment_id)).unwrap();
        let provenance = parse_provenance_artifact(&provenance_bytes).unwrap();
        assert_eq!(provenance.stream_id, stream);
        assert_eq!(provenance.segment_id, segment_id);
        assert_eq!(provenance.content_hash, raw.reference.content_hash);
        assert_eq!(provenance.semantic_record_ids, vec![expected_id]);
    }
}

#[test]
fn reader_chunk_size_does_not_change_any_bundle_artifact() {
    let left = TempDir::new();
    let right = TempDir::new();
    let stream = ContextStreamId::new(22).unwrap();
    let a = SemanticStreamingContextIngestor::<8>::new(
        stream,
        &left.path,
        raw_policy(),
        semantic_policy(),
    )
    .unwrap();
    let b = SemanticStreamingContextIngestor::<8>::new(
        stream,
        &right.path,
        raw_policy(),
        semantic_policy(),
    )
    .unwrap();
    let bytes = b"abcdefghijklm";
    let mut ta = ByteTokens;
    let mut tb = ByteTokens;
    let mut ea = DeterministicEmitter;
    let mut eb = DeterministicEmitter;

    let oa = a
        .ingest_reader(ChunkedReader::new(bytes, 1), &mut ta, &mut ea)
        .unwrap();
    let ob = b
        .ingest_reader(ChunkedReader::new(bytes, 11), &mut tb, &mut eb)
        .unwrap();

    assert_eq!(oa.stats, ob.stats);
    for segment_id in 1..=oa.stats.segments_published {
        assert_eq!(
            fs::read(a.raw_path(segment_id)).unwrap(),
            fs::read(b.raw_path(segment_id)).unwrap()
        );
        assert_eq!(
            fs::read(a.semantic_path(segment_id)).unwrap(),
            fs::read(b.semantic_path(segment_id)).unwrap()
        );
        assert_eq!(
            fs::read(a.provenance_path(segment_id)).unwrap(),
            fs::read(b.provenance_path(segment_id)).unwrap()
        );
    }
}

#[test]
fn duplicate_or_empty_semantic_emission_fails_before_publication() {
    let temp = TempDir::new();
    let ingestor = SemanticStreamingContextIngestor::<8>::new(
        ContextStreamId::new(23).unwrap(),
        &temp.path,
        raw_policy(),
        semantic_policy(),
    )
    .unwrap();
    let mut tokens = ByteTokens;
    let mut duplicate = DuplicateEmitter;

    assert!(matches!(
        ingestor.ingest_reader(&b"abcd"[..], &mut tokens, &mut duplicate),
        Err(ContextSemanticIngestError::DuplicateSemanticId(7))
    ));
    assert_eq!(fs::read_dir(&temp.path).unwrap().count(), 0);

    let mut tokens = ByteTokens;
    let mut empty = EmptyEmitter;
    assert!(matches!(
        ingestor.ingest_reader(&b"abcd"[..], &mut tokens, &mut empty),
        Err(ContextSemanticIngestError::EmptySemanticEmission)
    ));
    assert_eq!(fs::read_dir(&temp.path).unwrap().count(), 0);
}

#[test]
fn semantic_store_budget_fails_before_any_bundle_file_is_published() {
    let temp = TempDir::new();
    let policy = SemanticIngestPolicy {
        max_records_per_segment: 4,
        max_semantic_store_bytes: 128,
        max_provenance_bytes: 4096,
    };
    let ingestor = SemanticStreamingContextIngestor::<8>::new(
        ContextStreamId::new(24).unwrap(),
        &temp.path,
        raw_policy(),
        policy,
    )
    .unwrap();
    let mut tokens = ByteTokens;
    let mut emitter = DeterministicEmitter;

    assert!(matches!(
        ingestor.ingest_reader(&b"abcd"[..], &mut tokens, &mut emitter),
        Err(ContextSemanticIngestError::SemanticStoreBudget { .. })
    ));
    assert_eq!(fs::read_dir(&temp.path).unwrap().count(), 0);
}

#[test]
fn preexisting_final_path_prevents_partial_bundle_publication() {
    let temp = TempDir::new();
    let ingestor = SemanticStreamingContextIngestor::<8>::new(
        ContextStreamId::new(25).unwrap(),
        &temp.path,
        raw_policy(),
        semantic_policy(),
    )
    .unwrap();
    fs::write(ingestor.provenance_path(1), b"occupied").unwrap();
    let mut tokens = ByteTokens;
    let mut emitter = DeterministicEmitter;

    assert!(matches!(
        ingestor.ingest_reader(&b"abcd"[..], &mut tokens, &mut emitter),
        Err(ContextSemanticIngestError::TargetExists)
    ));
    assert!(!ingestor.raw_path(1).exists());
    assert!(!ingestor.semantic_path(1).exists());
    assert_eq!(fs::read(ingestor.provenance_path(1)).unwrap(), b"occupied");
}

#[test]
fn provenance_payload_and_header_corruption_are_detected_independently() {
    let temp = TempDir::new();
    let ingestor = SemanticStreamingContextIngestor::<8>::new(
        ContextStreamId::new(26).unwrap(),
        &temp.path,
        raw_policy(),
        semantic_policy(),
    )
    .unwrap();
    let mut tokens = ByteTokens;
    let mut emitter = DeterministicEmitter;
    ingestor
        .ingest_reader(&b"abcd"[..], &mut tokens, &mut emitter)
        .unwrap();

    let bytes = fs::read(ingestor.provenance_path(1)).unwrap();

    let mut payload_bad = bytes.clone();
    *payload_bad.last_mut().unwrap() ^= 1;
    assert!(matches!(
        parse_provenance_artifact(&payload_bad),
        Err(ContextSemanticIngestError::ProvenancePayloadChecksum)
    ));

    let mut header_bad = bytes;
    header_bad[16] ^= 1;
    assert!(matches!(
        parse_provenance_artifact(&header_bad),
        Err(ContextSemanticIngestError::ProvenanceHeaderChecksum)
    ));
}

#[test]
fn semantic_record_budget_is_checked_before_store_construction() {
    struct ManyEmitter;
    impl SemanticSegmentEmitter<8> for ManyEmitter {
        fn emit_segment(
            &mut self,
            _raw_segment: &[u8],
            _context: SegmentEmissionContext,
        ) -> Result<Vec<(u64, SemanticState<8>)>, ContextSemanticIngestError> {
            Ok((0..5)
                .map(|id| (100 + id, SemanticState::<8>::zeroed().unwrap()))
                .collect())
        }
    }

    let temp = TempDir::new();
    let ingestor = SemanticStreamingContextIngestor::<8>::new(
        ContextStreamId::new(27).unwrap(),
        &temp.path,
        raw_policy(),
        semantic_policy(),
    )
    .unwrap();
    let mut tokens = ByteTokens;
    let mut emitter = ManyEmitter;

    assert!(matches!(
        ingestor.ingest_reader(&b"abcd"[..], &mut tokens, &mut emitter),
        Err(ContextSemanticIngestError::SemanticRecordBudget {
            actual: 5,
            maximum: 4,
        })
    ));
    assert_eq!(fs::read_dir(&temp.path).unwrap().count(), 0);
}

#[test]
fn semantic_stats_report_segment_local_peaks_not_whole_stream_residency() {
    let temp = TempDir::new();
    let ingestor = SemanticStreamingContextIngestor::<8>::new(
        ContextStreamId::new(28).unwrap(),
        &temp.path,
        raw_policy(),
        semantic_policy(),
    )
    .unwrap();
    let mut tokens = ByteTokens;
    let mut emitter = DeterministicEmitter;
    let outcome = ingestor
        .ingest_reader(&b"abcdefghijkl"[..], &mut tokens, &mut emitter)
        .unwrap();

    assert_eq!(outcome.stats.segments_published, 3);
    assert_eq!(outcome.stats.peak_raw_segment_bytes, 4);
    assert!(outcome.stats.peak_semantic_store_bytes > 0);
    assert!(outcome.stats.peak_provenance_bytes > 0);
    assert!(
        outcome.stats.semantic_store_bytes_published
            >= outcome.stats.peak_semantic_store_bytes * 3
    );
}
