//! PCEQ-3: sparse published i8 blocks, with no payload for quantized-zero blocks.
//!
//! Research-only fixed-point i16 experiment. The producer is trusted to give a
//! version-coherent envelope BEFORE any value; an incorrect CertifiedZero is
//! unobservable by design and must never be treated as self-authenticating.
//! See docs/research/PCEQ3_SPARSE_PAYLOAD.md.

use crate::pceq0::{
    check_block_size, encode_value, exact_exponent, PceqStreamingError, ProducerBound, MAX_BLOCK,
};

/// Immutable sparse block snapshot. Active blocks have full i8 block payloads,
/// while inactive blocks have no i8 payload at all. A block can be inactive
/// either because it is certified all-zero or all its encoded codes are zero.
///
/// `exponents` holds one byte per block; `active_words` is a packed bitset.
/// Blocks are stored in ascending order, so a sequential consumer needs no
/// per-block 64-bit offset index. The sparse representation has no public
/// mutable access; update requires creating a new complete snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SparsePceqI16 {
    logical_len: usize,
    block_size: usize,
    codes: Vec<i8>,
    exponents: Vec<u8>,
    active_words: Vec<u64>,
}

impl SparsePceqI16 {
    #[must_use]
    pub const fn logical_len(&self) -> usize {
        self.logical_len
    }

    #[must_use]
    pub const fn block_size(&self) -> usize {
        self.block_size
    }

    #[must_use]
    pub fn codes(&self) -> &[i8] {
        &self.codes
    }

    #[must_use]
    pub fn exponents(&self) -> &[u8] {
        &self.exponents
    }

    #[must_use]
    pub fn active_words(&self) -> &[u64] {
        &self.active_words
    }

    #[must_use]
    pub fn active_blocks(&self) -> usize {
        self.active_words
            .iter()
            .map(|w| w.count_ones() as usize)
            .sum()
    }

    /// Only logical i8 payload + u8 exponents + u64 activity bitset; not RSS,
    /// allocator capacity, write traffic or an interoperable wire format.
    #[must_use]
    pub fn logical_bytes(&self) -> u128 {
        self.codes.len() as u128
            + self.exponents.len() as u128
            + self.active_words.len() as u128 * 8
    }

    fn is_active(&self, block: usize) -> bool {
        (self.active_words[block / 64] >> (block % 64)) & 1 != 0
    }

    /// Reconstructs the published *quantized* values. No attempt is made to
    /// reconstruct original i16 values exactly (this codec is lossy).
    #[must_use]
    pub fn reconstruct(&self) -> Vec<i32> {
        let mut output = Vec::with_capacity(self.logical_len);
        let mut offset = 0;
        for (block, &exponent) in self.exponents.iter().enumerate() {
            let count = (self.logical_len - block * self.block_size).min(self.block_size);
            if self.is_active(block) {
                output.extend(
                    self.codes[offset..offset + count]
                        .iter()
                        .map(|&x| i32::from(x) << exponent),
                );
                offset += count;
            } else {
                output.resize(output.len() + count, 0);
            }
        }
        debug_assert_eq!(offset, self.codes.len());
        output
    }

    /// When a block is inactive, neither its i8 payload nor its weight values
    /// are read. The shape must match; all arithmetic is exact in i128.
    #[must_use]
    pub fn weighted_sum(&self, weights: &[i16]) -> Option<i128> {
        if weights.len() != self.logical_len {
            return None;
        }
        let mut sum = 0_i128;
        let mut offset = 0;
        for (block, &exponent) in self.exponents.iter().enumerate() {
            let start = block * self.block_size;
            let count = (self.logical_len - start).min(self.block_size);
            if self.is_active(block) {
                let scale = 1_i128 << exponent;
                for (&code, &weight) in self.codes[offset..offset + count]
                    .iter()
                    .zip(&weights[start..start + count])
                {
                    sum += i128::from(code) * scale * i128::from(weight);
                }
                offset += count;
            }
        }
        debug_assert_eq!(offset, self.codes.len());
        Some(sum)
    }
}

