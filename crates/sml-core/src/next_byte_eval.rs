//! Pre-registered next-byte evaluation harness with independent baselines.
//!
//! Protocol: `docs/SML-INTEGRATED-LOOP-EVAL-PREREGISTRATION.md`. Every arm
//! (the frozen Q4 atlas, TRAIN-only count baselines and the future integrated
//! SBG -> MOR -> memory -> recurrence -> generation loop) is scored through the
//! same [`NextBytePredictor`] trait, on the same byte positions, by the same
//! [`NextByteEvaluator`]. This module is read-only with respect to any model:
//! it never trains a learned atlas and never writes files.
//!
//! Nothing here establishes language-model quality, a trained router, or a
//! 25B model. Numbers only exist once the CLI has been run on frozen inputs.
use crate::{
    byte_factorization::ByteFactorization,
    elastic_ngram_paged_sha::{hex, parse_hex, sha256, Sha256},
    sparse_gradient_atlas::QuantizedSparseAtlas,
};
use std::{
    collections::HashSet,
    error::Error,
    fs::{self, File},
    io::{BufReader, Read},
    path::{Component, Path, PathBuf},
    time::Instant,
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Bytes of left context before the first scored position of a document.
/// Matches `QuantizedSparseAtlas::evaluate_document[_grouped]`, whose events
/// are `document.len().saturating_sub(2)`.
pub const FIRST_SCORED_POSITION: usize = 2;
/// Fixed a priori Dirichlet back-off strength of the count baselines. It is
/// NOT tuned on DEV (or on anything else).
pub const BACKOFF_BETA: f64 = 1.0;
/// Tolerance for `sum_c 2^log2 P(c | h) == 1` checks in the evaluator.
pub const NORMALIZATION_TOLERANCE: f64 = 1e-9;

/// One next-byte model under evaluation.
///
/// Call contract enforced by [`NextByteEvaluator`]:
/// 1. [`NextBytePredictor::begin_document`] is called once before the first
///    position of every document, and must clear any recurrent state.
/// 2. Positions of a document are visited strictly left to right, starting at
///    [`FIRST_SCORED_POSITION`]. For position `i` the evaluator passes
///    `history = &document[..i]`, so between consecutive calls the history
///    grows by exactly one byte. A stateful predictor (recurrence, memory) may
///    therefore update itself from `history.last()` only.
/// 3. For each position, [`NextBytePredictor::top1`] (when enabled) is called
///    before [`NextBytePredictor::log2_probability`], with the same history.
///    The target byte is never visible to `top1`.
/// 4. A predictor must not learn from evaluation documents. Count baselines
///    enforce this by freezing at the first `begin_document`.
pub trait NextBytePredictor {
    /// Stable arm identifier written into reports (e.g. `q4_atlas_s2a`).
    fn arm(&self) -> &str;
    /// Start a new, independent document.
    fn begin_document(&mut self) -> Result<()>;
    /// `log2 P(next | history)`; must be finite and `<= 0`.
    fn log2_probability(&mut self, history: &[u8], next: u8) -> Result<f64>;
    /// Deterministic maximum-probability next byte, lowest byte on ties.
    fn top1(&mut self, history: &[u8]) -> Result<u8>;
    /// Cumulative model pages loaded from disk (0 for in-RAM baselines).
    fn page_loads(&self) -> u64 {
        0
    }
}

/// Report of one arm, produced by [`NextByteEvaluator::finish`].
#[derive(Clone, Debug, PartialEq)]
pub struct ArmReport {
    pub arm: String,
    pub documents: u64,
    pub scored_bytes: u64,
    /// Sum of `-log2 P(target)` over scored bytes.
    pub total_loss_bits: f64,
    pub top1_scored: bool,
    pub top1_correct: u64,
    /// SHA-256 over the emitted top-1 bytes, in evaluation order (hex), or
    /// the SHA-256 of the empty string when top-1 is disabled.
    pub top1_stream_sha256: String,
    /// Wall time spent inside `log2_probability` calls.
    pub probability_ns: u128,
    /// Wall time spent inside `top1` calls.
    pub top1_ns: u128,
    /// Pages loaded during this evaluation (difference of cumulative counter).
    pub page_loads: u64,
}

impl ArmReport {
    #[must_use]
    pub fn bits_per_byte(&self) -> Option<f64> {
        (self.scored_bytes > 0).then(|| self.total_loss_bits / self.scored_bytes as f64)
    }

    #[must_use]
    pub fn top1_accuracy(&self) -> Option<f64> {
        (self.top1_scored && self.scored_bytes > 0)
            .then(|| self.top1_correct as f64 / self.scored_bytes as f64)
    }

    #[must_use]
    pub fn probability_ns_per_byte(&self) -> Option<f64> {
        (self.scored_bytes > 0).then(|| self.probability_ns as f64 / self.scored_bytes as f64)
    }

    #[must_use]
    pub fn top1_ns_per_byte(&self) -> Option<f64> {
        (self.top1_scored && self.scored_bytes > 0)
            .then(|| self.top1_ns as f64 / self.scored_bytes as f64)
    }

    /// Tab-separated header matching [`ArmReport::tsv_row`].
    #[must_use]
    pub fn tsv_header() -> &'static str {
        "arm\tdocuments\tscored_bytes\ttotal_loss_bits\tbits_per_byte\ttop1_correct\ttop1_accuracy\ttop1_stream_sha256\tprobability_ns_per_byte\ttop1_ns_per_byte\tpage_loads"
    }

    #[must_use]
    pub fn tsv_row(&self) -> String {
        fn opt(value: Option<f64>) -> String {
            value.map_or_else(|| "NA".to_string(), |v| format!("{v:.9}"))
        }
        format!(
            "{}\t{}\t{}\t{:.6}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.arm,
            self.documents,
            self.scored_bytes,
            self.total_loss_bits,
            opt(self.bits_per_byte()),
            if self.top1_scored {
                self.top1_correct.to_string()
            } else {
                "NA".to_string()
            },
            opt(self.top1_accuracy()),
            self.top1_stream_sha256,
            opt(self.probability_ns_per_byte()),
            opt(self.top1_ns_per_byte()),
            self.page_loads,
        )
    }
}

