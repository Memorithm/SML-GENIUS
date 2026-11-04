//! CONTEXT-3 G1: real-text eval derivation rules (scaffolding only).
//!
//! Pre-registered, deterministic rules that turn a [`PinnedRealTextCorpus`]
//! into segments, (query, target) pairs and a near-duplicate leakage check.
//! Measures nothing; produces no score. See
//! `docs/G1-CONTEXT-3-REALTEXT-CORPUS-PROTOCOL.md` ("Derivation rules v1").
//!
//! The near-duplicate threshold is **provisional** and has not been validated
//! against any labelled data.

use std::collections::BTreeSet;

use crate::g1_realtext_corpus::{PinnedRealTextCorpus, RealTextDoc, RealTextManifest};
use crate::raw_context::sha256_raw_bytes;

/// Frozen v1 parameters. Changing any value is a new protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivationParams {
    /// Tokens per segment (non-overlapping windows).
    pub segment_tokens: usize,
    /// A trailing partial segment is kept iff it has at least this many tokens.
    pub min_tail_tokens: usize,
    /// Tokens per query window (must be <= `min_tail_tokens`).
    pub query_tokens: usize,
    /// Shingle width for near-duplicate Jaccard.
    pub shingle: usize,
    /// Near-duplicate refusal if Jaccard >= `num / den` (provisional).
    pub near_dup_num: u64,
    pub near_dup_den: u64,
}

pub const PARAMS_V1: DerivationParams = DerivationParams {
    segment_tokens: 64,
    min_tail_tokens: 16,
    query_tokens: 8,
    shingle: 5,
    near_dup_num: 8,
    near_dup_den: 10,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DerivationError {
    BadParams,
    Hash,
    /// Normalized-exact or shingle-Jaccard near duplicate across train/eval.
    NearDuplicate {
        train_id: String,
        eval_id: String,
        /// Jaccard numerator/denominator (intersection, union) of shingle sets.
        inter: u64,
        union: u64,
    },
    /// No eval document yields a segment, so no query can be derived.
    NoQueries,
}

/// Normalization: Unicode lowercase; every non-alphanumeric char is a
/// separator; tokens are maximal alphanumeric runs.
#[must_use]
pub fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// `<doc_id>#<index>` (index from 0).
    pub segment_id: String,
    pub doc_id: String,
    pub index: usize,
    pub tokens: Vec<String>,
}

impl DerivationParams {
    fn check(&self) -> Result<(), DerivationError> {
        let ok = self.segment_tokens > 0
            && self.min_tail_tokens > 0
            && self.min_tail_tokens <= self.segment_tokens
            && self.query_tokens > 0
            && self.query_tokens <= self.min_tail_tokens
            && self.shingle > 0
            && self.near_dup_den > 0
            && self.near_dup_num <= self.near_dup_den;
        if ok {
            Ok(())
        } else {
            Err(DerivationError::BadParams)
        }
    }
}

/// Cut one document into non-overlapping `segment_tokens` windows; a trailing
/// window shorter than `min_tail_tokens` is dropped. Segments never cross
/// documents.
pub fn segment_doc(
    doc: &RealTextDoc,
    p: &DerivationParams,
) -> Result<Vec<Segment>, DerivationError> {
    p.check()?;
    Ok(tokens(&doc.text)
        .chunks(p.segment_tokens)
        .filter(|c| c.len() >= p.min_tail_tokens)
        .enumerate()
        .map(|(index, c)| Segment {
            segment_id: format!("{}#{index}", doc.doc_id),
            doc_id: doc.doc_id.clone(),
            index,
            tokens: c.to_vec(),
        })
        .collect())
}

