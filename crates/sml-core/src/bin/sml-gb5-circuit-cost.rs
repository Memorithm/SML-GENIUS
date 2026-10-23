//! GB-5D: independently preregistered matched CPU cost probes.
//! Usage: cargo run --locked --release -p sml-core --bin sml-gb5-circuit-cost -- deterministic
//!        cargo run --locked --release -p sml-core --bin sml-gb5-circuit-cost -- benchmark
//!
//! No model-training, hardware-speedup or SIMD acceleration claim.

use std::hint::black_box;
use std::time::Instant;

use sml_core::geometric_anf_circuit::{pack_sectors, AnfCircuit64};
use sml_core::geometric_fan::{pi_fan_mask, FanMask, RationalFan};

const A_B_ITERATIONS: u32 = 1024;
const C_ITERATIONS: u32 = 256;
const WARMUPS: usize = 3;
const REPEATS: usize = 9;

#[derive(Clone, Copy, Debug)]
enum Arm {
    LutAnd,
    AnfAnd,
    LutRestrict,
    AnfRestrict,
    LutReverse,
    AnfReverse,
    ScalarLut,
    ScalarAnf,
    BitslicePrepacked,
    BitsliceIncludingPack,
}

impl Arm {
    const fn name(self) -> &'static str {
        match self {
            Self::LutAnd => "lut_and",
            Self::AnfAnd => "anf_and",
            Self::LutRestrict => "lut_restrict",
            Self::AnfRestrict => "anf_restrict",
            Self::LutReverse => "lut_reverse",
            Self::AnfReverse => "anf_reverse",
            Self::ScalarLut => "scalar_lut",
            Self::ScalarAnf => "scalar_anf",
            Self::BitslicePrepacked => "bitslice_prepacked",
            Self::BitsliceIncludingPack => "bitslice_including_pack",
        }
    }

    const fn group(self) -> &'static str {
        match self {
            Self::LutAnd | Self::AnfAnd => "A_compose",
            Self::LutRestrict | Self::AnfRestrict | Self::LutReverse | Self::AnfReverse => {
                "B_transform"
            }
            Self::ScalarLut
            | Self::ScalarAnf
            | Self::BitslicePrepacked
            | Self::BitsliceIncludingPack => "C_64lanes",
        }
    }

    const fn iterations(self) -> u32 {
        match self {
            Self::ScalarLut
            | Self::ScalarAnf
            | Self::BitslicePrepacked
            | Self::BitsliceIncludingPack => C_ITERATIONS,
            _ => A_B_ITERATIONS,
        }
    }
}

const GROUPS: [&[Arm]; 3] = [
    &[Arm::LutAnd, Arm::AnfAnd],
    &[
        Arm::LutRestrict,
        Arm::AnfRestrict,
        Arm::LutReverse,
        Arm::AnfReverse,
    ],
    &[
        Arm::ScalarLut,
        Arm::ScalarAnf,
        Arm::BitslicePrepacked,
        Arm::BitsliceIncludingPack,
    ],
];

fn fan_masks(fan: &RationalFan, family: &str) -> (FanMask, FanMask) {
    let width = fan.sector_count() as u8;
    let domain = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    match family {
        "sparse" => (
            FanMask::new(width, 1).unwrap(),
            FanMask::new(width, 1_u64 << (width - 1)).unwrap(),
        ),
        "alternating" => (
            FanMask::new(width, 0xaaaa_aaaa_aaaa_aaaa & domain).unwrap(),
            FanMask::new(width, 0xcccc_cccc_cccc_cccc & domain).unwrap(),
        ),
        "pi" => (
            pi_fan_mask(fan, 0).unwrap(),
            pi_fan_mask(fan, width as usize).unwrap(),
        ),
        _ => unreachable!("family is frozen"),
    }
}

