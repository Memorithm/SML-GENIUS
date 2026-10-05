//! Pascal-GF(2) / Sierpiński reversible Boolean mixer.
//!
//! This module implements the preregistered PGF2 packed-state transform.
//! It is an integer-only research primitive, not Transformer attention and not
//! evidence of model-quality improvement.

use core::fmt;

/// Exact structural accounting for one packed PGF2 transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PascalGf2Stats {
    /// Logical Boolean state width.
    pub bits: usize,
    /// Number of packed `u64` words.
    pub words: usize,
    /// Number of butterfly stages, equal to `log2(bits)`.
    pub stages: u32,
    /// Logical destination-bit XOR updates.
    pub logical_xor_ops: u64,
    /// Packed `u64` destinations updated by the reference implementation.
    pub packed_word_updates: u64,
    /// Bytes occupied by the caller-provided packed state.
    pub state_bytes: usize,
    /// Algorithmic scratch words beyond the caller-provided state.
    pub scratch_words: usize,
}

/// Structural errors for PGF2 transforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PascalGf2Error {
    /// A zero-width state is undefined.
    Empty,
    /// The logical width must be a power of two.
    WidthNotPowerOfTwo { bits: usize },
    /// Converting packed words to a logical bit width overflowed `usize`.
    WidthOverflow,
}

impl fmt::Display for PascalGf2Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "PGF2 state must not be empty"),
            Self::WidthNotPowerOfTwo { bits } => {
                write!(f, "PGF2 width must be a power of two, got {bits} bits")
            }
            Self::WidthOverflow => write!(f, "PGF2 logical width overflow"),
        }
    }
}

impl std::error::Error for PascalGf2Error {}

const INTRA_WORD_STAGES: [(u32, u64); 6] = [
    (1, 0x5555_5555_5555_5555),
    (2, 0x3333_3333_3333_3333),
    (4, 0x0f0f_0f0f_0f0f_0f0f),
    (8, 0x00ff_00ff_00ff_00ff),
    (16, 0x0000_ffff_0000_ffff),
    (32, 0x0000_0000_ffff_ffff),
];

fn validate_bits(bits: usize) -> Result<(), PascalGf2Error> {
    if bits == 0 {
        return Err(PascalGf2Error::Empty);
    }
    if !bits.is_power_of_two() {
        return Err(PascalGf2Error::WidthNotPowerOfTwo { bits });
    }
    Ok(())
}

/// Applies the lower-triangular Pascal transform modulo 2 in place.
///
/// For logical indices `n` and `k`, the transform is
/// `y[n] = XOR(x[k])` over all binary submasks `k` of `n`.
///
/// The implementation performs six bit-parallel stages inside each `u64`,
/// followed by whole-word butterfly stages for widths above 64 bits. It does
/// not allocate after the caller has provided the packed slice.
pub fn pascal_gf2_in_place(words: &mut [u64]) -> Result<PascalGf2Stats, PascalGf2Error> {
    if words.is_empty() {
        return Err(PascalGf2Error::Empty);
    }

    let bits = words
        .len()
        .checked_mul(u64::BITS as usize)
        .ok_or(PascalGf2Error::WidthOverflow)?;
    validate_bits(bits)?;

    let stages = bits.ilog2();
    let mut packed_word_updates = 0_u64;

    for (shift, source_mask) in INTRA_WORD_STAGES {
        for word in words.iter_mut() {
            *word ^= (*word & source_mask) << shift;
            packed_word_updates += 1;
        }
    }

    let mut word_stride = 1_usize;
    while word_stride < words.len() {
        for index in 0..words.len() {
            if index & word_stride != 0 {
                let source = words[index ^ word_stride];
                words[index] ^= source;
                packed_word_updates += 1;
            }
        }
        word_stride <<= 1;
    }

    Ok(PascalGf2Stats {
        bits,
        words: words.len(),
        stages,
        logical_xor_ops: (bits as u64 / 2) * u64::from(stages),
        packed_word_updates,
        state_bytes: core::mem::size_of_val(words),
        scratch_words: 0,
    })
}

