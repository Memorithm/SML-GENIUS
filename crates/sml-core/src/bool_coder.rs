//! ENG-1d/BAC: exact, deterministic Boolean (binary) arithmetic coder.
//!
//! Every byte is coded as 8 binary decisions, most significant bit first.
//! A 256-way byte distribution is decomposed exactly into those decisions
//! through a fixed bottom-up sum tree, so the ideal coded size equals the
//! byte model's log-loss; only the per-decision quantisation remains.
//!
//! Coder: 32-bit range, 64-bit low with byte-wise carry propagation (the
//! classic LZMA layout), probabilities as `u32` fractions of 2^32. The split
//! point is the exact 64-bit product `(range * p1) >> 32`, so no precision is
//! lost to a pre-shifted range. Probabilities are clamped to
//! `[P_MIN, 2^32 - P_MIN]` with `P_MIN = 2^8`; because the range is always at
//! least 2^24 after normalisation, both sub-intervals are non-empty.
//!
//! Determinism: the quantisation uses only IEEE-754 `+`, `/` and `*` on
//! `f64` in a fixed order, then `round`. Encoder and decoder therefore derive
//! identical integers from identical model outputs on any conforming target.
//! The model itself must be deterministic; the container records a caller
//! model identity and the plaintext SHA-256, and decoding fails closed on
//! any mismatch, truncation or corruption instead of returning wrong bytes.
//!
//! This module codes; it makes no model-quality claim.

use std::fmt;

use crate::elastic_ngram_paged_sha::sha256;

/// Smallest representable probability of either branch, as a fraction of 2^32.
pub const P_MIN: u32 = 1 << 8;
/// Largest representable probability of either branch.
pub const P_MAX: u32 = u32::MAX - P_MIN + 1;
const TOP: u32 = 1 << 24;
const MAGIC: &[u8; 8] = b"SMLBAC01";
/// Upper bound on the caller model identity carried in the container.
pub const MAX_MODEL_ID: usize = 1024;
/// Upper bound on the decoded length accepted from a container header.
pub const MAX_PLAINTEXT: u64 = 1 << 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoderError {
    InvalidDistribution,
    InvalidModelId,
    TruncatedStream,
    TrailingData,
    BadMagic,
    ModelMismatch,
    LengthOutOfRange,
    PlaintextMismatch,
    Model(String),
}

impl fmt::Display for CoderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CoderError {}

/// Quantises P(bit = 1) to the coder's `u32` scale, clamped to keep both
/// branches codable. Rejects non-finite or out-of-range input.
pub fn quantize_p1(p1: f64) -> Result<u32, CoderError> {
    if !p1.is_finite() || !(0.0..=1.0).contains(&p1) {
        return Err(CoderError::InvalidDistribution);
    }
    let scaled = (p1 * 4_294_967_296.0).round();
    Ok(scaled.clamp(f64::from(P_MIN), f64::from(P_MAX)) as u32)
}

/// Exact effective probability of `bit` that the coder applies for `p1`
/// with the current `range` (useful for accounting and tests).
#[must_use]
pub fn effective_probability(range: u32, p1: u32, bit: bool) -> f64 {
    let bound = split(range, p1);
    if bit {
        f64::from(bound) / f64::from(range)
    } else {
        f64::from(range - bound) / f64::from(range)
    }
}

fn split(range: u32, p1: u32) -> u32 {
    ((u64::from(range) * u64::from(p1)) >> 32) as u32
}

/// Binary arithmetic encoder.
#[derive(Debug)]
pub struct BinaryEncoder {
    low: u64,
    range: u32,
    cache: u8,
    cache_size: u64,
    out: Vec<u8>,
}

