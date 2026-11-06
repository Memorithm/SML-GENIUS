//! PCEQ-0: bounded producer-carried quantization (research-only).
//!
//! Converts signed fixed-point i16 producer values to block-scaled i8 without
//! first materializing the entire i16 tensor. It is NOT a drop-in path for
//! SML's exact Boolean ANF/PVP operations, nor a measured DRAM speedup.
//! See docs/research/PCEQ0_PRODUCER_CARRIED_BLOCKS.md.

/// Highest supported number of unquantized values held in a local block.
pub const MAX_BLOCK: usize = 128;

/// No conversion or silent block-size adjustment is performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PceqError {
    UnsupportedBlockSize(usize),
}

/// Immutable per-block metadata; exact numeric exponent and a skip hint.
/// The active flag describes *quantized* nonzero values, not source nonzeros.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockMeta {
    exponent: u8,
    active: bool,
}

impl BlockMeta {
    #[must_use]
    pub const fn exponent(self) -> u8 {
        self.exponent
    }

    #[must_use]
    pub const fn active(self) -> bool {
        self.active
    }
}

/// An immutable quantized snapshot. Its producer-owned metadata cannot become
/// stale via a public mutable payload reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PceqI16 {
    codes: Vec<i8>,
    block_size: usize,
    metadata: Vec<BlockMeta>,
}

impl PceqI16 {
    #[must_use]
    pub fn codes(&self) -> &[i8] {
        &self.codes
    }

    #[must_use]
    pub fn metadata(&self) -> &[BlockMeta] {
        &self.metadata
    }

    #[must_use]
    pub const fn block_size(&self) -> usize {
        self.block_size
    }

    /// Logical packed payload + two bytes per metadata entry (not RSS,
    /// allocator capacity, alignment, DRAM traffic, or a serialized format).
    #[must_use]
    pub fn logical_bytes(&self) -> u128 {
        self.codes.len() as u128 + 2 * self.metadata.len() as u128
    }

    #[must_use]
    pub fn reconstruct(&self) -> Vec<i32> {
        self.codes
            .chunks(self.block_size)
            .zip(&self.metadata)
            .flat_map(|(codes, meta)| {
                codes
                    .iter()
                    .map(move |&code| i32::from(code) << meta.exponent)
            })
            .collect()
    }

    /// Signed weighted sum over reconstructed values. Entire inactive blocks
    /// are skipped without reading their payload. Returns None on bad shape.
    #[must_use]
    pub fn weighted_sum(&self, weights: &[i16]) -> Option<i128> {
        if weights.len() != self.codes.len() {
            return None;
        }
        let mut total = 0_i128;
        for (block_index, meta) in self.metadata.iter().enumerate() {
            if !meta.active {
                continue;
            }
            let start = block_index * self.block_size;
            let end = (start + self.block_size).min(self.codes.len());
            let multiplier = 1_i128 << meta.exponent;
            for (&code, &weight) in self.codes[start..end].iter().zip(&weights[start..end]) {
                total += i128::from(code) * multiplier * i128::from(weight);
            }
        }
        Some(total)
    }
}

fn check_block_size(block_size: usize) -> Result<(), PceqError> {
    match block_size {
        32 | 64 | 128 => Ok(()),
        _ => Err(PceqError::UnsupportedBlockSize(block_size)),
    }
}

/// Smallest nonnegative power-of-two shift with max_abs <= 127 * 2^shift.
fn exact_exponent(max_abs: u32) -> u8 {
    let mut exponent = 0_u8;
    while max_abs > (127_u32 << exponent) {
        exponent += 1;
    }
    exponent
}

/// Upper bound from a bitwise OR envelope. This deliberately ignores some
/// mantissa information; it is safe but can be coarser than exact absmax.
fn or_exponent(or_magnitudes: u32) -> u8 {
    let width = u32::BITS - or_magnitudes.leading_zeros();
    if width <= 7 {
        0
    } else {
        (width - 6) as u8
    }
}

