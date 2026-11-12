//! BPB-3-A5: per-16 coefficient signed Boolean planes AND independent
//! local dyadic scales, strictly budgeted. Experimental, NOT Posit Standard.
#![forbid(unsafe_code)]
#[path = "bpb3_a4_local16.rs"]
#[allow(dead_code)]
mod a4;

use std::fs;
use std::path::Path;

pub const LANES: usize = 64;
pub const GROUPS: usize = 4;
pub const GROUP_WIDTH: usize = 16;
pub const ROW_BYTES: usize = 108;
pub const MAX_BLOCKS: usize = 256;
pub const MASK_SEED: u64 = 0xa3b0_114d_5c15_2026;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    TooManyBlocks,
    WrongLength,
    InvalidMode,
    InvalidExponent,
    InvalidCode,
    NegativeZero,
    NonFinite,
    Budget,
    NoCandidate,
    Overflow,
}

#[derive(Clone, Copy, Debug)]
pub struct GroupChoice {
    pub mode: u8,
    pub exponent: i8,
    pub codes: [i8; GROUP_WIDTH],
    pub sse: f64,
}

#[derive(Clone, Debug)]
pub struct Allocation {
    pub groups: Vec<GroupChoice>,
    pub wires: Vec<Vec<u8>>,
    pub total_bytes: usize,
    pub sse: f64,
}

pub fn wire_bytes(modes: &[u8; GROUPS]) -> Result<usize, Error> {
    if modes.iter().any(|&m| !(1..=3).contains(&m)) {
        return Err(Error::InvalidMode);
    }
    Ok(13 + 2 * modes.iter().map(|&m| usize::from(m)).sum::<usize>())
}

/// Header: 2-bit mode-minus-one per 16-lane group, 4 i8 exponents,
/// sign u64 LE, then 2 bytes for each active local magnitude plane.
/// The mode value 3 in any packed two-bit slot is forbidden.
pub fn pack(groups: &[GroupChoice; GROUPS]) -> Result<Vec<u8>, Error> {
    let modes = groups.map(|g| g.mode);
    let size = wire_bytes(&modes)?;
    let mut header = 0u8;
    let mut sign = 0u64;
    for (group, choice) in groups.iter().enumerate() {
        if !(-24..=27).contains(&choice.exponent) {
            return Err(Error::InvalidExponent);
        }
        header |= (choice.mode - 1) << (2 * group);
        for (local, &code) in choice.codes.iter().enumerate() {
            if i16::from(code).abs() > (1i16 << choice.mode) - 1 {
                return Err(Error::InvalidCode);
            }
            if code < 0 {
                sign |= 1u64 << (group * GROUP_WIDTH + local);
            }
        }
    }
    let mut wire = Vec::with_capacity(size);
    wire.push(header);
    wire.extend(groups.map(|g| g.exponent as u8));
    wire.extend_from_slice(&sign.to_le_bytes());
    for choice in groups {
        for bitplane in 0..usize::from(choice.mode) {
            let mut plane = 0u16;
            for (index, &code) in choice.codes.iter().enumerate() {
                if code.unsigned_abs() & (1 << bitplane) != 0 {
                    plane |= 1 << index;
                }
            }
            wire.extend_from_slice(&plane.to_le_bytes());
        }
    }
    assert_eq!(wire.len(), size);
    Ok(wire)
}

fn parse(wire: &[u8]) -> Result<([u8; GROUPS], [i8; GROUPS], u64, [[u16; 3]; GROUPS]), Error> {
    if wire.len() < 13 {
        return Err(Error::WrongLength);
    }
    let modes = std::array::from_fn(|g| ((wire[0] >> (2 * g)) & 3) + 1);
    if wire.len() != wire_bytes(&modes)? {
        return Err(Error::WrongLength);
    }
    let exponents = std::array::from_fn(|g| wire[g + 1] as i8);
    if exponents.iter().any(|&k| !(-24..=27).contains(&k)) {
        return Err(Error::InvalidExponent);
    }
    let sign = u64::from_le_bytes(wire[5..13].try_into().unwrap());
    let mut planes = [[0u16; 3]; GROUPS];
    let mut cursor = 13;
    for (group, magnitude) in planes.iter_mut().enumerate() {
        let mut occupied = 0u16;
        for plane in &mut magnitude[..usize::from(modes[group])] {
            *plane = u16::from_le_bytes(wire[cursor..cursor + 2].try_into().unwrap());
            occupied |= *plane;
            cursor += 2;
        }
        let group_sign = ((sign >> (group * GROUP_WIDTH)) & 0xffff) as u16;
        if group_sign & !occupied != 0 {
            return Err(Error::NegativeZero);
        }
    }
    assert_eq!(cursor, wire.len());
    Ok((modes, exponents, sign, planes))
}

