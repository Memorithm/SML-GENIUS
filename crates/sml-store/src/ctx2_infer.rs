//! CTX2-INFER step 2: model-free plan / audit / score substrate for the
//! preregistration `docs/research/CTX2_INFER_PREREG.md` (PR #909).
//!
//! This module loads no model weights, calls no runtime, embeds no tokenizer
//! and produces no measurement. Every artifact carries `NOT_MEASURED`; mock
//! anchors are `NOT_QUALIFIED`; the RemoteOps loop is `NOT_CONNECTED`.
//!
//! Contents:
//! - [`PreregFreeze`]: the prereg pinned by path, exact file SHA-256, content
//!   commit and PR head. Test seeds can only be derived from an attested merge
//!   SHA ([`PreregFreeze::test_seeds`] fails closed while unmerged).
//! - [`generator`]: CTX2-FACTS-v1 sessions (48 turns, 6 queries, one-based
//!   distance, disjoint dev/test vocabularies), pinned by SHA-256.
//! - [`plan`]: arms A0, A1, A2, A3, A3r and R, CONTEXT-2 prompt assembly through
//!   the raw store, prompt/plan hashes and the enforced call ledger.
//! - [`restart`]: the A3r seal / open / anchored-restore path and its
//!   fail-closed tamper, rollback and missing-anchor controls.
//! - [`audit`]: exact token-ID budget audit (section 4.1 of the prereg).
//! - [`compute`]: the bounded compute envelope (section 8.1), derived.
//! - [`score`]: EM, Wilson, one-sided exact McNemar, Holm, clustered bootstrap.
#![forbid(unsafe_code)]

pub mod audit;
pub mod compute;
pub mod generator;
pub mod plan;
pub mod restart;
pub mod score;

use core::fmt;

use sml_core::elastic_ngram_paged_sha::sha256;

/// Measurement status carried by every CTX2-INFER artifact of this step.
pub const STATUS: &str = "NOT_MEASURED";
/// Anchor status: anchors are mocks only.
pub const ANCHOR_STATUS: &str = "NOT_QUALIFIED";
/// RemoteOps loop status.
pub const LOOP_STATUS: &str = "NOT_CONNECTED";

/// Repository path of the frozen preregistration.
pub const PREREG_PATH: &str = "docs/research/CTX2_INFER_PREREG.md";
/// SHA-256 of the exact prereg bytes (r1).
pub const PREREG_SHA256_HEX: &str =
    "e26dfe16f7222b297d47cd02d74442868de8e7ea58388e4f45e87d8a7d51bea1";
/// Last commit that changed the prereg bytes (r1).
pub const PREREG_CONTENT_COMMIT: &str = "ecec94b4a7bc12ca1b83c454475d157b025b1f37";
/// Pull request carrying the prereg.
pub const PREREG_PR: u32 = 909;
/// Exact PR head at freeze time (contains the r1 bytes unchanged).
pub const PREREG_PR_HEAD: &str = "eff41fab52196809139bd8b7646f0712a1de6450";

/// Domain prefix for test seeds (prereg section 7).
pub const TEST_SEED_DOMAIN: &[u8] = b"CTX2-INFER-test-v1";
/// Domain prefix for dev seeds (plan interpretation; see the plan/score doc).
pub const DEV_SEED_DOMAIN: &[u8] = b"CTX2-INFER-dev-v1";
/// Number of test seeds (prereg section 7).
pub const TEST_SEED_COUNT: usize = 5;
/// Number of dev seeds (prereg section 7).
pub const DEV_SEED_COUNT: usize = 2;

/// Errors of the CTX2-INFER plan/score substrate. All are fail-closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ctx2InferError {
    /// Prereg bytes do not hash to [`PREREG_SHA256_HEX`].
    PreregHashMismatch { actual_hex: String },
    /// Test seeds requested before a merge SHA was attested.
    PreregNotMerged,
    /// A commit SHA is not 40 lowercase hex characters.
    InvalidCommitSha,
    /// Invalid generator parameter.
    Generator(&'static str),
    /// CONTEXT-2 substrate refused an operation.
    Context(String),
    /// Raw store missing segment.
    MissingSegment { stream_id: u64, segment_id: u64 },
    /// Raw store refused an artifact or the content hash did not match.
    RawStore {
        stream_id: u64,
        segment_id: u64,
        reason: String,
    },
    /// A restart checkpoint was refused; the session stops with no prompt.
    RestartRefused { turn: u32, reason: String },
    /// Plan digest differs from the pinned manifest.
    PlanDigestMismatch,
    /// Plan call count differs from the compute envelope.
    PlanCallCount { expected: usize, actual: usize },
    /// Call ledger refusal (unplanned, duplicate, retry exhausted, incomplete).
    Ledger(String),
}

