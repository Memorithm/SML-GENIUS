//! ENG-1d/BMX: adaptive Boolean context-mixing bit predictor.
//!
//! Predicts each bit of a byte stream (most significant bit first) from
//! several hashed byte contexts, a match model and an order-0 context, mixes
//! the predictions in the logistic domain with online-learned weights, then
//! refines the result with an adaptive probability map. Everything learns
//! while coding: there is no frozen TRAIN count table.
//!
//! [`Architecture::V4`] (`bmx-v4`, the default) adds two things to the
//! classic `bmx-v2`/`bmx-v3` design, at the same table memory:
//! * collision-checked context slots: each slot stores 5 check bits of its
//!   context hash; a slot whose check differs is read as unseen and reset on
//!   update, instead of lending another context's statistics;
//! * three mixer weight sets, selected by the partial byte, by the previous
//!   byte × match state, and by the classes (bits 7-5) of the two previous
//!   bytes × bit position, combined by a small learned final mixer.
//!
//! Determinism: integer arithmetic only (no floating point anywhere in the
//! prediction or update path), fixed hash functions and fixed table sizes.
//! Two predictors built with the same [`MixerConfig`] and fed the same bits
//! produce identical probabilities on any target, which is what the
//! arithmetic decoder requires.
//!
//! This module predicts; it makes no quality claim. Quality is measured by
//! the coded size (see `bool_coder`).

use crate::bool_coder::{BitPredictor, P_MAX, P_MIN};

/// Context orders (number of preceding whole bytes) hashed as inputs.
pub const ORDERS: [usize; 6] = [1, 2, 3, 4, 6, 8];
/// Fixed hashed contexts: the orders plus the word context.
const N_FIXED: usize = ORDERS.len() + 1;
/// Non-hashed inputs: order 0, match model, bias.
const N_EXTRA: usize = 3;
/// Most Boolean mask contexts a predictor may carry.
pub const MAX_MASKS: usize = 16;

/// A Boolean byte-mask context: `mask[j]` is ANDed with the byte `j + 1`
/// positions back, and the masked bytes are hashed together. Each mask
/// selects an explicit set of context bits (a Boolean sub-cube of the last
/// 8 bytes): skip contexts, case-folded or character-class sequences.
pub type ByteMask = [u8; 8];

/// Boolean mask contexts selected by `sml-bmx-mask-search` on the tuning
/// corpora only (Opticks DEV with TRAIN priming, Vim `version7.txt`), then
/// frozen. See docs/ENG-1D-BOOLEAN-CONTEXT-MIXING.md.
pub const V3_MASKS: &[ByteMask] = &[
    [0xE0, 0xE0, 0xE0, 0xE0, 0xE0, 0xE0, 0xE0, 0xE0],
    [0xFF, 0xE0, 0xE0, 0xE0, 0xE0, 0xE0, 0x00, 0x00],
];

/// An external bit predictor fed to the mixer as extra inputs (for example
/// the SML recurrent ANF byte model, see `anf_side`). It follows the byte
/// stream through [`SideModel::push_byte`] and, for each chain bit, returns
/// a 12-bit probability of a 1 and a context address.
pub trait SideModel: Send {
    /// Stable identity, recorded in coded containers.
    fn label(&self) -> String;
    /// Called after every completed byte (priming and coding alike).
    fn push_byte(&mut self, byte: u8);
    /// For chain bit `bit_pos` (0 = most significant) of the next byte,
    /// given `c0` = 1 followed by the bits already coded: `(p12, address)`.
    fn predict(&self, bit_pos: u32, c0: u32) -> (i32, u32);
}

/// Which side-model signals enter the mixer (for ablation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideInputs {
    /// Calibrated side probability and adaptive counter on its address.
    Both,
    /// Only the adaptive counter keyed by the side address.
    AddressOnly,
    /// Only the calibrated side probability.
    ProbabilityOnly,
}

impl SideInputs {
    fn label(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::AddressOnly => "address",
            Self::ProbabilityOnly => "probability",
        }
    }
}

/// Predictor architecture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Architecture {
    /// One weight set selected by the partial byte, unchecked slots
    /// (`bmx-v2` / `bmx-v3`).
    Classic,
    /// Collision-checked slots and three weight sets with a learned final
    /// mixer (`bmx-v4`).
    V4,
}

/// Number of check bits stored in each `bmx-v4` context slot.
const CHECK_BITS: u32 = 5;
const CHECK_MASK: u32 = (1 << CHECK_BITS) - 1;
/// Weight sets of the `bmx-v4` mixer: partial byte, previous byte × match
/// state, byte classes of t-1 and t-2 × bit position.
const SETS_V4: [usize; 3] = [256, 256 * 4, 8 * 8 * 8];
/// Final-mixer inputs: the three set outputs and a bias.
const FINAL_INPUTS: usize = SETS_V4.len() + 1;

/// Which Boolean mask contexts the predictor uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskSet {
    /// No mask contexts (the `bmx-v2` model).
    None,
    /// The frozen [`V3_MASKS`].
    V3,
}

impl MaskSet {
    #[must_use]
    pub fn masks(self) -> &'static [ByteMask] {
        match self {
            Self::None => &[],
            Self::V3 => V3_MASKS,
        }
    }
}

