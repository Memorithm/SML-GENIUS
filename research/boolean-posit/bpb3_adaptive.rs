//! Research-only adaptive Boolean bitplanes over 64 FP32-source coefficients.
//! This is not a scalar IEEE/Posit Standard numeric type, a GGUF format,
//! a deployment-ready codec or an SML model-quality result.
//!
//! Each serialized block is [mode: u8, BPB-3 regime scale: u16 LE,
//! negative: u64 LE, mode magnitude planes: u64 LE each]. Mode 1/2/3
//! uses 19/27/35 bytes respectively and stores signed codes in
//! [-1,1]/[-3,3]/[-7,7]. The sign bit must be zero for a zero code.
//! The original fixed BPB-3 is byte-identical to a mode-2 block without
//! the extra leading mode byte. Adaptive budgets INCLUDE that byte.
//!
//! A bounded exact dynamic program selects precision per block to minimize
//! the sum of reconstruction SSE under a global serialized-byte budget.
//! Source weights are required by any quantizer; the budget, codebook and
//! evaluation protocol are frozen independently of held-out measurements.
#![forbid(unsafe_code)]

#[path = "prototype.rs"]
#[allow(dead_code)]
mod fixed;

use std::fs;
use std::path::Path;

pub const LANES: usize = 64;
pub const MIN_BYTES: usize = 19;
pub const STEP_BYTES: usize = 8;
pub const FIXED_TAGGED_BYTES: usize = 27;
pub const MAX_BLOCKS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidMode(u8),
    InvalidExponent(i8),
    InvalidCode { index: usize, value: i8, mode: u8 },
    WrongLength { actual: usize, expected: usize },
    NoncanonicalExponent,
    NegativeZero,
    NonFiniteSource { block: usize, index: usize },
    UnrepresentableSse,
    EmptyInput,
    TooManyBlocks(usize),
    BudgetTooSmall { budget: usize, minimum: usize },
    BudgetOverflow,
    NoCandidate,
    ArithmeticOverflow,
}

