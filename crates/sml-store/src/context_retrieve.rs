//! CONTEXT-3 hierarchical retrieval (tranche B, approach 1).
//!
//! Pipeline: SBG query bitfield -> bounded SBG prefix-bucket shortlist ->
//! stable MOR top-k ranking by exact Hamming distance -> optional bounded
//! associative lookup -> raw reread with SHA-256 verification against the
//! CONTEXT-1c provenance manifest -> accounting emission.
//!
//! This is not a context window. A mapped or ingested store is not effective
//! context. The sublinear-work test in `tests/context_retrieve.rs` is
//! fixture-only and supports no scale claim.

use std::{
    collections::BTreeMap,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use sml_core::{
    context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId},
    memory::{BoundedAssociativeMemory, ReplacementPolicy},
    semantic::{SemanticBitField, SemanticMorTopK, SemanticStoreView},
};

use crate::{
    context_semantic_ingest::{parse_provenance_artifact, ContextProvenanceManifest},
    raw_context::RawContextStoreLimits,
};

/// Width of the associative value: one stable `segment_id` (u64).
pub const ASSOCIATIVE_VALUE_BITS: usize = 64;

/// Per-query retrieval budget (frozen CONTEXT-3 shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextRetrievePolicy {
    pub max_shortlist: usize,
    pub top_k: usize,
    pub associative_slots: usize,
    /// Must be `true`; `false` is rejected by [`ContextRetrievePolicy::validate`].
    pub fail_on_hash_mismatch: bool,
}

impl ContextRetrievePolicy {
    /// Rejects zero budgets, `top_k` outside the MOR fanouts {1, 2, 4},
    /// `top_k > max_shortlist` and `fail_on_hash_mismatch == false`.
    pub fn validate(self) -> Result<Self, ContextRetrieveError> {
        if !self.fail_on_hash_mismatch || self.max_shortlist == 0 || self.top_k > self.max_shortlist
        {
            return Err(ContextRetrieveError::BudgetOverflow);
        }
        mor_top_k(self.top_k)?;
        Ok(self)
    }
}

/// Index construction limits for one retriever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextRetrieverConfig {
    pub raw_limits: RawContextStoreLimits,
    pub max_provenance_bytes: u64,
    pub max_semantic_store_bytes: u64,
    /// Low bits of the first SBG `value` word used as the shortlist bucket key.
    pub bucket_prefix_bits: u32,
    /// Capacity of the bounded associative memory; zero disables it.
    pub associative_capacity: usize,
}

impl ContextRetrieverConfig {
    pub fn validate<const BITS: usize>(self) -> Result<Self, ContextRetrieveError> {
        if self.bucket_prefix_bits == 0
            || self.bucket_prefix_bits > 64
            || self.bucket_prefix_bits as usize > BITS
            || self.max_provenance_bytes == 0
            || self.max_semantic_store_bytes == 0
        {
            return Err(ContextRetrieveError::BudgetOverflow);
        }
        self.raw_limits
            .validate()
            .map_err(|_| ContextRetrieveError::BudgetOverflow)?;
        Ok(self)
    }
}

/// Ranked candidate: segment id, SBG score and provenance reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextCandidate {
    pub segment: ContextSegmentId,
    pub hamming_distance: u64,
    pub content_hash: ContextContentHash,
}

/// Query bitfield plus optional associative key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrieveQuery<const BITS: usize> {
    pub bitfield: SemanticBitField<BITS>,
    pub associative_key: Option<u64>,
}

/// One verified hit. Raw bytes are returned only after SHA-256 verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrieveHit {
    pub segment: ContextSegmentId,
    pub rank: usize,
    pub hamming_distance: u64,
    /// `true` only when the SBG signature matches the query exactly.
    pub source_exact: bool,
    pub byte_start: u64,
    pub byte_len: u64,
    pub content_hash: ContextContentHash,
    pub raw_bytes: Vec<u8>,
}

/// Work accounting. Invariant: `shortlisted >= decoded >= raw_reread_segments`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetrieveAccounting {
    pub shortlisted: u64,
    pub decoded: u64,
    pub raw_reread_segments: u64,
    pub raw_reread_bytes: u64,
    pub associative_hits: u64,
    pub associative_misses: u64,
}

impl RetrieveAccounting {
    #[must_use]
    pub fn invariant_holds(&self) -> bool {
        self.shortlisted >= self.decoded && self.decoded >= self.raw_reread_segments
    }
}

#[derive(Debug)]
pub enum ContextRetrieveError {
    EmptyIndex,
    BudgetOverflow,
    HashMismatch,
    MissingRaw,
    MissingProvenance,
    AssociativeDenied,
    Io(io::Error),
}

impl fmt::Display for ContextRetrieveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ContextRetrieveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ContextRetrieveError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexEntry<const BITS: usize> {
    segment: ContextSegmentId,
    content_hash: ContextContentHash,
    signature: SemanticBitField<BITS>,
}

