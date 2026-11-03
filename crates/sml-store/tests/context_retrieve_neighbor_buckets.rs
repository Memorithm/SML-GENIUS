//! CONTEXT-3 G1 tranche: neighbour-bucket shortlist. Compares the current
//! single-bucket shortlist (radius 0, identical to `retrieve`) against
//! `retrieve_with_neighbor_radius` at bucket-key Hamming radius 1 (primary,
//! chosen: probes prefix_bits+1 buckets) and 2 (secondary), on the same
//! synthetic matrix as `context_retrieve_recall_k`, vs the exhaustive oracle.
//!
//! Synthetic only; NOT a LONGCTX-B1 measurement; no generalization claim.
//! Report: `cargo test -p sml-store --test context_retrieve_neighbor_buckets
//! -- --nocapture`. Peak RSS is NOT_MEASURED.

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
            std::env::temp_dir().join(format!("sml-context-neighbor-{}-{id}", std::process::id()));
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

fn dir_bytes(path: &Path) -> u64 {
    fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum()
}

const RADII: [u32; 3] = [0, 1, 2];

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

#[test]
fn g1_neighbor_bucket_recall_matrix_vs_single_bucket() {
    let mut stream = 800;
    println!(
        "G1_NEIGHBOR dist size prefix_bits radius R@1 R@2 R@4 abstain_rate mean_shortlisted \
         mean_decoded mean_reread_bytes us_per_query store_bytes"
    );
    let mut rows = Vec::new();
    for dist in [Dist::Uniform, Dist::Clustered, Dist::NoisyClustered] {
        for size in [64_u64, 256, 1024] {
            let fixture = Fixture::build(stream, size, dist);
            stream += 1;
            let store = dir_bytes(&fixture.temp.path);
            for prefix_bits in [4_u32, 8] {
                let retriever = fixture.open(prefix_bits);
                for radius in RADII {
                    let cell = measure(&fixture, &retriever, radius, 32, 48);
                    let calls = (cell.queries * KS.len() as u64) as f64;
                    let r: Vec<f64> = cell
                        .recall_sum
                        .iter()
                        .map(|s| s / cell.queries as f64)
                        .collect();
                    println!(
                        "G1_NEIGHBOR {} {size} {prefix_bits} {radius} {:.3} {:.3} {:.3} {:.3} \
                         {:.1} {:.2} {:.1} {:.1} {store}",
                        dist.name(),
                        r[0],
                        r[1],
                        r[2],
                        cell.abstain as f64 / calls,
                        cell.shortlisted as f64 / calls,
                        cell.decoded as f64 / calls,
                        cell.reread_bytes as f64 / calls,
                        cell.time.as_secs_f64() * 1e6 / calls,
                    );
                    rows.push((dist.name(), size, prefix_bits, radius, r, cell.shortlisted));
                }
            }
        }
    }
    // Structural: a wider radius never shortlists fewer candidates (same cap).
    for w in rows.chunks(RADII.len()) {
        assert!(w[0].5 <= w[1].5 && w[1].5 <= w[2].5);
    }
    // Measured on this fixed seed set (not a generalization claim): on the
    // uniform 1024 / 8-bit cell, radius 1 Recall@1 >= radius 0.
    let cell = |radius| {
        rows.iter()
            .find(|r| r.0 == "uniform" && r.1 == 1024 && r.2 == 8 && r.3 == radius)
            .unwrap()
            .4[0]
    };
    assert!(cell(1) >= cell(0));
}

#[test]
fn g1_neighbor_radius_recovers_one_bit_bucket_miss() {
    let fixture = Fixture::build(900, 256, Dist::Uniform);
    let prefix_bits = 8;
    let retriever = fixture.open(prefix_bits);
    let mut recovered = 0;
    let mut cases = 0;
    for target in 1..=64_u64 {
        let word = fixture.sig(target) ^ (1 << (target % u64::from(prefix_bits)));
        if oracle_top_k(&fixture, word, 1)[0] != (1, target) {
            continue;
        }
        cases += 1;
        let (miss, _) = retriever.retrieve(&query(word), &policy(256, 1)).unwrap();
        assert!(miss.iter().all(|h| h.segment.segment_id != target));
        let (hits, acc) = retriever
            .retrieve_with_neighbor_radius(&query(word), &policy(256, 1), 1)
            .unwrap();
        check_hits(&fixture, word, &hits, &acc);
        assert_eq!(hits[0].segment.segment_id, target);
        assert_eq!(hits[0].hamming_distance, 1);
        assert!(!hits[0].source_exact);
        recovered += 1;
    }
    assert!(cases >= 32 && recovered == cases);
    println!("G1_NEIGHBOR_RECOVER radius=1 recovered={recovered}/{cases}");
}

