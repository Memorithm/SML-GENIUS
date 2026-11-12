//! BPB-3-A7: independently encoded 6-bit absolute dyadic exponents per
//! 16 coefficients, with Boolean signed bitplanes and exact byte budget.
//! Research only, not Posit Standard.
#![forbid(unsafe_code)]

#[path = "bpb3_a4_local16.rs"]
#[allow(dead_code)]
mod a4;
#[cfg(test)]
#[path = "bpb3_a6_delta_scales.rs"]
#[allow(dead_code)]
mod a6;

use std::fs;
use std::path::Path;

const LANES: usize = 64;
const GROUPS: usize = 4;
const SUBGROUP: usize = 16;
const ROW_BYTES: usize = 108;
const MIN_BYTES: usize = 20;
const MAX_BLOCKS: usize = 256;
const MASK_SEED: u64 = 0xa3b0_114d_5c15_2026;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Empty,
    TooManyBlocks,
    InvalidMode,
    InvalidExponent,
    InvalidCode,
    NegativeZero,
    WrongLength,
    NonFinite,
    Budget,
    NoCandidate,
    Overflow,
}

#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub mode: u8,
    pub exponent: i8,
    pub codes: [i8; SUBGROUP],
    pub sse: f64,
}

#[derive(Clone, Debug)]
pub struct Allocation {
    pub groups: Vec<Group>,
    pub wires: Vec<Vec<u8>>,
    pub total_bytes: usize,
    pub sse: f64,
}

pub fn wire_bytes(modes: &[u8; GROUPS]) -> Result<usize, Error> {
    if modes.iter().any(|&mode| !(1..=3).contains(&mode)) {
        return Err(Error::InvalidMode);
    }
    Ok(12 + 2 * modes.iter().map(|&mode| usize::from(mode)).sum::<usize>())
}

/// One mode byte, 24-bit LE (4 x 6-bit exponent+24), sign u64 LE,
/// and per-group 16-bit unsigned magnitude bitplanes in LSB-first order.
pub fn pack(groups: &[Group; GROUPS]) -> Result<Vec<u8>, Error> {
    let modes = groups.map(|g| g.mode);
    let expected = wire_bytes(&modes)?;
    let mut mode_byte = 0u8;
    let mut packed_exponents = 0u32;
    let mut sign = 0u64;
    for (group, choice) in groups.iter().enumerate() {
        if !(-24..=27).contains(&choice.exponent) {
            return Err(Error::InvalidExponent);
        }
        mode_byte |= (choice.mode - 1) << (2 * group);
        packed_exponents |= ((i16::from(choice.exponent) + 24) as u32) << (6 * group);
        let limit = (1i16 << choice.mode) - 1;
        for (i, &code) in choice.codes.iter().enumerate() {
            if i16::from(code).abs() > limit {
                return Err(Error::InvalidCode);
            }
            if code < 0 {
                sign |= 1u64 << (group * SUBGROUP + i);
            }
        }
    }
    let mut bytes = Vec::with_capacity(expected);
    bytes.push(mode_byte);
    bytes.extend_from_slice(&packed_exponents.to_le_bytes()[..3]);
    bytes.extend_from_slice(&sign.to_le_bytes());
    for group in groups {
        for plane in 0..usize::from(group.mode) {
            let mut bits = 0u16;
            for (index, &code) in group.codes.iter().enumerate() {
                if code.unsigned_abs() & (1 << plane) != 0 {
                    bits |= 1u16 << index;
                }
            }
            bytes.extend_from_slice(&bits.to_le_bytes());
        }
    }
    assert_eq!(bytes.len(), expected);
    Ok(bytes)
}

type Parsed = ([u8; GROUPS], [i8; GROUPS], u64, [[u16; 3]; GROUPS]);