/// Accumulates one arm over a stream of documents.
pub struct NextByteEvaluator {
    arm: String,
    score_top1: bool,
    documents: u64,
    scored_bytes: u64,
    total_loss_bits: f64,
    top1_correct: u64,
    top1_hash: Sha256,
    probability_ns: u128,
    top1_ns: u128,
    page_loads_start: u64,
}

impl NextByteEvaluator {
    pub fn new<P: NextBytePredictor + ?Sized>(predictor: &P, score_top1: bool) -> Self {
        Self {
            arm: predictor.arm().to_string(),
            score_top1,
            documents: 0,
            scored_bytes: 0,
            total_loss_bits: 0.0,
            top1_correct: 0,
            top1_hash: Sha256::default(),
            probability_ns: 0,
            top1_ns: 0,
            page_loads_start: predictor.page_loads(),
        }
    }

    /// Score every position `i >= FIRST_SCORED_POSITION` of one document.
    pub fn score_document<P: NextBytePredictor + ?Sized>(
        &mut self,
        predictor: &mut P,
        document: &[u8],
    ) -> Result<()> {
        if predictor.arm() != self.arm {
            return Err("evaluator fed a different predictor arm".into());
        }
        predictor.begin_document()?;
        for position in FIRST_SCORED_POSITION..document.len() {
            let history = &document[..position];
            let target = document[position];
            if self.score_top1 {
                let started = Instant::now();
                let guess = predictor.top1(history)?;
                self.top1_ns += started.elapsed().as_nanos();
                self.top1_hash.update(&[guess]);
                if guess == target {
                    self.top1_correct += 1;
                }
            }
            let started = Instant::now();
            let log2_p = predictor.log2_probability(history, target)?;
            self.probability_ns += started.elapsed().as_nanos();
            if !log2_p.is_finite() || log2_p > 1e-12 {
                return Err("predictor returned a nonfinite or positive log2 probability".into());
            }
            self.total_loss_bits -= log2_p;
            self.scored_bytes = self
                .scored_bytes
                .checked_add(1)
                .ok_or("scored byte overflow")?;
        }
        if !self.total_loss_bits.is_finite() {
            return Err("nonfinite accumulated loss".into());
        }
        self.documents += 1;
        Ok(())
    }