/// Stable hexadecimal label of a mask list (`-` when empty).
#[must_use]
pub fn masks_label(masks: &[ByteMask]) -> String {
    if masks.is_empty() {
        return "-".into();
    }
    masks
        .iter()
        .map(|m| m.iter().map(|b| format!("{b:02x}")).collect::<String>())
        .collect::<Vec<_>>()
        .join(",")
}
const MATCH_MIN: usize = 6;
const MAX_MATCH: usize = 65_535;
const CONFIDENCE_BUCKETS: usize = 16;

fn confidence_bucket(n: u32) -> usize {
    match n {
        0..=7 => n as usize,
        8..=11 => 8,
        12..=15 => 9,
        16..=23 => 10,
        24..=31 => 11,
        32..=63 => 12,
        64..=127 => 13,
        128..=255 => 14,
        _ => 15,
    }
}

/// Table sizes and learning limits. The default is the frozen ENG-1d/BMX
/// configuration; changing any field changes the model identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MixerConfig {
    /// log2 of slots per hashed context table.
    pub table_bits: u32,
    /// log2 of the match-model hash index.
    pub match_bits: u32,
    /// Counter adaptation limit (higher = more stationary).
    pub counter_limit: u32,
    /// Mixer learning rate, in 1/16 steps of the classic `err * rate >> 10`.
    pub mixer_rate: i32,
    /// Boolean mask contexts.
    pub mask_set: MaskSet,
    /// Mixer architecture.
    pub architecture: Architecture,
}

impl Default for MixerConfig {
    fn default() -> Self {
        Self {
            table_bits: 22,
            match_bits: 20,
            counter_limit: 127,
            mixer_rate: 6,
            mask_set: MaskSet::V3,
            architecture: Architecture::V4,
        }
    }
}

impl MixerConfig {
    /// Every released configuration, newest first: `bmx-v4`, `bmx-v3`,
    /// `bmx-v2` (for decoding a container by its stored identity).
    #[must_use]
    pub fn released() -> [Self; 3] {
        let v4 = Self::default();
        let v3 = Self {
            architecture: Architecture::Classic,
            ..v4
        };
        let v2 = Self {
            mask_set: MaskSet::None,
            ..v3
        };
        [v4, v3, v2]
    }

    /// Stable text identity, recorded in coded containers.
    #[must_use]
    pub fn label(&self) -> String {
        Self::label_with(self, self.mask_set.masks())
    }

    /// Identity with an explicit mask list (used by mask search).
    #[must_use]
    pub fn label_with(&self, masks: &[ByteMask]) -> String {
        let base = format!(
            "orders=1,2,3,4,6,8+word+o0+match;table_bits={};match_bits={};limit={};rate={}",
            self.table_bits, self.match_bits, self.counter_limit, self.mixer_rate
        );
        // Classic identities are byte-identical to the released bmx-v2 and
        // bmx-v3 labels, so existing containers stay decodable.
        match (self.architecture, masks.is_empty()) {
            (Architecture::Classic, true) => format!("bmx-v2:{base}"),
            (Architecture::Classic, false) => {
                format!("bmx-v3:{base};masks={}", masks_label(masks))
            }
            (Architecture::V4, _) => format!(
                "bmx-v4:{base};masks={};check_bits={CHECK_BITS};sets=c0,c1xmatch,class2xbit;final=learned",
                masks_label(masks)
            ),
        }
    }

    fn valid(&self) -> bool {
        (10..=24).contains(&self.table_bits)
            && (10..=24).contains(&self.match_bits)
            && (1..=1023).contains(&self.counter_limit)
            // Checked slots store a 7-bit count.
            && (self.architecture == Architecture::Classic || self.counter_limit <= 127)
            && (1..=1024).contains(&self.mixer_rate)
    }
}

// ------------------------------------------------------------- logistic

/// Fixed logistic tables on the 12-bit probability scale.
#[derive(Clone)]
struct Logistic {
    stretch: Vec<i16>,
}

/// squash(d) = 4096 / (1 + e^-d/256), piecewise linear between 33 knots.
fn squash(d: i32) -> i32 {
    const KNOTS: [i32; 33] = [
        1, 2, 3, 6, 10, 16, 27, 45, 73, 120, 194, 310, 488, 747, 1101, 1546, 2047, 2549, 2994,
        3348, 3607, 3785, 3901, 3975, 4022, 4050, 4068, 4079, 4085, 4089, 4092, 4093, 4094,
    ];
    if d > 2047 {
        return 4095;
    }
    if d < -2047 {
        return 1;
    }
    let w = d & 127;
    let i = ((d >> 7) + 16) as usize;
    (KNOTS[i] * (128 - w) + KNOTS[i + 1] * w + 64) >> 7
}

impl Logistic {
    fn new() -> Self {
        // stretch = inverse of squash on 0..4095.
        let mut stretch = vec![0i16; 4096];
        let mut next = 0usize;
        for x in -2047..=2047 {
            let v = squash(x) as usize;
            for slot in stretch.iter_mut().take(v + 1).skip(next) {
                *slot = x as i16;
            }
            next = next.max(v + 1);
        }
        for slot in stretch.iter_mut().skip(next) {
            *slot = 2047;
        }
        Self { stretch }
    }

    fn stretch(&self, p: i32) -> i32 {
        i32::from(self.stretch[p.clamp(0, 4095) as usize])
    }
}

// ------------------------------------------------------------- counters

