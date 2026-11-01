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
        exhaustive_semantic_scan, ContextRetrieveError, ContextRetrievePolicy, ContextRetriever,
        ContextRetrieverConfig, RetrieveQuery,
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

#[test]
fn context3_t02_exact_needle_hit_only_after_sha256_verification() {
    let fixture = Fixture::build(32, 64);
    let retriever = fixture.open_all();
    let needle = 37;
    let (hits, accounting) = retriever
        .retrieve(&exact_query(needle), &policy(16, 1))
        .unwrap();
    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(hit.segment, fixture.segment(needle));
    assert!(hit.source_exact);
    assert_eq!(hit.raw_bytes, raw_bytes_for_order(needle - 1));
    let provenance =
        parse_provenance_artifact(&fs::read(retriever.provenance_path(needle)).unwrap()).unwrap();
    assert_eq!(
        sha256_raw_bytes(&hit.raw_bytes).unwrap(),
        provenance.content_hash
    );
    assert_eq!(hit.content_hash, provenance.content_hash);
    assert_eq!(accounting.raw_reread_bytes, SEGMENT_BYTES as u64);

    tamper_provenance_hash(&retriever, needle);
    assert!(matches!(
        retriever.retrieve(&exact_query(needle), &policy(16, 1)),
        Err(ContextRetrieveError::HashMismatch)
    ));
}

#[test]
fn context3_t04_hash_mismatch_or_truncated_raw_segment_aborts() {
    let fixture = Fixture::build(34, 64);
    let retriever = fixture.open_all();

    tamper_provenance_hash(&retriever, 10);
    assert!(matches!(
        retriever.retrieve(&exact_query(10), &policy(16, 1)),
        Err(ContextRetrieveError::HashMismatch)
    ));

    let corrupted = retriever.raw_path(20);
    let mut bytes = fs::read(&corrupted).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    fs::remove_file(&corrupted).unwrap();
    fs::write(&corrupted, &bytes).unwrap();
    assert!(matches!(
        retriever.retrieve(&exact_query(20), &policy(16, 1)),
        Err(ContextRetrieveError::HashMismatch)
    ));

    let truncated = retriever.raw_path(30);
    let bytes = fs::read(&truncated).unwrap();
    fs::remove_file(&truncated).unwrap();
    fs::write(&truncated, &bytes[..bytes.len() - 1]).unwrap();
    assert!(matches!(
        retriever.retrieve(&exact_query(30), &policy(16, 1)),
        Err(ContextRetrieveError::HashMismatch)
    ));
}

#[test]
fn context3_t05_missing_raw_segment_or_provenance_returns_error() {
    let fixture = Fixture::build(35, 64);
    let retriever = fixture.open_all();

    fs::remove_file(retriever.raw_path(11)).unwrap();
    assert!(matches!(
        retriever.retrieve(&exact_query(11), &policy(16, 1)),
        Err(ContextRetrieveError::MissingRaw)
    ));

    fs::remove_file(retriever.provenance_path(12)).unwrap();
    assert!(matches!(
        retriever.retrieve(&exact_query(12), &policy(16, 1)),
        Err(ContextRetrieveError::MissingProvenance)
    ));

    assert!(matches!(
        ContextRetriever::<BITS>::open(&fixture.temp.path, fixture.stream, 64, config()),
        Err(ContextRetrieveError::MissingProvenance)
    ));
}

