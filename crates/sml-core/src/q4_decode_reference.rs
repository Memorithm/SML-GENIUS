//! Independent reference oracle for the Q4 sparse-atlas next-byte decoder.
//!
//! This module re-derives, from the *raw bytes* of a headerless Q4 export,
//! the full 256-way next-byte distribution of
//! [`crate::sparse_gradient_atlas::QuantizedSparseAtlas`] for both
//! [`crate::byte_factorization::ByteFactorization`] modes, plus an exact
//! top-k. It deliberately shares **no code** with the production decoder:
//! the routing hash, feature slots, nibble decoding, logistic and the
//! factorization paths are re-implemented from the written specification
//! below, with different algorithms where possible (closed-form MSB-tree
//! node indices, breadth-first tree mass propagation, stable log-sigmoid).
//! A divergence between this oracle and production therefore points at a
//! real defect in one of the two, not at a shared helper.
//!
//! Specification (as merged in #1011/#1012, `sparse_gradient_atlas.rs`):
//! - File: `pages * 512` bytes, page `p` at offset `512 * p`, no header.
//! - Slot `s` of a page is nibble `s % 2` (0 = low, 1 = high) of byte `s / 2`;
//!   the nibble is a two's-complement integer `q` in `-8..=7`; weight `q / 2`.
//! - Address `x = a << 16 | b << 8 | head`; page `mix(x ^ K_PAGE) % pages`,
//!   slot of feature `f` = `(mix(x ^ K_SLOT) & 1023) + 59 f) & 1023`.
//! - Feature signs: `f = 0` bias `+1`; `f = 1..=8` bit `f-1` of `a`;
//!   `f = 9..=16` bit `f-9` of `b`; `+1` if set else `-1`.
//! - Head probability `P(bit = 1) = logistic(clamp(sum / 4, -16, 16))`.
//! - IndependentBits: head `k` (0..8) predicts bit `k` of the byte.
//!   MsbTree: the byte's bit at depth `d` (MSB first) is predicted by node
//!   `(1 << d) | (byte >> (8 - d))`, i.e. 255 internal nodes `1..=255`.
//!
//! The oracle is meant for tests and conformance checks of any decoder or
//! top-k API (e.g. Ripple's). It keeps the whole file in memory and is not
//! a production path. Scores are **log2 probabilities**.

use crate::byte_factorization::ByteFactorization;
use std::{cmp::Ordering, fmt};

/// Bytes per Q4 page (1024 four-bit weights).
pub const Q4_PAGE_BYTES: usize = 512;
const SLOTS: usize = 1024;
const FEATURES: usize = 17;
const LOGIT_DIVISOR: f64 = 4.0;
const LOGIT_CLAMP: f64 = 16.0;
const K_PAGE: u64 = 0x534d_4c2d_4752_4144;
const K_SLOT: u64 = 0x00A1_1FEB_1234;
const FEATURE_STRIDE: usize = 59;

/// Error raised by the oracle or the conformance checker.
#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceError(pub String);

impl fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ReferenceError {}

fn fail<T>(message: impl Into<String>) -> Result<T, ReferenceError> {
    Err(ReferenceError(message.into()))
}

/// SplitMix64 finaliser, written out from its published constants.
fn splitmix_finalize(z: u64) -> u64 {
    let z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Decodes one 4-bit two's-complement nibble to its integer level.
#[must_use]
pub fn nibble_level(nibble: u8) -> i8 {
    debug_assert!(nibble < 16);
    // Sign-extend bit 3 by shifting into the i8 sign position and back.
    ((nibble << 4) as i8) >> 4
}

/// Raw head addresses of a byte under a factorization, MSB-first for the
/// tree. Returns `(head, bit)` pairs, computed in closed form.
#[must_use]
pub fn reference_decisions(factorization: ByteFactorization, byte: u8) -> [(usize, bool); 8] {
    let mut out = [(0usize, false); 8];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = match factorization {
            ByteFactorization::IndependentBits => (index, byte & (1 << index) != 0),
            ByteFactorization::MsbTree => {
                let prefix = if index == 0 {
                    0
                } else {
                    usize::from(byte) >> (8 - index)
                };
                ((1usize << index) | prefix, byte & (0x80 >> index) != 0)
            }
        };
    }
    out
}

