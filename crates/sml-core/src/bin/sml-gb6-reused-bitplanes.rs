//! GB-6: amortized 64-lane bitplane transposition across distinct expert gates.
//! This bench excludes geometric classification and model training by design.
//! Modes: deterministic, benchmark. No SIMD/GPU or SML-25B quality claim.

use std::hint::black_box;
use std::time::Instant;

use sml_core::geometric_anf_circuit::{pack_sectors, AnfCircuit64};
use sml_core::geometric_fan::{pi_fan_mask, FanMask, RationalFan};

const REUSES: [usize; 6] = [1, 2, 4, 8, 16, 32];
const FAMILIES: [&str; 3] = ["sparse", "alternating", "pi"];
const WARMUPS: usize = 3;
const MEASUREMENTS: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pattern {
    Cyclic,
    Clustered,
}

impl Pattern {
    const ALL: [Self; 2] = [Self::Cyclic, Self::Clustered];

    const fn label(self) -> &'static str {
        match self {
            Self::Cyclic => "cyclic",
            Self::Clustered => "clustered",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    ScalarLut,
    BitslicePrepacked,
    BitsliceOnePack,
}

impl Arm {
    const ALL: [Self; 3] = [
        Self::ScalarLut,
        Self::BitslicePrepacked,
        Self::BitsliceOnePack,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::ScalarLut => "scalar_lut",
            Self::BitslicePrepacked => "bitslice_prepacked",
            Self::BitsliceOnePack => "bitslice_one_pack",
        }
    }
}

fn sparse_mask(width: u8, gate: usize) -> u64 {
    if gate < usize::from(width) {
        return 1_u64 << gate;
    }
    let mut remaining = gate - usize::from(width);
    for left in 0..width {
        for right in (left + 1)..width {
            if remaining == 0 {
                return (1_u64 << left) | (1_u64 << right);
            }
            remaining -= 1;
        }
    }
    unreachable!("there are enough unique sparse masks for at most 32 experts")
}

fn generate_bank(fan: &RationalFan, family: &str, count: usize) -> Vec<FanMask> {
    let width = fan.sector_count() as u8;
    let domain = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    (0..count)
        .map(|gate| match family {
            "sparse" => FanMask::new(width, sparse_mask(width, gate)).unwrap(),
            "alternating" => {
                FanMask::new(width, (0xaaaa_aaaa_aaaa_aaaa & domain) ^ gate as u64).unwrap()
            }
            "pi" => pi_fan_mask(fan, gate * usize::from(width)).unwrap(),
            _ => unreachable!("frozen family"),
        })
        .collect()
}

fn queries(pattern: Pattern, width: u8) -> [u8; 64] {
    std::array::from_fn(|i| {
        let address = match pattern {
            Pattern::Cyclic => i,
            Pattern::Clustered => i / 4,
        };
        (address % usize::from(width)) as u8
    })
}

fn scalar_lut(mask: FanMask, addresses: &[u8; 64]) -> u64 {
    let bits = mask.bits();
    let mut output = 0_u64;
    for (lane, &address) in addresses.iter().enumerate() {
        output |= ((bits >> address) & 1) << lane;
    }
    output
}

fn oracle(masks: &[FanMask], addresses: &[u8; 64]) -> Vec<u64> {
    masks
        .iter()
        .map(|&mask| scalar_lut(mask, addresses))
        .collect()
}

fn run_one(
    arm: Arm,
    masks: &[FanMask],
    circuits: &[AnfCircuit64],
    addresses: &[u8; 64],
    prepacked: &[u64; 6],
    active: u64,
    output: &mut [u64],
) {
    match arm {
        Arm::ScalarLut => {
            for (slot, &mask) in masks.iter().enumerate() {
                output[slot] = scalar_lut(black_box(mask), black_box(addresses));
            }
        }
        Arm::BitslicePrepacked => {
            for (slot, &circuit) in circuits.iter().enumerate() {
                output[slot] =
                    black_box(circuit).evaluate_bitplanes(black_box(prepacked), black_box(active));
            }
        }
        Arm::BitsliceOnePack => {
            let (planes, mask) = pack_sectors(black_box(addresses), masks[0].width()).unwrap();
            for (slot, &circuit) in circuits.iter().enumerate() {
                output[slot] =
                    black_box(circuit).evaluate_bitplanes(black_box(&planes), black_box(mask));
            }
        }
    }
}

fn output_checksum(output: &[u64]) -> u64 {
    output
        .iter()
        .enumerate()
        .fold(0_u64, |sum, (i, &bits)| {
            sum.wrapping_add(bits.rotate_left(i as u32))
        })
}