/// Adaptive probability slots: 22-bit probability << 10 | 10-bit count.
#[derive(Clone)]
struct CounterTable {
    slots: Vec<u32>,
    mask: usize,
}

const COUNTER_INIT: u32 = (1 << 31) & !1023;

fn reciprocal(n: u32) -> i64 {
    // 2^16 * 2 / (2n + 3): learning step 1/(n + 1.5).
    131_072 / (2 * i64::from(n) + 3)
}

impl CounterTable {
    fn new(bits: u32) -> Self {
        Self {
            slots: vec![COUNTER_INIT; 1 << bits],
            mask: (1 << bits) - 1,
        }
    }

    /// 12-bit probability of a 1 at `index`.
    fn p(&self, index: usize) -> i32 {
        (self.slots[index & self.mask] >> 20) as i32
    }

    /// Number of updates seen by the slot (saturating at the limit).
    fn count(&self, index: usize) -> u32 {
        self.slots[index & self.mask] & 1023
    }

    /// Checked slot layout (`bmx-v4`): 20-bit probability << 12 | 7-bit
    /// count << 5 | 5 check bits. Returns `(p12, count)`; a slot holding
    /// another check value reads as unseen.
    fn p_checked(&self, index: usize, check: u32) -> (i32, u32) {
        let slot = self.slots[index & self.mask];
        if slot & CHECK_MASK != check {
            return (2048, 0);
        }
        ((slot >> 20) as i32, (slot >> CHECK_BITS) & 127)
    }

    fn update_checked(&mut self, index: usize, check: u32, bit: bool, limit: u32) {
        let slot = &mut self.slots[index & self.mask];
        if *slot & CHECK_MASK != check {
            *slot = (1 << 31) | check;
        }
        let limit = limit.min(127);
        let n = (*slot >> CHECK_BITS) & 127;
        let p = i64::from(*slot >> 12);
        let target = if bit { (1i64 << 20) - 1 } else { 0 };
        let p = (p + (((target - p) * reciprocal(n)) >> 16)).clamp(0, (1 << 20) - 1);
        let n = if n < limit { n + 1 } else { limit };
        *slot = ((p as u32) << 12) | (n << CHECK_BITS) | check;
    }

    fn update(&mut self, index: usize, bit: bool, limit: u32) {
        let slot = &mut self.slots[index & self.mask];
        let n = *slot & 1023;
        let p = i64::from(*slot >> 10);
        let target = if bit { (1i64 << 22) - 1 } else { 0 };
        let p = (p + (((target - p) * reciprocal(n)) >> 16)).clamp(0, (1 << 22) - 1);
        let n = if n < limit { n + 1 } else { limit };
        *slot = ((p as u32) << 10) | n;
    }
}

// ------------------------------------------------------------ APM / SSE

/// Adaptive probability map: refines a probability given a small context
/// by interpolating between 33 learned points on the stretch axis.
#[derive(Clone)]
struct Apm {
    table: Vec<u16>,
    index: usize,
}

impl Apm {
    fn new(contexts: usize) -> Self {
        let mut table = vec![0u16; contexts * 33];
        for (i, slot) in table.iter_mut().enumerate() {
            let x = ((i % 33) as i32 - 16) * 128;
            *slot = (squash(x) * 16) as u16;
        }
        Self { table, index: 0 }
    }

    fn refine(&mut self, p: i32, context: usize, logistic: &Logistic) -> i32 {
        let s = logistic.stretch(p) + 2048;
        let lo = (s >> 7) as usize;
        let w = s & 127;
        self.index = context * 33 + lo + usize::from(w >= 64);
        let base = context * 33 + lo;
        (i32::from(self.table[base]) * (128 - w) + i32::from(self.table[base + 1]) * w) >> 11
    }

    fn update(&mut self, bit: bool) {
        let target = if bit { 65_535 } else { 0 };
        let slot = &mut self.table[self.index];
        let v = i32::from(*slot);
        *slot = (v + ((target - v) >> 7)) as u16;
    }
}

// ---------------------------------------------------------------- hashing

fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^ (h >> 13)
}

/// Dot product of `inputs` with weight row `row` of `weights`.
fn dot_row(inputs: &[i32], weights: &[i32], row: usize) -> i64 {
    let n = inputs.len();
    inputs
        .iter()
        .zip(&weights[row * n..(row + 1) * n])
        .map(|(&x, &w)| i64::from(x) * i64::from(w))
        .sum()
}

/// One gradient step on weight row `row` for scaled error `err`.
fn train_row(weights: &mut [i32], inputs: &[i32], row: usize, err: i32) {
    let n = inputs.len();
    for (w, &x) in weights[row * n..(row + 1) * n].iter_mut().zip(inputs) {
        *w = w.saturating_add(((i64::from(x) * i64::from(err) + 0x2000) >> 14) as i32);
    }
}

// ------------------------------------------------------------- predictor

