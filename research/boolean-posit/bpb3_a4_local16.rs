//! BPB-3-A4: independent dyadic scale per 16 lanes, and fully costed
//! adaptive precision across 64-lane signed Boolean bitplane blocks.
//! Research only: the wire is NOT Posit Standard 2022 or GGUF-compatible.
#![forbid(unsafe_code)]

use std::fs;
use std::path::Path;

pub const LANES: usize = 64;
pub const SUBGROUPS: usize = 4;
pub const GROUP_WIDTH: usize = 16;
pub const MIN_BYTES: usize = 21;
pub const STEP_BYTES: usize = 8;
pub const LIMIT: usize = 256;
pub const ROW_BYTES: usize = 108;
pub const MASK_SEED: u64 = 0xa3b0_114d_5c15_2026;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Empty,
    TooManyBlocks,
    WrongLength,
    InvalidMode,
    InvalidExponent,
    InvalidCode,
    NegativeZero,
    NonFinite,
    NoCandidate,
    Budget,
    Overflow,
}

pub fn wire_bytes(mode: u8) -> Result<usize, Error> {
    match mode {
        1..=3 => Ok(13 + 8 * usize::from(mode)),
        _ => Err(Error::InvalidMode),
    }
}

#[derive(Clone, Debug)]
pub struct Choice {
    pub mode: u8,
    pub exponents: [i8; SUBGROUPS],
    pub codes: [i8; LANES],
    pub sse: f64,
}

#[derive(Clone, Debug)]
pub struct Allocation {
    pub choices: Vec<Choice>,
    pub wires: Vec<Vec<u8>>,
    pub total_bytes: usize,
    pub sse: f64,
}

/// Wire: mode u8, 4 subgroup exponents i8, 64 sign bits, then mode
/// magnitude bitplanes of 64 LE bits. All metadata is charged to storage.
pub fn pack(choice: &Choice) -> Result<Vec<u8>, Error> {
    let bytes = wire_bytes(choice.mode)?;
    if choice.exponents.iter().any(|&k| !(-24..=27).contains(&k)) {
        return Err(Error::InvalidExponent);
    }
    let max_code = (1i16 << choice.mode) - 1;
    let mut sign = 0u64;
    let mut planes = [0u64; 3];
    for (index, &code) in choice.codes.iter().enumerate() {
        if i16::from(code).abs() > max_code {
            return Err(Error::InvalidCode);
        }
        let bit = 1u64 << index;
        if code < 0 {
            sign |= bit;
        }
        for (p, plane) in planes[..usize::from(choice.mode)].iter_mut().enumerate() {
            if code.unsigned_abs() & (1 << p) != 0 {
                *plane |= bit;
            }
        }
    }
    let mut wire = Vec::with_capacity(bytes);
    wire.push(choice.mode);
    wire.extend(choice.exponents.map(|x| x as u8));
    wire.extend_from_slice(&sign.to_le_bytes());
    for plane in &planes[..usize::from(choice.mode)] {
        wire.extend_from_slice(&plane.to_le_bytes());
    }
    assert_eq!(wire.len(), bytes);
    Ok(wire)
}

fn parse(wire: &[u8]) -> Result<(u8, [i8; SUBGROUPS], u64, [u64; 3]), Error> {
    let mode = *wire.first().ok_or(Error::WrongLength)?;
    if wire.len() != wire_bytes(mode)? {
        return Err(Error::WrongLength);
    }
    let exponents = std::array::from_fn(|i| wire[1 + i] as i8);
    if exponents.iter().any(|&k| !(-24..=27).contains(&k)) {
        return Err(Error::InvalidExponent);
    }
    let sign = u64::from_le_bytes(wire[5..13].try_into().unwrap());
    let mut planes = [0u64; 3];
    let mut occupied = 0u64;
    for (p, plane) in planes[..usize::from(mode)].iter_mut().enumerate() {
        let at = 13 + p * 8;
        *plane = u64::from_le_bytes(wire[at..at + 8].try_into().unwrap());
        occupied |= *plane;
    }
    if sign & !occupied != 0 {
        return Err(Error::NegativeZero);
    }
    Ok((mode, exponents, sign, planes))
}

