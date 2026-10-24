//! GB-11 P1: Boolean-addressed count banks for next-byte prediction on S5h.
//!
//! Each bank reads one address: a learned subset of the context bits (the
//! last eight bytes plus a 16-bit word register `W` updated by a GF(2)-linear
//! map), selected by exact greedy search on a TRAIN prefix. A cell holds two
//! 8-bit counts for one MSB-tree decision; the expert is the closed-form cell
//! statistic (the P0b majority refit generalized to counts). Banks are mixed
//! by a fixed integer rule, with no learned continuous weight. Probabilities
//! are integer ln-odds (`STRETCH_SCALE` units) squashed by a table.
//!
//! Research pilot only (docs/GB11-P1-S5H-PREREGISTRATION.md). No timing,
//! energy or promotion claim.

use crate::elastic_ngram_paged_sha::Sha256;

/// Bytes of one table block: a tag slot plus 15 nibble-tree nodes of 2 bytes.
pub const BLOCK_BYTES: usize = 32;
/// Table bytes of the witness's deployed Q4 weights (`weights.q4`).
pub const Q4_EQUAL_BYTES: usize = 25_000_448;
/// Table bytes of the witness's f32 master (`master.f32`).
pub const F32_EQUAL_BYTES: usize = 200_003_584;
/// Largest bank count.
pub const MAX_BANKS: usize = 8;
/// Candidate address bits: 64 context-byte bits, then 16 `W` bits.
pub const CANDIDATE_BITS: u32 = 80;
/// Ln-odds fixed point: one unit is 1/256 nat.
pub const STRETCH_SCALE: f64 = 256.0;
/// Saturation of every stretch value (±8 nats).
pub const STRETCH_LIMIT: i32 = 2047;
/// Mixing weight at full confidence.
const WEIGHT_ONE: i32 = 64;
/// Greedy search stops below this DEV-free holdout gain, in bits per byte.
pub const SEARCH_MIN_GAIN_BPC: f64 = 0.001;
/// Largest address width of one bank.
pub const SEARCH_MAX_BITS: u32 = 40;
/// Trace record size in bytes.
pub const TRACE_RECORD_BYTES: usize = 40;

/// GF(2)-linear word register update. Letters (ASCII, case folded) and bytes
/// >= 0x80 rotate-and-XOR into `w`; any other byte resets it to zero.
#[must_use]
pub fn word_update(w: u16, byte: u8) -> u16 {
    if byte.is_ascii_alphabetic() {
        w.rotate_left(3) ^ u16::from(byte | 0x20)
    } else if byte >= 0x80 {
        w.rotate_left(3) ^ u16::from(byte)
    } else {
        0
    }
}

/// Prediction context: `bytes` holds c1 (the previous byte) in its low byte.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Context {
    pub bytes: u64,
    pub word: u16,
}

impl Context {
    #[must_use]
    pub fn bits(self) -> u128 {
        u128::from(self.bytes) | (u128::from(self.word) << 64)
    }

    pub fn push(&mut self, byte: u8) {
        self.bytes = (self.bytes << 8) | u64::from(byte);
        self.word = word_update(self.word, byte);
    }
}

/// Address mask of a byte n-gram of `order` bytes.
#[must_use]
pub fn order_mask(order: u32) -> u128 {
    if order == 0 {
        0
    } else {
        (1u128 << (8 * order.min(8))) - 1
    }
}

fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Each bank owns its table, so the key needs no bank index.
fn block_key(masked: u128, phase_prefix: u8) -> u64 {
    let low = masked as u64;
    let high = (masked >> 64) as u64;
    mix64(
        mix64(low ^ 0x4742_3131_5031_0001) ^ high.rotate_left(17) ^ (u64::from(phase_prefix) << 40),
    )
}

/// Integer probability tables shared by every model.
pub struct Tables {
    stretch: Vec<i16>,
    weight: Vec<i32>,
    squash: Vec<u32>,
}