#[allow(clippy::too_many_arguments)]
fn measure(
    arm: Arm,
    masks: &[FanMask],
    circuits: &[AnfCircuit64],
    addresses: &[u8; 64],
    prepacked: &[u64; 6],
    active: u64,
    iterations: usize,
    expected: &[u64],
) -> (u128, u64) {
    let mut output = vec![0_u64; masks.len()];
    let start = Instant::now();
    for _ in 0..iterations {
        run_one(
            black_box(arm),
            black_box(masks),
            black_box(circuits),
            black_box(addresses),
            black_box(prepacked),
            black_box(active),
            black_box(&mut output),
        );
    }
    let elapsed_ns = start.elapsed().as_nanos();
    assert_eq!(output, expected, "timed method lost exact output parity");
    (elapsed_ns, black_box(output_checksum(&output)))
}

fn execute(mode: &str) {
    match mode {
        "deterministic" => {
            println!("width,family,pattern,bank,method,iterations,checksum")
        }
        "benchmark" => {
            println!("width,family,pattern,bank,method,repeat,iterations,elapsed_ns,checksum")
        }
        _ => panic!("mode must be deterministic or benchmark"),
    }
    for level in 0..=3 {
        let fan = RationalFan::new(level).unwrap();
        let width = fan.sector_count() as u8;
        for family in FAMILIES {
            let bank = generate_bank(&fan, family, 32);
            for pattern in Pattern::ALL {
                let addresses = queries(pattern, width);
                let (planes, active) = pack_sectors(&addresses, width).unwrap();
                for reuse in REUSES {
                    let masks = &bank[..reuse];
                    let circuits: Vec<_> = masks
                        .iter()
                        .copied()
                        .map(AnfCircuit64::from_mask)
                        .collect();
                    let expected = oracle(masks, &addresses);
                    let checksum = output_checksum(&expected);
                    for arm in Arm::ALL {
                        let mut result = vec![0_u64; reuse];
                        run_one(
                            arm,
                            masks,
                            &circuits,
                            &addresses,
                            &planes,
                            active,
                            &mut result,
                        );
                        assert_eq!(result, expected, "pre-timing semantic divergence");
                    }
                    let iterations = 4096 / reuse;
                    let trials = if mode == "deterministic" {
                        1
                    } else {
                        WARMUPS + MEASUREMENTS
                    };
                    for trial in 0..trials {
                        for order in 0..3 {
                            let arm = Arm::ALL[(order + trial) % 3];
                            let (elapsed, actual_checksum) = measure(
                                arm,
                                masks,
                                &circuits,
                                &addresses,
                                &planes,
                                active,
                                iterations,
                                &expected,
                            );
                            assert_eq!(actual_checksum, checksum);
                            if mode == "deterministic" {
                                println!(
                                    "{width},{family},{},{reuse},{},{iterations},{checksum}",
                                    pattern.label(),
                                    arm.label()
                                );
                            } else if trial >= WARMUPS {
                                println!(
                                    "{width},{family},{},{reuse},{},{},{iterations},{elapsed},{checksum}",
                                    pattern.label(),
                                    arm.label(),
                                    trial - WARMUPS
                                );
                            }
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
    fn all_pre_registered_banks_and_patterns_match_exact_oracle() {
        for level in 0..=3 {
            let fan = RationalFan::new(level).unwrap();
            let width = fan.sector_count() as u8;
            for family in FAMILIES {
                let bank = generate_bank(&fan, family, 32);
                assert_eq!(bank.len(), 32);
                for index in 0..bank.len() {
                    for previous in &bank[..index] {
                        assert_ne!(*previous, bank[index], "expert masks must be distinct");
                    }
                }
                for pattern in Pattern::ALL {
                    let addresses = queries(pattern, width);
                    let (planes, active) = pack_sectors(&addresses, width).unwrap();
                    for reuse in REUSES {
                        let masks = &bank[..reuse];
                        let circuits: Vec<_> = masks
                            .iter()
                            .copied()
                            .map(AnfCircuit64::from_mask)
                            .collect();
                        let expected = oracle(masks, &addresses);
                        for arm in Arm::ALL {
                            let mut actual = vec![0_u64; reuse];
                            run_one(
                                arm, masks, &circuits, &addresses, &planes, active, &mut actual,
                            );
                            assert_eq!(actual, expected);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn frozen_reuse_iterations_equalize_gate_application_counts() {
        for reuse in REUSES {
            assert_eq!(reuse * (4096 / reuse), 4096);
        }
    }
}