    pub fn finish<P: NextBytePredictor + ?Sized>(self, predictor: &P) -> Result<ArmReport> {
        let page_loads = predictor
            .page_loads()
            .checked_sub(self.page_loads_start)
            .ok_or("predictor page-load counter decreased")?;
        Ok(ArmReport {
            arm: self.arm,
            documents: self.documents,
            scored_bytes: self.scored_bytes,
            total_loss_bits: self.total_loss_bits,
            top1_scored: self.score_top1,
            top1_correct: self.top1_correct,
            top1_stream_sha256: hex(&self.top1_hash.finish()),
            probability_ns: self.probability_ns,
            top1_ns: self.top1_ns,
            page_loads,
        })
    }
}

/// Convenience wrapper: score all `documents` with a fresh evaluator.
pub fn evaluate_documents<P: NextBytePredictor + ?Sized>(
    predictor: &mut P,
    documents: &[&[u8]],
    score_top1: bool,
) -> Result<ArmReport> {
    let mut evaluator = NextByteEvaluator::new(predictor, score_top1);
    for document in documents {
        evaluator.score_document(predictor, document)?;
    }
    evaluator.finish(predictor)
}

/// Checks that a predictor's distribution for `history` sums to one. Used by
/// tests and by the CLI's pre-run sanity probe; not part of scoring.
pub fn check_normalized<P: NextBytePredictor + ?Sized>(
    predictor: &mut P,
    history: &[u8],
) -> Result<f64> {
    let mut mass = 0.0f64;
    for byte in 0..=u8::MAX {
        let log2_p = predictor.log2_probability(history, byte)?;
        if !log2_p.is_finite() || log2_p > 1e-12 {
            return Err("nonfinite or positive log2 probability".into());
        }
        mass += log2_p.exp2();
    }
    if (mass - 1.0).abs() > NORMALIZATION_TOLERANCE {
        return Err(format!("distribution mass {mass} is not 1").into());
    }
    Ok(mass)
}

/// The reproduction gate of the pre-registration: the measured value printed
/// with nine decimals (as `metrics.tsv` does) must equal `expected` exactly.
#[must_use]
pub fn reproduces_exactly(measured_bits_per_byte: f64, expected: &str) -> bool {
    measured_bits_per_byte.is_finite() && format!("{measured_bits_per_byte:.9}") == expected
}

fn argmax_lowest(log2_probabilities: impl Iterator<Item = (u8, f64)>) -> Result<u8> {
    let mut best = None::<(u8, f64)>;
    for (byte, value) in log2_probabilities {
        if !value.is_finite() {
            return Err("nonfinite score in top-1 search".into());
        }
        // Strict comparison: on equal scores the lowest byte stays selected.
        if best.is_none_or(|(_, score)| value > score) {
            best = Some((byte, value));
        }
    }
    best.map(|(byte, _)| byte)
        .ok_or("empty top-1 search".into())
}

fn last_two(history: &[u8]) -> Result<(u8, u8)> {
    match history {
        [.., a, b] => Ok((*a, *b)),
        _ => Err("two-byte context predictor needs at least two history bytes".into()),
    }
}

