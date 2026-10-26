//! SKY-R4: Exact finite-horizon GF(2) decoding vs genuine streaming recall.
//!
//! No physical skyrmions, no learned readout, no inference integration.
//! A full-rank 75-input controllability map is inverted algebraically,
//! then the decoder is evaluated both after reset and on a never-reset
//! input stream. The 75-bit Shift75 is the equal-logical-bits control.

const WIDTH: usize = 75;
const MASK: u128 = (1_u128 << WIDTH) - 1;
const DELAYS: [usize; 5] = [1, 7, 31, 63, 74];
const EPISODES: usize = 512;
const STREAM_STEPS: usize = 4096;
const SEEDS: [u64; 4] = [
    0x0d3f_41ab_7901_2357,
    0x8a3d_51b2_9823_4671,
    0x2e1b_7485_a2c9_f301,
    0x5e34_7b80_9c2a_671d,
];

#[derive(Clone, Copy)]
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_bit(&mut self) -> u8 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut mixed = self.0;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        mixed ^= mixed >> 31;
        ((mixed >> 32) & 1) as u8
    }
}

fn rule90_without_input(state: u128) -> u128 {
    let left = ((state << 1) & MASK) | (state >> (WIDTH - 1));
    let right = (state >> 1) | ((state & 1) << (WIDTH - 1));
    left ^ right
}

fn shift75(state: u128, bit: u8) -> u128 {
    ((state << 1) & MASK) | u128::from(bit)
}

fn alternate_rule90(state: u128, bit: u8, time: usize) -> u128 {
    rule90_without_input(state) ^ (u128::from(bit) << (time % 2))
}

fn rank(columns: &[u128]) -> usize {
    let mut pivot = [0_u128; WIDTH];
    let mut total = 0;
    for column in columns {
        let mut value = *column;
        for index in (0..WIDTH).rev() {
            if (value >> index) & 1 == 0 {
                continue;
            }
            if pivot[index] == 0 {
                pivot[index] = value;
                total += 1;
                break;
            }
            value ^= pivot[index];
        }
    }
    total
}

/// Columns by input age: column[0] is current input's effect.
/// Observation time parity selects which site each age used.
fn impulse_columns(time_parity: usize) -> [u128; WIDTH] {
    assert!(time_parity < 2);
    let mut zero = 1_u128;
    let mut one = 2_u128;
    let mut cols = [0_u128; WIDTH];
    for (age, column) in cols.iter_mut().enumerate() {
        let site = (time_parity + age) % 2;
        *column = if site == 0 { zero } else { one };
        zero = rule90_without_input(zero);
        one = rule90_without_input(one);
    }
    cols
}

#[derive(Clone, Copy)]
struct Decoder {
    /// Linear readout weights for the 75 input ages (one u128 each).
    rows: [u128; WIDTH],
}

impl Decoder {
    fn from_columns(columns: &[u128; WIDTH]) -> Option<Self> {
        // 75x75 over GF(2), rows indexed by output state bit,
        // columns indexed by input age. Gauss-Jordan produces inverse rows.
        let mut left = [0_u128; WIDTH];
        let mut right = [0_u128; WIDTH];
        for state_bit in 0..WIDTH {
            for (age, column) in columns.iter().enumerate() {
                if (column >> state_bit) & 1 != 0 {
                    left[state_bit] |= 1_u128 << age;
                }
            }
            right[state_bit] = 1_u128 << state_bit;
        }

        for age in 0..WIDTH {
            let pivot = (age..WIDTH).find(|row| ((left[*row] >> age) & 1) != 0)?;
            left.swap(age, pivot);
            right.swap(age, pivot);
            for row in 0..WIDTH {
                if row != age && ((left[row] >> age) & 1) != 0 {
                    left[row] ^= left[age];
                    right[row] ^= right[age];
                }
            }
        }
        for (age, row) in left.iter().enumerate() {
            assert_eq!(*row, 1_u128 << age);
        }
        Some(Self { rows: right })
    }