impl Default for BinaryEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl BinaryEncoder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            low: 0,
            range: u32::MAX,
            cache: 0,
            cache_size: 1,
            out: Vec::new(),
        }
    }

    /// Current range (for exact accounting).
    #[must_use]
    pub fn range(&self) -> u32 {
        self.range
    }

    /// Codes `bit` with P(bit = 1) = `p1 / 2^32`; `p1` must lie in
    /// `[P_MIN, P_MAX]` (use [`quantize_p1`]).
    pub fn encode(&mut self, bit: bool, p1: u32) {
        debug_assert!((P_MIN..=P_MAX).contains(&p1));
        let bound = split(self.range, p1);
        if bit {
            self.range = bound;
        } else {
            self.low += u64::from(bound);
            self.range -= bound;
        }
        while self.range < TOP {
            self.range <<= 8;
            self.shift_low();
        }
    }

    fn shift_low(&mut self) {
        if self.low < 0xFF00_0000 || self.low > 0xFFFF_FFFF {
            let carry = (self.low >> 32) as u8;
            let mut byte = self.cache;
            loop {
                self.out.push(byte.wrapping_add(carry));
                byte = 0xFF;
                self.cache_size -= 1;
                if self.cache_size == 0 {
                    break;
                }
            }
            self.cache = ((self.low >> 24) & 0xFF) as u8;
        }
        self.cache_size += 1;
        self.low = (self.low & 0x00FF_FFFF) << 8;
    }

    /// Flushes and returns the coded bytes.
    #[must_use]
    pub fn finish(mut self) -> Vec<u8> {
        for _ in 0..5 {
            self.shift_low();
        }
        self.out
    }
}

/// Binary arithmetic decoder over a byte slice; never reads past the end.
#[derive(Debug)]
pub struct BinaryDecoder<'a> {
    code: u32,
    range: u32,
    input: &'a [u8],
    pos: usize,
}

impl<'a> BinaryDecoder<'a> {
    pub fn new(input: &'a [u8]) -> Result<Self, CoderError> {
        if input.len() < 5 {
            return Err(CoderError::TruncatedStream);
        }
        let mut decoder = Self {
            code: 0,
            range: u32::MAX,
            input,
            pos: 1,
        };
        if input[0] != 0 {
            return Err(CoderError::InvalidDistribution);
        }
        for _ in 0..4 {
            decoder.code = (decoder.code << 8) | u32::from(decoder.next_byte()?);
        }
        Ok(decoder)
    }

    fn next_byte(&mut self) -> Result<u8, CoderError> {
        let byte = *self
            .input
            .get(self.pos)
            .ok_or(CoderError::TruncatedStream)?;
        self.pos += 1;
        Ok(byte)
    }

    /// Current range (for exact accounting).
    #[must_use]
    pub fn range(&self) -> u32 {
        self.range
    }

    pub fn decode(&mut self, p1: u32) -> Result<bool, CoderError> {
        debug_assert!((P_MIN..=P_MAX).contains(&p1));
        let bound = split(self.range, p1);
        let bit = if self.code < bound {
            self.range = bound;
            true
        } else {
            self.code -= bound;
            self.range -= bound;
            false
        };
        while self.range < TOP {
            self.range <<= 8;
            self.code = (self.code << 8) | u32::from(self.next_byte()?);
        }
        Ok(bit)
    }

    /// Bytes consumed so far.
    #[must_use]
    pub fn consumed(&self) -> usize {
        self.pos
    }
}

/// Exact decomposition of a byte distribution into the 255 binary decisions
/// of a most-significant-bit-first tree. Node 1 is the root; node `k` has
/// children `2k` (bit 0) and `2k+1` (bit 1); leaves are `256 + byte`.
#[derive(Clone, Debug)]
pub struct ByteBitTree {
    sums: [f64; 512],
}

impl ByteBitTree {
    /// Builds the tree. The distribution must be finite, non-negative and
    /// sum to 1 within `1e-9`; every byte that may occur needs mass > 0.
    pub fn new(probabilities: &[f64; 256]) -> Result<Self, CoderError> {
        let mut sums = [0.0; 512];
        for (slot, &p) in sums[256..].iter_mut().zip(probabilities) {
            if !p.is_finite() || p < 0.0 {
                return Err(CoderError::InvalidDistribution);
            }
            *slot = p;
        }
        for node in (1..256).rev() {
            sums[node] = sums[2 * node] + sums[2 * node + 1];
        }
        if (sums[1] - 1.0).abs() > 1e-9 {
            return Err(CoderError::InvalidDistribution);
        }
        Ok(Self { sums })
    }