/// The adaptive Boolean predictor. Feed it bits in coding order.
pub struct BitMixer {
    config: MixerConfig,
    logistic: Logistic,
    tables: Vec<CounterTable>,
    /// Per-input confidence maps: refine each hashed prediction given how
    /// often its slot has been seen, so a context seen once on noisy data is
    /// not trusted like a well-observed one (indirect learning).
    adapters: Vec<Apm>,
    order0: CounterTable,
    match_counters: CounterTable,
    weights: Vec<i32>,
    apm_order1: Apm,
    apm_order2: Apm,
    history: Vec<u8>,
    match_index: Vec<u32>,
    match_ptr: usize,
    match_len: usize,
    masks: Vec<ByteMask>,
    side: Option<Box<dyn SideModel>>,
    side_inputs: SideInputs,
    side_apm: Apm,
    side_table: CounterTable,
    side_adapter: Apm,
    side_slot: usize,
    n_hashed: usize,
    byte_hashes: Vec<u32>,
    word_hash: u32,
    c0: u32,
    bit_pos: u32,
    // per-bit state
    inputs: Vec<i32>,
    slots: Vec<usize>,
    /// Check values of the current slots (`bmx-v4`).
    checks: Vec<u32>,
    /// `bmx-v4` extra weight sets (empty for the classic architecture).
    extra_weights: [Vec<i32>; 2],
    /// Selected row of each `bmx-v4` weight set.
    selectors: [usize; 3],
    /// Stretched output and probability of each `bmx-v4` weight set.
    set_out: [i32; 3],
    set_p: [i32; 3],
    final_weights: Vec<i32>,
    match_slot: usize,
    mixer_p: i32,
    final_p: i32,
}

impl BitMixer {
    /// Builds a fresh predictor. Panics only on an invalid configuration,
    /// which [`Self::try_new`] reports instead.
    #[must_use]
    pub fn new(config: MixerConfig) -> Self {
        Self::try_new(config).expect("invalid MixerConfig")
    }