/// Direct Boolean oracle for the Pascal/binomial-mod-2 definition.
///
/// This routine intentionally favors transparency over speed. It enumerates the
/// binary submasks of every output index and is used to qualify the packed
/// implementation.
pub fn pascal_gf2_oracle(input: &[bool]) -> Result<Vec<bool>, PascalGf2Error> {
    validate_bits(input.len())?;

    let mut output = vec![false; input.len()];
    for (n, value) in output.iter_mut().enumerate() {
        let mut parity = false;
        let mut k = n;
        loop {
            parity ^= input[k];
            if k == 0 {
                break;
            }
            k = (k - 1) & n;
        }
        *value = parity;
    }
    Ok(output)
}

/// Expands packed words into logical bits in little-endian bit order.
#[must_use]
pub fn unpack_words(words: &[u64]) -> Vec<bool> {
    let mut bits = Vec::with_capacity(words.len() * u64::BITS as usize);
    for &word in words {
        for bit in 0..u64::BITS {
            bits.push((word & (1_u64 << bit)) != 0);
        }
    }
    bits
}

/// Packs a power-of-two Boolean state into `u64` words.
///
/// The current packed PGF2 implementation requires widths of at least 64 bits
/// and exact multiples of 64.
pub fn pack_bits(bits: &[bool]) -> Result<Vec<u64>, PascalGf2Error> {
    validate_bits(bits.len())?;
    if bits.len() < u64::BITS as usize || bits.len() % u64::BITS as usize != 0 {
        return Err(PascalGf2Error::WidthNotPowerOfTwo { bits: bits.len() });
    }

    let mut words = vec![0_u64; bits.len() / u64::BITS as usize];
    for (index, &bit) in bits.iter().enumerate() {
        if bit {
            words[index / u64::BITS as usize] |= 1_u64 << (index % u64::BITS as usize);
        }
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deterministic_words(count: usize, seed: u64) -> Vec<u64> {
        let mut state = seed | 1;
        let mut words = Vec::with_capacity(count);
        for _ in 0..count {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            words.push(state);
        }
        words
    }

    #[test]
    fn oracle_is_self_inverse_at_small_preregistered_widths() {
        for width in [8_usize, 16, 32, 64] {
            let input: Vec<bool> = (0..width)
                .map(|index| ((index * 13 + width) % 7) < 3)
                .collect();
            let once = pascal_gf2_oracle(&input).expect("oracle transform");
            let twice = pascal_gf2_oracle(&once).expect("oracle inverse");
            assert_eq!(twice, input, "width={width}");
        }
    }

    #[test]
    fn packed_rejects_non_power_of_two_widths() {
        let mut words = [0_u64; 3];
        assert_eq!(
            pascal_gf2_in_place(&mut words),
            Err(PascalGf2Error::WidthNotPowerOfTwo { bits: 192 })
        );
    }

    #[test]
    fn packed_k2048_matches_direct_oracle_and_is_self_inverse() {
        let original = deterministic_words(32, 0x5047_4632_4b32_3034);
        let expected_bits =
            pascal_gf2_oracle(&unpack_words(&original)).expect("direct K=2048 oracle");

        let mut transformed = original.clone();
        let stats = pascal_gf2_in_place(&mut transformed).expect("packed K=2048 transform");

        assert_eq!(unpack_words(&transformed), expected_bits);
        assert_eq!(stats.bits, 2048);
        assert_eq!(stats.words, 32);
        assert_eq!(stats.stages, 11);
        assert_eq!(stats.logical_xor_ops, 11_264);
        assert_eq!(stats.packed_word_updates, 272);
        assert_eq!(stats.state_bytes, 256);
        assert_eq!(stats.scratch_words, 0);

        pascal_gf2_in_place(&mut transformed).expect("packed K=2048 inverse");
        assert_eq!(transformed, original);
    }
}
