//! BPB-3-A6: fixed-width base-plus-local-delta exponents and mixed
//! per-16 signed Boolean magnitude precision under exact byte budgets.
//! Research-only, not a Posit Standard numeric representation.
#![forbid(unsafe_code)]

use std::fs;
use std::path::Path;

const LANES: usize = 64;
const GROUPS: usize = 4;
const WIDTH: usize = 16;
const EXPONENTS: usize = 52;
const MIN_BYTES: usize = 19;
const MAX_BLOCKS: usize = 256;
const ROW_BYTES: usize = 108;
const MASK_SEED: u64 = 0xa3b0_114d_5c15_2026;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Empty,
    TooManyBlocks,
    InvalidMode,
    InvalidExponent,
    InvalidDelta,
    InvalidBaseSymbol,
    NegativeZero,
    WrongLength,
    NonFinite,
    Budget,
    NoCandidate,
    Overflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delta {
    Two,
    Four,
}

impl Delta {
    fn storage(self) -> usize {
        match self {
            Self::Two => 1,
            Self::Four => 2,
        }
    }
    fn bounds(self) -> (i8, i8) {
        match self {
            Self::Two => (-1, 2),
            Self::Four => (-8, 7),
        }
    }
    fn tag(self) -> u8 {
        match self {
            Self::Two => 0,
            Self::Four => EXPONENTS as u8,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub mode: u8,
    pub exponent: i8,
    pub codes: [i8; WIDTH],
    pub sse: f64,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub delta: Delta,
    pub base: i8,
    pub groups: [Group; GROUPS],
    pub sse: f64,
}

#[derive(Clone, Debug)]
pub struct Allocation {
    pub blocks: Vec<Block>,
    pub wires: Vec<Vec<u8>>,
    pub total_bytes: usize,
    pub sse: f64,
}

#[derive(Clone, Copy)]
struct Quant16 {
    codes: [i8; WIDTH],
    sse: f64,
}

pub fn wire_bytes(delta: Delta, modes: &[u8; GROUPS]) -> Result<usize, Error> {
    if modes.iter().any(|&mode| !(1..=3).contains(&mode)) {
        return Err(Error::InvalidMode);
    }
    Ok(10 + delta.storage() + 2 * modes.iter().map(|&m| usize::from(m)).sum::<usize>())
}

/// On wire: 1 byte of 4x2-bit magnitude modes; 1 base+variant symbol;
/// 1 or 2 bytes packed local exponent offsets; 8-byte 64-bit sign mask;
/// then u16 LE bitplanes for each subgroup in low-to-high significance.
pub fn pack(block: &Block) -> Result<Vec<u8>, Error> {
    if !(-24..=27).contains(&block.base) {
        return Err(Error::InvalidExponent);
    }
    let modes = block.groups.map(|g| g.mode);
    let expected = wire_bytes(block.delta, &modes)?;
    let mut mode_byte = 0u8;
    let mut offsets = [0u8; 2];
    let mut sign = 0u64;
    let (low, high) = block.delta.bounds();
    for (g, choice) in block.groups.iter().enumerate() {
        mode_byte |= (choice.mode - 1) << (2 * g);
        if !(-24..=27).contains(&choice.exponent) {
            return Err(Error::InvalidExponent);
        }
        let difference = i16::from(choice.exponent) - i16::from(block.base);
        if difference < i16::from(low) || difference > i16::from(high) {
            return Err(Error::InvalidDelta);
        }
        let packed = (difference - i16::from(low)) as u8;
        match block.delta {
            Delta::Two => offsets[0] |= packed << (2 * g),
            Delta::Four => offsets[g / 2] |= packed << (4 * (g % 2)),
        }
        let max = (1i16 << choice.mode) - 1;
        for (i, &code) in choice.codes.iter().enumerate() {
            if i16::from(code).abs() > max {
                return Err(Error::InvalidMode);
            }
            if code < 0 {
                sign |= 1u64 << (g * WIDTH + i);
            }
        }
    }
    let mut output = Vec::with_capacity(expected);
    output.push(mode_byte);
    output.push((block.base + 24) as u8 + block.delta.tag());
    output.extend_from_slice(&offsets[..block.delta.storage()]);
    output.extend_from_slice(&sign.to_le_bytes());
    for group in &block.groups {
        for bit in 0..usize::from(group.mode) {
            let mut plane = 0u16;
            for (index, &code) in group.codes.iter().enumerate() {
                if code.unsigned_abs() & (1 << bit) != 0 {
                    plane |= 1u16 << index;
                }
            }
            output.extend_from_slice(&plane.to_le_bytes());
        }
    }
    assert_eq!(output.len(), expected);
    Ok(output)
}

type Parsed = (
    [u8; GROUPS],
    [i8; GROUPS],
    u64,
    [[u16; 3]; GROUPS],
    Delta,
    i8,
);

fn parse(raw: &[u8]) -> Result<Parsed, Error> {
    if raw.len() < 11 {
        return Err(Error::WrongLength);
    }
    let modes: [u8; GROUPS] = std::array::from_fn(|g| ((raw[0] >> (2 * g)) & 3) + 1);
    if modes.iter().any(|&m| m == 4) {
        return Err(Error::InvalidMode);
    }
    let symbol = raw[1];
    let (variant, idx) = match symbol {
        0..=51 => (Delta::Two, symbol),
        52..=103 => (Delta::Four, symbol - 52),
        _ => return Err(Error::InvalidBaseSymbol),
    };
    let base = idx as i8 - 24;
    let expected = wire_bytes(variant, &modes)?;
    if raw.len() != expected {
        return Err(Error::WrongLength);
    }
    let (low, _) = variant.bounds();
    let exponents = std::array::from_fn(|g| {
        let code = match variant {
            Delta::Two => (raw[2] >> (2 * g)) & 3,
            Delta::Four => (raw[2 + g / 2] >> (4 * (g % 2))) & 15,
        };
        base as i16 + i16::from(code) + i16::from(low)
    });
    if exponents.iter().any(|&k| !(-24..=27).contains(&k)) {
        return Err(Error::InvalidExponent);
    }
    let exponent_bytes: [i8; GROUPS] = exponents.map(|x| x as i8);
    let sign_pos = 2 + variant.storage();
    let sign = u64::from_le_bytes(raw[sign_pos..sign_pos + 8].try_into().unwrap());
    let mut cursor = sign_pos + 8;
    let mut planes = [[0u16; 3]; GROUPS];
    for (g, row) in planes.iter_mut().enumerate() {
        let mut occupied = 0u16;
        for plane in &mut row[..usize::from(modes[g])] {
            *plane = u16::from_le_bytes(raw[cursor..cursor + 2].try_into().unwrap());
            occupied |= *plane;
            cursor += 2;
        }
        let sign16 = ((sign >> (16 * g)) & 0xffff) as u16;
        if sign16 & !occupied != 0 {
            return Err(Error::NegativeZero);
        }
    }
    assert_eq!(cursor, raw.len());
    Ok((modes, exponent_bytes, sign, planes, variant, base))
}

pub fn unpack(raw: &[u8]) -> Result<([i8; LANES], [i8; GROUPS]), Error> {
    let (modes, exponents, sign, planes, _, _) = parse(raw)?;
    let codes = std::array::from_fn(|i| {
        let group = i / WIDTH;
        let local = i % WIDTH;
        let magnitude: i8 = planes[group][..usize::from(modes[group])]
            .iter()
            .enumerate()
            .map(|(p, b)| (((b >> local) & 1) << p) as i8)
            .sum();
        if (sign >> i) & 1 != 0 {
            -magnitude
        } else {
            magnitude
        }
    });
    Ok((codes, exponents))
}

/// Direct 16-bit masked POPCOUNT, no dense coefficient materialization.
pub fn dot_groups(raw: &[u8], bipolar: u64) -> Result<[(i32, i8); GROUPS], Error> {
    let (modes, exponents, sign, planes, _, _) = parse(raw)?;
    Ok(std::array::from_fn(|group| {
        let input = ((bipolar >> (WIDTH * group)) & 0xffff) as u16;
        let negative = ((sign >> (WIDTH * group)) & 0xffff) as u16;
        let equal_sign = !(input ^ negative);
        let mut total = 0i32;
        for (p, &plane) in planes[group][..usize::from(modes[group])]
            .iter()
            .enumerate()
        {
            total += (2 * (plane & equal_sign).count_ones() as i32 - plane.count_ones() as i32)
                * (1 << p);
        }
        (total, exponents[group])
    }))
}

pub fn dot_exact(wires: &[Vec<u8>], inputs: &[u64]) -> Result<i128, Error> {
    if wires.len() != inputs.len() {
        return Err(Error::WrongLength);
    }
    let mut sum = 0i128;
    for (wire, &input) in wires.iter().zip(inputs) {
        for (integer, exponent) in dot_groups(wire, input)? {
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

pub fn dot_256(wires: &[Vec<u8>], inputs: &[u64; 4]) -> Result<f64, Error> {
    if wires.len() != 4 {
        return Err(Error::WrongLength);
    }
    let mut result = 0.0;
    for (wire, &input) in wires.iter().zip(inputs) {
        for (integer, exponent) in dot_groups(wire, input)? {
            result += f64::from(integer) * 2.0f64.powi(i32::from(exponent));
        }
    }
    Ok(result)
}

/// Full 52-exponent candidates, evaluated once per subgroup and precision.
fn quant_table(source: &[f64; LANES]) -> Result<[[[Quant16; EXPONENTS]; 3]; GROUPS], Error> {
    if source.iter().any(|v| !v.is_finite()) {
        return Err(Error::NonFinite);
    }
    let table: [[[Quant16; EXPONENTS]; 3]; GROUPS] = std::array::from_fn(|g| {
        std::array::from_fn(|m| {
            std::array::from_fn(|idx| {
                let exponent = idx as i32 - 24;
                let scale = 2.0f64.powi(exponent);
                let magnitude_max = ((1u16 << (m + 1)) - 1) as f64;
                let mut squared = 0.0;
                let codes = std::array::from_fn(|i| {
                    let value = source[g * WIDTH + i];
                    let signed = (value / scale).round().clamp(-magnitude_max, magnitude_max) as i8;
                    let residual = value - f64::from(signed) * scale;
                    squared += residual * residual;
                    signed
                });
                Quant16 {
                    codes,
                    sse: squared,
                }
            })
        })
    });
    if table
        .iter()
        .flat_map(|g| g.iter().flat_map(|m| m.iter()))
        .any(|q| !q.sse.is_finite())
    {
        return Err(Error::NonFinite);
    }
    Ok(table)
}

/// For a given block, enumerate ALL valid common bases for both delta
/// widths and use an exact four-group mode knapsack. Retain the minimum
/// reconstruction SSE candidate for each *actual* packed byte length.
pub fn candidates(source: &[f64; LANES]) -> Result<Vec<Block>, Error> {
    let table = quant_table(source)?;
    let mut best_by_cost: [Option<Block>; 18] = std::array::from_fn(|_| None);
    for variant in [Delta::Two, Delta::Four] {
        let (delta_low, delta_high) = variant.bounds();
        for base in -24i8..=27 {
            let group_modes: [[Option<Group>; 3]; GROUPS] = std::array::from_fn(|g| {
                std::array::from_fn(|m| {
                    let mut best: Option<Group> = None;
                    for delta in delta_low..=delta_high {
                        let exponent = i16::from(base) + i16::from(delta);
                        if !(-24..=27).contains(&exponent) {
                            continue;
                        }
                        let q = table[g][m][(exponent + 24) as usize];
                        if best.as_ref().is_none_or(|p| q.sse < p.sse) {
                            best = Some(Group {
                                mode: (m + 1) as u8,
                                exponent: exponent as i8,
                                codes: q.codes,
                                sse: q.sse,
                            });
                        }
                    }
                    best
                })
            });
            let mut dp = [[f64::INFINITY; 9]; 5];
            let mut back = [[0u8; 9]; 5];
            dp[0][0] = 0.0;
            for g in 0..GROUPS {
                for used in 0..=8usize {
                    if !dp[g][used].is_finite() {
                        continue;
                    }
                    for mode in 1..=3usize {
                        let next = used + mode - 1;
                        if next > 8 {
                            continue;
                        }
                        if let Some(choice) = group_modes[g][mode - 1] {
                            let value = dp[g][used] + choice.sse;
                            if value < dp[g + 1][next] {
                                dp[g + 1][next] = value;
                                back[g + 1][next] = mode as u8;
                            }
                        }
                    }
                }
            }
            for used in 0..=8usize {
                let sse = dp[GROUPS][used];
                if !sse.is_finite() {
                    continue;
                }
                let byte_count = 10 + variant.storage() + 2 * (4 + used);
                let index = byte_count - MIN_BYTES;
                if best_by_cost[index]
                    .as_ref()
                    .is_some_and(|prior| prior.sse <= sse)
                {
                    continue;
                }
                let mut remaining = used;
                let mut groups: [Option<Group>; GROUPS] = [None; GROUPS];
                for i in (1..=GROUPS).rev() {
                    let mode = back[i][remaining];
                    if mode == 0 {
                        return Err(Error::NoCandidate);
                    }
                    groups[i - 1] = group_modes[i - 1][usize::from(mode - 1)];
                    remaining -= usize::from(mode - 1);
                }
                if remaining != 0 {
                    return Err(Error::NoCandidate);
                }
                let groups = groups.map(|g| g.expect("admitted group choice"));
                let block = Block {
                    delta: variant,
                    base,
                    groups,
                    sse,
                };
                if pack(&block)?.len() != byte_count {
                    return Err(Error::WrongLength);
                }
                best_by_cost[index] = Some(block);
            }
        }
    }
    Ok(best_by_cost.into_iter().flatten().collect())
}

/// One exact global multiple-choice DP for each prospective A6 arm.
/// 19B minimum per block, no free base, delta or random-access metadata.
pub fn allocate(
    blocks: &[Vec<Block>],
    budget: usize,
    allow_two: bool,
    allow_four: bool,
) -> Result<Allocation, Error> {
    if blocks.is_empty() {
        return Err(Error::Empty);
    }
    if blocks.len() > MAX_BLOCKS {
        return Err(Error::TooManyBlocks);
    }
    let minimum = blocks.len().checked_mul(MIN_BYTES).ok_or(Error::Overflow)?;
    if budget < minimum {
        return Err(Error::Budget);
    }
    let extra = budget - minimum;
    let width = extra + 1;
    let mut dp = vec![f64::INFINITY; (blocks.len() + 1) * width];
    let mut back = vec![0u8; (blocks.len() + 1) * width];
    dp[0] = 0.0;
    for (block_index, candidates) in blocks.iter().enumerate() {
        for used in 0..=extra {
            let previous = dp[block_index * width + used];
            if !previous.is_finite() {
                continue;
            }
            for (choice_index, choice) in candidates.iter().enumerate() {
                if (choice.delta == Delta::Two && !allow_two)
                    || (choice.delta == Delta::Four && !allow_four)
                {
                    continue;
                }
                let cost = wire_bytes(choice.delta, &choice.groups.map(|x| x.mode))?;
                let next = used + cost - MIN_BYTES;
                if next > extra {
                    continue;
                }
                let value = previous + choice.sse;
                let position = (block_index + 1) * width + next;
                if value < dp[position] {
                    dp[position] = value;
                    back[position] = (choice_index + 1) as u8;
                }
            }
        }
    }
    let mut best_sse = f64::INFINITY;
    let mut used = 0usize;
    for n in 0..=extra {
        if dp[blocks.len() * width + n] < best_sse {
            best_sse = dp[blocks.len() * width + n];
            used = n;
        }
    }
    if !best_sse.is_finite() {
        return Err(Error::NoCandidate);
    }
    let total_bytes = minimum + used;
    let mut out = Vec::with_capacity(blocks.len());
    for i in (1..=blocks.len()).rev() {
        let idx = back[i * width + used];
        if idx == 0 {
            return Err(Error::NoCandidate);
        }
        let candidate = blocks[i - 1][usize::from(idx - 1)].clone();
        let price = wire_bytes(candidate.delta, &candidate.groups.map(|x| x.mode))?;
        used -= price - MIN_BYTES;
        out.push(candidate);
    }
    out.reverse();
    let wires = out.iter().map(pack).collect::<Result<Vec<_>, _>>()?;
    if wires.iter().map(Vec::len).sum::<usize>() != total_bytes || total_bytes > budget {
        return Err(Error::Budget);
    }
    Ok(Allocation {
        blocks: out,
        wires,
        total_bytes,
        sse: best_sse,
    })
}

fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed >> 12;
    *seed ^= *seed << 25;
    *seed ^= *seed >> 27;
    (*seed).wrapping_mul(2685821657736338717)
}

fn evaluate(split: &str, rows: &[[f32; 256]]) -> Result<(), Error> {
    let source: Vec<[f64; LANES]> = rows
        .iter()
        .flat_map(|row| (0..4).map(move |g| std::array::from_fn(|i| f64::from(row[g * 64 + i]))))
        .collect();
    let all = source
        .iter()
        .map(candidates)
        .collect::<Result<Vec<_>, _>>()?;
    let arms = [
        ("bpb3_a6_delta2", true, false),
        ("bpb3_a6_delta4", false, true),
        ("bpb3_a6_mixed", true, true),
    ];
    let mut measurements = Vec::new();
    for (name, two, four) in arms {
        let chosen = allocate(&all, rows.len() * ROW_BYTES, two, four)?;
        let mut sse = 0.0f64;
        let mut dot_sse = 0.0f64;
        let mut dots = 0usize;
        let mut rng = MASK_SEED
            ^ if split == "TEST" {
                0x9e37_79b9_7f4a_7c15
            } else {
                0
            };
        for (row, original) in rows.iter().enumerate() {
            let mut exact = [0.0f64; 256];
            for (i, &value) in original.iter().enumerate() {
                exact[i] = f64::from(value);
            }
            let wires = &chosen.wires[row * 4..row * 4 + 4];
            for (group, wire) in wires.iter().enumerate() {
                let (codes, exp) = unpack(wire)?;
                for (i, &q) in codes.iter().enumerate() {
                    let scale = 2.0f64.powi(i32::from(exp[i / WIDTH]));
                    let err = exact[group * LANES + i] - f64::from(q) * scale;
                    sse += err * err;
                }
            }
            for _ in 0..16 {
                let masks: [u64; 4] = std::array::from_fn(|_| random(&mut rng));
                let original_dot: f64 = exact
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
                let dot = dot_256(wires, &masks)?;
                dot_sse += (dot - original_dot) * (dot - original_dot);
                dots += 1;
            }
        }
        if (sse - chosen.sse).abs() > 1e-7 * sse.max(1.0) {
            return Err(Error::NonFinite);
        }
        let counts = [Delta::Two, Delta::Four]
            .map(|v| chosen.blocks.iter().filter(|b| b.delta == v).count());
        println!(
            "{split},{name},{},{:.6},{:.12},{:.12},{},{},{},{},{},{},{}",
            chosen.total_bytes,
            chosen.total_bytes as f64 * 8.0 / (rows.len() * 256) as f64,
            (sse / (rows.len() * 256) as f64).sqrt(),
            (dot_sse / dots as f64).sqrt(),
            rows.len() * 256,
            dots,
            counts[0],
            counts[1],
            chosen
                .blocks
                .iter()
                .map(|b| b.groups.iter().filter(|g| g.mode == 1).count())
                .sum::<usize>(),
            chosen
                .blocks
                .iter()
                .map(|b| b.groups.iter().filter(|g| g.mode == 2).count())
                .sum::<usize>(),
            chosen
                .blocks
                .iter()
                .map(|b| b.groups.iter().filter(|g| g.mode == 3).count())
                .sum::<usize>(),
        );
        measurements.push(chosen.sse);
    }
    if measurements[2] > measurements[0] + 1e-9 * measurements[0].max(1.0)
        || measurements[2] > measurements[1] + 1e-9 * measurements[1].max(1.0)
    {
        return Err(Error::NoCandidate);
    }
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 || args[1] != "--external-f32le" {
        eprintln!("usage: bpb3-a6 --external-f32le verified-48x256-original-source");
        std::process::exit(2);
    }
    let bytes = fs::read(Path::new(&args[2])).expect("STOP_A6_SOURCE_READ");
    assert_eq!(bytes.len(), 48 * 256 * 4, "STOP_A6_SOURCE_SIZE");
    let rows: Vec<[f32; 256]> = bytes
        .chunks_exact(256 * 4)
        .map(|chunk| {
            std::array::from_fn(|i| f32::from_le_bytes(chunk[4 * i..4 * i + 4].try_into().unwrap()))
        })
        .collect();
    assert!(rows.iter().all(|row| row.iter().all(|x| x.is_finite())));
    println!("protocol=bpb3-a6-delta2-delta4-mixed-budget108-v1,source=EXTERNALLY_SHA_PINNED,claim=COMPONENT_ONLY");
    println!("split,codec,payload_bytes,bits_per_weight,weight_rmse,dot_rmse,coefficients,dots,delta2_blocks,delta4_blocks,mode1_groups,mode2_groups,mode3_groups");
    evaluate("DEV", &rows[..24]).expect("A6 DEV");
    evaluate("TEST", &rows[24..]).expect("A6 TEST");
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(seed: u64) -> [f64; LANES] {
        let mut rng = seed;
        std::array::from_fn(|i| {
            let d = (random(&mut rng) >> 32) as f64 / u32::MAX as f64 - 0.5;
            d * 2.0f64.powi((i / 16) as i32 * 4 - 10)
        })
    }

    #[test]
    fn canonical_wire_and_all_bytes_are_charged() {
        let q = source(13);
        let choices = candidates(&q).unwrap();
        for c in &choices {
            let wire = pack(c).unwrap();
            assert_eq!(
                wire.len(),
                wire_bytes(c.delta, &c.groups.map(|g| g.mode)).unwrap()
            );
            let (decoded, scales) = unpack(&wire).unwrap();
            for g in 0..4 {
                assert_eq!(scales[g], c.groups[g].exponent);
                for i in 0..WIDTH {
                    assert_eq!(decoded[g * WIDTH + i], c.groups[g].codes[i]);
                }
            }
            let mut bad = wire.clone();
            bad[0] |= 0b11;
            assert_eq!(unpack(&bad), Err(Error::InvalidMode));
            bad = wire.clone();
            bad[1] = 104;
            assert_eq!(unpack(&bad), Err(Error::InvalidBaseSymbol));
            assert_eq!(unpack(&wire[..wire.len() - 1]), Err(Error::WrongLength));
        }
    }

    #[test]
    fn signed_popcount_matches_independent_integer_dot_all_variants() {
        let src = source(0xb14f);
        for candidate in candidates(&src).unwrap() {
            let wire = pack(&candidate).unwrap();
            for mask in [0, u64::MAX, 0x1122_3344_aabb_ccdd] {
                let dots = dot_groups(&wire, mask).unwrap();
                for g in 0..GROUPS {
                    let expected: i32 = candidate.groups[g]
                        .codes
                        .iter()
                        .enumerate()
                        .map(|(i, &q)| {
                            if (mask >> (g * WIDTH + i)) & 1 == 0 {
                                i32::from(q)
                            } else {
                                -i32::from(q)
                            }
                        })
                        .sum();
                    assert_eq!(dots[g], (expected, candidate.groups[g].exponent));
                }
            }
        }
    }

    #[test]
    fn exact_budget_dp_matches_brute_force_three_blocks() {
        let blocks = [source(1), source(2), source(3)];
        let options: Vec<Vec<Block>> = blocks.iter().map(|b| candidates(b).unwrap()).collect();
        let budget = 90;
        for (two, four) in [(true, false), (false, true), (true, true)] {
            let selected = allocate(&options, budget, two, four).unwrap();
            let mut brute = f64::INFINITY;
            for a in &options[0] {
                for b in &options[1] {
                    for c in &options[2] {
                        if [a, b, c].iter().any(|block| {
                            (block.delta == Delta::Two && !two)
                                || (block.delta == Delta::Four && !four)
                        }) {
                            continue;
                        }
                        let cost = [a, b, c]
                            .iter()
                            .map(|block| {
                                wire_bytes(block.delta, &block.groups.map(|g| g.mode)).unwrap()
                            })
                            .sum::<usize>();
                        if cost <= budget {
                            brute = brute.min(a.sse + b.sse + c.sse);
                        }
                    }
                }
            }
            assert!((selected.sse - brute).abs() <= 1e-10 * brute.max(1.0));
            assert_eq!(
                selected.total_bytes,
                selected.wires.iter().map(Vec::len).sum::<usize>()
            );
        }
    }

    #[test]
    fn mixed_is_superset_of_two_or_four_and_exact_dot_is_checked() {
        let input = [source(11), source(12), source(13), source(14)];
        let options: Vec<_> = input.iter().map(|x| candidates(x).unwrap()).collect();
        assert!(matches!(
            allocate(&options, 75, true, true),
            Err(Error::Budget)
        ));
        let two = allocate(&options, 108, true, false).unwrap();
        let four = allocate(&options, 108, false, true).unwrap();
        let mixed = allocate(&options, 108, true, true).unwrap();
        assert!(mixed.sse <= two.sse + 1e-10 * two.sse.max(1.0));
        assert!(mixed.sse <= four.sse + 1e-10 * four.sse.max(1.0));
        let masks = [0, u64::MAX, 0x1223_4354_6567_7889, 0xabcd_ef01_2345_6789];
        let exact = dot_exact(&mixed.wires, &masks).unwrap();
        let dense: i128 = mixed
            .wires
            .iter()
            .zip(masks)
            .map(|(wire, mask)| {
                let (codes, exponents) = unpack(wire).unwrap();
                (0..LANES)
                    .map(|i| {
                        let q = i128::from(codes[i]);
                        (if (mask >> i) & 1 == 0 { q } else { -q })
                            << (i32::from(exponents[i / WIDTH]) + 24)
                    })
                    .sum::<i128>()
            })
            .sum();
        assert_eq!(exact, dense);
        assert_eq!(dot_exact(&mixed.wires, &[]), Err(Error::WrongLength));
    }

    #[test]
    fn source_validation_and_boundary_deltas() {
        let mut bad = source(9);
        bad[7] = f64::NAN;
        assert!(matches!(candidates(&bad), Err(Error::NonFinite)));
        let valid = source(77);
        let mut all = candidates(&valid).unwrap();
        let block = all.pop().unwrap();
        let mut wrong = block.clone();
        wrong.base = -25;
        assert_eq!(pack(&wrong), Err(Error::InvalidExponent));
        wrong = block.clone();
        wrong.base = 27;
        wrong.groups[0].exponent = -24;
        assert_eq!(pack(&wrong), Err(Error::InvalidDelta));
        assert!(
            wire_bytes(Delta::Two, &[1, 2, 3, 1]).unwrap()
                < wire_bytes(Delta::Four, &[1, 2, 3, 1]).unwrap()
        );
    }
}
