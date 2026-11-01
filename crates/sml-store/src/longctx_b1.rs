//! LONGCTX-B1 harness substrate (docs/SML-LONGCTX-B1-PREREGISTRATION.md).
//!
//! Generator, scorer and per-cell accounting only. This module runs no
//! campaign, reports no metric and supports no H1–H4 claim. Filler corpus pin,
//! B-lex, B-attn and RemoteOps dispatch plug in through the hooks below.

use std::fmt;

use sml_core::context_ledger::ContextContentHash;

use crate::raw_context::sha256_raw_bytes;

/// Frozen B1 tokenizer identifier (prereg §2).
pub const SML_BYTE_V0: &str = "sml-byte-v0";

/// `sml-byte-v0`: one token = one UTF-8 byte of the raw payload.
#[must_use]
pub fn sml_byte_v0_token_count(raw: &[u8]) -> u64 {
    raw.len() as u64
}

/// Frozen span ladder in byte-tokens (prereg §3).
pub const SPAN_LADDER: [u64; 5] = [4096, 16384, 65536, 262144, 1048576];

/// Frozen generation seeds (prereg §9).
pub const B1_SEEDS: [u64; 5] = [
    0xB100_0001,
    0xB100_0002,
    0xB100_0003,
    0xB100_0004,
    0xB100_0005,
];

/// Frozen task-1 depths in permille of the filler (prereg §5).
pub const TASK1_DEPTHS_PERMILLE: [u16; 7] = [0, 100, 250, 500, 750, 900, 1000];
/// Frozen task-1 near-miss distractor count (prereg §5).
pub const TASK1_DISTRACTORS: usize = 16;
/// Frozen task-2 needle counts (prereg §5).
pub const TASK2_K_VALUES: [usize; 3] = [2, 4, 8];
/// Frozen per-cell instance count (prereg §5).
pub const INSTANCES_PER_CELL: usize = 20;

/// DEV/TEST split (prereg §9). DEV = seeds 1–2, TEST = seeds 3–5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum B1Split {
    Dev,
    Test,
}

impl B1Split {
    /// Fails closed on any seed outside the frozen list.
    pub fn for_seed(seed: u64) -> Result<Self, B1Error> {
        match B1_SEEDS.iter().position(|s| *s == seed) {
            Some(0 | 1) => Ok(Self::Dev),
            Some(_) => Ok(Self::Test),
            None => Err(B1Error::UnknownSeed(seed)),
        }
    }

