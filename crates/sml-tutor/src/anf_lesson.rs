//! GF(2) Algebraic-Normal-Form (ANF) learner, used as an engine qualification
//! workload for the Mathematics lane (Boolean algebra).
//!
//! A Boolean function over `inputs` variables is represented by a set of
//! monomials (variable subsets encoded as `u32` masks) whose XOR is the
//! function. The learner recovers those coefficients from labelled examples by
//! Gaussian elimination over GF(2). It abstains when the examples do not pin
//! down every coefficient, and it reports inconsistency when no ANF of the
//! requested shape fits the labels. These are engine components only: nothing
//! here models linguistic or general understanding.

use std::fmt;

/// Largest number of input variables accepted by the validated entry points.
pub const MAX_INPUTS: u8 = 12;

/// Magic prefix of an encoded checkpoint.
pub const CHECKPOINT_MAGIC: &[u8; 8] = b"SMLANF1\0";

/// Byte length of the checkpoint header: magic, inputs, degree, count (u32 LE).
const HEADER_LEN: usize = 14;

/// Errors produced by the ANF lesson engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnfLessonError {
    /// The number of input variables is outside `1..=MAX_INPUTS`.
    InputsOutOfRange(u8),
    /// A partition asked for more disjoint inputs than the domain holds.
    TooManySamples { requested: usize, available: usize },
    /// The checkpoint does not start with `CHECKPOINT_MAGIC`.
    BadMagic,
    /// The checkpoint ends before its declared contents.
    Truncated,
    /// The checkpoint has this many bytes after its declared contents.
    TrailingBytes(usize),
    /// The stored coefficient count does not match the monomial count.
    CountMismatch { expected: u32, found: u32 },
    /// A coefficient bit is set at a monomial index that does not exist.
    StrayBits,
}

impl fmt::Display for AnfLessonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputsOutOfRange(n) => {
                write!(f, "input count {n} is outside 1..={MAX_INPUTS}")
            }
            Self::TooManySamples {
                requested,
                available,
            } => write!(
                f,
                "partition needs {requested} inputs but the domain has {available}"
            ),
            Self::BadMagic => write!(f, "checkpoint magic mismatch"),
            Self::Truncated => write!(f, "checkpoint is truncated"),
            Self::TrailingBytes(n) => write!(f, "checkpoint has {n} trailing bytes"),
            Self::CountMismatch { expected, found } => write!(
                f,
                "checkpoint declares {found} coefficients, expected {expected}"
            ),
            Self::StrayBits => write!(
                f,
                "checkpoint sets coefficient bits beyond the monomial count"
            ),
        }
    }
}

impl std::error::Error for AnfLessonError {}

/// Deterministic SplitMix64 pseudo-random generator.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `0..bound` by rejection sampling. A zero bound yields 0.
    pub fn below(&mut self, bound: u64) -> u64 {
        if bound <= 1 {
            return 0;
        }
        // 2^64 mod bound: draws below this threshold would bias the result.
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let r = self.next_u64();
            if r >= threshold {
                return r % bound;
            }
        }
    }
}

/// A target Boolean function in ANF form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnfTask {
    /// Number of input variables, `1..=MAX_INPUTS`.
    pub inputs: u8,
    /// Maximum monomial degree.
    pub max_degree: u8,
    /// Coefficient bitset aligned with `monomials(inputs, max_degree)`.
    pub target: Vec<u64>,
}

/// Disjoint input sets for training, development, testing and held-out testing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partitions {
    pub train: Vec<u32>,
    pub dev: Vec<u32>,
    pub test: Vec<u32>,
    pub testtest: Vec<u32>,
}

impl Partitions {
    /// True when no input value appears in more than one slot, or twice in one slot.
    pub fn is_disjoint(&self) -> bool {
        let mut all: Vec<u32> = Vec::with_capacity(
            self.train.len() + self.dev.len() + self.test.len() + self.testtest.len(),
        );
        all.extend_from_slice(&self.train);
        all.extend_from_slice(&self.dev);
        all.extend_from_slice(&self.test);
        all.extend_from_slice(&self.testtest);
        all.sort_unstable();
        all.windows(2).all(|w| w[0] != w[1])
    }
}

/// Outcome of `learn`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LearnOutcome {
    /// Every coefficient is determined; `coeffs` is the exact ANF.
    Learned { coeffs: Vec<u64> },
    /// The examples do not determine all coefficients. The learner abstains.
    Underdetermined { rank: usize, unknowns: usize },
    /// The labels admit no ANF of the requested shape.
    Inconsistent,
}