pub fn unpack(wire: &[u8]) -> Result<([i8; LANES], [i8; SUBGROUPS]), Error> {
    let (mode, exponents, sign, planes) = parse(wire)?;
    let codes = std::array::from_fn(|index| {
        let magnitude = planes[..usize::from(mode)]
            .iter()
            .enumerate()
            .map(|(p, plane)| (((plane >> index) & 1) << p) as i8)
            .sum::<i8>();
        if sign & (1u64 << index) != 0 {
            -magnitude
        } else {
            magnitude
        }
    });
    Ok((codes, exponents))
}

/// One signed bipolar input bit means -1; zero means +1.
/// Returns exact SIGNED integer sums and exponents for each 16-lane subgroup.
pub fn dot_subgroups(wire: &[u8], bipolar_negative: u64) -> Result<[(i32, i8); SUBGROUPS], Error> {
    let (mode, exponents, negative, planes) = parse(wire)?;
    let equal_sign = !(negative ^ bipolar_negative);
    Ok(std::array::from_fn(|group| {
        let subgroup_mask = 0xffffu64 << (group * GROUP_WIDTH);
        let mut integer = 0i32;
        for (p, &plane) in planes[..usize::from(mode)].iter().enumerate() {
            let occupied = plane & subgroup_mask;
            let positive = (occupied & equal_sign).count_ones() as i32;
            let total = occupied.count_ones() as i32;
            integer += (2 * positive - total) * (1 << p);
        }
        (integer, exponents[group])
    }))
}

pub fn dot_256(wires: &[Vec<u8>], masks: &[u64; 4]) -> Result<f64, Error> {
    if wires.len() != 4 {
        return Err(Error::WrongLength);
    }
    let mut sum = 0.0f64;
    for (wire, &mask) in wires.iter().zip(masks) {
        for (integer, exponent) in dot_subgroups(wire, mask)? {
            sum += f64::from(integer) * 2.0f64.powi(i32::from(exponent));
        }
    }
    Ok(sum)
}

/// Exact dyadic signed-dot sum aligned at the smallest exponent 2^-24.
pub fn dot_exact(wires: &[Vec<u8>], masks: &[u64]) -> Result<i128, Error> {
    if wires.len() != masks.len() {
        return Err(Error::WrongLength);
    }
    let mut sum = 0i128;
    for (wire, &mask) in wires.iter().zip(masks) {
        for (integer, k) in dot_subgroups(wire, mask)? {
            let shift = (i32::from(k) + 24) as u32;
            let multiplier = 1i128.checked_shl(shift).ok_or(Error::Overflow)?;
            let term = i128::from(integer)
                .checked_mul(multiplier)
                .ok_or(Error::Overflow)?;
            sum = sum.checked_add(term).ok_or(Error::Overflow)?;
        }
    }
    Ok(sum)
}

/// Exhaustively optimize four independent 16-coefficient source groups.
/// No post-TEST scale bounds and no free scale metadata.
pub fn quantize(source: &[f64; LANES], mode: u8) -> Result<Choice, Error> {
    wire_bytes(mode)?;
    if source.iter().any(|x| !x.is_finite()) {
        return Err(Error::NonFinite);
    }
    let max_code = ((1u16 << mode) - 1) as f64;
    let mut result = Choice {
        mode,
        exponents: [-24; SUBGROUPS],
        codes: [0; LANES],
        sse: 0.0,
    };
    for group in 0..SUBGROUPS {
        let mut best_sse = f64::INFINITY;
        let mut best_exponent = -24;
        let mut best_codes = [0i8; GROUP_WIDTH];
        for exponent in -24i8..=27 {
            let scale = 2.0f64.powi(i32::from(exponent));
            let mut codes = [0i8; GROUP_WIDTH];
            let mut sse = 0.0;
            for (index, code) in codes.iter_mut().enumerate() {
                let value = source[group * GROUP_WIDTH + index];
                let q = (value / scale).round().clamp(-max_code, max_code);
                *code = q as i8;
                let error = value - f64::from(*code) * scale;
                sse += error * error;
            }
            if sse < best_sse {
                best_sse = sse;
                best_exponent = exponent;
                best_codes = codes;
            }
        }
        if !best_sse.is_finite() {
            return Err(Error::NonFinite);
        }
        result.exponents[group] = best_exponent;
        result.codes[group * GROUP_WIDTH..(group + 1) * GROUP_WIDTH].copy_from_slice(&best_codes);
        result.sse += best_sse;
    }
    if !result.sse.is_finite() {
        return Err(Error::NonFinite);
    }
    Ok(result)
}