/// Heads that a factorization reads, in ascending order.
#[must_use]
pub fn reference_heads(factorization: ByteFactorization) -> std::ops::Range<usize> {
    match factorization {
        ByteFactorization::IndependentBits => 0..8,
        ByteFactorization::MsbTree => 1..256,
    }
}

/// `log2(sigmoid(x))` without cancellation: `-log2(1 + e^-x)` for `x >= 0`,
/// `(x - ln(1 + e^x)) / ln 2` otherwise.
#[must_use]
pub fn log2_sigmoid(x: f64) -> f64 {
    let ln = if x >= 0.0 {
        -(-x).exp().ln_1p()
    } else {
        x - x.exp().ln_1p()
    };
    ln / std::f64::consts::LN_2
}

/// Sum of eight log2 factors in a canonical (ascending) order, so that two
/// bytes whose factor multisets are equal get *bitwise* equal scores. This
/// makes the oracle's exact ties coincide with mathematical ties, which is
/// what the lowest-byte tie-break rule is about.
#[must_use]
pub fn canonical_sum(mut terms: [f64; 8]) -> f64 {
    terms.sort_by(f64::total_cmp);
    terms.iter().sum()
}

/// In-memory oracle over a raw Q4 export.
#[derive(Clone, Debug)]
pub struct ReferenceQ4 {
    pages: u64,
    bytes: Vec<u8>,
}