    pub fn try_new(config: MixerConfig) -> Result<Self, &'static str> {
        Self::try_with_masks(config, config.mask_set.masks())
    }

    /// Builds a predictor with an explicit Boolean mask list, overriding
    /// `config.mask_set` (for mask search; coded containers must then use
    /// [`MixerConfig::label_with`] as identity).
    pub fn try_with_masks(config: MixerConfig, masks: &[ByteMask]) -> Result<Self, &'static str> {
        Self::build(config, masks, None, SideInputs::Both)
    }

    /// Builds a predictor that also mixes an external [`SideModel`].
    pub fn try_with_side(
        config: MixerConfig,
        masks: &[ByteMask],
        side: Box<dyn SideModel>,
        inputs: SideInputs,
    ) -> Result<Self, &'static str> {
        Self::build(config, masks, Some(side), inputs)
    }

    /// Identity of this predictor: configuration, masks and side model.
    #[must_use]
    pub fn identity(&self) -> String {
        let base = self.config.label_with(&self.masks);
        match &self.side {
            Some(side) => format!(
                "{base};side={};side_inputs={}",
                side.label(),
                self.side_inputs.label()
            ),
            None => base,
        }
    }

    fn build(
        config: MixerConfig,
        masks: &[ByteMask],
        side: Option<Box<dyn SideModel>>,
        side_inputs: SideInputs,
    ) -> Result<Self, &'static str> {
        if !config.valid() {
            return Err("invalid MixerConfig");
        }
        if masks.len() > MAX_MASKS || masks.iter().any(|m| m.iter().all(|&b| b == 0)) {
            return Err("invalid Boolean mask list");
        }
        let n_hashed = N_FIXED + masks.len();
        let side_present = side.is_some();
        let n_inputs = n_hashed + N_EXTRA + if side_present { 2 } else { 0 };
        let logistic = Logistic::new();
        let apm_order1 = Apm::new(256 * 256);
        let apm_order2 = Apm::new(1 << 16);
        let mut mixer = Self {
            config,
            tables: (0..n_hashed)
                .map(|_| CounterTable::new(config.table_bits))
                .collect(),
            adapters: (0..n_hashed)
                .map(|_| Apm::new(CONFIDENCE_BUCKETS))
                .collect(),
            order0: CounterTable::new(8),
            match_counters: CounterTable::new(16),
            weights: vec![0; 256 * n_inputs],
            apm_order1,
            apm_order2,
            logistic,
            history: Vec::new(),
            match_index: vec![0; 1 << config.match_bits],
            match_ptr: 0,
            match_len: 0,
            masks: masks.to_vec(),
            side,
            side_inputs,
            side_apm: Apm::new(8),
            // Side state is only sized when a side model is present.
            side_table: CounterTable::new(if side_present {
                config.table_bits.min(20)
            } else {
                1
            }),
            side_adapter: Apm::new(CONFIDENCE_BUCKETS),
            side_slot: 0,
            n_hashed,
            byte_hashes: vec![0; n_hashed],
            word_hash: 0,
            c0: 1,
            bit_pos: 0,
            inputs: vec![0; n_inputs],
            slots: vec![0; n_hashed],
            checks: vec![0; n_hashed],
            extra_weights: match config.architecture {
                Architecture::Classic => [Vec::new(), Vec::new()],
                Architecture::V4 => [
                    vec![0; SETS_V4[1] * n_inputs],
                    vec![0; SETS_V4[2] * n_inputs],
                ],
            },
            selectors: [0; 3],
            set_out: [0; 3],
            set_p: [2048; 3],
            final_weights: match config.architecture {
                Architecture::Classic => Vec::new(),
                // Start as the plain average of the three sets, no bias.
                Architecture::V4 => (0..256 * FINAL_INPUTS)
                    .map(|i| if i % FINAL_INPUTS < 3 { 21_845 } else { 0 })
                    .collect(),
            },
            match_slot: 0,
            mixer_p: 2048,
            final_p: 2048,
        };
        let initial = (1 << 16) / n_inputs as i32;
        for w in mixer
            .weights
            .iter_mut()
            .chain(mixer.extra_weights.iter_mut().flatten())
        {
            *w = initial;
        }
        mixer.predict();
        Ok(mixer)
    }

    #[must_use]
    pub fn config(&self) -> MixerConfig {
        self.config
    }

    /// Learns from `bytes` exactly as if they had been coded (priming).
    pub fn prime(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            for shift in (0..8).rev() {
                self.update((byte >> shift) & 1 == 1);
            }
        }
    }

    /// 12-bit probability that the next bit is 1 (1..=4095).
    #[must_use]
    pub fn p12(&self) -> i32 {
        self.final_p
    }

    fn expected_bit(&self) -> Option<bool> {
        if self.match_len == 0 {
            return None;
        }
        let predicted = u32::from(self.history[self.match_ptr]) | 256;
        // Only while the bits coded so far agree with the predicted byte.
        if (predicted >> (8 - self.bit_pos)) != self.c0 {
            return None;
        }
        Some((predicted >> (7 - self.bit_pos)) & 1 == 1)
    }

    fn predict(&mut self) {
        let c0 = self.c0;
        let checked = self.config.architecture == Architecture::V4;
        for (i, slot) in self.slots.iter_mut().enumerate() {
            let h = hash(self.byte_hashes[i], c0);
            *slot = h as usize;
            let (raw, count) = if checked {
                // Check bits come from the top of the hash; the slot index
                // uses at most the low 24 bits.
                self.checks[i] = (h >> (32 - CHECK_BITS)) & CHECK_MASK;
                self.tables[i].p_checked(*slot, self.checks[i])
            } else {
                (self.tables[i].p(*slot), self.tables[i].count(*slot))
            };
            let bucket = confidence_bucket(count);
            let adapted = self.adapters[i].refine(raw, bucket, &self.logistic);
            self.inputs[i] = self.logistic.stretch(adapted.clamp(1, 4095));
        }
        let n_hashed = self.n_hashed;
        self.inputs[n_hashed] = self.logistic.stretch(self.order0.p(c0 as usize));
        self.inputs[n_hashed + 1] = match self.expected_bit() {
            Some(bit) => {
                let len_bucket = self.match_len.min(31);
                self.match_slot = (len_bucket << 1) | usize::from(bit);
                self.logistic
                    .stretch(self.match_counters.p(self.match_slot))
            }
            None => {
                self.match_slot = usize::MAX;
                0
            }
        };
        self.inputs[n_hashed + 2] = 256;
        if let Some(side) = &self.side {
            let (p, address) = side.predict(self.bit_pos, c0);
            let calibrated =
                self.side_apm
                    .refine(p.clamp(1, 4095), self.bit_pos as usize, &self.logistic);
            self.inputs[n_hashed + 3] = if self.side_inputs == SideInputs::AddressOnly {
                0
            } else {
                self.logistic.stretch(calibrated.clamp(1, 4095))
            };
            self.side_slot = hash(address, 0x51DE_0001) as usize;
            let raw = self.side_table.p(self.side_slot);
            let bucket = confidence_bucket(self.side_table.count(self.side_slot));
            let adapted = self.side_adapter.refine(raw, bucket, &self.logistic);
            self.inputs[n_hashed + 4] = if self.side_inputs == SideInputs::ProbabilityOnly {
                0
            } else {
                self.logistic.stretch(adapted.clamp(1, 4095))
            };
        }
        let c1 = usize::from(self.history.last().copied().unwrap_or(0));
        let c2 = usize::from(if self.history.len() >= 2 {
            self.history[self.history.len() - 2]
        } else {
            0
        });
        let dot = dot_row(&self.inputs, &self.weights, c0 as usize);
        match self.config.architecture {
            Architecture::Classic => {
                self.mixer_p = squash((dot >> 16) as i32).clamp(1, 4095);
            }
            Architecture::V4 => {
                let match_state = match (self.match_slot != usize::MAX, self.match_len) {
                    (false, _) => 0,
                    (true, 0..=15) => 1,
                    (true, 16..=31) => 2,
                    _ => 3,
                };
                self.selectors = [
                    c0 as usize,
                    (c1 << 2) | match_state,
                    ((c2 >> 5) << 6) | ((c1 >> 5) << 3) | self.bit_pos as usize,
                ];
                let dots = [
                    dot,
                    dot_row(&self.inputs, &self.extra_weights[0], self.selectors[1]),
                    dot_row(&self.inputs, &self.extra_weights[1], self.selectors[2]),
                ];
                for ((out, p), dot) in self.set_out.iter_mut().zip(&mut self.set_p).zip(dots) {
                    *out = ((dot >> 16) as i32).clamp(-2047, 2047);
                    *p = squash(*out).clamp(1, 4095);
                }
                let row = &self.final_weights[c0 as usize * FINAL_INPUTS..][..FINAL_INPUTS];
                let final_in = [self.set_out[0], self.set_out[1], self.set_out[2], 256];
                let d: i64 = final_in
                    .iter()
                    .zip(row)
                    .map(|(&x, &w)| i64::from(x) * i64::from(w))
                    .sum();
                self.mixer_p = squash(((d >> 16) as i32).clamp(-2047, 2047)).clamp(1, 4095);
            }
        }
        let a1 = self
            .apm_order1
            .refine(self.mixer_p, (c1 << 8) | c0 as usize, &self.logistic);
        let h2 = (hash((c2 << 8 | c1) as u32, c0) & 0xFFFF) as usize;
        let a2 = self.apm_order2.refine(self.mixer_p, h2, &self.logistic);
        self.final_p = ((2 * self.mixer_p + a1 + a2 + 2) >> 2).clamp(1, 4095);
    }

    /// Learns `bit` and prepares the next prediction.
    pub fn update(&mut self, bit: bool) {
        let limit = self.config.counter_limit;
        // Mixer.
        let rate = self.config.mixer_rate;
        let target = i32::from(bit) << 12;
        match self.config.architecture {
            Architecture::Classic => {
                let err = (target - self.mixer_p) * rate;
                train_row(&mut self.weights, &self.inputs, self.c0 as usize, err);
            }
            Architecture::V4 => {
                let err = (target - self.set_p[0]) * rate;
                train_row(&mut self.weights, &self.inputs, self.selectors[0], err);
                for ((weights, &row), &p) in self
                    .extra_weights
                    .iter_mut()
                    .zip(&self.selectors[1..])
                    .zip(&self.set_p[1..])
                {
                    train_row(weights, &self.inputs, row, (target - p) * rate);
                }
                let final_in = [self.set_out[0], self.set_out[1], self.set_out[2], 256];
                let err = (target - self.mixer_p) * 2;
                let row =
                    &mut self.final_weights[self.selectors[0] * FINAL_INPUTS..][..FINAL_INPUTS];
                for (w, &x) in row.iter_mut().zip(&final_in) {
                    *w = w.saturating_add(((i64::from(x) * i64::from(err) + 0x2000) >> 14) as i32);
                }
            }
        }
        // Counters and maps.
        let checked = self.config.architecture == Architecture::V4;
        for (i, &slot) in self.slots.iter().enumerate() {
            if checked {
                self.tables[i].update_checked(slot, self.checks[i], bit, limit);
            } else {
                self.tables[i].update(slot, bit, limit);
            }
            self.adapters[i].update(bit);
        }
        self.order0.update(self.c0 as usize, bit, limit);
        if self.side.is_some() {
            self.side_apm.update(bit);
            self.side_table.update(self.side_slot, bit, limit);
            self.side_adapter.update(bit);
        }
        if self.match_slot != usize::MAX {
            self.match_counters.update(self.match_slot, bit, 1023);
        }
        self.apm_order1.update(bit);
        self.apm_order2.update(bit);
        if let Some(expected) = self.expected_bit() {
            if expected != bit {
                self.match_len = 0;
            }
        }
        // Bit-tree state.
        self.c0 = (self.c0 << 1) | u32::from(bit);
        self.bit_pos += 1;
        if self.bit_pos == 8 {
            let byte = (self.c0 & 0xFF) as u8;
            self.end_of_byte(byte);
            self.c0 = 1;
            self.bit_pos = 0;
        }
        self.predict();
    }

    fn end_of_byte(&mut self, byte: u8) {
        self.history.push(byte);
        if let Some(side) = &mut self.side {
            side.push_byte(byte);
        }
        let n = self.history.len();
        // Match model: extend or look up.
        if self.match_len > 0 {
            self.match_ptr += 1;
            self.match_len = (self.match_len + 1).min(MAX_MATCH);
        }
        if n >= MATCH_MIN {
            let key = self.history[n - MATCH_MIN..]
                .iter()
                .fold(0u32, |h, &b| hash(h, u32::from(b)));
            let slot = (key as usize) & ((1 << self.config.match_bits) - 1);
            if self.match_len == 0 {
                let candidate = self.match_index[slot] as usize;
                if candidate > 0 && candidate < n {
                    let mut len = 0;
                    while len < MAX_MATCH
                        && len < candidate
                        && self.history[candidate - 1 - len] == self.history[n - 1 - len]
                    {
                        len += 1;
                    }
                    if len >= MATCH_MIN {
                        self.match_len = len;
                        self.match_ptr = candidate;
                    }
                }
            }
            self.match_index[slot] = n as u32;
        }
        if self.match_len > 0 && self.match_ptr >= n {
            self.match_len = 0;
        }
        // Word context: letters, case-folded; reset on non-letters.
        let lower = byte.to_ascii_lowercase();
        self.word_hash = if lower.is_ascii_alphabetic() || byte >= 128 {
            hash(self.word_hash, u32::from(lower))
        } else {
            0
        };
        // Order-k hashes over the last k bytes.
        for (i, &order) in ORDERS.iter().enumerate() {
            let start = n.saturating_sub(order);
            let h = self.history[start..]
                .iter()
                .fold(order as u32 * 0x1000_0001, |h, &b| hash(h, u32::from(b)));
            self.byte_hashes[i] = h;
        }
        self.byte_hashes[ORDERS.len()] = hash(self.word_hash, 0x5741_5244);
        // Boolean mask contexts over the last 8 bytes.
        for (k, mask) in self.masks.iter().enumerate() {
            let mut h = (k as u32 + 1).wrapping_mul(0x2545_F491);
            for (j, &m) in mask.iter().enumerate() {
                if m != 0 {
                    let b = if j < n {
                        self.history[n - 1 - j] & m
                    } else {
                        0
                    };
                    h = hash(h, ((j as u32) << 8) | u32::from(b));
                }
            }
            self.byte_hashes[N_FIXED + k] = h;
        }
    }
}