/// Single-read producer with sparse materialization of active i8 blocks.
///
/// `block_bound` runs once per logical block before `producer`. An explicit
/// `CertifiedZero` avoids ALL producer calls and ALL i8 payload writes for
/// that block. For `MaxAbs`, the source is called exactly once per value;
/// each bound is verified and its quantized i8 code is produced once into
/// bounded 128-byte scratch. Only active *quantized* blocks are published.
///
/// The temporary i8 block is revisited to append its codes when active, but
/// source i16 values are never stored or reread. This is a deliberate tradeoff
/// for eliding inactive payload writes and reads. A stronger bound does not
/// imply an exact max; it may increase quantization error.
///
/// False `CertifiedZero` hints cannot be checked without observing source
/// values. Thus the producer MUST own and version its envelope coherently.
pub fn quantize_sparse_bounded<F, B>(
    len: usize,
    block_size: usize,
    mut producer: F,
    mut block_bound: B,
) -> Result<SparsePceqI16, PceqStreamingError>
where
    F: FnMut(usize) -> i16,
    B: FnMut(usize) -> ProducerBound,
{
    check_block_size(block_size).map_err(|_| PceqStreamingError::InvalidBlockSize(block_size))?;
    let block_count = len.div_ceil(block_size);
    let mut result = SparsePceqI16 {
        logical_len: len,
        block_size,
        codes: Vec::new(),
        exponents: Vec::with_capacity(block_count),
        active_words: Vec::with_capacity(block_count.div_ceil(64)),
    };
    let mut scratch = [0_i8; MAX_BLOCK];
    for (block, start) in (0..len).step_by(block_size).enumerate() {
        if block % 64 == 0 {
            result.active_words.push(0);
        }
        let count = (len - start).min(block_size);
        match block_bound(block) {
            ProducerBound::CertifiedZero => result.exponents.push(0),
            ProducerBound::MaxAbs(bound) => {
                if bound > u32::from(i16::MIN.unsigned_abs()) {
                    return Err(PceqStreamingError::OutOfRangeBound { block, bound });
                }
                let exponent = exact_exponent(bound);
                let mut active = false;
                for (index, slot) in scratch[..count].iter_mut().enumerate() {
                    let value_index = start + index;
                    let value = producer(value_index);
                    let observed = u32::from(value.unsigned_abs());
                    if observed > bound {
                        return Err(PceqStreamingError::BrokenBound {
                            index: value_index,
                            bound,
                            observed,
                        });
                    }
                    let code = encode_value(value, exponent);
                    *slot = code;
                    active |= code != 0;
                }
                result.exponents.push(exponent);
                if active {
                    result.active_words[block / 64] |= 1_u64 << (block % 64);
                    result.codes.extend_from_slice(&scratch[..count]);
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pceq0::quantize_streaming_bounded;

    fn recipe(index: usize, block_size: usize) -> i16 {
        let block = index / block_size;
        let slot = index % block_size;
        match block % 4 {
            0 => 0,
            1 => {
                if slot == 0 {
                    i16::MIN
                } else if slot == 1 {
                    i16::MAX
                } else {
                    (slot % 181) as i16 - 90
                }
            }
            2 => {
                if slot == 0 {
                    2_000
                } else {
                    (slot % 125) as i16 - 62
                }
            }
            _ => 0,
        }
    }

    fn recipe_bound(block: usize) -> ProducerBound {
        match block % 4 {
            0 | 3 => ProducerBound::CertifiedZero,
            1 => ProducerBound::MaxAbs(32_768),
            2 => ProducerBound::MaxAbs(2_000),
            _ => unreachable!(),
        }
    }

    #[test]
    fn sparse_matches_verified_dense_stream_and_skips_zero_generation() {
        for block_size in [32, 64, 128] {
            for len in [0, 1, 31, 32, 33, 63, 64, 65, 129, 257, 1025] {
                let mut producer_calls = 0_usize;
                let mut bound_calls = 0_usize;
                let sparse = quantize_sparse_bounded(
                    len,
                    block_size,
                    |i| {
                        producer_calls += 1;
                        recipe(i, block_size)
                    },
                    |block| {
                        bound_calls += 1;
                        recipe_bound(block)
                    },
                )
                .unwrap();
                let dense = quantize_streaming_bounded(
                    len,
                    block_size,
                    |i| recipe(i, block_size),
                    recipe_bound,
                )
                .unwrap();
                let expected_calls = (0..len)
                    .filter(|&i| i / block_size % 4 == 1 || i / block_size % 4 == 2)
                    .count();
                assert_eq!(producer_calls, expected_calls);
                assert_eq!(bound_calls, len.div_ceil(block_size));
                assert_eq!(sparse.reconstruct(), dense.reconstruct());
                assert_eq!(sparse.logical_len(), len);
                assert_eq!(sparse.exponents().len(), len.div_ceil(block_size));
                assert_eq!(
                    sparse.active_words().len(),
                    len.div_ceil(block_size).div_ceil(64)
                );
                assert!(sparse.codes().len() <= dense.codes().len());
                let weights = (0..len).map(|i| i as i16 % 29 - 14).collect::<Vec<_>>();
                assert_eq!(sparse.weighted_sum(&weights), dense.weighted_sum(&weights));
                assert_eq!(
                    sparse.weighted_sum(&weights[..len.saturating_sub(1)]),
                    if len == 0 { Some(0) } else { None }
                );
            }
        }
    }

    #[test]
    fn certified_zero_blocks_use_no_source_payload_even_across_bitmap_words() {
        let len = 66 * 32 + 7;
        let mut calls = 0;
        let snapshot = quantize_sparse_bounded(
            len,
            32,
            |i| {
                calls += 1;
                if i / 32 == 1 || i / 32 == 63 || i / 32 == 64 {
                    5
                } else {
                    0
                }
            },
            |block| {
                if [1, 63, 64].contains(&block) {
                    ProducerBound::MaxAbs(5)
                } else {
                    ProducerBound::CertifiedZero
                }
            },
        )
        .unwrap();
        assert_eq!(calls, 96);
        assert_eq!(snapshot.active_blocks(), 3);
        assert_eq!(snapshot.codes().len(), 96);
        assert_eq!(snapshot.active_words().len(), 2);
        assert_eq!(snapshot.active_words()[0], (1 << 1) | (1 << 63));
        assert_eq!(snapshot.active_words()[1], 1);
        assert_eq!(snapshot.logical_bytes(), 96 + 67 + 16);
        assert_eq!(snapshot.reconstruct().len(), len);
        assert_eq!(snapshot.weighted_sum(&vec![1_i16; len]), Some(480));
    }

    #[test]
    fn a_nonzero_source_can_have_a_skippable_quantized_zero_block() {
        let mut calls = 0;
        let encoded = quantize_sparse_bounded(
            32,
            32,
            |_| {
                calls += 1;
                1
            },
            |_| ProducerBound::MaxAbs(32_768),
        )
        .unwrap();
        assert_eq!(calls, 32);
        assert_eq!(encoded.exponents(), &[9]);
        assert_eq!(encoded.active_blocks(), 0);
        assert!(encoded.codes().is_empty());
        assert_eq!(encoded.reconstruct(), vec![0; 32]);
        assert_eq!(encoded.logical_bytes(), 9);
    }

    #[test]
    fn invalid_and_stale_bounds_fail_closed_without_published_snapshot() {
        let mut called = false;
        let invalid = quantize_sparse_bounded(
            32,
            32,
            |_| {
                called = true;
                0
            },
            |_| ProducerBound::MaxAbs(32_769),
        );
        assert_eq!(
            invalid,
            Err(PceqStreamingError::OutOfRangeBound {
                block: 0,
                bound: 32_769
            })
        );
        assert!(!called);

        let mut seen = 0;
        let broken = quantize_sparse_bounded(
            33,
            32,
            |i| {
                seen += 1;
                if i == 32 {
                    9
                } else {
                    2
                }
            },
            |_| ProducerBound::MaxAbs(8),
        );
        assert_eq!(
            broken,
            Err(PceqStreamingError::BrokenBound {
                index: 32,
                bound: 8,
                observed: 9
            })
        );
        assert_eq!(seen, 33);

        let producer_called = std::cell::Cell::new(false);
        let bound_called = std::cell::Cell::new(false);
        let invalid_size = quantize_sparse_bounded(
            32,
            0,
            |_| {
                producer_called.set(true);
                0
            },
            |_| {
                bound_called.set(true);
                ProducerBound::CertifiedZero
            },
        );
        assert_eq!(invalid_size, Err(PceqStreamingError::InvalidBlockSize(0)));
        assert!(!producer_called.get() && !bound_called.get());
    }
}