impl ReferenceQ4 {
    /// Wraps raw export bytes. The page count is the byte length / 512.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ReferenceError> {
        if bytes.is_empty() || bytes.len() % Q4_PAGE_BYTES != 0 {
            return fail("Q4 bytes must be a non-empty multiple of 512");
        }
        let pages = (bytes.len() / Q4_PAGE_BYTES) as u64;
        Ok(Self { pages, bytes })
    }

    #[must_use]
    pub fn pages(&self) -> u64 {
        self.pages
    }

    fn address(a: u8, b: u8, head: usize) -> u64 {
        (u64::from(a) << 16) | (u64::from(b) << 8) | head as u64
    }

    /// Page routed for `(a, b, head)`.
    #[must_use]
    pub fn page_of(&self, a: u8, b: u8, head: usize) -> u64 {
        splitmix_finalize(Self::address(a, b, head) ^ K_PAGE) % self.pages
    }

    /// Slot (0..1024) of feature `feature` for `(a, b, head)`.
    #[must_use]
    pub fn slot_of(a: u8, b: u8, head: usize, feature: usize) -> usize {
        let base = splitmix_finalize(Self::address(a, b, head) ^ K_SLOT) as usize % SLOTS;
        (base + FEATURE_STRIDE * feature) % SLOTS
    }

    fn sign(a: u8, b: u8, feature: usize) -> f64 {
        let set = match feature {
            0 => true,
            1..=8 => a & (1 << (feature - 1)) != 0,
            _ => b & (1 << (feature - 9)) != 0,
        };
        if set {
            1.0
        } else {
            -1.0
        }
    }

    /// Dequantised weight (`level / 2`) at `(page, slot)`.
    pub fn weight(&self, page: u64, slot: usize) -> Result<f64, ReferenceError> {
        if page >= self.pages || slot >= SLOTS {
            return fail("page/slot outside the Q4 atlas");
        }
        let byte = self.bytes[page as usize * Q4_PAGE_BYTES + slot / 2];
        let nibble = if slot % 2 == 0 {
            byte & 0x0f
        } else {
            byte >> 4
        };
        Ok(f64::from(nibble_level(nibble)) * 0.5)
    }

    /// Clamped logit `clamp(sum / 4, -16, 16)` of one head.
    pub fn head_logit(&self, a: u8, b: u8, head: usize) -> Result<f64, ReferenceError> {
        if head > 255 {
            return fail("head index must fit in eight bits");
        }
        let page = self.page_of(a, b, head);
        let mut sum = 0.0;
        for feature in 0..FEATURES {
            sum +=
                self.weight(page, Self::slot_of(a, b, head, feature))? * Self::sign(a, b, feature);
        }
        Ok((sum / LOGIT_DIVISOR).clamp(-LOGIT_CLAMP, LOGIT_CLAMP))
    }

    /// `P(bit = 1)` of one head.
    pub fn head_probability(&self, a: u8, b: u8, head: usize) -> Result<f64, ReferenceError> {
        let z = self.head_logit(a, b, head)?;
        Ok(1.0 / (1.0 + (-z).exp()))
    }

    /// `log2 P(bit = value)` from the head logit, via a stable log-sigmoid.
    /// The term depends only on `+z` (bit 1) or `-z` (bit 0), so
    /// mathematically equal factors are bitwise equal.
    fn head_term(&self, a: u8, b: u8, head: usize, value: bool) -> Result<f64, ReferenceError> {
        let z = self.head_logit(a, b, head)?;
        Ok(log2_sigmoid(if value { z } else { -z }))
    }

    /// Full 256-way distribution in the linear domain. For the tree, the
    /// mass is propagated breadth-first from the root to the 256 leaves.
    pub fn distribution(
        &self,
        factorization: ByteFactorization,
        a: u8,
        b: u8,
    ) -> Result<[f64; 256], ReferenceError> {
        let mut out = [0.0; 256];
        match factorization {
            ByteFactorization::IndependentBits => {
                let mut p1 = [0.0; 8];
                for (bit, p) in p1.iter_mut().enumerate() {
                    *p = self.head_probability(a, b, bit)?;
                }
                for (byte, mass) in out.iter_mut().enumerate() {
                    *mass = (0..8)
                        .map(|bit| {
                            if byte & (1 << bit) != 0 {
                                p1[bit]
                            } else {
                                1.0 - p1[bit]
                            }
                        })
                        .product();
                }
            }
            ByteFactorization::MsbTree => {
                // node n has children 2n (bit 0) and 2n+1 (bit 1); leaves 256..512.
                let mut mass = vec![0.0; 512];
                mass[1] = 1.0;
                for node in 1..256 {
                    let p = self.head_probability(a, b, node)?;
                    mass[2 * node] = mass[node] * (1.0 - p);
                    mass[2 * node + 1] = mass[node] * p;
                }
                out.copy_from_slice(&mass[256..512]);
            }
        }
        Ok(out)
    }

    /// Full 256-way distribution as log2 probabilities (stable domain).
    pub fn log2_distribution(
        &self,
        factorization: ByteFactorization,
        a: u8,
        b: u8,
    ) -> Result<[f64; 256], ReferenceError> {
        let mut table = [(0.0, 0.0); 256];
        for head in reference_heads(factorization) {
            table[head] = (
                self.head_term(a, b, head, true)?,
                self.head_term(a, b, head, false)?,
            );
        }
        let mut out = [0.0; 256];
        for (byte, score) in out.iter_mut().enumerate() {
            let mut terms = [0.0; 8];
            for (term, &(head, bit)) in terms
                .iter_mut()
                .zip(&reference_decisions(factorization, byte as u8))
            {
                *term = if bit { table[head].0 } else { table[head].1 };
            }
            *score = canonical_sum(terms);
        }
        Ok(out)
    }

    /// Exact top-k `(byte, log2 p)` with the deterministic tie-break.
    pub fn top_k(
        &self,
        factorization: ByteFactorization,
        a: u8,
        b: u8,
        k: usize,
    ) -> Result<Vec<(u8, f64)>, ReferenceError> {
        exact_top_k(&self.log2_distribution(factorization, a, b)?, k)
    }
}

/// Total order used by every top-k in this module: higher score first,
/// then the numerically lower byte. NaN is rejected before sorting.
fn rank(x: &(u8, f64), y: &(u8, f64)) -> Ordering {
    y.1.total_cmp(&x.1).then(x.0.cmp(&y.0))
}

/// Exact top-k of a 256-way score table (log2 probabilities). Returns
/// `min(k, 256)` entries ordered by score descending, ties by byte ascending.
pub fn exact_top_k(scores: &[f64; 256], k: usize) -> Result<Vec<(u8, f64)>, ReferenceError> {
    if scores.iter().any(|s| s.is_nan()) {
        return fail("NaN score in the byte distribution");
    }
    let mut all: Vec<(u8, f64)> = scores
        .iter()
        .enumerate()
        .map(|(byte, &s)| (byte as u8, s))
        .collect();
    all.sort_by(rank);
    all.truncate(k.min(256));
    Ok(all)
}

