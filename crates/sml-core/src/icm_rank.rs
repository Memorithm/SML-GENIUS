//! GROK-ICM-5 **experimental** document ranking (feature `icm-rank`, default off).
//!
//! **Not BEIR-validated.** This module defines the ranking surface consumed by
//! the CCOS-Enterprise `SmlRetrievalAdapter` (arm `sml-grok-icm-concurrent`,
//! CCOS-Enterprise #241) and one naive, deterministic, CPU-only backend. No
//! retrieval-quality claim is made; see
//! `docs/SML-GROK-ICM-5-DOCUMENT-RANKING-PREREGISTRATION.md` for the frozen
//! campaign that will decide CONFIRMED / NEGATIVE against BM25 and
//! `docmem_fused_minilm`.
//!
//! # Backend `sigpage-v0`
//! Each document becomes a 4096-bit binary signature (64 `u64` words): every
//! distinct token (lower-cased ASCII alphanumeric run, length >= 2; other
//! bytes are separators) sets [`BITS_PER_TOKEN`] bits chosen by
//! SplitMix64(FNV-1a-64(token) ^ seed). The signature page is stored in an
//! ICM [`GuardedPage`] (SECDED + CRC-32) and scrubbed before every scoring
//! pass; an uncorrectable page fails the whole query closed
//! ([`RankError::CorruptPage`]) instead of being silently skipped.
//!
//! Score = Ochiai (binary cosine) coefficient
//! `popcount(q & d) / sqrt(popcount(q) * popcount(d))`, in `[0, 1]`, and `0`
//! when either signature is empty. Ordering is `score desc, id asc`, then the
//! list is cut at `k` — the same order `GovernedSmlRetrieval::recall` imposes.
//! ICM guards the *integrity* of the index; it does not rank.

use crate::icm_guard::{GuardedPage, ScrubOutcome};
use std::collections::BTreeSet;
use std::fmt;

/// Arm label shared with CCOS-Enterprise `SML_CONCURRENT_ARM_ID`.
pub const ARM_ID: &str = "sml-grok-icm-concurrent";
/// Words per signature page.
pub const SIGNATURE_WORDS: usize = 64;
/// Bits set per distinct token.
pub const BITS_PER_TOKEN: usize = 2;

/// Evidence status attached to every ranking result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationStatus {
    /// No preregistered BEIR campaign has been run for this backend.
    NotBeirValidated,
}

/// One ranked document.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedCandidate {
    /// Caller-supplied document id.
    pub id: String,
    /// Finite score in `[0, 1]`.
    pub score: f32,
}

/// Ranking result, always labelled with arm, backend and evidence status.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedList {
    /// Always [`ARM_ID`].
    pub arm_id: &'static str,
    /// Backend identifier.
    pub backend_id: &'static str,
    /// Always [`ValidationStatus::NotBeirValidated`] until a campaign lands.
    pub status: ValidationStatus,
    /// Candidates sorted `score desc, id asc`, at most `k`.
    pub candidates: Vec<RankedCandidate>,
}

/// Ranking errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RankError {
    /// Query is empty or whitespace.
    EmptyQuery,
    /// Backend cannot rank (honest mirror of CCOS `UnavailableSmlRetrieval`).
    Unavailable(&'static str),
    /// A document id was inserted twice.
    DuplicateDocument(String),
    /// A guarded signature page failed its integrity check.
    CorruptPage(String),
}

impl fmt::Display for RankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyQuery => write!(f, "empty query"),
            Self::Unavailable(why) => write!(f, "ranker unavailable: {why}"),
            Self::DuplicateDocument(id) => write!(f, "duplicate document {id}"),
            Self::CorruptPage(id) => write!(f, "uncorrectable signature page for {id}"),
        }
    }
}

impl std::error::Error for RankError {}

/// Text → ranked candidates. `k == 0` returns an empty list.
pub trait DocumentRanker {
    /// Backend identifier.
    fn backend_id(&self) -> &'static str;
    /// Rank indexed documents for `query`.
    fn rank(&mut self, query: &str, k: usize) -> Result<RankedList, RankError>;
}