/// Exact multiple-choice DP, accounting for every local scale byte.
/// No assumption that local16 must be better than A1 at matched bytes.
pub fn allocate(source: &[[f64; LANES]], budget: usize) -> Result<Allocation, Error> {
    let count = source.len();
    if count == 0 {
        return Err(Error::Empty);
    }
    if count > LIMIT {
        return Err(Error::TooManyBlocks);
    }
    let base = count.checked_mul(MIN_BYTES).ok_or(Error::Overflow)?;
    if budget < base {
        return Err(Error::Budget);
    }
    let extra = ((budget - base) / STEP_BYTES).min(count * 2);
    let width = extra + 1;
    let candidates: Vec<[Choice; 3]> = source
        .iter()
        .map(|b| Ok([quantize(b, 1)?, quantize(b, 2)?, quantize(b, 3)?]))
        .collect::<Result<_, Error>>()?;
    let mut costs = vec![f64::INFINITY; (count + 1) * width];
    let mut back = vec![0u8; (count + 1) * width];
    costs[0] = 0.0;
    for i in 0..count {
        for spent in 0..=extra {
            let previous = costs[i * width + spent];
            if !previous.is_finite() {
                continue;
            }
            for mode in 1..=3usize {
                let next = spent + mode - 1;
                if next > extra {
                    continue;
                }
                let candidate = previous + candidates[i][mode - 1].sse;
                let index = (i + 1) * width + next;
                if candidate < costs[index] {
                    costs[index] = candidate;
                    back[index] = mode as u8;
                }
            }
        }
    }
    let mut best = f64::INFINITY;
    let mut spent = 0;
    for i in 0..=extra {
        let sse = costs[count * width + i];
        if sse < best {
            best = sse;
            spent = i;
        }
    }
    if !best.is_finite() {
        return Err(Error::NoCandidate);
    }
    let used = spent;
    let mut choices = Vec::with_capacity(count);
    for index in (1..=count).rev() {
        let mode = back[index * width + spent];
        if !(1..=3).contains(&mode) {
            return Err(Error::NoCandidate);
        }
        choices.push(candidates[index - 1][usize::from(mode - 1)].clone());
        spent -= usize::from(mode - 1);
    }
    choices.reverse();
    let wires = choices.iter().map(pack).collect::<Result<Vec<_>, _>>()?;
    let total_bytes = base + used * STEP_BYTES;
    assert_eq!(wires.iter().map(Vec::len).sum::<usize>(), total_bytes);
    assert!(total_bytes <= budget);
    Ok(Allocation {
        choices,
        wires,
        total_bytes,
        sse: best,
    })
}

fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed >> 12;
    *seed ^= *seed << 25;
    *seed ^= *seed >> 27;
    (*seed).wrapping_mul(2685821657736338717)
}

