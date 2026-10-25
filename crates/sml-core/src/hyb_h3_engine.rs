//! HYB H-3: the hybrid engine as a native lossless compressor.
//! Protocol: docs/HYB-H3-NATIVE-ENGINE-PREREGISTRATION.md.
//!
//! H-1d and H-2 judged heads on traces: P1 wrote its logits and bank
//! stretches to disk, and the heads were scored against them. Here the
//! engine runs end to end, decision by decision, the way a decoder must:
//! the frozen P1 count banks predict each bit from what has been decoded
//! so far (`Model::frozen_blocks`, `Model::mix`), an optional head
//! (a single H-1d head, or the H-2 router with its admitted experts) adds
//! an integer stretch correction, and the binary arithmetic coder of
//! `bool_coder` codes the bit with the resulting `u32` probability.
//!
//! Everything from the counts to the coder is integer arithmetic, so the
//! coded bytes are the same on every conforming host. `oracle_p1s`
//! recomputes the same probabilities the trace way (`code_document`,
//! scalar head oracle, scalar router oracle); a run checks the two agree
//! on every decision. Research only, NOT_PROMOTED.

use crate::{
    bool_coder::{BinaryDecoder, BinaryEncoder, BitPredictor, CoderError},
    boolean_associative_focus::AnfActivation,
    gb11_p1_stream::{ByteCode, Context, Model, PhaseBlocks, Tables, TraceRecord, MAX_BANKS},
    hyb_baf_neural_gate::{causal_features, hash_gate_is_active, oracle_causal_features},
    hyb_h1_residual::DEPTHS,
    hyb_h1b_calibrated::LAMBDA_EIGHTHS,
    hyb_h1d_bank_head::{BankHead, BankPlane, BankRecord, FusedBankHead, ROW},
    hyb_h1d_copy::{mix_probability, oracle_probability, CopyExpert, DEFAULT_COPY_WEIGHT_TENTHS},
    hyb_h1d_gaussian_copy::{mix_gaussian_probability, oracle_longest_match, GaussianCopyProfile},
    hyb_h2_mor_experts::{route_bits, Router, EXPERTS, ROUTE_WORDS},
};

/// Positions a P1 model does not predict (its witness starts at 2): the
/// engine codes them at probability 1/2.
pub const UNCODED_PREFIX: usize = 2;
const HALF: u32 = 1 << 31;

/// The neural part of the engine: no head (the Boolean engine alone), one
/// H-1d head, or the H-2 router with its experts and admission mask.
#[derive(Debug, Clone)]
pub struct HybridHead {
    baf_gate: Option<AnfActivation>,
    hash_gate_threshold: Option<u16>,
    router: Option<Router>,
    heads: Vec<BankHead>,
    fused: Vec<FusedBankHead>,
    admitted: Vec<bool>,
    eighths: i32,
    copy_weight_tenths: u8,
    gaussian_copy: Option<GaussianCopyProfile>,
}

impl HybridHead {
    /// P1 alone.
    #[must_use]
    pub fn none() -> Self {
        Self {
            router: None,
            baf_gate: None,
            hash_gate_threshold: None,
            heads: Vec::new(),
            fused: Vec::new(),
            admitted: Vec::new(),
            eighths: 0,
            copy_weight_tenths: 0,
            gaussian_copy: None,
        }
    }

    /// One H-1d head on every byte.
    #[must_use]
    pub fn single(head: BankHead, eighths: i32) -> Self {
        Self {
            router: None,
            baf_gate: None,
            hash_gate_threshold: None,
            fused: vec![FusedBankHead::new(&head)],
            heads: vec![head],
            admitted: vec![true],
            eighths,
            copy_weight_tenths: 0,
            gaussian_copy: None,
        }
    }

