//! SKY-R3 — Exact GF(2) controllability rank of two 75-bit local memories.
//!
//! An algebraic structural bound, not a trained neural decoder, physical
//! spintronics simulation, measured memory throughput, or topological proof.
//! Models mirror the Shift75 and Rule90 recurrences in SKY-R1.

const WIDTH: usize = 75;
const MASK: u128 = (1_u128 << WIDTH) - 1;

#[derive(Clone, Copy, Debug)]
enum Arm {
    Shift75,
    PeriodicRule90,
}

impl Arm {
    const ALL: [Self; 2] = [Self::Shift75, Self::PeriodicRule90];

    const fn name(self) -> &'static str {
        match self {
            Self::Shift75 => "SHIFT75",
            Self::PeriodicRule90 => "PERIODIC_RULE90",
        }
    }

    fn step(self, state: u128, input: u8) -> u128 {
        assert!(input <= 1);
        assert_eq!(state & !MASK, 0);
        match self {
            Self::Shift75 => ((state << 1) & MASK) | u128::from(input),
            Self::PeriodicRule90 => {
                // Neighbors of site i are i-1 and i+1, wrap on a ring.
                let left = ((state << 1) & MASK) | (state >> (WIDTH - 1));
                let right = (state >> 1) | ((state & 1) << (WIDTH - 1));
                left ^ right ^ u128::from(input)
            }
        }
    }
}

fn impulse_columns(arm: Arm, terms: usize) -> Vec<u128> {
    let mut state = 0;
    let mut columns = Vec::with_capacity(terms);
    for step in 0..terms {
        state = arm.step(state, u8::from(step == 0));
        columns.push(state);
    }
    columns
}

/// Exact binary Gaussian elimination on 75-bit packed vectors.
/// The rank is independent of order and uses no floating-point tolerance.
fn rank(vectors: &[u128]) -> usize {
    let mut pivots = [0_u128; WIDTH];
    let mut dimension = 0;
    for vector in vectors {
        let mut row = *vector;
        for position in (0..WIDTH).rev() {
            if (row >> position) & 1 == 0 {
                continue;
            }
            if pivots[position] == 0 {
                pivots[position] = row;
                dimension += 1;
                break;
            }
            row ^= pivots[position];
        }
    }
    dimension
}

/// Reflection i -> -i mod 75. A central injection and rule90 commute with
/// this reflection, so reachable impulse fields must be reflection symmetric.
fn reflected(state: u128) -> u128 {
    let mut mirror = 0_u128;
    for position in 0..WIDTH {
        if (state >> position) & 1 != 0 {
            mirror |= 1_u128 << ((WIDTH - position) % WIDTH);
        }
    }
    mirror
}

/// Impulse columns for an input injected at an arbitrary ring site.
/// Each column is the state after that input has aged a given number
/// of steps; the subsequent evolution is unforced Rule90.
fn site_impulse_columns(site: usize, terms: usize) -> Vec<u128> {
    assert!(site < WIDTH);
    let mut state = 1_u128 << site;
    let mut columns = Vec::with_capacity(terms);
    for _ in 0..terms {
        columns.push(state);
        state = Arm::PeriodicRule90.step(state, 0);
    }
    columns
}

/// Two independently controllable input channels at neighboring sites.
/// Each time step contributes one column per independent input channel.
fn two_channel_rank(terms: usize) -> usize {
    let first = site_impulse_columns(0, terms);
    let second = site_impulse_columns(1, terms);
    let mut combined = Vec::with_capacity(terms * 2);
    for age in 0..terms {
        combined.push(first[age]);
        combined.push(second[age]);
    }
    rank(&combined)
}

/// Same physical two injection sites, but one tied Boolean input channel.
/// Its impulse at each age is the XOR of the two one-site responses.
fn tied_two_site_rank(terms: usize) -> usize {
    let first = site_impulse_columns(0, terms);
    let second = site_impulse_columns(1, terms);
    let combined: Vec<u128> = first
        .iter()
        .zip(&second)
        .map(|(left, right)| left ^ right)
        .collect();
    rank(&combined)
}

/// One Boolean input per step, routed alternately to site 0 and site 1.
/// At a fixed observation horizon, impulse age determines the injection
/// site up to an overall time-origin parity. Routing is a predefined clock
/// schedule; there is no second independent Boolean data input.
fn alternating_one_channel_rank(terms: usize, swap: bool) -> usize {
    let first = site_impulse_columns(0, terms);
    let second = site_impulse_columns(1, terms);
    let columns: Vec<u128> = (0..terms)
        .map(|age| {
            if (age % 2 == 0) ^ swap {
                first[age]
            } else {
                second[age]
            }
        })
        .collect();
    rank(&columns)
}