fn evaluate(split: &str, weights: &[[f32; 256]]) -> Result<(), Error> {
    let source: Vec<[f64; 64]> = weights
        .iter()
        .flat_map(|row| (0..4).map(move |g| std::array::from_fn(|i| f64::from(row[g * 64 + i]))))
        .collect();
    let allocation = allocate(&source, weights.len() * ROW_BYTES)?;
    assert_eq!(allocation.wires.len(), weights.len() * 4);
    let mut mse = 0.0;
    let mut dot_mse = 0.0;
    let mut n_dots = 0usize;
    let mut rng = MASK_SEED
        ^ if split == "TEST" {
            0x9e37_79b9_7f4a_7c15
        } else {
            0
        };
    for (row, original) in weights.iter().enumerate() {
        let slice = &allocation.wires[row * 4..row * 4 + 4];
        let mut original_f64 = [0.0f64; 256];
        for (i, &value) in original.iter().enumerate() {
            original_f64[i] = f64::from(value);
        }
        for (group, wire) in slice.iter().enumerate() {
            let (codes, exponents) = unpack(wire)?;
            for (i, &q) in codes.iter().enumerate() {
                let value = f64::from(q) * 2.0f64.powi(i32::from(exponents[i / GROUP_WIDTH]));
                let delta = value - original_f64[group * 64 + i];
                mse += delta * delta;
            }
        }
        for _ in 0..16 {
            let masks: [u64; 4] = std::array::from_fn(|_| random(&mut rng));
            let truth: f64 = original_f64
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    if (masks[i / 64] >> (i % 64)) & 1 != 0 {
                        -v
                    } else {
                        v
                    }
                })
                .sum();
            let result = dot_256(slice, &masks)?;
            let d = result - truth;
            dot_mse += d * d;
            n_dots += 1;
        }
    }
    let coefficients = weights.len() * 256;
    let counts =
        [1u8, 2, 3].map(|mode| allocation.choices.iter().filter(|c| c.mode == mode).count());
    assert!((mse - allocation.sse).abs() <= 1e-7 * mse.max(1.0));
    println!(
        "{split},bpb3_a4_local16,{},{:.6},{:.12},{:.12},{},{},{},{},{}",
        allocation.total_bytes,
        allocation.total_bytes as f64 * 8.0 / coefficients as f64,
        (mse / coefficients as f64).sqrt(),
        (dot_mse / n_dots as f64).sqrt(),
        coefficients,
        n_dots,
        counts[0],
        counts[1],
        counts[2]
    );
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 || args[1] != "--external-f32le" {
        eprintln!("usage: bpb3-a4 --external-f32le 48x256-f32le-source");
        std::process::exit(2);
    }
    let bytes = fs::read(Path::new(&args[2])).expect("STOP_A4_SOURCE_UNREADABLE");
    assert_eq!(bytes.len(), 48 * 256 * 4, "STOP_A4_SOURCE_LENGTH");
    let weights: Vec<[f32; 256]> = bytes
        .chunks_exact(256 * 4)
        .map(|chunk| {
            std::array::from_fn(|i| f32::from_le_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap()))
        })
        .collect();
    assert!(weights.iter().all(|row| row.iter().all(|v| v.is_finite())));
    println!(
        "protocol=bpb3-a4-local16-budget108-v1,claim=RECONSTRUCTION_ONLY,source=EXTERNALLY_PINNED"
    );
    println!("split,codec,payload_bytes,bits_per_weight,weight_rmse,dot_rmse,coefficients,dots,mode1_blocks,mode2_blocks,mode3_blocks");
    evaluate("DEV", &weights[..24]).expect("A4 DEV");
    evaluate("TEST", &weights[24..]).expect("A4 TEST");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arbitrary_codes(mode: u8, seed: u64) -> [i8; 64] {
        let mut rng = seed;
        let maximum = (1i8 << mode) - 1;
        std::array::from_fn(|i| {
            if i % 11 == 0 {
                0
            } else {
                (random(&mut rng) % (u64::from(maximum as u8) * 2 + 1)) as i8 - maximum
            }
        })
    }

    #[test]
    fn wire_counts_reject_invalid_mode_and_scale() {
        assert_eq!(wire_bytes(1), Ok(21));
        assert_eq!(wire_bytes(2), Ok(29));
        assert_eq!(wire_bytes(3), Ok(37));
        assert_eq!(wire_bytes(0), Err(Error::InvalidMode));
        assert_eq!(wire_bytes(4), Err(Error::InvalidMode));
        let choice = Choice {
            mode: 2,
            codes: arbitrary_codes(2, 1),
            exponents: [-24, -1, 0, 27],
            sse: 0.0,
        };
        let bytes = pack(&choice).unwrap();
        assert_eq!(bytes.len(), 29);
        assert_eq!(unpack(&bytes), Ok((choice.codes, choice.exponents)));
        assert_eq!(unpack(&bytes[..28]), Err(Error::WrongLength));
        let mut invalid = bytes.clone();
        invalid[2] = 28u8;
        assert_eq!(unpack(&invalid), Err(Error::InvalidExponent));
        let mut negative_zero = pack(&Choice {
            codes: [0; 64],
            ..choice.clone()
        })
        .unwrap();
        negative_zero[5] = 1;
        assert_eq!(unpack(&negative_zero), Err(Error::NegativeZero));
    }

    #[test]
    fn independent_signed_popcount_oracle_all_modes_and_subgroups() {
        let mut rng = 0x49da_278a_b21d_98efu64;
        for mode in 1..=3u8 {
            for case in 0..32 {
                let codes = arbitrary_codes(mode, random(&mut rng));
                let exponents = [-24, -5, 7, 27];
                let wire = pack(&Choice {
                    mode,
                    codes,
                    exponents,
                    sse: 0.0,
                })
                .unwrap();
                for mask in [0, u64::MAX, random(&mut rng), random(&mut rng)] {
                    let values = dot_subgroups(&wire, mask).unwrap();
                    for group in 0..SUBGROUPS {
                        let dense: i32 = codes[group * 16..(group + 1) * 16]
                            .iter()
                            .enumerate()
                            .map(|(i, &code)| {
                                if (mask >> (group * 16 + i)) & 1 != 0 {
                                    -i32::from(code)
                                } else {
                                    i32::from(code)
                                }
                            })
                            .sum();
                        assert_eq!(values[group], (dense, exponents[group]), "case={case}");
                    }
                }
            }
        }
    }

    #[test]
    fn allocation_matches_bruteforce_small_inputs() {
        let source: Vec<[f64; 64]> = (0..3)
            .map(|block| {
                std::array::from_fn(|i| {
                    if i % 21 == 0 {
                        (block + 1) as f64 * 7.0
                    } else {
                        (i as f64 - 32.0) * (block as f64 + 1.0) / 256.0
                    }
                })
            })
            .collect();
        let choices: Vec<_> = source
            .iter()
            .map(|s| {
                [
                    quantize(s, 1).unwrap(),
                    quantize(s, 2).unwrap(),
                    quantize(s, 3).unwrap(),
                ]
            })
            .collect();
        let budget = 3 * 29;
        let allocated = allocate(&source, budget).unwrap();
        let mut brute = f64::INFINITY;
        for a in 1..=3usize {
            for b in 1..=3usize {
                for c in 1..=3usize {
                    let price = wire_bytes(a as u8).unwrap()
                        + wire_bytes(b as u8).unwrap()
                        + wire_bytes(c as u8).unwrap();
                    if price <= budget {
                        let sse =
                            choices[0][a - 1].sse + choices[1][b - 1].sse + choices[2][c - 1].sse;
                        brute = brute.min(sse);
                    }
                }
            }
        }
        assert_eq!(
            allocated.total_bytes,
            allocated.wires.iter().map(Vec::len).sum()
        );
        assert!(allocated.total_bytes <= budget);
        assert!((allocated.sse - brute).abs() <= 1e-12 * brute.max(1.0));
    }

    #[test]
    fn exact_dyadic_multi_block_oracle_and_budget_fail_closed() {
        let sources: Vec<_> = (0..4)
            .map(|i| {
                std::array::from_fn(|j| {
                    if j % 16 == 0 {
                        i as f64 + 2.0
                    } else {
                        j as f64 / 8192.0
                    }
                })
            })
            .collect();
        assert!(matches!(allocate(&sources, 83), Err(Error::Budget)));
        let selected = allocate(&sources, ROW_BYTES).unwrap();
        assert!(selected.total_bytes <= ROW_BYTES);
        let masks = [0, !0, 0x55aa_55aa_55aa_55aa, 0x1234_5678_9abc_def0];
        let exact = dot_exact(&selected.wires, &masks).unwrap();
        let dense: i128 = selected
            .wires
            .iter()
            .zip(masks)
            .map(|(wire, mask)| {
                let (codes, exp) = unpack(wire).unwrap();
                (0..64)
                    .map(|i| {
                        let signed = if (mask >> i) & 1 != 0 {
                            -i128::from(codes[i])
                        } else {
                            i128::from(codes[i])
                        };
                        signed << (i32::from(exp[i / 16]) + 24)
                    })
                    .sum::<i128>()
            })
            .sum();
        assert_eq!(exact, dense);
        assert_eq!(dot_exact(&selected.wires, &[]), Err(Error::WrongLength));
    }

    #[test]
    fn canonical_quantize_finite_source_and_synthetic_local_scale_case() {
        let mut source = [0.0f64; 64];
        for group in 0..4 {
            for i in 0..16 {
                source[group * 16 + i] =
                    ((i as f64 - 7.5) / 8.0) * 2.0f64.powi((group as i32) * 5 - 12);
            }
        }
        let mut bad = source;
        bad[0] = f64::NAN;
        assert_eq!(quantize(&bad, 2).err(), Some(Error::NonFinite));
        for mode in 1..=3 {
            let choice = quantize(&source, mode).unwrap();
            let wire = pack(&choice).unwrap();
            let (codes, exponents) = unpack(&wire).unwrap();
            assert_eq!(codes, choice.codes);
            assert_eq!(exponents, choice.exponents);
            let mse: f64 = source
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let d = v - f64::from(codes[i]) * 2.0f64.powi(i32::from(exponents[i / 16]));
                    d * d
                })
                .sum();
            assert!((mse - choice.sse).abs() <= 1e-9 * mse.max(1.0));
        }
    }
}
