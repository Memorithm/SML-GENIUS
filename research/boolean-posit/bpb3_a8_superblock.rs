//! BPB-3-A8 research codec: one dyadic base for 256 original weights,
//! 16 two-bit local exponent offsets, and independent per-16 bitplanes.
//! This is not a Posit Standard numeric representation.
#![forbid(unsafe_code)]

use std::fs;
use std::path::Path;

const WEIGHTS: usize = 256;
const GROUPS: usize = 16;
const WIDTH: usize = 16;
const SCALES: usize = 52;
const MIN_BYTES: usize = 73;
const MAX_ROWS: usize = 64;
const BUDGET_PER_ROW: usize = 108;
const MASK_SEED: u64 = 0xa3b0_114d_5c15_2026;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Empty,
    TooManyRows,
    WrongLength,
    InvalidMode,
    InvalidBase,
    InvalidLocalExponent,
    InvalidCode,
    NegativeZero,
    NonFinite,
    Budget,
    NoCandidate,
    Overflow,
}

#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub mode: u8,
    pub exponent: i8,
    pub codes: [i8; WIDTH],
    pub sse: f64,
}

#[derive(Clone, Debug)]
pub struct Superblock {
    pub base: i8,
    pub groups: [Group; GROUPS],
    pub sse: f64,
}

#[derive(Clone, Debug)]
pub struct Allocation {
    pub selected: Vec<Superblock>,
    pub wires: Vec<Vec<u8>>,
    pub total_bytes: usize,
    pub sse: f64,
}

#[derive(Clone, Copy)]
struct Quantized {
    codes: [i8; WIDTH],
    sse: f64,
}

pub fn wire_bytes(modes: &[u8; GROUPS]) -> Result<usize, Error> {
    if modes.iter().any(|&m| !(1..=3).contains(&m)) {
        return Err(Error::InvalidMode);
    }
    Ok(41 + 2 * modes.iter().map(|&mode| usize::from(mode)).sum::<usize>())
}

/// Canonical wire: biased 1B base exponent, 4B mode fields, 4B local
/// exponent differences, 32B sign bits, and 2B per magnitude plane.
pub fn pack(block: &Superblock) -> Result<Vec<u8>, Error> {
    if !(-24..=27).contains(&block.base) {
        return Err(Error::InvalidBase);
    }
    let modes = block.groups.map(|g| g.mode);
    let length = wire_bytes(&modes)?;
    let mut mode_bits = 0u32;
    let mut delta_bits = 0u32;
    let mut signs = [0u64; 4];
    for (group, choice) in block.groups.iter().enumerate() {
        mode_bits |= u32::from(choice.mode - 1) << (2 * group);
        if !(-24..=27).contains(&choice.exponent) {
            return Err(Error::InvalidLocalExponent);
        }
        let delta = i16::from(choice.exponent) - i16::from(block.base);
        if !(-1..=2).contains(&delta) {
            return Err(Error::InvalidLocalExponent);
        }
        delta_bits |= ((delta + 1) as u32) << (2 * group);
        let max = (1i16 << choice.mode) - 1;
        for (i, &code) in choice.codes.iter().enumerate() {
            if i16::from(code).abs() > max {
                return Err(Error::InvalidCode);
            }
            if code < 0 {
                let index = group * WIDTH + i;
                signs[index / 64] |= 1u64 << (index % 64);
            }
        }
    }
    let mut out = Vec::with_capacity(length);
    out.push((block.base + 24) as u8);
    out.extend_from_slice(&mode_bits.to_le_bytes());
    out.extend_from_slice(&delta_bits.to_le_bytes());
    for &s in &signs {
        out.extend_from_slice(&s.to_le_bytes());
    }
    for group in &block.groups {
        for p in 0..usize::from(group.mode) {
            let mut bits = 0u16;
            for (i, &code) in group.codes.iter().enumerate() {
                if (code.unsigned_abs() & (1 << p)) != 0 {
                    bits |= 1 << i;
                }
            }
            out.extend_from_slice(&bits.to_le_bytes());
        }
    }
    assert_eq!(out.len(), length);
    Ok(out)
}

type Parsed = ([u8; GROUPS], [i8; GROUPS], [u64; 4], [[u16; 3]; GROUPS]);