fn hash_u64(label: &str) -> Result<u64, DerivationError> {
    let h = sha256_raw_bytes(label.as_bytes()).map_err(|_| DerivationError::Hash)?;
    let mut b = [0_u8; 8];
    b.copy_from_slice(&h.bytes()[..8]);
    Ok(u64::from_be_bytes(b))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTarget {
    pub doc_id: String,
    /// Gold segment (the only correct answer).
    pub target_segment_id: String,
    /// Token offset of the query window inside the target segment.
    pub offset: usize,
    pub query_tokens: Vec<String>,
}

/// One query per eval document with >= 1 segment:
/// target index `= H("g1q-target|" + doc_id) mod n_segments`,
/// offset `= H("g1q-offset|" + doc_id) mod (len(target) - query_tokens + 1)`,
/// query = the contiguous `query_tokens` window at that offset. `H` = first 8
/// bytes of SHA-256, big-endian. Lexical span-recall probe, not semantic.
pub fn derive_query(
    doc: &RealTextDoc,
    p: &DerivationParams,
) -> Result<Option<QueryTarget>, DerivationError> {
    let segs = segment_doc(doc, p)?;
    if segs.is_empty() {
        return Ok(None);
    }
    let n = segs.len() as u64;
    let t = &segs[(hash_u64(&format!("g1q-target|{}", doc.doc_id))? % n) as usize];
    let span = (t.tokens.len() - p.query_tokens + 1) as u64;
    let offset = (hash_u64(&format!("g1q-offset|{}", doc.doc_id))? % span) as usize;
    Ok(Some(QueryTarget {
        doc_id: doc.doc_id.clone(),
        target_segment_id: t.segment_id.clone(),
        offset,
        query_tokens: t.tokens[offset..offset + p.query_tokens].to_vec(),
    }))
}

fn shingles(toks: &[String], w: usize) -> BTreeSet<String> {
    if toks.len() < w {
        return std::iter::once(toks.join(" ")).collect();
    }
    toks.windows(w).map(|s| s.join(" ")).collect()
}

/// Provisional near-duplicate rule: refuse if the normalized token sequences
/// are equal, or if `w`-shingle Jaccard >= `num/den`. Pairwise O(train*eval);
/// adequate for small pinned corpora only.
pub fn check_near_duplicates(
    train: &[RealTextDoc],
    eval: &[RealTextDoc],
    p: &DerivationParams,
) -> Result<(), DerivationError> {
    p.check()?;
    let prep = |d: &RealTextDoc| {
        let t = tokens(&d.text);
        (shingles(&t, p.shingle), t)
    };
    let tr: Vec<_> = train.iter().map(prep).collect();
    for e in eval {
        let (es, et) = prep(e);
        for (d, (ts, tt)) in train.iter().zip(&tr) {
            let inter = es.intersection(ts).count() as u64;
            let union = es.union(ts).count() as u64;
            if *tt == et || inter * p.near_dup_den >= p.near_dup_num * union {
                return Err(DerivationError::NearDuplicate {
                    train_id: d.doc_id.clone(),
                    eval_id: e.doc_id.clone(),
                    inter,
                    union,
                });
            }
        }
    }
    Ok(())
}

/// Eval set for a flagged real-text arm. Haystack = segments of eval docs only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealTextEvalSet {
    pub corpus_id: String,
    pub sha256_hex: String,
    pub segments: Vec<Segment>,
    pub queries: Vec<QueryTarget>,
}

impl RealTextEvalSet {
    /// Meta fields a real-text arm must print. `synthetic=false` makes the
    /// synthetic aggregator (`g1_latency_aggregate`) refuse the run.
    #[must_use]
    pub fn meta_fields(&self) -> String {
        meta_fields(&self.corpus_id, &self.sha256_hex)
    }
}

#[must_use]
pub fn meta_fields_for(m: &RealTextManifest) -> String {
    meta_fields(&m.corpus_id, &m.sha256_hex)
}

fn meta_fields(corpus_id: &str, sha: &str) -> String {
    format!("synthetic=false arm=realtext derivation=v1 corpus_id={corpus_id} sha256={sha}")
}

/// Near-dup check, then segmentation + one query per eval doc. Fail-closed.
pub fn build_eval_set(
    c: &PinnedRealTextCorpus,
    p: &DerivationParams,
) -> Result<RealTextEvalSet, DerivationError> {
    check_near_duplicates(c.train(), c.eval(), p)?;
    let mut segments = Vec::new();
    let mut queries = Vec::new();
    for d in c.eval() {
        segments.extend(segment_doc(d, p)?);
        queries.extend(derive_query(d, p)?);
    }
    if queries.is_empty() {
        return Err(DerivationError::NoQueries);
    }
    Ok(RealTextEvalSet {
        corpus_id: c.manifest().corpus_id.clone(),
        sha256_hex: c.manifest().sha256_hex.clone(),
        segments,
        queries,
    })
}