pub fn wire_bytes(mode: u8) -> Result<usize, Error> {
    if (1..=3).contains(&mode) {
        Ok(11 + usize::from(mode) * STEP_BYTES)
    } else {
        Err(Error::InvalidMode(mode))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub mode: u8,
    pub exponent: i8,
    pub codes: [i8; LANES],
    pub sse: f64,
}

#[derive(Clone, Debug)]
pub struct Allocation {
    pub wires: Vec<Vec<u8>>,
    pub choices: Vec<Choice>,
    pub total_bytes: usize,
    pub sse: f64,
}

pub fn pack(codes: &[i8; LANES], exponent: i8, mode: u8) -> Result<Vec<u8>, Error> {
    let bytes = wire_bytes(mode)?;
    let scale = fixed::encode_scale(exponent).ok_or(Error::InvalidExponent(exponent))?;
    let mut negative = 0u64;
    let mut planes = [0u64; 3];
    let maximum = (1i16 << mode) - 1;
    for (index, &code) in codes.iter().enumerate() {
        if i16::from(code).abs() > maximum {
            return Err(Error::InvalidCode {
                index,
                value: code,
                mode,
            });
        }
        let magnitude = code.unsigned_abs();
        let bit = 1u64 << index;
        if code < 0 {
            negative |= bit;
        }
        for plane in 0..usize::from(mode) {
            if magnitude & (1 << plane) != 0 {
                planes[plane] |= bit;
            }
        }
    }
    let mut result = Vec::with_capacity(bytes);
    result.push(mode);
    result.extend_from_slice(&scale.to_le_bytes());
    result.extend_from_slice(&negative.to_le_bytes());
    for plane in &planes[..usize::from(mode)] {
        result.extend_from_slice(&plane.to_le_bytes());
    }
    debug_assert_eq!(result.len(), bytes);
    Ok(result)
}

fn parse(raw: &[u8]) -> Result<(u8, i8, u64, [u64; 3]), Error> {
    let Some(&mode) = raw.first() else {
        return Err(Error::WrongLength {
            actual: 0,
            expected: 1,
        });
    };
    let bytes = wire_bytes(mode)?;
    if raw.len() != bytes {
        return Err(Error::WrongLength {
            actual: raw.len(),
            expected: bytes,
        });
    }
    let scale = u16::from_le_bytes([raw[1], raw[2]]);
    let exponent = fixed::decode_scale(scale).ok_or(Error::NoncanonicalExponent)?;
    let negative = u64::from_le_bytes(raw[3..11].try_into().unwrap());
    let mut planes = [0u64; 3];
    let mut active = 0u64;
    for (index, plane) in planes[..usize::from(mode)].iter_mut().enumerate() {
        let start = 11 + 8 * index;
        *plane = u64::from_le_bytes(raw[start..start + 8].try_into().unwrap());
        active |= *plane;
    }
    if negative & !active != 0 {
        return Err(Error::NegativeZero);
    }
    Ok((mode, exponent, negative, planes))
}

pub fn unpack(raw: &[u8]) -> Result<([i8; LANES], i8), Error> {
    let (mode, exponent, negative, planes) = parse(raw)?;
    let codes = std::array::from_fn(|index| {
        let magnitude: i8 = (0..usize::from(mode))
            .map(|p| (((planes[p] >> index) & 1) << p) as i8)
            .sum();
        if negative & (1u64 << index) != 0 {
            -magnitude
        } else {
            magnitude
        }
    });
    Ok((codes, exponent))
}

/// Exact integer Boolean dot for one block, with an independent dense oracle.
/// One bipolar input bit set means -1; clear means +1.
pub fn dot(raw: &[u8], input_negative: u64) -> Result<(i32, i8), Error> {
    let (mode, exponent, negative, planes) = parse(raw)?;
    let equals = !(negative ^ input_negative);
    let mut integer = 0i32;
    for (p, &plane) in planes[..usize::from(mode)].iter().enumerate() {
        let count = 2 * (equals & plane).count_ones() as i32 - plane.count_ones() as i32;
        integer += count * (1 << p);
    }
    Ok((integer, exponent))
}

/// Inter-block dyadic accumulation in exact fixed point with scale 2^-24.
/// This avoids f32 reassociation; it is not ggml's FP32 accumulation contract.
pub fn dot_sum_exact(wires: &[Vec<u8>], bipolar: &[u64]) -> Result<i128, Error> {
    if wires.len() != bipolar.len() {
        return Err(Error::WrongLength {
            actual: bipolar.len(),
            expected: wires.len(),
        });
    }
    let mut total = 0i128;
    for (wire, &mask) in wires.iter().zip(bipolar) {
        let (integer, exponent) = dot(wire, mask)?;
        let multiplier = 1i128 << (i32::from(exponent) + 24);
        let term = i128::from(integer)
            .checked_mul(multiplier)
            .ok_or(Error::ArithmeticOverflow)?;
        total = total.checked_add(term).ok_or(Error::ArithmeticOverflow)?;
    }
    Ok(total)
}

/// Exhaustively select the best shared power of two for a declared mode.
/// No TEST-informed exponent limits, correction tables or sampled candidates.
pub fn quantize(source: &[f64; LANES], mode: u8, block: usize) -> Result<Choice, Error> {
    wire_bytes(mode)?;
    for (index, &value) in source.iter().enumerate() {
        if !value.is_finite() {
            return Err(Error::NonFiniteSource { block, index });
        }
    }
    let mut best = Choice {
        mode,
        exponent: -24,
        codes: [0; LANES],
        sse: f64::INFINITY,
    };
    let max = ((1u16 << mode) - 1) as f64;
    for exponent in -24i8..=27 {
        let scale = 2.0f64.powi(i32::from(exponent));
        let mut codes = [0i8; LANES];
        let mut error = 0.0;
        for (index, &value) in source.iter().enumerate() {
            let code = (value / scale).round().clamp(-max, max) as i8;
            codes[index] = code;
            let residual = value - f64::from(code) * scale;
            error += residual * residual;
        }
        if error < best.sse {
            best.sse = error;
            best.exponent = exponent;
            best.codes = codes;
        }
    }
    if !best.sse.is_finite() {
        return Err(Error::UnrepresentableSse);
    }
    Ok(best)
}

pub fn fixed_mode(source: &[[f64; LANES]], mode: u8) -> Result<Allocation, Error> {
    wire_bytes(mode)?;
    let mut wires = Vec::with_capacity(source.len());
    let mut choices = Vec::with_capacity(source.len());
    let mut sse = 0.0;
    for (block, weights) in source.iter().enumerate() {
        let choice = quantize(weights, mode, block)?;
        wires.push(pack(&choice.codes, choice.exponent, mode)?);
        sse += choice.sse;
        choices.push(choice);
    }
    Ok(Allocation {
        total_bytes: source
            .len()
            .checked_mul(wire_bytes(mode)?)
            .ok_or(Error::BudgetOverflow)?,
        wires,
        choices,
        sse,
    })
}

/// Exact bounded multiple-choice knapsack: cost 19+8*(mode-1) per block.
/// In ties, stable iteration prefers the lower-used-budget predecessor.
/// No extra precision is granted without paying its mode byte/bitplanes.
pub fn allocate(source: &[[f64; LANES]], budget_bytes: usize) -> Result<Allocation, Error> {
    let count = source.len();
    if count == 0 {
        return Err(Error::EmptyInput);
    }
    if count > MAX_BLOCKS {
        return Err(Error::TooManyBlocks(count));
    }
    let minimum = count.checked_mul(MIN_BYTES).ok_or(Error::BudgetOverflow)?;
    if budget_bytes < minimum {
        return Err(Error::BudgetTooSmall {
            budget: budget_bytes,
            minimum,
        });
    }
    let max_extra = (budget_bytes - minimum) / STEP_BYTES;
    let max_extra = max_extra.min(2 * count);
    let width = max_extra + 1;
    let mut candidates = Vec::with_capacity(count);
    for (index, weights) in source.iter().enumerate() {
        candidates.push([
            quantize(weights, 1, index)?,
            quantize(weights, 2, index)?,
            quantize(weights, 3, index)?,
        ]);
    }
    // Rows represent completed source blocks; columns are consumed 8-byte
    // precision increments above the mandatory 19-byte per-block cost.
    let mut dp = vec![f64::INFINITY; (count + 1) * width];
    let mut back = vec![0u8; (count + 1) * width];
    dp[0] = 0.0;
    for i in 0..count {
        for spent in 0..=max_extra {
            let prev = dp[i * width + spent];
            if !prev.is_finite() {
                continue;
            }
            for mode in 1..=3usize {
                let next = spent + mode - 1;
                if next > max_extra {
                    continue;
                }
                let candidate = prev + candidates[i][mode - 1].sse;
                let pos = (i + 1) * width + next;
                if candidate < dp[pos] {
                    dp[pos] = candidate;
                    back[pos] = mode as u8;
                }
            }
        }
    }
    let mut best = f64::INFINITY;
    let mut spent = 0usize;
    for extra in 0..=max_extra {
        let value = dp[count * width + extra];
        if value < best {
            best = value;
            spent = extra;
        }
    }
    if !best.is_finite() {
        return Err(Error::NoCandidate);
    }
    let actual_extra = spent;
    let mut choices = Vec::with_capacity(count);
    for i in (1..=count).rev() {
        let mode = back[i * width + spent];
        if !(1..=3).contains(&mode) {
            return Err(Error::NoCandidate);
        }
        choices.push(candidates[i - 1][usize::from(mode - 1)].clone());
        spent -= usize::from(mode - 1);
    }
    choices.reverse();
    let wires = choices
        .iter()
        .map(|q| pack(&q.codes, q.exponent, q.mode))
        .collect::<Result<Vec<_>, _>>()?;
    let total_bytes = minimum + actual_extra * STEP_BYTES;
    debug_assert_eq!(wires.iter().map(Vec::len).sum::<usize>(), total_bytes);
    debug_assert!(total_bytes <= budget_bytes);
    Ok(Allocation {
        wires,
        choices,
        total_bytes,
        sse: best,
    })
}

fn dense_dot(codes: &[i8; LANES], input_negative: u64) -> i32 {
    codes
        .iter()
        .enumerate()
        .map(|(i, &code)| {
            if (input_negative >> i) & 1 != 0 {
                -i32::from(code)
            } else {
                i32::from(code)
            }
        })
        .sum()
}

fn rand64(seed: &mut u64) -> u64 {
    *seed ^= *seed >> 12;
    *seed ^= *seed << 25;
    *seed ^= *seed >> 27;
    (*seed).wrapping_mul(2685821657736338717)
}

fn evaluate(split: &str, data: &[[f64; LANES]]) -> Result<(), Error> {
    let fixed = fixed_mode(data, 2)?;
    let adaptive = allocate(data, data.len() * FIXED_TAGGED_BYTES)?;
    for (name, result) in [
        ("fixed_3bit_tagged", &fixed),
        ("adaptive_2to4bit", &adaptive),
    ] {
        let mut squared = 0.0f64;
        let mut dot_squared = 0.0f64;
        let mut count = 0;
        let mut masks = 0;
        let mut rng = 0x74ac_6d91_5e23_8831u64;
        for (original, wire) in data.iter().zip(&result.wires) {
            let (codes, exponent) = unpack(wire)?;
            let scale = 2.0f64.powi(i32::from(exponent));
            for (reference, code) in original.iter().zip(codes) {
                let error = reference - f64::from(code) * scale;
                squared += error * error;
                count += 1;
            }
            for _ in 0..16 {
                let mask = rand64(&mut rng);
                let actual = original
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| if (mask >> i) & 1 == 0 { v } else { -v })
                    .sum::<f64>();
                let (integer, _) = dot(wire, mask)?;
                assert_eq!(integer, dense_dot(&codes, mask));
                let delta = f64::from(integer) * scale - actual;
                dot_squared += delta * delta;
                masks += 1;
            }
        }
        assert!((squared - result.sse).abs() <= 1e-8 * squared.max(1.0));
        let m = [1u8, 2, 3].map(|mode| result.choices.iter().filter(|c| c.mode == mode).count());
        println!(
            "{split},{name},{},{:.6},{:.12},{:.12},{},{},{},{}",
            result.total_bytes,
            result.total_bytes as f64 * 8.0 / count as f64,
            (squared / count as f64).sqrt(),
            (dot_squared / masks as f64).sqrt(),
            count,
            m[0],
            m[1],
            m[2]
        );
    }
    Ok(())
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 3 || args[1] != "--external-f32le" {
        eprintln!("usage: bpb3-adaptive --external-f32le path-to-48x256-f32le");
        std::process::exit(2);
    }
    let raw = fs::read(Path::new(&args[2])).expect("read declared external tensor slice");
    const PAIRS: usize = 48 * 256;
    assert_eq!(raw.len(), PAIRS * 4, "STOP_NOT_48_X_256_F32LE");
    let mut values = Vec::with_capacity(PAIRS / LANES);
    for block in raw.chunks_exact(LANES * 4) {
        let mut numbers = [0.0; LANES];
        for (i, bytes) in block.chunks_exact(4).enumerate() {
            numbers[i] = f64::from(f32::from_le_bytes(bytes.try_into().unwrap()));
        }
        values.push(numbers);
    }
    println!("protocol=bpb3-adaptive-fixed-budget-v1,source=EXTERNAL_UNVERIFIED,split=24x256-dev-24x256-test,adaptive_budget=108B_per256,claim=RECONSTRUCTION_ONLY");
    println!("split,codec,payload_bytes,bits_per_weight,weight_rmse,bipolar_dot_rmse,weights,mode1_blocks,mode2_blocks,mode3_blocks");
    evaluate("DEV", &values[..96]).expect("DEV evaluation failed");
    evaluate("TEST", &values[96..]).expect("TEST evaluation failed");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic(seed: usize) -> [f64; LANES] {
        let mut rng = 0x92b7_d119_3e65_8f31u64 ^ (seed as u64 + 7);
        std::array::from_fn(|index| {
            let u = (rand64(&mut rng) >> 11) as f64 / ((1u64 << 53) as f64);
            let signed = 2.0 * u - 1.0;
            match seed % 4 {
                0 => signed * 0.9,
                1 => {
                    if index % 6 == 0 {
                        signed * 0.7
                    } else {
                        0.0
                    }
                }
                2 => {
                    if index % 31 == 0 {
                        signed * 7.0
                    } else {
                        signed * 0.12
                    }
                }
                _ => signed * 2.0f64.powi((index % 13) as i32 - 10),
            }
        })
    }

    #[test]
    fn format_budget_and_original_bpb3_exact_bytes() {
        assert_eq!(wire_bytes(1), Ok(19));
        assert_eq!(wire_bytes(2), Ok(27));
        assert_eq!(wire_bytes(3), Ok(35));
        assert_eq!(wire_bytes(0), Err(Error::InvalidMode(0)));
        assert_eq!(wire_bytes(4), Err(Error::InvalidMode(4)));
        let q = std::array::from_fn(|i| (i % 7) as i8 - 3);
        let baseline = fixed::Bpb3::pack(&q, -7).unwrap().to_bytes();
        let adaptive = pack(&q, -7, 2).unwrap();
        assert_eq!(&adaptive[1..], &baseline);
        assert_eq!(unpack(&adaptive), Ok((q, -7)));
    }

    #[test]
    fn exact_bitplanes_match_independent_signed_dense_oracle() {
        let mut rng = 0x1ea3_9b37_f5e2_d017u64;
        for mode in 1..=3u8 {
            for exponent in [-24i8, -15, -1, 0, 1, 15, 27] {
                let max = (1u8 << mode) - 1;
                for case in 0..20usize {
                    let codes = std::array::from_fn(|i| {
                        let value = (rand64(&mut rng) % u64::from(2 * max + 1)) as i8;
                        if i % 7 == case % 7 {
                            0
                        } else {
                            value - max as i8
                        }
                    });
                    let wire = pack(&codes, exponent, mode).unwrap();
                    assert_eq!(unpack(&wire), Ok((codes, exponent)));
                    for mask in [0, u64::MAX, rand64(&mut rng), rand64(&mut rng)] {
                        let (actual, k) = dot(&wire, mask).unwrap();
                        assert_eq!(k, exponent);
                        assert_eq!(actual, dense_dot(&codes, mask));
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_or_noncanonical_bytes_are_rejected() {
        let zero = [0i8; LANES];
        for mode in 1..=3u8 {
            let mut wire = pack(&zero, 0, mode).unwrap();
            assert!(unpack(&wire[..wire.len() - 1]).is_err());
            wire[1] = 0;
            wire[2] = 0;
            assert_eq!(unpack(&wire), Err(Error::NoncanonicalExponent));
            let mut zero_sign = pack(&zero, 0, mode).unwrap();
            zero_sign[3] = 1;
            assert_eq!(unpack(&zero_sign), Err(Error::NegativeZero));
        }
        assert_eq!(
            unpack(&[]),
            Err(Error::WrongLength {
                actual: 0,
                expected: 1
            })
        );
        let mut first = pack(&zero, 0, 1).unwrap();
        first[0] = 9;
        assert_eq!(unpack(&first), Err(Error::InvalidMode(9)));
        let mut invalid = zero;
        invalid[5] = -4;
        assert_eq!(
            pack(&invalid, 0, 2),
            Err(Error::InvalidCode {
                index: 5,
                value: -4,
                mode: 2
            })
        );
    }

    #[test]
    fn quantizer_monotonicity_and_source_validation() {
        for seed in 0..32 {
            let block = synthetic(seed);
            let a = quantize(&block, 1, seed).unwrap();
            let b = quantize(&block, 2, seed).unwrap();
            let c = quantize(&block, 3, seed).unwrap();
            assert!(b.sse <= a.sse + 1e-12, "seed={seed} 2 vs 1");
            assert!(c.sse <= b.sse + 1e-12, "seed={seed} 3 vs 2");
        }
        let mut bad = synthetic(7);
        bad[13] = f64::INFINITY;
        assert_eq!(
            quantize(&bad, 2, 6),
            Err(Error::NonFiniteSource {
                block: 6,
                index: 13
            })
        );
        bad[13] = f64::MAX;
        assert_eq!(quantize(&bad, 2, 6), Err(Error::UnrepresentableSse));
    }

    #[test]
    fn allocation_never_exceeds_tagged_budget_or_worsens_fixed_three_bit_sse() {
        let values: Vec<_> = (0..32).map(synthetic).collect();
        let fixed = fixed_mode(&values, 2).unwrap();
        let budget = fixed.total_bytes;
        let adaptive = allocate(&values, budget).unwrap();
        assert!(adaptive.total_bytes <= budget);
        assert!(adaptive.sse <= fixed.sse + 1e-11);
        assert_eq!(
            adaptive.wires.iter().map(Vec::len).sum::<usize>(),
            adaptive.total_bytes
        );
        assert_eq!(allocate(&[], 0).unwrap_err(), Error::EmptyInput);
        assert_eq!(
            allocate(&values, MIN_BYTES * 32 - 1).unwrap_err(),
            Error::BudgetTooSmall {
                budget: MIN_BYTES * 32 - 1,
                minimum: MIN_BYTES * 32
            }
        );
        assert_eq!(
            allocate(&vec![[0.0; 64]; MAX_BLOCKS + 1], 9000).unwrap_err(),
            Error::TooManyBlocks(MAX_BLOCKS + 1)
        );
    }

    #[test]
    fn exact_global_bit_allocation_matches_bruteforce_on_small_corpus() {
        let values: Vec<_> = (0..4).map(synthetic).collect();
        let budget = 4 * FIXED_TAGGED_BYTES;
        let dynamic = allocate(&values, budget).unwrap();
        let mut best = f64::INFINITY;
        for choices in 0..81usize {
            let mut number = choices;
            let mut total = 0;
            let mut sse = 0.0;
            for (index, block) in values.iter().enumerate() {
                let mode = (number % 3 + 1) as u8;
                number /= 3;
                total += wire_bytes(mode).unwrap();
                sse += quantize(block, mode, index).unwrap().sse;
            }
            if total <= budget && sse < best {
                best = sse;
            }
        }
        assert_eq!(dynamic.sse.to_bits(), best.to_bits());
    }

    #[test]
    fn heterogeneous_blocks_have_strict_useful_mode_adaptation() {
        let exact_low = std::array::from_fn(|i| ((i % 3) as i8 - 1) as f64);
        let exact_high = std::array::from_fn(|i| ((i % 15) as i8 - 7) as f64);
        let values = [exact_low, exact_high];
        let fixed = fixed_mode(&values, 2).unwrap();
        let mixed = allocate(&values, FIXED_TAGGED_BYTES * 2).unwrap();
        assert_eq!(mixed.choices[0].mode, 1);
        assert_eq!(mixed.choices[1].mode, 3);
        assert_eq!(mixed.total_bytes, 54);
        assert!(mixed.sse < fixed.sse);
        assert_eq!(mixed.sse, 0.0);
    }

    #[test]
    fn exact_interblock_fixed_point_accumulation_including_extreme_scales() {
        let mut rng = 0x1931_913b_8a6e_20e9u64;
        let mut wires = Vec::new();
        let mut masks = Vec::new();
        let mut expected = 0i128;
        for (mode, exponent) in [(1, -24i8), (2, 0), (3, 27), (1, 12), (3, -8)] {
            let max = (1i8 << mode) - 1;
            let codes = std::array::from_fn(|i| {
                if i % 3 == 0 {
                    -max
                } else if i % 3 == 1 {
                    max
                } else {
                    0
                }
            });
            let wire = pack(&codes, exponent, mode).unwrap();
            let mask = rand64(&mut rng);
            expected += i128::from(dense_dot(&codes, mask)) << (i32::from(exponent) + 24);
            wires.push(wire);
            masks.push(mask);
        }
        assert_eq!(dot_sum_exact(&wires, &masks), Ok(expected));
        assert!(matches!(
            dot_sum_exact(&wires, &[]),
            Err(Error::WrongLength { .. })
        ));
    }
}
