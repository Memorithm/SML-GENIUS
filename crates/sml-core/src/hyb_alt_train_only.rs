//! H-ALT-1: TRAIN-only continuous master for an immutable integer ternary
//! residual head. No MOR writes, DEV training, TEST opening or model-quality
//! claims. Floating-point is used only offline for optimization and BPC.
use std::collections::{BTreeSet, HashSet};

use crate::{
    hyb_alt_ternary_sidecar::{
        byte_log_loss_bits, HybridAltError, TernaryResidualSidecar, TernaryRow, MAX_INPUT_BITS,
    },
    pvp6_sbg::SbgCoordinate,
    semantic::SemanticState,
};

const CLASSES: usize = 256;
const LOGIT_QUANTUM: f64 = 8.0;
const INFERENCE_SCALE: i32 = 8;
const TERNARY_THRESHOLD: f64 = 0.125;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceSplit {
    Train,
    Dev,
    Test,
}

/// Caller must bind this trace to a verified corpus manifest and tokenizer.
/// Split labels alone are not evidence of non-overlapping raw source text.
#[derive(Clone, Copy)]
pub struct ResidualTrace<'a, const S: usize> {
    pub document_id: u64,
    pub position: u64,
    pub split: TraceSplit,
    pub state: &'a SemanticState<S>,
    pub baseline_logits: &'a [i32; 256],
    pub target: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResidualTrainError {
    InvalidRate,
    InvalidFeatures,
    DuplicateFeature(SbgCoordinate),
    OutOfRangeFeature(SbgCoordinate),
    WrongSplit(TraceSplit),
    DocumentLeakage(u64),
    DuplicateDevPosition { document: u64, position: u64 },
    NonfiniteMaster,
    BiasTooLarge,
    InvalidQuantizedHead(HybridAltError),
    InvalidLogLoss,
    EmptyDev,
    ReadError,
}

/// An offline logistic residual readout trained only with labeled TRAIN pairs.
/// Master weights are f64 to make the inference/export precision boundary
/// explicit. Training itself is not bit-identical between different CPUs.
pub struct TrainOnlyResidual<const S: usize> {
    features: Vec<SbgCoordinate>,
    master: Vec<f64>, // [256, F], row-major
    biases: [f64; 256],
    learning_rate: f64,
    seen_train_docs: BTreeSet<u64>,
    updates: u64,
}

impl<const S: usize> TrainOnlyResidual<S> {
    pub fn new(
        features: Vec<SbgCoordinate>,
        learning_rate: f64,
    ) -> Result<Self, ResidualTrainError> {
        if !learning_rate.is_finite()
            || !(0.0..=1.0).contains(&learning_rate)
            || learning_rate == 0.0
        {
            return Err(ResidualTrainError::InvalidRate);
        }
        if !(1..=MAX_INPUT_BITS).contains(&features.len()) {
            return Err(ResidualTrainError::InvalidFeatures);
        }
        let mut seen = BTreeSet::new();
        for &coordinate in &features {
            if coordinate.bit >= S {
                return Err(ResidualTrainError::OutOfRangeFeature(coordinate));
            }
            if !seen.insert(coordinate) {
                return Err(ResidualTrainError::DuplicateFeature(coordinate));
            }
        }
        let width = features.len();
        Ok(Self {
            features,
            master: vec![0.0; CLASSES * width],
            biases: [0.0; CLASSES],
            learning_rate,
            seen_train_docs: BTreeSet::new(),
            updates: 0,
        })
    }

    fn bits(&self, state: &SemanticState<S>) -> Result<Vec<bool>, ResidualTrainError> {
        self.features
            .iter()
            .map(|&coordinate| {
                coordinate
                    .read(state)
                    .map_err(|_| ResidualTrainError::ReadError)
            })
            .collect()
    }

