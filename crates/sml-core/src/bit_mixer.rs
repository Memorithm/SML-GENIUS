//! ENG-1d/BMX: adaptive Boolean context-mixing bit predictor.
//!
//! Predicts each bit of a byte stream (most significant bit first) from
//! several hashed byte contexts, a match model and an order-0 context, mixes
//! the predictions in the logistic domain with online-learned weights, then
//! refines the result with an adaptive probability map. Everything learns
//! while coding: there is no frozen TRAIN count table.
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
const N_HASHED: usize = ORDERS.len() + 1; // + word context
const N_INPUTS: usize = N_HASHED + 1 + 1 + 1; // + order 0, match, bias
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
}

impl Default for MixerConfig {
    fn default() -> Self {
        Self {
            table_bits: 22,
            match_bits: 20,
            counter_limit: 127,
            mixer_rate: 6,
        }
    }
}

impl MixerConfig {
    /// Stable text identity, recorded in coded containers.
    #[must_use]
    pub fn label(&self) -> String {
        format!(
            "bmx-v2:orders=1,2,3,4,6,8+word+o0+match;table_bits={};match_bits={};limit={};rate={}",
            self.table_bits, self.match_bits, self.counter_limit, self.mixer_rate
        )
    }

    fn valid(&self) -> bool {
        (10..=24).contains(&self.table_bits)
            && (10..=24).contains(&self.match_bits)
            && (1..=1023).contains(&self.counter_limit)
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
    byte_hashes: [u32; N_HASHED],
    word_hash: u32,
    c0: u32,
    bit_pos: u32,
    // per-bit state
    inputs: [i32; N_INPUTS],
    slots: [usize; N_HASHED],
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
        if !config.valid() {
            return Err("invalid MixerConfig");
        }
        let logistic = Logistic::new();
        let apm_order1 = Apm::new(256 * 256);
        let apm_order2 = Apm::new(1 << 16);
        let mut mixer = Self {
            config,
            tables: (0..N_HASHED)
                .map(|_| CounterTable::new(config.table_bits))
                .collect(),
            adapters: (0..N_HASHED)
                .map(|_| Apm::new(CONFIDENCE_BUCKETS))
                .collect(),
            order0: CounterTable::new(8),
            match_counters: CounterTable::new(16),
            weights: vec![0; 256 * N_INPUTS],
            apm_order1,
            apm_order2,
            logistic,
            history: Vec::new(),
            match_index: vec![0; 1 << config.match_bits],
            match_ptr: 0,
            match_len: 0,
            byte_hashes: [0; N_HASHED],
            word_hash: 0,
            c0: 1,
            bit_pos: 0,
            inputs: [0; N_INPUTS],
            slots: [0; N_HASHED],
            match_slot: 0,
            mixer_p: 2048,
            final_p: 2048,
        };
        let initial = (1 << 16) / N_INPUTS as i32;
        for w in &mut mixer.weights {
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
        for (i, slot) in self.slots.iter_mut().enumerate() {
            *slot = hash(self.byte_hashes[i], c0) as usize;
            let raw = self.tables[i].p(*slot);
            let bucket = confidence_bucket(self.tables[i].count(*slot));
            let adapted = self.adapters[i].refine(raw, bucket, &self.logistic);
            self.inputs[i] = self.logistic.stretch(adapted.clamp(1, 4095));
        }
        self.inputs[N_HASHED] = self.logistic.stretch(self.order0.p(c0 as usize));
        self.inputs[N_HASHED + 1] = match self.expected_bit() {
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
        self.inputs[N_HASHED + 2] = 256;
        let base = c0 as usize * N_INPUTS;
        let dot: i64 = self
            .inputs
            .iter()
            .zip(&self.weights[base..base + N_INPUTS])
            .map(|(&x, &w)| i64::from(x) * i64::from(w))
            .sum();
        self.mixer_p = squash((dot >> 16) as i32).clamp(1, 4095);
        let c1 = usize::from(self.history.last().copied().unwrap_or(0));
        let c2 = usize::from(if self.history.len() >= 2 {
            self.history[self.history.len() - 2]
        } else {
            0
        });
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
        let err = ((i32::from(bit) << 12) - self.mixer_p) * self.config.mixer_rate;
        let base = self.c0 as usize * N_INPUTS;
        for (w, &x) in self.weights[base..base + N_INPUTS]
            .iter_mut()
            .zip(&self.inputs)
        {
            *w = w.saturating_add(((i64::from(x) * i64::from(err) + 0x2000) >> 14) as i32);
        }
        // Counters and maps.
        for (i, &slot) in self.slots.iter().enumerate() {
            self.tables[i].update(slot, bit, limit);
            self.adapters[i].update(bit);
        }
        self.order0.update(self.c0 as usize, bit, limit);
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
        ] {
            assert!(BitMixer::try_new(bad).is_err());
        }
        assert!(MixerConfig::default().label().starts_with("bmx-v2:"));
    }
}
