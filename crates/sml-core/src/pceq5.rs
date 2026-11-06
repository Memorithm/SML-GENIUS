//! PCEQ-5: exact Boolean zero-by-identity elimination for packed PVP-like banks.
//!
//! This is a research-only GF(2) XOR bitplane prototype, distinct from lossy
//! PCEQ-0..4 i16 quantization. It uses immutable Arc identity to prove
//! XOR(A,A)=0 without reading *any* source word. No production PVP changes.

use std::sync::Arc;

/// Only canonical bitplanes with an exact logical width are constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalBitplane {
    logical_bits: usize,
    words: Box<[u64]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanXorError {
    WordCount { expected: usize, observed: usize },
    NonzeroTail { invalid_bits: u64 },
    WidthMismatch { left: usize, right: usize },
}

impl CanonicalBitplane {
    pub fn from_words(logical_bits: usize, words: Vec<u64>) -> Result<Self, BooleanXorError> {
        let expected = logical_bits.div_ceil(64);
        if words.len() != expected {
            return Err(BooleanXorError::WordCount {
                expected,
                observed: words.len(),
            });
        }
        if logical_bits % 64 != 0 {
            let tail = !((1_u64 << (logical_bits % 64)) - 1);
            let invalid_bits = words[expected - 1] & tail;
            if invalid_bits != 0 {
                return Err(BooleanXorError::NonzeroTail { invalid_bits });
            }
        }
        Ok(Self {
            logical_bits,
            words: words.into_boxed_slice(),
        })
    }

    #[must_use]
    pub const fn logical_bits(&self) -> usize {
        self.logical_bits
    }

    #[must_use]
    pub fn words(&self) -> &[u64] {
        &self.words
    }
}

/// Packed, exact GF(2) XOR output. A missing word is mathematically zero.
/// No source-word payload is stored for zero output words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SparseBooleanXor {
    logical_bits: usize,
    active_bitmap: Vec<u64>,
    payload: Vec<u64>,
    source_word_reads: usize,
    zero_by_identity: bool,
}

impl SparseBooleanXor {
    #[must_use]
    pub const fn logical_bits(&self) -> usize {
        self.logical_bits
    }

    #[must_use]
    pub fn payload(&self) -> &[u64] {
        &self.payload
    }

    #[must_use]
    pub fn active_bitmap(&self) -> &[u64] {
        &self.active_bitmap
    }

    /// Source word *accesses in this algorithm*, not DDR read events.
    #[must_use]
    pub const fn source_word_reads(&self) -> usize {
        self.source_word_reads
    }

    /// True only for pointer-identity proof, not for coincidentally equal data.
    #[must_use]
    pub const fn zero_by_identity(&self) -> bool {
        self.zero_by_identity
    }

    #[must_use]
    pub fn logical_bytes(&self) -> u128 {
        (self.payload.len() as u128 + self.active_bitmap.len() as u128) * 8
    }

    #[must_use]
    pub fn reconstruct_words(&self) -> Vec<u64> {
        let word_count = self.logical_bits.div_ceil(64);
        let mut output = vec![0_u64; word_count];
        let mut index = 0;
        for (word_index, slot) in output.iter_mut().enumerate() {
            if (self.active_bitmap[word_index / 64] >> (word_index % 64)) & 1 == 1 {
                *slot = self.payload[index];
                index += 1;
            }
        }
        debug_assert_eq!(index, self.payload.len());
        output
    }
}

