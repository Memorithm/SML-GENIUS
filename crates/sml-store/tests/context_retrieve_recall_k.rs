//! CONTEXT-3 G1 tranche: Recall@K of bucket-shortlist retrieval vs the
//! independent exhaustive oracle, plus work accounting, near-miss/bucket-miss
//! cases and a reproducible wall-clock protocol.
//!
//! All corpora are synthetic and declared in this file. This is NOT a
//! LONGCTX-B1 measurement. Quality assertions are *measured-floor* checks on
//! fixed seeds, not generalization claims. Run the report with:
//! `cargo test -p sml-store --test context_retrieve_recall_k -- --nocapture`.
//! Peak memory/RSS is NOT_MEASURED here (no allocator instrumentation); only
//! on-disk store bytes are measured.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use sml_core::{
    context_ledger::ContextStreamId,
    semantic::{SemanticBitField, SemanticState},
};
use sml_store::{
    context_ingest::{ContextIngestError, SegmentTokenCounter, StreamingIngestPolicy},
    context_retrieve::{
        exhaustive_semantic_scan, ContextRetrievePolicy, ContextRetriever, ContextRetrieverConfig,
        RetrieveAccounting, RetrieveHit, RetrieveQuery,
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
const KS: [usize; 3] = [1, 2, 4];
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("sml-context-recall-{}-{id}", std::process::id()));
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

fn splitmix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Declared signature distributions (pure functions of the 0-based order).
#[derive(Clone, Copy, Debug)]
enum Dist {
    /// Independent uniform 64-bit words.
    Uniform,
    /// 4 centroids; each member flips ~6 random high bits (low 16 bits shared).
    Clustered,
    /// 8 centroids; members flip random bits anywhere, incl. bucket bits.
    NoisyClustered,
}

impl Dist {
    fn name(self) -> &'static str {
        match self {
            Self::Uniform => "uniform",
            Self::Clustered => "clustered",
            Self::NoisyClustered => "noisy_clustered",
        }
    }

    fn signature(self, order: u64) -> u64 {
        match self {
            Self::Uniform => splitmix(order),
            Self::Clustered => {
                let centroid = splitmix(0xc1u64 << 32 | (order % 4));
                let noise = splitmix(order ^ 0x5555) & splitmix(order ^ 0xaaaa);
                centroid ^ (noise & splitmix(order ^ 0x3333) & !0xffff)
            }
            Self::NoisyClustered => {
                let centroid = splitmix(0xc2u64 << 32 | (order % 8));
                let noise = splitmix(order ^ 0x1111) & splitmix(order ^ 0x2222);
                centroid ^ (noise & splitmix(order ^ 0x4444))
            }
        }
    }

    /// Query near a random member (~3 flipped bits anywhere).
    fn query(self, segments: u64, seed: u64) -> u64 {
        let base = self.signature(splitmix(seed ^ 0xfeed) % segments);
        let mut word = base;
        for step in 0..3 {
            word ^= 1 << (splitmix(seed.wrapping_mul(31) + step) % 64);
        }
        word
    }
}

fn raw_bytes_for_order(order_index: u64) -> [u8; SEGMENT_BYTES] {
    let mut bytes = [0_u8; SEGMENT_BYTES];
    bytes[0..4].copy_from_slice(b"seg:");
    bytes[4..12].copy_from_slice(&order_index.to_le_bytes());
    bytes[12..16].copy_from_slice(b"\n...");
    bytes
}

struct Emitter(Dist);

impl SemanticSegmentEmitter<BITS> for Emitter {
    fn emit_segment(
        &mut self,
        _raw_segment: &[u8],
        context: SegmentEmissionContext,
    ) -> Result<Vec<(u64, SemanticState<BITS>)>, ContextSemanticIngestError> {
        let mut state = SemanticState::<BITS>::zeroed().unwrap();
        state.value =
            SemanticBitField::<BITS>::from_words(vec![self.0.signature(context.order_index)])
                .unwrap();
        state.meaning_mask = SemanticBitField::<BITS>::ones().unwrap();
        Ok(vec![(RECORD_ID_BASE + context.segment_id, state)])
    }
}

