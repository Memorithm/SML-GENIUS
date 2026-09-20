//! Reference trainability path for hard Boolean two-input gates.
//!
//! Training uses continuous latent logits over the 16 possible two-input
//! Boolean truth tables. Inference remains a hard discrete truth-table lookup.

use core::fmt;

/// Number of distinct Boolean functions of two Boolean inputs.
pub const TRUTH_TABLE_COUNT: usize = 16;

/// Canonical four-bit truth table for two Boolean inputs.
///
/// Bit order is:
/// 0 = (false, false)
/// 1 = (true, false)
/// 2 = (false, true)
/// 3 = (true, true)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TruthTable2(u8);

impl TruthTable2 {
    pub const FALSE: Self = Self(0b0000);
    pub const AND: Self = Self(0b1000);
    pub const XOR: Self = Self(0b0110);
    pub const OR: Self = Self(0b1110);
    pub const TRUE: Self = Self(0b1111);

    /// Constructs a valid two-input Boolean truth table.
    pub const fn new(code: u8) -> Result<Self, TrainError> {
        if code < TRUTH_TABLE_COUNT as u8 {
            Ok(Self(code))
        } else {
            Err(TrainError::InvalidTruthTable(code))
        }
    }

    /// Returns the canonical four-bit code.
    #[must_use]
    pub const fn code(self) -> u8 {
        self.0
    }

    /// Evaluates the hard Boolean function.
    #[must_use]
    pub const fn evaluate(self, left: bool, right: bool) -> bool {
        let index = left as u8 | ((right as u8) << 1);
        self.0 & (1_u8 << index) != 0
    }
}

/// Continuous optimization settings for one latent gate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateTrainingConfig {
    pub epochs: u32,
    pub learning_rate: f64,
    pub epsilon: f64,
}

impl Default for GateTrainingConfig {
    fn default() -> Self {
        Self {
            epochs: 300,
            learning_rate: 0.2,
            epsilon: 1.0e-6,
        }
    }
}

impl GateTrainingConfig {
    /// Rejects non-finite and degenerate optimization settings.
    pub fn validate(self) -> Result<Self, TrainError> {
        if self.epochs == 0 {
            return Err(TrainError::ZeroEpochs);
        }
        if !self.learning_rate.is_finite() || self.learning_rate <= 0.0 {
            return Err(TrainError::InvalidLearningRate);
        }
        if !self.epsilon.is_finite() || self.epsilon <= 0.0 || self.epsilon >= 0.5 {
            return Err(TrainError::InvalidEpsilon);
        }
        Ok(self)
    }
}

/// Exact reference counters for a gate-training run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GateTrainingCounters {
    pub epochs: u32,
    pub samples: u64,
    pub latent_parameter_updates: u64,
}

/// Result of training one latent gate against a complete truth table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateTrainingReport {
    pub initial_table: TruthTable2,
    pub final_table: TruthTable2,
    pub final_loss: f64,
    pub counters: GateTrainingCounters,
}

/// Trainable latent selector over all 16 two-input Boolean functions.
#[derive(Debug, Clone, PartialEq)]
pub struct LatentGate2 {
    logits: [f64; TRUTH_TABLE_COUNT],
}

impl Default for LatentGate2 {
    fn default() -> Self {
        Self {
            logits: [0.0; TRUTH_TABLE_COUNT],
        }
    }
}

impl LatentGate2 {
    /// Constructs a gate from explicit finite latent logits.
    pub fn from_logits(logits: [f64; TRUTH_TABLE_COUNT]) -> Result<Self, TrainError> {
        if logits.iter().any(|value| !value.is_finite()) {
            return Err(TrainError::NonFiniteLatent);
        }
        Ok(Self { logits })
    }

    /// Returns the latent logits for evidence and debugging.
    #[must_use]
    pub const fn logits(&self) -> &[f64; TRUTH_TABLE_COUNT] {
        &self.logits
    }

    /// Returns the hard selected truth table using deterministic lowest-index tie breaking.
    pub fn hard_table(&self) -> Result<TruthTable2, TrainError> {
        if self.logits.iter().any(|value| !value.is_finite()) {
            return Err(TrainError::NonFiniteLatent);
        }

        let mut best_index = 0_usize;
        let mut best_value = self.logits[0];
        for (index, value) in self.logits.iter().copied().enumerate().skip(1) {
            if value > best_value {
                best_index = index;
                best_value = value;
            }
        }
        TruthTable2::new(best_index as u8)
    }

    /// Executes only the hard discrete Boolean table selected by the current logits.
    pub fn hard_forward(&self, left: bool, right: bool) -> Result<bool, TrainError> {
        Ok(self.hard_table()?.evaluate(left, right))
    }

