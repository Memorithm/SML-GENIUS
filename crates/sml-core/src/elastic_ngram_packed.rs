//! EL-NG1: an exactly accounted, reencodable Witten-Bell byte n-gram.
//!
//! This is a *baseline whose physical representation can change*, built so an
//! external control plane (ElasticXxx, in the separate `sml-elastic-bench`
//! crate) has something concrete and checkable to adapt. It is not an SML
//! architecture component and makes no quality claim: with count width 32 it
//! reproduces `char_baselines::NGramModel` bit for bit (tested), and every
//! other state is a deterministic, lossy [`ElasticNgram::reencode`] of it.
//!
//! The model owns three things the control plane cannot know:
//!
//! * the exact storage cost of a state ([`ElasticNgram::account`]), verified
//!   against a real canonical bit stream ([`ElasticNgram::encode`]);
//! * which transitions are possible at all (reencode only ever narrows;
//!   widening is a typed refusal because the information is gone);
//! * the quality of a state ([`ElasticNgram::score`]).
//!
//! Frozen protocol: `docs/EL-NG1-ELASTIC-NGRAM-PREREGISTRATION.md`.

use std::collections::HashMap;
use std::fmt;

use crate::anf_char::{ChunkLayout, LANES};
use crate::char_baselines::ByteScore;

/// Largest context length the key packing supports (matches `NGramModel`).
pub const MAX_SUPPORTED_CONTEXT: usize = 7;
/// Count widths (bits per stored count) a state may declare.
pub const WIDTHS: [u8; 5] = [32, 16, 8, 4, 2];
/// Width of the freshly trained base model.
pub const BASE_WIDTH: u8 = 32;

/// Typed failures of the elastic n-gram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElasticNgramError {
    /// Requested context length exceeds [`MAX_SUPPORTED_CONTEXT`].
    ContextTooLong(usize),
    /// Width is not one of [`WIDTHS`].
    UnsupportedWidth(u8),
    /// Training stream too short for the 64-lane chunk layout.
    TrainTooShort(usize),
    /// A reencode asked for a longer context than the source holds.
    ContextWidening { from: usize, to: usize },
    /// A reencode asked for a wider count than the source holds.
    WidthWidening { from: u8, to: u8 },
    /// A stored count does not fit the declared width.
    CountExceedsWidth { count: u32, width: u8 },
    /// The packed stream is malformed (message names the first violation).
    Packed(&'static str),
}

impl fmt::Display for ElasticNgramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextTooLong(k) => {
                write!(f, "context length {k} exceeds the supported maximum")
            }
            Self::UnsupportedWidth(w) => write!(f, "count width {w} is not a supported width"),
            Self::TrainTooShort(n) => {
                write!(f, "{n} training bytes are too few for the chunk layout")
            }
            Self::ContextWidening { from, to } => {
                write!(
                    f,
                    "cannot widen context length {from} to {to}: information was dropped"
                )
            }
            Self::WidthWidening { from, to } => {
                write!(
                    f,
                    "cannot widen count width {from} to {to}: precision was dropped"
                )
            }
            Self::CountExceedsWidth { count, width } => {
                write!(f, "count {count} does not fit in {width} bits")
            }
            Self::Packed(msg) => write!(f, "malformed packed stream: {msg}"),
        }
    }
}

impl std::error::Error for ElasticNgramError {}

/// Exact storage accounting of one state, in bits, by category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageAccount {
    pub header_bits: u64,
    pub key_bits: u64,
    pub length_bits: u64,
    pub symbol_bits: u64,
    pub count_bits: u64,
    pub contexts: u64,
    pub entries: u64,
}