/// Computes exact Boolean XOR from canonical immutable sources.
///
/// When `Arc::ptr_eq(left, right)`, both operands are the *same immutable
/// object*, so the result is provably all zero and the source words need
/// not be inspected at all. When identities differ, the implementation
/// reads both words once, XORs, and omits zero output words.
///
/// This does not automatically detect equal values in distinct snapshots.
/// The identity fast path does not support mutable or stale aliases.
/// It is not directly wired into SML's production PVP executor.
pub fn xor_sparse_exact(
    left: &Arc<CanonicalBitplane>,
    right: &Arc<CanonicalBitplane>,
) -> Result<SparseBooleanXor, BooleanXorError> {
    if left.logical_bits != right.logical_bits {
        return Err(BooleanXorError::WidthMismatch {
            left: left.logical_bits,
            right: right.logical_bits,
        });
    }
    let word_count = left.words.len();
    let mut output = SparseBooleanXor {
        logical_bits: left.logical_bits,
        active_bitmap: vec![0_u64; word_count.div_ceil(64)],
        payload: Vec::new(),
        source_word_reads: 0,
        zero_by_identity: Arc::ptr_eq(left, right),
    };
    if output.zero_by_identity {
        return Ok(output);
    }
    for (word_index, (&a, &b)) in left.words.iter().zip(right.words.iter()).enumerate() {
        output.source_word_reads += 2;
        let value = a ^ b;
        if value != 0 {
            output.active_bitmap[word_index / 64] |= 1_u64 << (word_index % 64);
            output.payload.push(value);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(logical_bits: usize, seed: u64) -> Arc<CanonicalBitplane> {
        let word_count = logical_bits.div_ceil(64);
        let words = (0..word_count)
            .map(|index| {
                let mut n = (index as u64)
                    .wrapping_add(seed)
                    .wrapping_mul(0x9e37_79b9_7f4a_7c15);
                n ^= n >> 30;
                n = n.wrapping_mul(0xbf58_476d_1ce4_e5b9);
                n ^= n >> 27;
                if index + 1 == word_count && logical_bits % 64 != 0 {
                    n & ((1_u64 << (logical_bits % 64)) - 1)
                } else {
                    n
                }
            })
            .collect();
        Arc::new(CanonicalBitplane::from_words(logical_bits, words).unwrap())
    }

    #[test]
    fn xor_same_immutable_snapshot_is_zero_with_no_source_word_loads() {
        for width in [0, 1, 63, 64, 65, 127, 128, 129, 4097, 8257] {
            let source = plane(width, 0xcafe_f00d);
            let aliased = Arc::clone(&source);
            let output = xor_sparse_exact(&source, &aliased).unwrap();
            assert!(output.zero_by_identity());
            assert_eq!(output.source_word_reads(), 0);
            assert!(output.payload().is_empty());
            assert_eq!(output.reconstruct_words(), vec![0; width.div_ceil(64)]);
            assert_eq!(
                output.active_bitmap().len(),
                width.div_ceil(64).div_ceil(64)
            );
        }
    }

    #[test]
    fn two_distinct_equal_sources_require_reads_but_have_exact_zero_output() {
        for width in [0, 1, 65, 128, 8193] {
            let a = plane(width, 0xaaaa);
            let b = Arc::new((*a).clone());
            assert!(!Arc::ptr_eq(&a, &b));
            let output = xor_sparse_exact(&a, &b).unwrap();
            assert!(!output.zero_by_identity());
            assert_eq!(output.source_word_reads(), width.div_ceil(64) * 2);
            assert!(output.payload().is_empty());
            assert_eq!(output.reconstruct_words(), vec![0; width.div_ceil(64)]);
        }
    }

    #[test]
    fn exact_xor_parity_and_bitset_boundary_63_64() {
        for width in [
            1, 31, 63, 64, 65, 127, 128, 129, 4095, 4096, 4097, 4161, 8193,
        ] {
            let a = plane(width, 0x1234_5678);
            let b = plane(width, 0x9876_4321);
            let result = xor_sparse_exact(&a, &b).unwrap();
            let reference = a
                .words()
                .iter()
                .zip(b.words())
                .map(|(left, right)| left ^ right)
                .collect::<Vec<_>>();
            assert_eq!(result.reconstruct_words(), reference);
            assert_eq!(result.source_word_reads(), width.div_ceil(64) * 2);
            assert_eq!(
                result.active_bitmap().len(),
                width.div_ceil(64).div_ceil(64)
            );
            assert_eq!(
                result
                    .active_bitmap()
                    .iter()
                    .map(|word| word.count_ones() as usize)
                    .sum::<usize>(),
                result.payload().len()
            );
            let inverse = xor_sparse_exact(&b, &a).unwrap();
            assert_eq!(result.reconstruct_words(), inverse.reconstruct_words());
        }
    }

    #[test]
    fn all_zero_payload_and_activity_boundary_are_exact() {
        let width = 4161;
        let mut lhs = vec![0_u64; width.div_ceil(64)];
        let rhs = vec![0_u64; width.div_ceil(64)];
        lhs[63] = 1;
        lhs[64] = u64::MAX;
        lhs[65] = 1;
        let a = Arc::new(CanonicalBitplane::from_words(width, lhs.clone()).unwrap());
        let b = Arc::new(CanonicalBitplane::from_words(width, rhs).unwrap());
        let encoded = xor_sparse_exact(&a, &b).unwrap();
        assert_eq!(encoded.payload(), &[1, u64::MAX, 1]);
        assert_eq!(encoded.active_bitmap(), &[1_u64 << 63, 3]);
        assert_eq!(encoded.reconstruct_words(), lhs);
    }

    #[test]
    fn rejects_noncanonical_tail_and_wrong_shape() {
        assert_eq!(
            CanonicalBitplane::from_words(65, vec![0, 2]),
            Err(BooleanXorError::NonzeroTail { invalid_bits: 2 })
        );
        assert_eq!(
            CanonicalBitplane::from_words(65, vec![0]),
            Err(BooleanXorError::WordCount {
                expected: 2,
                observed: 1
            })
        );
        let a = plane(65, 1);
        let b = plane(64, 2);
        assert_eq!(
            xor_sparse_exact(&a, &b),
            Err(BooleanXorError::WidthMismatch {
                left: 65,
                right: 64
            })
        );
    }
}
