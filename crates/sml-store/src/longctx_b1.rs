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

/// `KEY=<16 hex> VALUE=<32 hex>\n` (prereg §5 task 1).
#[must_use]
pub fn needle_line(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut line = Vec::with_capacity(4 + key.len() + 7 + value.len() + 1);
    line.extend_from_slice(b"KEY=");
    line.extend_from_slice(key);
    line.extend_from_slice(b" VALUE=");
    line.extend_from_slice(value);
    line.push(b'\n');
    line
}

/// Exact-needle instance (task 1). `answer` is the exact value bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task1Instance {
    pub seed: u64,
    pub split: B1Split,
    pub span: u64,
    pub depth_permille: u16,
    pub haystack: Vec<u8>,
    pub query_key: Vec<u8>,
    pub answer: Vec<u8>,
    pub insertions: Vec<Insertion>,
}

/// Generates one task-1 instance. Distractors get uniform random depths.
/// `span` is not ladder-checked here so fixtures can use small spans; the
/// campaign entry point must call [`validate_span`].
pub fn generate_task1(
    source: &dyn B1FillerSource,
    seed: u64,
    span: u64,
    depth_permille: u16,
    instance: u64,
) -> Result<Task1Instance, B1Error> {
    let split = B1Split::for_seed(seed)?;
    if !TASK1_DEPTHS_PERMILLE.contains(&depth_permille) {
        return Err(B1Error::InvalidDepth(depth_permille));
    }
    let mut rng = B1Rng::new(seed ^ (1 << 56) ^ (u64::from(depth_permille) << 32) ^ instance);
    let key = hex_in_namespace(&mut rng, 16, split);
    let value = hex_in_namespace(&mut rng, 32, split);
    let mut lines = vec![(depth_permille, needle_line(&key, &value))];
    for i in 0..TASK1_DISTRACTORS {
        let mut near = near_miss_key(&mut rng, &key, 1 + i % 2);
        while near == key {
            near = near_miss_key(&mut rng, &key, 1 + i % 2);
        }
        let other = hex_in_namespace(&mut rng, 32, split);
        let depth = rng.below(1001) as u16;
        lines.push((depth, needle_line(&near, &other)));
    }
    let (haystack, insertions) = splice_haystack(source, seed, split, span, lines)?;
    Ok(Task1Instance {
        seed,
        split,
        span,
        depth_permille,
        haystack,
        query_key: key,
        answer: value,
        insertions,
    })
}