impl StorageAccount {
    /// Sum of every category.
    pub fn total_bits(&self) -> u64 {
        self.header_bits + self.key_bits + self.length_bits + self.symbol_bits + self.count_bits
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Context {
    total: u32,
    followers: Vec<(u8, u32)>,
}

impl Context {
    fn from_followers(followers: Vec<(u8, u32)>) -> Self {
        let total = followers.iter().map(|f| f.1).sum();
        Self { total, followers }
    }
}

fn context_key(history: &[u8], len: usize) -> u64 {
    history[history.len() - len..]
        .iter()
        .fold(0u64, |k, &b| k << 8 | u64::from(b))
}

fn count_cap(width: u8) -> u32 {
    if width >= 32 {
        u32::MAX
    } else {
        (1u32 << width) - 1
    }
}

fn check_width(width: u8) -> Result<(), ElasticNgramError> {
    if WIDTHS.contains(&width) {
        Ok(())
    } else {
        Err(ElasticNgramError::UnsupportedWidth(width))
    }
}

/// Interpolated Witten-Bell byte n-gram with a declared count width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElasticNgram {
    max_context: usize,
    width: u8,
    levels: Vec<HashMap<u64, Context>>,
}

impl ElasticNgram {
    /// Train on the shared 64-lane chunk layout (no context across chunk
    /// starts, first byte of a chunk not predicted), exactly like
    /// `char_baselines::NGramModel::train`. The result has width
    /// [`BASE_WIDTH`].
    pub fn train(bytes: &[u8], max_context: usize) -> Result<Self, ElasticNgramError> {
        if max_context > MAX_SUPPORTED_CONTEXT {
            return Err(ElasticNgramError::ContextTooLong(max_context));
        }
        let layout = ChunkLayout::new(bytes.len())
            .map_err(|_| ElasticNgramError::TrainTooShort(bytes.len()))?;
        let mut raw: Vec<HashMap<u64, HashMap<u8, u32>>> = vec![HashMap::new(); max_context + 1];
        for lane in 0..LANES {
            let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
            for t in 1..chunk.len() {
                for (len, level) in raw.iter_mut().enumerate() {
                    if len <= t {
                        *level
                            .entry(context_key(&chunk[..t], len))
                            .or_default()
                            .entry(chunk[t])
                            .or_default() += 1;
                    }
                }
            }
        }
        let levels = raw
            .into_iter()
            .map(|level| {
                level
                    .into_iter()
                    .map(|(key, followers)| {
                        let mut followers: Vec<(u8, u32)> = followers.into_iter().collect();
                        followers.sort_unstable();
                        (key, Context::from_followers(followers))
                    })
                    .collect()
            })
            .collect();
        Ok(Self {
            max_context,
            width: BASE_WIDTH,
            levels,
        })
    }

    /// Longest context length held.
    pub fn max_context(&self) -> usize {
        self.max_context
    }

    /// Declared count width in bits.
    pub fn width(&self) -> u8 {
        self.width
    }

    /// Stored (context, byte) counts.
    pub fn entries(&self) -> u64 {
        self.levels
            .iter()
            .flat_map(|l| l.values())
            .map(|c| c.followers.len() as u64)
            .sum()
    }

    /// Exact accounting of this state, by category.
    ///
    /// Header `16 + 32 * levels`; per context `8 * len` key bits and 8 length
    /// bits; per follower 8 symbol bits and `width` count bits.
    pub fn account(&self) -> StorageAccount {
        let mut acct = StorageAccount {
            header_bits: 16 + 32 * self.levels.len() as u64,
            key_bits: 0,
            length_bits: 0,
            symbol_bits: 0,
            count_bits: 0,
            contexts: 0,
            entries: 0,
        };
        for (len, level) in self.levels.iter().enumerate() {
            let contexts = level.len() as u64;
            let entries: u64 = level.values().map(|c| c.followers.len() as u64).sum();
            acct.contexts += contexts;
            acct.entries += entries;
            acct.key_bits += 8 * len as u64 * contexts;
            acct.length_bits += 8 * contexts;
            acct.symbol_bits += 8 * entries;
            acct.count_bits += u64::from(self.width) * entries;
        }
        acct
    }

