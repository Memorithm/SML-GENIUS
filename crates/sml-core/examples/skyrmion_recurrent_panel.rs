//! SKY-R1: preregistered local-Boolean recurrent-memory ablation.
//!
//! This is explicitly NOT a simulation of skyrmion dynamics. It tests whether
//! neighborhood-only Boolean recurrences or charge-preserving static sectors
//! improve held-out recall over a 75-bit shift register at identical logical
//! state-bit budgets and identical single-bit probe training procedures.
//! All model states are byte-backed reference arrays, not packed SBG storage.

#[allow(dead_code)]
#[path = "skyrmion_octahedron.rs"]
mod topology;

const WIDTH: usize = 75;
const WARMUP: usize = WIDTH;
const TRAIN_STEPS: usize = 1024;
const TEST_STEPS: usize = 2048;
const TRAIN_SEEDS: [u64; 2] = [0x1234_5678_9abc_def1, 0x7182_93a4_b5c6_d7e8];
const TEST_SEEDS: [u64; 4] = [
    0x3141_5926_5358_9793,
    0x2718_2818_2845_9045,
    0x1618_0339_8874_9895,
    0x1123_5813_2134_5589,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    Shift75,
    Rule90,
    Rule30,
    LastOnly,
    FixedSector,
}

impl Arm {
    const ALL: [Self; 5] = [
        Self::Shift75,
        Self::Rule90,
        Self::Rule30,
        Self::LastOnly,
        Self::FixedSector,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Shift75 => "SHIFT75",
            Self::Rule90 => "LOCAL_RULE90",
            Self::Rule30 => "LOCAL_RULE30",
            Self::LastOnly => "LAST_ONLY",
            Self::FixedSector => "FIXED_Q_SECTOR",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
    Copy1,
    Delay7,
    Delay31,
    Delay63,
    Context1Or7,
}

impl Task {
    const ALL: [Self; 5] = [
        Self::Copy1,
        Self::Delay7,
        Self::Delay31,
        Self::Delay63,
        Self::Context1Or7,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Copy1 => "COPY_1",
            Self::Delay7 => "DELAY_7",
            Self::Delay31 => "DELAY_31",
            Self::Delay63 => "DELAY_63",
            Self::Context1Or7 => "CTX_1_OR_7",
        }
    }