impl BitPredictor for BitMixer {
    fn p1(&self) -> u32 {
        ((self.final_p as u32) << 20).clamp(P_MIN, P_MAX)
    }

    fn update(&mut self, bit: bool) {
        BitMixer::update(self, bit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bool_coder::{decode_with_predictor, encode_with_predictor};
    use crate::toy_tasks::SplitMix64;

    fn small() -> MixerConfig {
        MixerConfig {
            table_bits: 16,
            match_bits: 14,
            ..MixerConfig::default()
        }
    }

    fn text(len: usize) -> Vec<u8> {
        let words = [
            "the ",
            "light ",
            "of ",
            "colours ",
            "is ",
            "refracted ",
            "by ",
            "a ",
            "prism ",
            "and ",
            "reflected ",
            "from ",
            "bodies. ",
        ];
        let mut rng = SplitMix64::new(5);
        let mut out = Vec::new();
        while out.len() < len {
            out.extend_from_slice(words[rng.below(words.len())].as_bytes());
        }
        out.truncate(len);
        out
    }

    #[test]
    fn logistic_tables_are_monotone_inverses() {
        let logistic = Logistic::new();
        let mut previous = 0;
        for d in -2047..=2047 {
            let p = squash(d);
            assert!(p >= previous && (1..=4095).contains(&p));
            previous = p;
        }
        for p in 1..4095 {
            let back = squash(logistic.stretch(p));
            assert!((back - p).abs() <= 40, "p={p} back={back}");
        }
        assert!(logistic.stretch(1) < -1500 && logistic.stretch(4094) > 1500);
    }

    #[test]
    fn counters_converge_and_respect_limits() {
        let mut table = CounterTable::new(4);
        for _ in 0..2000 {
            table.update(3, true, 255);
        }
        assert!(table.p(3) > 4000);
        for _ in 0..2000 {
            table.update(3, false, 255);
        }
        assert!(table.p(3) < 100);
        assert_eq!(table.slots[3] & 1023, 255);
    }

    #[test]
    fn identical_inputs_give_identical_predictions() {
        let data = text(20_000);
        let mut a = BitMixer::new(small());
        let mut b = BitMixer::new(small());
        for &byte in &data {
            for shift in (0..8).rev() {
                assert_eq!(a.p12(), b.p12());
                let bit = (byte >> shift) & 1 == 1;
                a.update(bit);
                b.update(bit);
            }
        }
        assert!((1..=4095).contains(&a.p12()));
    }

    #[test]
    fn learns_redundant_text_and_round_trips() {
        let data = text(40_000);
        let id = small().label();
        let encoded =
            encode_with_predictor(&data, id.as_bytes(), &mut BitMixer::new(small())).unwrap();
        let decoded = decode_with_predictor(
            &encoded.container,
            id.as_bytes(),
            &mut BitMixer::new(small()),
        )
        .unwrap();
        assert_eq!(decoded, data);
        let bpb = 8.0 * encoded.payload_bytes as f64 / data.len() as f64;
        // 13 equiprobable words: about 3.7 bits per word of ~5.6 bytes.
        assert!(bpb < 1.2, "{bpb}");
    }

    #[test]
    fn priming_helps_and_must_match_on_both_sides() {
        let all = text(60_000);
        let (prefix, data) = all.split_at(40_000);
        let id = small().label();
        let cold = encode_with_predictor(data, id.as_bytes(), &mut BitMixer::new(small())).unwrap();
        let mut primed = BitMixer::new(small());
        primed.prime(prefix);
        let warm = encode_with_predictor(data, id.as_bytes(), &mut primed).unwrap();
        assert!(warm.payload_bytes < cold.payload_bytes);
        let mut decoder = BitMixer::new(small());
        decoder.prime(prefix);
        assert_eq!(
            decode_with_predictor(&warm.container, id.as_bytes(), &mut decoder).unwrap(),
            data
        );
        // An unprimed decoder cannot silently produce wrong data.
        assert!(
            decode_with_predictor(&warm.container, id.as_bytes(), &mut BitMixer::new(small()))
                .is_err()
        );
    }

    #[test]
    fn random_bytes_cost_close_to_eight_bits() {
        let mut rng = SplitMix64::new(3);
        let data: Vec<u8> = (0..30_000).map(|_| rng.next_u64() as u8).collect();
        let id = small().label();
        let encoded =
            encode_with_predictor(&data, id.as_bytes(), &mut BitMixer::new(small())).unwrap();
        let bpb = 8.0 * encoded.payload_bytes as f64 / data.len() as f64;
        assert!(bpb < 8.15, "{bpb}");
        let decoded = decode_with_predictor(
            &encoded.container,
            id.as_bytes(),
            &mut BitMixer::new(small()),
        )
        .unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn boolean_mask_contexts_change_predictions_and_round_trip() {
        let data = text(30_000);
        let v2 = MixerConfig {
            mask_set: MaskSet::None,
            architecture: Architecture::Classic,
            ..small()
        };
        let v3 = MixerConfig {
            mask_set: MaskSet::V3,
            architecture: Architecture::Classic,
            ..small()
        };
        assert_eq!(V3_MASKS.len(), 2);
        assert!(v3.label().starts_with("bmx-v3:") && v3.label().contains("e0e0e0e0e0e0e0e0"));
        let mut a = BitMixer::new(v2);
        let mut b = BitMixer::new(v3);
        let mut differ = 0usize;
        for &byte in &data[..2_000] {
            for shift in (0..8).rev() {
                differ += usize::from(a.p12() != b.p12());
                let bit = (byte >> shift) & 1 == 1;
                a.update(bit);
                b.update(bit);
            }
        }
        assert!(differ > 1_000, "mask inputs must influence predictions");
        let id = v3.label();
        let encoded = encode_with_predictor(&data, id.as_bytes(), &mut BitMixer::new(v3)).unwrap();
        // A v2 decoder under the v3 identity is detected, never silently wrong.
        assert!(
            decode_with_predictor(&encoded.container, id.as_bytes(), &mut BitMixer::new(v2))
                .is_err()
        );
        assert_eq!(
            decode_with_predictor(&encoded.container, id.as_bytes(), &mut BitMixer::new(v3))
                .unwrap(),
            data
        );
    }

    #[test]
    fn invalid_configurations_are_rejected() {
        for bad in [
            MixerConfig {
                table_bits: 9,
                ..MixerConfig::default()
            },
            MixerConfig {
                match_bits: 25,
                ..MixerConfig::default()
            },
            MixerConfig {
                counter_limit: 0,
                ..MixerConfig::default()
            },
            MixerConfig {
                mixer_rate: 0,
                ..MixerConfig::default()
            },
            // bmx-v4 slots hold a 7-bit count.
            MixerConfig {
                counter_limit: 128,
                ..MixerConfig::default()
            },
        ] {
            assert!(BitMixer::try_new(bad).is_err());
        }
        let [v4, v3, v2] = MixerConfig::released();
        assert!(BitMixer::try_new(MixerConfig {
            counter_limit: 128,
            ..v3
        })
        .is_ok());
        assert_eq!(v4, MixerConfig::default());
        assert!(v4.label().starts_with("bmx-v4:") && v4.label().contains(";check_bits=5;"));
        assert_eq!(
            v3.label(),
            "bmx-v3:orders=1,2,3,4,6,8+word+o0+match;table_bits=22;match_bits=20;limit=127;rate=6;masks=e0e0e0e0e0e0e0e0,ffe0e0e0e0e00000"
        );
        assert_eq!(
            v2.label(),
            "bmx-v2:orders=1,2,3,4,6,8+word+o0+match;table_bits=22;match_bits=20;limit=127;rate=6"
        );
        assert!(BitMixer::try_with_masks(v2, &[[0; 8]]).is_err());
        // Without a side model no side table is sized.
        assert_eq!(BitMixer::new(v2).side_table.slots.len(), 2);
        assert!(
            BitMixer::try_with_masks(v2, &[[0xFF, 0, 0, 0, 0, 0, 0, 0]; MAX_MASKS + 1]).is_err()
        );
    }
    #[test]
    fn checked_slots_isolate_colliding_contexts() {
        let mut table = CounterTable::new(10);
        // Same slot index, two different check values.
        let (a, b) = (5u32, 17u32);
        for _ in 0..40 {
            table.update_checked(3, a, true, 127);
        }
        let (p, n) = table.p_checked(3, a);
        assert!(p > 3900 && n == 40, "{p} {n}");
        // The other context reads the slot as unseen, not as a confident 1.
        assert_eq!(table.p_checked(3, b), (2048, 0));
        // Its first update replaces the slot.
        table.update_checked(3, b, false, 127);
        let (p, n) = table.p_checked(3, b);
        assert!(p < 2048 && n == 1, "{p} {n}");
        assert_eq!(table.p_checked(3, a), (2048, 0));
        // Counts saturate at the 7-bit limit.
        for _ in 0..300 {
            table.update_checked(4, a, true, 127);
        }
        assert_eq!(table.p_checked(4, a).1, 127);
    }

    #[test]
    fn v4_mixer_changes_predictions_round_trips_and_binds_identity() {
        let data = text(30_000);
        let v4 = small();
        let v3 = MixerConfig {
            architecture: Architecture::Classic,
            ..v4
        };
        assert_eq!(v4.architecture, Architecture::V4);
        let mut a = BitMixer::new(v3);
        let mut b = BitMixer::new(v4);
        let mut differ = 0usize;
        for &byte in &data[..2_000] {
            for shift in (0..8).rev() {
                differ += usize::from(a.p12() != b.p12());
                let bit = (byte >> shift) & 1 == 1;
                a.update(bit);
                b.update(bit);
            }
        }
        assert!(differ > 8_000, "v4 must change predictions ({differ})");
        let id = v4.label();
        let encoded = encode_with_predictor(&data, id.as_bytes(), &mut BitMixer::new(v4)).unwrap();
        assert!(
            decode_with_predictor(&encoded.container, id.as_bytes(), &mut BitMixer::new(v3))
                .is_err()
        );
        assert_eq!(
            decode_with_predictor(&encoded.container, id.as_bytes(), &mut BitMixer::new(v4))
                .unwrap(),
            data
        );
    }
}
