//! CONTEXT-3 G1 tranche: two-level (intra-bucket) shortlist ordering.
//! When probed buckets hold more candidates than `max_shortlist`, the
//! single-level engine fills the cap in insertion order. The two-level
//! engine fills it in (secondary-key Hamming distance, secondary key,
//! insertion) order, where the secondary key is the TOP `bucket_prefix_bits`
//! bits of the first SBG word (declared before measuring). Same cap.
//!
//! Note: the clustered fixture shares its low 16 bits within a cluster, so a
//! high-bit secondary key is favourable for it by construction; all three
//! distributions are reported, including cells where it does not help.
//! Synthetic only; NOT a LONGCTX-B1 measurement; no generalization claim.
//! Report: `cargo test -p sml-store --test context_retrieve_two_level
//! -- --nocapture --test-threads=1`. Peak RSS / release latency NOT_MEASURED.

use std::{
    fs,
    path::PathBuf,
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
        ContextRetrievePolicy, ContextRetriever, ContextRetrieverConfig, RetrieveAccounting,
        RetrieveHit, RetrieveQuery,
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
            std::env::temp_dir().join(format!("sml-context-twolevel-{}-{id}", std::process::id()));
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
    time: Duration,
    secondary: u64,
}

fn measure(
    fixture: &Fixture,
    retriever: &ContextRetriever<BITS>,
    radius: u32,
    max_shortlist: usize,
    queries: u64,
    two_level: bool,
) -> Cell {
    let mut cell = Cell::default();
    for seed in 0..queries {
        let word = fixture.dist.query(fixture.segments, seed);
        cell.queries += 1;
        for (slot, &k) in KS.iter().enumerate() {
            let oracle = oracle_top_k(fixture, word, k);
            let kth = oracle.last().unwrap().0;
            let t0 = Instant::now();
            let (hits, acc) = if two_level {
                retriever.retrieve_two_level(&query(word), &policy(max_shortlist, k), radius)
            } else {
                retriever.retrieve_with_neighbor_radius(
                    &query(word),
                    &policy(max_shortlist, k),
                    radius,
                )
            }
            .unwrap();
            cell.secondary += acc.secondary_keys_scanned;
            cell.time += t0.elapsed();
            check_hits(fixture, word, &hits, &acc);
            assert!(acc.shortlisted <= max_shortlist as u64);
            for (hit, best) in hits.iter().zip(&oracle) {
                assert!(hit.hamming_distance >= best.0);
            }
            if radius == 0 && !two_level {
                // Radius 0 must be byte-for-byte the existing engine.
                let (base, base_acc) = retriever
                    .retrieve(&query(word), &policy(max_shortlist, k))
                    .unwrap();
                assert_eq!(base, hits);
                assert_eq!(base_acc, acc);
            }
            cell.recall_sum[slot] += recall_at_k(&hits, &oracle, kth);
            if hits.is_empty() {
                cell.abstain += 1;
            }
            cell.shortlisted += acc.shortlisted;
            cell.decoded += acc.decoded;
            cell.reread_bytes += acc.raw_reread_bytes;
        }
    }
    cell
}