impl Tables {
    #[must_use]
    pub fn new() -> Self {
        let mut stretch = vec![0i16; 256 * 256];
        for n0 in 0..256 {
            for n1 in 0..256 {
                let v = STRETCH_SCALE * ((n1 as f64 + 0.4) / (n0 as f64 + 0.4)).ln();
                stretch[n0 * 256 + n1] = v
                    .round()
                    .clamp(f64::from(-STRETCH_LIMIT), f64::from(STRETCH_LIMIT))
                    as i16;
            }
        }
        let weight = (0..511)
            .map(|n| (f64::from(WEIGHT_ONE) * n as f64 / (n as f64 + 1.5)).round() as i32)
            .collect();
        Self {
            stretch,
            weight,
            squash: (-STRETCH_LIMIT..=STRETCH_LIMIT).map(squash_value).collect(),
        }
    }

    /// P(bit = 1) in 1/65536 units.
    #[must_use]
    pub fn squash(&self, st: i32) -> u32 {
        self.squash[(st.clamp(-STRETCH_LIMIT, STRETCH_LIMIT) + STRETCH_LIMIT) as usize]
    }
}

impl Default for Tables {
    fn default() -> Self {
        Self::new()
    }
}

/// P(bit = 1) in 1/65536 units for a saturated stretch `st`, within [1, 65535].
#[must_use]
pub fn squash_value(st: i32) -> u32 {
    let st = st.clamp(-STRETCH_LIMIT, STRETCH_LIMIT);
    let p = 65536.0 / (1.0 + (-f64::from(st) / STRETCH_SCALE).exp());
    (p.round() as u32).clamp(1, 65535)
}

/// Code length in bits of `bit` under P(1) = `p1` / 65536.
#[must_use]
pub fn bit_cost(p1: u32, bit: bool) -> f64 {
    let p = if bit { p1 } else { 65536 - p1 };
    16.0 - f64::from(p).log2()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixRule {
    /// Confidence-weighted mean of the bank stretches.
    Average,
    /// Confidence-weighted sum, halved.
    HalfSum,
    /// Highest-priority bank with at least two observations.
    Top,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateRule {
    /// Increment, halve both counts at saturation.
    Stationary,
    /// Also shrink the opposite count above 2 to `n / 2 + 1`.
    Adaptive,
}

impl MixRule {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Average => "average",
            Self::HalfSum => "half_sum",
            Self::Top => "top",
        }
    }
}

impl UpdateRule {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Stationary => "stationary",
            Self::Adaptive => "adaptive",
        }
    }
}

/// The six preregistered mixer/update configurations, in tie-break order.
pub const CONFIGS: [(MixRule, UpdateRule); 6] = [
    (MixRule::Average, UpdateRule::Adaptive),
    (MixRule::Average, UpdateRule::Stationary),
    (MixRule::HalfSum, UpdateRule::Adaptive),
    (MixRule::HalfSum, UpdateRule::Stationary),
    (MixRule::Top, UpdateRule::Adaptive),
    (MixRule::Top, UpdateRule::Stationary),
];

/// One bank's hashed block table.
#[derive(Clone)]
struct BankTable {
    data: Vec<u8>,
    blocks: u64,
}

impl BankTable {
    fn new(blocks: usize) -> Self {
        Self {
            data: vec![0; blocks * BLOCK_BYTES],
            blocks: blocks as u64,
        }
    }

    /// Byte offset of the block for `key`, claiming it when `learn`. A frozen
    /// lookup whose tag differs reads as unseen.
    fn locate(&mut self, key: u64, learn: bool) -> Option<usize> {
        let base = (key % self.blocks) as usize * BLOCK_BYTES;
        let tag = ((key >> 56) as u8) | 1;
        if self.data[base] == tag {
            Some(base)
        } else if learn {
            self.data[base..base + BLOCK_BYTES].fill(0);
            self.data[base] = tag;
            Some(base)
        } else {
            None
        }
    }
}

/// A frozen-architecture P1 model: masks, mixer and update rule plus tables.
#[derive(Clone)]
pub struct Model {
    pub masks: Vec<u128>,
    pub rule: MixRule,
    pub update: UpdateRule,
    tables: Vec<BankTable>,
}

/// Per-byte outputs used by the trace writer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ByteCode {
    /// Mixed stretch of each decision on the true MSB path.
    pub logits: [i16; 8],
    /// Per bank, `min(15, n0 + n1)` at the root decision (4 bits per bank).
    pub confidence: u32,
    /// Stretch of each bank's own cell at each decision (HYB H-1d inputs);
    /// unused banks 0.
    pub banks: [[i16; MAX_BANKS]; 8],
}