/// Task-2 query: value lookup or membership (prereg §5 task 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Task2Query {
    ValueOf { key: Vec<u8>, answer: Vec<u8> },
    IsPresent { key: Vec<u8>, present: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task2Instance {
    pub seed: u64,
    pub split: B1Split,
    pub span: u64,
    pub k: usize,
    pub haystack: Vec<u8>,
    pub queries: Vec<Task2Query>,
    pub insertions: Vec<Insertion>,
}

/// Stratified depth of needle `i` among `k`: centre of the i-th of k strata.
#[must_use]
pub fn stratified_depth(i: usize, k: usize) -> u16 {
    ((2 * i + 1) * 1000 / (2 * k)) as u16
}

/// Generates one task-2 instance: k value queries plus k membership queries,
/// exactly half of the membership keys absent (prereg: 50% absent).
pub fn generate_task2(
    source: &dyn B1FillerSource,
    seed: u64,
    span: u64,
    k: usize,
    instance: u64,
) -> Result<Task2Instance, B1Error> {
    let split = B1Split::for_seed(seed)?;
    if !TASK2_K_VALUES.contains(&k) {
        return Err(B1Error::InvalidK(k));
    }
    let mut rng = B1Rng::new(seed ^ (2 << 56) ^ ((k as u64) << 32) ^ instance);
    let mut keys: Vec<Vec<u8>> = Vec::with_capacity(k);
    let mut lines = Vec::with_capacity(k);
    let mut queries = Vec::with_capacity(2 * k);
    for i in 0..k {
        let mut key = hex_in_namespace(&mut rng, 16, split);
        while keys.contains(&key) {
            key = hex_in_namespace(&mut rng, 16, split);
        }
        let value = hex_in_namespace(&mut rng, 32, split);
        lines.push((stratified_depth(i, k), needle_line(&key, &value)));
        queries.push(Task2Query::ValueOf {
            key: key.clone(),
            answer: value,
        });
        keys.push(key);
    }
    let mut absent: Vec<Vec<u8>> = Vec::with_capacity(k / 2);
    while absent.len() < k / 2 {
        let key = hex_in_namespace(&mut rng, 16, split);
        if !keys.contains(&key) && !absent.contains(&key) {
            absent.push(key);
        }
    }
    for key in keys.iter().take(k - k / 2) {
        queries.push(Task2Query::IsPresent {
            key: key.clone(),
            present: true,
        });
    }
    let (haystack, insertions) = splice_haystack(source, seed, split, span, lines)?;
    let absent_refs: Vec<&[u8]> = absent.iter().map(Vec::as_slice).collect();
    ensure_absent_from_filler(&haystack, &absent_refs)?;
    for key in absent {
        queries.push(Task2Query::IsPresent {
            key,
            present: false,
        });
    }
    Ok(Task2Instance {
        seed,
        split,
        span,
        k,
        haystack,
        queries,
        insertions,
    })
}

/// `LINK=<16 hex> NEXT=<16 hex>\n`, the first hop of task 3.
#[must_use]
pub fn link_line(from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut line = Vec::with_capacity(5 + from.len() + 6 + to.len() + 1);
    line.extend_from_slice(b"LINK=");
    line.extend_from_slice(from);
    line.extend_from_slice(b" NEXT=");
    line.extend_from_slice(to);
    line.push(b'\n');
    line
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task3Instance {
    pub seed: u64,
    pub split: B1Split,
    pub span: u64,
    pub d1_permille: u16,
    pub d2_permille: u16,
    pub haystack: Vec<u8>,
    pub start_key: Vec<u8>,
    pub answer: Vec<u8>,
    pub insertions: Vec<Insertion>,
}

/// Frozen depth pairs with |d1 − d2| ≥ 500 permille (prereg: hops ≥ L/2 apart).
#[must_use]
pub fn task3_depth_pairs() -> Vec<(u16, u16)> {
    let mut pairs = Vec::new();
    for d1 in TASK1_DEPTHS_PERMILLE {
        for d2 in TASK1_DEPTHS_PERMILLE {
            if d1.abs_diff(d2) >= 500 {
                pairs.push((d1, d2));
            }
        }
    }
    pairs
}

/// Generates one task-3 instance: `LINK=A NEXT=B` at d1, `KEY=B VALUE=C` at d2,
/// plus one distractor chain whose keys share the first 8 hex digits of A and B.
pub fn generate_task3(
    source: &dyn B1FillerSource,
    seed: u64,
    span: u64,
    instance: u64,
) -> Result<Task3Instance, B1Error> {
    let split = B1Split::for_seed(seed)?;
    let pairs = task3_depth_pairs();
    let mut rng = B1Rng::new(seed ^ (3 << 56) ^ instance);
    let (d1, d2) = pairs[rng.below(pairs.len() as u64) as usize];
    let a = hex_in_namespace(&mut rng, 16, split);
    let b = hex_in_namespace(&mut rng, 16, split);
    let c = hex_in_namespace(&mut rng, 32, split);
    let mut a2 = a[..8].to_vec();
    a2.extend(hex_in_namespace(&mut rng, 8, split));
    let mut b2 = b[..8].to_vec();
    b2.extend(hex_in_namespace(&mut rng, 8, split));
    if a2 == a || b2 == b {
        return Err(B1Error::AccountingInvariant(
            "distractor chain collided with target chain",
        ));
    }
    let c2 = hex_in_namespace(&mut rng, 32, split);
    let lines = vec![
        (d1, link_line(&a, &b)),
        (d2, needle_line(&b, &c)),
        (d2, link_line(&a2, &b2)),
        (d1, needle_line(&b2, &c2)),
    ];
    let (haystack, insertions) = splice_haystack(source, seed, split, span, lines)?;
    Ok(Task3Instance {
        seed,
        split,
        span,
        d1_permille: d1,
        d2_permille: d2,
        haystack,
        start_key: a,
        answer: c,
        insertions,
    })
}

/// Frozen gate thresholds (prereg §4) as exact rationals (numerator, denominator).
pub const H1_MEAN_MIN: (u64, u64) = (95, 100);
pub const H1_BUCKET_MIN: (u64, u64) = (85, 100);
pub const H2_F1_MIN: (u64, u64) = (90, 100);
pub const H2_FPR_MAX: (u64, u64) = (2, 100);
pub const H3_ACC_MIN: (u64, u64) = (80, 100);
pub const H4_EXPONENT_MAX: (u64, u64) = (75, 100);

/// Source-exact scoring: byte-for-byte equality, no normalisation.
#[must_use]
pub fn exact_match(predicted: Option<&[u8]>, gold: &[u8]) -> bool {
    predicted == Some(gold)
}

/// Per-depth exact-match counts for H1.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct H1Tally {
    /// depth_permille -> (correct, total)
    pub buckets: std::collections::BTreeMap<u16, (u64, u64)>,
}

impl H1Tally {
    pub fn record(&mut self, depth_permille: u16, correct: bool) {
        let e = self.buckets.entry(depth_permille).or_insert((0, 0));
        e.0 += u64::from(correct);
        e.1 += 1;
    }

    /// Mean over depth buckets of bucket accuracy, as f64 for reports only.
    #[must_use]
    pub fn mean_over_depths(&self) -> Option<f64> {
        if self.buckets.is_empty() {
            return None;
        }
        let sum: f64 = self
            .buckets
            .values()
            .map(|(c, t)| *c as f64 / *t as f64)
            .sum();
        Some(sum / self.buckets.len() as f64)
    }

    /// H1 verdict: mean ≥ 0.95 and every bucket ≥ 0.85; `None` with no data.
    #[must_use]
    pub fn passes(&self) -> Option<bool> {
        if self.buckets.is_empty() {
            return None;
        }
        let n = self.buckets.len() as u128;
        let bucket_ok = self.buckets.values().all(|(c, t)| {
            u128::from(*c) * u128::from(H1_BUCKET_MIN.1)
                >= u128::from(*t) * u128::from(H1_BUCKET_MIN.0)
        });
        // mean_i(c_i/t_i) >= p/q  <=>  sum_i c_i * (L/t_i) * q >= p * n * L, L = prod t_i.
        let l: u128 = self.buckets.values().map(|(_, t)| u128::from(*t)).product();
        let lhs: u128 = self
            .buckets
            .values()
            .map(|(c, t)| u128::from(*c) * (l / u128::from(*t)))
            .sum::<u128>()
            * u128::from(H1_MEAN_MIN.1);
        let rhs = u128::from(H1_MEAN_MIN.0) * n * l;
        Some(bucket_ok && lhs >= rhs)
    }
}

/// Binary confusion counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Confusion {
    pub tp: u64,
    pub fp: u64,
    pub tn: u64,
    pub fn_: u64,
}