/// The frozen Q4 atlas through its public API only: per-head probabilities
/// from [`QuantizedSparseAtlas::probability`] and the whole-byte MAP decoder
/// [`QuantizedSparseAtlas::predict_next_byte`]. The raw Q4 file has no header,
/// so the caller supplies the factorization of the exporting checkpoint.
pub struct Q4AtlasPredictor {
    arm: String,
    atlas: QuantizedSparseAtlas,
    factorization: ByteFactorization,
}

impl Q4AtlasPredictor {
    pub fn new(
        arm: impl Into<String>,
        atlas: QuantizedSparseAtlas,
        factorization: ByteFactorization,
    ) -> Self {
        Self {
            arm: arm.into(),
            atlas,
            factorization,
        }
    }

    pub fn atlas_mut(&mut self) -> &mut QuantizedSparseAtlas {
        &mut self.atlas
    }

    #[must_use]
    pub fn into_inner(self) -> QuantizedSparseAtlas {
        self.atlas
    }
}

impl NextBytePredictor for Q4AtlasPredictor {
    fn arm(&self) -> &str {
        &self.arm
    }

    fn begin_document(&mut self) -> Result<()> {
        Ok(())
    }

    fn log2_probability(&mut self, history: &[u8], next: u8) -> Result<f64> {
        let (a, b) = last_two(history)?;
        let mut log2_p = 0.0f64;
        for (head, label) in self.factorization.decisions(next) {
            let p = self.atlas.probability(a, b, head)?;
            log2_p += if label { p.log2() } else { (1.0 - p).log2() };
        }
        Ok(log2_p)
    }

    fn top1(&mut self, history: &[u8]) -> Result<u8> {
        let (a, b) = last_two(history)?;
        Ok(self.atlas.predict_next_byte(a, b)?.byte)
    }

    fn page_loads(&self) -> u64 {
        self.atlas.page_loads()
    }
}

/// Add-one unigram over the 256 byte values, fitted on scored TRAIN
/// positions only and frozen at the first evaluation document.
#[derive(Clone, Debug)]
pub struct UnigramCounts {
    counts: [u64; 256],
    total: u64,
    frozen: bool,
}

impl Default for UnigramCounts {
    fn default() -> Self {
        Self {
            counts: [0; 256],
            total: 0,
            frozen: false,
        }
    }
}

impl UnigramCounts {
    /// Count every byte at positions `>= FIRST_SCORED_POSITION`.
    pub fn fit_document(&mut self, document: &[u8]) -> Result<()> {
        if self.frozen {
            return Err("count baseline is frozen: no fitting after evaluation began".into());
        }
        for &byte in document.iter().skip(FIRST_SCORED_POSITION) {
            self.counts[usize::from(byte)] += 1;
            self.total = self.total.checked_add(1).ok_or("unigram overflow")?;
        }
        Ok(())
    }

    #[must_use]
    pub fn probability(&self, byte: u8) -> f64 {
        (self.counts[usize::from(byte)] as f64 + 1.0) / (self.total as f64 + 256.0)
    }

    #[must_use]
    pub fn total(&self) -> u64 {
        self.total
    }
}

impl NextBytePredictor for UnigramCounts {
    fn arm(&self) -> &str {
        "unigram_add1_train"
    }

    fn begin_document(&mut self) -> Result<()> {
        self.frozen = true;
        Ok(())
    }

    fn log2_probability(&mut self, _history: &[u8], next: u8) -> Result<f64> {
        Ok(self.probability(next).log2())
    }

    fn top1(&mut self, _history: &[u8]) -> Result<u8> {
        argmax_lowest((0..=u8::MAX).map(|b| (b, self.counts[usize::from(b)] as f64)))
    }
}

/// Order-2 (two previous bytes, same context as the atlas) count model with
/// fixed Dirichlet back-off `BACKOFF_BETA`:
///
/// `P2(c|ab) = (n(abc) + beta P1(c|b)) / (n(ab.) + beta)`,
/// `P1(c|b) = (n(bc) + beta P0(c)) / (n(b.) + beta)`, `P0` = add-one unigram.
///
/// All counts come from scored TRAIN positions only.
pub struct Order2Counts {
    trigram: Vec<u32>,
    context2: Vec<u64>,
    bigram: Vec<u64>,
    context1: Vec<u64>,
    unigram: UnigramCounts,
    frozen: bool,
}