#[test]
fn g1_two_level_matrix_vs_single_level_same_cap() {
    let mut stream = 2800;
    println!(
        "G1_TWOLEVEL dist size prefix_bits radius cap engine R@1 R@2 R@4 abstain_rate \
         mean_shortlisted mean_secondary_keys mean_reread_bytes us_per_query"
    );
    for dist in [Dist::Clustered, Dist::NoisyClustered, Dist::Uniform] {
        for size in [256_u64, 1024] {
            let fixture = Fixture::build(stream, size, dist);
            stream += 1;
            for prefix_bits in [4_u32, 8] {
                let retriever = fixture.open(prefix_bits);
                for radius in [0_u32, 1] {
                    for cap in [32_usize, 64] {
                        let mut shortlisted = [0_u64; 2];
                        for (e, two) in [false, true].into_iter().enumerate() {
                            let cell = measure(&fixture, &retriever, radius, cap, 48, two);
                            let calls = (cell.queries * KS.len() as u64) as f64;
                            let r: Vec<f64> = cell
                                .recall_sum
                                .iter()
                                .map(|s| s / cell.queries as f64)
                                .collect();
                            println!(
                                "G1_TWOLEVEL {} {size} {prefix_bits} {radius} {cap} {} {:.3} \
                                 {:.3} {:.3} {:.3} {:.1} {:.1} {:.1} {:.1}",
                                dist.name(),
                                if two { "two_level" } else { "single" },
                                r[0],
                                r[1],
                                r[2],
                                cell.abstain as f64 / calls,
                                cell.shortlisted as f64 / calls,
                                cell.secondary as f64 / calls,
                                cell.reread_bytes as f64 / calls,
                                cell.time.as_secs_f64() * 1e6 / calls,
                            );
                            if !two {
                                assert_eq!(cell.secondary, 0);
                            }
                            shortlisted[e] = cell.shortlisted;
                        }
                        // Same probe set, same cap: identical shortlist size.
                        assert_eq!(shortlisted[0], shortlisted[1]);
                    }
                }
            }
        }
    }
}

#[test]
fn g1_two_level_equals_single_level_when_cap_covers_buckets() {
    let fixture = Fixture::build(2900, 64, Dist::Uniform);
    let retriever = fixture.open(4);
    for seed in 0..16 {
        let word = fixture.dist.query(64, seed);
        for k in KS {
            for radius in [0_u32, 1] {
                let p = policy(64, k);
                let (a, aa) = retriever
                    .retrieve_with_neighbor_radius(&query(word), &p, radius)
                    .unwrap();
                let (b, ba) = retriever
                    .retrieve_two_level(&query(word), &p, radius)
                    .unwrap();
                check_hits(&fixture, word, &b, &ba);
                assert_eq!(a, b);
                assert_eq!(aa.shortlisted, ba.shortlisted);
            }
        }
    }
}

#[test]
fn g1_two_level_rejects_over_radius_and_respects_cap() {
    let fixture = Fixture::build(2901, 256, Dist::Clustered);
    let retriever = fixture.open(4);
    assert!(retriever
        .retrieve_two_level(&query(0), &policy(8, 1), 5)
        .is_err());
    for seed in 0..16 {
        let word = fixture.dist.query(256, seed);
        let (hits, acc) = retriever
            .retrieve_two_level(&query(word), &policy(8, 4), 1)
            .unwrap();
        check_hits(&fixture, word, &hits, &acc);
        assert!(acc.shortlisted <= 8);
    }
}

#[test]
fn g1_two_level_corruption_fails_closed() {
    let fixture = Fixture::build(2902, 64, Dist::Clustered);
    let retriever = fixture.open(4);
    let word = fixture.dist.query(64, 3);
    let (hits, _) = retriever
        .retrieve_two_level(&query(word), &policy(16, 1), 0)
        .unwrap();
    let id = hits[0].segment.segment_id;
    let path = retriever.raw_path(id);
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(&path, bytes).unwrap();
    assert!(retriever
        .retrieve_two_level(&query(word), &policy(16, 1), 0)
        .is_err());
}

// ---- G1 CONTEXT-3 tranche: adaptive secondary width (same cap) ----
// `retrieve_multi_level(.., secondary_bits)` orders an overflowing bucket by
// Hamming distance over the TOP `secondary_bits` bits of the first SBG word
// (computed from the in-memory index signatures; no extra raw reread).
// secondary_bits == bucket_prefix_bits reproduces `retrieve_two_level`.
// Width 64 - prefix_bits on BITS=64 is an exact in-bucket ordering by the
// non-bucket bits: it costs one popcount per overflowing-bucket member
// (reported as secondary_keys_scanned), NOT a wider cap or extra reread.