    /// A frozen H-1d head blended with a causally recurrent byte-copy expert.
    /// The original P1 and neural weights remain unchanged. w=0 retains
    /// byte-for-byte compatibility with the existing H-1d engine.
    pub fn single_with_copy(
        head: BankHead,
        eighths: i32,
        copy_weight_tenths: u8,
    ) -> Result<Self, &'static str> {
        if copy_weight_tenths > 10 {
            return Err("H-1d copy mixture weight must be in 0..=10 tenths");
        }
        let mut result = Self::single(head, eighths);
        result.copy_weight_tenths = copy_weight_tenths;
        Ok(result)
    }

    /// Activate the owner-selected initial 20% causal copy / 80% H-1d mixture.
    /// This is opt-in: existing single() retains exactly the prior behavior.
    #[must_use]
    pub fn single_with_default_copy(head: BankHead, eighths: i32) -> Self {
        Self::single_with_copy(head, eighths, DEFAULT_COPY_WEIGHT_TENTHS)
            .expect("fixed 20/80 copy mixture is in the validated range")
    }

    /// Gaussian competitor: identical frozen neural H-1d and copy history.
    /// The profile is frozen by label-free TRAIN-only exposure calibration;
    /// weights depend on the longest decoder-visible matching suffix.
    #[must_use]
    pub fn single_with_gaussian_copy(
        head: BankHead,
        eighths: i32,
        gaussian_copy: GaussianCopyProfile,
    ) -> Self {
        let mut result = Self::single(head, eighths);
        result.gaussian_copy = Some(gaussian_copy);
        result
    }

    /// Native BAF ANF activation for one frozen H-1d neural head.
    /// Only the declared four context and four depth-zero bank bits are legal.
    pub fn single_baf(
        head: BankHead,
        eighths: i32,
        gate: AnfActivation,
    ) -> Result<Self, &'static str> {
        if gate
            .terms
            .iter()
            .any(|t| t.event_mask & !15 != 0 || t.state_mask & !15 != 0)
        {
            return Err("BAF gate has out-of-range event/state feature mask");
        }
        let mut result = Self::single(head, eighths);
        result.baf_gate = Some(gate);
        Ok(result)
    }

    /// Non-learned TRAIN-activity-matched hash control, with identical
    /// frozen head, P1 bytes, and exact arithmetic coder.
    pub fn single_hash(head: BankHead, eighths: i32, threshold: u16) -> Result<Self, &'static str> {
        if threshold > 256 {
            return Err("BAF-H4 matched hash threshold is outside 0..=256");
        }
        let mut result = Self::single(head, eighths);
        result.hash_gate_threshold = Some(threshold);
        Ok(result)
    }

    /// The H-2 router; bytes routed to an expert that is not admitted are
    /// coded by P1 alone.
    #[must_use]
    pub fn routed(exported: Exported) -> Self {
        Self {
            router: Some(exported.router),
            baf_gate: None,
            hash_gate_threshold: None,
            fused: exported.heads.iter().map(FusedBankHead::new).collect(),
            heads: exported.heads,
            admitted: exported.admitted.to_vec(),
            eighths: exported.eighths,
            copy_weight_tenths: 0,
            gaussian_copy: None,
        }
    }

    /// Expert that corrects this byte, from what is known at decision 0.
    #[inline]
    fn select(&self, r: &BankRecord) -> Option<usize> {
        if self
            .hash_gate_threshold
            .is_some_and(|threshold| !hash_gate_is_active(r, threshold))
        {
            return None;
        }
        if self.baf_gate.as_ref().is_some_and(|gate| {
            let (event, state) = causal_features(r);
            !gate.is_active(u64::from(event), u64::from(state))
        }) {
            return None;
        }
        let k = match &self.router {
            Some(router) => router.route_bits(&route_bits(r)),
            None if self.heads.is_empty() => return None,
            None => 0,
        };
        self.admitted[k].then_some(k)
    }

    fn oracle_select(&self, r: &BankRecord) -> Option<usize> {
        if self
            .hash_gate_threshold
            .is_some_and(|threshold| !hash_gate_is_active(r, threshold))
        {
            return None;
        }
        if self.baf_gate.as_ref().is_some_and(|gate| {
            let (event, state) = oracle_causal_features(r);
            !gate.is_active(u64::from(event), u64::from(state))
        }) {
            return None;
        }
        let k = match &self.router {
            Some(router) => router.oracle_route(r),
            None if self.heads.is_empty() => return None,
            None => 0,
        };
        self.admitted[k].then_some(k)
    }

    /// Ternary weights of the admitted heads (the extra parameters).
    #[must_use]
    pub fn admitted_weights(&self) -> usize {
        self.admitted.iter().filter(|&&a| a).count() * DEPTHS * ROW
    }
}

/// `u32` coder probability of a stretch (the trace scale is 1/65536).
#[inline]
fn coder_p1(tables: &Tables, stretch: i32) -> u32 {
    tables.squash(stretch) << 16
}

/// Decision-by-decision engine for one document; encoder and decoder
/// each run their own copy from the start of the document.
pub struct HybridPredictor<'a> {
    model: &'a Model,
    tables: &'a Tables,
    head: &'a HybridHead,
    ctx: Context,
    position: usize,
    decision: usize,
    node: usize,
    blocks: PhaseBlocks,
    code: ByteCode,
    record: BankRecord,
    expert: Option<usize>,
    copy: Option<CopyExpert>,
    p1: u32,
}