impl Default for Order2Counts {
    fn default() -> Self {
        Self {
            trigram: vec![0; 1 << 24],
            context2: vec![0; 1 << 16],
            bigram: vec![0; 1 << 16],
            context1: vec![0; 256],
            unigram: UnigramCounts::default(),
            frozen: false,
        }
    }
}

impl Order2Counts {
    pub fn fit_document(&mut self, document: &[u8]) -> Result<()> {
        if self.frozen {
            return Err("count baseline is frozen: no fitting after evaluation began".into());
        }
        self.unigram.fit_document(document)?;
        for window in document.windows(3) {
            let (a, b, c) = (
                usize::from(window[0]),
                usize::from(window[1]),
                usize::from(window[2]),
            );
            let ab = (a << 8) | b;
            let slot = &mut self.trigram[(ab << 8) | c];
            *slot = slot.checked_add(1).ok_or("trigram count overflow")?;
            self.context2[ab] += 1;
            self.bigram[(b << 8) | c] += 1;
            self.context1[b] += 1;
        }
        Ok(())
    }

    #[must_use]
    pub fn probability(&self, a: u8, b: u8, c: u8) -> f64 {
        let (a, b, cu) = (usize::from(a), usize::from(b), usize::from(c));
        let p0 = self.unigram.probability(c);
        let p1 = (self.bigram[(b << 8) | cu] as f64 + BACKOFF_BETA * p0)
            / (self.context1[b] as f64 + BACKOFF_BETA);
        let ab = (a << 8) | b;
        (f64::from(self.trigram[(ab << 8) | cu]) + BACKOFF_BETA * p1)
            / (self.context2[ab] as f64 + BACKOFF_BETA)
    }

    #[must_use]
    pub fn fitted_events(&self) -> u64 {
        self.unigram.total()
    }
}

impl NextBytePredictor for Order2Counts {
    fn arm(&self) -> &str {
        "order2_dirichlet_backoff_train"
    }

    fn begin_document(&mut self) -> Result<()> {
        self.frozen = true;
        self.unigram.frozen = true;
        Ok(())
    }

    fn log2_probability(&mut self, history: &[u8], next: u8) -> Result<f64> {
        let (a, b) = last_two(history)?;
        Ok(self.probability(a, b, next).log2())
    }

    fn top1(&mut self, history: &[u8]) -> Result<u8> {
        let (a, b) = last_two(history)?;
        argmax_lowest((0..=u8::MAX).map(|c| (c, self.probability(a, b, c))))
    }
}

/// Lets an owned or borrowed predictor be an expert of a mixture.
impl<P: NextBytePredictor + ?Sized> NextBytePredictor for &mut P {
    fn arm(&self) -> &str {
        (**self).arm()
    }

    fn begin_document(&mut self) -> Result<()> {
        (**self).begin_document()
    }

    fn log2_probability(&mut self, history: &[u8], next: u8) -> Result<f64> {
        (**self).log2_probability(history, next)
    }

    fn top1(&mut self, history: &[u8]) -> Result<u8> {
        (**self).top1(history)
    }

    fn page_loads(&self) -> u64 {
        (**self).page_loads()
    }
}

/// Fixed a priori weight of the first expert in [`LinearMixture`] (NBE-2
/// arm `mix_q4_order2_linear_half`). NOT tuned on DEV or anything else.
pub const LINEAR_MIXTURE_WEIGHT: f64 = 0.5;

/// `log2(2^x + 2^y)` without overflow or underflow.
fn log2_add(x: f64, y: f64) -> f64 {
    let (hi, lo) = if x >= y { (x, y) } else { (y, x) };
    hi + (lo - hi).exp2().ln_1p() / std::f64::consts::LN_2
}