/// Symmetric integer round-half-away-from-zero. The caller's shift guarantees
/// no clipping for every i16 source, including i16::MIN.
fn encode_value(value: i16, exponent: u8) -> i8 {
    let divisor = 1_u32 << exponent;
    let rounded = (u32::from(value.unsigned_abs()) + divisor / 2) / divisor;
    debug_assert!(rounded <= 127);
    let magnitude = rounded.min(127) as i8;
    if value.is_negative() {
        -magnitude
    } else {
        magnitude
    }
}

fn append_block(values: &[i16], exponent: u8, result: &mut PceqI16) {
    let mut active = false;
    for &value in values {
        let q = encode_value(value, exponent);
        active |= q != 0;
        result.codes.push(q);
    }
    result.metadata.push(BlockMeta { exponent, active });
}

fn new_snapshot(len: usize, block_size: usize) -> PceqI16 {
    PceqI16 {
        codes: Vec::with_capacity(len),
        block_size,
        metadata: Vec::with_capacity(len.div_ceil(block_size)),
    }
}

/// Control: reads a *materialized* i16 block to find the exact scale, then
/// revisits that block to quantize. Producer/materialization cost is external.
pub fn quantize_materialized(values: &[i16], block_size: usize) -> Result<PceqI16, PceqError> {
    check_block_size(block_size)?;
    let mut result = new_snapshot(values.len(), block_size);
    for chunk in values.chunks(block_size) {
        let max_abs = chunk
            .iter()
            .map(|x| u32::from(x.unsigned_abs()))
            .max()
            .unwrap_or(0);
        append_block(chunk, exact_exponent(max_abs), &mut result);
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Envelope {
    Exact,
    Or,
}

fn quantize_from_producer<F>(
    len: usize,
    block_size: usize,
    mut producer: F,
    envelope: Envelope,
) -> Result<PceqI16, PceqError>
where
    F: FnMut(usize) -> i16,
{
    check_block_size(block_size)?;
    let mut result = new_snapshot(len, block_size);
    // Small fixed stack buffer: each value is produced exactly once. Reuse
    // never traverses a full source tensor. Hardware cache residency must be
    // checked on-target; the source-language code does not prove it.
    let mut scratch = [0_i16; MAX_BLOCK];
    for start in (0..len).step_by(block_size) {
        let count = (len - start).min(block_size);
        let mut max_abs = 0_u32;
        let mut or_magnitudes = 0_u32;
        for (offset, slot) in scratch[..count].iter_mut().enumerate() {
            let value = producer(start + offset);
            *slot = value;
            let magnitude = u32::from(value.unsigned_abs());
            max_abs = max_abs.max(magnitude);
            or_magnitudes |= magnitude;
        }
        let exponent = match envelope {
            Envelope::Exact => exact_exponent(max_abs),
            Envelope::Or => or_exponent(or_magnitudes),
        };
        append_block(&scratch[..count], exponent, &mut result);
    }
    Ok(result)
}

/// Exact per-block scale from producer-carried maximum. Each input is
/// generated only once, without allocating a full unquantized tensor.
pub fn quantize_fused<F>(len: usize, block_size: usize, producer: F) -> Result<PceqI16, PceqError>
where
    F: FnMut(usize) -> i16,
{
    quantize_from_producer(len, block_size, producer, Envelope::Exact)
}

/// Experimental OR-envelope scale; safe against clipping but not generally
/// distortion-optimal. Shares the one-producer-call property of exact mode.
pub fn quantize_or_envelope<F>(
    len: usize,
    block_size: usize,
    producer: F,
) -> Result<PceqI16, PceqError>
where
    F: FnMut(usize) -> i16,
{
    quantize_from_producer(len, block_size, producer, Envelope::Or)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(i: usize) -> i16 {
        // Deterministic, mixed-sign, bounded fixture with extreme endpoints.
        match i % 31 {
            0 => i16::MIN,
            1 => i16::MAX,
            2 => 0,
            3 => 127,
            4 => -127,
            5 => 128,
            6 => -128,
            _ => ((i as u32).wrapping_mul(1103515245).wrapping_add(12345) >> 16) as i16,
        }
    }

    #[test]
    fn exact_fused_matches_two_pass_control_with_one_production() {
        for block_size in [32, 64, 128] {
            for len in [0, 1, 2, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 1025] {
                let source: Vec<_> = (0..len).map(sample).collect();
                let baseline = quantize_materialized(&source, block_size).unwrap();
                let mut calls = 0;
                let fused = quantize_fused(len, block_size, |index| {
                    calls += 1;
                    sample(index)
                })
                .unwrap();
                assert_eq!(calls, len);
                assert_eq!(fused, baseline);
                assert_eq!(fused.codes().len(), len);
                assert_eq!(fused.metadata().len(), len.div_ceil(block_size));
                assert_eq!(
                    fused.logical_bytes(),
                    len as u128 + 2 * len.div_ceil(block_size) as u128
                );
            }
        }
    }

    #[test]
    fn or_envelope_is_conservative_with_bounded_reconstruction_error() {
        for block_size in [32, 64, 128] {
            for len in [0, 1, 64, 65, 129, 512] {
                let source: Vec<_> = (0..len).map(sample).collect();
                let exact = quantize_materialized(&source, block_size).unwrap();
                let mut calls = 0;
                let candidate = quantize_or_envelope(len, block_size, |i| {
                    calls += 1;
                    source[i]
                })
                .unwrap();
                assert_eq!(calls, len);
                for (&e, &o) in exact.metadata().iter().zip(candidate.metadata()) {
                    assert!(o.exponent() >= e.exponent());
                }
                for (i, (&original, &recovered)) in source
                    .iter()
                    .zip(candidate.reconstruct().iter())
                    .enumerate()
                {
                    let shift = candidate.metadata()[i / block_size].exponent();
                    let allowed = (1_i32 << shift) / 2;
                    assert!((i32::from(original) - recovered).abs() <= allowed);
                }
            }
        }
    }

    #[test]
    fn inactivity_skips_blocks_and_preserves_weighted_sum() {
        let mut source = vec![0_i16; 130];
        source[64] = i16::MIN;
        source[65] = i16::MAX;
        source[129] = 1;
        let encoded = quantize_fused(source.len(), 64, |i| source[i]).unwrap();
        assert_eq!(encoded.metadata().len(), 3);
        assert!(!encoded.metadata()[0].active());
        assert!(encoded.metadata()[1].active());
        assert!(encoded.metadata()[2].active());
        let weights: Vec<i16> = (0..source.len()).map(|i| (i % 13) as i16 - 6).collect();
        let expected: i128 = encoded
            .reconstruct()
            .iter()
            .zip(&weights)
            .map(|(&v, &w)| i128::from(v) * i128::from(w))
            .sum();
        assert_eq!(encoded.weighted_sum(&weights), Some(expected));
        assert_eq!(encoded.weighted_sum(&weights[..129]), None);
    }

    #[test]
    fn all_zero_data_retains_zero_scale_and_skippable_blocks() {
        for block_size in [32, 64, 128] {
            let encoded = quantize_or_envelope(257, block_size, |_| 0).unwrap();
            assert!(encoded
                .metadata()
                .iter()
                .all(|meta| !meta.active() && meta.exponent() == 0));
            assert!(encoded.codes().iter().all(|&code| code == 0));
        }
    }

    #[test]
    fn rejects_unsupported_block_sizes_without_calling_producer() {
        for invalid in [0, 1, 31, 33, 63, 65, 129] {
            let mut called = false;
            assert_eq!(
                quantize_fused(100, invalid, |_| {
                    called = true;
                    0
                }),
                Err(PceqError::UnsupportedBlockSize(invalid))
            );
            assert!(!called);
        }
        assert_eq!(
            quantize_materialized(&[1, 2, 3], 0),
            Err(PceqError::UnsupportedBlockSize(0))
        );
    }

    #[test]
    fn exhaustive_i16_extrema_and_or_upper_bound() {
        for value in i16::MIN..=i16::MAX {
            let max_abs = u32::from(value.unsigned_abs());
            let shift = exact_exponent(max_abs);
            let or_shift = or_exponent(max_abs);
            assert!(or_shift >= shift);
            for e in [shift, or_shift] {
                let q = encode_value(value, e);
                let reconstructed = i32::from(q) << e;
                assert!((i32::from(value) - reconstructed).abs() <= (1_i32 << e) / 2);
            }
        }
    }
}
