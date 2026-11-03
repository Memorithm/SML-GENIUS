//! CONTEXT-3 G1 slice: independent exhaustive oracle + provenance/tie/abstention/corruption.
//!
//! All fixtures are synthetic and declared in this file. Nothing here is a
//! LONGCTX-B1 measurement; test 10 is a fixture-only sublinear-work check.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use sml_core::{
    context_ledger::{ContextSegmentId, ContextStreamId},
    semantic::{SemanticBitField, SemanticState},
};
use sml_store::{
    context_ingest::{ContextIngestError, SegmentTokenCounter, StreamingIngestPolicy},
    context_retrieve::{
        exhaustive_semantic_scan, ContextRetrievePolicy, ContextRetriever, ContextRetrieverConfig,
        RetrieveQuery,
    },
    context_semantic_ingest::{
        ContextSemanticIngestError, SegmentEmissionContext, SemanticIngestPolicy,
        SemanticSegmentEmitter, SemanticStreamingContextIngestor,
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

/// Independent oracle: derives every signature from the declared fixture
/// formula (never from retriever state) and ranks by (popcount xor, segment_id).
fn oracle_top_k(segments: u64, query: u64, k: usize) -> Vec<(u64, u64)> {
    let mut all: Vec<(u64, u64)> = (1..=segments)
        .map(|id| (u64::from((signature_word(id - 1) ^ query).count_ones()), id))
        .collect();
    all.sort_unstable();
    all.truncate(k);
    all
}

fn query_word(seed: u64) -> u64 {
    signature_word(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) ^ 0xdead_beef)
}

fn bucket(word: u64) -> u64 {
    word & ((1 << 12) - 1)
}

#[test]
fn g1_oracle_matches_exhaustive_scan_for_random_queries_and_all_k() {
    let fixture = Fixture::build(51, 300);
    let retriever = fixture.open_all();
    let mut ties_seen = 0;
    for seed in 0..64 {
        let word = query_word(seed);
        let bitfield = SemanticBitField::<BITS>::from_words(vec![word]).unwrap();
        for k in [1, 2, 4] {
            let expected = oracle_top_k(300, word, k);
            let (hits, accounting) =
                exhaustive_semantic_scan(&retriever, &bitfield, &policy(16, k)).unwrap();
            let got: Vec<(u64, u64)> = hits
                .iter()
                .map(|hit| (hit.hamming_distance, hit.segment.segment_id))
                .collect();
            assert_eq!(got, expected, "seed {seed} k {k}");
            assert!(accounting.invariant_holds());
            assert_eq!(accounting.raw_reread_segments, k as u64);
            ties_seen += expected.windows(2).filter(|w| w[0].0 == w[1].0).count();
        }
    }
    // Tie-break by segment_id must actually be exercised, not vacuous.
    assert!(ties_seen > 0, "fixture produced no distance ties");
}

#[test]
fn g1_every_hit_carries_verified_provenance_matching_declared_raw_bytes() {
    let fixture = Fixture::build(52, 64);
    let retriever = fixture.open_all();
    for needle in 1..=64 {
        let (hits, _) = retriever
            .retrieve(&exact_query(needle), &policy(16, 4))
            .unwrap();
        assert_eq!(hits[0].segment, fixture.segment(needle));
        for (rank, hit) in hits.iter().enumerate() {
            let order = hit.segment.segment_id - 1;
            let declared = raw_bytes_for_order(order);
            assert_eq!(hit.rank, rank);
            assert_eq!(hit.raw_bytes, declared);
            assert_eq!(hit.byte_start, order * SEGMENT_BYTES as u64);
            assert_eq!(hit.byte_len, SEGMENT_BYTES as u64);
            assert_eq!(hit.content_hash, sha256_raw_bytes(&declared).unwrap());
            assert_eq!(hit.source_exact, hit.hamming_distance == 0);
        }
    }
}

#[test]
fn g1_shortlist_is_sound_against_oracle_never_better_than_optimum() {
    let fixture = Fixture::build(53, 300);
    let retriever = fixture.open_all();
    for seed in 0..64 {
        let word = query_word(seed);
        let query = RetrieveQuery {
            bitfield: SemanticBitField::<BITS>::from_words(vec![word]).unwrap(),
            associative_key: None,
        };
        let (hits, accounting) = retriever.retrieve(&query, &policy(16, 4)).unwrap();
        let oracle = oracle_top_k(300, word, 4);
        assert!(accounting.invariant_holds());
        for (hit, best) in hits.iter().zip(&oracle) {
            assert!(hit.hamming_distance >= best.0);
            assert_eq!(
                hit.hamming_distance,
                u64::from((signature_word(hit.segment.segment_id - 1) ^ word).count_ones())
            );
            assert_eq!(
                bucket(signature_word(hit.segment.segment_id - 1)),
                bucket(word)
            );
        }
    }
}

#[test]
fn g1_abstains_with_zero_work_when_no_bucket_matches() {
    let fixture = Fixture::build(54, 128);
    let retriever = fixture.open_all();
    let used: Vec<u64> = (0..128).map(|o| bucket(signature_word(o))).collect();
    let free = (0..4096_u64).find(|b| !used.contains(b)).unwrap();
    let query = RetrieveQuery {
        bitfield: SemanticBitField::<BITS>::from_words(vec![free]).unwrap(),
        associative_key: None,
    };
    let (hits, accounting) = retriever.retrieve(&query, &policy(16, 4)).unwrap();
    assert!(hits.is_empty(), "must abstain, not fabricate a hit");
    assert_eq!(accounting.shortlisted, 0);
    assert_eq!(accounting.decoded, 0);
    assert_eq!(accounting.raw_reread_bytes, 0);
}

#[test]
fn g1_raw_corruption_is_isolated_to_queries_that_reread_it() {
    let fixture = Fixture::build(55, 64);
    let retriever = fixture.open_all();
    let victim = 10_u64;
    let bystander = (1..=64)
        .find(|&id| bucket(signature_word(id - 1)) != bucket(signature_word(victim - 1)))
        .unwrap();
    let path = retriever.raw_path(victim);
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    fs::write(&path, bytes).unwrap();
    let (hits, _) = retriever
        .retrieve(&exact_query(bystander), &policy(16, 1))
        .unwrap();
    assert_eq!(hits[0].segment, fixture.segment(bystander));
    assert!(retriever
        .retrieve(&exact_query(victim), &policy(16, 1))
        .is_err());
    assert!(
        exhaustive_semantic_scan(&retriever, &signature_for_segment(victim), &policy(16, 1))
            .is_err()
    );
}
