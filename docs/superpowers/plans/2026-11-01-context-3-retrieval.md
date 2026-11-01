# CONTEXT-3 Hierarchical Retrieval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the frozen CONTEXT-3 design (`docs/SML-CONTEXT-3-RETRIEVAL.md`): a bounded SBG shortlist → stable MOR top-k → optional bounded associative lookup → SHA-256-verified raw reread pipeline in `crates/sml-store/src/context_retrieve.rs`, with the 12 mandatory tests in `crates/sml-store/tests/context_retrieve.rs` and an exhaustive-scan control.

**GitHub stack admission:** Merge the frozen design PR #766 before this plan PR #767; merge implementation PR #768 only after this plan is in `main`. For each PR, require all applicable CI checks green on the exact head SHA, without `--no-verify`. Local worktree merges do not count as GitHub merges. Neither this plan nor fixture-only CONTEXT-3 tests authorize LONGCTX-B1 runs or model-quality claims.

**Architecture:** `ContextRetriever<BITS>` opens a directory already produced by CONTEXT-1c `SemanticStreamingContextIngestor` (`.smlctx` raw, `.smlsbg` semantic store, `.smlprov` provenance). At open it indexes, per segment, the SBG signature (semantic `value` of the first provenance record) and the provenance content hash into deterministic `BTreeMap` buckets keyed by a low-bit prefix of the signature. `retrieve` takes one bucket (truncated to `max_shortlist`), ranks it by `(hamming_distance, segment_id)` with a stable sort truncated to the `SemanticMorTopK` fanout, then reloads provenance and the raw segment (`load_raw_segment`) and verifies SHA-256 before emitting any hit. `BoundedAssociativeMemory` (`ReplacementPolicy::RejectCollision`) provides the optional key → segment lookup. `exhaustive_semantic_scan` is the control (scores all N). MOR, Core, the ledger, the raw store and semantic ingest are reused, never rewritten.

**Tech Stack:** Rust 1.89, edition 2021, workspace lint `unsafe_code = "forbid"`; crates `sml-core` (`context_ledger`, `semantic`, `memory`) and `sml-store` (`raw_context`, `context_ingest`, `context_semantic_ingest`). No new dependencies. No NVIDIA/CUDA stack.

## Global Constraints

Copied verbatim from `docs/SML-CONTEXT-3-RETRIEVAL.md`:

> **DESIGN FROZEN, NOT IMPLEMENTED.** This document freezes the CONTEXT-3 tranche B design. It contains no measured numbers. No code ships with it, and no LONGCTX-B1 campaign is started or authorized by it.
>
> The primary arm `sml-ctx-sbg-mor-topk` of the LONGCTX-B1 preregistration ([`SML-LONGCTX-B1-PREREGISTRATION.md`](SML-LONGCTX-B1-PREREGISTRATION.md)) stays blocked until this design is implemented and merged with its tests. Landing this design alone unblocks nothing.
>
> ## Non-goals
>
> - No 1M-token context claim. CONTEXT-3 is **not** a 1M context window.
> - A mapped store is not a context window. Bytes on disk or ingested segments are not effective context.
> - No mandatory NVIDIA stack (no CUDA/GPU requirement).
> - No copy of HYB/Claude designs or code.
> - No B1 run.
> - CONTEXT-2 (recurrent working context) and CONTEXT-4 (retention/compaction) are out of scope.
>
> MOR and Core are not rewritten by this track.
>
> Invariant: `shortlisted >= decoded >= reread segments`.
>
> ## Scope of the sublinear proof
>
> The sublinear proof (test 10) is fixture-only. It does **not** satisfy LONGCTX-B1 H4 nor any wording gate, and supports no scale claim.
>
> 2. Separate later work: filler SHA, B-lex, B-attn, B1 harness — all before any B1 campaign.

Additional execution constraints for this plan:

- Out of scope: B1 filler SHA, B-lex, B-attn, B1 harness, any B1 campaign, CCOS wiring (CCOS Voie A is a separate track). HYB Claude ternary/hybrid is a competitor/baseline only, never architecture.
- No metric is reported from these tests. Test 10 asserts thresholds on a declared synthetic fixture only; never quote its exponent as a result.
- Never push with `--no-verify`; the pre-push hook (`core.hooksPath=.githooks`) must pass. A PR merges only when mandatory CI is green on the exact head SHA. Read the ecosystem roadmap before any PR/merge decision (AGENTS.md).

## Spec interpretations (frozen error enum and MOR primitive)

- **`top_k` ∈ {1, 2, 4}.** "MOR top-k" reuses `sml_core::semantic::SemanticMorTopK`, which only supports fanout 1, 2 or 4. Any other `top_k` is rejected by `validate()` with `BudgetOverflow` (the spec's error enum is frozen; no new variant).
- **Invalid policy/config → `BudgetOverflow`**: `max_shortlist == 0`, `top_k > max_shortlist`, unsupported `top_k`, `fail_on_hash_mismatch == false` (must be `true`), `bucket_prefix_bits == 0` or `> min(64, BITS)`, zero byte limits. `segment_count == 0` at open → `EmptyIndex`.
- **Missing or unparsable `.smlprov` or `.smlsbg` → `MissingProvenance`**; missing `.smlctx` → `MissingRaw`; truncated or corrupt raw / hash disagreement → `HashMismatch`; raw size over limits → `BudgetOverflow`; other I/O → `Io`.
- **Accounting** adds `raw_reread_segments` next to the spec's five fields so the invariant `shortlisted >= decoded >= raw_reread_segments` is checkable (`RetrieveAccounting::invariant_holds`).
- **`associative_slots`** from the spec is the capacity of the `BoundedAssociativeMemory` (`ContextRetrieverConfig::associative_capacity`) fixed at open; the per-query policy keeps `max_shortlist`, `top_k`, `fail_on_hash_mismatch`. An associative hit is prepended to the shortlist and still counts against `max_shortlist`.

## File Map

| File | Action | Responsibility |
|---|---|---|
| `crates/sml-store/src/context_retrieve.rs` | Create | Policy/config validation, candidate/query/hit/accounting types, `ContextRetrieveError`, `ContextRetriever<BITS>` (open/index, remember, retrieve, rank, verified reread), `exhaustive_semantic_scan` control. |
| `crates/sml-store/src/lib.rs` | Modify (1 line) | Add `pub mod context_retrieve;` right after `pub mod context_ingest;`. |
| `crates/sml-store/tests/context_retrieve.rs` | Create | Declared synthetic fixture (SplitMix64 signatures, 16-byte segments) + path test + 12 mandatory tests. |
| `docs/SML-CONTEXT-3-RETRIEVAL.md` | Modify | Status: implemented with mandatory tests, still no measured numbers, B1 still not authorized. |
| `docs/SML-CONTEXT-BOOTSTRAP.md` | Modify | CONTEXT-3 status line. |
| `docs/ROADMAP.md` | Modify | CONTEXT-3 sub-checkbox. |

Not touched: MOR, Core, `context_ledger`, `raw_context`, `context_ingest`, `context_semantic_ingest`, `Cargo.toml`/`Cargo.lock`, any B1 harness file.

## Mandatory test → task map

| Spec test | Test function | Task |
|---|---|---|
| (support) | `context3_retriever_paths_match_semantic_ingestor_paths` | 1 |
| 8 | `context3_t08_empty_index_or_zero_budget_fails_validation` | 1 (open/validate), 2 (extended to `retrieve`) |
| 1 | `context3_t01_shortlist_and_top_k_budgets_respected_with_stable_sort` | 2 |
| 3 | `context3_t03_hamming_near_miss_is_not_a_source_exact_hit` | 2 |
| 9 | `context3_t09_multi_query_independence` | 2 |
| 11 | `context3_t11_accounting_invariant_holds` | 2 |
| 12 | `context3_t12_determinism_across_two_runs` | 2 |
| 2 | `context3_t02_exact_needle_hit_only_after_sha256_verification` | 3 |
| 4 | `context3_t04_hash_mismatch_or_truncated_raw_segment_aborts` | 3 |
| 5 | `context3_t05_missing_raw_segment_or_provenance_returns_error` | 3 |
| 6 | `context3_t06_associative_hit_miss_and_full_capacity` | 4 |
| 7 | `context3_t07_exhaustive_scan_and_shortlist_agree_with_few_reads` | 5 |
| 10 | `context3_t10_sublinear_fit_on_declared_fixture_with_exhaustive_negative_control` | 5 |

All commands run from the repository root of a clean worktree branched for the implementation (e.g. `feat/context-3-retrieval`). Intermediate tasks may print `dead_code`/`unused` **warnings** in the test target (helpers used by later tasks); they are gone by Task 5 and Task 7 enforces `clippy -D warnings`.

---

## Task 1: Module skeleton, validation, index open, fixture

**Files:**
- Create: `crates/sml-store/tests/context_retrieve.rs`
- Create: `crates/sml-store/src/context_retrieve.rs`
- Modify: `crates/sml-store/src/lib.rs`

- [ ] **Step 1: Write the failing tests (fixture + path test + test 8 open/validate part)**

Create `crates/sml-store/tests/context_retrieve.rs` with exactly:

```rust
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
        ContextRetrieveError, ContextRetrievePolicy, ContextRetriever,
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
    for candidate in invalid {
        assert!(matches!(
            candidate.validate(),
            Err(ContextRetrieveError::BudgetOverflow)
        ));
    }
    assert!(policy(8, 4).validate().is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
CARGO_TARGET_DIR=target cargo test --locked -p sml-store --test context_retrieve
```

Expected: FAIL to compile with `` error[E0432]: unresolved import `sml_store::context_retrieve` ``.

- [ ] **Step 3: Write the minimal implementation**

Create `crates/sml-store/src/context_retrieve.rs` with exactly:

```rust
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
```

In `crates/sml-store/src/lib.rs`, add the line `pub mod context_retrieve;` immediately after the existing `pub mod context_ingest;` line.

- [ ] **Step 4: Run tests to verify they pass**

```bash
CARGO_TARGET_DIR=target cargo test --locked -p sml-store --test context_retrieve
```

Expected: PASS — `test result: ok. 2 passed; 0 failed` (test-target warnings for not-yet-used helpers are expected).

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/context_retrieve.rs crates/sml-store/tests/context_retrieve.rs crates/sml-store/src/lib.rs
git commit -m "feat(context-3): retriever skeleton, policy validation and index open"
```

---

## Task 2: Bounded shortlist, stable MOR top-k, raw reread (tests 1, 3, 9, 11, 12; test 8 completed)

**Files:**
- Modify: `crates/sml-store/src/context_retrieve.rs`
- Modify: `crates/sml-store/tests/context_retrieve.rs`

- [ ] **Step 1: Write the failing tests**

In `crates/sml-store/tests/context_retrieve.rs`, replace the whole `context3_t08_empty_index_or_zero_budget_fails_validation` function with:

```rust
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
```

Then append at the end of the file:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
CARGO_TARGET_DIR=target cargo test --locked -p sml-store --test context_retrieve
```

Expected: FAIL to compile with `` error[E0599]: no method named `retrieve` found for struct `ContextRetriever` ``.

- [ ] **Step 3: Write the minimal implementation**

In `crates/sml-store/src/context_retrieve.rs`, replace the three top `use` blocks with:

```rust
use std::{
    collections::BTreeMap,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use sml_core::{
    context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId},
    memory::{BoundedAssociativeMemory, ReplacementPolicy},
    semantic::{hamming_distance, SemanticBitField, SemanticMorTopK, SemanticStoreView},
};

use crate::{
    context_semantic_ingest::{parse_provenance_artifact, ContextProvenanceManifest},
    raw_context::{
        load_raw_segment, sha256_raw_bytes, RawContextStoreError, RawContextStoreLimits,
    },
};
```

Insert these methods inside `impl<const BITS: usize> ContextRetriever<BITS>`, directly after `provenance_path` and before `load_manifest` (this reread verifies against the index-time provenance hash only; Task 3 adds the provenance reload; an `associative_key` fails closed until Task 4):

```rust
    /// Bounded retrieval: shortlist -> MOR top-k -> verified raw reread.
    pub fn retrieve(
        &self,
        query: &RetrieveQuery<BITS>,
        policy: &ContextRetrievePolicy,
    ) -> Result<(Vec<RetrieveHit>, RetrieveAccounting), ContextRetrieveError> {
        let policy = policy.validate()?;
        if self.entries.is_empty() {
            return Err(ContextRetrieveError::EmptyIndex);
        }
        let mut accounting = RetrieveAccounting::default();
        let mut shortlist: Vec<usize> = Vec::with_capacity(policy.max_shortlist);
        if query.associative_key.is_some() {
            // Fail closed until Task 4 wires the bounded associative memory.
            return Err(ContextRetrieveError::AssociativeDenied);
        }
        let key = bucket_key(&query.bitfield, self.config.bucket_prefix_bits);
        if let Some(bucket) = self.buckets.get(&key) {
            for &index in bucket {
                if shortlist.len() == policy.max_shortlist {
                    break;
                }
                if !shortlist.contains(&index) {
                    shortlist.push(index);
                }
            }
        }
        accounting.shortlisted = shortlist.len() as u64;
        let candidates = self.rank(&shortlist, &query.bitfield, policy.top_k)?;
        self.reread(candidates, accounting)
    }

    fn rank(
        &self,
        shortlist: &[usize],
        query: &SemanticBitField<BITS>,
        top_k: usize,
    ) -> Result<Vec<ContextCandidate>, ContextRetrieveError> {
        let fanout = mor_top_k(top_k)?.as_usize();
        let mut candidates: Vec<ContextCandidate> = shortlist
            .iter()
            .map(|&index| {
                let entry = &self.entries[index];
                ContextCandidate {
                    segment: entry.segment,
                    hamming_distance: hamming_distance(&entry.signature, query),
                    content_hash: entry.content_hash,
                }
            })
            .collect();
        candidates
            .sort_by_key(|candidate| (candidate.hamming_distance, candidate.segment.segment_id));
        candidates.truncate(fanout);
        Ok(candidates)
    }

    fn reread(
        &self,
        candidates: Vec<ContextCandidate>,
        mut accounting: RetrieveAccounting,
    ) -> Result<(Vec<RetrieveHit>, RetrieveAccounting), ContextRetrieveError> {
        accounting.decoded = candidates.len() as u64;
        let mut hits = Vec::with_capacity(candidates.len());
        for (rank, candidate) in candidates.into_iter().enumerate() {
            let hit = self.reread_segment(rank, candidate)?;
            accounting.raw_reread_segments += 1;
            accounting.raw_reread_bytes += hit.byte_len;
            hits.push(hit);
        }
        Ok((hits, accounting))
    }

    fn reread_segment(
        &self,
        rank: usize,
        candidate: ContextCandidate,
    ) -> Result<RetrieveHit, ContextRetrieveError> {
        let raw = load_raw_segment(
            self.raw_path(candidate.segment.segment_id),
            self.config.raw_limits,
        )
        .map_err(map_raw_error)?;
        if raw.reference.identity != candidate.segment {
            return Err(ContextRetrieveError::MissingRaw);
        }
        let actual = sha256_raw_bytes(&raw.raw_bytes).map_err(map_raw_error)?;
        if actual != candidate.content_hash || raw.reference.content_hash != candidate.content_hash
        {
            return Err(ContextRetrieveError::HashMismatch);
        }
        Ok(RetrieveHit {
            segment: candidate.segment,
            rank,
            hamming_distance: candidate.hamming_distance,
            source_exact: candidate.hamming_distance == 0,
            byte_start: raw.reference.byte_start,
            byte_len: raw.reference.byte_len,
            content_hash: actual,
            raw_bytes: raw.raw_bytes,
        })
    }
```

Append at the end of the file:

```rust
fn map_raw_error(error: RawContextStoreError) -> ContextRetrieveError {
    match error {
        RawContextStoreError::Io(error) if error.kind() == io::ErrorKind::NotFound => {
            ContextRetrieveError::MissingRaw
        }
        RawContextStoreError::Io(error) => ContextRetrieveError::Io(error),
        RawContextStoreError::FileLimit { .. } | RawContextStoreError::RawLimit { .. } => {
            ContextRetrieveError::BudgetOverflow
        }
        _ => ContextRetrieveError::HashMismatch,
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
CARGO_TARGET_DIR=target cargo test --locked -p sml-store --test context_retrieve
```

Expected: PASS — `test result: ok. 7 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/context_retrieve.rs crates/sml-store/tests/context_retrieve.rs
git commit -m "feat(context-3): bounded shortlist, stable MOR top-k and raw reread"
```

---

## Task 3: Provenance reload + SHA-256 verification at reread (tests 2, 4, 5)

**Files:**
- Modify: `crates/sml-store/src/context_retrieve.rs`
- Modify: `crates/sml-store/tests/context_retrieve.rs`

- [ ] **Step 1: Write the failing tests**

Append at the end of `crates/sml-store/tests/context_retrieve.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
CARGO_TARGET_DIR=target cargo test --locked -p sml-store --test context_retrieve
```

Expected: FAIL at runtime — `test result: FAILED. 7 passed; 3 failed` with `context3_t02_…`, `context3_t04_…` and `context3_t05_…` panicking (a tampered or deleted provenance after open is not yet detected).

- [ ] **Step 3: Write the minimal implementation**

In `crates/sml-store/src/context_retrieve.rs`, replace the whole `reread_segment` method with:

```rust
    fn reread_segment(
        &self,
        rank: usize,
        candidate: ContextCandidate,
    ) -> Result<RetrieveHit, ContextRetrieveError> {
        let manifest = self.load_manifest(candidate.segment.segment_id)?;
        if manifest.content_hash != candidate.content_hash {
            return Err(ContextRetrieveError::HashMismatch);
        }
        let raw = load_raw_segment(
            self.raw_path(candidate.segment.segment_id),
            self.config.raw_limits,
        )
        .map_err(map_raw_error)?;
        if raw.reference.identity != candidate.segment {
            return Err(ContextRetrieveError::MissingRaw);
        }
        let actual = sha256_raw_bytes(&raw.raw_bytes).map_err(map_raw_error)?;
        if actual != candidate.content_hash || raw.reference.content_hash != candidate.content_hash
        {
            return Err(ContextRetrieveError::HashMismatch);
        }
        Ok(RetrieveHit {
            segment: candidate.segment,
            rank,
            hamming_distance: candidate.hamming_distance,
            source_exact: candidate.hamming_distance == 0,
            byte_start: raw.reference.byte_start,
            byte_len: raw.reference.byte_len,
            content_hash: actual,
            raw_bytes: raw.raw_bytes,
        })
    }
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
CARGO_TARGET_DIR=target cargo test --locked -p sml-store --test context_retrieve
```

Expected: PASS — `test result: ok. 10 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/context_retrieve.rs crates/sml-store/tests/context_retrieve.rs
git commit -m "feat(context-3): verify provenance and SHA-256 before emitting hits"
```

---

## Task 4: Bounded associative lookup (test 6)

**Files:**
- Modify: `crates/sml-store/src/context_retrieve.rs`
- Modify: `crates/sml-store/tests/context_retrieve.rs`

- [ ] **Step 1: Write the failing test**

Append at the end of `crates/sml-store/tests/context_retrieve.rs`:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

```bash
CARGO_TARGET_DIR=target cargo test --locked -p sml-store --test context_retrieve
```

Expected: FAIL to compile with `` error[E0599]: no method named `remember` found for struct `ContextRetriever` ``.

- [ ] **Step 3: Write the minimal implementation**

In `crates/sml-store/src/context_retrieve.rs`, replace the three top `use` blocks with:

```rust
use std::{
    collections::BTreeMap,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use sml_core::{
    context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId},
    memory::{BoundedAssociativeMemory, ReadOutcome, ReplacementPolicy, WriteOutcome},
    semantic::{hamming_distance, SemanticBitField, SemanticMorTopK, SemanticStoreView},
    BooleanWorkspace, ExecutionCounters,
};

use crate::{
    context_semantic_ingest::{parse_provenance_artifact, ContextProvenanceManifest},
    raw_context::{
        load_raw_segment, sha256_raw_bytes, RawContextStoreError, RawContextStoreLimits,
    },
};
```

Insert `remember` directly before `retrieve`:

```rust
    /// Binds `key` to an indexed segment in the bounded associative memory.
    /// Collisions are rejected, never silently replaced.
    pub fn remember(
        &mut self,
        key: u64,
        segment: ContextSegmentId,
    ) -> Result<(), ContextRetrieveError> {
        if segment.stream_id != self.stream_id
            || segment.segment_id == 0
            || segment.segment_id > self.entries.len() as u64
        {
            return Err(ContextRetrieveError::AssociativeDenied);
        }
        let memory = self
            .memory
            .as_mut()
            .ok_or(ContextRetrieveError::AssociativeDenied)?;
        let bits: Vec<bool> = (0..ASSOCIATIVE_VALUE_BITS)
            .map(|bit| (segment.segment_id >> bit) & 1 == 1)
            .collect();
        let mut counters = ExecutionCounters::default();
        match memory
            .write(key, BooleanWorkspace::from_bits(&bits), &mut counters)
            .map_err(|_| ContextRetrieveError::AssociativeDenied)?
        {
            WriteOutcome::Inserted { .. }
            | WriteOutcome::Updated { .. }
            | WriteOutcome::Replaced { .. } => Ok(()),
            WriteOutcome::CollisionRejected { .. } => Err(ContextRetrieveError::AssociativeDenied),
        }
    }
```

Replace the whole `retrieve` method (removes the fail-closed associative branch) with:

```rust
    /// Bounded retrieval: shortlist -> MOR top-k -> verified raw reread.
    pub fn retrieve(
        &self,
        query: &RetrieveQuery<BITS>,
        policy: &ContextRetrievePolicy,
    ) -> Result<(Vec<RetrieveHit>, RetrieveAccounting), ContextRetrieveError> {
        let policy = policy.validate()?;
        if self.entries.is_empty() {
            return Err(ContextRetrieveError::EmptyIndex);
        }
        let mut accounting = RetrieveAccounting::default();
        let mut shortlist: Vec<usize> = Vec::with_capacity(policy.max_shortlist);
        if let Some(key) = query.associative_key {
            if let Some(index) = self.associative_lookup(key, &policy, &mut accounting)? {
                shortlist.push(index);
            }
        }
        let key = bucket_key(&query.bitfield, self.config.bucket_prefix_bits);
        if let Some(bucket) = self.buckets.get(&key) {
            for &index in bucket {
                if shortlist.len() == policy.max_shortlist {
                    break;
                }
                if !shortlist.contains(&index) {
                    shortlist.push(index);
                }
            }
        }
        accounting.shortlisted = shortlist.len() as u64;
        let candidates = self.rank(&shortlist, &query.bitfield, policy.top_k)?;
        self.reread(candidates, accounting)
    }
```

Insert `associative_lookup` directly after `retrieve`:

```rust
    fn associative_lookup(
        &self,
        key: u64,
        policy: &ContextRetrievePolicy,
        accounting: &mut RetrieveAccounting,
    ) -> Result<Option<usize>, ContextRetrieveError> {
        let memory = self
            .memory
            .as_ref()
            .ok_or(ContextRetrieveError::AssociativeDenied)?;
        if policy.associative_slots == 0 || policy.associative_slots > memory.capacity() {
            return Err(ContextRetrieveError::AssociativeDenied);
        }
        let mut counters = ExecutionCounters::default();
        match memory.read(key, &mut counters) {
            ReadOutcome::Hit { value, .. } => {
                let mut segment_id = 0_u64;
                for bit in 0..ASSOCIATIVE_VALUE_BITS {
                    if value.get(bit) == Some(true) {
                        segment_id |= 1_u64 << bit;
                    }
                }
                if segment_id == 0 || segment_id > self.entries.len() as u64 {
                    return Err(ContextRetrieveError::AssociativeDenied);
                }
                accounting.associative_hits += 1;
                Ok(Some((segment_id - 1) as usize))
            }
            ReadOutcome::EmptyMiss { .. } | ReadOutcome::KeyMismatch { .. } => {
                accounting.associative_misses += 1;
                Ok(None)
            }
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
CARGO_TARGET_DIR=target cargo test --locked -p sml-store --test context_retrieve
```

Expected: PASS — `test result: ok. 11 passed; 0 failed`.

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/context_retrieve.rs crates/sml-store/tests/context_retrieve.rs
git commit -m "feat(context-3): bounded associative lookup with collision rejection"
```

---

## Task 5: Exhaustive-scan control and fixture-only sublinear check (tests 7, 10)

**Files:**
- Modify: `crates/sml-store/src/context_retrieve.rs`
- Modify: `crates/sml-store/tests/context_retrieve.rs`

- [ ] **Step 1: Write the failing tests**

In `crates/sml-store/tests/context_retrieve.rs`, replace the `context_retrieve::{ ... }` import group with:

```rust
    context_retrieve::{
        exhaustive_semantic_scan, ContextRetrieveError, ContextRetrievePolicy, ContextRetriever,
        ContextRetrieverConfig, RetrieveQuery,
    },
```

Then append at the end of the file:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
CARGO_TARGET_DIR=target cargo test --locked -p sml-store --test context_retrieve
```

Expected: FAIL to compile with `` error[E0432]: unresolved import `sml_store::context_retrieve::exhaustive_semantic_scan` ``.

- [ ] **Step 3: Write the minimal implementation**

In `crates/sml-store/src/context_retrieve.rs`, insert directly after the closing `}` of `impl<const BITS: usize> ContextRetriever<BITS>` and before `fn mor_top_k`:

```rust
/// Control only: exhaustive semantic scan over all indexed segments.
///
/// Scores every segment (`shortlisted == len`), then applies the same
/// stable MOR top-k and verified raw reread as [`ContextRetriever::retrieve`].
/// `policy.max_shortlist` is intentionally ignored by this control.
pub fn exhaustive_semantic_scan<const BITS: usize>(
    retriever: &ContextRetriever<BITS>,
    query: &SemanticBitField<BITS>,
    policy: &ContextRetrievePolicy,
) -> Result<(Vec<RetrieveHit>, RetrieveAccounting), ContextRetrieveError> {
    let policy = policy.validate()?;
    if retriever.entries.is_empty() {
        return Err(ContextRetrieveError::EmptyIndex);
    }
    let all: Vec<usize> = (0..retriever.entries.len()).collect();
    let accounting = RetrieveAccounting {
        shortlisted: all.len() as u64,
        ..RetrieveAccounting::default()
    };
    let candidates = retriever.rank(&all, query, policy.top_k)?;
    retriever.reread(candidates, accounting)
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
CARGO_TARGET_DIR=target cargo test --locked -p sml-store --test context_retrieve
```

Expected: PASS — `test result: ok. 13 passed; 0 failed`, with no warnings in the test target. Do not record or quote the fitted exponents; test 10 is a fixture-only threshold assertion (α_shortlist < 0.75, |α_scan − 1| < 0.05) and does **not** satisfy LONGCTX-B1 H4.

- [ ] **Step 5: Commit**

```bash
git add crates/sml-store/src/context_retrieve.rs crates/sml-store/tests/context_retrieve.rs
git commit -m "feat(context-3): exhaustive-scan control and fixture-only sublinear check"
```

---

## Task 6: Status docs (no metrics)

**Files:**
- Modify: `docs/SML-CONTEXT-3-RETRIEVAL.md`
- Modify: `docs/SML-CONTEXT-BOOTSTRAP.md`
- Modify: `docs/ROADMAP.md`

- [ ] **Step 1: Update the spec status**

In `docs/SML-CONTEXT-3-RETRIEVAL.md`, replace the paragraph under `## Status`:

```markdown
**DESIGN FROZEN, NOT IMPLEMENTED.** This document freezes the CONTEXT-3 tranche B design. It contains no measured numbers. No code ships with it, and no LONGCTX-B1 campaign is started or authorized by it.
```

with:

```markdown
**IMPLEMENTED (design frozen).** `crates/sml-store/src/context_retrieve.rs` implements this design and `crates/sml-store/tests/context_retrieve.rs` carries the 12 mandatory tests on declared synthetic fixtures. This document still contains no measured numbers, and no LONGCTX-B1 campaign is started or authorized by it. Filler SHA, B-lex, B-attn and the B1 harness remain separate prerequisites.
```

- [ ] **Step 2: Update the bootstrap status line**

In `docs/SML-CONTEXT-BOOTSTRAP.md`, replace:

```markdown
Design frozen, not implemented: [`SML-CONTEXT-3-RETRIEVAL.md`](SML-CONTEXT-3-RETRIEVAL.md).
```

with:

```markdown
Implemented with fixture-only mandatory tests (no B1 measurement): [`SML-CONTEXT-3-RETRIEVAL.md`](SML-CONTEXT-3-RETRIEVAL.md), `crates/sml-store/src/context_retrieve.rs`.
```

- [ ] **Step 3: Update the roadmap**

In `docs/ROADMAP.md`, replace:

```markdown
  - [ ] design frozen, not implemented: [`SML-CONTEXT-3-RETRIEVAL.md`](SML-CONTEXT-3-RETRIEVAL.md);
```

with:

```markdown
  - [x] design frozen: [`SML-CONTEXT-3-RETRIEVAL.md`](SML-CONTEXT-3-RETRIEVAL.md);
  - [ ] `context_retrieve` + 12 mandatory fixture-only tests (implementation PR; check after merge with green required CI);
```

Leave the parent `CONTEXT-3` checkbox unchecked (no B1 evidence exists).

- [ ] **Step 4: Verify no metric or scale wording slipped in**

```bash
git diff -- docs/ | grep -nEi "1M|million|α ?=|alpha ?=|[0-9]+(\.[0-9]+)? ?(x|×) faster|B1 H4 (met|satisfied)" || echo "no metric wording"
```

Expected: `no metric wording`.

- [ ] **Step 5: Commit**

```bash
git add docs/SML-CONTEXT-3-RETRIEVAL.md docs/SML-CONTEXT-BOOTSTRAP.md docs/ROADMAP.md
git commit -m "docs(context-3): mark retrieval implemented, no metrics, B1 still blocked"
```

---

## Task 7: Local gates and implementation PR (no merge)

**Files:** none modified.

- [ ] **Step 1: Format check**

```bash
cargo fmt --all -- --check
```

Expected: exit 0, no output.

- [ ] **Step 2: Clippy with warnings denied**

```bash
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Expected: `Finished` with no warnings or errors.

- [ ] **Step 3: Full workspace tests**

```bash
cargo test --locked --workspace
```

Expected: every `test result:` line is `ok`; `context_retrieve` shows `13 passed; 0 failed`.

- [ ] **Step 4: Confirm hooks and push without bypass**

```bash
git config core.hooksPath   # expected: .githooks
git push -u origin HEAD          # never --no-verify; the pre-push hook re-runs fmt, clippy and tests
```

Expected: hook passes and the branch is pushed.

- [ ] **Step 5: Open the PR, do not merge**

Read the ecosystem roadmap first (AGENTS.md), then:

```bash
gh pr create --title "feat(context-3): hierarchical SBG/MOR top-k retrieval with verified raw reread" --body "Implements docs/SML-CONTEXT-3-RETRIEVAL.md. 12 mandatory tests on declared synthetic fixtures; test 10 is fixture-only and does NOT satisfy LONGCTX-B1 H4. No metrics, no B1 campaign, not a 1M context window, no NVIDIA stack, no CCOS wiring."
```

Expected: PR URL printed. Merge only after every mandatory CI check is green on the exact head SHA (`gh pr checks <PR>` and `gh pr view <PR> --json headRefOid`) and the base design PR (#766) has merged.