fn parse(bytes: &[u8]) -> Result<Parsed, Error> {
    if bytes.len() < MIN_BYTES {
        return Err(Error::WrongLength);
    }
    if bytes[0] > 51 {
        return Err(Error::InvalidBase);
    }
    let base = bytes[0] as i16 - 24;
    let mode_bits = u32::from_le_bytes(bytes[1..5].try_into().unwrap());
    let modes: [u8; GROUPS] = std::array::from_fn(|i| ((mode_bits >> (2 * i)) & 3) as u8 + 1);
    if modes.contains(&4) {
        return Err(Error::InvalidMode);
    }
    if bytes.len() != wire_bytes(&modes)? {
        return Err(Error::WrongLength);
    }
    let delta_bits = u32::from_le_bytes(bytes[5..9].try_into().unwrap());
    let absolute: [i16; GROUPS] =
        std::array::from_fn(|g| base + ((delta_bits >> (2 * g)) & 3) as i16 - 1);
    if absolute.iter().any(|&k| !(-24..=27).contains(&k)) {
        return Err(Error::InvalidLocalExponent);
    }
    let exponents = absolute.map(|k| k as i8);
    let signs = std::array::from_fn(|word| {
        let at = 9 + word * 8;
        u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
    });
    let mut planes = [[0u16; 3]; GROUPS];
    let mut cursor = 41;
    for (g, group) in planes.iter_mut().enumerate() {
        let mut occupied = 0u16;
        for plane in &mut group[..usize::from(modes[g])] {
            *plane = u16::from_le_bytes(bytes[cursor..cursor + 2].try_into().unwrap());
            occupied |= *plane;
            cursor += 2;
        }
        let sign_mask = ((signs[g / 4] >> ((g % 4) * WIDTH)) & 0xffff) as u16;
        if sign_mask & !occupied != 0 {
            return Err(Error::NegativeZero);
        }
    }
    assert_eq!(cursor, bytes.len());
    Ok((modes, exponents, signs, planes))
}

pub fn unpack(bytes: &[u8]) -> Result<([i8; WEIGHTS], [i8; GROUPS]), Error> {
    let (modes, exponents, signs, planes) = parse(bytes)?;
    let codes = std::array::from_fn(|index| {
        let g = index / WIDTH;
        let i = index % WIDTH;
        let magnitude = planes[g][..usize::from(modes[g])]
            .iter()
            .enumerate()
            .map(|(p, plane)| (((plane >> i) & 1) << p) as i8)
            .sum::<i8>();
        if (signs[index / 64] >> (index % 64)) & 1 != 0 {
            -magnitude
        } else {
            magnitude
        }
    });
    Ok((codes, exponents))
}

/// Signed Boolean dot with 16 local groups. A mask bit set means -1.
pub fn dot_groups(bytes: &[u8], mask: &[u64; 4]) -> Result<[(i32, i8); GROUPS], Error> {
    let (modes, exponents, signs, planes) = parse(bytes)?;
    Ok(std::array::from_fn(|g| {
        let position = (g % 4) * WIDTH;
        let weight_sign = ((signs[g / 4] >> position) & 0xffff) as u16;
        let input_sign = ((mask[g / 4] >> position) & 0xffff) as u16;
        let same_sign = !(weight_sign ^ input_sign);
        let mut sum = 0i32;
        for (p, &plane) in planes[g][..usize::from(modes[g])].iter().enumerate() {
            sum += (2 * (plane & same_sign).count_ones() as i32 - plane.count_ones() as i32)
                * (1 << p);
        }
        (sum, exponents[g])
    }))
}

pub fn dot_exact(bytes: &[u8], mask: &[u64; 4]) -> Result<i128, Error> {
    let mut total = 0i128;
    for (integer, exponent) in dot_groups(bytes, mask)? {
        let mul = 1i128
            .checked_shl((i32::from(exponent) + 24) as u32)
            .ok_or(Error::Overflow)?;
        total = total
            .checked_add(
                i128::from(integer)
                    .checked_mul(mul)
                    .ok_or(Error::Overflow)?,
            )
            .ok_or(Error::Overflow)?;
    }
    Ok(total)
}

pub fn dot_f64(bytes: &[u8], mask: &[u64; 4]) -> Result<f64, Error> {
    let mut total = 0.0;
    for (integer, exponent) in dot_groups(bytes, mask)? {
        total += f64::from(integer) * 2.0f64.powi(i32::from(exponent));
    }
    Ok(total)
}