/// Exact lazy top-k for [`ByteFactorization::MsbTree`]: best-first search
/// over the byte tree. A path's log2 mass never increases with depth, so
/// leaves leave the frontier in rank order and only internal nodes on
/// explored paths are evaluated. `logit(node)` returns the clamped head
/// logit of internal node `1..=255`. Leaves are re-scored with
/// [`canonical_sum`], and the search continues past the k-th leaf until the
/// frontier is [`BEST_FIRST_MARGIN`] bits below it, so path-order rounding
/// cannot break the lowest-byte tie rule. Returns the top-k and the number
/// of distinct heads evaluated (each at most once).
///
/// This is an *alternative algorithm* proposed for comparison with the
/// 255-head enumeration in `predict_next_byte`; it is checked against
/// [`exact_top_k`] by the conformance helper, not trusted on its own.
pub fn msb_tree_top_k_best_first<F>(
    mut logit: F,
    k: usize,
) -> Result<(Vec<(u8, f64)>, u16), ReferenceError>
where
    F: FnMut(usize) -> Result<f64, ReferenceError>,
{
    #[derive(PartialEq)]
    struct Entry(f64, usize);
    impl Eq for Entry {}
    impl PartialOrd for Entry {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Entry {
        fn cmp(&self, other: &Self) -> Ordering {
            self.0.total_cmp(&other.0).then(other.1.cmp(&self.1))
        }
    }
    let k = k.min(256);
    let mut z = [f64::NAN; 256];
    let mut evaluated = 0u16;
    let mut leaves: Vec<(u8, f64)> = Vec::new();
    let mut heap = std::collections::BinaryHeap::new();
    heap.push(Entry(0.0, 1));
    while let Some(top) = heap.peek() {
        if leaves.len() >= k {
            leaves.sort_by(rank);
            let boundary = if k == 0 {
                f64::INFINITY
            } else {
                leaves[k - 1].1
            };
            if top.0 < boundary - BEST_FIRST_MARGIN {
                break;
            }
        }
        let Entry(score, node) = heap.pop().expect("peeked");
        if node >= 256 {
            let byte = (node - 256) as u8;
            let mut terms = [0.0; 8];
            for (term, &(head, bit)) in terms
                .iter_mut()
                .zip(&reference_decisions(ByteFactorization::MsbTree, byte))
            {
                *term = log2_sigmoid(if bit { z[head] } else { -z[head] });
            }
            leaves.push((byte, canonical_sum(terms)));
            continue;
        }
        let value = logit(node)?;
        if !value.is_finite() {
            return fail("nonfinite head logit");
        }
        z[node] = value;
        evaluated += 1;
        heap.push(Entry(score + log2_sigmoid(-value), 2 * node));
        heap.push(Entry(score + log2_sigmoid(value), 2 * node + 1));
    }
    leaves.sort_by(rank);
    leaves.truncate(k);
    Ok((leaves, evaluated))
}

/// Safety margin (bits) of [`msb_tree_top_k_best_first`] beyond the k-th leaf.
pub const BEST_FIRST_MARGIN: f64 = 1e-6;

/// Tie policy of the conformance checker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TieRule {
    /// Bytes with bitwise-equal oracle scores (mathematical ties, see
    /// [`canonical_sum`]) must appear lowest byte first, including across
    /// the k-th boundary. Required for a new top-k API.
    LowestByte,
    /// Any order among bytes whose oracle scores are within `tol`.
    WithinTolerance,
}