    /// Soft probability used only by the reference optimizer.
    pub fn soft_probability(&self, left: bool, right: bool) -> Result<f64, TrainError> {
        let weights = self.softmax_weights()?;
        Ok(weighted_output(&weights, left, right))
    }

    /// Trains against all four rows of a complete Boolean truth table.
    pub fn train_truth_table(
        &mut self,
        target: TruthTable2,
        config: GateTrainingConfig,
    ) -> Result<GateTrainingReport, TrainError> {
        let config = config.validate()?;
        let initial_table = self.hard_table()?;
        let mut counters = GateTrainingCounters::default();

        for _ in 0..config.epochs {
            let weights = self.softmax_weights()?;
            let mut gradient = [0.0_f64; TRUTH_TABLE_COUNT];

            for input_index in 0..4_u8 {
                let left = input_index & 1 != 0;
                let right = input_index & 2 != 0;
                let target_value = if target.evaluate(left, right) {
                    1.0
                } else {
                    0.0
                };
                let probability = weighted_output(&weights, left, right)
                    .clamp(config.epsilon, 1.0 - config.epsilon);
                let loss_gradient =
                    (probability - target_value) / (probability * (1.0 - probability));

                for candidate_code in 0..TRUTH_TABLE_COUNT {
                    let candidate = TruthTable2::new(candidate_code as u8)?;
                    let candidate_value = if candidate.evaluate(left, right) {
                        1.0
                    } else {
                        0.0
                    };
                    gradient[candidate_code] +=
                        loss_gradient * weights[candidate_code] * (candidate_value - probability);
                }
            }

            for (logit, grad) in self.logits.iter_mut().zip(gradient) {
                *logit -= config.learning_rate * grad / 4.0;
                if !logit.is_finite() {
                    return Err(TrainError::NonFiniteLatent);
                }
            }

            counters.epochs = counters.epochs.saturating_add(1);
            counters.samples = counters.samples.saturating_add(4);
            counters.latent_parameter_updates = counters
                .latent_parameter_updates
                .saturating_add(TRUTH_TABLE_COUNT as u64);
        }

        Ok(GateTrainingReport {
            initial_table,
            final_table: self.hard_table()?,
            final_loss: self.loss_for_truth_table(target, config.epsilon)?,
            counters,
        })
    }

    /// Exact hard-forward accuracy over the four rows of a target truth table.
    pub fn hard_accuracy(&self, target: TruthTable2) -> Result<u8, TrainError> {
        let selected = self.hard_table()?;
        let mut correct = 0_u8;
        for input_index in 0..4_u8 {
            let left = input_index & 1 != 0;
            let right = input_index & 2 != 0;
            if selected.evaluate(left, right) == target.evaluate(left, right) {
                correct += 1;
            }
        }
        Ok(correct)
    }

    fn softmax_weights(&self) -> Result<[f64; TRUTH_TABLE_COUNT], TrainError> {
        if self.logits.iter().any(|value| !value.is_finite()) {
            return Err(TrainError::NonFiniteLatent);
        }

        let maximum = self
            .logits
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let mut weights = [0.0_f64; TRUTH_TABLE_COUNT];
        let mut denominator = 0.0_f64;

        for (weight, logit) in weights.iter_mut().zip(self.logits) {
            *weight = (logit - maximum).exp();
            denominator += *weight;
        }

        if !denominator.is_finite() || denominator <= 0.0 {
            return Err(TrainError::InvalidSoftmax);
        }
        for weight in &mut weights {
            *weight /= denominator;
        }
        Ok(weights)
    }

    fn loss_for_truth_table(&self, target: TruthTable2, epsilon: f64) -> Result<f64, TrainError> {
        let weights = self.softmax_weights()?;
        let mut loss = 0.0_f64;

        for input_index in 0..4_u8 {
            let left = input_index & 1 != 0;
            let right = input_index & 2 != 0;
            let target_value = if target.evaluate(left, right) {
                1.0
            } else {
                0.0
            };
            let probability = weighted_output(&weights, left, right).clamp(epsilon, 1.0 - epsilon);
            loss -=
                target_value * probability.ln() + (1.0 - target_value) * (1.0 - probability).ln();
        }

        Ok(loss / 4.0)
    }
}

fn weighted_output(weights: &[f64; TRUTH_TABLE_COUNT], left: bool, right: bool) -> f64 {
    weights
        .iter()
        .enumerate()
        .map(|(code, weight)| {
            let table = TruthTable2(code as u8);
            if table.evaluate(left, right) {
                *weight
            } else {
                0.0
            }
        })
        .sum()
}

/// Invalid truth table, latent state, or training configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainError {
    InvalidTruthTable(u8),
    ZeroEpochs,
    InvalidLearningRate,
    InvalidEpsilon,
    NonFiniteLatent,
    InvalidSoftmax,
}

impl fmt::Display for TrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for TrainError {}