#[test]
fn context3_t06_associative_hit_miss_and_full_capacity() {
    let fixture = Fixture::build(36, 64);
    let mut retriever = fixture.open(
        64,
        ContextRetrieverConfig {
            associative_capacity: 1,
            ..config()
        },
    );
    let needle = 44;
    retriever.remember(7, fixture.segment(needle)).unwrap();

    let mut far = signature_for_segment(needle);
    far.not_assign();
    let one_slot = ContextRetrievePolicy {
        associative_slots: 1,
        ..policy(4, 4)
    };

    let hit_query = RetrieveQuery {
        bitfield: far.clone(),
        associative_key: Some(7),
    };
    let (hits, accounting) = retriever.retrieve(&hit_query, &one_slot).unwrap();
    assert_eq!(accounting.associative_hits, 1);
    assert_eq!(accounting.associative_misses, 0);
    assert!(hits
        .iter()
        .any(|hit| hit.segment == fixture.segment(needle)));

    let miss_query = RetrieveQuery {
        bitfield: far.clone(),
        associative_key: Some(8),
    };
    let (_, accounting) = retriever.retrieve(&miss_query, &one_slot).unwrap();
    assert_eq!(accounting.associative_hits, 0);
    assert_eq!(accounting.associative_misses, 1);

    assert!(matches!(
        retriever.remember(8, fixture.segment(1)),
        Err(ContextRetrieveError::AssociativeDenied)
    ));
    assert!(matches!(
        retriever.remember(9, ContextSegmentId::new(fixture.stream, 65).unwrap()),
        Err(ContextRetrieveError::AssociativeDenied)
    ));
    for denied in [
        ContextRetrievePolicy {
            associative_slots: 0,
            ..policy(4, 4)
        },
        ContextRetrievePolicy {
            associative_slots: 2,
            ..policy(4, 4)
        },
    ] {
        assert!(matches!(
            retriever.retrieve(&hit_query, &denied),
            Err(ContextRetrieveError::AssociativeDenied)
        ));
    }

    let mut disabled = fixture.open(
        64,
        ContextRetrieverConfig {
            associative_capacity: 0,
            ..config()
        },
    );
    assert!(matches!(
        disabled.remember(7, fixture.segment(needle)),
        Err(ContextRetrieveError::AssociativeDenied)
    ));
    assert!(matches!(
        disabled.retrieve(&hit_query, &one_slot),
        Err(ContextRetrieveError::AssociativeDenied)
    ));
}

#[test]
fn context3_t07_exhaustive_scan_and_shortlist_agree_with_few_reads() {
    let fixture = Fixture::build(37, 256);
    let retriever = fixture.open_all();
    for needle in [1, 64, 128, 200, 256] {
        let (short_hits, short_accounting) = retriever
            .retrieve(&exact_query(needle), &policy(16, 1))
            .unwrap();
        let (scan_hits, scan_accounting) =
            exhaustive_semantic_scan(&retriever, &signature_for_segment(needle), &policy(16, 1))
                .unwrap();
        assert_eq!(short_hits, scan_hits);
        assert_eq!(short_hits[0].segment, fixture.segment(needle));
        assert_eq!(scan_accounting.shortlisted, 256);
        assert!(short_accounting.shortlisted <= 16);
        assert!(short_accounting.shortlisted * 8 < scan_accounting.shortlisted);
    }
}

/// Fixture-only sublinear-work check. Does NOT satisfy LONGCTX-B1 H4 nor any
/// wording gate, and supports no scale claim.
#[test]
fn context3_t10_sublinear_fit_on_declared_fixture_with_exhaustive_negative_control() {
    const LENGTHS: [u64; 5] = [256, 512, 1024, 2048, 4096];
    const QUERIES: u64 = 32;
    let fixture = Fixture::build(40, 4096);
    let mut shortlist_points = Vec::new();
    let mut scan_points = Vec::new();
    for length in LENGTHS {
        let retriever = fixture.open(length, config());
        let mut shortlist_work = 0_u64;
        let mut scan_work = 0_u64;
        for query_index in 0..QUERIES {
            let needle = 1 + query_index * length / QUERIES;
            let (hits, accounting) = retriever
                .retrieve(&exact_query(needle), &policy(16, 1))
                .unwrap();
            assert_eq!(hits[0].segment, fixture.segment(needle));
            assert!(hits[0].source_exact);
            shortlist_work += accounting.shortlisted;
            let (_, scan) = exhaustive_semantic_scan(
                &retriever,
                &signature_for_segment(needle),
                &policy(16, 1),
            )
            .unwrap();
            scan_work += scan.shortlisted;
        }
        shortlist_points.push((length as f64, shortlist_work as f64 / QUERIES as f64));
        scan_points.push((length as f64, scan_work as f64 / QUERIES as f64));
    }
    let alpha_shortlist = fit_exponent(&shortlist_points);
    let alpha_scan = fit_exponent(&scan_points);
    assert!(alpha_shortlist < 0.75, "shortlist alpha {alpha_shortlist}");
    assert!(
        (alpha_scan - 1.0).abs() < 0.05,
        "exhaustive alpha {alpha_scan}"
    );
}