pub fn unpack(wire: &[u8]) -> Result<([i8; LANES], [i8; GROUPS]), Error> {
    let (modes, exponents, sign, planes) = parse(wire)?;
    let codes = std::array::from_fn(|index| {
        let group = index / GROUP_WIDTH;
        let local = index % GROUP_WIDTH;
        let magnitude: i8 = planes[group][..usize::from(modes[group])]
            .iter()
            .enumerate()
            .map(|(p, plane)| (((plane >> local) & 1) << p) as i8)
            .sum();
        if (sign >> index) & 1 != 0 {
            -magnitude
        } else {
            magnitude
        }
    });
    Ok((codes, exponents))
}

/// Exact 16-lane signed bipolar dot (set activation bit means -1).
pub fn dot_subgroups(wire: &[u8], masks: u64) -> Result<[(i32, i8); GROUPS], Error> {
    let (modes, exponents, sign, planes) = parse(wire)?;
    Ok(std::array::from_fn(|group| {
        let input_sign = ((masks >> (group * GROUP_WIDTH)) & 0xffff) as u16;
        let weight_sign = ((sign >> (group * GROUP_WIDTH)) & 0xffff) as u16;
        let equal_sign = !(weight_sign ^ input_sign);
        let mut integer = 0i32;
        for (p, &plane) in planes[group][..usize::from(modes[group])]
            .iter()
            .enumerate()
        {
            integer += ((2 * (equal_sign & plane).count_ones() as i32) - plane.count_ones() as i32)
                * (1 << p);
        }
        (integer, exponents[group])
    }))
}

pub fn dot_exact(wires: &[Vec<u8>], masks: &[u64]) -> Result<i128, Error> {
    if wires.len() != masks.len() {
        return Err(Error::WrongLength);
    }
    let mut sum = 0i128;
    for (wire, &mask) in wires.iter().zip(masks) {
        for (integer, k) in dot_subgroups(wire, mask)? {
            let shift = (i32::from(k) + 24) as u32;
            let term = i128::from(integer)
                .checked_mul(1i128.checked_shl(shift).ok_or(Error::Overflow)?)
                .ok_or(Error::Overflow)?;
            sum = sum.checked_add(term).ok_or(Error::Overflow)?;
        }
    }
    Ok(sum)
}

pub fn dot_256(wires: &[Vec<u8>], masks: &[u64; 4]) -> Result<f64, Error> {
    if wires.len() != 4 {
        return Err(Error::WrongLength);
    }
    let mut sum = 0.0;
    for (wire, &mask) in wires.iter().zip(masks) {
        for (integer, exponent) in dot_subgroups(wire, mask)? {
            sum += f64::from(integer) * 2.0f64.powi(i32::from(exponent));
        }
    }
    Ok(sum)
}

/// Use the independently validated A4 exhaustive per-subgroup 52-scale
/// search for each mode, splitting results into precisely 16-lane oracles.
fn alternatives(source: &[f64; LANES]) -> Result<[[GroupChoice; 3]; GROUPS], Error> {
    if source.iter().any(|v| !v.is_finite()) {
        return Err(Error::NonFinite);
    }
    let original = [
        a4::quantize(source, 1).map_err(|_| Error::NonFinite)?,
        a4::quantize(source, 2).map_err(|_| Error::NonFinite)?,
        a4::quantize(source, 3).map_err(|_| Error::NonFinite)?,
    ];
    Ok(std::array::from_fn(|group| {
        std::array::from_fn(|mode_index| {
            let option = &original[mode_index];
            let exponent = option.exponents[group];
            let scale = 2.0f64.powi(i32::from(exponent));
            let codes = std::array::from_fn(|i| option.codes[group * GROUP_WIDTH + i]);
            let sse = (0..GROUP_WIDTH)
                .map(|i| {
                    let value = source[group * GROUP_WIDTH + i];
                    let error = value - f64::from(codes[i]) * scale;
                    error * error
                })
                .sum();
            GroupChoice {
                mode: (mode_index + 1) as u8,
                exponent,
                codes,
                sse,
            }
        })
    }))
}