/// Checks a candidate top-k list against the oracle's 256-way log2 table.
///
/// Rules (all must hold):
/// 1. exactly `min(k, 256)` entries, distinct bytes, finite scores;
/// 2. each score equals the oracle log2 probability of its byte within `tol`;
/// 3. entry `i` has the oracle score of the oracle's `i`-th byte within `tol`
///    (so only bytes whose scores are within `tol` may swap ranks);
/// 4. scores are non-increasing (within `tol`);
/// 5. with [`TieRule::LowestByte`], bytes whose oracle scores are *bitwise
///    equal* appear in ascending byte order, also across the k-th boundary.
pub fn check_topk_conformant(
    candidate: &[(u8, f64)],
    oracle_log2: &[f64; 256],
    k: usize,
    tol: f64,
    ties: TieRule,
) -> Result<(), ReferenceError> {
    let expected = exact_top_k(oracle_log2, k)?;
    if candidate.len() != expected.len() {
        return fail(format!(
            "top-k length {} != min(k, 256) = {}",
            candidate.len(),
            expected.len()
        ));
    }
    let mut seen = [false; 256];
    for (rank_index, (&(byte, score), &(want_byte, want))) in
        candidate.iter().zip(&expected).enumerate()
    {
        if seen[usize::from(byte)] {
            return fail(format!("byte {byte:#04x} repeated in top-k"));
        }
        seen[usize::from(byte)] = true;
        if !score.is_finite() {
            return fail(format!("rank {rank_index}: nonfinite score"));
        }
        let truth = oracle_log2[usize::from(byte)];
        if (score - truth).abs() > tol {
            return fail(format!(
                "rank {rank_index}: byte {byte:#04x} score {score} != oracle {truth}"
            ));
        }
        if (truth - want).abs() > tol {
            return fail(format!(
                "rank {rank_index}: byte {byte:#04x} (oracle {truth}) is not the \
                 rank-{rank_index} byte (oracle {want})"
            ));
        }
        if ties == TieRule::LowestByte && byte != want_byte && truth.to_bits() == want.to_bits() {
            return fail(format!(
                "rank {rank_index}: exact tie must select byte {want_byte:#04x}, got {byte:#04x}"
            ));
        }
    }
    for (index, pair) in candidate.windows(2).enumerate() {
        let (x, y) = (pair[0], pair[1]);
        if y.1 > x.1 + tol {
            return fail(format!(
                "scores increase between ranks {index} and {}",
                index + 1
            ));
        }
        let (tx, ty) = (oracle_log2[usize::from(x.0)], oracle_log2[usize::from(y.0)]);
        if ties == TieRule::LowestByte && tx.to_bits() == ty.to_bits() && x.0 > y.0 {
            return fail(format!(
                "exact tie between {:#04x} and {:#04x} not broken by ascending byte",
                x.0, y.0
            ));
        }
    }
    Ok(())
}

/// Conformance harness for any top-k implementation (production,
/// Ripple's API, or an alternative). `impl_fn(a, b, k)` must return the
/// `(byte, log2 p)` list. Checks every `k` in `ks` against the oracle and
/// returns the first violation.
pub fn check_topk_impl<F, E>(
    mut impl_fn: F,
    oracle: &ReferenceQ4,
    factorization: ByteFactorization,
    context: (u8, u8),
    ks: &[usize],
    tol: f64,
    ties: TieRule,
) -> Result<(), ReferenceError>
where
    F: FnMut(u8, u8, usize) -> Result<Vec<(u8, f64)>, E>,
    E: fmt::Display,
{
    let table = oracle.log2_distribution(factorization, context.0, context.1)?;
    for &k in ks {
        let candidate = impl_fn(context.0, context.1, k)
            .map_err(|error| ReferenceError(format!("implementation failed for k={k}: {error}")))?;
        check_topk_conformant(&candidate, &table, k, tol, ties).map_err(|error| {
            ReferenceError(format!(
                "{factorization:?} ctx=({:#04x},{:#04x}) k={k}: {error}",
                context.0, context.1
            ))
        })?;
    }
    Ok(())
}

/// Default score tolerance (bits) of [`assert_topk_conformant`]. A decoder
/// that forms `log2(1 - p)` from a saturated `p` (|logit| = 16) loses about
/// `8 * 2^-53 / 1.1e-7 / ln 2 = 1.2e-8` bits per byte to cancellation; the
/// production decoder does exactly that, so `1e-9` would be too strict.
pub const DEFAULT_TOL_BITS: f64 = 1e-7;

