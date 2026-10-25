//! COPY-1: bounded exact-repeat copy expert for HYB H-1d.
//!
//! All candidates come from previously decoded bytes; the expert is
//! independent of the P1 bank weights and the fixed H-1d neural head.
//! Each new byte selects the next-byte candidates after the longest matching
//! recent suffix. At bit depth d only the d already decoded MSB bits may
//! condition the copy distribution. There is no lookup into the future.
//!
//! Protocol: docs/HYB-H1D-COPY-EXPERT-PREREGISTRATION.md.
//! Research-only; no real-text quality or performance claim.

/// Previous decoded bytes retained for causal recopy.
pub const COPY_WINDOW: usize = 256;
/// Longest suffix supported by the initial research control.
pub const COPY_MAX_SUFFIX: usize = 4;
const Q32: u64 = 1u64 << 32;

/// Fixed-footprint recurrent copy expert. It never stores any part of the
/// currently predicted byte until its final bit has been decoded.
#[derive(Clone, Debug)]
pub struct CopyExpert {
    history: [u8; COPY_WINDOW],
    cursor: usize,
    len: usize,
    candidates: [u8; COPY_WINDOW],
    count: usize,
    longest: usize,
}

impl Default for CopyExpert {
    fn default() -> Self {
        Self {
            history: [0; COPY_WINDOW],
            cursor: 0,
            len: 0,
            candidates: [0; COPY_WINDOW],
            count: 0,
            longest: 0,
        }
    }
}

impl CopyExpert {
    #[inline]
    fn history_at(&self, index: usize) -> u8 {
        debug_assert!(index < self.len);
        if self.len < COPY_WINDOW {
            self.history[index]
        } else {
            self.history[(self.cursor + index) % COPY_WINDOW]
        }
    }

    /// Advance only after decoding the full byte. Ring capacity is fixed.
    pub fn push_decoded(&mut self, byte: u8) {
        self.history[self.cursor] = byte;
        self.cursor = (self.cursor + 1) % COPY_WINDOW;
        self.len = (self.len + 1).min(COPY_WINDOW);
    }

    /// Select all next-byte candidates tied at the maximum suffix length.
    /// Every matched occurrence, and its next byte, predates the current
    /// prediction. A copy is not available without at least a one-byte match.
    pub fn prepare_byte(&mut self) {
        self.count = 0;
        self.longest = 0;
        for candidate_index in 1..self.len {
            let mut matched = 0;
            let limit = candidate_index.min(COPY_MAX_SUFFIX);
            for length in 1..=limit {
                if self.history_at(candidate_index - length)
                    == self.history_at(self.len - length)
                {
                    matched = length;
                } else {
                    break;
                }
            }
            if matched == 0 || matched < self.longest {
                continue;
            }
            if matched > self.longest {
                self.longest = matched;
                self.count = 0;
            }
            self.candidates[self.count] = self.history_at(candidate_index);
            self.count += 1;
        }
    }

    /// The Q32 probability for the next bit, with Laplace(1,1) smoothing.
    /// `decoded_msb` includes only the already decoded high bits of the
    /// current byte, never the next bit or any unknown lower bits.
    #[must_use]
    pub fn probability(&self, depth: usize, decoded_msb: u8) -> Option<u32> {
        assert!(depth < 8);
        if self.count == 0 {
            return None;
        }
        let mask = if depth == 0 {
            0
        } else {
            u8::MAX << (8 - depth)
        };
        let wanted = decoded_msb & mask;
        let mut count = 0u64;
        let mut ones = 0u64;
        for &byte in &self.candidates[..self.count] {
            if byte & mask != wanted {
                continue;
            }
            count += 1;
            ones += u64::from((byte >> (7 - depth)) & 1);
        }
        if count == 0 {
            return None;
        }
        let probability = ((ones + 1) * Q32) / (count + 2);
        Some(probability.clamp(1, u64::from(u32::MAX) - 1) as u32)
    }

    #[must_use]
    pub fn has_match(&self) -> bool {
        self.count != 0
    }

    #[must_use]
    pub fn longest_match(&self) -> usize {
        self.longest
    }
}

