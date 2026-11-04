//! Shared synthetic CONTEXT-3 G1 fixture (same declared distributions as
//! `tests/context_retrieve_two_level.rs`). Synthetic only.
#![allow(dead_code)]

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use sml_core::{
    context_ledger::ContextStreamId,
    semantic::{SemanticBitField, SemanticState},
};
use sml_store::{
    context_ingest::{ContextIngestError, SegmentTokenCounter, StreamingIngestPolicy},
    context_retrieve::{
        ContextRetrievePolicy, ContextRetriever, ContextRetrieverConfig, RetrieveQuery,
    },
    context_semantic_ingest::{
        ContextSemanticIngestError, SegmentEmissionContext, SemanticIngestPolicy,
        SemanticSegmentEmitter, SemanticStreamingContextIngestor,
    },
    raw_context::RawContextStoreLimits,
};

pub const BITS: usize = 64;
pub const SEGMENT_BYTES: usize = 16;
const RECORD_ID_BASE: u64 = 1_000_000;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

pub struct TempDir {
    pub path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("sml-g1-latency-{}-{id}", std::process::id()));
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

pub fn splitmix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[derive(Clone, Copy, Debug)]
pub enum Dist {
    Uniform,
    Clustered,
    NoisyClustered,
}

impl Dist {
    pub fn name(self) -> &'static str {
        match self {
            Self::Uniform => "uniform",
            Self::Clustered => "clustered",
            Self::NoisyClustered => "noisy_clustered",
        }
    }

    pub fn signature(self, order: u64) -> u64 {
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

    pub fn query(self, segments: u64, seed: u64) -> u64 {
        let mut word = self.signature(splitmix(seed ^ 0xfeed) % segments);
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

/// Key source: a synthetic distribution, or explicit precomputed keys
/// (index = order). Explicit keys are used by the flagged real-text arm.
#[derive(Clone)]
pub enum Keys {
    Dist(Dist),
    Explicit(Vec<u64>),
}

impl Keys {
    fn key(&self, order: u64) -> u64 {
        match self {
            Self::Dist(d) => d.signature(order),
            Self::Explicit(v) => v[order as usize],
        }
    }
}

struct Emitter(Keys);

impl SemanticSegmentEmitter<BITS> for Emitter {
    fn emit_segment(
        &mut self,
        _raw_segment: &[u8],
        context: SegmentEmissionContext,
    ) -> Result<Vec<(u64, SemanticState<BITS>)>, ContextSemanticIngestError> {
        let mut state = SemanticState::<BITS>::zeroed().unwrap();
        state.value =
            SemanticBitField::<BITS>::from_words(vec![self.0.key(context.order_index)]).unwrap();
        state.meaning_mask = SemanticBitField::<BITS>::ones().unwrap();
        Ok(vec![(RECORD_ID_BASE + context.segment_id, state)])
    }
}

pub struct Fixture {
    pub temp: TempDir,
    pub stream: ContextStreamId,
    pub segments: u64,
    pub dist: Dist,
    keys: Keys,
}

impl Fixture {
    pub fn build(stream: u64, segments: u64, dist: Dist) -> Self {
        Self::build_keys(stream, segments, dist, Keys::Dist(dist))
    }

    /// Index explicit keys (one segment per key). `dist` is a label only.
    pub fn build_explicit(stream: u64, keys: Vec<u64>) -> Self {
        let n = keys.len() as u64;
        Self::build_keys(stream, n, Dist::Uniform, Keys::Explicit(keys))
    }

    fn build_keys(stream: u64, segments: u64, dist: Dist, keys: Keys) -> Self {
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
            .ingest_reader(&corpus[..], &mut ByteTokens, &mut Emitter(keys.clone()))
            .unwrap();
        assert_eq!(outcome.stats.segments_published, segments);
        Self {
            temp,
            stream,
            segments,
            dist,
            keys,
        }
    }

    pub fn open(&self, prefix_bits: u32) -> ContextRetriever<BITS> {
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

    pub fn sig(&self, segment_id: u64) -> u64 {
        self.keys.key(segment_id - 1)
    }

    /// Oracle nearest Hamming distance over all segments.
    pub fn oracle_best(&self, word: u64) -> u64 {
        (1..=self.segments)
            .map(|id| u64::from((self.sig(id) ^ word).count_ones()))
            .min()
            .unwrap()
    }
}

pub fn policy(max_shortlist: usize, top_k: usize) -> ContextRetrievePolicy {
    ContextRetrievePolicy {
        max_shortlist,
        top_k,
        associative_slots: 8,
        fail_on_hash_mismatch: true,
    }
}

pub fn query(word: u64) -> RetrieveQuery<BITS> {
    RetrieveQuery {
        bitfield: SemanticBitField::<BITS>::from_words(vec![word]).unwrap(),
        associative_key: None,
    }
}