impl Model {
    /// `table_bytes` is split evenly across banks, in whole blocks.
    #[must_use]
    pub fn new(masks: Vec<u128>, rule: MixRule, update: UpdateRule, table_bytes: usize) -> Self {
        assert!(!masks.is_empty());
        let bank_bytes = table_bytes / masks.len();
        Self::with_bank_bytes(masks, rule, update, bank_bytes)
    }

    /// Every bank gets `bank_bytes`, in whole blocks. A searched architecture
    /// keeps the allocation `search_masks` scored, even when the search
    /// stopped before its requested bank count.
    #[must_use]
    pub fn with_bank_bytes(
        masks: Vec<u128>,
        rule: MixRule,
        update: UpdateRule,
        bank_bytes: usize,
    ) -> Self {
        assert!(!masks.is_empty() && masks.len() <= MAX_BANKS);
        let blocks = bank_bytes / BLOCK_BYTES;
        assert!(blocks > 0, "table too small for the bank count");
        let tables = vec![BankTable::new(blocks); masks.len()];
        Self {
            masks,
            rule,
            update,
            tables,
        }
    }

    #[must_use]
    pub fn table_bytes(&self) -> usize {
        self.tables.iter().map(|t| t.data.len()).sum()
    }

    /// Node slots with at least one observation (the learned parameters).
    #[must_use]
    pub fn touched_slots(&self) -> u64 {
        let mut touched = 0;
        for table in &self.tables {
            for block in table.data.chunks_exact(BLOCK_BYTES) {
                touched += block[2..]
                    .chunks_exact(2)
                    .filter(|s| s[0] != 0 || s[1] != 0)
                    .count() as u64;
            }
        }
        touched
    }

    /// SHA-256 over the bank tables in order (the weights identity).
    #[must_use]
    pub fn weights_sha(&self) -> [u8; 32] {
        let mut sha = Sha256::default();
        for table in &self.tables {
            sha.update(&table.data);
        }
        sha.finish()
    }

    /// Writes the bank tables in order.
    pub fn write_weights(&self, out: &mut impl std::io::Write) -> std::io::Result<()> {
        for table in &self.tables {
            out.write_all(&table.data)?;
        }
        Ok(())
    }

    /// Codes `target` after `ctx`, returning its cost in bits. When `learn`,
    /// counts are updated after each decision is predicted (prequential).
    pub fn code_byte(
        &mut self,
        tables: &Tables,
        ctx: Context,
        target: u8,
        learn: bool,
        out: Option<&mut ByteCode>,
    ) -> f64 {
        let bits = ctx.bits();
        let banks = self.masks.len();
        let mut code = ByteCode::default();
        let mut cost = 0.0;
        for phase in 0..2u8 {
            let prefix = if phase == 0 { 0 } else { 0x10 | (target >> 4) };
            let mut blocks = [None; MAX_BANKS];
            for (b, block) in blocks.iter_mut().enumerate().take(banks) {
                let key = block_key(bits & self.masks[b], prefix);
                *block = self.tables[b].locate(key, learn);
            }
            let mut node = 1usize;
            for depth in 0..4 {
                let decision = usize::from(phase) * 4 + depth;
                let bit = (target >> (7 - decision)) & 1 != 0;
                let mut sum = 0i64;
                let mut weight = 0i64;
                let mut top = None;
                let mut first = 0i32;
                for (b, block) in blocks.iter().enumerate().take(banks) {
                    let (n0, n1) = match block {
                        Some(base) => {
                            let slot = base + 2 * node;
                            let data = &self.tables[b].data;
                            (usize::from(data[slot]), usize::from(data[slot + 1]))
                        }
                        None => (0, 0),
                    };
                    let st = i32::from(tables.stretch[n0 * 256 + n1]);
                    code.banks[decision][b] = st as i16;
                    let w = tables.weight[n0 + n1];
                    sum += i64::from(w) * i64::from(st);
                    weight += i64::from(w);
                    if b == 0 {
                        first = st;
                    }
                    if n0 + n1 >= 2 {
                        top = Some(st);
                    }
                    if decision == 0 {
                        code.confidence |= ((n0 + n1).min(15) as u32) << (4 * b);
                    }
                }
                let st = match self.rule {
                    MixRule::Average if weight > 0 => (sum / weight) as i32,
                    MixRule::Average => 0,
                    MixRule::HalfSum => (sum / i64::from(2 * WEIGHT_ONE)) as i32,
                    MixRule::Top => top.unwrap_or(first),
                }
                .clamp(-STRETCH_LIMIT, STRETCH_LIMIT);
                code.logits[decision] = st as i16;
                cost += bit_cost(tables.squash(st), bit);
                if learn {
                    for (b, block) in blocks.iter().enumerate().take(banks) {
                        let base = block.expect("learning claims every block");
                        let slot = base + 2 * node;
                        let data = &mut self.tables[b].data;
                        let (hit, other) = if bit {
                            (slot + 1, slot)
                        } else {
                            (slot, slot + 1)
                        };
                        if data[hit] == 255 {
                            data[hit] >>= 1;
                            data[other] >>= 1;
                        }
                        data[hit] += 1;
                        if self.update == UpdateRule::Adaptive && data[other] > 2 {
                            data[other] = data[other] / 2 + 1;
                        }
                    }
                }
                node = 2 * node + usize::from(bit);
            }
        }
        if let Some(out) = out {
            *out = code;
        }
        cost
    }