impl<'a> HybridPredictor<'a> {
    #[must_use]
    pub fn new(model: &'a Model, tables: &'a Tables, head: &'a HybridHead) -> Self {
        let mut predictor = Self {
            model,
            tables,
            head,
            ctx: Context::default(),
            position: 0,
            decision: 0,
            node: 1,
            blocks: [None; MAX_BANKS],
            code: ByteCode::default(),
            record: BankRecord::default(),
            expert: None,
            copy: (head.copy_weight_tenths > 0 || head.gaussian_copy.is_some())
                .then(CopyExpert::default),
            p1: HALF,
        };
        predictor.prepare();
        predictor
    }

    /// Computes the probability of the next decision.
    #[inline]
    fn prepare(&mut self) {
        if self.position < UNCODED_PREFIX {
            self.p1 = HALF;
            return;
        }
        let d = self.decision;
        if d == 0 {
            if let Some(copy) = self.copy.as_mut() {
                copy.prepare_byte();
            }
            self.code = ByteCode::default();
            self.blocks = self.model.frozen_blocks(self.ctx, 0);
            self.record = BankRecord::default();
            self.record.record.word = self.ctx.word;
            self.record.record.context = self.ctx.bytes;
        } else if d == 4 {
            let high = self.record.record.target >> 4;
            self.blocks = self.model.frozen_blocks(self.ctx, 0x10 | high);
        }
        let st = self
            .model
            .mix(self.tables, &self.blocks, self.node, d, &mut self.code);
        let r = &mut self.record;
        r.record.logits[d] = st as i16;
        if d == 0 {
            r.record.confidence = self.code.confidence;
        }
        for (q, &s) in r.banks.stretch[d].iter_mut().zip(&self.code.banks[d]) {
            *q = crate::hyb_h1d_bank_head::quantize(i32::from(s));
        }
        if d == 0 {
            self.expert = self.head.select(r);
        }
        let shift = self.expert.map_or(0, |k| {
            self.head.fused[k].correction(r, d, self.head.eighths)
        });
        let neural = coder_p1(self.tables, st.saturating_add(shift));
        self.p1 = self.copy.as_ref().map_or(neural, |copy| {
            let proposal = copy.probability(d, r.record.target);
            if let Some(gaussian) = self.head.gaussian_copy {
                mix_gaussian_probability(
                    neural,
                    proposal,
                    gaussian.weight_for_match(copy.longest_match()),
                )
            } else {
                mix_probability(neural, proposal, self.head.copy_weight_tenths)
            }
        });
    }
}

impl BitPredictor for HybridPredictor<'_> {
    #[inline]
    fn p1(&self) -> u32 {
        self.p1
    }

    #[inline]
    fn update(&mut self, bit: bool) {
        let d = self.decision;
        // Prefix bytes are coded at 1/2 but still enter the context.
        self.record.record.target |= u8::from(bit) << (7 - d);
        self.node = if d == 3 {
            1
        } else {
            2 * self.node + usize::from(bit)
        };
        self.decision += 1;
        if self.decision == 8 {
            if let Some(copy) = self.copy.as_mut() {
                copy.push_decoded(self.record.record.target);
            }
            self.ctx.push(self.record.record.target);
            self.position += 1;
            self.decision = 0;
            self.node = 1;
            self.record.record.target = 0;
        }
        self.prepare();
    }
}

/// Compresses `data`: an 8-byte little-endian length, then the coded
/// payload.
#[must_use]
pub fn compress(model: &Model, tables: &Tables, head: &HybridHead, data: &[u8]) -> Vec<u8> {
    let mut predictor = HybridPredictor::new(model, tables, head);
    let mut encoder = BinaryEncoder::new();
    for &byte in data {
        for shift in (0..8).rev() {
            let bit = (byte >> shift) & 1 == 1;
            encoder.encode(bit, predictor.p1());
            predictor.update(bit);
        }
    }
    let mut out = (data.len() as u64).to_le_bytes().to_vec();
    out.extend_from_slice(&encoder.finish());
    out
}

