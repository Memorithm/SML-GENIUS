//! CONTEXT-3 G1 tranche: shortlist-cap sweep. Measures how much of the weak
//! clustered N=1024 Recall@K after #804 is caused by the fixed
//! `max_shortlist = 32` cap truncating oversized buckets, by sweeping the cap
//! {32, 64, 128, 256} at radius 0/1 on the same synthetic matrix and seeds.
//! Also counts, per query, whether the probed buckets held more candidates
//! than the cap (`overflow_rate`), computed independently from the declared
//! signatures. No engine change; measurement only.
//!
//! Synthetic only; NOT a LONGCTX-B1 measurement; no generalization claim.
//! Report: `cargo test -p sml-store --test context_retrieve_cap_sweep
//! -- --nocapture`. Peak RSS NOT_MEASURED; release latency NOT_MEASURED.

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
            std::env::temp_dir().join(format!("sml-context-capsweep-{}-{id}", std::process::id()));
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

const RADII: [u32; 2] = [0, 1];
const CAPS: [usize; 4] = [32, 64, 128, 256];

#[derive(Default)]
struct Cell {
    recall_sum: [f64; 3],
    abstain: u64,
    queries: u64,
    shortlisted: u64,
    decoded: u64,
    reread_bytes: u64,
    time: Duration,
}

fn measure(
    fixture: &Fixture,
    retriever: &ContextRetriever<BITS>,
    radius: u32,
    max_shortlist: usize,
    queries: u64,
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
                .retrieve_with_neighbor_radius(&query(word), &policy(max_shortlist, k), radius)
                .unwrap();
            cell.time += t0.elapsed();
            check_hits(fixture, word, &hits, &acc);
            assert!(acc.shortlisted <= max_shortlist as u64);
            for (hit, best) in hits.iter().zip(&oracle) {
                assert!(hit.hamming_distance >= best.0);
            }
            if radius == 0 {
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

/// Independent count of candidates in buckets within `radius` of the query key.
fn probed_population(fixture: &Fixture, word: u64, prefix_bits: u32, radius: u32) -> u64 {
    let mask = if prefix_bits == 64 {
        u64::MAX
    } else {
        (1u64 << prefix_bits) - 1
    };
    (1..=fixture.segments)
        .filter(|&id| ((fixture.sig(id) ^ word) & mask).count_ones() <= radius)
        .count() as u64
}

#[test]
fn g1_shortlist_cap_sweep_clustered() {
    let mut stream = 1800;
    println!(
        "G1_CAPSWEEP dist size prefix_bits radius cap R@1 R@2 R@4 abstain_rate overflow_rate \
         mean_shortlisted mean_decoded mean_reread_bytes us_per_query"
    );
    let mut rows = Vec::new();
    for dist in [Dist::Clustered, Dist::NoisyClustered, Dist::Uniform] {
        let size = 1024_u64;
        let fixture = Fixture::build(stream, size, dist);
        stream += 1;
        for prefix_bits in [4_u32, 8] {
            let retriever = fixture.open(prefix_bits);
            for radius in RADII {
                let mut overflow = 0_u64;
                for seed in 0..48 {
                    let word = fixture.dist.query(size, seed);
                    if probed_population(&fixture, word, prefix_bits, radius) > 32 {
                        overflow += 1;
                    }
                }
                for cap in CAPS {
                    let cell = measure(&fixture, &retriever, radius, cap, 48);
                    let calls = (cell.queries * KS.len() as u64) as f64;
                    let r: Vec<f64> = cell
                        .recall_sum
                        .iter()
                        .map(|s| s / cell.queries as f64)
                        .collect();
                    let mut over_cap = 0_u64;
                    for seed in 0..48 {
                        let word = fixture.dist.query(size, seed);
                        if probed_population(&fixture, word, prefix_bits, radius) > cap as u64 {
                            over_cap += 1;
                        }
                    }
                    println!(
                        "G1_CAPSWEEP {} {size} {prefix_bits} {radius} {cap} {:.3} {:.3} {:.3} \
                         {:.3} {:.3} {:.1} {:.2} {:.1} {:.1}",
                        dist.name(),
                        r[0],
                        r[1],
                        r[2],
                        cell.abstain as f64 / calls,
                        over_cap as f64 / 48.0,
                        cell.shortlisted as f64 / calls,
                        cell.decoded as f64 / calls,
                        cell.reread_bytes as f64 / calls,
                        cell.time.as_secs_f64() * 1e6 / calls,
                    );
                    rows.push((dist.name(), prefix_bits, radius, cap, r, cell.shortlisted));
                }
                let _ = overflow;
            }
        }
    }
    // Structural: a larger cap never shortlists fewer candidates.
    for w in rows.chunks(CAPS.len()) {
        for pair in w.windows(2) {
            assert!(pair[0].5 <= pair[1].5);
        }
    }
}

#[test]
fn g1_cap_sweep_never_exceeds_cap_and_matches_oracle_when_uncapped() {
    // When the cap covers every probed candidate, ranking inside the probed
    // buckets must equal the oracle restricted to those buckets.
    let fixture = Fixture::build(1900, 256, Dist::Clustered);
    let prefix_bits = 4;
    let retriever = fixture.open(prefix_bits);
    let mask = (1u64 << prefix_bits) - 1;
    for seed in 0..16 {
        let word = fixture.dist.query(256, seed);
        let (hits, acc) = retriever
            .retrieve_with_neighbor_radius(&query(word), &policy(256, 4), 0)
            .unwrap();
        check_hits(&fixture, word, &hits, &acc);
        let mut restricted: Vec<(u64, u64)> = (1..=fixture.segments)
            .filter(|&id| (fixture.sig(id) ^ word) & mask == 0)
            .map(|id| (u64::from((fixture.sig(id) ^ word).count_ones()), id))
            .collect();
        restricted.sort_unstable();
        assert_eq!(acc.shortlisted, restricted.len() as u64);
        for (hit, want) in hits.iter().zip(&restricted) {
            assert_eq!(hit.hamming_distance, want.0);
        }
    }
}