fn parse(bytes: &[u8]) -> Result<Parsed, Error> {
    if bytes.len() < 12 {
        return Err(Error::WrongLength);
    }
    let modes: [u8; GROUPS] = std::array::from_fn(|i| ((bytes[0] >> (2 * i)) & 3) + 1);
    if modes.contains(&4) {
        return Err(Error::InvalidMode);
    }
    if bytes.len() != wire_bytes(&modes)? {
        return Err(Error::WrongLength);
    }
    let packed = u32::from_le_bytes([bytes[1], bytes[2], bytes[3], 0]);
    let exponents = std::array::from_fn(|g| ((packed >> (6 * g)) & 63) as u8);
    if exponents.iter().any(|&e| e > 51) {
        return Err(Error::InvalidExponent);
    }
    let signed_exponents = exponents.map(|e| e as i8 - 24);
    let sign = u64::from_le_bytes(bytes[4..12].try_into().unwrap());
    let mut cursor = 12;
    let mut planes = [[0u16; 3]; GROUPS];
    for (g, group) in planes.iter_mut().enumerate() {
        let mut occupied = 0u16;
        for plane in &mut group[..usize::from(modes[g])] {
            *plane = u16::from_le_bytes(bytes[cursor..cursor + 2].try_into().unwrap());
            occupied |= *plane;
            cursor += 2;
        }
        let signs = ((sign >> (g * SUBGROUP)) & 0xffff) as u16;
        if signs & !occupied != 0 {
            return Err(Error::NegativeZero);
        }
    }
    assert_eq!(cursor, bytes.len());
    Ok((modes, signed_exponents, sign, planes))
}

pub fn unpack(bytes: &[u8]) -> Result<([i8; LANES], [i8; GROUPS]), Error> {
    let (modes, exponents, sign, planes) = parse(bytes)?;
    let codes = std::array::from_fn(|i| {
        let group = i / SUBGROUP;
        let local = i % SUBGROUP;
        let magnitude = planes[group][..usize::from(modes[group])]
            .iter()
            .enumerate()
            .map(|(plane, bits)| (((bits >> local) & 1) << plane) as i8)
            .sum::<i8>();
        if (sign >> i) & 1 != 0 {
            -magnitude
        } else {
            magnitude
        }
    });
    Ok((codes, exponents))
}

/// A set input mask bit is bipolar -1. Every group integer dot is
/// computed directly from 16-lane magnitude bitplanes using POPCOUNT.
pub fn dot_subgroups(bytes: &[u8], mask: u64) -> Result<[(i32, i8); GROUPS], Error> {
    let (modes, exponents, sign, planes) = parse(bytes)?;
    Ok(std::array::from_fn(|g| {
        let weight_sign = ((sign >> (g * SUBGROUP)) & 0xffff) as u16;
        let input_sign = ((mask >> (g * SUBGROUP)) & 0xffff) as u16;
        let signs_equal = !(weight_sign ^ input_sign);
        let mut sum = 0i32;
        for (index, &plane) in planes[g][..usize::from(modes[g])].iter().enumerate() {
            sum += (2 * (plane & signs_equal).count_ones() as i32 - plane.count_ones() as i32)
                * (1 << index);
        }
        (sum, exponents[g])
    }))
}