    /// Codes positions `2..len` of one document (the witness's events);
    /// returns (bits, events). `each` sees every coded position.
    pub fn code_document(
        &mut self,
        tables: &Tables,
        document: &[u8],
        learn: bool,
        mut each: impl FnMut(usize, Context, u8, &ByteCode),
    ) -> (f64, u64) {
        let mut ctx = Context::default();
        let mut cost = 0.0;
        let mut code = ByteCode::default();
        for (position, &byte) in document.iter().enumerate() {
            if position >= 2 {
                cost += self.code_byte(tables, ctx, byte, learn, Some(&mut code));
                each(position, ctx, byte, &code);
            }
            ctx.push(byte);
        }
        (cost, document.len().saturating_sub(2) as u64)
    }
}

/// Per-decision mixing contribution of one bank on a holdout: (Σ w·st, Σ w).
/// Trains one bank on `fit`, then calls `each(i, Σ w·st, w)` for every
/// holdout decision `i`. Counts are per bank, so this is exactly the bank's
/// share of the full average-rule, stationary-update model.
fn bank_contribution(
    tables: &Tables,
    mask: u128,
    table_bytes: usize,
    fit: &[&[u8]],
    holdout: &[&[u8]],
    mut each: impl FnMut(usize, i32, i32),
) {
    let mut model = Model::new(
        vec![mask],
        MixRule::Average,
        UpdateRule::Stationary,
        table_bytes,
    );
    for doc in fit {
        model.code_document(tables, doc, true, |_, _, _, _| {});
    }
    let mut index = 0;
    for doc in holdout {
        let mut ctx = Context::default();
        for (position, &byte) in doc.iter().enumerate() {
            if position >= 2 {
                let bits = ctx.bits() & mask;
                for phase in 0..2u8 {
                    let prefix = if phase == 0 { 0 } else { 0x10 | (byte >> 4) };
                    let block = model.tables[0].locate(block_key(bits, prefix), false);
                    let mut node = 1usize;
                    for depth in 0..4 {
                        let bit = (byte >> (7 - (usize::from(phase) * 4 + depth))) & 1 != 0;
                        let (n0, n1) = block.map_or((0, 0), |base| {
                            let d = &model.tables[0].data;
                            (
                                usize::from(d[base + 2 * node]),
                                usize::from(d[base + 2 * node + 1]),
                            )
                        });
                        let w = tables.weight[n0 + n1];
                        each(index, w * i32::from(tables.stretch[n0 * 256 + n1]), w);
                        index += 1;
                        node = 2 * node + usize::from(bit);
                    }
                }
            }
            ctx.push(byte);
        }
    }
}

fn holdout_bits(holdout: &[&[u8]]) -> Vec<bool> {
    let mut bits = Vec::new();
    for doc in holdout {
        for &byte in doc.iter().skip(2) {
            for d in 0..8 {
                bits.push((byte >> (7 - d)) & 1 != 0);
            }
        }
    }
    bits
}