    /// P(next bit = 1) at internal `node` (1..=255). A zero-mass node yields
    /// 1/2; it cannot be reached by a byte with positive mass.
    #[must_use]
    pub fn p1(&self, node: usize) -> f64 {
        let total = self.sums[node];
        if total > 0.0 {
            self.sums[2 * node + 1] / total
        } else {
            0.5
        }
    }

    /// Model probability of `byte` (the leaf mass).
    #[must_use]
    pub fn probability(&self, byte: u8) -> f64 {
        self.sums[256 + usize::from(byte)]
    }
}

/// Result of [`encode_bytes`].
#[derive(Clone, Debug)]
pub struct Encoded {
    /// Complete container: header plus coded payload.
    pub container: Vec<u8>,
    /// Coded payload length in bytes (without header).
    pub payload_bytes: usize,
    /// Sum of `-log2 p(byte)` under the model's `f64` distributions.
    pub model_bits: f64,
    /// Sum of `-log2` of the probabilities the coder actually applied.
    pub coded_bits: f64,
}

fn header(model_id: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CoderError> {
    if model_id.is_empty() || model_id.len() > MAX_MODEL_ID {
        return Err(CoderError::InvalidModelId);
    }
    let mut out = Vec::with_capacity(8 + 2 + model_id.len() + 8 + 32);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(model_id.len() as u16).to_le_bytes());
    out.extend_from_slice(model_id);
    out.extend_from_slice(&(plaintext.len() as u64).to_le_bytes());
    out.extend_from_slice(&sha256(plaintext));
    Ok(out)
}

/// Encodes `data`. `model(history)` returns the byte distribution for the
/// next position, where `history` is the preceding bytes (at most
/// `max_history` of them). `model_id` identifies the model for decoding.
pub fn encode_bytes<F>(
    data: &[u8],
    max_history: usize,
    model_id: &[u8],
    mut model: F,
) -> Result<Encoded, CoderError>
where
    F: FnMut(&[u8]) -> Result<[f64; 256], CoderError>,
{
    if data.len() as u64 > MAX_PLAINTEXT {
        return Err(CoderError::LengthOutOfRange);
    }
    let mut container = header(model_id, data)?;
    let mut encoder = BinaryEncoder::new();
    let (mut model_bits, mut coded_bits) = (0.0f64, 0.0f64);
    for (i, &byte) in data.iter().enumerate() {
        let tree = ByteBitTree::new(&model(&data[i.saturating_sub(max_history)..i])?)?;
        let p = tree.probability(byte);
        if p <= 0.0 {
            return Err(CoderError::InvalidDistribution);
        }
        model_bits -= p.log2();
        let mut node = 1usize;
        for shift in (0..8).rev() {
            let bit = (byte >> shift) & 1 == 1;
            let p1 = quantize_p1(tree.p1(node))?;
            coded_bits -= effective_probability(encoder.range(), p1, bit).log2();
            encoder.encode(bit, p1);
            node = 2 * node + usize::from(bit);
        }
    }
    let payload = encoder.finish();
    let payload_bytes = payload.len();
    container.extend_from_slice(&payload);
    Ok(Encoded {
        container,
        payload_bytes,
        model_bits,
        coded_bits,
    })
}

/// Reads the model identity stored in a container without decoding it.
pub fn container_model_id(container: &[u8]) -> Result<&[u8], CoderError> {
    if container.len() < 10 || &container[..8] != MAGIC {
        return Err(CoderError::BadMagic);
    }
    let len = usize::from(u16::from_le_bytes([container[8], container[9]]));
    if len == 0 || len > MAX_MODEL_ID {
        return Err(CoderError::InvalidModelId);
    }
    container
        .get(10..10 + len)
        .ok_or(CoderError::TruncatedStream)
}

