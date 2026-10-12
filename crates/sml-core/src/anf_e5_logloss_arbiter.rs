//! A6-E5: bounded, five-parameter contextual mixture optimized exclusively
//! by proper negative log-likelihood on chronologically out-of-fold predictions.
//! ECE and Brier are diagnostics only. No TEST data is accepted by fit()
//! except through a caller violation of the explicit fitted_until < scored_at
//! fold contract (which the API checks but cannot externally authenticate).

pub const ARBITER_SIGNALS: usize = 4;
pub const ARBITER_MAX_ROWS: usize = 600_000;
pub const CALIBRATION_BINS: usize = 10;
const PROBABILITY_SUM_TOLERANCE: f64 = 1.0e-9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArbiterError {
    InvalidShape,
    InvalidProbability,
    InvalidSignals,
    InvalidConfiguration,
    EmptyData,
    ResourceBudget,
    TemporalLeakage,
}

impl std::fmt::Display for ArbiterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ArbiterError {}

fn check_distribution(p: &[f64]) -> Result<(), ArbiterError> {
    if !(2..=256).contains(&p.len())
        || p.iter().any(|&v| !v.is_finite() || v <= 0.0 || v > 1.0)
        || (p.iter().sum::<f64>() - 1.0).abs() > PROBABILITY_SUM_TOLERANCE
    {
        return Err(ArbiterError::InvalidProbability);
    }
    Ok(())
}

fn normalized_entropy(p: &[f64]) -> f64 {
    -p.iter().map(|&value| value * value.ln()).sum::<f64>() / (p.len() as f64).ln()
}

/// Four explicitly bounded signals; no target, TEST or oracle dependence:
/// ngram entropy, log support, JS disagreement, and candidate resource cost.
pub fn context_signals(
    ng: &[f64],
    anf: &[f64],
    support_count: u32,
    support_cap: u32,
    execution_cost: f64,
    cost_cap: f64,
) -> Result<[f64; ARBITER_SIGNALS], ArbiterError> {
    check_distribution(ng)?;
    check_distribution(anf)?;
    if ng.len() != anf.len() {
        return Err(ArbiterError::InvalidShape);
    }
    if support_cap == 0
        || !execution_cost.is_finite()
        || !cost_cap.is_finite()
        || execution_cost < 0.0
        || cost_cap <= 0.0
    {
        return Err(ArbiterError::InvalidConfiguration);
    }
    let entropy = normalized_entropy(ng);
    let js_nats = ng
        .iter()
        .zip(anf)
        .map(|(&base, &candidate)| {
            let mean = (base + candidate) / 2.0;
            0.5 * (base * (base / mean).ln() + candidate * (candidate / mean).ln())
        })
        .sum::<f64>();
    let log_support =
        f64::from(support_count.min(support_cap)).ln_1p() / f64::from(support_cap).ln_1p();
    let features = [
        entropy,
        log_support,
        js_nats / std::f64::consts::LN_2,
        (execution_cost / cost_cap).min(1.0),
    ];
    if features
        .iter()
        .any(|&x| !x.is_finite() || !(-1.0e-12..=1.0 + 1.0e-12).contains(&x))
    {
        return Err(ArbiterError::InvalidSignals);
    }
    Ok(features.map(|x| x.clamp(0.0, 1.0)))
}