fn measure_multi(
    fixture: &Fixture,
    retriever: &ContextRetriever<BITS>,
    radius: u32,
    cap: usize,
    queries: u64,
    width: u32,
) -> Cell {
    let mut cell = Cell::default();
    for seed in 0..queries {
        let word = fixture.dist.query(fixture.segments, seed);
        cell.queries += 1;
        for (slot, &k) in KS.iter().enumerate() {
            let oracle = oracle_top_k(fixture, word, k);
            let kth = oracle.last().unwrap().0;
            let t0 = Instant::now();
            let (hits, acc) = retriever
                .retrieve_multi_level(&query(word), &policy(cap, k), radius, width)
                .unwrap();
            cell.time += t0.elapsed();
            check_hits(fixture, word, &hits, &acc);
            assert!(acc.shortlisted <= cap as u64);
            cell.secondary += acc.secondary_keys_scanned;
            cell.recall_sum[slot] += recall_at_k(&hits, &oracle, kth);
            if hits.is_empty() {
                cell.abstain += 1;
            }
            cell.shortlisted += acc.shortlisted;
            cell.reread_bytes += acc.raw_reread_bytes;
        }
    }
    cell
}

#[test]
fn g1_multi_level_width_equal_prefix_matches_two_level() {
    let fixture = Fixture::build(2910, 256, Dist::Clustered);
    for prefix in [4_u32, 8] {
        let retriever = fixture.open(prefix);
        for seed in 0..16 {
            let word = fixture.dist.query(256, seed);
            for k in KS {
                for radius in [0_u32, 1] {
                    let p = policy(32, k);
                    let (a, aa) = retriever
                        .retrieve_two_level(&query(word), &p, radius)
                        .unwrap();
                    let (b, ba) = retriever
                        .retrieve_multi_level(&query(word), &p, radius, prefix)
                        .unwrap();
                    assert_eq!(a, b);
                    assert_eq!(aa.shortlisted, ba.shortlisted);
                }
            }
        }
    }
}

#[test]
fn g1_multi_level_rejects_bad_width_and_radius() {
    let fixture = Fixture::build(2911, 64, Dist::Clustered);
    let retriever = fixture.open(8);
    assert!(retriever
        .retrieve_multi_level(&query(0), &policy(8, 1), 0, 0)
        .is_err());
    assert!(retriever
        .retrieve_multi_level(&query(0), &policy(8, 1), 0, 65)
        .is_err());
    assert!(retriever
        .retrieve_multi_level(&query(0), &policy(8, 1), 9, 8)
        .is_err());
}

#[test]
fn g1_multi_level_matrix_same_cap() {
    let mut stream = 2920;
    println!(
        "G1_MULTILEVEL dist size prefix_bits radius cap width R@1 R@2 R@4 abstain \
         mean_shortlisted mean_secondary_scanned mean_reread_bytes us_per_query"
    );
    for dist in [Dist::Clustered, Dist::NoisyClustered, Dist::Uniform] {
        for size in [256_u64, 1024] {
            let fixture = Fixture::build(stream, size, dist);
            stream += 1;
            for prefix in [4_u32, 8] {
                let retriever = fixture.open(prefix);
                for radius in [0_u32, 1] {
                    let cap = 32;
                    let single = measure(&fixture, &retriever, radius, cap, 48, false);
                    let mut rows = vec![("single".to_string(), single)];
                    for width in [prefix, 16, 32, 64 - prefix] {
                        let c = measure_multi(&fixture, &retriever, radius, cap, 48, width);
                        rows.push((format!("w{width}"), c));
                    }
                    let base_short = rows[0].1.shortlisted;
                    for (name, cell) in &rows {
                        let calls = (cell.queries * KS.len() as u64) as f64;
                        let r: Vec<f64> = cell
                            .recall_sum
                            .iter()
                            .map(|s| s / cell.queries as f64)
                            .collect();
                        println!(
                            "G1_MULTILEVEL {} {size} {prefix} {radius} {cap} {name} {:.3} {:.3} \
                             {:.3} {:.3} {:.1} {:.1} {:.1} {:.1}",
                            dist.name(),
                            r[0],
                            r[1],
                            r[2],
                            cell.abstain as f64 / calls,
                            cell.shortlisted as f64 / calls,
                            cell.secondary as f64 / calls,
                            cell.reread_bytes as f64 / calls,
                            cell.time.as_secs_f64() * 1e6 / calls,
                        );
                        // Same probe set, same cap: identical shortlist size.
                        assert_eq!(cell.shortlisted, base_short);
                    }
                }
            }
        }
    }
}