/// Inverse of [`compress`]; fails closed on a truncated or padded stream.
pub fn decompress(
    model: &Model,
    tables: &Tables,
    head: &HybridHead,
    container: &[u8],
) -> Result<Vec<u8>, CoderError> {
    let length: [u8; 8] = container
        .get(..8)
        .ok_or(CoderError::TruncatedStream)?
        .try_into()
        .map_err(|_| CoderError::TruncatedStream)?;
    let length =
        usize::try_from(u64::from_le_bytes(length)).map_err(|_| CoderError::LengthOutOfRange)?;
    let payload = &container[8..];
    let mut decoder = BinaryDecoder::new(payload)?;
    let mut predictor = HybridPredictor::new(model, tables, head);
    let mut out = Vec::with_capacity(length.min(payload.len().saturating_mul(64)));
    for _ in 0..length {
        let mut byte = 0u8;
        for _ in 0..8 {
            let bit = decoder.decode(predictor.p1())?;
            predictor.update(bit);
            byte = (byte << 1) | u8::from(bit);
        }
        out.push(byte);
    }
    if decoder.consumed() != payload.len() {
        return Err(CoderError::TrailingData);
    }
    Ok(out)
}

/// The probabilities of every decision of `data`, computed the trace way:
/// `code_document` (as the trace writers), the scalar head oracle and the
/// scalar router oracle. The engine must reproduce them exactly.
#[must_use]
pub fn oracle_p1s(model: &mut Model, tables: &Tables, head: &HybridHead, data: &[u8]) -> Vec<u32> {
    let mut out = vec![HALF; UNCODED_PREFIX.min(data.len()) * 8];
    model.code_document(tables, data, false, |position, ctx, byte, code| {
        let r = BankRecord {
            record: TraceRecord {
                doc_index: 0,
                position: position as u32,
                split: 0,
                target: byte,
                word: ctx.word,
                context: ctx.bytes,
                logits: code.logits,
                confidence: code.confidence,
            },
            banks: BankPlane::from_code(0, position as u32, code),
        };
        let expert = head.oracle_select(&r);
        for d in 0..DEPTHS {
            let shift = expert.map_or(0, |k| {
                let h = &head.heads[k];
                h.correction(h.oracle_raw(&r, d), d, head.eighths)
            });
            let neural = coder_p1(tables, i32::from(code.logits[d]).saturating_add(shift));
            let copy = if head.copy_weight_tenths == 0 && head.gaussian_copy.is_none() {
                None
            } else {
                let known = if d == 0 {
                    0
                } else {
                    byte & (u8::MAX << (8 - d))
                };
                oracle_probability(&data[..position], d, known)
            };
            if let Some(profile) = head.gaussian_copy {
                out.push(mix_gaussian_probability(
                    neural,
                    copy,
                    profile.weight_for_match(oracle_longest_match(&data[..position])),
                ));
            } else {
                out.push(mix_probability(neural, copy, head.copy_weight_tenths));
            }
        }
    });
    out
}

/// The probabilities the engine applies to every decision of `data`.
#[must_use]
pub fn engine_p1s(model: &Model, tables: &Tables, head: &HybridHead, data: &[u8]) -> Vec<u32> {
    let mut predictor = HybridPredictor::new(model, tables, head);
    let mut out = Vec::with_capacity(data.len() * 8);
    for &byte in data {
        for shift in (0..8).rev() {
            out.push(predictor.p1());
            predictor.update((byte >> shift) & 1 == 1);
        }
    }
    out
}

/// A learned router, its experts, their admission and the shared lambda
/// (`experts-a2-seed*.tsv`, written by `sml-hyb-h2-s5h`).
#[derive(Debug, Clone)]
pub struct Exported {
    pub router: Router,
    pub heads: Vec<BankHead>,
    pub admitted: [bool; EXPERTS],
    pub eighths: i32,
}

fn field<'t>(text: &'t str, name: &str) -> Result<&'t str, String> {
    text.lines()
        .find_map(|l| l.strip_prefix(name)?.strip_prefix('\t'))
        .ok_or_else(|| format!("missing {name}"))
}

fn ints(text: &str, name: &str) -> Result<[i32; DEPTHS], String> {
    let v: Vec<i32> = field(text, name)?
        .split(',')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{name}: {e}"))?;
    v.try_into()
        .map_err(|_| format!("{name} needs {DEPTHS} values"))
}

fn ternary(text: &str, name: &str) -> Result<Vec<i8>, String> {
    let weights: Vec<i8> = field(text, name)?
        .split(',')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{name}: {e}"))?;
    if weights.len() != DEPTHS * ROW || weights.iter().any(|w| !(-1..=1).contains(w)) {
        return Err("weights must be DEPTHS*ROW ternary values".into());
    }
    Ok(weights)
}