struct Fixture {
    temp: TempDir,
    stream: ContextStreamId,
    segments: u64,
    dist: Dist,
}

impl Fixture {
    fn build(stream: u64, segments: u64, dist: Dist) -> Self {
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
        for order in 0..segments {
            corpus.extend_from_slice(&raw_bytes_for_order(order));
        }
        let outcome = ingestor
            .ingest_reader(&corpus[..], &mut ByteTokens, &mut Emitter(dist))
            .unwrap();
        assert_eq!(outcome.stats.segments_published, segments);
        Self {
            temp,
            stream,
            segments,
            dist,
        }
    }

    fn open(&self, prefix_bits: u32) -> ContextRetriever<BITS> {
        let config = ContextRetrieverConfig {
            raw_limits: RawContextStoreLimits {
                max_file_bytes: 1 << 16,
                max_raw_bytes: 1 << 12,
                max_semantic_links: 4,
            },
            max_provenance_bytes: 4096,
            max_semantic_store_bytes: 1 << 16,
            bucket_prefix_bits: prefix_bits,
            associative_capacity: 8,
        };
        ContextRetriever::<BITS>::open(&self.temp.path, self.stream, self.segments, config).unwrap()
    }

    fn sig(&self, segment_id: u64) -> u64 {
        self.dist.signature(segment_id - 1)
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

fn bf(word: u64) -> SemanticBitField<BITS> {
    SemanticBitField::<BITS>::from_words(vec![word]).unwrap()
}

fn query(word: u64) -> RetrieveQuery<BITS> {
    RetrieveQuery {
        bitfield: bf(word),
        associative_key: None,
    }
}

/// Independent oracle from the declared formula: rank by (popcount xor, id).
fn oracle_top_k(fixture: &Fixture, word: u64, k: usize) -> Vec<(u64, u64)> {
    let mut all: Vec<(u64, u64)> = (1..=fixture.segments)
        .map(|id| (u64::from((fixture.sig(id) ^ word).count_ones()), id))
        .collect();
    all.sort_unstable();
    all.truncate(k);
    all
}

/// Recall@K = |retrieved ids ∩ oracle top-K ids| / K. Distance-tied
/// substitutes at the K-th oracle distance count as hits (tie-tolerant), so
/// recall measures ranking quality, not id tie-break luck.
fn recall_at_k(hits: &[RetrieveHit], oracle_k: &[(u64, u64)], kth_distance: u64) -> f64 {
    let k = oracle_k.len();
    let mut found = 0;
    for hit in hits.iter().take(k) {
        let id = hit.segment.segment_id;
        if oracle_k.iter().any(|&(_, o)| o == id) || hit.hamming_distance <= kth_distance {
            found += 1;
        }
    }
    f64::from(found) / k as f64
}

fn check_hits(fixture: &Fixture, word: u64, hits: &[RetrieveHit], acc: &RetrieveAccounting) {
    assert!(acc.invariant_holds());
    assert_eq!(acc.decoded, hits.len() as u64);
    assert_eq!(acc.raw_reread_segments, hits.len() as u64);
    assert_eq!(
        acc.raw_reread_bytes,
        hits.len() as u64 * SEGMENT_BYTES as u64
    );
    for (rank, hit) in hits.iter().enumerate() {
        let order = hit.segment.segment_id - 1;
        let declared = raw_bytes_for_order(order);
        assert_eq!(hit.rank, rank);
        assert_eq!(hit.raw_bytes, declared);
        assert_eq!(hit.content_hash, sha256_raw_bytes(&declared).unwrap());
        assert_eq!(
            hit.hamming_distance,
            u64::from((fixture.sig(hit.segment.segment_id) ^ word).count_ones())
        );
        assert_eq!(hit.source_exact, hit.hamming_distance == 0);
    }
}

#[derive(Default)]
struct Cell {
    recall_sum: [f64; 3],
    abstain: u64,
    queries: u64,
    shortlisted: u64,
    decoded: u64,
    reread_bytes: u64,
    exh_shortlisted: u64,
    exh_reread_bytes: u64,
    short_time: Duration,
    exh_time: Duration,
}

fn dir_bytes(path: &Path) -> u64 {
    fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum()
}

fn measure(fixture: &Fixture, prefix_bits: u32, max_shortlist: usize, queries: u64) -> Cell {
    let retriever = fixture.open(prefix_bits);
    let mut cell = Cell::default();
    for seed in 0..queries {
        let word = fixture.dist.query(fixture.segments, seed);
        cell.queries += 1;
        for (slot, &k) in KS.iter().enumerate() {
            let oracle = oracle_top_k(fixture, word, k);
            let kth = oracle.last().unwrap().0;
            let t0 = Instant::now();
            let (hits, acc) = retriever
                .retrieve(&query(word), &policy(max_shortlist, k))
                .unwrap();
            cell.short_time += t0.elapsed();
            let t1 = Instant::now();
            let (exh, exh_acc) =
                exhaustive_semantic_scan(&retriever, &bf(word), &policy(max_shortlist, k)).unwrap();
            cell.exh_time += t1.elapsed();
            check_hits(fixture, word, &hits, &acc);
            check_hits(fixture, word, &exh, &exh_acc);
            // Existing exhaustive control must equal the independent oracle.
            let exh_ids: Vec<(u64, u64)> = exh
                .iter()
                .map(|h| (h.hamming_distance, h.segment.segment_id))
                .collect();
            assert_eq!(exh_ids, oracle);
            assert_eq!(exh_acc.shortlisted, fixture.segments);
            assert!(acc.shortlisted <= max_shortlist as u64);
            // Shortlist can never beat the optimum at any rank.
            for (hit, best) in hits.iter().zip(&oracle) {
                assert!(hit.hamming_distance >= best.0);
            }
            cell.recall_sum[slot] += recall_at_k(&hits, &oracle, kth);
            if hits.is_empty() {
                cell.abstain += 1;
            }
            cell.shortlisted += acc.shortlisted;
            cell.decoded += acc.decoded;
            cell.reread_bytes += acc.raw_reread_bytes;
            cell.exh_shortlisted += exh_acc.shortlisted;
            cell.exh_reread_bytes += exh_acc.raw_reread_bytes;
        }
    }
    cell
}

#[test]
fn g1_recall_at_k_matrix_sizes_distributions_and_accounting() {
    // Fixed protocol: 3 distributions x 3 sizes x 2 bucket widths, 48 queries
    // each, K in {1,2,4}, max_shortlist 32. Debug-build timings are indicative.
    let mut stream = 600;
    println!(
        "G1_RECALL dist size prefix_bits R@1 R@2 R@4 abstain_rate mean_shortlisted \
         mean_decoded mean_reread_bytes exh_mean_scored exh_mean_reread_bytes \
         shortlist_us_per_query exhaustive_us_per_query store_bytes"
    );
    let mut floors = Vec::new();
    for dist in [Dist::Uniform, Dist::Clustered, Dist::NoisyClustered] {
        for size in [64_u64, 256, 1024] {
            let fixture = Fixture::build(stream, size, dist);
            stream += 1;
            let store = dir_bytes(&fixture.temp.path);
            for prefix_bits in [4_u32, 8] {
                let cell = measure(&fixture, prefix_bits, 32, 48);
                let calls = (cell.queries * KS.len() as u64) as f64;
                let r: Vec<f64> = cell
                    .recall_sum
                    .iter()
                    .map(|s| s / cell.queries as f64)
                    .collect();
                println!(
                    "G1_RECALL {} {size} {prefix_bits} {:.3} {:.3} {:.3} {:.3} {:.1} {:.2} {:.1} \
                     {:.1} {:.1} {:.1} {:.1} {store}",
                    dist.name(),
                    r[0],
                    r[1],
                    r[2],
                    cell.abstain as f64 / calls,
                    cell.shortlisted as f64 / calls,
                    cell.decoded as f64 / calls,
                    cell.reread_bytes as f64 / calls,
                    cell.exh_shortlisted as f64 / calls,
                    cell.exh_reread_bytes as f64 / calls,
                    cell.short_time.as_secs_f64() * 1e6 / calls,
                    cell.exh_time.as_secs_f64() * 1e6 / calls,
                );
                // Compute-cost claim that is structural, not timing-based.
                assert!(cell.shortlisted <= cell.exh_shortlisted);
                floors.push((dist.name(), size, prefix_bits, r));
            }
        }
    }
    // Generalization limit made explicit: on uniform data with 8 prefix bits
    // and 3-bit query noise, shortlist recall is measurably below 1.0.
    let uniform_wide = floors
        .iter()
        .find(|(d, s, p, _)| *d == "uniform" && *s == 1024 && *p == 8)
        .unwrap();
    assert!(uniform_wide.3[0] < 1.0, "expected measurable recall loss");
}

#[test]
fn g1_bucket_miss_best_result_outside_selected_bucket() {
    let fixture = Fixture::build(700, 256, Dist::Uniform);
    let prefix_bits = 8;
    let retriever = fixture.open(prefix_bits);
    let mut misses = 0;
    for target in 1..=64_u64 {
        // Flip one bucket bit: the true nearest (distance 1) lives in another bucket.
        let word = fixture.sig(target) ^ (1 << (target % u64::from(prefix_bits)));
        let oracle = oracle_top_k(&fixture, word, 1);
        if oracle[0] != (1, target) {
            continue; // another segment ties/beats it; not a clean near-miss case
        }
        let (exh, _) = exhaustive_semantic_scan(&retriever, &bf(word), &policy(32, 1)).unwrap();
        assert_eq!(exh[0].segment.segment_id, target);
        let (hits, acc) = retriever.retrieve(&query(word), &policy(32, 1)).unwrap();
        check_hits(&fixture, word, &hits, &acc);
        assert!(hits.iter().all(|h| h.segment.segment_id != target));
        // Either honest abstention or a strictly worse, correctly labelled hit.
        match hits.first() {
            None => assert_eq!(acc.raw_reread_bytes, 0),
            Some(h) => {
                assert!(h.hamming_distance > 1);
                assert!(!h.source_exact);
            }
        }
        misses += 1;
    }
    assert!(misses >= 32, "only {misses} clean near-miss cases");
}

#[test]
fn g1_shortlist_cap_truncation_loses_recall_and_is_counted() {
    // Clustered: all 512 members share the low 16 bits per centroid, so a
    // 4-bit bucket holds ~128 entries and the cap is binding.
    let fixture = Fixture::build(701, 512, Dist::Clustered);
    let small = measure(&fixture, 4, 4, 32);
    let large = measure(&fixture, 4, 256, 32);
    let calls = (small.queries * KS.len() as u64) as f64;
    assert!(small.shortlisted as f64 / calls <= 4.0);
    assert!(large.shortlisted > small.shortlisted);
    // Larger cap never lowers mean Recall@K on the same queries.
    for slot in 0..KS.len() {
        assert!(large.recall_sum[slot] >= small.recall_sum[slot]);
    }
    println!(
        "G1_CAP clustered 512 cap4_R@1={:.3} cap256_R@1={:.3} cap4_R@4={:.3} cap256_R@4={:.3}",
        small.recall_sum[0] / small.queries as f64,
        large.recall_sum[0] / large.queries as f64,
        small.recall_sum[2] / small.queries as f64,
        large.recall_sum[2] / large.queries as f64,
    );
}

#[test]
fn g1_exact_member_queries_have_full_recall_at_one() {
    // Positive control: querying a stored signature with no noise must hit it.
    for dist in [Dist::Uniform, Dist::Clustered, Dist::NoisyClustered] {
        let fixture = Fixture::build(710 + dist as u64, 128, dist);
        let retriever = fixture.open(8);
        for id in 1..=128 {
            let word = fixture.sig(id);
            let (hits, acc) = retriever.retrieve(&query(word), &policy(256, 1)).unwrap();
            check_hits(&fixture, word, &hits, &acc);
            assert_eq!(hits[0].hamming_distance, 0, "{} id {id}", dist.name());
            assert!(hits[0].source_exact);
        }
    }
}