/// `P(c|h) = w P_a(c|h) + (1 - w) P_b(c|h)` with a constant weight `w`.
///
/// By concavity of `log2`, its total loss can never exceed the mean of the two
/// experts' losses on the same positions (NBE-2 gate M1).
pub struct LinearMixture<A, B> {
    arm: String,
    a: A,
    b: B,
    log2_wa: f64,
    log2_wb: f64,
}

impl<A: NextBytePredictor, B: NextBytePredictor> LinearMixture<A, B> {
    pub fn new(arm: impl Into<String>, a: A, b: B, weight_a: f64) -> Result<Self> {
        if !(weight_a > 0.0 && weight_a < 1.0) {
            return Err("linear mixture weight must lie strictly inside (0, 1)".into());
        }
        Ok(Self {
            arm: arm.into(),
            a,
            b,
            log2_wa: weight_a.log2(),
            log2_wb: (1.0 - weight_a).log2(),
        })
    }
}

impl<A: NextBytePredictor, B: NextBytePredictor> NextBytePredictor for LinearMixture<A, B> {
    fn arm(&self) -> &str {
        &self.arm
    }

    fn begin_document(&mut self) -> Result<()> {
        self.a.begin_document()?;
        self.b.begin_document()
    }

    fn log2_probability(&mut self, history: &[u8], next: u8) -> Result<f64> {
        let la = self.a.log2_probability(history, next)?;
        let lb = self.b.log2_probability(history, next)?;
        Ok(log2_add(self.log2_wa + la, self.log2_wb + lb).min(0.0))
    }

    /// Exact argmax over all 256 bytes (256 calls per expert; slow for Q4).
    fn top1(&mut self, history: &[u8]) -> Result<u8> {
        let mut scores = [0f64; 256];
        for (c, score) in scores.iter_mut().enumerate() {
            *score = self.log2_probability(history, c as u8)?;
        }
        argmax_lowest(scores.iter().enumerate().map(|(c, s)| (c as u8, *s)))
    }

    fn page_loads(&self) -> u64 {
        self.a.page_loads() + self.b.page_loads()
    }
}

/// Per-document causal Bayesian model average of two experts (NBE-2 arm
/// `mix_q4_order2_bayes_doc`). Prior 1/2 each, reset at every
/// `begin_document`; the posterior at position `i` uses only the already
/// revealed bytes `history[FIRST_SCORED_POSITION..i]`, never the target.
///
/// Its total loss over a document is at most `min(L_a, L_b) + 1` bit (prior
/// 1/2), so over `D` documents at most `min + D` bits (NBE-2 gate M2).
pub struct CausalBayesMixture<A, B> {
    arm: String,
    a: A,
    b: B,
    /// Cumulative log2 likelihood of each expert on revealed bytes.
    log2_la: f64,
    log2_lb: f64,
    consumed: usize,
    /// Expert log2 probabilities from the last `log2_probability` call:
    /// `(history length, byte, la, lb)`. Reused when that byte is revealed, so
    /// each expert sees exactly one probability call per position.
    pending: Option<(usize, u8, f64, f64)>,
}

impl<A: NextBytePredictor, B: NextBytePredictor> CausalBayesMixture<A, B> {
    pub fn new(arm: impl Into<String>, a: A, b: B) -> Self {
        Self {
            arm: arm.into(),
            a,
            b,
            log2_la: 0.0,
            log2_lb: 0.0,
            consumed: FIRST_SCORED_POSITION,
            pending: None,
        }
    }

    /// Current posterior weight of the first expert (for tests/diagnostics).
    #[must_use]
    pub fn weight_a(&self) -> f64 {
        let norm = log2_add(self.log2_la, self.log2_lb);
        (self.log2_la - norm).exp2()
    }