/// Always-unavailable ranker.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableRanker;

impl DocumentRanker for UnavailableRanker {
    fn backend_id(&self) -> &'static str {
        "sml-unavailable"
    }
    fn rank(&mut self, _query: &str, _k: usize) -> Result<RankedList, RankError> {
        Err(RankError::Unavailable("no validated GROK-ICM ranker"))
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Distinct tokens of `text` (see module docs).
#[must_use]
pub fn tokens(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Binary signature of `text` under `seed`.
#[must_use]
pub fn signature(text: &str, seed: u64) -> Vec<u64> {
    let mut words = vec![0u64; SIGNATURE_WORDS];
    for t in tokens(text) {
        let mut s = fnv1a64(t.as_bytes()) ^ seed;
        for _ in 0..BITS_PER_TOKEN {
            let bit = (splitmix64(&mut s) % (SIGNATURE_WORDS as u64 * 64)) as usize;
            words[bit / 64] |= 1 << (bit % 64);
        }
    }
    words
}

fn popcount(w: &[u64]) -> u32 {
    w.iter().map(|x| x.count_ones()).sum()
}

/// Ochiai coefficient between two signatures; finite, in `[0, 1]`.
#[must_use]
pub fn ochiai(q: &[u64], d: &[u64]) -> f32 {
    let (pq, pd) = (popcount(q), popcount(d));
    if pq == 0 || pd == 0 {
        return 0.0;
    }
    let inter: u32 = q.iter().zip(d).map(|(a, b)| (a & b).count_ones()).sum();
    (f64::from(inter) / (f64::from(pq) * f64::from(pd)).sqrt()) as f32
}

#[derive(Debug, Clone)]
struct Entry {
    id: String,
    page: GuardedPage,
}

/// Experimental `sigpage-v0` ranker over ICM-guarded signature pages.
#[derive(Debug, Clone)]
pub struct SignaturePageRanker {
    seed: u64,
    docs: Vec<Entry>,
}

impl SignaturePageRanker {
    /// Backend id.
    pub const BACKEND_ID: &'static str = "sml-icm-rank-sigpage-v0-experimental";

    /// Empty index with hashing `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            docs: Vec::new(),
        }
    }

    /// Index one document; duplicate ids are rejected.
    pub fn insert(&mut self, id: &str, text: &str) -> Result<(), RankError> {
        if self.docs.iter().any(|e| e.id == id) {
            return Err(RankError::DuplicateDocument(id.to_owned()));
        }
        self.docs.push(Entry {
            id: id.to_owned(),
            page: GuardedPage::new(signature(text, self.seed)),
        });
        Ok(())
    }

    /// Number of indexed documents.
    #[must_use]
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// True when no document is indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Raw page access for fault-injection tests.
    #[doc(hidden)]
    pub fn page_words_mut_unguarded(&mut self, idx: usize) -> &mut [u64] {
        self.docs[idx].page.words_mut_unguarded()
    }
}