/// All original-value dyadic source quantizers are enumerated once;
/// no data-dependent or post-TEST exponent range tuning.
fn quant_table(source: &[f64; WEIGHTS]) -> Result<[[[Quantized; SCALES]; 3]; GROUPS], Error> {
    if source.iter().any(|v| !v.is_finite()) {
        return Err(Error::NonFinite);
    }
    let result: [[[Quantized; SCALES]; 3]; GROUPS] = std::array::from_fn(|g| {
        std::array::from_fn(|m| {
            std::array::from_fn(|e| {
                let scale = 2.0f64.powi(e as i32 - 24);
                let max = ((1u16 << (m + 1)) - 1) as f64;
                let mut sse = 0.0;
                let codes = std::array::from_fn(|i| {
                    let v = source[g * WIDTH + i];
                    let q = (v / scale).round().clamp(-max, max) as i8;
                    let error = v - f64::from(q) * scale;
                    sse += error * error;
                    q
                });
                Quantized { codes, sse }
            })
        })
    });
    if result
        .iter()
        .flat_map(|g| g.iter().flat_map(|m| m.iter()))
        .any(|q| !q.sse.is_finite())
    {
        return Err(Error::NonFinite);
    }
    Ok(result)
}

/// Inner exact multiple-choice knapsack: one common scale base shared
/// by all 16 groups. Keep best candidate for every actual byte count,
/// not just the scalar minimum.
pub fn candidates(source: &[f64; WEIGHTS]) -> Result<Vec<Superblock>, Error> {
    let table = quant_table(source)?;
    let mut best: [Option<Superblock>; 33] = std::array::from_fn(|_| None);
    for base in -24i8..=27 {
        let choices: [[Option<Group>; 3]; GROUPS] = std::array::from_fn(|g| {
            std::array::from_fn(|mode_index| {
                let mut preferred: Option<Group> = None;
                for offset in -1i16..=2 {
                    let exponent = i16::from(base) + offset;
                    if !(-24..=27).contains(&exponent) {
                        continue;
                    }
                    let q = table[g][mode_index][(exponent + 24) as usize];
                    if preferred.as_ref().is_none_or(|old| q.sse < old.sse) {
                        preferred = Some(Group {
                            mode: (mode_index + 1) as u8,
                            exponent: exponent as i8,
                            codes: q.codes,
                            sse: q.sse,
                        });
                    }
                }
                preferred
            })
        });
        let mut dp = [[f64::INFINITY; 33]; 17];
        let mut back = [[0u8; 33]; 17];
        dp[0][0] = 0.0;
        for g in 0..GROUPS {
            for spent in 0..=32 {
                if !dp[g][spent].is_finite() {
                    continue;
                }
                for mode in 1..=3usize {
                    let next = spent + mode - 1;
                    if next > 32 {
                        continue;
                    }
                    if let Some(choice) = choices[g][mode - 1] {
                        let sse = dp[g][spent] + choice.sse;
                        if sse < dp[g + 1][next] {
                            dp[g + 1][next] = sse;
                            back[g + 1][next] = mode as u8;
                        }
                    }
                }
            }
        }
        for extra in 0..=32usize {
            let sse = dp[GROUPS][extra];
            if !sse.is_finite() || best[extra].as_ref().is_some_and(|old| old.sse <= sse) {
                continue;
            }
            let mut spent = extra;
            let mut groups: [Option<Group>; GROUPS] = [None; GROUPS];
            for i in (1..=GROUPS).rev() {
                let mode = back[i][spent];
                if !(1..=3).contains(&mode) {
                    return Err(Error::NoCandidate);
                }
                groups[i - 1] = choices[i - 1][usize::from(mode - 1)];
                spent -= usize::from(mode - 1);
            }
            if spent != 0 {
                return Err(Error::NoCandidate);
            }
            let groups = groups.map(|group| group.expect("selected group exists"));
            let block = Superblock { base, groups, sse };
            if pack(&block)?.len() != MIN_BYTES + 2 * extra {
                return Err(Error::WrongLength);
            }
            best[extra] = Some(block);
        }
    }
    Ok(best.into_iter().flatten().collect())
}

