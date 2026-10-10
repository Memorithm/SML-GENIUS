//! Deterministic toy sequence tasks with known answers (A5 learning prototype).
//!
//! Every task maps a stream of input bits `x_0, x_1, ...` to one target bit per
//! step. Inputs before the start of a sequence are defined as `0`, which matches
//! the all-zero initial recurrent state used by the learners. Data is generated
//! from a seeded SplitMix64 stream, so a seed fully determines every split.

use core::fmt;

/// Small deterministic pseudo-random generator (SplitMix64).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    pub fn next_bool(&mut self) -> bool {
        self.next_u64() >> 63 == 1
    }
    /// Uniform integer in `0..bound` (`bound > 0`), by rejection.
    pub fn below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "bound must be positive");
        let bound = bound as u64;
        let zone = u64::MAX - u64::MAX % bound;
        loop {
            let value = self.next_u64();
            if value < zone {
                return (value % bound) as usize;
            }
        }
    }
    /// Uniform `f32` in `[-1, 1)`.
    pub fn next_signed_unit(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
    }
}

/// A toy sequence task with a closed-form target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToyTask {
    /// Parity of all inputs seen so far (including the current one).
    RunningParity,
    /// Parity of the last `w` inputs (including the current one).
    WindowParity(usize),
    /// The input seen `d` steps ago (`0` before the sequence start).
    DelayedRecall(usize),
    /// `1` when strictly more than half of the last `w` inputs are `1`.
    WindowMajority(usize),
}

impl fmt::Display for ToyTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunningParity => write!(f, "running-parity"),
            Self::WindowParity(w) => write!(f, "window-parity w={w}"),
            Self::DelayedRecall(d) => write!(f, "delayed-recall d={d}"),
            Self::WindowMajority(w) => write!(f, "window-majority w={w}"),
        }
    }
}

impl ToyTask {
    /// Exact targets for one input stream.
    pub fn targets(self, inputs: &[bool]) -> Vec<bool> {
        let past = |t: usize, back: usize| t.checked_sub(back).is_some_and(|i| inputs[i]);
        let mut running = false;
        (0..inputs.len())
            .map(|t| match self {
                Self::RunningParity => {
                    running ^= inputs[t];
                    running
                }
                Self::WindowParity(w) => (0..w).fold(false, |p, back| p ^ past(t, back)),
                Self::DelayedRecall(d) => past(t, d),
                Self::WindowMajority(w) => (0..w).filter(|&back| past(t, back)).count() * 2 > w,
            })
            .collect()
    }
}

/// One input stream and its targets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToySequence {
    pub inputs: Vec<bool>,
    pub targets: Vec<bool>,
}

/// Generates `count` independent uniform-bit sequences of length `len`.
pub fn generate(task: ToyTask, count: usize, len: usize, seed: u64) -> Vec<ToySequence> {
    let mut rng = SplitMix64::new(seed);
    (0..count)
        .map(|_| {
            let inputs: Vec<bool> = (0..len).map(|_| rng.next_bool()).collect();
            let targets = task.targets(&inputs);
            ToySequence { inputs, targets }
        })
        .collect()
}

/// Split sizes: sequence counts and lengths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToySplitSpec {
    pub train: usize,
    pub val: usize,
    pub test: usize,
    pub test_long: usize,
    pub len: usize,
    pub long_len: usize,
}

impl Default for ToySplitSpec {
    fn default() -> Self {
        Self {
            train: 128,
            val: 64,
            test: 128,
            test_long: 64,
            len: 32,
            long_len: 128,
        }
    }
}

/// Fixed train / validation / test splits plus a longer-sequence test split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToySplits {
    pub train: Vec<ToySequence>,
    pub val: Vec<ToySequence>,
    pub test: Vec<ToySequence>,
    pub test_long: Vec<ToySequence>,
}

impl ToySplits {
    /// Each split draws from its own derived seed, so splits never share a stream.
    pub fn new(task: ToyTask, spec: ToySplitSpec, seed: u64) -> Self {
        let derive =
            |salt: u64| SplitMix64::new(seed ^ salt.wrapping_mul(0xa076_1d64_78bd_642f)).next_u64();
        Self {
            train: generate(task, spec.train, spec.len, derive(1)),
            val: generate(task, spec.val, spec.len, derive(2)),
            test: generate(task, spec.test, spec.len, derive(3)),
            test_long: generate(task, spec.test_long, spec.long_len, derive(4)),
        }
    }
}

/// Fraction of correct per-step predictions (`predict` returns one bit per step).
pub fn accuracy(sequences: &[ToySequence], mut predict: impl FnMut(&[bool]) -> Vec<bool>) -> f64 {
    let mut correct = 0usize;
    let mut total = 0usize;
    for sequence in sequences {
        let output = predict(&sequence.inputs);
        assert_eq!(output.len(), sequence.targets.len());
        correct += output
            .iter()
            .zip(&sequence.targets)
            .filter(|(a, b)| a == b)
            .count();
        total += sequence.targets.len();
    }
    if total == 0 {
        0.0
    } else {
        correct as f64 / total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_match_hand_computed_examples() {
        let x = [true, false, true, true, false];
        assert_eq!(
            ToyTask::RunningParity.targets(&x),
            vec![true, true, false, true, true]
        );
        assert_eq!(
            ToyTask::WindowParity(2).targets(&x),
            vec![true, true, true, false, true]
        );
        assert_eq!(
            ToyTask::DelayedRecall(2).targets(&x),
            vec![false, false, true, false, true]
        );
        assert_eq!(
            ToyTask::WindowMajority(3).targets(&x),
            vec![false, false, true, true, true]
        );
    }

    #[test]
    fn splits_are_deterministic_per_seed_and_distinct_across_splits() {
        let spec = ToySplitSpec::default();
        let a = ToySplits::new(ToyTask::RunningParity, spec, 7);
        assert_eq!(a, ToySplits::new(ToyTask::RunningParity, spec, 7));
        assert_ne!(a, ToySplits::new(ToyTask::RunningParity, spec, 8));
        assert_ne!(a.train[0], a.test[0]);
        assert_eq!(a.test_long[0].inputs.len(), spec.long_len);
    }
}