    /// Softmax cross-entropy SGD with matched frozen Boolean baseline logits.
    /// Called with TRAIN examples only. The caller selects TRAIN ordering.
    pub fn observe_train(
        &mut self,
        example: &ResidualTrace<'_, S>,
    ) -> Result<f64, ResidualTrainError> {
        if example.split != TraceSplit::Train {
            return Err(ResidualTrainError::WrongSplit(example.split));
        }
        let bits = self.bits(example.state)?;
        let width = self.features.len();
        let mut scores = [0.0_f64; CLASSES];
        for (class, score) in scores.iter_mut().enumerate() {
            let mut value =
                f64::from(example.baseline_logits[class]) / LOGIT_QUANTUM + self.biases[class];
            for (column, &active) in bits.iter().enumerate() {
                if active {
                    value += self.master[class * width + column];
                }
            }
            *score = value;
        }
        let maximum = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let normalizer: f64 = scores.iter().map(|&score| (score - maximum).exp()).sum();
        if !maximum.is_finite() || !normalizer.is_finite() || normalizer <= 0.0 {
            return Err(ResidualTrainError::NonfiniteMaster);
        }
        let loss_bits = (maximum + normalizer.ln() - scores[usize::from(example.target)])
            / std::f64::consts::LN_2;
        if !loss_bits.is_finite() {
            return Err(ResidualTrainError::NonfiniteMaster);
        }
        let mut gradients = [0.0_f64; CLASSES];
        for (class, gradient) in gradients.iter_mut().enumerate() {
            let probability = (scores[class] - maximum).exp() / normalizer;
            *gradient = self.learning_rate
                * (probability - f64::from(u8::from(class == usize::from(example.target))));
        }
        // Validate the full update before modifying the trainer.
        if self.updates == u64::MAX
            || gradients.iter().enumerate().any(|(class, &gradient)| {
                !((self.biases[class] - gradient).is_finite())
                    || bits.iter().enumerate().any(|(index, &active)| {
                        active && !(self.master[class * width + index] - gradient).is_finite()
                    })
            })
        {
            return Err(ResidualTrainError::NonfiniteMaster);
        }
        for (class, &gradient) in gradients.iter().enumerate() {
            self.biases[class] -= gradient;
            for (column, &active) in bits.iter().enumerate() {
                if active {
                    self.master[class * width + column] -= gradient;
                }
            }
        }
        self.seen_train_docs.insert(example.document_id);
        self.updates += 1;
        Ok(loss_bits)
    }

    #[must_use]
    pub fn train_updates(&self) -> u64 {
        self.updates
    }

    /// One-way freeze destroys TRAIN mutability and exports exact ternary masks.
    pub fn freeze(self) -> Result<FrozenResidual<S>, ResidualTrainError> {
        let width = self.features.len();
        let mut rows = Vec::with_capacity(CLASSES);
        for class in 0..CLASSES {
            let start = class * width;
            let weights: Vec<i8> = self.master[start..start + width]
                .iter()
                .map(|&weight| {
                    if weight >= TERNARY_THRESHOLD {
                        1
                    } else if weight <= -TERNARY_THRESHOLD {
                        -1
                    } else {
                        0
                    }
                })
                .collect();
            let bias = self.biases[class].round();
            if !bias.is_finite() || !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&bias) {
                return Err(ResidualTrainError::BiasTooLarge);
            }
            rows.push(
                TernaryRow::from_weights(&weights, bias as i32)
                    .map_err(ResidualTrainError::InvalidQuantizedHead)?,
            );
        }
        let head = TernaryResidualSidecar::new(self.features, rows)
            .map_err(ResidualTrainError::InvalidQuantizedHead)?;
        Ok(FrozenResidual {
            head,
            seen_train_docs: self.seen_train_docs,
            train_updates: self.updates,
        })
    }
}

/// No update API, no rerouting API, and no ability to write SBG.
pub struct FrozenResidual<const S: usize> {
    head: TernaryResidualSidecar<S>,
    seen_train_docs: BTreeSet<u64>,
    train_updates: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PairedByteBpc {
    pub positions: usize,
    pub baseline_bpc: f64,
    pub hybrid_bpc: f64,
    /// Positive means the residual improved BPC.
    pub benefit_bits_per_byte: f64,
}

impl<const S: usize> FrozenResidual<S> {
    #[must_use]
    pub fn train_updates(&self) -> u64 {
        self.train_updates
    }

    pub fn integer_logits(
        &self,
        state: &SemanticState<S>,
        baseline: &[i32; 256],
    ) -> Result<Vec<i32>, ResidualTrainError> {
        self.head
            .add_to_baseline(state, baseline, INFERENCE_SCALE)
            .map_err(ResidualTrainError::InvalidQuantizedHead)
    }