impl Confusion {
    pub fn record(&mut self, predicted: bool, gold: bool) {
        match (predicted, gold) {
            (true, true) => self.tp += 1,
            (true, false) => self.fp += 1,
            (false, false) => self.tn += 1,
            (false, true) => self.fn_ += 1,
        }
    }

    /// F1 = 2tp / (2tp + fp + fn); `None` when undefined.
    #[must_use]
    pub fn f1(&self) -> Option<f64> {
        let d = 2 * self.tp + self.fp + self.fn_;
        (d > 0).then(|| (2 * self.tp) as f64 / d as f64)
    }

    /// FPR on membership negatives = fp / (fp + tn); `None` with no negatives.
    #[must_use]
    pub fn fpr(&self) -> Option<f64> {
        let d = self.fp + self.tn;
        (d > 0).then(|| self.fp as f64 / d as f64)
    }
}

/// Task-2 scorer per instance: a value query is a positive answered correctly
/// (tp) or not (fn; a wrong value also counts fp); membership feeds `membership`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct H2Tally {
    /// k -> confusion over value + present-membership answers
    pub per_k: std::collections::BTreeMap<usize, Confusion>,
    /// absent-key membership negatives across all k
    pub membership_negatives: Confusion,
}

/// Arm answer for one task-2 query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Task2Answer {
    Value(Option<Vec<u8>>),
    Present(bool),
}