fn lambda(text: &str) -> Result<i32, String> {
    let eighths: i32 = field(text, "lambda_eighths")?
        .parse()
        .map_err(|e| format!("lambda_eighths: {e}"))?;
    if !LAMBDA_EIGHTHS.contains(&eighths) {
        return Err("lambda outside the registered grid".into());
    }
    Ok(eighths)
}

fn shape(text: &str, experts: Option<usize>) -> Result<(), String> {
    let number = |name: &str| -> Result<usize, String> {
        field(text, name)?
            .parse()
            .map_err(|e| format!("{name}: {e}"))
    };
    let experts_ok = experts.map_or(Ok(true), |n| number("experts").map(|e| e == n))?;
    if number("row")? != ROW
        || number("depths")? != DEPTHS
        || number("banks")? != MAX_BANKS
        || !experts_ok
    {
        return Err("shape mismatch".into());
    }
    Ok(())
}

/// Parses `experts-a2-seed*.tsv`.
pub fn parse_experts(text: &str) -> Result<Exported, String> {
    shape(text, Some(EXPERTS))?;
    let words: Vec<u64> = field(text, "router")?
        .split(',')
        .map(|w| u64::from_str_radix(w, 16))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("router: {e}"))?;
    if words.len() != EXPERTS * ROUTE_WORDS {
        return Err("router needs EXPERTS * ROUTE_WORDS words".into());
    }
    let mut prototypes = [[0u64; ROUTE_WORDS]; EXPERTS];
    for (k, p) in prototypes.iter_mut().enumerate() {
        p.copy_from_slice(&words[k * ROUTE_WORDS..(k + 1) * ROUTE_WORDS]);
    }
    let mut heads = Vec::with_capacity(EXPERTS);
    for k in 0..EXPERTS {
        heads.push(BankHead::new(
            ternary(text, &format!("expert{k}_weights"))?,
            ints(text, &format!("expert{k}_unit"))?,
            ints(text, &format!("expert{k}_bias"))?,
        ));
    }
    let eighths = lambda(text)?;
    let flags: Vec<&str> = field(text, "admitted")?.split(',').collect();
    if flags.len() != EXPERTS || flags.iter().any(|f| *f != "0" && *f != "1") {
        return Err("admitted needs EXPERTS flags 0 or 1".into());
    }
    let mut admitted = [false; EXPERTS];
    for (a, f) in admitted.iter_mut().zip(flags) {
        *a = f == "1";
    }
    Ok(Exported {
        router: Router { prototypes },
        heads,
        admitted,
        eighths,
    })
}

/// Parses an H-1d head (`head-a1d-seed*.tsv`): one head and its lambda.
pub fn parse_h1d_head(text: &str) -> Result<(BankHead, i32), String> {
    shape(text, None)?;
    Ok((
        BankHead::new(
            ternary(text, "weights")?,
            ints(text, "unit")?,
            ints(text, "bias")?,
        ),
        lambda(text)?,
    ))
}

/// Synthetic models, heads and documents for tests and for the cross-host
/// determinism file (`deterministic_csv`): no data is read.
pub mod synthetic {
    use super::*;
    use crate::{
        elastic_ngram_paged_sha::{hex, sha256},
        gb11_p1_stream::{order_mask, MixRule, UpdateRule},
        hyb_h1_residual::splitmix,
    };

    /// Word-salad text with an accent and parentheses, from `seed`.
    #[must_use]
    pub fn text(seed: u64, len: usize) -> Vec<u8> {
        let words = [
            "the ",
            "of ",
            "model ",
            "bank ",
            "count ",
            "and ",
            "a ",
            "bit ",
            "(Fig. 2) ",
            "results ",
            "we ",
            "show ",
            "that ",
            "é ",
            "\n",
        ];
        let mut out = Vec::with_capacity(len);
        let mut s = seed;
        while out.len() < len {
            s = splitmix(s);
            out.extend_from_slice(words[(s % words.len() as u64) as usize].as_bytes());
        }
        out.truncate(len);
        out
    }

    /// A small P1 trained on synthetic text (integer counts only).
    #[must_use]
    pub fn model(tables: &Tables) -> Model {
        let mut m = Model::new(
            vec![order_mask(1), order_mask(2), order_mask(3) | (0xffff << 64)],
            MixRule::Average,
            UpdateRule::Adaptive,
            1 << 16,
        );
        for k in 0..20 {
            m.code_document(tables, &text(k, 4000), true, |_, _, _, _| {});
        }
        m
    }

    #[must_use]
    pub fn head(seed: u64) -> BankHead {
        let weights = (0..DEPTHS * ROW)
            .map(|i| ((splitmix(seed ^ i as u64) % 3) as i8) - 1)
            .collect();
        BankHead::new(weights, [5; DEPTHS], [-3; DEPTHS])
    }