    fn sync(&mut self, history: &[u8]) -> Result<()> {
        if history.len() < self.consumed {
            return Err("bayes mixture: history shrank without begin_document".into());
        }
        while self.consumed < history.len() {
            let j = self.consumed;
            let t = history[j];
            let (la, lb) = match self.pending {
                Some((len, byte, la, lb)) if len == j && byte == t => (la, lb),
                _ => (
                    self.a.log2_probability(&history[..j], t)?,
                    self.b.log2_probability(&history[..j], t)?,
                ),
            };
            self.log2_la += la;
            self.log2_lb += lb;
            self.consumed += 1;
        }
        self.pending = None;
        Ok(())
    }

    fn log2_weights(&self) -> (f64, f64) {
        let norm = log2_add(self.log2_la, self.log2_lb);
        (self.log2_la - norm, self.log2_lb - norm)
    }
}

impl<A: NextBytePredictor, B: NextBytePredictor> NextBytePredictor for CausalBayesMixture<A, B> {
    fn arm(&self) -> &str {
        &self.arm
    }

    fn begin_document(&mut self) -> Result<()> {
        self.log2_la = 0.0;
        self.log2_lb = 0.0;
        self.consumed = FIRST_SCORED_POSITION;
        self.pending = None;
        self.a.begin_document()?;
        self.b.begin_document()
    }

    fn log2_probability(&mut self, history: &[u8], next: u8) -> Result<f64> {
        self.sync(history)?;
        let la = self.a.log2_probability(history, next)?;
        let lb = self.b.log2_probability(history, next)?;
        self.pending = Some((history.len(), next, la, lb));
        let (wa, wb) = self.log2_weights();
        Ok(log2_add(wa + la, wb + lb).min(0.0))
    }

    /// Exact argmax over all 256 bytes (256 calls per expert; slow for Q4).
    fn top1(&mut self, history: &[u8]) -> Result<u8> {
        self.sync(history)?;
        let (wa, wb) = self.log2_weights();
        let mut scores = [0f64; 256];
        for (c, score) in scores.iter_mut().enumerate() {
            let la = self.a.log2_probability(history, c as u8)?;
            let lb = self.b.log2_probability(history, c as u8)?;
            *score = log2_add(wa + la, wb + lb);
        }
        argmax_lowest(scores.iter().enumerate().map(|(c, s)| (c as u8, *s)))
    }

    fn page_loads(&self) -> u64 {
        self.a.page_loads() + self.b.page_loads()
    }
}

/// Role of a document in the frozen stream-fit manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManifestRole {
    Train,
    Dev,
}

#[derive(Clone, Debug)]
pub struct ManifestEntry {
    pub role: ManifestRole,
    pub path: PathBuf,
    pub length: u64,
    pub digest: [u8; 32],
}

/// Read-only view of a `sml-sparse-gradient-stream-fit` manifest
/// (`role\tpath\tbytes\tsha256\trights_review`, TRAIN rows then DEV rows).
/// Any other role (e.g. a TEST split) is rejected, so TEST stays unopened.
#[derive(Clone, Debug)]
pub struct FrozenManifest {
    pub manifest_sha256: String,
    pub entries: Vec<ManifestEntry>,
}

const MANIFEST_HEADER: &str = "role\tpath\tbytes\tsha256\trights_review";
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const REVIEWED_LABELS: [&str; 7] = [
    "OWNED_REVIEWED",
    "PUBLIC_DOMAIN_REVIEWED",
    "CC0_REVIEWED",
    "CC_BY_ATTRIBUTION_REVIEWED",
    "FR_OPEN_LICENSE_V2_ATTRIBUTION_REVIEWED",
    "APACHE_2_0_ATTRIBUTION_REVIEWED",
    "MIT_ATTRIBUTION_REVIEWED",
];

fn regular_file(path: &Path) -> Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("expected a regular non-symlink file".into());
    }
    Ok(meta)
}