    /// Evaluation only, on identities not seen during TRAIN.
    /// Caller must additionally verify raw corpus/tokenizer/content identity.
    pub fn evaluate_dev(
        &self,
        examples: &[ResidualTrace<'_, S>],
    ) -> Result<PairedByteBpc, ResidualTrainError> {
        if examples.is_empty() {
            return Err(ResidualTrainError::EmptyDev);
        }
        let mut positions = HashSet::new();
        let mut baseline_bits = 0.0_f64;
        let mut hybrid_bits = 0.0_f64;
        for example in examples {
            if example.split != TraceSplit::Dev {
                return Err(ResidualTrainError::WrongSplit(example.split));
            }
            if self.seen_train_docs.contains(&example.document_id) {
                return Err(ResidualTrainError::DocumentLeakage(example.document_id));
            }
            if !positions.insert((example.document_id, example.position)) {
                return Err(ResidualTrainError::DuplicateDevPosition {
                    document: example.document_id,
                    position: example.position,
                });
            }
            baseline_bits +=
                byte_log_loss_bits(example.baseline_logits, example.target, LOGIT_QUANTUM)
                    .map_err(|_| ResidualTrainError::InvalidLogLoss)?;
            let logits = self.integer_logits(example.state, example.baseline_logits)?;
            let byte_logits: [i32; 256] = logits.try_into().map_err(|_| {
                ResidualTrainError::InvalidQuantizedHead(HybridAltError::InvalidOutputs)
            })?;
            hybrid_bits += byte_log_loss_bits(&byte_logits, example.target, LOGIT_QUANTUM)
                .map_err(|_| ResidualTrainError::InvalidLogLoss)?;
        }
        let tokens = examples.len() as f64;
        Ok(PairedByteBpc {
            positions: examples.len(),
            baseline_bpc: baseline_bits / tokens,
            hybrid_bpc: hybrid_bits / tokens,
            benefit_bits_per_byte: (baseline_bits - hybrid_bits) / tokens,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pvp6_sbg::SbgField;

    fn feature() -> SbgCoordinate {
        SbgCoordinate {
            field: SbgField::Value,
            bit: 0,
        }
    }

    #[test]
    fn train_only_and_dev_is_read_only_with_explicit_leakage_rejection() {
        let mut trainer = TrainOnlyResidual::<1>::new(vec![feature()], 0.15).unwrap();
        let mut state = SemanticState::<1>::zeroed().unwrap();
        let uniform = [0_i32; 256];
        for i in 0..1000_u64 {
            let bit = i % 2 == 0;
            state.value.set(0, bit).unwrap();
            trainer
                .observe_train(&ResidualTrace {
                    document_id: i / 10,
                    position: i,
                    split: TraceSplit::Train,
                    state: &state,
                    baseline_logits: &uniform,
                    target: if bit { 42 } else { 17 },
                })
                .unwrap();
        }
        assert_eq!(trainer.train_updates(), 1000);
        let frozen = trainer.freeze().unwrap();
        let mut dev_states = Vec::new();
        for i in 0..40 {
            let mut state = SemanticState::<1>::zeroed().unwrap();
            state.value.set(0, i % 2 == 0).unwrap();
            dev_states.push(state);
        }
        let dev: Vec<_> = dev_states
            .iter()
            .enumerate()
            .map(|(i, state)| ResidualTrace {
                document_id: 10_000 + i as u64 / 2,
                position: i as u64,
                split: TraceSplit::Dev,
                state,
                baseline_logits: &uniform,
                target: if i % 2 == 0 { 42 } else { 17 },
            })
            .collect();
        let score = frozen.evaluate_dev(&dev).unwrap();
        assert_eq!(score.positions, 40);
        assert!((score.baseline_bpc - 8.0).abs() < 1e-12);
        assert!(
            score.hybrid_bpc < score.baseline_bpc,
            "toy wiring sanity only"
        );
        assert!(matches!(
            frozen.evaluate_dev(&[ResidualTrace {
                document_id: 4,
                position: 4,
                split: TraceSplit::Dev,
                state: &dev_states[0],
                baseline_logits: &uniform,
                target: 42,
            }]),
            Err(ResidualTrainError::DocumentLeakage(4))
        ));
        assert!(matches!(
            frozen.evaluate_dev(&[ResidualTrace {
                document_id: 50_000,
                position: 7,
                split: TraceSplit::Test,
                state: &dev_states[0],
                baseline_logits: &uniform,
                target: 42,
            }]),
            Err(ResidualTrainError::WrongSplit(TraceSplit::Test))
        ));
    }

    #[test]
    fn refuses_dev_train_and_duplicate_dev_positions() {
        let uniform = [0_i32; 256];
        let state = SemanticState::<1>::zeroed().unwrap();
        let mut trainer = TrainOnlyResidual::<1>::new(vec![feature()], 0.1).unwrap();
        let dev = ResidualTrace {
            document_id: 10,
            position: 0,
            split: TraceSplit::Dev,
            state: &state,
            baseline_logits: &uniform,
            target: 17,
        };
        assert_eq!(
            trainer.observe_train(&dev),
            Err(ResidualTrainError::WrongSplit(TraceSplit::Dev))
        );
        let frozen = trainer.freeze().unwrap();
        assert_eq!(
            frozen.evaluate_dev(&[dev, dev]),
            Err(ResidualTrainError::DuplicateDevPosition {
                document: 10,
                position: 0,
            })
        );
    }

    #[test]
    fn no_train_samples_gives_zero_residual_and_equal_dev_bpc() {
        let uniform = [0_i32; 256];
        let state = SemanticState::<1>::zeroed().unwrap();
        let frozen = TrainOnlyResidual::<1>::new(vec![feature()], 0.1)
            .unwrap()
            .freeze()
            .unwrap();
        let baseline = frozen.integer_logits(&state, &uniform).unwrap();
        assert_eq!(baseline, vec![0_i32; 256]);
        let report = frozen
            .evaluate_dev(&[ResidualTrace {
                document_id: 99,
                position: 0,
                split: TraceSplit::Dev,
                state: &state,
                baseline_logits: &uniform,
                target: 7,
            }])
            .unwrap();
        assert_eq!(report.baseline_bpc, report.hybrid_bpc);
    }
}