/// Exact bounded DP with TWO-BYTE precision increments per 16-lane group.
/// The A4 uniform mode is an admitted subset at the exact same bytes.
pub fn allocate(source: &[[f64; LANES]], budget: usize) -> Result<Allocation, Error> {
    if source.is_empty() {
        return Err(Error::Empty);
    }
    if source.len() > MAX_BLOCKS {
        return Err(Error::TooManyBlocks);
    }
    let count = source.len() * GROUPS;
    let base = source.len() * 13 + count * 2;
    if budget < base {
        return Err(Error::Budget);
    }
    let extras = ((budget - base) / 2).min(count * 2);
    let width = extras + 1;
    let choices: Vec<[GroupChoice; 3]> = source
        .iter()
        .map(alternatives)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flat_map(|block| block.into_iter())
        .collect();
    assert_eq!(choices.len(), count);
    let mut dp = vec![f64::INFINITY; (count + 1) * width];
    let mut back = vec![0u8; (count + 1) * width];
    dp[0] = 0.0;
    for i in 0..count {
        for spent in 0..=extras {
            let previous = dp[i * width + spent];
            if !previous.is_finite() {
                continue;
            }
            for mode in 1..=3usize {
                let next = spent + mode - 1;
                if next > extras {
                    continue;
                }
                let candidate = previous + choices[i][mode - 1].sse;
                let position = (i + 1) * width + next;
                if candidate < dp[position] {
                    dp[position] = candidate;
                    back[position] = mode as u8;
                }
            }
        }
    }
    let mut best = f64::INFINITY;
    let mut spent = 0;
    for extra in 0..=extras {
        if dp[count * width + extra] < best {
            best = dp[count * width + extra];
            spent = extra;
        }
    }
    if !best.is_finite() {
        return Err(Error::NoCandidate);
    }
    let used = spent;
    let mut groups = Vec::with_capacity(count);
    for index in (1..=count).rev() {
        let mode = back[index * width + spent];
        if !(1..=3).contains(&mode) {
            return Err(Error::NoCandidate);
        }
        groups.push(choices[index - 1][usize::from(mode - 1)]);
        spent -= usize::from(mode - 1);
    }
    groups.reverse();
    let wires: Vec<Vec<u8>> = groups
        .chunks_exact(GROUPS)
        .map(|chunk| {
            let block: &[GroupChoice; GROUPS] = chunk.try_into().unwrap();
            pack(block)
        })
        .collect::<Result<_, _>>()?;
    let total_bytes = base + 2 * used;
    assert_eq!(wires.iter().map(Vec::len).sum::<usize>(), total_bytes);
    assert!(total_bytes <= budget);
    Ok(Allocation {
        groups,
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
    let source: Vec<[f64; LANES]> = weights
        .iter()
        .flat_map(|row| (0..4).map(move |g| std::array::from_fn(|i| f64::from(row[g * 64 + i]))))
        .collect();
    let chosen = allocate(&source, weights.len() * ROW_BYTES)?;
    let mut source_sse = 0.0;
    let mut signed_dot_sse = 0.0;
    let mut dots = 0usize;
    let mut rng = MASK_SEED
        ^ if split == "TEST" {
            0x9e37_79b9_7f4a_7c15
        } else {
            0
        };
    for (row, original) in weights.iter().enumerate() {
        let mut truth = [0.0f64; 256];
        for (i, &value) in original.iter().enumerate() {
            truth[i] = f64::from(value);
        }
        let wires = &chosen.wires[row * 4..row * 4 + 4];
        for (block, wire) in wires.iter().enumerate() {
            let (codes, exponents) = unpack(wire)?;
            for (index, &code) in codes.iter().enumerate() {
                let reconstructed =
                    f64::from(code) * 2.0f64.powi(i32::from(exponents[index / GROUP_WIDTH]));
                let delta = reconstructed - truth[block * LANES + index];
                source_sse += delta * delta;
            }
        }
        for _ in 0..16 {
            let masks: [u64; 4] = std::array::from_fn(|_| random(&mut rng));
            let actual: f64 = truth
                .iter()
                .enumerate()
                .map(|(i, &value)| {
                    if (masks[i / 64] >> (i % 64)) & 1 != 0 {
                        -value
                    } else {
                        value
                    }
                })
                .sum();
            let observed = dot_256(wires, &masks)?;
            let error = actual - observed;
            signed_dot_sse += error * error;
            dots += 1;
        }
    }
    assert!((source_sse - chosen.sse).abs() <= 1e-7 * source_sse.max(1.0));
    let coefficients = weights.len() * 256;
    let mode_counts =
        [1u8, 2, 3].map(|mode| chosen.groups.iter().filter(|c| c.mode == mode).count());
    println!(
        "{split},bpb3_a5_mixed16,{},{:.6},{:.12},{:.12},{},{},{},{},{}",
        chosen.total_bytes,
        chosen.total_bytes as f64 * 8.0 / coefficients as f64,
        (source_sse / coefficients as f64).sqrt(),
        (signed_dot_sse / dots as f64).sqrt(),
        coefficients,
        dots,
        mode_counts[0],
        mode_counts[1],
        mode_counts[2]
    );
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 || args[1] != "--external-f32le" {
        eprintln!("usage: bpb3-a5 --external-f32le verified-source-48x256");
        std::process::exit(2);
    }
    let bytes = fs::read(Path::new(&args[2])).expect("STOP_A5_INPUT_READ");
    assert_eq!(bytes.len(), 48 * 256 * 4, "STOP_A5_INPUT_LENGTH");
    let rows: Vec<[f32; 256]> = bytes
        .chunks_exact(256 * 4)
        .map(|raw| {
            std::array::from_fn(|i| f32::from_le_bytes(raw[4 * i..4 * i + 4].try_into().unwrap()))
        })
        .collect();
    assert!(rows.iter().all(|r| r.iter().all(|x| x.is_finite())));
    println!(
        "protocol=bpb3-a5-mixed16-budget108-v1,source=EXTERNALLY_PINNED,claim=RECONSTRUCTION_ONLY"
    );
    println!("split,codec,payload_bytes,bits_per_weight,weight_rmse,dot_rmse,coefficients,dots,mode1_blocks,mode2_blocks,mode3_blocks");
    evaluate("DEV", &rows[..24]).expect("A5 DEV");
    evaluate("TEST", &rows[24..]).expect("A5 TEST");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(seed: u64) -> [f64; 64] {
        let mut state = seed;
        std::array::from_fn(|i| {
            let raw = ((random(&mut state) >> 40) as f64 / ((1u64 << 24) as f64)) * 2.0 - 1.0;
            let scale = 2.0f64.powi((i % 4) as i32 * 4 - 10);
            raw * scale
        })
    }

    fn dummy(mode: u8, exponent: i8) -> GroupChoice {
        let max = (1i8 << mode) - 1;
        GroupChoice {
            mode,
            exponent,
            codes: std::array::from_fn(|i| {
                if i % 13 == 0 {
                    0
                } else {
                    i as i8 % (2 * max + 1) - max
                }
            }),
            sse: 0.0,
        }
    }

    #[test]
    fn variable_wire_cost_and_canonical_bitfields() {
        let groups = [dummy(1, -24), dummy(2, 0), dummy(3, 27), dummy(1, -4)];
        let wire = pack(&groups).unwrap();
        assert_eq!(wire.len(), 13 + 2 * (1 + 2 + 3 + 1));
        assert_eq!(unpack(&wire).unwrap().1, [-24, 0, 27, -4]);
        let expected_codes: [i8; 64] = std::array::from_fn(|i| groups[i / 16].codes[i % 16]);
        assert_eq!(unpack(&wire).unwrap().0, expected_codes);
        let mut reserved = wire.clone();
        reserved[0] |= 0b11;
        assert_eq!(unpack(&reserved), Err(Error::InvalidMode));
        let mut bad_exponent = wire.clone();
        bad_exponent[1] = 28;
        assert_eq!(unpack(&bad_exponent), Err(Error::InvalidExponent));
        let mut bad_negative_zero = pack(&[dummy(1, 0); 4]).unwrap();
        bad_negative_zero[5] |= 1;
        // dummy mode1 always has a zero at index0
        assert_eq!(unpack(&bad_negative_zero), Err(Error::NegativeZero));
        assert_eq!(unpack(&wire[..wire.len() - 1]), Err(Error::WrongLength));
    }

    #[test]
    fn signed_popcount_local16_oracle_all_mixed_modes() {
        let mut rng = 0x4b71_33ea_0192_0015u64;
        for case in 0..100 {
            let modes = [
                1 + (case % 3) as u8,
                1 + ((case + 1) % 3) as u8,
                1 + ((case + 2) % 3) as u8,
                1 + ((case + 1) % 3) as u8,
            ];
            let groups = std::array::from_fn(|g| dummy(modes[g], [27, -24, 0, 5][g]));
            let wire = pack(&groups).unwrap();
            for masks in [0, u64::MAX, random(&mut rng), random(&mut rng)] {
                let observed = dot_subgroups(&wire, masks).unwrap();
                for g in 0..4 {
                    let dense: i32 = groups[g]
                        .codes
                        .iter()
                        .enumerate()
                        .map(|(i, &v)| {
                            if (masks >> (g * 16 + i)) & 1 != 0 {
                                -i32::from(v)
                            } else {
                                i32::from(v)
                            }
                        })
                        .sum();
                    assert_eq!(observed[g], (dense, groups[g].exponent));
                }
            }
        }
    }

    #[test]
    fn exact_dp_dominates_every_a4_mode_at_same_wire_budget() {
        for seed in 0..8 {
            let blocks: Vec<_> = (0..4).map(|j| source((seed * 4 + j + 1) as u64)).collect();
            let a4_result = a4::allocate(&blocks, ROW_BYTES).unwrap();
            let a5_result = allocate(&blocks, ROW_BYTES).unwrap();
            assert!(a5_result.total_bytes <= ROW_BYTES);
            assert!(
                a5_result.sse <= a4_result.sse + 1e-10 * a4_result.sse.max(1.0),
                "seed={seed}: a5={} a4={}",
                a5_result.sse,
                a4_result.sse
            );
        }
    }

    #[test]
    fn exact_dp_matches_exhaustive_enumeration_for_one_block() {
        let block = source(21);
        let alternatives = alternatives(&block).unwrap();
        let candidate = allocate(&[block], 27).unwrap();
        let mut best = f64::INFINITY;
        // 3^4 allocations to four independent 16-lane groups.
        for a in 0..3 {
            for b in 0..3 {
                for c in 0..3 {
                    for d in 0..3 {
                        let modes = [a, b, c, d];
                        let used = 13 + 2 * modes.iter().map(|x| x + 1).sum::<usize>();
                        if used <= 27 {
                            let sse: f64 = (0..4).map(|g| alternatives[g][modes[g]].sse).sum();
                            best = best.min(sse);
                        }
                    }
                }
            }
        }
        assert_eq!(candidate.total_bytes, candidate.wires[0].len());
        assert!((candidate.sse - best).abs() <= 1e-10 * best.max(1.0));
    }

    #[test]
    fn exact_signed_i128_dot_and_malformed_inputs() {
        let blocks: Vec<_> = (0..4).map(|i| source(i as u64 + 100)).collect();
        assert!(matches!(allocate(&blocks, 80), Err(Error::Budget)));
        let selected = allocate(&blocks, ROW_BYTES).unwrap();
        let masks = [0, u64::MAX, 0x0123_4567_89ab_cdef, 0xa55a_5aa5_55aa_ff00];
        let actual = dot_exact(&selected.wires, &masks).unwrap();
        let expected: i128 = selected
            .wires
            .iter()
            .zip(masks)
            .map(|(wire, mask)| {
                let (codes, exp) = unpack(wire).unwrap();
                (0..64)
                    .map(|i| {
                        let q = i128::from(codes[i]);
                        let signed = if (mask >> i) & 1 != 0 { -q } else { q };
                        signed << (i32::from(exp[i / 16]) + 24)
                    })
                    .sum::<i128>()
            })
            .sum();
        assert_eq!(actual, expected);
        assert_eq!(dot_exact(&selected.wires, &[]), Err(Error::WrongLength));
        let mut invalid = source(11);
        invalid[17] = f64::INFINITY;
        assert!(matches!(allocate(&[invalid], 27), Err(Error::NonFinite)));
    }

    #[test]
    fn adversarial_uniform_modes_preserve_a4_reconstruction_values() {
        let block = source(42);
        for mode in 1..=3 {
            let a4_choice = a4::quantize(&block, mode).unwrap();
            let groups = alternatives(&block)
                .unwrap()
                .map(|opts| opts[usize::from(mode - 1)]);
            let a5_wire = pack(&groups).unwrap();
            let (a5_codes, exp) = unpack(&a5_wire).unwrap();
            assert_eq!(a5_codes, a4_choice.codes);
            assert_eq!(exp, a4_choice.exponents);
            assert_eq!(a5_wire.len(), a4::wire_bytes(mode).unwrap());
        }
    }
}
