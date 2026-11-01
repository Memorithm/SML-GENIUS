//! CONTEXT-3 mandatory tests (docs/SML-CONTEXT-3-RETRIEVAL.md, "Mandatory tests").
//!
//! All fixtures are synthetic and declared in this file. Nothing here is a
//! LONGCTX-B1 measurement; test 10 is a fixture-only sublinear-work check.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use sml_core::{
    context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId},
    semantic::{SemanticBitField, SemanticState},
};
use sml_store::{
    context_ingest::{ContextIngestError, SegmentTokenCounter, StreamingIngestPolicy},
    context_retrieve::{
        ContextRetrieveError, ContextRetrievePolicy, ContextRetriever, ContextRetrieverConfig,
        RetrieveQuery,
    },
    context_semantic_ingest::{
        build_provenance_artifact, parse_provenance_artifact, ContextSemanticIngestError,
        SegmentEmissionContext, SemanticIngestPolicy, SemanticSegmentEmitter,
        SemanticStreamingContextIngestor,
    },
    raw_context::{sha256_raw_bytes, RawContextStoreLimits},
};

const BITS: usize = 64;
const SEGMENT_BYTES: usize = 16;
const RECORD_ID_BASE: u64 = 1_000_000;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("sml-context-retrieve-{}-{id}", std::process::id()));
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