    #[must_use]
    pub fn router() -> Router {
        Router {
            prototypes: std::array::from_fn(|k| [splitmix(k as u64), splitmix(100 + k as u64)]),
        }
    }

    /// No head, one head, the router with one expert not admitted.
    #[must_use]
    pub fn heads() -> [(&'static str, HybridHead); 3] {
        [
            ("p1", HybridHead::none()),
            ("single", HybridHead::single(head(1), 8)),
            (
                "routed",
                HybridHead::routed(Exported {
                    router: router(),
                    heads: (0..EXPERTS as u64).map(head).collect(),
                    admitted: [true, false, true, true],
                    eighths: 4,
                }),
            ),
        ]
    }

    /// Documents of every edge length plus text and random bytes.
    #[must_use]
    pub fn documents() -> Vec<Vec<u8>> {
        let random: Vec<u8> = (0..3000u64).map(|i| splitmix(i ^ 0x4833) as u8).collect();
        vec![
            Vec::new(),
            vec![b'a'],
            b"ab".to_vec(),
            b"abc".to_vec(),
            text(1001, 200),
            text(1002, 20_000),
            random,
            vec![0; 500],
        ]
    }

    /// One row per (head, document): lengths, the container SHA-256, the
    /// SHA-256 of the applied probabilities, and whether the engine
    /// matches the trace way and round-trips. Identical bytes on every
    /// host are the determinism criterion of H-3.
    #[must_use]
    pub fn deterministic_csv() -> String {
        let tables = Tables::new();
        let mut m = model(&tables);
        let mut out = format!(
            "cell,head,doc,bytes,container_bytes,container_sha256,p1s_sha256,oracle_equal,round_trip\n\
             tables,,,,,{},,,\nweights,,,,,{},,,\n",
            hex(&tables.sha()),
            hex(&m.weights_sha()),
        );
        for (name, h) in heads() {
            for (i, doc) in documents().iter().enumerate() {
                let p1s = engine_p1s(&m, &tables, &h, doc);
                let oracle = p1s == oracle_p1s(&mut m, &tables, &h, doc);
                let packed = compress(&m, &tables, &h, doc);
                let back = decompress(&m, &tables, &h, &packed).is_ok_and(|d| d == *doc);
                let bytes: Vec<u8> = p1s.iter().flat_map(|p| p.to_le_bytes()).collect();
                out += &format!(
                    "engine,{name},{i},{},{},{},{},{oracle},{back}\n",
                    doc.len(),
                    packed.len(),
                    hex(&sha256(&packed)),
                    hex(&sha256(&bytes)),
                );
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::synthetic::{head, model, router, text};
    use super::*;

    fn heads() -> Vec<HybridHead> {
        super::synthetic::heads()
            .into_iter()
            .map(|(_, h)| h)
            .collect()
    }

    #[test]
    fn native_boolean_gate_preserves_coder_and_independent_oracle() {
        let tables = Tables::new();
        let mut m = model(&tables);
        let gate = AnfActivation {
            bias: true,
            terms: vec![crate::boolean_associative_focus::AnfMonomial {
                event_mask: 1,
                state_mask: 1,
            }],
        };
        let h = HybridHead::single_baf(head(1), 8, gate).unwrap();
        let never = HybridHead::single_baf(
            head(1),
            8,
            AnfActivation {
                bias: false,
                terms: vec![],
            },
        )
        .unwrap();
        let p1 = HybridHead::none();
        let data = text(77, 3000);
        assert_eq!(
            engine_p1s(&m, &tables, &h, &data),
            oracle_p1s(&mut m, &tables, &h, &data)
        );
        let compressed = compress(&m, &tables, &h, &data);
        assert_eq!(decompress(&m, &tables, &h, &compressed).unwrap(), data);
        assert_eq!(
            engine_p1s(&m, &tables, &never, &data),
            engine_p1s(&m, &tables, &p1, &data)
        );
        assert_eq!(
            compress(&m, &tables, &never, &data),
            compress(&m, &tables, &p1, &data)
        );
    }

    #[test]
    fn native_baf_gate_rejects_unknown_feature_bits() {
        let invalid = AnfActivation {
            bias: true,
            terms: vec![crate::boolean_associative_focus::AnfMonomial {
                event_mask: 16,
                state_mask: 0,
            }],
        };
        assert!(HybridHead::single_baf(head(1), 8, invalid).is_err());
    }

    #[test]
    fn gaussian_copy_competes_with_fixed_twenty_eighty_in_full_lossless_coder() {
        use crate::hyb_h1d_gaussian_copy::{
            CopyActivityHistogram, GaussianCopyProfile, GAUSSIAN_KERNEL_Q1024,
        };
        let mut histogram = CopyActivityHistogram::default();
        histogram
            .observe_train_document(&b"abababab".repeat(1024))
            .unwrap();
        let gaussian = GaussianCopyProfile::fit_train_only(&histogram).unwrap();
        assert_eq!(GAUSSIAN_KERNEL_Q1024, [0, 139, 421, 820, 1024]);

        let tables = Tables::new();
        let mut m = model(&tables);
        let neural = HybridHead::single(head(1), 8);
        let fixed = HybridHead::single_with_default_copy(head(1), 8);
        let gaussian_head = HybridHead::single_with_gaussian_copy(head(1), 8, gaussian);
        let documents = [
            Vec::new(),
            vec![7],
            vec![7, 8],
            vec![7, 8, 9],
            b"abababcabababc".repeat(200),
            b"abcabcabc___abcabcabc___".repeat(200),
            text(137, 2048),
            (0..1024).map(|i| (i as u8).wrapping_mul(41)).collect(),
        ];
        for data in documents {
            for variant in [&neural, &fixed, &gaussian_head] {
                assert_eq!(
                    engine_p1s(&m, &tables, variant, &data),
                    oracle_p1s(&mut m, &tables, variant, &data),
                    "independent oracle must agree for all competitor arms"
                );
                let packed = compress(&m, &tables, variant, &data);
                assert_eq!(
                    decompress(&m, &tables, variant, &packed).unwrap(),
                    data,
                    "each competitor must be losslessly decoded"
                );
            }
        }
        let data = b"abababababa".repeat(200);
        assert_ne!(
            engine_p1s(&m, &tables, &neural, &data),
            engine_p1s(&m, &tables, &gaussian_head, &data),
            "Gaussian mixture must be active, not merely a configuration flag"
        );
    }

    #[test]
    fn copy_expert_and_frozen_h1d_blend_are_causal_and_losslessly_decodable() {
        let tables = Tables::new();
        let mut m = model(&tables);
        let base = HybridHead::single(head(1), 8);
        let disabled = HybridHead::single_with_copy(head(1), 8, 0).unwrap();
        let mixed = HybridHead::single_with_copy(head(1), 8, 2).unwrap();
        let default_mixed = HybridHead::single_with_default_copy(head(1), 8);
        let copy_heavy = HybridHead::single_with_copy(head(1), 8, 10).unwrap();
        let inputs = [
            Vec::new(),
            vec![7],
            vec![7, 8],
            vec![7, 8, 9],
            vec![0u8; 2048],
            b"abcabc".repeat(500),
            text(71, 3000),
            (0..4096).map(|i| (i * 29) as u8).collect::<Vec<_>>(),
        ];
        for data in inputs {
            assert_eq!(
                compress(&m, &tables, &mixed, &data),
                compress(&m, &tables, &default_mixed, &data),
                "default opt-in copy must use 20/80"
            );
            assert_eq!(
                compress(&m, &tables, &base, &data),
                compress(&m, &tables, &disabled, &data),
                "weight 0 must preserve legacy H-1d stream"
            );
            for h in [&mixed, &copy_heavy] {
                let fast = engine_p1s(&m, &tables, h, &data);
                let oracle = oracle_p1s(&mut m, &tables, h, &data);
                assert_eq!(fast, oracle, "independent causal oracle mismatch");
                let container = compress(&m, &tables, h, &data);
                assert_eq!(decompress(&m, &tables, h, &container).unwrap(), data);
            }
        }
        let repeated = b"abcabc".repeat(50);
        assert_ne!(
            engine_p1s(&m, &tables, &base, &repeated),
            engine_p1s(&m, &tables, &mixed, &repeated),
            "repeated context should actually activate the copy expert"
        );
    }

    #[test]
    fn copy_expert_rejects_invalid_weight_and_does_not_look_at_future_bits() {
        let tables = Tables::new();
        let m = model(&tables);
        let h = HybridHead::single_with_copy(head(1), 8, 2).unwrap();
        assert!(HybridHead::single_with_copy(head(1), 8, 11).is_err());
        let history = b"abcabcabcabc";
        let a = [history.as_slice(), b"aX"].concat();
        let b = [history.as_slice(), b"zY"].concat();
        let pa = engine_p1s(&m, &tables, &h, &a);
        let pb = engine_p1s(&m, &tables, &h, &b);
        let current = history.len() * 8;
        assert_eq!(pa[..=current], pb[..=current]);
        // A new document must reset the copy history: same data, same output.
        assert_eq!(compress(&m, &tables, &h, &a), compress(&m, &tables, &h, &a));
    }

    #[test]
    fn the_engine_matches_the_trace_way_on_every_decision() {
        let tables = Tables::new();
        let mut m = model(&tables);
        for h in heads() {
            for data in super::synthetic::documents()
                .into_iter()
                .chain([text(99, 3000), vec![0xff; 40]])
            {
                assert_eq!(
                    engine_p1s(&m, &tables, &h, &data),
                    oracle_p1s(&mut m, &tables, &h, &data)
                );
            }
        }
    }

    #[test]
    fn compress_round_trips_and_the_head_changes_the_stream() {
        let tables = Tables::new();
        let m = model(&tables);
        let data = text(77, 5000);
        let mut streams = Vec::new();
        for h in heads() {
            let packed = compress(&m, &tables, &h, &data);
            assert_eq!(decompress(&m, &tables, &h, &packed).unwrap(), data);
            assert!(packed.len() < data.len() / 2, "{}", packed.len());
            streams.push(packed);
        }
        assert_ne!(streams[0], streams[1]);
        assert_ne!(streams[1], streams[2]);
        // Wrong head, truncation and padding fail or decode other bytes.
        let other = decompress(&m, &tables, &heads()[0], &streams[1]);
        assert!(other.map_or(true, |d| d != data));
        let cut = &streams[0][..streams[0].len() - 3];
        assert!(decompress(&m, &tables, &heads()[0], cut).map_or(true, |d| d != data));
        let mut padded = streams[0].clone();
        padded.push(0);
        assert!(decompress(&m, &tables, &heads()[0], &padded).is_err());
        for data in [Vec::new(), vec![7u8], vec![1, 2, 3]] {
            let packed = compress(&m, &tables, &heads()[2], &data);
            assert_eq!(decompress(&m, &tables, &heads()[2], &packed).unwrap(), data);
        }
    }

    #[test]
    fn experts_and_h1d_heads_parse() {
        let h = head(3);
        let join = |v: &[i32]| v.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
        let weights: Vec<String> = h.weights().iter().map(i8::to_string).collect();
        let single = format!(
            "row\t{ROW}\ndepths\t{DEPTHS}\nbanks\t{MAX_BANKS}\nlambda_eighths\t8\nunit\t{}\nbias\t{}\nweights\t{}\n",
            join(&h.unit()),
            join(&h.bias()),
            weights.join(",")
        );
        let (parsed, eighths) = parse_h1d_head(&single).unwrap();
        assert_eq!((parsed.weights(), eighths), (h.weights(), 8));
        assert!(parse_h1d_head(&single.replace("lambda_eighths\t8", "lambda_eighths\t3")).is_err());
        assert!(parse_h1d_head(&single.replace("row\t192", "row\t191")).is_err());
        let mut routed = format!(
            "row\t{ROW}\ndepths\t{DEPTHS}\nbanks\t{MAX_BANKS}\nexperts\t{EXPERTS}\nlambda_eighths\t4\nrouter\t{}\nadmitted\t1,0,1,1\n",
            router()
                .prototypes
                .iter()
                .flatten()
                .map(|w| format!("{w:016x}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        for k in 0..EXPERTS {
            routed += &format!(
                "expert{k}_unit\t{}\nexpert{k}_bias\t{}\nexpert{k}_weights\t{}\n",
                join(&h.unit()),
                join(&h.bias()),
                weights.join(",")
            );
        }
        let e = parse_experts(&routed).unwrap();
        assert_eq!(e.router, router());
        assert_eq!(e.admitted, [true, false, true, true]);
        assert_eq!(e.eighths, 4);
        assert!(parse_experts(&routed.replace("admitted\t1,0,1,1", "admitted\t1,0,1")).is_err());
    }

    #[test]
    fn the_determinism_file_is_reproducible_and_all_cells_pass() {
        let csv = super::synthetic::deterministic_csv();
        assert_eq!(csv, super::synthetic::deterministic_csv());
        let rows: Vec<&str> = csv.lines().filter(|l| l.starts_with("engine,")).collect();
        assert_eq!(rows.len(), 3 * super::synthetic::documents().len());
        assert!(rows.iter().all(|r| r.ends_with(",true,true")), "{csv}");
    }
}