/// Average-rule mixture cost of `base` plus one candidate bank.
fn candidate_cost(
    tables: &Tables,
    base: &[(i32, i32)],
    bits: &[bool],
    mask: u128,
    per_bank: usize,
    fit: &[&[u8]],
    holdout: &[&[u8]],
) -> f64 {
    let mut cost = 0.0;
    bank_contribution(tables, mask, per_bank, fit, holdout, |i, s, w| {
        let (s, w) = (base[i].0 + s, base[i].1 + w);
        let st = if w > 0 { s / w } else { 0 };
        cost += bit_cost(tables.squash(st), bits[i]);
    });
    cost
}

/// One greedy step of the address search, for the log.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchStep {
    pub bank: usize,
    pub bit: u32,
    pub holdout_bpc: f64,
}

/// Exact greedy forward selection of bank addresses (P1 arm `search`).
/// Bank 0 is the empty (order-0) address. Each next bank adds, one at a time,
/// the candidate bit that most lowers the holdout code length of the
/// average-rule mixture, until the gain drops below `SEARCH_MIN_GAIN_BPC` or
/// the width reaches `SEARCH_MAX_BITS`. Ties go to the lowest bit index.
/// `parallel` evaluates candidates on that many threads.
pub fn search_masks(
    banks: usize,
    table_bytes: usize,
    fit: &[&[u8]],
    holdout: &[&[u8]],
    parallel: usize,
    mut log: impl FnMut(&SearchStep),
) -> Vec<u128> {
    assert!((1..=MAX_BANKS).contains(&banks));
    let tables = Tables::new();
    let per_bank = table_bytes / banks;
    let bits = holdout_bits(holdout);
    let bytes = (bits.len() / 8) as f64;
    let mut base = vec![(0i32, 0i32); bits.len()];
    let add = |base: &mut Vec<(i32, i32)>, mask: u128| {
        bank_contribution(&tables, mask, per_bank, fit, holdout, |i, s, w| {
            base[i].0 += s;
            base[i].1 += w;
        });
    };
    add(&mut base, 0);
    let mut masks = vec![0u128];
    let mut current = 0.0;
    for (i, &(s, w)) in base.iter().enumerate() {
        let st = if w > 0 { s / w } else { 0 };
        current += bit_cost(tables.squash(st), bits[i]);
    }
    log(&SearchStep {
        bank: 0,
        bit: u32::MAX,
        holdout_bpc: current / bytes,
    });
    for bank in 1..banks {
        let mut mask = 0u128;
        for _ in 0..SEARCH_MAX_BITS {
            let candidates: Vec<u32> = (0..CANDIDATE_BITS)
                .filter(|&j| mask & (1u128 << j) == 0)
                .collect();
            let next = std::sync::atomic::AtomicUsize::new(0);
            let costs = std::sync::Mutex::new(vec![f64::INFINITY; candidates.len()]);
            std::thread::scope(|scope| {
                for _ in 0..parallel.max(1) {
                    scope.spawn(|| loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&j) = candidates.get(i) else { break };
                        let cost = candidate_cost(
                            &tables,
                            &base,
                            &bits,
                            mask | (1u128 << j),
                            per_bank,
                            fit,
                            holdout,
                        );
                        costs.lock().expect("costs")[i] = cost;
                    });
                }
            });
            let costs = costs.into_inner().expect("costs");
            let mut best = 0;
            for (i, &cost) in costs.iter().enumerate() {
                if cost < costs[best] {
                    best = i;
                }
            }
            if (current - costs[best]) / bytes < SEARCH_MIN_GAIN_BPC {
                break;
            }
            mask |= 1u128 << candidates[best];
            current = costs[best];
            log(&SearchStep {
                bank,
                bit: candidates[best],
                holdout_bpc: current / bytes,
            });
        }
        if mask == 0 {
            break;
        }
        add(&mut base, mask);
        masks.push(mask);
    }
    masks
}

/// Holdout cost in bits of a fresh model trained on `fit`.
#[must_use]
pub fn holdout_cost(model: &mut Model, fit: &[&[u8]], holdout: &[&[u8]]) -> f64 {
    let tables = Tables::new();
    for doc in fit {
        model.code_document(&tables, doc, true, |_, _, _, _| {});
    }
    holdout
        .iter()
        .map(|doc| model.code_document(&tables, doc, false, |_, _, _, _| {}).0)
        .sum()
}