    /// Deterministic narrowing to `(max_context, width)`.
    ///
    /// Drops levels above `max_context`; per surviving context shifts every
    /// count right by the smallest `s` such that the largest count fits the
    /// new width, then drops followers that reach 0 and contexts left empty.
    /// Widening either axis is refused: the dropped information cannot be
    /// recovered from this state (use [`ElasticNgram::train`] again).
    pub fn reencode(&self, max_context: usize, width: u8) -> Result<Self, ElasticNgramError> {
        check_width(width)?;
        if max_context > self.max_context {
            return Err(ElasticNgramError::ContextWidening {
                from: self.max_context,
                to: max_context,
            });
        }
        if width > self.width {
            return Err(ElasticNgramError::WidthWidening {
                from: self.width,
                to: width,
            });
        }
        let cap = count_cap(width);
        let levels = self.levels[..=max_context]
            .iter()
            .map(|level| {
                level
                    .iter()
                    .filter_map(|(&key, ctx)| {
                        let max = ctx.followers.iter().map(|f| f.1).max().unwrap_or(0);
                        let mut shift = 0u32;
                        while (max >> shift) > cap {
                            shift += 1;
                        }
                        let followers: Vec<(u8, u32)> = ctx
                            .followers
                            .iter()
                            .map(|&(sym, c)| (sym, c >> shift))
                            .filter(|f| f.1 > 0)
                            .collect();
                        if followers.is_empty() {
                            None
                        } else {
                            Some((key, Context::from_followers(followers)))
                        }
                    })
                    .collect()
            })
            .collect();
        Ok(Self {
            max_context,
            width,
            levels,
        })
    }

    /// Next-byte distribution after `history` (same arithmetic and operation
    /// order as `NGramModel::distribution`, so parity is bit-exact).
    pub fn distribution(&self, history: &[u8], probs: &mut [f64; 256]) {
        probs.iter_mut().for_each(|p| *p = 1.0 / 256.0);
        for len in 0..=self.max_context.min(history.len()) {
            if let Some(ctx) = self.levels[len].get(&context_key(history, len)) {
                let n = f64::from(ctx.total);
                let distinct = ctx.followers.len() as f64;
                let scale = distinct / (n + distinct);
                probs.iter_mut().for_each(|p| *p *= scale);
                for &(byte, count) in &ctx.followers {
                    probs[usize::from(byte)] += f64::from(count) / (n + distinct);
                }
            }
        }
    }

    /// Bits per byte and argmax accuracy on the shared chunked layout.
    pub fn score(&self, bytes: &[u8]) -> ByteScore {
        let layout = match ChunkLayout::new(bytes.len()) {
            Ok(layout) => layout,
            Err(_) => {
                return ByteScore {
                    tokens: 0,
                    bits_per_byte: f64::NAN,
                    accuracy: f64::NAN,
                }
            }
        };
        let mut probs = [0.0f64; 256];
        let (mut bits, mut correct, mut tokens) = (0.0, 0u64, 0u64);
        for lane in 0..LANES {
            let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
            for t in 1..chunk.len() {
                self.distribution(&chunk[..t], &mut probs);
                let y = usize::from(chunk[t]);
                bits -= probs[y].log2();
                let best = (0..256).fold(0, |b, v| if probs[v] > probs[b] { v } else { b });
                correct += u64::from(best == y);
                tokens += 1;
            }
        }
        ByteScore {
            tokens,
            bits_per_byte: bits / tokens as f64,
            accuracy: correct as f64 / tokens as f64,
        }
    }