/// Outer exact global DP across independently readable 256-weight
/// superblocks, with a complete 24*108 byte budget in real packed units.
pub fn allocate(rows: &[[f64; WEIGHTS]], budget: usize) -> Result<Allocation, Error> {
    if rows.is_empty() {
        return Err(Error::Empty);
    }
    if rows.len() > MAX_ROWS {
        return Err(Error::TooManyRows);
    }
    let base = rows.len() * MIN_BYTES;
    if budget < base {
        return Err(Error::Budget);
    }
    let extras = ((budget - base) / 2).min(rows.len() * 32);
    let width = extras + 1;
    let all: Vec<Vec<Superblock>> = rows.iter().map(candidates).collect::<Result<_, _>>()?;
    let mut dp = vec![f64::INFINITY; (rows.len() + 1) * width];
    let mut back = vec![0u8; (rows.len() + 1) * width];
    dp[0] = 0.0;
    for (row, options) in all.iter().enumerate() {
        for spent in 0..=extras {
            let previous = dp[row * width + spent];
            if !previous.is_finite() {
                continue;
            }
            for (index, option) in options.iter().enumerate() {
                let cost = wire_bytes(&option.groups.map(|g| g.mode))?;
                let next = spent + (cost - MIN_BYTES) / 2;
                if next > extras {
                    continue;
                }
                let value = previous + option.sse;
                let position = (row + 1) * width + next;
                if value < dp[position] {
                    dp[position] = value;
                    back[position] = (index + 1) as u8;
                }
            }
        }
    }
    let mut best = f64::INFINITY;
    let mut spent = 0;
    for extra in 0..=extras {
        let value = dp[rows.len() * width + extra];
        if value < best {
            best = value;
            spent = extra;
        }
    }
    if !best.is_finite() {
        return Err(Error::NoCandidate);
    }
    let total_bytes = base + 2 * spent;
    let mut selected = Vec::with_capacity(rows.len());
    for i in (1..=rows.len()).rev() {
        let idx = back[i * width + spent];
        if idx == 0 {
            return Err(Error::NoCandidate);
        }
        let block = all[i - 1][usize::from(idx - 1)].clone();
        spent -= (wire_bytes(&block.groups.map(|g| g.mode))? - MIN_BYTES) / 2;
        selected.push(block);
    }
    selected.reverse();
    let wires = selected.iter().map(pack).collect::<Result<Vec<_>, _>>()?;
    if wires.iter().map(Vec::len).sum::<usize>() != total_bytes || total_bytes > budget {
        return Err(Error::Budget);
    }
    Ok(Allocation {
        selected,
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

fn evaluate(split: &str, rows: &[[f32; WEIGHTS]]) -> Result<(), Error> {
    let source: Vec<[f64; WEIGHTS]> = rows.iter().map(|row| row.map(f64::from)).collect();
    let allocated = allocate(&source, rows.len() * BUDGET_PER_ROW)?;
    let mut source_sse = 0.0;
    let mut dot_sse = 0.0;
    let mut dots = 0usize;
    let mut rng = MASK_SEED
        ^ if split == "TEST" {
            0x9e37_79b9_7f4a_7c15
        } else {
            0
        };
    for (index, original) in source.iter().enumerate() {
        let (codes, exponents) = unpack(&allocated.wires[index])?;
        for (i, &code) in codes.iter().enumerate() {
            let restored = f64::from(code) * 2.0f64.powi(i32::from(exponents[i / WIDTH]));
            let delta = restored - original[i];
            source_sse += delta * delta;
        }
        for _ in 0..16 {
            let masks: [u64; 4] = std::array::from_fn(|_| random(&mut rng));
            let reference: f64 = original
                .iter()
                .enumerate()
                .map(|(i, &x)| {
                    if (masks[i / 64] >> (i % 64)) & 1 != 0 {
                        -x
                    } else {
                        x
                    }
                })
                .sum();
            let d = dot_f64(&allocated.wires[index], &masks)? - reference;
            dot_sse += d * d;
            dots += 1;
        }
    }
    if (source_sse - allocated.sse).abs() > 1e-7 * source_sse.max(1.0) {
        return Err(Error::NoCandidate);
    }
    let modes = [1u8, 2, 3].map(|mode| {
        allocated
            .selected
            .iter()
            .map(|block| block.groups.iter().filter(|g| g.mode == mode).count())
            .sum::<usize>()
    });
    let coefficients = rows.len() * WEIGHTS;
    println!(
        "{split},bpb3_a8_superblock_delta2,{},{:.6},{:.12},{:.12},{},{},,,{},{},{}",
        allocated.total_bytes,
        allocated.total_bytes as f64 * 8.0 / coefficients as f64,
        (source_sse / coefficients as f64).sqrt(),
        (dot_sse / dots as f64).sqrt(),
        coefficients,
        dots,
        modes[0],
        modes[1],
        modes[2],
    );
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 || args[1] != "--external-f32le" {
        eprintln!("usage: bpb3-a8 --external-f32le pinned-48x256-f32le");
        std::process::exit(2);
    }
    let bytes = fs::read(Path::new(&args[2])).expect("STOP_A8_SOURCE_UNREADABLE");
    assert_eq!(bytes.len(), 48 * WEIGHTS * 4, "STOP_A8_SOURCE_BYTES");
    let rows: Vec<[f32; WEIGHTS]> = bytes
        .chunks_exact(WEIGHTS * 4)
        .map(|chunk| {
            std::array::from_fn(|i| f32::from_le_bytes(chunk[4 * i..4 * i + 4].try_into().unwrap()))
        })
        .collect();
    assert!(rows.iter().all(|row| row.iter().all(|v| v.is_finite())));
    println!("protocol=bpb3-a8-superblock256-delta2-budget108-v1,claim=COMPONENT_ONLY,source=EXTERNALLY_SHA_PINNED");
    println!("split,codec,payload_bytes,bits_per_weight,weight_rmse,dot_rmse,coefficients,dots,delta2_blocks,delta4_blocks,mode1_groups,mode2_groups,mode3_groups");
    evaluate("DEV", &rows[..24]).expect("STOP_A8_DEV");
    evaluate("TEST", &rows[24..]).expect("STOP_A8_TEST");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(seed: u64) -> [f64; WEIGHTS] {
        let mut state = seed;
        std::array::from_fn(|i| {
            let v = (random(&mut state) >> 32) as f64 / u32::MAX as f64 - 0.5;
            v * 2.0f64.powi((i / WIDTH % 4) as i32 * 3 - 9)
        })
    }

    #[test]
    fn canonical_pack_unpack_all_mode_boundaries_and_reserved_fields() {
        let source = fixture(100);
        let options = candidates(&source).unwrap();
        assert!(!options.is_empty());
        for block in options {
            let wire = pack(&block).unwrap();
            assert_eq!(
                wire.len(),
                wire_bytes(&block.groups.map(|g| g.mode)).unwrap()
            );
            let (codes, exponents) = unpack(&wire).unwrap();
            for group in 0..GROUPS {
                assert_eq!(exponents[group], block.groups[group].exponent);
                for i in 0..WIDTH {
                    assert_eq!(codes[group * WIDTH + i], block.groups[group].codes[i]);
                }
            }
            let mut bad = wire.clone();
            bad[0] = 52;
            assert_eq!(unpack(&bad), Err(Error::InvalidBase));
            bad = wire.clone();
            bad[1] |= 0b11;
            assert_eq!(unpack(&bad), Err(Error::InvalidMode));
            bad = wire.clone();
            bad[5] = 0;
            bad[0] = 0; // k=-24; offset code0 means -25 outside range
            assert_eq!(unpack(&bad), Err(Error::InvalidLocalExponent));
            bad = wire.clone();
            bad.pop();
            assert_eq!(unpack(&bad), Err(Error::WrongLength));
        }
    }

    #[test]
    fn popcount_matches_dense_integer_dot_on_independent_signed_masks() {
        let mut rng = 0xa813_7c15_cad5_2245u64;
        for trial in 0..16 {
            let candidates = candidates(&fixture(trial + 1)).unwrap();
            let wire = pack(&candidates[candidates.len() / 2]).unwrap();
            let (codes, exponents) = unpack(&wire).unwrap();
            for mask in [
                [0u64; 4],
                [u64::MAX; 4],
                std::array::from_fn(|_| random(&mut rng)),
            ] {
                let actual = dot_groups(&wire, &mask).unwrap();
                for group in 0..GROUPS {
                    let reference: i32 = (0..WIDTH)
                        .map(|i| {
                            let at = group * WIDTH + i;
                            if (mask[at / 64] >> (at % 64)) & 1 != 0 {
                                -i32::from(codes[at])
                            } else {
                                i32::from(codes[at])
                            }
                        })
                        .sum();
                    assert_eq!(actual[group], (reference, exponents[group]));
                }
            }
        }
    }

    #[test]
    fn global_dp_matches_exhaustive_two_superblock_oracle() {
        let rows = [fixture(9), fixture(13)];
        let all: Vec<_> = rows.iter().map(|row| candidates(row).unwrap()).collect();
        let budget = 190;
        let chosen = allocate(&rows, budget).unwrap();
        let mut oracle = f64::INFINITY;
        for a in &all[0] {
            for b in &all[1] {
                let bytes = wire_bytes(&a.groups.map(|g| g.mode)).unwrap()
                    + wire_bytes(&b.groups.map(|g| g.mode)).unwrap();
                if bytes <= budget {
                    oracle = oracle.min(a.sse + b.sse);
                }
            }
        }
        assert!((chosen.sse - oracle).abs() <= 1e-10 * oracle.max(1.0));
        assert_eq!(
            chosen.total_bytes,
            chosen.wires.iter().map(Vec::len).sum::<usize>()
        );
    }

    #[test]
    fn inner_dp_zero_extra_matches_independent_base_exponent_enumeration() {
        let source = fixture(27);
        let candidates = candidates(&source).unwrap();
        let all_one_plane = candidates
            .iter()
            .find(|b| wire_bytes(&b.groups.map(|g| g.mode)).unwrap() == MIN_BYTES)
            .unwrap();
        let mut optimum = f64::INFINITY;
        for base in -24i8..=27 {
            let mut total = 0.0;
            for group in 0..GROUPS {
                let mut best_group = f64::INFINITY;
                for offset in -1i16..=2 {
                    let exponent = i16::from(base) + offset;
                    if !(-24..=27).contains(&exponent) {
                        continue;
                    }
                    let scale = 2.0f64.powi(i32::from(exponent));
                    let sse: f64 = (0..WIDTH)
                        .map(|i| {
                            let x = source[group * WIDTH + i];
                            let quant = (x / scale).round().clamp(-1.0, 1.0);
                            let d = x - quant * scale;
                            d * d
                        })
                        .sum();
                    best_group = best_group.min(sse);
                }
                total += best_group;
            }
            optimum = optimum.min(total);
        }
        assert!((all_one_plane.sse - optimum).abs() <= 1e-10 * optimum.max(1.0));
    }

    #[test]
    fn exact_i128_alignment_matches_dense_signed_coefficient_reference() {
        let block = allocate(&[fixture(45)], BUDGET_PER_ROW).unwrap();
        let masks = [0, u64::MAX, 0x55aa_55aa_55aa_55aa, 0x1234_5678_9abc_def0];
        let exact = dot_exact(&block.wires[0], &masks).unwrap();
        let (codes, exponents) = unpack(&block.wires[0]).unwrap();
        let oracle: i128 = (0..WEIGHTS)
            .map(|i| {
                let q = i128::from(codes[i]);
                (if (masks[i / 64] >> (i % 64)) & 1 != 0 {
                    -q
                } else {
                    q
                }) << (i32::from(exponents[i / WIDTH]) + 24)
            })
            .sum();
        assert_eq!(exact, oracle);
        assert!(matches!(allocate(&[], 10_000), Err(Error::Empty)));
        assert!(matches!(allocate(&[fixture(1)], 72), Err(Error::Budget)));
    }

    #[test]
    fn reject_nonfinite_and_negative_zero_on_wire() {
        let mut source = fixture(7);
        source[0] = f64::NAN;
        assert!(matches!(candidates(&source), Err(Error::NonFinite)));
        let block = allocate(&[fixture(7)], BUDGET_PER_ROW).unwrap();
        let mut wire = block.wires[0].clone();
        let (codes, _) = unpack(&wire).unwrap();
        let zero = codes
            .iter()
            .position(|&x| x == 0)
            .expect("a zero source code");
        wire[9 + (zero / 64) * 8 + (zero % 64) / 8] |= 1 << (zero % 8);
        assert_eq!(unpack(&wire), Err(Error::NegativeZero));
    }
}