impl fmt::Display for Ctx2InferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Ctx2InferError {}

/// Whether the prereg has an attested merge SHA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreregMergeState {
    /// PR #909 is open: test seeds are not derivable.
    Unmerged,
    /// Caller-attested merge SHA (40 lowercase hex). Not verified on-chain here.
    Merged { merge_sha: String },
}

/// Immutable pin of the preregistration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreregFreeze {
    pub path: &'static str,
    pub sha256_hex: &'static str,
    pub content_commit: &'static str,
    pub pr: u32,
    pub pr_head: &'static str,
    pub merge: PreregMergeState,
}

/// The frozen prereg as of this step (unmerged).
#[must_use]
pub fn frozen_prereg() -> PreregFreeze {
    PreregFreeze {
        path: PREREG_PATH,
        sha256_hex: PREREG_SHA256_HEX,
        content_commit: PREREG_CONTENT_COMMIT,
        pr: PREREG_PR,
        pr_head: PREREG_PR_HEAD,
        merge: PreregMergeState::Unmerged,
    }
}

impl PreregFreeze {
    /// Verify exact prereg bytes against the pinned SHA-256 (fail closed).
    pub fn verify_bytes(&self, bytes: &[u8]) -> Result<(), Ctx2InferError> {
        let actual_hex = hex(&sha256(bytes));
        if actual_hex == self.sha256_hex {
            Ok(())
        } else {
            Err(Ctx2InferError::PreregHashMismatch { actual_hex })
        }
    }

    /// Record a merge SHA after the prereg is merged. Validates format only.
    pub fn attest_merge(mut self, merge_sha: &str) -> Result<Self, Ctx2InferError> {
        validate_commit_sha(merge_sha)?;
        self.merge = PreregMergeState::Merged {
            merge_sha: merge_sha.to_owned(),
        };
        Ok(self)
    }

    /// Test seeds `i ∈ {0..4}` (prereg section 7). Fails closed while unmerged.
    pub fn test_seeds(&self) -> Result<[u64; TEST_SEED_COUNT], Ctx2InferError> {
        match &self.merge {
            PreregMergeState::Unmerged => Err(Ctx2InferError::PreregNotMerged),
            PreregMergeState::Merged { merge_sha } => {
                let mut out = [0_u64; TEST_SEED_COUNT];
                for (i, slot) in out.iter_mut().enumerate() {
                    *slot = derive_test_seed(merge_sha, i as u8)?;
                }
                Ok(out)
            }
        }
    }
}

/// `first 8 bytes LE of SHA-256(TEST_SEED_DOMAIN || merge_sha_ascii_hex || u8(i))`.
///
/// The merge SHA enters as its 40 lowercase ASCII hex characters.
pub fn derive_test_seed(merge_sha_hex: &str, index: u8) -> Result<u64, Ctx2InferError> {
    validate_commit_sha(merge_sha_hex)?;
    let mut input = Vec::with_capacity(TEST_SEED_DOMAIN.len() + 41);
    input.extend_from_slice(TEST_SEED_DOMAIN);
    input.extend_from_slice(merge_sha_hex.as_bytes());
    input.push(index);
    Ok(first_u64_le(&sha256(&input)))
}

/// Dev seeds: `first 8 bytes LE of SHA-256(DEV_SEED_DOMAIN || u8(i))`, `i ∈ {0,1}`.
#[must_use]
pub fn dev_seeds() -> [u64; DEV_SEED_COUNT] {
    let mut out = [0_u64; DEV_SEED_COUNT];
    for (i, slot) in out.iter_mut().enumerate() {
        let mut input = DEV_SEED_DOMAIN.to_vec();
        input.push(i as u8);
        *slot = first_u64_le(&sha256(&input));
    }
    out
}

fn validate_commit_sha(sha: &str) -> Result<(), Ctx2InferError> {
    if sha.len() == 40 && sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        Ok(())
    } else {
        Err(Ctx2InferError::InvalidCommitSha)
    }
}

pub(crate) fn first_u64_le(digest: &[u8; 32]) -> u64 {
    let mut b = [0_u8; 8];
    b.copy_from_slice(&digest[..8]);
    u64::from_le_bytes(b)
}

/// Lowercase hex.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from(DIGITS[usize::from(b >> 4)]));
        s.push(char::from(DIGITS[usize::from(b & 0x0f)]));
    }
    s
}

/// SplitMix64: deterministic, documented generator PRNG (not cryptographic).
#[derive(Debug, Clone)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` by rejection (no modulo bias). `n` must be non-zero.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below(0)");
        let zone = u64::MAX - (u64::MAX % n);
        loop {
            let x = self.next_u64();
            if x < zone {
                return x % n;
            }
        }
    }
}