    fn read(self, state: u128, age: usize) -> u8 {
        (self.rows[age] & state).count_ones() as u8 & 1
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Outcome {
    correct: usize,
    total: usize,
}

impl Outcome {
    fn observe(&mut self, expected: u8, actual: u8) {
        self.correct += usize::from(expected == actual);
        self.total += 1;
    }
}

/// One reset every 75 inputs: finite-horizon, *known zero initial state*.
fn reset_episodes(is_shift: bool, delay: usize, decoder: Decoder) -> Outcome {
    let mut outcome = Outcome::default();
    for seed in SEEDS {
        let mut signal = SplitMix64(seed);
        for _ in 0..EPISODES {
            let mut state = 0_u128;
            let mut newest = [0_u8; WIDTH];
            for time in 0..WIDTH {
                let bit = signal.next_bit();
                newest.rotate_right(1);
                newest[0] = bit;
                state = if is_shift {
                    shift75(state, bit)
                } else {
                    alternate_rule90(state, bit, time)
                };
            }
            let read = if is_shift {
                ((state >> delay) & 1) as u8
            } else {
                decoder.read(state, delay)
            };
            outcome.observe(newest[delay], read);
        }
    }
    outcome
}

/// No reset: includes unknown old input histories beyond 75 steps.
fn continuous_stream(is_shift: bool, delay: usize, decoders: [Decoder; 2]) -> Outcome {
    let mut outcome = Outcome::default();
    for seed in SEEDS {
        let mut signal = SplitMix64(seed);
        let mut state = 0_u128;
        let mut newest = [0_u8; WIDTH];
        for time in 0..STREAM_STEPS {
            let bit = signal.next_bit();
            newest.rotate_right(1);
            newest[0] = bit;
            state = if is_shift {
                shift75(state, bit)
            } else {
                alternate_rule90(state, bit, time)
            };
            if time >= WIDTH {
                let read = if is_shift {
                    ((state >> delay) & 1) as u8
                } else {
                    decoders[time % 2].read(state, delay)
                };
                outcome.observe(newest[delay], read);
            }
        }
    }
    outcome
}

fn old_state_persistence_rank() -> usize {
    let mut aged: Vec<u128> = (0..WIDTH).map(|position| 1_u128 << position).collect();
    for _ in 0..WIDTH {
        for vector in &mut aged {
            *vector = rule90_without_input(*vector);
        }
    }
    rank(&aged)
}

fn main() {
    let even = Decoder::from_columns(&impulse_columns(0)).expect("full-rank even input map");
    let odd = Decoder::from_columns(&impulse_columns(1)).expect("full-rank odd input map");
    println!("SKY-R4: exact reset-vs-stream readouts; not skyrmion physics");
    println!("arm,scenario,age,logical_bits,correct,total,old_state_rank_after_75");
    for delay in DELAYS {
        for (name, is_shift) in [("SHIFT75", true), ("ALT_RULE90", false)] {
            let finite = reset_episodes(is_shift, delay, even);
            let stream = continuous_stream(is_shift, delay, [even, odd]);
            let old_rank = if is_shift {
                0
            } else {
                old_state_persistence_rank()
            };
            println!(
                "{name},RESET_EVERY_75,{delay},{WIDTH},{},{},{}",
                finite.correct, finite.total, old_rank
            );
            println!(
                "{name},STREAM_NO_RESET,{delay},{WIDTH},{},{},{}",
                stream.correct, stream.total, old_rank
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_window_alternating_input_is_full_rank_and_exact() {
        let columns = impulse_columns(0);
        assert_eq!(rank(&columns), WIDTH);
        let decoder = Decoder::from_columns(&columns).unwrap();
        for age in DELAYS {
            let score = reset_episodes(false, age, decoder);
            assert_eq!(score.correct, score.total, "age={age}");
        }
    }

    #[test]
    fn both_alternation_phases_invert_exactly() {
        for phase in 0..2 {
            let columns = impulse_columns(phase);
            assert_eq!(rank(&columns), WIDTH);
            let decoder = Decoder::from_columns(&columns).unwrap();
            for (age, column) in columns.iter().enumerate() {
                for requested in 0..WIDTH {
                    assert_eq!(decoder.read(*column, requested), u8::from(age == requested));
                }
            }
        }
    }

    #[test]
    fn streaming_shift75_is_perfect_but_alt_rule90_is_not() {
        let even = Decoder::from_columns(&impulse_columns(0)).unwrap();
        let odd = Decoder::from_columns(&impulse_columns(1)).unwrap();
        for age in DELAYS {
            let shift = continuous_stream(true, age, [even, odd]);
            assert_eq!(shift.correct, shift.total);
            let alternate = continuous_stream(false, age, [even, odd]);
            assert!(alternate.correct < alternate.total, "age={age}");
        }
    }

    #[test]
    fn old_state_dynamics_retains_74_independent_directions() {
        assert_eq!(old_state_persistence_rank(), WIDTH - 1);
    }

    #[test]
    fn zero_state_single_pulse_matches_controllability_columns() {
        for phase in 0..2 {
            let columns = impulse_columns(phase);
            for age in 0..WIDTH {
                let mut state = 0_u128;
                for time in 0..=age {
                    let bit = u8::from(time == 0);
                    // To obtain observation phase 'phase' after 'age'
                    // steps, initial time parity is (phase + age)%2.
                    let site_parity = (phase + age + time) % 2;
                    state = rule90_without_input(state) ^ (u128::from(bit) << site_parity);
                }
                assert_eq!(state, columns[age]);
            }
        }
    }
}
