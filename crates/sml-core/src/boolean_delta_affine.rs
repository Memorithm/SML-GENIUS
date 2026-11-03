//! BDE-1: bounded online affine Boolean readout over BDE-0 gate features.
//!
//! GF(2) elimination learns parity/affine rules from labelled examples. Unlike
//! BDE-0's signature-vote baseline, it can identify labels for signatures never
//! seen in TRAIN when the relevant equations span that query. An underdetermined
//! query abstains rather than inventing a prediction. This is a limited affine
//! hypothesis class, not a general nonlinear learner or model-quality claim.
#![forbid(unsafe_code)]

use crate::boolean_delta_engine::{BooleanDeltaEngine, BooleanDeltaError, BooleanDeltaWork};

const MAX_FEATURES: usize = 12;
const MAX_COLUMNS: usize = MAX_FEATURES + 1; // affine intercept

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Equation {
    mask: u16,
    label: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanAffineError {
    Engine(BooleanDeltaError),
    FeatureBudgetExceeded,
    TrainingBudgetExceeded,
    InconsistentLabels,
}

impl From<BooleanDeltaError> for BooleanAffineError {
    fn from(error: BooleanDeltaError) -> Self {
        Self::Engine(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanAffineStep {
    pub work: BooleanDeltaWork,
    /// None means that the previous equations could not determine the label.
    pub prediction_before_update: Option<bool>,
    pub independent_equation: bool,
    pub rank: usize,
    pub examples_seen: u64,
}

/// Exact online GF(2) learner for affine functions of up to 12 Boolean gate
/// outputs. It maintains a row-echelon basis of labelled equations and never
/// guesses on a query outside the span of the observed feature rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooleanDeltaAffineLearner {
    engine: BooleanDeltaEngine,
    feature_count: usize,
    pivots: [Option<Equation>; MAX_COLUMNS],
    rank: usize,
    examples_seen: u64,
    max_examples: u64,
}

fn reduce(mut equation: Equation, pivots: &[Option<Equation>; MAX_COLUMNS]) -> Equation {
    // Low-to-high pivots: a higher-pivot row has no lower set bits, so
    // elimination cannot reintroduce a column already processed.
    for (bit, pivot) in pivots.iter().enumerate() {
        if equation.mask & (1u16 << bit) != 0 {
            if let Some(row) = pivot {
                equation.mask ^= row.mask;
                equation.label ^= row.label;
            }
        }
    }
    equation
}

impl BooleanDeltaAffineLearner {
    /// Admission limits: at most 12 output features and 13 GF(2) columns.
    pub fn new(
        engine: BooleanDeltaEngine,
        max_feature_bits: usize,
        max_examples: u64,
    ) -> Result<Self, BooleanAffineError> {
        let feature_count = engine.outputs().len();
        if feature_count > MAX_FEATURES || feature_count > max_feature_bits {
            return Err(BooleanAffineError::FeatureBudgetExceeded);
        }
        Ok(Self {
            engine,
            feature_count,
            pivots: [None; MAX_COLUMNS],
            rank: 0,
            examples_seen: 0,
            max_examples,
        })
    }

    fn mask(&self, engine: &BooleanDeltaEngine) -> u16 {
        let mut mask = 1u16 << self.feature_count;
        for (index, value) in engine.outputs().iter().enumerate() {
            if *value {
                mask |= 1u16 << index;
            }
        }
        mask
    }

    fn infer(&self, mask: u16) -> Option<bool> {
        let reduced = reduce(Equation { mask, label: false }, &self.pivots);
        (reduced.mask == 0).then_some(reduced.label)
    }

    /// A missing prediction is an explicit abstention, not a negative class.
    pub fn predict(
        &mut self,
        inputs: &[bool],
    ) -> Result<(Option<bool>, BooleanDeltaWork), BooleanAffineError> {
        let work = self.engine.apply(inputs)?;
        Ok((self.infer(self.mask(&self.engine)), work))
    }

    /// Atomically admit a labelled example, including the engine cache. A
    /// contradictory label, wrong input length or exhausted budget leaves the
    /// complete learner unchanged. Redundant consistent examples are allowed.
    pub fn train(
        &mut self,
        inputs: &[bool],
        label: bool,
    ) -> Result<BooleanAffineStep, BooleanAffineError> {
        if self.examples_seen >= self.max_examples {
            return Err(BooleanAffineError::TrainingBudgetExceeded);
        }
        let mut candidate = self.engine.clone();
        let work = candidate.apply(inputs)?;
        let mask = self.mask(&candidate);
        let prediction_before_update = self.infer(mask);
        let mut equation = Equation { mask, label };
        let mut pivots = self.pivots;
        let mut independent_equation = false;
        for (bit, pivot) in pivots.iter_mut().enumerate() {
            if equation.mask & (1u16 << bit) == 0 {
                continue;
            }
            if let Some(row) = pivot {
                equation.mask ^= row.mask;
                equation.label ^= row.label;
            } else {
                *pivot = Some(equation);
                independent_equation = true;
                break;
            }
        }
        if !independent_equation && equation.label {
            return Err(BooleanAffineError::InconsistentLabels);
        }
        self.engine = candidate;
        self.pivots = pivots;
        self.rank += usize::from(independent_equation);
        self.examples_seen += 1;
        Ok(BooleanAffineStep {
            work,
            prediction_before_update,
            independent_equation,
            rank: self.rank,
            examples_seen: self.examples_seen,
        })
    }

    #[must_use]
    pub fn rank(&self) -> usize {
        self.rank
    }

    #[must_use]
    pub fn examples_seen(&self) -> u64 {
        self.examples_seen
    }

    /// Allocated fixed pivot-array bytes; not process RSS or engine heap size.
    #[must_use]
    pub fn pivot_capacity_bytes(&self) -> usize {
        core::mem::size_of::<[Option<Equation>; MAX_COLUMNS]>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean_delta_engine::{BooleanDeltaBudget, BooleanDeltaLearner};
    use crate::Wire;
    use std::hint::black_box;
    use std::time::Instant;

    const TRAIN: [usize; 5] = [0, 1, 2, 4, 8];
    const HELDOUT: [usize; 11] = [3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15];

    fn inputs(bits: usize) -> [bool; 4] {
        [bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0]
    }

    fn label(bits: usize) -> bool {
        !((bits & 1 != 0) ^ (bits & 4 != 0))
    }

    fn engine() -> BooleanDeltaEngine {
        BooleanDeltaEngine::new(
            4,
            vec![],
            (0..4).map(Wire::Input).collect(),
            BooleanDeltaBudget {
                max_inputs: 4,
                max_gates: 0,
                max_edges: 0,
                max_outputs: 4,
            },
        )
        .unwrap()
    }

    fn trained_models() -> (BooleanDeltaAffineLearner, BooleanDeltaLearner) {
        let mut affine = BooleanDeltaAffineLearner::new(engine(), 4, 5).unwrap();
        let mut votes = BooleanDeltaLearner::new(engine(), 4, 5).unwrap();
        for bits in TRAIN {
            affine.train(&inputs(bits), label(bits)).unwrap();
            votes.train(&inputs(bits), label(bits)).unwrap();
        }
        (affine, votes)
    }

    #[test]
    fn learns_affine_rule_and_predicts_all_unseen_signatures() {
        let (mut affine, mut votes) = trained_models();
        assert_eq!(affine.rank(), 5);
        assert_eq!(affine.examples_seen(), 5);
        let mut vote_errors = 0;
        for bits in HELDOUT {
            let (prediction, _) = affine.predict(&inputs(bits)).unwrap();
            assert_eq!(prediction, Some(label(bits)));
            let (vote_prediction, _) = votes.predict(&inputs(bits)).unwrap();
            vote_errors += usize::from(vote_prediction != label(bits));
        }
        assert!(
            vote_errors > 0,
            "unseen-signature vote baseline must be imperfect"
        );
    }

    #[test]
    fn underdetermined_abstains_and_consistent_duplicate_does_not_add_rank() {
        let mut model = BooleanDeltaAffineLearner::new(engine(), 4, 5).unwrap();
        assert_eq!(model.predict(&inputs(0)).unwrap().0, None);
        assert!(
            model
                .train(&inputs(0), label(0))
                .unwrap()
                .independent_equation
        );
        assert_eq!(model.predict(&inputs(0)).unwrap().0, Some(true));
        assert_eq!(model.predict(&inputs(2)).unwrap().0, None);
        assert!(
            !model
                .train(&inputs(0), label(0))
                .unwrap()
                .independent_equation
        );
        assert_eq!(model.rank(), 1);
    }

    #[test]
    fn contradictory_labels_invalid_inputs_and_budget_are_atomic() {
        let mut model = BooleanDeltaAffineLearner::new(engine(), 4, 1).unwrap();
        let original = model.clone();
        assert_eq!(
            model.train(&[false], true),
            Err(BooleanAffineError::Engine(BooleanDeltaError::InputCount {
                expected: 4,
                actual: 1,
            }))
        );
        assert_eq!(model, original);
        model.train(&inputs(0), true).unwrap();
        let full = model.clone();
        assert_eq!(
            model.train(&inputs(1), false),
            Err(BooleanAffineError::TrainingBudgetExceeded)
        );
        assert_eq!(model, full);

        let mut inconsistent = BooleanDeltaAffineLearner::new(engine(), 4, 3).unwrap();
        inconsistent.train(&inputs(0), true).unwrap();
        let before = inconsistent.clone();
        assert_eq!(
            inconsistent.train(&inputs(0), false),
            Err(BooleanAffineError::InconsistentLabels)
        );
        assert_eq!(inconsistent, before);
        assert_eq!(
            BooleanDeltaAffineLearner::new(engine(), 3, 5),
            Err(BooleanAffineError::FeatureBudgetExceeded)
        );
    }

    #[test]
    fn exhaustive_four_feature_affine_truth_tables_match_independent_oracle() {
        // Enumerate all 2^(4+1) affine label functions, including the
        // constant functions. Five independent TRAIN rows determine each
        // function; all eleven HELDOUT signatures must then be predictable.
        // This is an exact finite-domain correctness oracle, not a claim
        // about noisy or nonlinear learning.
        for coefficients in 0u32..32 {
            let expected = |bits: usize| {
                let selected = bits & (coefficients as usize & 0b1111);
                (selected.count_ones() & 1 != 0) ^ (coefficients & 0b10000 != 0)
            };
            let mut learner = BooleanDeltaAffineLearner::new(engine(), 4, 5).unwrap();
            for bits in TRAIN {
                learner.train(&inputs(bits), expected(bits)).unwrap();
            }
            assert_eq!(learner.rank(), 5, "coefficients={coefficients}");
            for bits in 0..16 {
                let (actual, _) = learner.predict(&inputs(bits)).unwrap();
                assert_eq!(
                    actual,
                    Some(expected(bits)),
                    "coefficients={coefficients} signature={bits}"
                );
            }
        }
    }

    #[test]
    fn nonlinear_label_conflict_is_rejected_without_state_mutation() {
        // XOR-affine interpolation cannot fit a nonlinear AND target.
        // Once all five independent rows are known, a contradictory
        // sixth row must fail atomically rather than corrupt the basis.
        let mut learner = BooleanDeltaAffineLearner::new(engine(), 4, 6).unwrap();
        for bits in TRAIN {
            learner
                .train(&inputs(bits), bits & 1 != 0 && bits & 2 != 0)
                .unwrap();
        }
        let before = learner.clone();
        assert_eq!(
            learner.train(&inputs(3), true),
            Err(BooleanAffineError::InconsistentLabels)
        );
        assert_eq!(learner, before);
    }

    #[test]
    fn measured_affine_generalization_against_truth_table() {
        // Fixed TRAIN/HELDOUT with disjoint complete feature signatures.
        // Timings are observational, not speedup or model-quality claims.
        let mut affine = BooleanDeltaAffineLearner::new(engine(), 4, 5).unwrap();
        let mut votes = BooleanDeltaLearner::new(engine(), 4, 5).unwrap();
        let start = Instant::now();
        for bits in TRAIN {
            affine.train(&inputs(bits), label(bits)).unwrap();
        }
        let train_ns = start.elapsed().as_nanos();
        for bits in TRAIN {
            votes.train(&inputs(bits), label(bits)).unwrap();
        }
        let mut covered = 0;
        let mut errors = 0;
        let mut vote_errors = 0;
        for bits in HELDOUT {
            let (prediction, _) = affine.predict(&inputs(bits)).unwrap();
            covered += usize::from(prediction.is_some());
            errors += usize::from(prediction != Some(label(bits)));
            vote_errors += usize::from(votes.predict(&inputs(bits)).unwrap().0 != label(bits));
        }
        assert_eq!(covered, HELDOUT.len());
        assert_eq!(errors, 0);
        assert!(vote_errors > 0);
        let iterations = 20_000usize;
        let mut abstentions = 0;
        let mut work = 0;
        let start = Instant::now();
        for index in 0..iterations {
            let bits = HELDOUT[index % HELDOUT.len()];
            let (prediction, delta) = affine.predict(&inputs(bits)).unwrap();
            abstentions += usize::from(black_box(prediction).is_none());
            work += delta.evaluated_gates;
        }
        let affine_ns = start.elapsed().as_nanos();
        let start = Instant::now();
        for index in 0..iterations {
            let bits = HELDOUT[index % HELDOUT.len()];
            black_box(votes.predict(&inputs(bits)).unwrap().0);
        }
        let vote_ns = start.elapsed().as_nanos();
        assert_eq!(abstentions, 0);
        println!(
            "BDE1_AFFINE_MEASURE_V1 train_samples={} heldout_unseen_signatures={} heldout_covered={covered} heldout_errors={errors} vote_baseline_errors={vote_errors} rank={} train_ns={train_ns} infer_samples={iterations} infer_abstentions={abstentions} affine_ns={affine_ns} vote_ns={vote_ns} gate_evaluations={work} pivot_capacity_bytes={}",
            TRAIN.len(),
            HELDOUT.len(),
            affine.rank(),
            affine.pivot_capacity_bytes()
        );
    }
}