fn check_inputs(inputs: u8) -> Result<(), AnfLessonError> {
    if (1..=MAX_INPUTS).contains(&inputs) {
        Ok(())
    } else {
        Err(AnfLessonError::InputsOutOfRange(inputs))
    }
}

fn words_for(bits: usize) -> usize {
    bits.div_ceil(64)
}

/// Clears the bits at positions `>= n` in a bitset of `n` meaningful bits.
fn mask_tail(words: &mut [u64], n: usize) {
    if let Some(last) = words.last_mut() {
        let r = n % 64;
        if r != 0 {
            *last &= (1u64 << r) - 1;
        }
    }
}

fn bit(words: &[u64], i: usize) -> bool {
    words.get(i / 64).is_some_and(|w| (w >> (i % 64)) & 1 == 1)
}

/// All subsets of `{0..inputs}` with at most `max_degree` elements, as `u32`
/// masks in increasing mask order. For `inputs` above 32 the masks cannot be
/// represented and the result is empty. Callers should keep `inputs` within
/// `1..=MAX_INPUTS`.
pub fn monomials(inputs: u8, max_degree: u8) -> Vec<u32> {
    if inputs > 32 {
        return Vec::new();
    }
    let limit = 1u64 << inputs;
    (0..limit)
        .filter(|m| m.count_ones() <= u32::from(max_degree))
        .map(|m| m as u32)
        .collect()
}

/// Value of the ANF at input `x`: the XOR of every monomial whose coefficient
/// is set and whose variables are all true in `x`.
pub fn evaluate(monos: &[u32], coeffs: &[u64], x: u32) -> bool {
    let mut acc = false;
    for (i, &m) in monos.iter().enumerate() {
        if bit(coeffs, i) && (x & m) == m {
            acc = !acc;
        }
    }
    acc
}

/// A random target function; each coefficient is set with probability 1/2.
pub fn random_task(inputs: u8, max_degree: u8, seed: u64) -> AnfTask {
    let n = monomials(inputs, max_degree).len();
    let mut rng = SplitMix64::new(seed);
    let mut target: Vec<u64> = (0..words_for(n)).map(|_| rng.next_u64()).collect();
    mask_tail(&mut target, n);
    AnfTask {
        inputs,
        max_degree,
        target,
    }
}

/// Splits all `2^inputs` inputs into disjoint slices after a seeded
/// Fisher-Yates shuffle. The same arguments always give the same partition.
pub fn partition(
    inputs: u8,
    train: usize,
    dev: usize,
    test: usize,
    testtest: usize,
    seed: u64,
) -> Result<Partitions, AnfLessonError> {
    check_inputs(inputs)?;
    let available = 1usize << inputs;
    let requested = train
        .saturating_add(dev)
        .saturating_add(test)
        .saturating_add(testtest);
    if requested > available {
        return Err(AnfLessonError::TooManySamples {
            requested,
            available,
        });
    }
    let mut all: Vec<u32> = (0..available as u32).collect();
    let mut rng = SplitMix64::new(seed);
    for i in (1..all.len()).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        all.swap(i, j);
    }
    let mut rest = all.into_iter();
    let mut take = |k: usize| -> Vec<u32> { rest.by_ref().take(k).collect() };
    let train_set = take(train);
    let dev_set = take(dev);
    let test_set = take(test);
    let testtest_set = take(testtest);
    Ok(Partitions {
        train: train_set,
        dev: dev_set,
        test: test_set,
        testtest: testtest_set,
    })
}