/// Panicking wrapper of [`check_topk_impl`] for tests: tolerance
/// [`DEFAULT_TOL_BITS`], [`TieRule::LowestByte`], and
/// `ks = [0, 1, 2, 5, k, 255, 256, 300]`.
pub fn assert_topk_conformant<F, E>(
    impl_fn: F,
    oracle: &ReferenceQ4,
    factorization: ByteFactorization,
    context: (u8, u8),
    k: usize,
) where
    F: FnMut(u8, u8, usize) -> Result<Vec<(u8, f64)>, E>,
    E: fmt::Display,
{
    let ks = [0, 1, 2, 5, k, 255, 256, 300];
    if let Err(error) = check_topk_impl(
        impl_fn,
        oracle,
        factorization,
        context,
        &ks,
        DEFAULT_TOL_BITS,
        TieRule::LowestByte,
    ) {
        panic!("top-k conformance failure: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nibble_levels_are_twos_complement() {
        let levels: Vec<i8> = (0..16).map(nibble_level).collect();
        assert_eq!(
            levels,
            [0, 1, 2, 3, 4, 5, 6, 7, -8, -7, -6, -5, -4, -3, -2, -1]
        );
    }

    #[test]
    fn closed_form_tree_nodes_cover_255_internal_nodes() {
        let mut count = [0u32; 256];
        for byte in 0..=255u8 {
            for (depth, (node, _)) in reference_decisions(ByteFactorization::MsbTree, byte)
                .iter()
                .enumerate()
            {
                assert_eq!(
                    node.leading_zeros() as usize,
                    usize::BITS as usize - 1 - depth
                );
                count[*node] += 1;
            }
        }
        assert_eq!(count[0], 0);
        for (node, &c) in count.iter().enumerate().skip(1) {
            let depth = usize::BITS - 1 - node.leading_zeros();
            assert_eq!(c, 256 >> depth, "node {node}");
        }
    }

    #[test]
    fn exact_top_k_tie_break_and_bounds() {
        let mut s = [-8.0; 256];
        s[9] = -1.0;
        s[3] = -1.0;
        let top = exact_top_k(&s, 3).unwrap();
        assert_eq!(top.iter().map(|e| e.0).collect::<Vec<_>>(), [3, 9, 0]);
        assert_eq!(exact_top_k(&s, 0).unwrap().len(), 0);
        assert_eq!(exact_top_k(&s, 999).unwrap().len(), 256);
        s[7] = f64::NAN;
        assert!(exact_top_k(&s, 1).is_err());
    }

    #[test]
    fn checker_rejects_wrong_lists() {
        let mut s = [-9.0; 256];
        s[1] = -0.5;
        s[2] = -2.0;
        s[4] = -2.0;
        assert!(
            check_topk_conformant(&[(1, -0.5), (2, -2.0)], &s, 2, 1e-9, TieRule::LowestByte)
                .is_ok()
        );
        // wrong tie-break, wrong score, wrong length, duplicate, missing best
        assert!(
            check_topk_conformant(&[(1, -0.5), (4, -2.0)], &s, 2, 1e-9, TieRule::LowestByte)
                .is_err()
        );
        assert!(check_topk_conformant(
            &[(1, -0.5), (4, -2.0), (2, -2.0)],
            &s,
            3,
            1e-9,
            TieRule::LowestByte
        )
        .is_err());
        assert!(
            check_topk_conformant(&[(1, -0.6), (2, -2.0)], &s, 2, 1e-9, TieRule::LowestByte)
                .is_err()
        );
        assert!(check_topk_conformant(&[(1, -0.5)], &s, 2, 1e-9, TieRule::LowestByte).is_err());
        assert!(
            check_topk_conformant(&[(1, -0.5), (1, -0.5)], &s, 2, 1e-9, TieRule::LowestByte)
                .is_err()
        );
        assert!(
            check_topk_conformant(&[(2, -2.0), (4, -2.0)], &s, 2, 1e-9, TieRule::LowestByte)
                .is_err()
        );
        assert!(check_topk_conformant(
            &[(1, -0.5), (4, -2.0)],
            &s,
            2,
            1e-9,
            TieRule::WithinTolerance
        )
        .is_ok());
    }

    #[test]
    fn log2_sigmoid_is_symmetric_and_stable() {
        for i in -128..=128 {
            let z = f64::from(i) / 8.0;
            let (p1, p0) = (log2_sigmoid(z), log2_sigmoid(-z));
            assert!(((p1.exp2() + p0.exp2()) - 1.0).abs() < 1e-15, "z={z}");
        }
        assert!(log2_sigmoid(16.0) < 0.0 && log2_sigmoid(16.0) > -2e-7);
        assert!((log2_sigmoid(-16.0) + 16.0 / std::f64::consts::LN_2).abs() < 1e-6);
    }
}