#[test]
fn g1_neighbor_radius_fails_honestly_beyond_radius() {
    // Flip two bucket bits: the true nearest is at key distance 2, outside radius 1.
    let fixture = Fixture::build(901, 256, Dist::Uniform);
    let retriever = fixture.open(8);
    let mut cases = 0;
    let mut r1_found = 0;
    let mut r2_found = 0;
    for target in 1..=64_u64 {
        let a = target % 8;
        let b = (a + 3) % 8;
        let word = fixture.sig(target) ^ (1 << a) ^ (1 << b);
        if oracle_top_k(&fixture, word, 1)[0] != (2, target) {
            continue;
        }
        cases += 1;
        let (h1, acc1) = retriever
            .retrieve_with_neighbor_radius(&query(word), &policy(256, 1), 1)
            .unwrap();
        check_hits(&fixture, word, &h1, &acc1);
        if h1.first().map(|h| h.segment.segment_id) == Some(target) {
            r1_found += 1;
        }
        let (h2, acc2) = retriever
            .retrieve_with_neighbor_radius(&query(word), &policy(256, 1), 2)
            .unwrap();
        check_hits(&fixture, word, &h2, &acc2);
        if h2.first().map(|h| h.segment.segment_id) == Some(target) {
            r2_found += 1;
        }
    }
    assert!(cases >= 16, "only {cases} clean cases");
    assert_eq!(r1_found, 0, "target bucket is outside radius 1");
    assert_eq!(r2_found, cases);
    println!("G1_NEIGHBOR_BEYOND cases={cases} r1_found={r1_found} r2_found={r2_found}");
}

#[test]
fn g1_neighbor_radius_abstains_and_rejects_over_radius() {
    let fixture = Fixture::build(902, 4, Dist::Uniform);
    let retriever = fixture.open(8);
    let keys: Vec<u64> = (1..=4).map(|id| fixture.sig(id) & 0xff).collect();
    let empty = (0..256_u64)
        .find(|k| keys.iter().all(|o| (o ^ k).count_ones() > 1))
        .unwrap();
    let word = (fixture.sig(1) & !0xff) | empty;
    let (hits, acc) = retriever
        .retrieve_with_neighbor_radius(&query(word), &policy(8, 1), 1)
        .unwrap();
    assert!(hits.is_empty());
    assert_eq!(acc, RetrieveAccounting::default());
    assert!(retriever
        .retrieve_with_neighbor_radius(&query(word), &policy(8, 1), 9)
        .is_err());
    // Full radius == prefix_bits probes every bucket: equals oracle top-1 by distance.
    let (all, _) = retriever
        .retrieve_with_neighbor_radius(&query(word), &policy(8, 1), 8)
        .unwrap();
    assert_eq!(
        all[0].hamming_distance,
        oracle_top_k(&fixture, word, 1)[0].0
    );
}

#[test]
fn g1_neighbor_bucket_corruption_fails_closed() {
    let fixture = Fixture::build(903, 256, Dist::Uniform);
    let retriever = fixture.open(8);
    let target = (1..=64_u64)
        .find(|&t| {
            let w = fixture.sig(t) ^ 1;
            oracle_top_k(&fixture, w, 1)[0] == (1, t)
        })
        .unwrap();
    let word = fixture.sig(target) ^ 1;
    let path = retriever.raw_path(target);
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    fs::write(&path, bytes).unwrap();
    // Radius 0 never rereads the victim, radius 1 does and must fail closed.
    assert!(retriever.retrieve(&query(word), &policy(256, 1)).is_ok());
    assert!(retriever
        .retrieve_with_neighbor_radius(&query(word), &policy(256, 1), 1)
        .is_err());
}