/// Recovers the ANF coefficients from labelled examples.
///
/// Each example is a row over the monomial columns plus one label column. The
/// rows are reduced by Gauss-Jordan elimination over GF(2). Inconsistency is
/// checked first, because no rank can rescue contradictory labels. The learner
/// then abstains unless every monomial column is a pivot.
pub fn learn(inputs: u8, max_degree: u8, examples: &[(u32, bool)]) -> LearnOutcome {
    let monos = monomials(inputs, max_degree);
    let n = monos.len();
    let width = words_for(n + 1);
    let label_col = n;
    let rows = examples.len();

    let mut mat = vec![0u64; rows * width];
    for (r, &(x, label)) in examples.iter().enumerate() {
        let row = &mut mat[r * width..(r + 1) * width];
        for (c, &m) in monos.iter().enumerate() {
            if x & m == m {
                row[c / 64] |= 1u64 << (c % 64);
            }
        }
        if label {
            row[label_col / 64] |= 1u64 << (label_col % 64);
        }
    }

    let mut rank = 0usize;
    let mut pivot_buf = vec![0u64; width];
    for col in 0..n {
        let w = col / 64;
        let mask = 1u64 << (col % 64);
        let Some(p) = (rank..rows).find(|&r| mat[r * width + w] & mask != 0) else {
            continue;
        };
        if p != rank {
            for k in 0..width {
                mat.swap(p * width + k, rank * width + k);
            }
        }
        pivot_buf.copy_from_slice(&mat[rank * width..(rank + 1) * width]);
        for r in 0..rows {
            if r != rank && mat[r * width + w] & mask != 0 {
                let dst = &mut mat[r * width..(r + 1) * width];
                // Columns before `col` are already zero in the pivot row.
                for k in w..width {
                    dst[k] ^= pivot_buf[k];
                }
            }
        }
        rank += 1;
    }

    // Rows at or past `rank` have an all-zero coefficient part; a set label
    // there is a contradiction.
    let label_word = label_col / 64;
    let label_mask = 1u64 << (label_col % 64);
    if (rank..rows).any(|r| mat[r * width + label_word] & label_mask != 0) {
        return LearnOutcome::Inconsistent;
    }
    if rank < n {
        return LearnOutcome::Underdetermined { rank, unknowns: n };
    }

    // Full rank: pivot row i belongs to column i and holds only that pivot,
    // so its label is the coefficient.
    let mut coeffs = vec![0u64; words_for(n)];
    for i in 0..n {
        if mat[i * width + label_word] & label_mask != 0 {
            coeffs[i / 64] |= 1u64 << (i % 64);
        }
    }
    LearnOutcome::Learned { coeffs }
}

/// Counts how many `cases` the hypothesis `(inputs, max_degree, coeffs)` gets
/// right against the true value of `target`. Returns `(correct, total)`.
pub fn exam(
    inputs: u8,
    max_degree: u8,
    coeffs: &[u64],
    target: &AnfTask,
    cases: &[u32],
) -> (u64, u64) {
    let hypothesis = monomials(inputs, max_degree);
    let truth = monomials(target.inputs, target.max_degree);
    let correct = cases
        .iter()
        .filter(|&&x| evaluate(&hypothesis, coeffs, x) == evaluate(&truth, &target.target, x))
        .count();
    (correct as u64, cases.len() as u64)
}