/// Direct truth-table substitution without invoking GB-5 ANF methods.
fn lut_restrict(mask: FanMask) -> FanMask {
    let mut result = 0_u64;
    for x in 0..mask.width() {
        if mask.bits() & (1_u64 << (x | 1)) != 0 {
            result |= 1_u64 << x;
        }
    }
    FanMask::new(mask.width(), result).unwrap()
}

/// Direct truth-table address permutation, reversing the variable order.
fn lut_reverse(mask: FanMask) -> FanMask {
    let mut result = 0_u64;
    let n = mask.width().ilog2() as u8;
    for x in 0..mask.width() {
        let mut original = 0_u8;
        for i in 0..n {
            if x & (1_u8 << (n - 1 - i)) != 0 {
                original |= 1_u8 << i;
            }
        }
        if mask.bits() & (1_u64 << original) != 0 {
            result |= 1_u64 << x;
        }
    }
    FanMask::new(mask.width(), result).unwrap()
}

fn scalar_lut_64(mask: FanMask, sectors: &[u8; 64]) -> u64 {
    let mut output = 0_u64;
    for (lane, &sector) in sectors.iter().enumerate() {
        if mask.bits() & (1_u64 << sector) != 0 {
            output |= 1_u64 << lane;
        }
    }
    output
}

fn scalar_anf_64(circuit: AnfCircuit64, sectors: &[u8; 64]) -> u64 {
    let mut output = 0_u64;
    for (lane, &sector) in sectors.iter().enumerate() {
        if circuit.evaluate_sector(sector).unwrap() {
            output |= 1_u64 << lane;
        }
    }
    output
}

/// All paths are checked pointwise / by exact LUT first; timed results
/// are never treated as their own correctness oracle.
fn validate(
    mask: FanMask,
    other: FanMask,
    circuit: AnfCircuit64,
    other_circuit: AnfCircuit64,
    reverse: &[u8],
    sectors: &[u8; 64],
    planes: &[u64; 6],
    active: u64,
) {
    assert_eq!(
        circuit.and(other_circuit).unwrap().truth_mask(),
        mask.intersection(other).unwrap()
    );
    assert_eq!(
        circuit.restrict_variable(0, true).unwrap().truth_mask(),
        lut_restrict(mask)
    );
    assert_eq!(
        circuit.permute_variables(reverse).unwrap().truth_mask(),
        lut_reverse(mask)
    );
    let direct = scalar_lut_64(mask, sectors);
    assert_eq!(scalar_anf_64(circuit, sectors), direct);
    assert_eq!(circuit.evaluate_bitplanes(planes, active), direct);
    let (repacked, repacked_mask) = pack_sectors(sectors, mask.width()).unwrap();
    assert_eq!(circuit.evaluate_bitplanes(&repacked, repacked_mask), direct);
}

/// Repeatedly run one type of operation. A/B output a *new expression* per
/// iteration; C outputs a batch of 64 Boolean results. The timers include
/// neither coefficient conversion nor corpus construction.
#[allow(clippy::too_many_arguments)]
fn measure(
    method: Arm,
    mask: FanMask,
    other: FanMask,
    circuit: AnfCircuit64,
    other_circuit: AnfCircuit64,
    reverse: &[u8],
    sectors: &[u8; 64],
    planes: &[u64; 6],
    active: u64,
) -> (u128, u64) {
    let start = Instant::now();
    let mut checksum = 0_u64;
    for iteration in 0..method.iterations() {
        let result = match method {
            Arm::LutAnd => black_box(
                black_box(mask)
                    .intersection(black_box(other))
                    .unwrap()
                    .bits(),
            ),
            Arm::AnfAnd => black_box(circuit)
                .and(black_box(other_circuit))
                .unwrap()
                .coefficients(),
            Arm::LutRestrict => lut_restrict(black_box(mask)).bits(),
            Arm::AnfRestrict => black_box(circuit)
                .restrict_variable(0, true)
                .unwrap()
                .coefficients(),
            Arm::LutReverse => lut_reverse(black_box(mask)).bits(),
            Arm::AnfReverse => black_box(circuit)
                .permute_variables(black_box(reverse))
                .unwrap()
                .coefficients(),
            Arm::ScalarLut => scalar_lut_64(black_box(mask), black_box(sectors)),
            Arm::ScalarAnf => scalar_anf_64(black_box(circuit), black_box(sectors)),
            Arm::BitslicePrepacked => {
                black_box(circuit).evaluate_bitplanes(black_box(planes), black_box(active))
            }
            Arm::BitsliceIncludingPack => {
                let (created, lanes) =
                    pack_sectors(black_box(sectors), black_box(mask.width())).unwrap();
                black_box(circuit).evaluate_bitplanes(&created, lanes)
            }
        };
        checksum = checksum.wrapping_add(black_box(result).rotate_left(iteration % 64));
    }
    (start.elapsed().as_nanos(), black_box(checksum))
}