/// Read-only retriever over one CONTEXT-1c output directory.
#[derive(Debug, Clone)]
pub struct ContextRetriever<const BITS: usize> {
    directory: PathBuf,
    stream_id: ContextStreamId,
    config: ContextRetrieverConfig,
    entries: Vec<IndexEntry<BITS>>,
    buckets: BTreeMap<u64, Vec<usize>>,
    memory: Option<BoundedAssociativeMemory>,
}

impl<const BITS: usize> ContextRetriever<BITS> {
    /// Indexes segments `1..=segment_count` written by
    /// `SemanticStreamingContextIngestor<BITS>` into `directory`.
    pub fn open(
        directory: impl AsRef<Path>,
        stream_id: ContextStreamId,
        segment_count: u64,
        config: ContextRetrieverConfig,
    ) -> Result<Self, ContextRetrieveError> {
        let config = config.validate::<BITS>()?;
        if segment_count == 0 {
            return Err(ContextRetrieveError::EmptyIndex);
        }
        let mut retriever = Self {
            directory: directory.as_ref().to_path_buf(),
            stream_id,
            config,
            entries: Vec::new(),
            buckets: BTreeMap::new(),
            memory: None,
        };
        for segment_id in 1..=segment_count {
            let segment = ContextSegmentId::new(stream_id, segment_id)
                .map_err(|_| ContextRetrieveError::EmptyIndex)?;
            let manifest = retriever.load_manifest(segment_id)?;
            let signature = retriever.load_signature(segment_id, &manifest)?;
            let index = retriever.entries.len();
            retriever
                .buckets
                .entry(bucket_key(&signature, config.bucket_prefix_bits))
                .or_default()
                .push(index);
            retriever.entries.push(IndexEntry {
                segment,
                content_hash: manifest.content_hash,
                signature,
            });
        }
        if config.associative_capacity != 0 {
            retriever.memory = Some(
                BoundedAssociativeMemory::new(
                    config.associative_capacity,
                    ASSOCIATIVE_VALUE_BITS,
                    ReplacementPolicy::RejectCollision,
                )
                .map_err(|_| ContextRetrieveError::AssociativeDenied)?,
            );
        }
        Ok(retriever)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn raw_path(&self, segment_id: u64) -> PathBuf {
        self.directory
            .join(base_name(self.stream_id, segment_id) + ".smlctx")
    }

    #[must_use]
    pub fn semantic_path(&self, segment_id: u64) -> PathBuf {
        self.directory
            .join(base_name(self.stream_id, segment_id) + ".smlsbg")
    }

    #[must_use]
    pub fn provenance_path(&self, segment_id: u64) -> PathBuf {
        self.directory
            .join(base_name(self.stream_id, segment_id) + ".smlprov")
    }

    fn load_manifest(
        &self,
        segment_id: u64,
    ) -> Result<ContextProvenanceManifest, ContextRetrieveError> {
        let bytes = read_bounded(
            &self.provenance_path(segment_id),
            self.config.max_provenance_bytes,
        )?
        .ok_or(ContextRetrieveError::MissingProvenance)?;
        let manifest = parse_provenance_artifact(&bytes)
            .map_err(|_| ContextRetrieveError::MissingProvenance)?;
        if manifest.stream_id != self.stream_id
            || manifest.segment_id != segment_id
            || manifest.semantic_record_ids.is_empty()
        {
            return Err(ContextRetrieveError::MissingProvenance);
        }
        Ok(manifest)
    }

    fn load_signature(
        &self,
        segment_id: u64,
        manifest: &ContextProvenanceManifest,
    ) -> Result<SemanticBitField<BITS>, ContextRetrieveError> {
        let bytes = read_bounded(
            &self.semantic_path(segment_id),
            self.config.max_semantic_store_bytes,
        )?
        .ok_or(ContextRetrieveError::MissingProvenance)?;
        let view = SemanticStoreView::<BITS>::parse(&bytes)
            .map_err(|_| ContextRetrieveError::MissingProvenance)?;
        let state = view
            .get(manifest.semantic_record_ids[0])
            .map_err(|_| ContextRetrieveError::MissingProvenance)?
            .ok_or(ContextRetrieveError::MissingProvenance)?;
        Ok(state.value)
    }
}

fn mor_top_k(top_k: usize) -> Result<SemanticMorTopK, ContextRetrieveError> {
    match top_k {
        1 => Ok(SemanticMorTopK::One),
        2 => Ok(SemanticMorTopK::Two),
        4 => Ok(SemanticMorTopK::Four),
        _ => Err(ContextRetrieveError::BudgetOverflow),
    }
}

fn bucket_key<const BITS: usize>(signature: &SemanticBitField<BITS>, prefix_bits: u32) -> u64 {
    let mask = if prefix_bits >= 64 {
        u64::MAX
    } else {
        (1_u64 << prefix_bits) - 1
    };
    signature.words()[0] & mask
}

fn base_name(stream_id: ContextStreamId, segment_id: u64) -> String {
    format!("stream-{:016x}-segment-{segment_id:016x}", stream_id.get())
}

fn read_bounded(path: &Path, maximum: u64) -> Result<Option<Vec<u8>>, ContextRetrieveError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() {
        return Ok(None);
    }
    if metadata.len() > maximum {
        return Err(ContextRetrieveError::BudgetOverflow);
    }
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