    /// Canonical packed stream: `(bytes, length_in_bits)`.
    ///
    /// Layout (LSB-first): `max_context:8`, `width:8`, then per level
    /// `contexts:32` followed by that level's contexts sorted by key, each
    /// `key (len bytes, most significant first)`, `followers - 1 : 8`, and
    /// followers sorted by byte as `symbol:8, count:width`. The length in
    /// bits equals [`StorageAccount::total_bits`] by construction and is
    /// checked by tests, which makes the accounting verifiable.
    pub fn encode(&self) -> Result<(Vec<u8>, u64), ElasticNgramError> {
        let cap = count_cap(self.width);
        let mut w = BitWriter::default();
        w.write(self.max_context as u64, 8);
        w.write(u64::from(self.width), 8);
        for (len, level) in self.levels.iter().enumerate() {
            w.write(level.len() as u64, 32);
            let mut keys: Vec<u64> = level.keys().copied().collect();
            keys.sort_unstable();
            for key in keys {
                let ctx = &level[&key];
                for i in (0..len).rev() {
                    w.write((key >> (8 * i)) & 0xff, 8);
                }
                w.write(ctx.followers.len() as u64 - 1, 8);
                for &(sym, count) in &ctx.followers {
                    if count > cap {
                        return Err(ElasticNgramError::CountExceedsWidth {
                            count,
                            width: self.width,
                        });
                    }
                    w.write(u64::from(sym), 8);
                    w.write(u64::from(count), self.width);
                }
            }
        }
        Ok((w.bytes, w.bits))
    }

    /// Strict inverse of [`ElasticNgram::encode`].
    pub fn decode(bytes: &[u8], bits: u64) -> Result<Self, ElasticNgramError> {
        let err = ElasticNgramError::Packed;
        if bits > bytes.len() as u64 * 8 || bits + 8 <= bytes.len() as u64 * 8 {
            return Err(err("bit length inconsistent with byte length"));
        }
        let mut r = BitReader {
            bytes,
            pos: 0,
            end: bits,
        };
        let max_context = r.read(8).ok_or(err("truncated header"))? as usize;
        let width = r.read(8).ok_or(err("truncated header"))? as u8;
        if max_context > MAX_SUPPORTED_CONTEXT {
            return Err(err("context length out of range"));
        }
        check_width(width).map_err(|_| err("unsupported width"))?;
        let cap = count_cap(width);
        let mut levels = Vec::with_capacity(max_context + 1);
        for len in 0..=max_context {
            let n = r.read(32).ok_or(err("truncated level header"))?;
            let mut level = HashMap::new();
            let mut previous: Option<u64> = None;
            for _ in 0..n {
                let mut key = 0u64;
                for _ in 0..len {
                    key = key << 8 | r.read(8).ok_or(err("truncated key"))?;
                }
                if previous.is_some_and(|p| p >= key) {
                    return Err(err("keys not strictly ascending"));
                }
                previous = Some(key);
                let count = r.read(8).ok_or(err("truncated follower count"))? + 1;
                let mut followers = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    let sym = r.read(8).ok_or(err("truncated symbol"))? as u8;
                    let c = r.read(width).ok_or(err("truncated count"))?;
                    if c == 0 || c > u64::from(cap) {
                        return Err(err("count outside 1..=cap"));
                    }
                    if followers.last().is_some_and(|&(s, _): &(u8, u32)| s >= sym) {
                        return Err(err("symbols not strictly ascending"));
                    }
                    followers.push((sym, c as u32));
                }
                level.insert(key, Context::from_followers(followers));
            }
            levels.push(level);
        }
        if r.pos != bits {
            return Err(err("trailing bits"));
        }
        Ok(Self {
            max_context,
            width,
            levels,
        })
    }

    /// FNV-1a-64 over the packed bytes and bit length. A change detector for
    /// rollback tests, **not** a cryptographic hash.
    pub fn packed_fingerprint(&self) -> Result<u64, ElasticNgramError> {
        let (bytes, bits) = self.encode()?;
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in bytes.iter().copied().chain(bits.to_le_bytes()) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Ok(h)
    }
}

#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    bits: u64,
}

impl BitWriter {
    fn write(&mut self, value: u64, nbits: u8) {
        for i in 0..nbits {
            if self.bits % 8 == 0 {
                self.bytes.push(0);
            }
            let bit = (value >> i) & 1;
            let last = self.bytes.len() - 1;
            self.bytes[last] |= (bit as u8) << (self.bits % 8);
            self.bits += 1;
        }
    }
}