/// Exact checked dyadic sum at common exponent 2^-24.
pub fn dot_exact(wires: &[Vec<u8>], masks: &[u64]) -> Result<i128, Error> {
    if wires.len() != masks.len() {
        return Err(Error::WrongLength);
    }
    let mut sum = 0i128;
    for (wire, &mask) in wires.iter().zip(masks) {
        for (integer, exponent) in dot_subgroups(wire, mask)? {
            let factor = 1i128
                .checked_shl((i32::from(exponent) + 24) as u32)
                .ok_or(Error::Overflow)?;
            let term = i128::from(integer)
                .checked_mul(factor)
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
        for (value, exponent) in dot_subgroups(wire, mask)? {
            sum += f64::from(value) * 2.0f64.powi(i32::from(exponent));
        }
    }
    Ok(sum)
}

/// Independent 52-scale exhaustive quantizer per group and magnitude
/// precision. A4's tested quantizer already searches each subgroup
/// independently when its uniform 64-lane mode is forced to m.
fn alternatives(source: &[f64; LANES]) -> Result<[[Group; 3]; GROUPS], Error> {
    if source.iter().any(|value| !value.is_finite()) {
        return Err(Error::NonFinite);
    }
    let quant = [
        a4::quantize(source, 1).map_err(|_| Error::NonFinite)?,
        a4::quantize(source, 2).map_err(|_| Error::NonFinite)?,
        a4::quantize(source, 3).map_err(|_| Error::NonFinite)?,
    ];
    let opts = std::array::from_fn(|g| {
        std::array::from_fn(|mode| {
            let chosen = &quant[mode];
            let exponent = chosen.exponents[g];
            let scale = 2.0f64.powi(i32::from(exponent));
            let codes = std::array::from_fn(|i| chosen.codes[g * SUBGROUP + i]);
            let sse: f64 = (0..SUBGROUP)
                .map(|i| {
                    let error = source[g * SUBGROUP + i] - f64::from(codes[i]) * scale;
                    error * error
                })
                .sum();
            Group {
                mode: (mode + 1) as u8,
                exponent,
                codes,
                sse,
            }
        })
    });
    if opts
        .iter()
        .flat_map(|group| group.iter())
        .any(|group: &Group| !group.sse.is_finite())
    {
        return Err(Error::NonFinite);
    }
    Ok(opts)
}

/// Full-cost global multiple-choice knapsack over 16-coefficient groups.
/// 12B fixed per 64 block, 2B per active 16-bit magnitude bitplane.
/// Backpointers are deterministic on equal-SSE ties.
pub fn allocate(source: &[[f64; LANES]], budget: usize) -> Result<Allocation, Error> {
    if source.is_empty() {
        return Err(Error::Empty);
    }
    if source.len() > MAX_BLOCKS {
        return Err(Error::TooManyBlocks);
    }
    let count = source.len() * GROUPS;
    let base = source.len() * MIN_BYTES;
    if budget < base {
        return Err(Error::Budget);
    }
    let extra = ((budget - base) / 2).min(count * 2);
    let width = extra + 1;
    let choices: Vec<[Group; 3]> = source
        .iter()
        .map(alternatives)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flat_map(|group| group.into_iter())
        .collect();
    assert_eq!(choices.len(), count);
    let mut dp = vec![f64::INFINITY; (count + 1) * width];
    let mut back = vec![0u8; (count + 1) * width];
    dp[0] = 0.0;
    for index in 0..count {
        for spent in 0..=extra {
            let previous = dp[index * width + spent];
            if !previous.is_finite() {
                continue;
            }
            for mode in 1..=3usize {
                let next = spent + mode - 1;
                if next > extra {
                    continue;
                }
                let candidate = previous + choices[index][mode - 1].sse;
                let slot = (index + 1) * width + next;
                if candidate < dp[slot] {
                    dp[slot] = candidate;
                    back[slot] = mode as u8;
                }
            }
        }
    }
    let mut best = f64::INFINITY;
    let mut spent = 0;
    for i in 0..=extra {
        let sse = dp[count * width + i];
        if sse < best {
            best = sse;
            spent = i;
        }
    }
    if !best.is_finite() {
        return Err(Error::NoCandidate);
    }
    let mut groups = Vec::with_capacity(count);
    let used = spent;
    for i in (1..=count).rev() {
        let mode = back[i * width + spent];
        if !(1..=3).contains(&mode) {
            return Err(Error::NoCandidate);
        }
        groups.push(choices[i - 1][usize::from(mode - 1)]);
        spent -= usize::from(mode - 1);
    }
    groups.reverse();
    let wires: Vec<Vec<u8>> = groups
        .chunks_exact(GROUPS)
        .map(|chunk| {
            let groups: &[Group; GROUPS] = chunk.try_into().unwrap();
            pack(groups)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let total_bytes = base + used * 2;
    if wires.iter().map(Vec::len).sum::<usize>() != total_bytes || total_bytes > budget {
        return Err(Error::Budget);
    }
    let measured_sse: f64 = groups.iter().map(|group| group.sse).sum();
    if (measured_sse - best).abs() > 1e-10 * best.max(1.0) {
        return Err(Error::NoCandidate);
    }
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
    let allocated = allocate(&source, weights.len() * ROW_BYTES)?;
    let mut source_sse = 0.0;
    let mut dot_sse = 0.0;
    let mut dots = 0usize;
    let mut rng = MASK_SEED
        ^ if split == "TEST" {
            0x9e37_79b9_7f4a_7c15
        } else {
            0
        };
    for (row, original) in weights.iter().enumerate() {
        let truth: [f64; 256] = original.map(f64::from);
        let wires = &allocated.wires[row * 4..row * 4 + 4];
        for (b, wire) in wires.iter().enumerate() {
            let (codes, exponents) = unpack(wire)?;
            for (i, &q) in codes.iter().enumerate() {
                let predicted = f64::from(q) * 2.0f64.powi(i32::from(exponents[i / SUBGROUP]));
                let residual = truth[b * LANES + i] - predicted;
                source_sse += residual * residual;
            }
        }
        for _ in 0..16 {
            let masks: [u64; 4] = std::array::from_fn(|_| random(&mut rng));
            let reference: f64 = truth
                .iter()
                .enumerate()
                .map(|(i, &x)| {
                    if (masks[i / 64] >> (i % 64)) & 1 == 0 {
                        x
                    } else {
                        -x
                    }
                })
                .sum();
            let value = dot_256(wires, &masks)?;
            let residual = reference - value;
            dot_sse += residual * residual;
            dots += 1;
        }
    }
    if (source_sse - allocated.sse).abs() > 1e-7 * source_sse.max(1.0) {
        return Err(Error::NoCandidate);
    }
    let coeffs = weights.len() * 256;
    let modes = [1u8, 2, 3].map(|mode| {
        allocated
            .groups
            .iter()
            .filter(|group| group.mode == mode)
            .count()
    });
    println!(
        "{split},bpb3_a7_absolute6,{},{:.6},{:.12},{:.12},{},{},,,{},{},{}",
        allocated.total_bytes,
        allocated.total_bytes as f64 * 8.0 / coeffs as f64,
        (source_sse / coeffs as f64).sqrt(),
        (dot_sse / dots as f64).sqrt(),
        coeffs,
        dots,
        modes[0],
        modes[1],
        modes[2],
    );
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 || args[1] != "--external-f32le" {
        eprintln!("usage: bpb3-a7 --external-f32le 48x256-pinned-trained-source");
        std::process::exit(2);
    }
    let bytes = fs::read(Path::new(&args[2])).expect("STOP_A7_SOURCE_READ");
    assert_eq!(bytes.len(), 48 * 256 * 4, "STOP_A7_SOURCE_LENGTH");
    let weights: Vec<[f32; 256]> = bytes
        .chunks_exact(256 * 4)
        .map(|bytes| {
            std::array::from_fn(|i| f32::from_le_bytes(bytes[4 * i..4 * i + 4].try_into().unwrap()))
        })
        .collect();
    assert!(weights.iter().all(|row| row.iter().all(|x| x.is_finite())));
    println!(
        "protocol=bpb3-a7-absolute6-budget108-v1,source=EXTERNALLY_PINNED,claim=COMPONENT_ONLY"
    );
    println!("split,codec,payload_bytes,bits_per_weight,weight_rmse,dot_rmse,coefficients,dots,delta2_blocks,delta4_blocks,mode1_groups,mode2_groups,mode3_groups");
    evaluate("DEV", &weights[..24]).expect("A7 DEV");
    evaluate("TEST", &weights[24..]).expect("A7 TEST");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(seed: u64) -> [f64; LANES] {
        let mut state = seed;
        std::array::from_fn(|i| {
            let x = (random(&mut state) >> 32) as f64 / (u32::MAX as f64) - 0.5;
            x * 2.0f64.powi((i / 16) as i32 * 6 - 12)
        })
    }

    #[test]
    fn canonical_full_24bit_exponent_field_roundtrip_and_rejections() {
        let a = alternatives(&fixture(12)).unwrap();
        let groups = [a[0][0], a[1][1], a[2][2], a[3][0]];
        let wire = pack(&groups).unwrap();
        assert_eq!(wire.len(), 12 + 2 * (1 + 2 + 3 + 1));
        let (codes, exponents) = unpack(&wire).unwrap();
        for (group, choice) in groups.iter().enumerate() {
            assert_eq!(exponents[group], choice.exponent);
            for i in 0..SUBGROUP {
                assert_eq!(codes[group * SUBGROUP + i], choice.codes[i]);
            }
        }
        let mut invalid = wire.clone();
        invalid[0] |= 0b11; // reserved two-bit precision field
        assert_eq!(unpack(&invalid), Err(Error::InvalidMode));
        let mut reserved = wire.clone();
        reserved[1] |= 0b11_0100; // group 0 code 52 (all code bits)
        assert_eq!(unpack(&reserved), Err(Error::InvalidExponent));
        let mut truncated = wire.clone();
        truncated.pop();
        assert_eq!(unpack(&truncated), Err(Error::WrongLength));
        let mut negative_zero = pack(
            &[Group {
                mode: 1,
                exponent: 0,
                codes: [0; SUBGROUP],
                sse: 0.0,
            }; GROUPS],
        )
        .unwrap();
        negative_zero[4] |= 1;
        assert_eq!(unpack(&negative_zero), Err(Error::NegativeZero));
    }

    #[test]
    fn full_exponent_range_and_invalid_group_values() {
        let bases = [-24, 27, -24, 27];
        let group = [Group {
            mode: 3,
            exponent: 0,
            codes: [1; SUBGROUP],
            sse: 0.0,
        }; GROUPS];
        let mut groups = group;
        for g in 0..GROUPS {
            groups[g].exponent = bases[g];
        }
        let wire = pack(&groups).unwrap();
        assert_eq!(unpack(&wire).unwrap().1, bases);
        let mut bad = groups;
        bad[2].exponent = 28;
        assert_eq!(pack(&bad).err(), Some(Error::InvalidExponent));
        bad = groups;
        bad[2].codes[3] = 8;
        assert_eq!(pack(&bad).err(), Some(Error::InvalidCode));
    }

    #[test]
    fn signed_popcount_matches_independent_dense_bipolar_oracle() {
        let mut state = 0x1ad7_a4c0_2185_u64;
        for seed in 0..32u64 {
            let opts = alternatives(&fixture(seed + 1)).unwrap();
            let groups = [opts[0][0], opts[1][1], opts[2][2], opts[3][1]];
            let wire = pack(&groups).unwrap();
            for mask in [0u64, u64::MAX, random(&mut state), random(&mut state)] {
                let seen = dot_subgroups(&wire, mask).unwrap();
                for g in 0..GROUPS {
                    let dense: i32 = groups[g]
                        .codes
                        .iter()
                        .enumerate()
                        .map(|(i, &q)| {
                            if (mask >> (g * SUBGROUP + i)) & 1 == 0 {
                                i32::from(q)
                            } else {
                                -i32::from(q)
                            }
                        })
                        .sum();
                    assert_eq!(seen[g], (dense, groups[g].exponent));
                }
            }
        }
    }

    #[test]
    fn exact_global_dp_matches_exhaustive_one_block_oracle() {
        let block = fixture(101);
        let choices = alternatives(&block).unwrap();
        let candidate = allocate(&[block], 27).unwrap();
        let mut brute = f64::INFINITY;
        for a in 0..3usize {
            for b in 0..3usize {
                for c in 0..3usize {
                    for d in 0..3usize {
                        let m = [a, b, c, d];
                        let cost = 12 + 2 * m.iter().map(|&v| v + 1).sum::<usize>();
                        if cost <= 27 {
                            brute = brute.min((0..4).map(|i| choices[i][m[i]].sse).sum());
                        }
                    }
                }
            }
        }
        assert!(candidate.total_bytes <= 27);
        assert_eq!(candidate.total_bytes, candidate.wires[0].len());
        assert!((candidate.sse - brute).abs() <= 1e-10 * brute.max(1.0));
    }

    #[test]
    fn checked_i128_dot_matches_dense_integer_and_budget_limits() {
        let sources: Vec<_> = (0..4).map(|i| fixture(i + 20)).collect();
        assert!(matches!(allocate(&sources, 79), Err(Error::Budget)));
        let result = allocate(&sources, ROW_BYTES).unwrap();
        let masks = [0, u64::MAX, 0x55aa_55aa_55aa_55aa, 0x0123_4567_89ab_cdef];
        let actual = dot_exact(&result.wires, &masks).unwrap();
        let oracle: i128 = result
            .wires
            .iter()
            .zip(masks)
            .map(|(wire, mask)| {
                let (codes, exponents) = unpack(wire).unwrap();
                (0..LANES)
                    .map(|i| {
                        let x = i128::from(codes[i]);
                        (if (mask >> i) & 1 == 0 { x } else { -x })
                            << (i32::from(exponents[i / SUBGROUP]) + 24)
                    })
                    .sum::<i128>()
            })
            .sum();
        assert_eq!(actual, oracle);
        assert_eq!(dot_exact(&result.wires, &[]), Err(Error::WrongLength));
    }

    #[test]
    fn a7_admits_every_delta4_exponent_combination_at_exact_same_bytes() {
        for seed in 1..=4u64 {
            let source = [
                fixture(seed),
                fixture(seed + 20),
                fixture(seed + 40),
                fixture(seed + 60),
            ];
            let old_opts: Vec<_> = source.iter().map(|x| a6::candidates(x).unwrap()).collect();
            let delta4 = a6::allocate(&old_opts, ROW_BYTES, false, true).unwrap();
            let absolute = allocate(&source, ROW_BYTES).unwrap();
            assert_eq!(delta4.total_bytes, ROW_BYTES);
            assert!(absolute.total_bytes <= ROW_BYTES);
            assert!(
                absolute.sse <= delta4.sse + 1e-9 * delta4.sse.max(1.0),
                "seed={seed}: a7={} delta4={}",
                absolute.sse,
                delta4.sse
            );
            for b in &delta4.blocks {
                assert_eq!(b.delta, a6::Delta::Four);
                let converted: [Group; GROUPS] = b.groups.map(|g| Group {
                    mode: g.mode,
                    exponent: g.exponent,
                    codes: g.codes,
                    sse: g.sse,
                });
                assert_eq!(
                    pack(&converted).unwrap().len(),
                    a6::wire_bytes(b.delta, &b.groups.map(|g| g.mode)).unwrap()
                );
            }
        }
    }

    #[test]
    fn source_finite_guard_and_scale_metadata_boundaries() {
        let mut source = fixture(8);
        source[7] = f64::NAN;
        assert!(matches!(allocate(&[source], 108), Err(Error::NonFinite)));
        let sources = vec![[0.0f64; LANES]; MAX_BLOCKS + 1];
        assert!(matches!(
            allocate(&sources, 10_000),
            Err(Error::TooManyBlocks)
        ));
        assert!(matches!(allocate(&[], 10_000), Err(Error::Empty)));
    }
}