/// Blend already-corrected H-1d probability and a causal copy probability.
/// A missing or mismatched-prefix copy leaves H-1d completely unchanged.
/// Weight is in eighths (0..=8), with nearest-integer Q32 rounding.
#[must_use]
pub fn mix_probability(neural: u32, copy: Option<u32>, weight_eighths: u8) -> u32 {
    assert!(weight_eighths <= 8);
    if weight_eighths == 0 {
        return neural;
    }
    let Some(copy) = copy else {
        return neural;
    };
    let value = (u64::from(neural) * u64::from(8 - weight_eighths)
        + u64::from(copy) * u64::from(weight_eighths)
        + 4)
        / 8;
    value.clamp(1, u64::from(u32::MAX) - 1) as u32
}

/// Independent, stateless oracle from a slice of *previously decoded bytes*.
/// Unlike CopyExpert, it does not use a ring, candidate cache, or stateful
/// update path; it is intentionally less efficient and is used for testing
/// complete compressor probabilities.
#[must_use]
pub fn oracle_probability(history: &[u8], depth: usize, decoded_msb: u8) -> Option<u32> {
    assert!(depth < 8);
    let start = history.len().saturating_sub(COPY_WINDOW);
    let previous = &history[start..];
    let len = previous.len();
    let mut strongest = 0;
    let mut candidates = Vec::new();
    for end in 1..len {
        let mut size = 0;
        for offset in 0..end.min(COPY_MAX_SUFFIX) {
            if previous[end - offset - 1] != previous[len - offset - 1] {
                break;
            }
            size += 1;
        }
        if size == 0 || size < strongest {
            continue;
        }
        if size > strongest {
            candidates.clear();
            strongest = size;
        }
        candidates.push(previous[end]);
    }
    if candidates.is_empty() {
        return None;
    }

    let prefix_shift = 8 - depth;
    let high = if depth == 0 {
        0
    } else {
        decoded_msb >> prefix_shift
    };
    let mut matched = 0u64;
    let mut bit_ones = 0u64;
    for byte in candidates {
        if depth != 0 && byte >> prefix_shift != high {
            continue;
        }
        matched += 1;
        if byte & (1u8 << (7 - depth)) != 0 {
            bit_ones += 1;
        }
    }
    if matched == 0 {
        return None;
    }
    Some((((bit_ones + 1) << 32) / (matched + 2))
        .clamp(1, u64::from(u32::MAX) - 1) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn causal_oracle_matches_ring_at_all_depths() {
        let mut expert = CopyExpert::default();
        let mut history = Vec::new();
        let mut x = 0x9e37_79b9_u64;
        for position in 0..500 {
            expert.prepare_byte();
            for target in [0u8, 0xff, 0x5a, (x >> 17) as u8] {
                for depth in 0..8 {
                    let known = target & (u8::MAX << (8 - depth));
                    assert_eq!(
                        expert.probability(depth, known),
                        oracle_probability(&history, depth, known),
                        "position={position} target={target} depth={depth}"
                    );
                }
            }
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let byte = if position % 7 <= 3 {
                b"abab"[position % 4]
            } else {
                (x >> 8) as u8
            };
            expert.push_decoded(byte);
            history.push(byte);
        }
    }

    #[test]
    fn copy_detects_repeated_suffix_but_never_looks_ahead() {
        let mut expert = CopyExpert::default();
        for &byte in b"abcabc" {
            expert.push_decoded(byte);
        }
        expert.prepare_byte();
        assert_eq!(expert.longest_match(), 3);
        assert_eq!(expert.probability(0, 0), Some((Q32 / 3) as u32));
        // The repeated "abc" is followed by 'a' historically, so its
        // first bit is zero and P(bit=1) = 1/3.
        let p = expert.probability(0, 0).unwrap();
        assert!(p < (1u32 << 31));
        // After a disallowed high-bit prefix, there is no candidate.
        assert_eq!(expert.probability(1, 0x80), None);
    }

    #[test]
    fn no_suffix_match_keeps_neural_head_unchanged() {
        let mut expert = CopyExpert::default();
        for &byte in b"abc" {
            expert.push_decoded(byte);
        }
        expert.prepare_byte();
        assert!(!expert.has_match());
        assert_eq!(expert.probability(0, 0), None);
        assert_eq!(mix_probability(123_456_789, None, 2), 123_456_789);
        assert_eq!(mix_probability(123_456_789, Some(345_678_901), 0), 123_456_789);
    }

    #[test]
    fn mix_extremes_and_bounds() {
        assert_eq!(mix_probability(100, Some(300), 0), 100);
        assert_eq!(mix_probability(100, Some(300), 4), 200);
        assert_eq!(mix_probability(100, Some(300), 8), 300);
        assert_eq!(mix_probability(1, Some(u32::MAX - 1), 4), 1 << 31);
    }
}