struct BitReader<'a> {
    bytes: &'a [u8],
    pos: u64,
    end: u64,
}

impl BitReader<'_> {
    fn read(&mut self, nbits: u8) -> Option<u64> {
        if self.pos + u64::from(nbits) > self.end {
            return None;
        }
        let mut v = 0u64;
        for i in 0..nbits {
            let byte = self.bytes[(self.pos / 8) as usize];
            v |= u64::from((byte >> (self.pos % 8)) & 1) << i;
            self.pos += 1;
        }
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::char_baselines::NGramModel;
    use crate::toy_tasks::SplitMix64;

    const WORDS: [&str; 12] = [
        "le ", "la ", "de ", "et ", "un ", "une ", "dans ", "pour ", "mais ", "avec ", "sans ",
        "tout ",
    ];

    /// Deterministic pseudo-French stream with repeated structure.
    fn corpus(len: usize, seed: u64) -> Vec<u8> {
        let mut rng = SplitMix64::new(seed);
        let mut out = Vec::with_capacity(len + 8);
        while out.len() < len {
            let w = WORDS[rng.below(WORDS.len())];
            out.extend_from_slice(w.as_bytes());
            if rng.below(7) == 0 {
                out.push(b'.');
            }
        }
        out.truncate(len);
        out
    }

    fn splits() -> (Vec<u8>, Vec<u8>) {
        let all = corpus(64 * 700, 11);
        let cut = all.len() * 8 / 10;
        (all[..cut].to_vec(), all[cut..].to_vec())
    }

    #[test]
    fn base_model_is_bit_identical_to_the_existing_baseline() {
        let (train, val) = splits();
        for k in 0..=5 {
            let elastic = ElasticNgram::train(&train, k).unwrap().score(&val);
            let baseline = NGramModel::train(&train, k).score(&val);
            assert_eq!(elastic.tokens, baseline.tokens, "k={k}");
            assert_eq!(
                elastic.bits_per_byte.to_bits(),
                baseline.bits_per_byte.to_bits(),
                "k={k}"
            );
            assert_eq!(
                elastic.accuracy.to_bits(),
                baseline.accuracy.to_bits(),
                "k={k}"
            );
        }
    }

    #[test]
    fn packed_length_equals_accounting_and_round_trips() {
        let (train, _) = splits();
        let base = ElasticNgram::train(&train, 4).unwrap();
        for k in 0..=4 {
            for &w in &WIDTHS {
                let m = base.reencode(k, w).unwrap();
                let (bytes, bits) = m.encode().unwrap();
                assert_eq!(bits, m.account().total_bits(), "k={k} w={w}");
                assert_eq!(
                    ElasticNgram::decode(&bytes, bits).unwrap(),
                    m,
                    "k={k} w={w}"
                );
                if w < 32 {
                    let cap = count_cap(w);
                    assert!(m
                        .levels
                        .iter()
                        .flat_map(|l| l.values())
                        .flat_map(|c| c.followers.iter())
                        .all(|f| f.1 >= 1 && f.1 <= cap));
                }
            }
        }
    }

    #[test]
    fn reencode_composition_is_path_independent() {
        let (train, _) = splits();
        let base = ElasticNgram::train(&train, 3).unwrap();
        let mut grid = Vec::new();
        for k in 0..=3 {
            for &w in &WIDTHS {
                grid.push((k, w, base.reencode(k, w).unwrap()));
            }
        }
        for (ki, wi, mi) in &grid {
            for (kt, wt, mt) in &grid {
                if kt <= ki && wt <= wi {
                    assert_eq!(
                        &mi.reencode(*kt, *wt).unwrap(),
                        mt,
                        "{ki}/{wi} -> {kt}/{wt}"
                    );
                }
            }
        }
        assert_eq!(base.reencode(3, 32).unwrap(), base);
    }

    #[test]
    fn narrowing_never_increases_size_and_widening_is_refused() {
        let (train, _) = splits();
        let base = ElasticNgram::train(&train, 3).unwrap();
        let mut prev = base.account().total_bits();
        for &w in &WIDTHS[1..] {
            let bits = base.reencode(3, w).unwrap().account().total_bits();
            assert!(bits <= prev, "w={w}");
            prev = bits;
        }
        let small = base.reencode(1, 8).unwrap();
        assert_eq!(
            small.reencode(2, 8),
            Err(ElasticNgramError::ContextWidening { from: 1, to: 2 })
        );
        assert_eq!(
            small.reencode(1, 16),
            Err(ElasticNgramError::WidthWidening { from: 8, to: 16 })
        );
        assert_eq!(
            small.reencode(1, 5),
            Err(ElasticNgramError::UnsupportedWidth(5))
        );
    }

    #[test]
    fn requantisation_changes_the_score_only_when_counts_are_clipped() {
        let (train, val) = splits();
        let base = ElasticNgram::train(&train, 3).unwrap();
        let exact = base.score(&val).bits_per_byte;
        // The pseudo-corpus has large counts: 2-bit counts must hurt.
        assert!(base.reencode(3, 2).unwrap().score(&val).bits_per_byte > exact);
        // Dropping to the same state is an identity on the score.
        assert_eq!(
            base.reencode(3, 32)
                .unwrap()
                .score(&val)
                .bits_per_byte
                .to_bits(),
            exact.to_bits()
        );
    }

    #[test]
    fn decode_rejects_malformed_streams() {
        let (train, _) = splits();
        let m = ElasticNgram::train(&train, 2)
            .unwrap()
            .reencode(2, 8)
            .unwrap();
        let (bytes, bits) = m.encode().unwrap();
        assert!(
            ElasticNgram::decode(&bytes, bits - 1).is_err(),
            "short length"
        );
        assert!(
            ElasticNgram::decode(&bytes[..bytes.len() - 1], bits).is_err(),
            "truncated bytes"
        );
        let mut zero_count = bytes.clone();
        // Header is 16 bits, then level 0 holds 32 bits of count, key (0 bytes),
        // 8 bits of follower count, 8 bits of symbol, then the first count.
        let first_count_bit = 16 + 32 + 8 + 8;
        for i in 0..8 {
            let b = first_count_bit + i;
            zero_count[b / 8] &= !(1 << (b % 8));
        }
        assert!(
            ElasticNgram::decode(&zero_count, bits).is_err(),
            "zero count"
        );
        assert!(
            ElasticNgram::decode(&bytes, bits + 8).is_err(),
            "bits beyond bytes"
        );
    }

    #[test]
    fn fingerprint_detects_any_change_and_is_stable() {
        let (train, _) = splits();
        let base = ElasticNgram::train(&train, 2).unwrap();
        let a = base.packed_fingerprint().unwrap();
        assert_eq!(a, base.clone().packed_fingerprint().unwrap());
        assert_ne!(
            a,
            base.reencode(2, 8).unwrap().packed_fingerprint().unwrap()
        );
        assert_ne!(
            a,
            base.reencode(1, 32).unwrap().packed_fingerprint().unwrap()
        );
    }

    #[test]
    fn rejects_unusable_training_and_configuration() {
        assert_eq!(
            ElasticNgram::train(&[1, 2, 3], 2),
            Err(ElasticNgramError::TrainTooShort(3))
        );
        let (train, _) = splits();
        assert_eq!(
            ElasticNgram::train(&train, MAX_SUPPORTED_CONTEXT + 1),
            Err(ElasticNgramError::ContextTooLong(MAX_SUPPORTED_CONTEXT + 1))
        );
        assert!(ElasticNgram::train(&train, 0)
            .unwrap()
            .score(&[1])
            .bits_per_byte
            .is_nan());
    }
}