fn execute(mode: &str) {
    match mode {
        "deterministic" => {
            println!("width,family,group,method,iterations,checksum,monomials")
        }
        "benchmark" => {
            println!("width,family,group,method,repeat,iterations,elapsed_ns,checksum,monomials")
        }
        _ => panic!("mode must be 'deterministic' or 'benchmark'"),
    }
    for level in 0..=3 {
        let fan = RationalFan::new(level).unwrap();
        let width = fan.sector_count() as u8;
        let sectors: [u8; 64] = std::array::from_fn(|i| i as u8 % width);
        let (planes, active) = pack_sectors(&sectors, width).unwrap();
        let reverse: Vec<u8> = (0..width.ilog2() as u8).rev().collect();
        for family in ["sparse", "alternating", "pi"] {
            let (mask, other) = fan_masks(&fan, family);
            let circuit = AnfCircuit64::from_mask(mask);
            let other_circuit = AnfCircuit64::from_mask(other);
            validate(
                mask,
                other,
                circuit,
                other_circuit,
                &reverse,
                &sectors,
                &planes,
                active,
            );
            for methods in GROUPS {
                let trials = if mode == "deterministic" {
                    1
                } else {
                    WARMUPS + REPEATS
                };
                for repetition in 0..trials {
                    for slot in 0..methods.len() {
                        let method = methods[(slot + repetition) % methods.len()];
                        let (elapsed, checksum) = measure(
                            method,
                            mask,
                            other,
                            circuit,
                            other_circuit,
                            &reverse,
                            &sectors,
                            &planes,
                            active,
                        );
                        if mode == "deterministic" {
                            println!(
                                "{width},{family},{},{},{},{checksum},{}",
                                method.group(),
                                method.name(),
                                method.iterations(),
                                circuit.monomial_count()
                            );
                        } else if repetition >= WARMUPS {
                            println!(
                                "{width},{family},{},{},{},{},{elapsed},{checksum},{}",
                                method.group(),
                                method.name(),
                                repetition - WARMUPS,
                                method.iterations(),
                                circuit.monomial_count()
                            );
                        }
                    }
                }
            }
        }
    }
}

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "deterministic".to_owned());
    execute(&mode);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_frozen_cases_pass_pre_timing_oracles() {
        for level in 0..=3 {
            let fan = RationalFan::new(level).unwrap();
            let width = fan.sector_count() as u8;
            let sectors: [u8; 64] = std::array::from_fn(|i| i as u8 % width);
            let (planes, active) = pack_sectors(&sectors, width).unwrap();
            let reverse: Vec<u8> = (0..width.ilog2() as u8).rev().collect();
            for family in ["sparse", "alternating", "pi"] {
                let (mask, other) = fan_masks(&fan, family);
                validate(
                    mask,
                    other,
                    AnfCircuit64::from_mask(mask),
                    AnfCircuit64::from_mask(other),
                    &reverse,
                    &sectors,
                    &planes,
                    active,
                );
            }
        }
    }
}