    /// Disjoint key namespaces: first hex nibble 0–7 = DEV, 8–f = TEST.
    #[must_use]
    pub const fn first_nibble_range(self) -> (u8, u8) {
        match self {
            Self::Dev => (0x0, 0x7),
            Self::Test => (0x8, 0xf),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum B1Error {
    UnknownSeed(u64),
    SpanNotOnLadder(u64),
    SpanTooSmall { span: u64, inserted: u64 },
    NeedleInFiller { offset: usize },
    FillerNotPinned,
    FillerShaMismatch,
    InvalidDepth(u16),
    InvalidK(usize),
    AccountingInvariant(&'static str),
    Hash,
}

impl fmt::Display for B1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for B1Error {}

/// Fails closed on any span outside the frozen ladder.
pub fn validate_span(span: u64) -> Result<u64, B1Error> {
    if SPAN_LADDER.contains(&span) {
        Ok(span)
    } else {
        Err(B1Error::SpanNotOnLadder(span))
    }
}

/// Deterministic SplitMix64 stream; the only randomness source of the generator.
#[derive(Debug, Clone)]
pub struct B1Rng {
    state: u64,
}

impl B1Rng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..bound` (bound > 0) by rejection, no modulo bias.
    pub fn below(&mut self, bound: u64) -> u64 {
        assert!(bound > 0, "below(0)");
        let zone = u64::MAX - (u64::MAX % bound);
        loop {
            let v = self.next_u64();
            if v < zone {
                return v % bound;
            }
        }
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// `n` lowercase hex digits; the first nibble is forced into the split namespace.
pub fn hex_in_namespace(rng: &mut B1Rng, n: usize, split: B1Split) -> Vec<u8> {
    let (lo, hi) = split.first_nibble_range();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let nibble = if i == 0 {
            lo + rng.below(u64::from(hi - lo) + 1) as u8
        } else {
            rng.below(16) as u8
        };
        out.push(HEX[usize::from(nibble)]);
    }
    out
}

/// Near-miss key at hex Hamming distance `distance` (1 or 2); nibble 0 is kept
/// so the near-miss stays in the same namespace.
pub fn near_miss_key(rng: &mut B1Rng, key: &[u8], distance: usize) -> Vec<u8> {
    assert!((1..=2).contains(&distance) && key.len() >= 3);
    let mut out = key.to_vec();
    let first = 1 + rng.below(key.len() as u64 - 1) as usize;
    let mut positions = vec![first];
    while positions.len() < distance {
        let p = 1 + rng.below(key.len() as u64 - 1) as usize;
        if !positions.contains(&p) {
            positions.push(p);
        }
    }
    for p in positions {
        let old = out[p];
        loop {
            let c = HEX[rng.below(16) as usize];
            if c != old {
                out[p] = c;
                break;
            }
        }
    }
    out
}

/// Filler hook (prereg §5). The open-license corpus pin is a later PR; until a
/// source returns a pinned SHA-256, generated instances are fixture-only.
pub trait B1FillerSource {
    /// Stable identifier recorded in every cell.
    fn id(&self) -> &str;
    /// SHA-256 pinned by the fixture-freeze note, `None` while unpinned.
    fn pinned_sha256(&self) -> Option<ContextContentHash>;
    /// Exactly `len` deterministic filler bytes for `(seed, split)`.
    fn fill(&self, seed: u64, split: B1Split, len: usize) -> Vec<u8>;
}

/// Hand-written fixture filler for unit tests only. Never campaign-admissible.
#[derive(Debug, Clone)]
pub struct FixtureFiller {
    pub text: &'static [u8],
}

impl B1FillerSource for FixtureFiller {
    fn id(&self) -> &str {
        "fixture-filler-unpinned"
    }

    fn pinned_sha256(&self) -> Option<ContextContentHash> {
        None
    }

    fn fill(&self, _seed: u64, _split: B1Split, len: usize) -> Vec<u8> {
        self.text.iter().copied().cycle().take(len).collect()
    }
}

/// Campaign gate: the filler must be pinned and its corpus bytes must hash to the pin.
pub fn verify_filler_pin(
    source: &dyn B1FillerSource,
    corpus_bytes: &[u8],
) -> Result<ContextContentHash, B1Error> {
    let pin = source.pinned_sha256().ok_or(B1Error::FillerNotPinned)?;
    let actual = sha256_raw_bytes(corpus_bytes).map_err(|_| B1Error::Hash)?;
    if actual == pin {
        Ok(pin)
    } else {
        Err(B1Error::FillerShaMismatch)
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Prereg §9: needle strings never appear in filler; exact scan, abort on hit.
pub fn ensure_absent_from_filler(filler: &[u8], needles: &[&[u8]]) -> Result<(), B1Error> {
    for needle in needles {
        if let Some(offset) = find(filler, needle) {
            return Err(B1Error::NeedleInFiller { offset });
        }
    }
    Ok(())
}

/// One inserted line and its byte offset in the final haystack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insertion {
    pub depth_permille: u16,
    pub line: Vec<u8>,
    pub offset: u64,
}

/// Splices `lines` into filler so the haystack is exactly `span` byte-tokens.
/// Depth `d` maps to filler position `d * filler_len / 1000`; ties keep input order.
pub fn splice_haystack(
    source: &dyn B1FillerSource,
    seed: u64,
    split: B1Split,
    span: u64,
    lines: Vec<(u16, Vec<u8>)>,
) -> Result<(Vec<u8>, Vec<Insertion>), B1Error> {
    let inserted: u64 = lines.iter().map(|(_, l)| l.len() as u64).sum();
    if inserted >= span {
        return Err(B1Error::SpanTooSmall { span, inserted });
    }
    let filler_len = (span - inserted) as usize;
    let filler = source.fill(seed, split, filler_len);
    let mut planned: Vec<(usize, usize, u16, Vec<u8>)> = Vec::with_capacity(lines.len());
    for (order, (depth, line)) in lines.into_iter().enumerate() {
        if depth > 1000 {
            return Err(B1Error::InvalidDepth(depth));
        }
        let at = usize::from(depth) * filler_len / 1000;
        planned.push((at, order, depth, line));
    }
    let needle_refs: Vec<&[u8]> = planned.iter().map(|p| p.3.as_slice()).collect();
    ensure_absent_from_filler(&filler, &needle_refs)?;
    planned.sort_by_key(|p| (p.0, p.1));
    let mut out = Vec::with_capacity(span as usize);
    let mut cursor = 0usize;
    let mut insertions = Vec::with_capacity(planned.len());
    for (at, _, depth, line) in planned {
        out.extend_from_slice(&filler[cursor..at]);
        cursor = at;
        insertions.push(Insertion {
            depth_permille: depth,
            offset: out.len() as u64,
            line: line.clone(),
        });
        out.extend_from_slice(&line);
    }
    out.extend_from_slice(&filler[cursor..]);
    debug_assert_eq!(out.len() as u64, span);
    Ok((out, insertions))
}