impl DocumentRanker for SignaturePageRanker {
    fn backend_id(&self) -> &'static str {
        Self::BACKEND_ID
    }

    fn rank(&mut self, query: &str, k: usize) -> Result<RankedList, RankError> {
        if query.trim().is_empty() {
            return Err(RankError::EmptyQuery);
        }
        let q = signature(query, self.seed);
        let mut candidates = Vec::with_capacity(self.docs.len());
        for e in &mut self.docs {
            if e.page.scrub() == ScrubOutcome::Uncorrectable {
                return Err(RankError::CorruptPage(e.id.clone()));
            }
            candidates.push(RankedCandidate {
                id: e.id.clone(),
                score: ochiai(&q, e.page.words()),
            });
        }
        candidates.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
        candidates.truncate(k);
        Ok(RankedList {
            arm_id: ARM_ID,
            backend_id: Self::BACKEND_ID,
            status: ValidationStatus::NotBeirValidated,
            candidates,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> SignaturePageRanker {
        let mut r = SignaturePageRanker::new(0);
        r.insert(
            "d1",
            "Vitamin D deficiency increases fracture risk in elderly patients",
        )
        .unwrap();
        r.insert(
            "d2",
            "Graph neural networks for molecule property prediction",
        )
        .unwrap();
        r.insert("d3", "Stock market volatility and interest rates")
            .unwrap();
        r.insert("d4", "vitamin d supplementation and bone fracture outcomes")
            .unwrap();
        r
    }

    #[test]
    fn oracle_exact_scores_on_hand_signatures() {
        let q = [0b1111u64];
        assert_eq!(ochiai(&q, &[0b1111]), 1.0);
        assert_eq!(ochiai(&q, &[0b0011]), (2.0 / 8f64.sqrt()) as f32);
        assert_eq!(ochiai(&q, &[0]), 0.0);
        assert_eq!(ochiai(&[0], &q), 0.0);
    }

    #[test]
    fn tokenizer_contract() {
        let t: Vec<_> = tokens("A b-CD, cd;  e3x!").into_iter().collect();
        assert_eq!(t, vec!["cd".to_owned(), "e3x".to_owned()]);
    }

    #[test]
    fn identical_text_scores_one_and_ranks_first() {
        let mut r = fixture();
        let out = r
            .rank("Graph neural networks for molecule property prediction", 4)
            .unwrap();
        assert_eq!(out.candidates[0].id, "d2");
        assert_eq!(out.candidates[0].score, 1.0);
    }

    #[test]
    fn labelled_and_not_validated() {
        let out = fixture().rank("fracture", 2).unwrap();
        assert_eq!(out.arm_id, "sml-grok-icm-concurrent");
        assert_eq!(out.backend_id, SignaturePageRanker::BACKEND_ID);
        assert_eq!(out.status, ValidationStatus::NotBeirValidated);
    }

    #[test]
    fn contract_finite_sorted_kcut_deterministic() {
        let mut r = fixture();
        for k in 0..6 {
            let out = r.rank("vitamin d fracture", k).unwrap();
            assert_eq!(out.candidates.len(), k.min(4));
            for w in out.candidates.windows(2) {
                assert!(w[0].score > w[1].score || (w[0].score == w[1].score && w[0].id < w[1].id));
            }
            assert!(out
                .candidates
                .iter()
                .all(|c| c.score.is_finite() && (0.0..=1.0).contains(&c.score)));
            assert_eq!(out, r.rank("vitamin d fracture", k).unwrap());
        }
    }

    #[test]
    fn ties_broken_by_id_asc() {
        let mut r = SignaturePageRanker::new(7);
        r.insert("b", "same text").unwrap();
        r.insert("a", "same text").unwrap();
        r.insert("c", "zz").unwrap();
        let ids: Vec<_> = r
            .rank("unrelated query words", 3)
            .unwrap()
            .candidates
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn empty_query_and_duplicates_rejected() {
        let mut r = fixture();
        assert_eq!(r.rank("  \t", 3), Err(RankError::EmptyQuery));
        assert_eq!(
            r.insert("d1", "x y"),
            Err(RankError::DuplicateDocument("d1".into()))
        );
        let out = r.rank("!!", 3).unwrap();
        assert!(out.candidates.iter().all(|c| c.score == 0.0));
    }

    #[test]
    fn icm_repairs_single_flip_and_fails_closed_on_double() {
        let mut r = fixture();
        let clean = r.rank("vitamin d fracture", 4).unwrap();
        r.page_words_mut_unguarded(0)[3] ^= 1 << 17;
        assert_eq!(r.rank("vitamin d fracture", 4).unwrap(), clean);
        let w = r.page_words_mut_unguarded(1);
        w[5] ^= 0b11;
        assert_eq!(
            r.rank("vitamin d fracture", 4),
            Err(RankError::CorruptPage("d2".into()))
        );
    }

    #[test]
    fn unavailable_mirror() {
        let mut u = UnavailableRanker;
        assert!(matches!(u.rank("q", 1), Err(RankError::Unavailable(_))));
        assert_eq!(u.backend_id(), "sml-unavailable");
    }
}