/// One trace record (little-endian, `TRACE_RECORD_BYTES` bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceRecord {
    pub doc_index: u32,
    pub position: u32,
    /// 0 = TRAIN (prequential), 1 = DEV (frozen).
    pub split: u8,
    pub target: u8,
    pub word: u16,
    pub context: u64,
    pub logits: [i16; 8],
    pub confidence: u32,
}

impl TraceRecord {
    #[must_use]
    pub fn encode(&self) -> [u8; TRACE_RECORD_BYTES] {
        let mut out = [0u8; TRACE_RECORD_BYTES];
        out[0..4].copy_from_slice(&self.doc_index.to_le_bytes());
        out[4..8].copy_from_slice(&self.position.to_le_bytes());
        out[8] = self.split;
        out[9] = self.target;
        out[10..12].copy_from_slice(&self.word.to_le_bytes());
        out[12..20].copy_from_slice(&self.context.to_le_bytes());
        for (d, logit) in self.logits.iter().enumerate() {
            out[20 + 2 * d..22 + 2 * d].copy_from_slice(&logit.to_le_bytes());
        }
        out[36..40].copy_from_slice(&self.confidence.to_le_bytes());
        out
    }

    #[must_use]
    pub fn decode(raw: &[u8; TRACE_RECORD_BYTES]) -> Self {
        let u32_at = |i: usize| u32::from_le_bytes(raw[i..i + 4].try_into().expect("4 bytes"));
        let mut logits = [0i16; 8];
        for (d, logit) in logits.iter_mut().enumerate() {
            *logit = i16::from_le_bytes([raw[20 + 2 * d], raw[21 + 2 * d]]);
        }
        Self {
            doc_index: u32_at(0),
            position: u32_at(4),
            split: raw[8],
            target: raw[9],
            word: u16::from_le_bytes([raw[10], raw[11]]),
            context: u64::from_le_bytes(raw[12..20].try_into().expect("8 bytes")),
            logits,
            confidence: u32_at(36),
        }
    }

    /// Code length in bits of `target` under the recorded logits.
    #[must_use]
    pub fn cost_bits(&self) -> f64 {
        (0..8)
            .map(|d| {
                bit_cost(
                    squash_value(i32::from(self.logits[d])),
                    (self.target >> (7 - d)) & 1 != 0,
                )
            })
            .sum()
    }
}