impl H2Tally {
    pub fn record(&mut self, k: usize, query: &Task2Query, answer: &Task2Answer) {
        let c = self.per_k.entry(k).or_default();
        match (query, answer) {
            (Task2Query::ValueOf { answer: gold, .. }, Task2Answer::Value(pred)) => match pred {
                Some(p) if p == gold => c.tp += 1,
                Some(_) => {
                    c.fn_ += 1;
                    c.fp += 1;
                }
                None => c.fn_ += 1,
            },
            (Task2Query::IsPresent { present: true, .. }, Task2Answer::Present(p)) => {
                c.record(*p, true);
            }
            (Task2Query::IsPresent { present: false, .. }, Task2Answer::Present(p)) => {
                self.membership_negatives.record(*p, false);
            }
            // Mismatched answer kind counts as a miss on a positive / fp on a negative.
            (Task2Query::IsPresent { present: false, .. }, Task2Answer::Value(_)) => {
                self.membership_negatives.record(true, false);
            }
            (_, _) => c.fn_ += 1,
        }
    }

    /// Macro-F1 over k values present in the tally.
    #[must_use]
    pub fn macro_f1(&self) -> Option<f64> {
        let f: Vec<f64> = self.per_k.values().filter_map(Confusion::f1).collect();
        (!f.is_empty()).then(|| f.iter().sum::<f64>() / f.len() as f64)
    }
}

/// Task-3 scorer: exact match on the final value C.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct H3Tally {
    pub correct: u64,
    pub total: u64,
}

impl H3Tally {
    pub fn record(&mut self, correct: bool) {
        self.correct += u64::from(correct);
        self.total += 1;
    }

    #[must_use]
    pub fn passes(&self) -> Option<bool> {
        (self.total > 0).then(|| {
            u128::from(self.correct) * u128::from(H3_ACC_MIN.1)
                >= u128::from(self.total) * u128::from(H3_ACC_MIN.0)
        })
    }
}

/// Frozen caps (prereg §8).
pub const CAP_QUERY_WALL_CLOCK_MS: u64 = 60_000;
pub const CAP_PROCESS_RSS_BYTES: u64 = 16 * 1024 * 1024 * 1024;
pub const CAP_RUNG_WALL_CLOCK_MS: u64 = 6 * 60 * 60 * 1000;

/// Arms and baselines (prereg §6–§7). `BHybCompetitor` is a competitor only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum B1ArmId {
    SmlCtxRawScan,
    SmlCtxSbgMorTopK,
    SmlCtxRecurrent,
    SmlCtxCompacted,
    BLex,
    BAttn,
    BHybCompetitor,
}

/// Fail-closed cell outcome (prereg §8): no silent retry with a larger budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellOutcome {
    Completed,
    BudgetFail(&'static str),
    ProvenanceMismatch,
    HashMismatch,
    Oom,
    Timeout,
    NotApplicable(&'static str),
}

/// Per-cell accounting record; fields mirror prereg §8 exactly.
/// `None` = not measured by this harness (e.g. RSS without a host probe).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellRecord {
    pub tokenizer: &'static str,
    pub rung: u64,
    pub arm: B1ArmId,
    pub seed: u64,
    pub split: B1Split,
    pub filler_id: String,
    pub filler_sha256: Option<ContextContentHash>,
    pub bytes_ingested: u64,
    pub byte_tokens: u64,
    pub resident_c0_bytes: Option<u64>,
    pub segments_shortlisted: u64,
    pub segments_decoded: u64,
    pub raw_bytes_reread: u64,
    pub rss_peak_bytes: Option<u64>,
    pub mapped_bytes: Option<u64>,
    pub disk_bytes: Option<u64>,
    pub vram_peak_bytes: Option<u64>,
    pub latency_p50_us: Option<u64>,
    pub latency_p95_us: Option<u64>,
    pub latency_p99_us: Option<u64>,
    pub host: String,
    pub commit_sha: String,
    pub outcome: CellOutcome,
}