/// Decodes a container produced by [`encode_bytes`] with the same model.
/// Fails closed on a different model identity, truncation, trailing bytes
/// or any plaintext checksum mismatch.
pub fn decode_bytes<F>(
    container: &[u8],
    max_history: usize,
    model_id: &[u8],
    mut model: F,
) -> Result<Vec<u8>, CoderError>
where
    F: FnMut(&[u8]) -> Result<[f64; 256], CoderError>,
{
    let (length, digest, payload) = parse_container(container, model_id)?;
    let mut decoder = BinaryDecoder::new(payload)?;
    // A coded byte costs at least 8 * -log2(P_MAX / 2^32) bits; cap the
    // allocation by what the payload could possibly hold.
    let mut out = Vec::with_capacity(length.min(payload.len().saturating_mul(1 << 16)));
    for i in 0..length {
        let tree = ByteBitTree::new(&model(&out[i.saturating_sub(max_history)..i])?)?;
        let mut node = 1usize;
        for _ in 0..8 {
            let bit = decoder.decode(quantize_p1(tree.p1(node))?)?;
            node = 2 * node + usize::from(bit);
        }
        out.push((node - 256) as u8);
    }
    if decoder.consumed() != payload.len() {
        return Err(CoderError::TrailingData);
    }
    if sha256(&out) != digest {
        return Err(CoderError::PlaintextMismatch);
    }
    Ok(out)
}

/// Splits a container into (plaintext length, plaintext SHA-256, payload)
/// after checking magic and model identity.
fn parse_container<'a>(
    container: &'a [u8],
    model_id: &[u8],
) -> Result<(usize, [u8; 32], &'a [u8]), CoderError> {
    if container_model_id(container)? != model_id {
        return Err(CoderError::ModelMismatch);
    }
    let mut pos = 10 + model_id.len();
    let len_bytes: [u8; 8] = container
        .get(pos..pos + 8)
        .ok_or(CoderError::TruncatedStream)?
        .try_into()
        .map_err(|_| CoderError::TruncatedStream)?;
    let length = u64::from_le_bytes(len_bytes);
    if length > MAX_PLAINTEXT {
        return Err(CoderError::LengthOutOfRange);
    }
    pos += 8;
    let digest: [u8; 32] = container
        .get(pos..pos + 32)
        .ok_or(CoderError::TruncatedStream)?
        .try_into()
        .map_err(|_| CoderError::TruncatedStream)?;
    pos += 32;
    let length = usize::try_from(length).map_err(|_| CoderError::LengthOutOfRange)?;
    Ok((length, digest, &container[pos..]))
}

/// A sequential bit model: it predicts the next bit, then learns it.
/// Encoder and decoder must start from identical states and see identical
/// bits, so the predictor must be deterministic.
pub trait BitPredictor {
    /// P(next bit = 1) as a fraction of 2^32, within `[P_MIN, P_MAX]`.
    fn p1(&self) -> u32;
    /// Learns the bit that was just coded.
    fn update(&mut self, bit: bool);
}

/// Encodes `data` bit by bit (MSB first) with an adaptive bit predictor.
/// `model_bits` and `coded_bits` both use the probabilities applied.
pub fn encode_with_predictor<P: BitPredictor>(
    data: &[u8],
    model_id: &[u8],
    predictor: &mut P,
) -> Result<Encoded, CoderError> {
    if data.len() as u64 > MAX_PLAINTEXT {
        return Err(CoderError::LengthOutOfRange);
    }
    let mut container = header(model_id, data)?;
    let mut encoder = BinaryEncoder::new();
    let (mut model_bits, mut coded_bits) = (0.0f64, 0.0f64);
    for &byte in data {
        for shift in (0..8).rev() {
            let bit = (byte >> shift) & 1 == 1;
            let p1 = predictor.p1();
            if !(P_MIN..=P_MAX).contains(&p1) {
                return Err(CoderError::InvalidDistribution);
            }
            let p = f64::from(p1) / 4_294_967_296.0;
            model_bits -= if bit { p } else { 1.0 - p }.log2();
            coded_bits -= effective_probability(encoder.range(), p1, bit).log2();
            encoder.encode(bit, p1);
            predictor.update(bit);
        }
    }
    let payload = encoder.finish();
    let payload_bytes = payload.len();
    container.extend_from_slice(&payload);
    Ok(Encoded {
        container,
        payload_bytes,
        model_bits,
        coded_bits,
    })
}