/// An OOF prediction row must be later than the last time used to fit BOTH
/// experts. The API accepts *only probabilities of the true class* for
/// proper log-loss fitting; it cannot optimize accuracy/ECE as a substitute.
#[derive(Clone, Copy, Debug)]
pub struct OffFoldRow {
    pub signals: [f64; ARBITER_SIGNALS],
    pub support_count: u32,
    pub ng_truth_probability: f64,
    pub anf_truth_probability: f64,
    pub fitted_until: u64,
    pub scored_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArbiterConfig {
    pub epochs: usize,
    pub learning_rate: f64,
    pub l2: f64,
    pub max_samples: usize,
}

impl Default for ArbiterConfig {
    fn default() -> Self {
        Self {
            epochs: 16,
            learning_rate: 0.02,
            l2: 0.001,
            max_samples: ARBITER_MAX_ROWS,
        }
    }
}

fn check_signals(signals: &[f64; ARBITER_SIGNALS]) -> bool {
    signals
        .iter()
        .all(|&x| x.is_finite() && (0.0..=1.0).contains(&x))
}

fn sigmoid(input: f64) -> f64 {
    if input >= 0.0 {
        1.0 / (1.0 + (-input).exp())
    } else {
        let e = input.exp();
        e / (1.0 + e)
    }
}

fn mix_truth_probability(lambda: f64, ng: f64, anf: f64) -> f64 {
    (1.0 - lambda) * ng + lambda * anf
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoglossArbiter {
    /// weight[0] is bias; 1..4 are context feature weights.
    weights: [f64; ARBITER_SIGNALS + 1],
    samples: usize,
    epochs: usize,
}

impl LoglossArbiter {
    pub fn fit(rows: &[OffFoldRow], cfg: ArbiterConfig) -> Result<Self, ArbiterError> {
        if cfg.epochs == 0
            || cfg.epochs > 128
            || !cfg.learning_rate.is_finite()
            || !(0.0..=1.0).contains(&cfg.learning_rate)
            || cfg.learning_rate == 0.0
            || !cfg.l2.is_finite()
            || !(0.0..=1.0).contains(&cfg.l2)
            || cfg.max_samples == 0
            || cfg.max_samples > ARBITER_MAX_ROWS
        {
            return Err(ArbiterError::InvalidConfiguration);
        }
        if rows.is_empty() {
            return Err(ArbiterError::EmptyData);
        }
        if rows.len() > cfg.max_samples {
            return Err(ArbiterError::ResourceBudget);
        }
        for row in rows {
            if row.fitted_until >= row.scored_at {
                return Err(ArbiterError::TemporalLeakage);
            }
            if !check_signals(&row.signals) {
                return Err(ArbiterError::InvalidSignals);
            }
            for p in [row.ng_truth_probability, row.anf_truth_probability] {
                if !p.is_finite() || p <= 0.0 || p > 1.0 {
                    return Err(ArbiterError::InvalidProbability);
                }
            }
        }

        let mut model = Self {
            weights: [-3.0, 0.0, 0.0, 0.0, 0.0],
            samples: rows.len(),
            epochs: cfg.epochs,
        };
        for _ in 0..cfg.epochs {
            for row in rows {
                // No supporting TRAIN examples: exact ngram fallback.
                if row.support_count == 0 {
                    continue;
                }
                let lambda = model.lambda(&row.signals, row.support_count)?;
                let probability = mix_truth_probability(
                    lambda,
                    row.ng_truth_probability,
                    row.anf_truth_probability,
                );
                // derivative of NLL wrt sigmoid logit, NOT ECE gradient.
                let gradient = -(row.anf_truth_probability - row.ng_truth_probability)
                    / probability
                    * lambda
                    * (1.0 - lambda);
                model.weights[0] -= cfg.learning_rate * gradient;
                for (index, &feature) in row.signals.iter().enumerate() {
                    let j = index + 1;
                    let derivative = gradient * feature + cfg.l2 * model.weights[j];
                    model.weights[j] -= cfg.learning_rate * derivative;
                }
                for weight in &mut model.weights {
                    *weight = weight.clamp(-20.0, 20.0);
                }
            }
        }
        Ok(model)
    }

    pub fn weights(&self) -> &[f64; ARBITER_SIGNALS + 1] {
        &self.weights
    }

    pub fn samples(&self) -> usize {
        self.samples
    }

    pub fn epochs(&self) -> usize {
        self.epochs
    }

    pub fn lambda(
        &self,
        signals: &[f64; ARBITER_SIGNALS],
        support_count: u32,
    ) -> Result<f64, ArbiterError> {
        if !check_signals(signals) {
            return Err(ArbiterError::InvalidSignals);
        }
        if support_count == 0 {
            return Ok(0.0);
        }
        let logit = self.weights[0]
            + self.weights[1..]
                .iter()
                .zip(signals)
                .map(|(w, x)| w * x)
                .sum::<f64>();
        Ok(sigmoid(logit))
    }

    /// Input distributions must each be full, positive, normalized 256-way
    /// posteriors from frozen predictors. Gate cannot make up probability mass.
    pub fn distribution(
        &self,
        ng: &[f64],
        anf: &[f64],
        signals: &[f64; ARBITER_SIGNALS],
        support_count: u32,
    ) -> Result<Vec<f64>, ArbiterError> {
        check_distribution(ng)?;
        check_distribution(anf)?;
        if ng.len() != anf.len() {
            return Err(ArbiterError::InvalidShape);
        }
        let lambda = self.lambda(signals, support_count)?;
        let mixed = ng
            .iter()
            .zip(anf)
            .map(|(&base, &conditional)| mix_truth_probability(lambda, base, conditional))
            .collect::<Vec<_>>();
        check_distribution(&mixed)?;
        Ok(mixed)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ReliabilityBin {
    count: usize,
    confidence_sum: f64,
    correct: usize,
}

/// Evaluation-only stream: NLL, multiclass Brier and top-1 ECE.
/// Neither Brier nor ECE feeds SGD or gate selection.
#[derive(Clone, Debug, Default)]
pub struct CalibrationMonitor {
    rows: usize,
    nll_bits: f64,
    brier_total: f64,
    correct: usize,
    bins: [ReliabilityBin; CALIBRATION_BINS],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibrationSummary {
    pub rows: usize,
    pub bits_per_event: f64,
    pub multiclass_brier: f64,
    pub top1_ece10: f64,
    pub accuracy: f64,
}

impl CalibrationMonitor {
    pub fn observe(&mut self, probabilities: &[f64], truth: u8) -> Result<(), ArbiterError> {
        check_distribution(probabilities)?;
        if usize::from(truth) >= probabilities.len() {
            return Err(ArbiterError::InvalidShape);
        }
        let truth_index = usize::from(truth);
        let best = probabilities
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1).then_with(|| right.0.cmp(&left.0)))
            .ok_or(ArbiterError::EmptyData)?;
        let correct = usize::from(best.0 == truth_index);
        self.nll_bits -= probabilities[truth_index].log2();
        let mut squared = 1.0 - 2.0 * probabilities[truth_index];
        for p in probabilities {
            squared += p * p;
        }
        self.brier_total += squared;
        self.correct += correct;
        self.rows += 1;
        let bin = ((best.1 * CALIBRATION_BINS as f64).floor() as usize).min(CALIBRATION_BINS - 1);
        self.bins[bin].count += 1;
        self.bins[bin].confidence_sum += best.1;
        self.bins[bin].correct += correct;
        Ok(())
    }

    pub fn report(&self) -> Result<CalibrationSummary, ArbiterError> {
        if self.rows == 0 {
            return Err(ArbiterError::EmptyData);
        }
        let total = self.rows as f64;
        let ece = self
            .bins
            .iter()
            .filter(|bin| bin.count != 0)
            .map(|bin| {
                let n = bin.count as f64;
                (n / total) * (bin.confidence_sum / n - bin.correct as f64 / n).abs()
            })
            .sum::<f64>();
        Ok(CalibrationSummary {
            rows: self.rows,
            bits_per_event: self.nll_bits / total,
            multiclass_brier: self.brier_total / total,
            top1_ece10: ece,
            accuracy: self.correct as f64 / total,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_support_js_and_cost_features_are_finite_bounded() {
        let base = [0.5, 0.5];
        let better = [0.95, 0.05];
        let features = context_signals(&base, &better, 10, 100, 4.0, 16.0).unwrap();
        assert!((features[0] - 1.0).abs() < 1.0e-12);
        assert!(features[1] > 0.0 && features[1] < 1.0);
        assert!(features[2] > 0.0 && features[2] < 1.0);
        assert_eq!(features[3], 0.25);
        let matched = context_signals(&base, &base, 0, 100, 0.0, 16.0).unwrap();
        assert_eq!(matched[1], 0.0);
        assert!(matched[2].abs() < 1.0e-12);
    }

    #[test]
    fn chronology_rejects_prediction_on_a_training_fold() {
        let row = OffFoldRow {
            signals: [0.5; 4],
            support_count: 12,
            ng_truth_probability: 0.4,
            anf_truth_probability: 0.7,
            fitted_until: 10,
            scored_at: 10,
        };
        assert_eq!(
            LoglossArbiter::fit(&[row], ArbiterConfig::default()),
            Err(ArbiterError::TemporalLeakage)
        );
    }

    #[test]
    fn proper_nll_learning_favors_complementary_anf_when_oof() {
        let examples = (0..256u64)
            .map(|id| OffFoldRow {
                signals: [0.6, 0.8, 0.3, 0.1],
                support_count: 18,
                ng_truth_probability: 0.25,
                anf_truth_probability: 0.85,
                fitted_until: 1,
                scored_at: id + 2,
            })
            .collect::<Vec<_>>();
        let model = LoglossArbiter::fit(&examples, ArbiterConfig::default()).unwrap();
        assert_eq!(model.samples(), examples.len());
        let lambda = model.lambda(&examples[0].signals, 18).unwrap();
        assert!(
            lambda > 0.5,
            "OOF log-loss gain should favor ANF, λ={lambda}"
        );
        assert!(lambda < 1.0);
    }

    #[test]
    fn zero_support_is_exact_baseline_and_mixture_stays_normalized() {
        let examples = [OffFoldRow {
            signals: [0.5; 4],
            support_count: 8,
            ng_truth_probability: 0.5,
            anf_truth_probability: 0.8,
            fitted_until: 1,
            scored_at: 2,
        }];
        let model = LoglossArbiter::fit(&examples, ArbiterConfig::default()).unwrap();
        let base = [0.8, 0.2];
        let anf = [0.1, 0.9];
        assert_eq!(model.lambda(&[0.0; 4], 0).unwrap(), 0.0);
        assert_eq!(
            model.distribution(&base, &anf, &[0.0; 4], 0).unwrap(),
            base.to_vec()
        );
        let p = model.distribution(&base, &anf, &[0.5; 4], 8).unwrap();
        assert!(p[0] > 0.0 && p[1] > 0.0);
        assert!((p.iter().sum::<f64>() - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn calibration_is_evaluation_only_and_brier_matches_closed_form() {
        let mut monitor = CalibrationMonitor::default();
        monitor.observe(&[0.9, 0.1], 0).unwrap();
        let score = monitor.report().unwrap();
        assert_eq!(score.rows, 1);
        assert!((score.accuracy - 1.0).abs() < 1.0e-12);
        assert!((score.multiclass_brier - 0.02).abs() < 1.0e-12);
        assert!((score.top1_ece10 - 0.1).abs() < 1.0e-12);
        assert!((score.bits_per_event - (-0.9f64.log2())).abs() < 1.0e-12);
        assert_eq!(
            monitor.observe(&[0.9, 0.9], 0),
            Err(ArbiterError::InvalidProbability)
        );
        assert_eq!(monitor.report().unwrap(), score);
    }

    #[test]
    fn invalid_nan_or_negative_signals_and_probabilities_fail_closed() {
        let mut row = OffFoldRow {
            signals: [0.5; 4],
            support_count: 1,
            ng_truth_probability: 0.4,
            anf_truth_probability: 0.8,
            fitted_until: 0,
            scored_at: 1,
        };
        row.ng_truth_probability = f64::NAN;
        assert_eq!(
            LoglossArbiter::fit(&[row], ArbiterConfig::default()),
            Err(ArbiterError::InvalidProbability)
        );
        row.ng_truth_probability = 0.4;
        row.signals[2] = -0.1;
        assert_eq!(
            LoglossArbiter::fit(&[row], ArbiterConfig::default()),
            Err(ArbiterError::InvalidSignals)
        );
    }
}