impl CellRecord {
    /// Structural invariants that hold for fixtures and campaigns alike.
    pub fn validate(&self) -> Result<(), B1Error> {
        if self.tokenizer != SML_BYTE_V0 {
            return Err(B1Error::AccountingInvariant(
                "tokenizer must be sml-byte-v0",
            ));
        }
        if self.byte_tokens != self.bytes_ingested {
            return Err(B1Error::AccountingInvariant(
                "sml-byte-v0 requires byte_tokens == bytes_ingested",
            ));
        }
        if self.segments_shortlisted < self.segments_decoded {
            return Err(B1Error::AccountingInvariant("shortlisted >= decoded"));
        }
        if B1Split::for_seed(self.seed)? != self.split {
            return Err(B1Error::AccountingInvariant("split does not match seed"));
        }
        if let (Some(p50), Some(p95), Some(p99)) = (
            self.latency_p50_us,
            self.latency_p95_us,
            self.latency_p99_us,
        ) {
            if !(p50 <= p95 && p95 <= p99) {
                return Err(B1Error::AccountingInvariant("p50 <= p95 <= p99"));
            }
        }
        Ok(())
    }

    /// Campaign admission: structural validity, ladder rung and a pinned filler.
    pub fn campaign_admissible(&self) -> Result<(), B1Error> {
        self.validate()?;
        validate_span(self.rung)?;
        if self.filler_sha256.is_none() {
            return Err(B1Error::FillerNotPinned);
        }
        Ok(())
    }
}

/// Applies frozen caps to observed values; any excess is `budget-fail`.
#[must_use]
pub fn apply_caps(max_query_ms: u64, rss_peak_bytes: Option<u64>) -> CellOutcome {
    if max_query_ms > CAP_QUERY_WALL_CLOCK_MS {
        return CellOutcome::BudgetFail("per-query wall clock > 60 s");
    }
    if rss_peak_bytes.is_some_and(|r| r > CAP_PROCESS_RSS_BYTES) {
        return CellOutcome::BudgetFail("process RSS > 16 GiB");
    }
    CellOutcome::Completed
}

/// Arm hook: later PRs implement this for CONTEXT-3, B-lex and B-attn.
pub trait B1Arm {
    fn id(&self) -> B1ArmId;
    /// Exact value bytes for `key`, or `None` to abstain.
    fn lookup_value(&mut self, haystack: &[u8], key: &[u8]) -> Option<Vec<u8>>;
    /// Membership answer for `key`.
    fn contains_key(&mut self, haystack: &[u8], key: &[u8]) -> bool;
    /// `NEXT=` target of `LINK=<from>`, or `None`.
    fn follow_link(&mut self, haystack: &[u8], from: &[u8]) -> Option<Vec<u8>>;
}

/// Harness smoke arm: exhaustive exact scan. Not a B1 result (prereg §6).
#[derive(Debug, Default)]
pub struct ExactScanSmokeArm;

fn value_after(haystack: &[u8], prefix: &[u8], width: usize) -> Option<Vec<u8>> {
    let at = find(haystack, prefix)? + prefix.len();
    haystack.get(at..at + width).map(<[u8]>::to_vec)
}

impl B1Arm for ExactScanSmokeArm {
    fn id(&self) -> B1ArmId {
        B1ArmId::SmlCtxRawScan
    }

    fn lookup_value(&mut self, haystack: &[u8], key: &[u8]) -> Option<Vec<u8>> {
        let mut prefix = b"KEY=".to_vec();
        prefix.extend_from_slice(key);
        prefix.extend_from_slice(b" VALUE=");
        value_after(haystack, &prefix, 32)
    }

    fn contains_key(&mut self, haystack: &[u8], key: &[u8]) -> bool {
        let mut prefix = b"KEY=".to_vec();
        prefix.extend_from_slice(key);
        prefix.push(b' ');
        find(haystack, &prefix).is_some()
    }

    fn follow_link(&mut self, haystack: &[u8], from: &[u8]) -> Option<Vec<u8>> {
        let mut prefix = b"LINK=".to_vec();
        prefix.extend_from_slice(from);
        prefix.extend_from_slice(b" NEXT=");
        value_after(haystack, &prefix, 16)
    }
}