/// Full-file SHA-256 (lowercase hex) of a regular non-symlink file.
pub fn file_sha256(path: &Path) -> Result<String> {
    regular_file(path)?;
    let mut reader = BufReader::new(File::open(path)?);
    let mut state = Sha256::default();
    let mut block = vec![0u8; 1024 * 1024];
    loop {
        let n = reader.read(&mut block)?;
        if n == 0 {
            break;
        }
        state.update(&block[..n]);
    }
    Ok(hex(&state.finish()))
}

impl FrozenManifest {
    /// Load and verify the manifest. When `pinned_sha256` is `Some`, the
    /// manifest bytes must hash to exactly that value.
    pub fn load(data_root: &Path, manifest: &Path, pinned_sha256: Option<&str>) -> Result<Self> {
        let root_meta = fs::symlink_metadata(data_root)?;
        if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
            return Err("data root must be a directory, not a symlink".into());
        }
        let root = fs::canonicalize(data_root)?;
        let info = regular_file(manifest)?;
        if info.len() == 0 || info.len() > MAX_MANIFEST_BYTES {
            return Err("manifest outside bounded size".into());
        }
        let contents = fs::read(manifest)?;
        let manifest_sha256 = hex(&sha256(&contents));
        if let Some(pin) = pinned_sha256 {
            if manifest_sha256 != pin {
                return Err("manifest SHA-256 differs from the pre-registered pin".into());
            }
        }
        let text = std::str::from_utf8(&contents)?;
        let mut lines = text.lines();
        if lines.next() != Some(MANIFEST_HEADER) {
            return Err("invalid document manifest header".into());
        }
        let mut entries = Vec::new();
        let mut seen_paths = HashSet::new();
        let mut seen_digests = HashSet::new();
        let mut dev_started = false;
        for line in lines {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() != 5 {
                return Err("invalid document manifest record".into());
            }
            let role = match fields[0] {
                "TRAIN" if !dev_started => ManifestRole::Train,
                "DEV" => {
                    dev_started = true;
                    ManifestRole::Dev
                }
                _ => return Err("invalid document role/order (TEST is never opened)".into()),
            };
            if !REVIEWED_LABELS.contains(&fields[4]) {
                return Err("document rights have not been declared reviewed".into());
            }
            let length: u64 = fields[2].parse()?;
            let digest = parse_hex(fields[3]).ok_or("invalid lowercase SHA256 field")?;
            if fields[3] != hex(&digest) {
                return Err("noncanonical SHA256 field".into());
            }
            let relative = fields[1];
            if relative.is_empty()
                || relative.len() > 1024
                || relative.chars().any(char::is_control)
                || !Path::new(relative)
                    .components()
                    .all(|c| matches!(c, Component::Normal(_)))
            {
                return Err("unsafe source-relative path".into());
            }
            let joined = root.join(relative);
            regular_file(&joined)?;
            let path = fs::canonicalize(&joined)?;
            if !path.starts_with(&root) {
                return Err("manifest source escaped the data directory".into());
            }
            if !seen_paths.insert(path.clone()) || !seen_digests.insert(digest) {
                return Err("duplicate document path or source digest".into());
            }
            entries.push(ManifestEntry {
                role,
                path,
                length,
                digest,
            });
        }
        Ok(Self {
            manifest_sha256,
            entries,
        })
    }

    pub fn role(&self, role: ManifestRole) -> impl Iterator<Item = &ManifestEntry> {
        self.entries.iter().filter(move |e| e.role == role)
    }
}

/// Read one manifest document and verify its length, SHA-256 and UTF-8.
pub fn load_verified_document(entry: &ManifestEntry) -> Result<Vec<u8>> {
    let info = regular_file(&entry.path)?;
    if info.len() != entry.length {
        return Err("document size differs from manifest".into());
    }
    let mut data = Vec::with_capacity(usize::try_from(entry.length)?);
    BufReader::new(File::open(&entry.path)?).read_to_end(&mut data)?;
    if data.len() as u64 != entry.length || sha256(&data) != entry.digest {
        return Err("document SHA-256 differs from manifest".into());
    }
    std::str::from_utf8(&data)?;
    Ok(data)
}