fn main() {
    println!("SKY-R3 exact GF2 impulse ranks; not a skyrmion device");
    println!("arm,taps,controllability_rank,reflection_symmetric");
    for arm in Arm::ALL {
        let columns = impulse_columns(arm, WIDTH + 25);
        for taps in [1_usize, 7, 31, 38, 39, 63, 75, 100] {
            let symmetric = columns[..taps]
                .iter()
                .all(|column| reflected(*column) == *column);
            println!(
                "{},{taps},{},{}",
                arm.name(),
                rank(&columns[..taps]),
                symmetric
            );
        }
    }
    println!("injection_scheme,taps,controllability_rank,input_channels");
    for taps in [1_usize, 7, 31, 38, 39, 63, 75, 100] {
        println!("TWO_INDEPENDENT_SITES,{taps},{},2", two_channel_rank(taps));
        println!("TWO_TIED_SITES,{taps},{},1", tied_two_site_rank(taps));
        println!(
            "ALTERNATING_ONE_CHANNEL,{taps},{},1",
            alternating_one_channel_rank(taps, false)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_impulse_rank_saturates_at_38_for_periodic_rule90() {
        let columns = impulse_columns(Arm::PeriodicRule90, 100);
        for count in 1..=38 {
            assert_eq!(rank(&columns[..count]), count);
        }
        for count in 39..=100 {
            assert_eq!(rank(&columns[..count]), 38);
        }
    }

    #[test]
    fn shift75_controllability_rank_reaches_75() {
        let columns = impulse_columns(Arm::Shift75, 100);
        for count in 1..=75 {
            assert_eq!(rank(&columns[..count]), count);
        }
        assert_eq!(rank(&columns), 75);
    }

    #[test]
    fn rule90_reachable_fields_obey_reflection_symmetry() {
        let columns = impulse_columns(Arm::PeriodicRule90, 100);
        assert!(columns.iter().all(|column| reflected(*column) == *column));
        assert_eq!(rank(&columns), WIDTH.div_ceil(2));
    }

    #[test]
    fn rule90_step_matches_unpacked_local_neighbors() {
        let mut state = 0_u128;
        let mut unpacked = [0_u8; WIDTH];
        for step in 0..150 {
            let input = u8::from(step % 3 == 0);
            state = Arm::PeriodicRule90.step(state, input);
            let previous = unpacked;
            for (index, value) in unpacked.iter_mut().enumerate() {
                *value = previous[(index + WIDTH - 1) % WIDTH]
                    ^ previous[(index + 1) % WIDTH]
                    ^ if index == 0 { input } else { 0 };
            }
            for (index, value) in unpacked.iter().enumerate() {
                assert_eq!(((state >> index) & 1) as u8, *value);
            }
        }
    }

    #[test]
    fn two_independent_neighbor_injections_reach_full_rank() {
        for count in 1..=37 {
            assert_eq!(two_channel_rank(count), 2 * count);
        }
        assert_eq!(two_channel_rank(38), WIDTH);
        for count in 39..=100 {
            assert_eq!(two_channel_rank(count), WIDTH);
        }
    }

    #[test]
    fn tied_two_site_injection_does_not_reach_full_rank() {
        assert_eq!(tied_two_site_rank(38), 37);
        assert_eq!(tied_two_site_rank(75), 37);
        assert_eq!(tied_two_site_rank(100), 37);
    }

    #[test]
    fn alternating_single_data_channel_is_full_rank() {
        for swapped in [false, true] {
            for count in 1..=WIDTH {
                assert_eq!(
                    alternating_one_channel_rank(count, swapped),
                    count,
                    "count={count}, swapped={swapped}"
                );
            }
            assert_eq!(alternating_one_channel_rank(100, swapped), WIDTH);
        }
    }

    #[test]
    fn alternating_impulse_columns_equal_sequential_state_updates() {
        let steps = WIDTH;
        let mut state = 0_u128;
        let mut impulse_sum = 0_u128;
        let mut source_bits = 0x9e37_79b9_7f4a_7c15_u64;
        let mut inputs = [0_u8; WIDTH];
        for input in &mut inputs {
            source_bits ^= source_bits >> 12;
            source_bits ^= source_bits << 25;
            source_bits ^= source_bits >> 27;
            *input = (source_bits & 1) as u8;
        }
        for (step, input) in inputs.into_iter().enumerate() {
            state = Arm::PeriodicRule90.step(state, 0) ^ (u128::from(input) << (step % 2));
        }

        let first = site_impulse_columns(0, steps);
        let second = site_impulse_columns(1, steps);
        for (time, input) in inputs.into_iter().enumerate() {
            if input == 1 {
                let age = steps - 1 - time;
                impulse_sum ^= if time % 2 == 0 {
                    first[age]
                } else {
                    second[age]
                };
            }
        }
        assert_eq!(state, impulse_sum);
    }

    #[test]
    fn zero_site_impulses_match_original_rule90_model() {
        assert_eq!(
            site_impulse_columns(0, 100),
            impulse_columns(Arm::PeriodicRule90, 100)
        );
    }

    #[test]
    fn independent_binary_vector_rank_checks() {
        assert_eq!(rank(&[0, 0]), 0);
        assert_eq!(rank(&[1, 1, 1]), 1);
        assert_eq!(rank(&[1, 2, 3]), 2);
        assert_eq!(rank(&[1, 2, 4, 7]), 3);
    }
}