    fn target(self, recent_inputs: &[u8; WIDTH], context: u8) -> u8 {
        match self {
            Self::Copy1 => recent_inputs[1],
            Self::Delay7 => recent_inputs[7],
            Self::Delay31 => recent_inputs[31],
            Self::Delay63 => recent_inputs[63],
            Self::Context1Or7 => {
                if context == 0 {
                    recent_inputs[1]
                } else {
                    recent_inputs[7]
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct DeterministicBits(u64);

impl DeterministicBits {
    fn next_bit(&mut self) -> u8 {
        // SplitMix64: nonlinear 64-bit mixing rather than GF(2)-linear
        // xorshift outputs, to avoid a trivial parity-readout shortcut.
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut mixed = self.0;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        mixed ^= mixed >> 31;
        ((mixed >> 32) & 1) as u8
    }
}

struct State {
    arm: Arm,
    bits: [u8; WIDTH],
    rejections: usize,
}

impl State {
    fn new(arm: Arm) -> Self {
        let mut bits = [0; WIDTH];
        if arm == Arm::FixedSector {
            let initial = topology::encoded_reference_texture(false);
            bits.copy_from_slice(&initial);
        }
        Self {
            arm,
            bits,
            rejections: 0,
        }
    }

    fn ingest(&mut self, input: u8) {
        assert!(input <= 1);
        match self.arm {
            Arm::Shift75 => {
                self.bits.rotate_right(1);
                self.bits[0] = input;
            }
            Arm::Rule90 | Arm::Rule30 => {
                let previous = self.bits;
                for (index, result) in self.bits.iter_mut().enumerate() {
                    let left = previous[(index + WIDTH - 1) % WIDTH];
                    let center = previous[index];
                    let right = previous[(index + 1) % WIDTH];
                    let updated = match self.arm {
                        Arm::Rule90 => left ^ right,
                        Arm::Rule30 => left ^ (center | right),
                        _ => unreachable!("frozen cellular control"),
                    };
                    *result = updated ^ if index == 0 { input } else { 0 };
                }
            }
            Arm::LastOnly => {
                self.bits = [0; WIDTH];
                self.bits[0] = input;
            }
            Arm::FixedSector => {
                let candidate = topology::encoded_reference_texture(input == 1);
                let before = topology::decode_reference_texture(&self.bits)
                    .expect("known admissible reference texture");
                let after = topology::decode_reference_texture(&candidate)
                    .expect("known admissible reference texture");
                if before == after {
                    self.bits.copy_from_slice(&candidate);
                } else {
                    self.rejections += 1;
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Sample {
    features: [u8; WIDTH],
    target: u8,
    context: u8,
}

struct Episode {
    samples: Vec<Sample>,
    rejections: usize,
}

fn episode(arm: Arm, task: Task, seed: u64, steps: usize) -> Episode {
    let mut signal = DeterministicBits(seed);
    let mut contexts = DeterministicBits(seed ^ 0xd1b5_4a32_d192_ed03);
    let mut recent = [0_u8; WIDTH];
    let mut state = State::new(arm);
    let mut samples = Vec::with_capacity(steps - WARMUP);
    for step in 0..steps {
        let input = signal.next_bit();
        let context = contexts.next_bit();
        recent.rotate_right(1);
        recent[0] = input;
        state.ingest(input);
        if step >= WARMUP {
            samples.push(Sample {
                features: state.bits,
                target: task.target(&recent, context),
                context,
            });
        }
    }
    Episode {
        samples,
        rejections: state.rejections,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Probe {
    index: usize,
    invert: bool,
}

impl Probe {
    fn predict(self, features: &[u8; WIDTH]) -> u8 {
        features[self.index] ^ u8::from(self.invert)
    }
}

fn train_probe(samples: &[Sample], selected_context: Option<u8>) -> Probe {
    let mut best = Probe {
        index: 0,
        invert: false,
    };
    let mut best_correct = 0;
    for index in 0..WIDTH {
        for invert in [false, true] {
            let proposal = Probe { index, invert };
            let correct = samples
                .iter()
                .filter(|sample| {
                    (selected_context.is_none() || selected_context == Some(sample.context))
                        && proposal.predict(&sample.features) == sample.target
                })
                .count();
            if correct > best_correct {
                best_correct = correct;
                best = proposal;
            }
        }
    }
    best
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Readout {
    if_zero: Probe,
    if_one: Probe,
}

impl Readout {
    fn train(task: Task, samples: &[Sample]) -> Self {
        let if_zero = train_probe(
            samples,
            if task == Task::Context1Or7 {
                Some(0)
            } else {
                None
            },
        );
        let if_one = if task == Task::Context1Or7 {
            train_probe(samples, Some(1))
        } else {
            if_zero
        };
        Self { if_zero, if_one }
    }

    fn predict(self, sample: &Sample) -> u8 {
        let probe = if sample.context == 0 {
            self.if_zero
        } else {
            self.if_one
        };
        probe.predict(&sample.features)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResultRow {
    arm: Arm,
    task: Task,
    train_correct: usize,
    train_total: usize,
    test_correct: usize,
    test_total: usize,
    rejected_writes: usize,
    readout: Readout,
}

fn count_correct(samples: &[Sample], readout: Readout) -> usize {
    samples
        .iter()
        .filter(|sample| readout.predict(sample) == sample.target)
        .count()
}

fn evaluate(arm: Arm, task: Task) -> ResultRow {
    let mut training = Vec::new();
    for seed in TRAIN_SEEDS {
        training.extend(episode(arm, task, seed, TRAIN_STEPS).samples);
    }
    let readout = Readout::train(task, &training);
    let mut test_correct = 0;
    let mut test_total = 0;
    let mut rejected_writes = 0;
    for seed in TEST_SEEDS {
        let heldout = episode(arm, task, seed, TEST_STEPS);
        test_correct += count_correct(&heldout.samples, readout);
        test_total += heldout.samples.len();
        rejected_writes += heldout.rejections;
    }
    ResultRow {
        arm,
        task,
        train_correct: count_correct(&training, readout),
        train_total: training.len(),
        test_correct,
        test_total,
        rejected_writes,
        readout,
    }
}

fn main() {
    println!("SKY-R1: binary local CA ablations; NOT magnetic or trained SML inference");
    println!(
        "task,arm,logical_bits,train_correct,train_total,test_correct,test_total,probe_zero,probe_one,rejected_writes"
    );
    for task in Task::ALL {
        for arm in Arm::ALL {
            let row = evaluate(arm, task);
            println!(
                "{},{},{WIDTH},{},{},{},{},{},{},{}",
                row.task.name(),
                row.arm.name(),
                row.train_correct,
                row.train_total,
                row.test_correct,
                row.test_total,
                row.readout.if_zero.index,
                row.readout.if_one.index,
                row.rejected_writes,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matched_logical_bits_and_independent_train_test_seeds() {
        assert_eq!(WIDTH, 75);
        assert_eq!(WARMUP, 75);
        for seed in TRAIN_SEEDS {
            assert!(!TEST_SEEDS.contains(&seed));
        }
        for arm in Arm::ALL {
            let mut state = State::new(arm);
            for value in [1, 0, 1, 1, 0] {
                state.ingest(value);
                assert!(state.bits.iter().all(|bit| *bit <= 1));
            }
        }
    }

    #[test]
    fn shift_register_is_exact_on_all_frozen_recall_tasks() {
        for task in Task::ALL {
            let row = evaluate(Arm::Shift75, task);
            assert_eq!(row.train_correct, row.train_total, "{task:?}");
            assert_eq!(row.test_correct, row.test_total, "{task:?}");
        }
    }

    #[test]
    fn a_protected_charge_sector_cannot_encode_an_alternating_bit_stream() {
        let mut state = State::new(Arm::FixedSector);
        for input in [0, 1, 0, 1, 1, 0, 1] {
            state.ingest(input);
            assert_eq!(topology::decode_reference_texture(&state.bits), Some(0));
        }
        assert_eq!(state.rejections, 4);
        assert_eq!(topology::decode_reference_texture(&state.bits), Some(0));
    }

    #[test]
    fn cellular_steps_really_use_local_neighbors() {
        let mut rule90 = State::new(Arm::Rule90);
        rule90.ingest(1);
        assert_eq!(rule90.bits[0], 1);
        rule90.ingest(0);
        assert_eq!(rule90.bits[1], 1);
        assert_eq!(rule90.bits[WIDTH - 1], 1);
        assert_eq!(rule90.bits[0], 0);

        let mut rule30 = State::new(Arm::Rule30);
        rule30.ingest(1);
        rule30.ingest(0);
        assert_eq!(rule30.bits[1], 1);
        assert_eq!(rule30.bits[WIDTH - 1], 1);
    }

    #[test]
    fn test_episodes_have_frozen_sample_counts() {
        let row = evaluate(Arm::Rule90, Task::Delay7);
        assert_eq!(row.train_total, TRAIN_SEEDS.len() * (TRAIN_STEPS - WARMUP));
        assert_eq!(row.test_total, TEST_SEEDS.len() * (TEST_STEPS - WARMUP));
        assert!(row.test_correct <= row.test_total);
    }

    #[test]
    fn heldout_rows_are_deterministically_reproducible() {
        assert_eq!(
            evaluate(Arm::Rule30, Task::Context1Or7),
            evaluate(Arm::Rule30, Task::Context1Or7)
        );
    }
}