/// Serializes a checkpoint. Bits at or beyond the monomial count are dropped,
/// so the output always decodes. `inputs` should lie in `1..=MAX_INPUTS`.
pub fn encode_checkpoint(inputs: u8, max_degree: u8, coeffs: &[u64]) -> Vec<u8> {
    let count = monomials(inputs, max_degree).len();
    let words = words_for(count);
    let mut out = Vec::with_capacity(HEADER_LEN + 8 * words);
    out.extend_from_slice(CHECKPOINT_MAGIC);
    out.push(inputs);
    out.push(max_degree);
    out.extend_from_slice(&(count as u32).to_le_bytes());
    for w in 0..words {
        let mut v = coeffs.get(w).copied().unwrap_or(0);
        if w + 1 == words {
            mask_tail(std::slice::from_mut(&mut v), count);
        }
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Parses a checkpoint produced by `encode_checkpoint`, checking the magic,
/// the lengths, the coefficient count and that no bit lies past the monomial
/// count. Returns `(inputs, max_degree, coeffs)`.
pub fn decode_checkpoint(bytes: &[u8]) -> Result<(u8, u8, Vec<u64>), AnfLessonError> {
    if bytes.len() < CHECKPOINT_MAGIC.len() {
        return Err(AnfLessonError::Truncated);
    }
    if &bytes[..CHECKPOINT_MAGIC.len()] != CHECKPOINT_MAGIC {
        return Err(AnfLessonError::BadMagic);
    }
    if bytes.len() < HEADER_LEN {
        return Err(AnfLessonError::Truncated);
    }
    let inputs = bytes[8];
    let max_degree = bytes[9];
    check_inputs(inputs)?;
    let found = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]);
    let count = monomials(inputs, max_degree).len();
    if found as usize != count {
        return Err(AnfLessonError::CountMismatch {
            expected: count as u32,
            found,
        });
    }
    let words = words_for(count);
    let body = &bytes[HEADER_LEN..];
    let need = 8 * words;
    if body.len() < need {
        return Err(AnfLessonError::Truncated);
    }
    if body.len() > need {
        return Err(AnfLessonError::TrailingBytes(body.len() - need));
    }
    let coeffs: Vec<u64> = body
        .chunks_exact(8)
        .map(|c| {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(c);
            u64::from_le_bytes(buf)
        })
        .collect();
    let used = count % 64;
    if used != 0 {
        if let Some(last) = coeffs.last() {
            if last >> used != 0 {
                return Err(AnfLessonError::StrayBits);
            }
        }
    }
    Ok((inputs, max_degree, coeffs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labelled(task: &AnfTask, monos: &[u32], xs: &[u32]) -> Vec<(u32, bool)> {
        xs.iter()
            .map(|&x| (x, evaluate(monos, &task.target, x)))
            .collect()
    }

    #[test]
    fn monomial_count_for_10_3_is_176() {
        assert_eq!(monomials(10, 3).len(), 176);
        assert_eq!(monomials(10, 3).len(), 1 + 10 + 45 + 120);
    }

    #[test]
    fn monomials_are_in_increasing_mask_order() {
        assert_eq!(monomials(3, 1), vec![0, 1, 2, 4]);
        let m = monomials(6, 2);
        assert!(m.windows(2).all(|w| w[0] < w[1]));
        assert!(m.iter().all(|x| x.count_ones() <= 2));
    }

    #[test]
    fn evaluate_xors_selected_monomials() {
        // Monomials over 2 inputs with degree 2: [0, 1, 2, 3].
        let monos = monomials(2, 2);
        // Only the x1*x2 monomial (mask 3) is set.
        let coeffs = [1u64 << 3];
        let values: Vec<bool> = (0..4).map(|x| evaluate(&monos, &coeffs, x)).collect();
        assert_eq!(values, vec![false, false, false, true]);
        // Constant 1 plus x1: value is 1 XOR x1.
        let coeffs = [0b0011u64];
        let values: Vec<bool> = (0..4).map(|x| evaluate(&monos, &coeffs, x)).collect();
        assert_eq!(values, vec![true, false, true, false]);
    }

    #[test]
    fn random_task_is_deterministic_and_tail_clean() {
        let a = random_task(10, 3, 99);
        let b = random_task(10, 3, 99);
        let c = random_task(10, 3, 100);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.target.len(), 3);
        assert_eq!(a.target[2] >> (176 - 128), 0);
    }

    #[test]
    fn learn_recovers_exact_target_across_seeds() {
        for seed in [1u64, 2, 3, 7, 42] {
            let task = random_task(10, 3, seed);
            let monos = monomials(10, 3);
            let parts = partition(10, 640, 0, 192, 192, seed).expect("partition");
            assert!(parts.is_disjoint());
            let examples = labelled(&task, &monos, &parts.train);
            let coeffs = match learn(10, 3, &examples) {
                LearnOutcome::Learned { coeffs } => coeffs,
                other => panic!("seed {seed}: expected Learned, got {other:?}"),
            };
            assert_eq!(coeffs, task.target, "seed {seed}");
            let (ok, total) = exam(10, 3, &coeffs, &task, &parts.test);
            assert_eq!((ok, total), (192, 192), "seed {seed}");
            let (ok, total) = exam(10, 3, &coeffs, &task, &parts.testtest);
            assert_eq!((ok, total), (192, 192), "seed {seed}");
        }
    }

    #[test]
    fn learn_abstains_when_underdetermined() {
        let task = random_task(10, 3, 5);
        let monos = monomials(10, 3);
        let parts = partition(10, 20, 0, 100, 100, 5).expect("partition");
        let examples = labelled(&task, &monos, &parts.train);
        match learn(10, 3, &examples) {
            LearnOutcome::Underdetermined { rank, unknowns } => {
                assert_eq!(unknowns, 176);
                assert!(rank <= 20, "rank {rank} exceeds row count");
            }
            other => panic!("expected Underdetermined, got {other:?}"),
        }
    }

    #[test]
    fn learn_reports_inconsistent_on_flipped_duplicate() {
        let task = random_task(10, 3, 11);
        let monos = monomials(10, 3);
        let parts = partition(10, 640, 0, 192, 192, 11).expect("partition");
        let mut examples = labelled(&task, &monos, &parts.train);
        assert!(matches!(
            learn(10, 3, &examples),
            LearnOutcome::Learned { .. }
        ));
        let (x, y) = examples[0];
        examples.push((x, !y));
        assert_eq!(learn(10, 3, &examples), LearnOutcome::Inconsistent);
    }

    #[test]
    fn checkpoint_roundtrip() {
        let task = random_task(10, 3, 8);
        let bytes = encode_checkpoint(10, 3, &task.target);
        assert_eq!(&bytes[..8], CHECKPOINT_MAGIC);
        assert_eq!(bytes.len(), HEADER_LEN + 3 * 8);
        let (inputs, degree, coeffs) = decode_checkpoint(&bytes).expect("decode");
        assert_eq!((inputs, degree), (10, 3));
        assert_eq!(coeffs, task.target);
    }

    #[test]
    fn checkpoint_rejects_bad_magic() {
        let mut bytes = encode_checkpoint(10, 3, &[0; 3]);
        bytes[0] = b'X';
        assert_eq!(decode_checkpoint(&bytes), Err(AnfLessonError::BadMagic));
    }

    #[test]
    fn checkpoint_rejects_truncation() {
        let bytes = encode_checkpoint(10, 3, &[1, 2, 3]);
        assert_eq!(
            decode_checkpoint(&bytes[..bytes.len() - 1]),
            Err(AnfLessonError::Truncated)
        );
        assert_eq!(
            decode_checkpoint(&bytes[..5]),
            Err(AnfLessonError::Truncated)
        );
        assert_eq!(
            decode_checkpoint(&bytes[..HEADER_LEN - 1]),
            Err(AnfLessonError::Truncated)
        );
    }

    #[test]
    fn checkpoint_rejects_trailing_bytes() {
        let mut bytes = encode_checkpoint(10, 3, &[1, 2, 3]);
        bytes.push(0);
        assert_eq!(
            decode_checkpoint(&bytes),
            Err(AnfLessonError::TrailingBytes(1))
        );
    }

    #[test]
    fn checkpoint_rejects_stray_bits() {
        // 176 monomials: the last word holds bits 128..176, so bit 180 is stray.
        let mut bytes = encode_checkpoint(10, 3, &[0; 3]);
        // Bit 180 is bit 52 of the third word: byte 6, bit 4.
        let last = bytes.len() - 8;
        bytes[last + 6] |= 1 << 4;
        assert_eq!(decode_checkpoint(&bytes), Err(AnfLessonError::StrayBits));
    }

    #[test]
    fn checkpoint_rejects_bad_header_fields() {
        let mut bytes = encode_checkpoint(10, 3, &[0; 3]);
        bytes[8] = 0;
        assert_eq!(
            decode_checkpoint(&bytes),
            Err(AnfLessonError::InputsOutOfRange(0))
        );
        let mut bytes = encode_checkpoint(10, 3, &[0; 3]);
        bytes[10] = 175;
        assert_eq!(
            decode_checkpoint(&bytes),
            Err(AnfLessonError::CountMismatch {
                expected: 176,
                found: 175
            })
        );
    }

    #[test]
    fn partitions_are_disjoint_and_deterministic() {
        let a = partition(10, 300, 100, 200, 200, 17).expect("partition");
        let b = partition(10, 300, 100, 200, 200, 17).expect("partition");
        let c = partition(10, 300, 100, 200, 200, 18).expect("partition");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.is_disjoint());
        assert_eq!(a.train.len(), 300);
        assert_eq!(a.dev.len(), 100);
        assert_eq!(a.test.len(), 200);
        assert_eq!(a.testtest.len(), 200);
    }

    #[test]
    fn partition_rejects_oversized_request() {
        assert_eq!(
            partition(10, 1000, 10, 10, 10, 1),
            Err(AnfLessonError::TooManySamples {
                requested: 1030,
                available: 1024
            })
        );
        assert_eq!(
            partition(0, 1, 0, 0, 0, 1),
            Err(AnfLessonError::InputsOutOfRange(0))
        );
    }

    #[test]
    fn is_disjoint_detects_overlap() {
        let p = Partitions {
            train: vec![1, 2],
            dev: vec![3],
            test: vec![2],
            testtest: vec![4],
        };
        assert!(!p.is_disjoint());
    }
}