/// Decodes a container from [`encode_with_predictor`] with a predictor in
/// the same initial state. Fails closed like [`decode_bytes`].
pub fn decode_with_predictor<P: BitPredictor>(
    container: &[u8],
    model_id: &[u8],
    predictor: &mut P,
) -> Result<Vec<u8>, CoderError> {
    let (length, digest, payload) = parse_container(container, model_id)?;
    let mut decoder = BinaryDecoder::new(payload)?;
    let mut out = Vec::with_capacity(length.min(payload.len().saturating_mul(1 << 16)));
    for _ in 0..length {
        let mut byte = 0u8;
        for _ in 0..8 {
            let p1 = predictor.p1();
            if !(P_MIN..=P_MAX).contains(&p1) {
                return Err(CoderError::InvalidDistribution);
            }
            let bit = decoder.decode(p1)?;
            predictor.update(bit);
            byte = (byte << 1) | u8::from(bit);
        }
        out.push(byte);
    }
    if decoder.consumed() != payload.len() {
        return Err(CoderError::TrailingData);
    }
    if sha256(&out) != digest {
        return Err(CoderError::PlaintextMismatch);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toy_tasks::SplitMix64;

    /// Deterministic order-1 toy model with full support.
    fn toy(history: &[u8]) -> Result<[f64; 256], CoderError> {
        let prev = history.last().copied().unwrap_or(0);
        let mut p = [0.0; 256];
        let mut total = 0.0;
        for (b, slot) in p.iter_mut().enumerate() {
            let near = (b as i32 - i32::from(prev.wrapping_add(1))).unsigned_abs();
            *slot = 1.0 / (1.0 + f64::from(near * near));
            total += *slot;
        }
        for slot in &mut p {
            *slot /= total;
        }
        Ok(p)
    }

    fn sample(len: usize, seed: u64) -> Vec<u8> {
        let mut rng = SplitMix64::new(seed);
        let mut out = vec![0u8];
        while out.len() < len {
            let prev = *out.last().unwrap();
            let step = if rng.below(4) == 0 {
                rng.below(256) as u8
            } else {
                1
            };
            out.push(prev.wrapping_add(step));
        }
        out
    }

    #[test]
    fn raw_bits_round_trip_at_extreme_probabilities() {
        let mut rng = SplitMix64::new(7);
        let mut bits = Vec::new();
        let mut probs = Vec::new();
        for i in 0..200_000 {
            let p1 = match i % 5 {
                0 => P_MIN,
                1 => P_MAX,
                2 => 1 << 31,
                _ => (rng.next_u64() as u32).clamp(P_MIN, P_MAX),
            };
            let bit = (rng.next_u64() >> 11) % 1000 < u64::from(p1 >> 22);
            bits.push(bit);
            probs.push(p1);
        }
        let mut enc = BinaryEncoder::new();
        for (&b, &p) in bits.iter().zip(&probs) {
            enc.encode(b, p);
        }
        let stream = enc.finish();
        let mut dec = BinaryDecoder::new(&stream).unwrap();
        for (&b, &p) in bits.iter().zip(&probs) {
            assert_eq!(dec.decode(p).unwrap(), b);
        }
        assert_eq!(dec.consumed(), stream.len());
    }

    #[test]
    fn long_carry_chains_are_propagated() {
        // Always coding the unlikely 0 branch at p1 = P_MAX pushes `low` up
        // repeatedly and forces runs of 0xFF and carries.
        let mut enc = BinaryEncoder::new();
        let pattern: Vec<bool> = (0..50_000).map(|i| i % 97 != 0).collect();
        for &b in &pattern {
            enc.encode(!b, P_MAX);
        }
        let stream = enc.finish();
        let mut dec = BinaryDecoder::new(&stream).unwrap();
        for &b in &pattern {
            assert_eq!(dec.decode(P_MAX).unwrap(), !b);
        }
    }

    #[test]
    fn bit_tree_is_an_exact_decomposition() {
        let p = toy(&[41]).unwrap();
        let tree = ByteBitTree::new(&p).unwrap();
        for byte in 0..=255u8 {
            let mut node = 1usize;
            let mut product = 1.0f64;
            for shift in (0..8).rev() {
                let bit = (byte >> shift) & 1 == 1;
                let p1 = tree.p1(node);
                product *= if bit { p1 } else { 1.0 - p1 };
                node = 2 * node + usize::from(bit);
            }
            assert_eq!(node - 256, usize::from(byte));
            let rel = (product - p[usize::from(byte)]).abs() / p[usize::from(byte)];
            assert!(rel < 1e-12, "byte {byte}: {rel}");
        }
        let mut bad = p;
        bad[3] = f64::NAN;
        assert!(ByteBitTree::new(&bad).is_err());
        let mut unnormalised = p;
        unnormalised[0] += 0.01;
        assert!(ByteBitTree::new(&unnormalised).is_err());
    }

    #[test]
    fn quantisation_is_clamped_and_rejects_invalid_input() {
        assert_eq!(quantize_p1(0.0).unwrap(), P_MIN);
        assert_eq!(quantize_p1(1.0).unwrap(), P_MAX);
        assert_eq!(quantize_p1(0.5).unwrap(), 1 << 31);
        for bad in [f64::NAN, -0.1, 1.1, f64::INFINITY] {
            assert_eq!(quantize_p1(bad), Err(CoderError::InvalidDistribution));
        }
    }

    #[test]
    fn bytes_round_trip_and_size_matches_model_loss() {
        for (len, seed) in [(0usize, 1u64), (1, 2), (257, 3), (60_000, 4)] {
            let data = sample(len, seed);
            let data = &data[..len];
            let encoded = encode_bytes(data, 1, b"toy-order1", toy).unwrap();
            let decoded = decode_bytes(&encoded.container, 1, b"toy-order1", toy).unwrap();
            assert_eq!(decoded, data);
            // Quantisation overhead per decision is ~2^-24 relative.
            let overhead = encoded.coded_bits - encoded.model_bits;
            assert!(overhead.abs() <= 1e-4 * (len as f64) + 1e-9, "{overhead}");
            // Payload is the ideal coded length plus a constant flush.
            let payload_bits = 8.0 * encoded.payload_bytes as f64;
            assert!(payload_bits >= encoded.coded_bits - 1e-6);
            assert!(
                payload_bits <= encoded.coded_bits + 48.0,
                "{payload_bits} vs {}",
                encoded.coded_bits
            );
        }
    }

    #[test]
    fn corruption_truncation_and_wrong_model_fail_closed() {
        let data = sample(5_000, 9);
        let encoded = encode_bytes(&data, 1, b"toy-order1", toy).unwrap();
        let c = &encoded.container;
        assert_eq!(
            decode_bytes(c, 1, b"other-model", toy),
            Err(CoderError::ModelMismatch)
        );
        let header = 10 + 10 + 8 + 32;
        let mut silent = 0;
        for offset in [header + 1, header + 7, header + c.len() / 3, c.len() - 3] {
            let mut flipped = c.clone();
            flipped[offset.min(c.len() - 1)] ^= 0x40;
            if let Ok(out) = decode_bytes(&flipped, 1, b"toy-order1", toy) {
                assert_eq!(
                    out, data,
                    "a corrupted stream must never decode to other bytes"
                );
                silent += 1;
            }
        }
        assert!(silent < 4);
        assert!(decode_bytes(&c[..c.len() - 1], 1, b"toy-order1", toy).is_err());
        let mut longer = c.clone();
        longer.push(0);
        assert_eq!(
            decode_bytes(&longer, 1, b"toy-order1", toy),
            Err(CoderError::TrailingData)
        );
        let mut bad_magic = c.clone();
        bad_magic[0] ^= 1;
        assert_eq!(
            decode_bytes(&bad_magic, 1, b"toy-order1", toy),
            Err(CoderError::BadMagic)
        );
        // A different (but valid) model under the same identity is caught by
        // the plaintext checksum, never returned as data.
        let shifted = |h: &[u8]| {
            let mut p = toy(h)?;
            p.rotate_left(1);
            Ok(p)
        };
        assert!(decode_bytes(c, 1, b"toy-order1", shifted).is_err());
    }

    /// Order-0 adaptive bit counter keyed by the partial byte: a minimal
    /// independent predictor for exercising the predictor API.
    struct Order0 {
        counts: Vec<[u32; 2]>,
        node: usize,
    }
    impl Order0 {
        fn new() -> Self {
            Self {
                counts: vec![[1, 1]; 256],
                node: 1,
            }
        }
    }
    impl BitPredictor for Order0 {
        fn p1(&self) -> u32 {
            let [zeros, ones] = self.counts[self.node];
            let p = f64::from(ones) / f64::from(zeros + ones);
            quantize_p1(p).unwrap()
        }
        fn update(&mut self, bit: bool) {
            self.counts[self.node][usize::from(bit)] += 1;
            self.node = 2 * self.node + usize::from(bit);
            if self.node >= 256 {
                self.node = 1;
            }
        }
    }

    #[test]
    fn predictor_api_round_trips_and_matches_ideal_size() {
        let data = sample(40_000, 21);
        let encoded = encode_with_predictor(&data, b"order0", &mut Order0::new()).unwrap();
        assert_eq!(
            decode_with_predictor(&encoded.container, b"order0", &mut Order0::new()).unwrap(),
            data
        );
        assert!((encoded.coded_bits - encoded.model_bits).abs() < 1e-3 * data.len() as f64);
        let payload_bits = 8.0 * encoded.payload_bytes as f64;
        assert!(
            payload_bits >= encoded.coded_bits - 1e-6 && payload_bits <= encoded.coded_bits + 48.0
        );
        // A predictor in a different state is caught, never silently wrong.
        let mut warm = Order0::new();
        warm.update(true);
        assert!(decode_with_predictor(&encoded.container, b"order0", &mut warm).is_err());
        assert_eq!(
            decode_with_predictor(&encoded.container, b"other", &mut Order0::new()),
            Err(CoderError::ModelMismatch)
        );
    }

    #[test]
    fn out_of_range_predictor_probability_is_rejected() {
        struct Bad;
        impl BitPredictor for Bad {
            fn p1(&self) -> u32 {
                0
            }
            fn update(&mut self, _: bool) {}
        }
        assert_eq!(
            encode_with_predictor(b"x", b"bad", &mut Bad).unwrap_err(),
            CoderError::InvalidDistribution
        );
    }

    #[test]
    fn model_errors_and_zero_mass_targets_are_reported() {
        let failing = |_: &[u8]| Err(CoderError::Model("offline".into()));
        assert_eq!(
            encode_bytes(b"ab", 1, b"m", failing).unwrap_err(),
            CoderError::Model("offline".into())
        );
        let point = |_: &[u8]| {
            let mut p = [0.0; 256];
            p[usize::from(b'a')] = 1.0;
            Ok(p)
        };
        assert!(encode_bytes(b"aaaa", 0, b"m", point).is_ok());
        assert_eq!(
            encode_bytes(b"ab", 0, b"m", point).unwrap_err(),
            CoderError::InvalidDistribution
        );
        assert_eq!(
            encode_bytes(b"a", 0, b"", point).unwrap_err(),
            CoderError::InvalidModelId
        );
    }
}