/// Reads a trace stream; returns (records, total bits). Fails on a torn
/// record.
pub fn read_trace(
    input: &mut impl std::io::Read,
    mut each: impl FnMut(&TraceRecord),
) -> std::io::Result<(u64, f64)> {
    let mut reader = std::io::BufReader::new(input);
    let mut raw = [0u8; TRACE_RECORD_BYTES];
    let mut records = 0u64;
    let mut bits = 0.0;
    loop {
        let mut filled = 0;
        while filled < TRACE_RECORD_BYTES {
            let n = std::io::Read::read(&mut reader, &mut raw[filled..])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled == 0 {
            return Ok((records, bits));
        }
        if filled != TRACE_RECORD_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "torn trace record",
            ));
        }
        let record = TraceRecord::decode(&raw);
        bits += record.cost_bits();
        records += 1;
        each(&record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus(seed: u64, len: usize) -> Vec<u8> {
        // Order-3 structured text: a few words with deterministic choices.
        let words: [&[u8]; 6] = [b"the ", b"bank ", b"routes ", b"a ", b"bit ", b"slice "];
        let mut state = seed;
        let mut out = Vec::new();
        while out.len() < len {
            state = mix64(state.wrapping_add(0x9e37_79b9_7f4a_7c15));
            out.extend_from_slice(words[(state % 6) as usize]);
        }
        out.truncate(len);
        out
    }

    #[test]
    fn word_register_is_case_folded_and_resets() {
        assert_eq!(word_update(0, b'A'), word_update(0, b'a'));
        assert_eq!(word_update(0x1234, b' '), 0);
        let w = word_update(word_update(0, b'a'), b'b');
        assert_eq!(w, (u16::from(b'a').rotate_left(3)) ^ u16::from(b'b'));
    }

    #[test]
    fn squash_is_monotone_and_saturated() {
        assert_eq!(squash_value(0), 32768);
        assert_eq!(squash_value(10_000), squash_value(STRETCH_LIMIT));
        assert!(squash_value(-STRETCH_LIMIT) >= 1);
        for st in -STRETCH_LIMIT..STRETCH_LIMIT {
            assert!(squash_value(st) <= squash_value(st + 1));
        }
    }

    #[test]
    fn learning_lowers_cost_and_frozen_coding_keeps_weights() {
        let tables = Tables::new();
        let text = corpus(1, 40_000);
        let dev = corpus(2, 8_000);
        let mut model = Model::new(
            vec![0, order_mask(1), order_mask(2), order_mask(3)],
            MixRule::Average,
            UpdateRule::Adaptive,
            1 << 20,
        );
        let (cold, events) = model.code_document(&tables, &dev, false, |_, _, _, _| {});
        assert_eq!(events, 7_998);
        assert!((cold / events as f64 - 8.0).abs() < 1e-9);
        model.code_document(&tables, &text, true, |_, _, _, _| {});
        let sha = model.weights_sha();
        let (warm, _) = model.code_document(&tables, &dev, false, |_, _, _, _| {});
        assert_eq!(model.weights_sha(), sha, "DEV must not change the weights");
        assert!(warm / (events as f64) < 2.5, "bpc {}", warm / events as f64);
        assert!(model.touched_slots() > 0);
    }

    #[test]
    fn trace_roundtrip_reproduces_the_coded_bits() {
        let tables = Tables::new();
        let text = corpus(3, 20_000);
        let mut model = Model::new(
            vec![0, order_mask(2)],
            MixRule::HalfSum,
            UpdateRule::Stationary,
            1 << 18,
        );
        model.code_document(&tables, &text, true, |_, _, _, _| {});
        let mut buffer = Vec::new();
        let dev = corpus(4, 3_000);
        let (bits, events) = model.code_document(&tables, &dev, false, |pos, ctx, byte, code| {
            let record = TraceRecord {
                doc_index: 7,
                position: pos as u32,
                split: 1,
                target: byte,
                word: ctx.word,
                context: ctx.bytes,
                logits: code.logits,
                confidence: code.confidence,
            };
            assert_eq!(TraceRecord::decode(&record.encode()), record);
            buffer.extend_from_slice(&record.encode());
        });
        let mut first = None;
        let (records, traced) = read_trace(&mut buffer.as_slice(), |r| {
            first.get_or_insert(*r);
        })
        .unwrap();
        assert_eq!(records, events);
        assert!((traced - bits).abs() < 1e-6 * bits);
        let first = first.unwrap();
        assert_eq!(
            (first.position, first.context & 0xffff),
            (2, u64::from(dev[1]) | u64::from(dev[0]) << 8)
        );
        assert!(read_trace(&mut &buffer[..buffer.len() - 1], |_| {}).is_err());
    }

    #[test]
    fn greedy_search_selects_context_bits_and_beats_order0() {
        let fit = corpus(5, 60_000);
        let holdout = corpus(6, 12_000);
        let mut steps = Vec::new();
        let masks = search_masks(3, 1 << 20, &[&fit], &[&holdout], 2, |s| {
            steps.push(s.clone())
        });
        assert!(masks.len() >= 2 && masks[0] == 0 && masks[1] != 0);
        let order0 = steps[0].holdout_bpc;
        let last = steps.last().unwrap().holdout_bpc;
        assert!(last < order0 - 0.5, "{order0} -> {last}");
        // The searched allocation reproduces the search score exactly, also
        // when fewer banks than requested were kept.
        let mut rebuilt = Model::with_bank_bytes(
            masks.clone(),
            MixRule::Average,
            UpdateRule::Stationary,
            (1 << 20) / 3,
        );
        let events = (holdout.len() - 2) as f64;
        let rebuilt_bpc = holdout_cost(&mut rebuilt, &[&fit], &[&holdout]) / events;
        assert!((rebuilt_bpc - last).abs() < 1e-9, "{rebuilt_bpc} vs {last}");
        let mut a = Model::new(
            masks.clone(),
            MixRule::Average,
            UpdateRule::Adaptive,
            1 << 20,
        );
        let mut b = Model::new(vec![0], MixRule::Average, UpdateRule::Adaptive, 1 << 20);
        assert!(
            holdout_cost(&mut a, &[&fit], &[&holdout]) < holdout_cost(&mut b, &[&fit], &[&holdout])
        );
    }
}