/// Declared fixture signature: SplitMix64 of the 0-based order index.
fn signature_word(order_index: u64) -> u64 {
    let mut z = order_index.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn signature_for_segment(segment_id: u64) -> SemanticBitField<BITS> {
    SemanticBitField::<BITS>::from_words(vec![signature_word(segment_id - 1)]).unwrap()
}

/// Declared fixture raw bytes: b"seg:" + LE u64 order index + b"\n...".
fn raw_bytes_for_order(order_index: u64) -> [u8; SEGMENT_BYTES] {
    let mut bytes = [0_u8; SEGMENT_BYTES];
    bytes[0..4].copy_from_slice(b"seg:");
    bytes[4..12].copy_from_slice(&order_index.to_le_bytes());
    bytes[12..16].copy_from_slice(b"\n...");
    bytes
}

struct SignatureEmitter;

impl SemanticSegmentEmitter<BITS> for SignatureEmitter {
    fn emit_segment(
        &mut self,
        _raw_segment: &[u8],
        context: SegmentEmissionContext,
    ) -> Result<Vec<(u64, SemanticState<BITS>)>, ContextSemanticIngestError> {
        let mut state = SemanticState::<BITS>::zeroed().unwrap();
        state.value =
            SemanticBitField::<BITS>::from_words(vec![signature_word(context.order_index)])
                .unwrap();
        state.meaning_mask = SemanticBitField::<BITS>::ones().unwrap();
        Ok(vec![(RECORD_ID_BASE + context.segment_id, state)])
    }
}

struct Fixture {
    temp: TempDir,
    stream: ContextStreamId,
    segments: u64,
}

impl Fixture {
    fn build(stream: u64, segments: u64) -> Self {
        let temp = TempDir::new();
        let stream = ContextStreamId::new(stream).unwrap();
        let ingestor = SemanticStreamingContextIngestor::<BITS>::new(
            stream,
            &temp.path,
            StreamingIngestPolicy {
                max_segment_bytes: SEGMENT_BYTES,
                max_segments: segments,
                max_total_raw_bytes: segments * SEGMENT_BYTES as u64,
            },
            SemanticIngestPolicy {
                max_records_per_segment: 1,
                max_semantic_store_bytes: 1 << 16,
                max_provenance_bytes: 4096,
            },
        )
        .unwrap();
        let mut corpus = Vec::with_capacity(segments as usize * SEGMENT_BYTES);
        for order_index in 0..segments {
            corpus.extend_from_slice(&raw_bytes_for_order(order_index));
        }
        let outcome = ingestor
            .ingest_reader(&corpus[..], &mut ByteTokens, &mut SignatureEmitter)
            .unwrap();
        assert_eq!(outcome.stats.segments_published, segments);
        Self {
            temp,
            stream,
            segments,
        }
    }

    fn open(&self, segment_count: u64, config: ContextRetrieverConfig) -> ContextRetriever<BITS> {
        ContextRetriever::<BITS>::open(&self.temp.path, self.stream, segment_count, config).unwrap()
    }

    fn open_all(&self) -> ContextRetriever<BITS> {
        self.open(self.segments, config())
    }

    fn segment(&self, segment_id: u64) -> ContextSegmentId {
        ContextSegmentId::new(self.stream, segment_id).unwrap()
    }
}

fn config() -> ContextRetrieverConfig {
    ContextRetrieverConfig {
        raw_limits: RawContextStoreLimits {
            max_file_bytes: 1 << 16,
            max_raw_bytes: 1 << 12,
            max_semantic_links: 4,
        },
        max_provenance_bytes: 4096,
        max_semantic_store_bytes: 1 << 16,
        bucket_prefix_bits: 12,
        associative_capacity: 8,
    }
}

fn policy(max_shortlist: usize, top_k: usize) -> ContextRetrievePolicy {
    ContextRetrievePolicy {
        max_shortlist,
        top_k,
        associative_slots: 8,
        fail_on_hash_mismatch: true,
    }
}

fn exact_query(segment_id: u64) -> RetrieveQuery<BITS> {
    RetrieveQuery {
        bitfield: signature_for_segment(segment_id),
        associative_key: None,
    }
}

fn tamper_provenance_hash(retriever: &ContextRetriever<BITS>, segment_id: u64) {
    let path = retriever.provenance_path(segment_id);
    let mut manifest = parse_provenance_artifact(&fs::read(&path).unwrap()).unwrap();
    let mut forged = *manifest.content_hash.bytes();
    forged[0] ^= 0xff;
    manifest.content_hash = ContextContentHash::from_sha256(forged);
    fs::remove_file(&path).unwrap();
    fs::write(&path, build_provenance_artifact(&manifest).unwrap()).unwrap();
}

/// Least-squares slope of ln(work) on ln(length).
fn fit_exponent(points: &[(f64, f64)]) -> f64 {
    let n = points.len() as f64;
    let xs: Vec<f64> = points.iter().map(|(length, _)| length.ln()).collect();
    let ys: Vec<f64> = points.iter().map(|(_, work)| work.ln()).collect();
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for (x, y) in xs.iter().zip(&ys) {
        numerator += (x - mean_x) * (y - mean_y);
        denominator += (x - mean_x) * (x - mean_x);
    }
    numerator / denominator
}

#[test]
fn context3_retriever_paths_match_semantic_ingestor_paths() {
    let fixture = Fixture::build(30, 4);
    let ingestor = SemanticStreamingContextIngestor::<BITS>::new(
        fixture.stream,
        &fixture.temp.path,
        StreamingIngestPolicy {
            max_segment_bytes: SEGMENT_BYTES,
            max_segments: 4,
            max_total_raw_bytes: 64,
        },
        SemanticIngestPolicy {
            max_records_per_segment: 1,
            max_semantic_store_bytes: 1 << 16,
            max_provenance_bytes: 4096,
        },
    )
    .unwrap();
    let retriever = fixture.open_all();
    assert_eq!(retriever.len(), 4);
    for segment_id in 1..=4 {
        assert_eq!(
            retriever.raw_path(segment_id),
            ingestor.raw_path(segment_id)
        );
        assert_eq!(
            retriever.semantic_path(segment_id),
            ingestor.semantic_path(segment_id)
        );
        assert_eq!(
            retriever.provenance_path(segment_id),
            ingestor.provenance_path(segment_id)
        );
    }
}

#[test]
fn context3_t08_empty_index_or_zero_budget_fails_validation() {
    let fixture = Fixture::build(38, 8);
    assert!(matches!(
        ContextRetriever::<BITS>::open(&fixture.temp.path, fixture.stream, 0, config()),
        Err(ContextRetrieveError::EmptyIndex)
    ));
    let zero_prefix = ContextRetrieverConfig {
        bucket_prefix_bits: 0,
        ..config()
    };
    assert!(matches!(
        ContextRetriever::<BITS>::open(&fixture.temp.path, fixture.stream, 8, zero_prefix),
        Err(ContextRetrieveError::BudgetOverflow)
    ));

    let invalid = [
        policy(0, 1),
        policy(8, 0),
        policy(8, 3),
        policy(1, 2),
        ContextRetrievePolicy {
            fail_on_hash_mismatch: false,
            ..policy(8, 1)
        },
    ];
    let retriever = fixture.open_all();
    for candidate in invalid {
        assert!(matches!(
            candidate.validate(),
            Err(ContextRetrieveError::BudgetOverflow)
        ));
        assert!(matches!(
            retriever.retrieve(&exact_query(1), &candidate),
            Err(ContextRetrieveError::BudgetOverflow)
        ));
    }
    assert!(policy(8, 4).validate().is_ok());
}

#[test]
fn context3_t01_shortlist_and_top_k_budgets_respected_with_stable_sort() {
    let fixture = Fixture::build(31, 64);
    let wide_buckets = ContextRetrieverConfig {
        bucket_prefix_bits: 1,
        ..config()
    };
    let retriever = fixture.open(64, wide_buckets);
    for segment_id in [1, 17, 40, 64] {
        let (hits, accounting) = retriever
            .retrieve(&exact_query(segment_id), &policy(8, 4))
            .unwrap();
        assert_eq!(accounting.shortlisted, 8);
        assert_eq!(accounting.decoded, 4);
        assert_eq!(hits.len(), 4);
        for (rank, hit) in hits.iter().enumerate() {
            assert_eq!(hit.rank, rank);
        }
        for pair in hits.windows(2) {
            let left = (pair[0].hamming_distance, pair[0].segment.segment_id);
            let right = (pair[1].hamming_distance, pair[1].segment.segment_id);
            assert!(left < right, "stable (distance, segment_id) order violated");
        }
    }
}

#[test]
fn context3_t03_hamming_near_miss_is_not_a_source_exact_hit() {
    let fixture = Fixture::build(33, 64);
    let retriever = fixture.open_all();
    let needle = 23;
    let mut near_miss = signature_for_segment(needle);
    let flipped = !near_miss.get(40).unwrap();
    near_miss.set(40, flipped).unwrap();
    let query = RetrieveQuery {
        bitfield: near_miss,
        associative_key: None,
    };
    let (hits, _) = retriever.retrieve(&query, &policy(16, 4)).unwrap();
    assert_eq!(hits[0].segment, fixture.segment(needle));
    assert_eq!(hits[0].hamming_distance, 1);
    assert!(hits.iter().all(|hit| !hit.source_exact));
}

#[test]
fn context3_t09_multi_query_independence() {
    let fixture = Fixture::build(39, 64);
    let retriever = fixture.open_all();
    let first = exact_query(5);
    let second = exact_query(50);
    let alone_first = retriever.retrieve(&first, &policy(16, 2)).unwrap();
    let alone_second = retriever.retrieve(&second, &policy(16, 2)).unwrap();
    for _ in 0..3 {
        assert_eq!(
            retriever.retrieve(&second, &policy(16, 2)).unwrap(),
            alone_second
        );
        assert_eq!(
            retriever.retrieve(&first, &policy(16, 2)).unwrap(),
            alone_first
        );
    }
    assert_ne!(alone_first.0[0].segment, alone_second.0[0].segment);
}

#[test]
fn context3_t11_accounting_invariant_holds() {
    let fixture = Fixture::build(41, 64);
    let retriever = fixture.open(
        64,
        ContextRetrieverConfig {
            bucket_prefix_bits: 2,
            ..config()
        },
    );
    for segment_id in 1..=64 {
        for top_k in [1, 2, 4] {
            let (hits, accounting) = retriever
                .retrieve(&exact_query(segment_id), &policy(4, top_k))
                .unwrap();
            assert!(accounting.invariant_holds(), "{accounting:?}");
            assert!(accounting.shortlisted <= 4);
            assert_eq!(accounting.raw_reread_segments, hits.len() as u64);
            assert_eq!(
                accounting.raw_reread_bytes,
                hits.iter().map(|hit| hit.byte_len).sum::<u64>()
            );
        }
    }
}

#[test]
fn context3_t12_determinism_across_two_runs() {
    let left = Fixture::build(42, 64);
    let right = Fixture::build(42, 64);
    let a = left.open_all();
    let b = right.open_all();
    for segment_id in [1, 9, 33, 64] {
        assert_eq!(
            a.retrieve(&exact_query(segment_id), &policy(16, 4))
                .unwrap(),
            b.retrieve(&exact_query(segment_id), &policy(16, 4))
                .unwrap()
        );
    }
}
