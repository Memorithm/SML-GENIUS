//! A6-I2: Bayesian-style posterior over a *supplied* native WideAnfGate
//! committee, updated ONLY with passive (input, noisy label) observations.
//!
//! Four candidates and their prior are supplied; this module does NOT
//! discover the teacher, monomials, or a MOR route. Its conservative
//! abstention is valid only when the true teacher is in the bank; we
//! deliberately test failure of this assumption on missing-teacher tasks.
//! Distinct observed signatures are compared exactly, not by hashes.

use crate::anf_wide_sparse::{WideAnfError, WideAnfGate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassiveCommitteeError {
    CandidateCount,
    DuplicateCandidate,
    Prior,
    Width,
    Budget,
    NoiseRate,
    Confidence,
    NoSamples,
    Gate(WideAnfError),
}
impl From<WideAnfError> for PassiveCommitteeError {
    fn from(err: WideAnfError) -> Self {
        Self::Gate(err)
    }
}
impl std::fmt::Display for PassiveCommitteeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PassiveCommitteeError {}

#[derive(Debug, Clone, PartialEq)]
pub struct PassiveCommittee {
    candidates: Vec<WideAnfGate>,
    /// Shared GLOBAL source length, allowing sparse candidate gates with
    /// independently learned input supports only via new_with_source_width.
    source_width: usize,
    logweights: Vec<f64>,
    prediction_signatures: Vec<Vec<bool>>,
    mismatches: Vec<usize>,
    samples: usize,
    max_samples: usize,
    assumed_label_noise: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PassivePrediction {
    pub candidate_bits: Vec<bool>,
    pub posterior: Vec<f64>,
    pub probability_true: f64,
    pub confidence: f64,
    /// Forced MAP-class prediction, even where bank disagreement remains.
    pub map_prediction: bool,
    /// Selective prediction only if confidence >= user supplied threshold.
    pub selective_prediction: Option<bool>,
    pub unanimous: bool,
}

impl PassiveCommittee {
    /// Only the supplied priors/candidates and previously observed passive
    /// labels affect predictions; no hidden teacher identifier is passed.
    pub fn new(
        candidates: Vec<WideAnfGate>,
        prior_weights: Vec<f64>,
        assumed_label_noise: f64,
        max_samples: usize,
    ) -> Result<Self, PassiveCommitteeError> {
        if !(2..=8).contains(&candidates.len()) {
            return Err(PassiveCommitteeError::CandidateCount);
        }
        let k = candidates[0].fan_in();
        if k == 0 || candidates.iter().any(|c| c.fan_in() != k) {
            return Err(PassiveCommitteeError::Width);
        }
        Self::new_with_source_width(
            candidates,
            prior_weights,
            assumed_label_noise,
            max_samples,
            k,
        )
    }

    /// Opt-in committee for native sparse gates over DIFFERENT subsets of
    /// one global source vector. Unlike new(), gate fan-ins may differ;
    /// all referenced global positions must be strictly within source_width.
    /// This does not discover supports: each gate was independently trained.
    pub fn new_with_source_width(
        candidates: Vec<WideAnfGate>,
        prior_weights: Vec<f64>,
        assumed_label_noise: f64,
        max_samples: usize,
        source_width: usize,
    ) -> Result<Self, PassiveCommitteeError> {
        if !(2..=8).contains(&candidates.len()) {
            return Err(PassiveCommitteeError::CandidateCount);
        }
        if source_width == 0
            || candidates.iter().any(|gate| {
                gate.input_positions()
                    .iter()
                    .any(|&position| position >= source_width)
            })
        {
            return Err(PassiveCommitteeError::Width);
        }
        for (i, candidate) in candidates.iter().enumerate() {
            if candidates[..i].contains(candidate) {
                return Err(PassiveCommitteeError::DuplicateCandidate);
            }
        }
        if prior_weights.len() != candidates.len()
            || prior_weights.iter().any(|&p| !p.is_finite() || p <= 0.0)
        {
            return Err(PassiveCommitteeError::Prior);
        }
        let sum = prior_weights.iter().sum::<f64>();
        if !sum.is_finite() || sum <= 0.0 {
            return Err(PassiveCommitteeError::Prior);
        }
        if !assumed_label_noise.is_finite()
            || !(0.0..0.5).contains(&assumed_label_noise)
            || assumed_label_noise == 0.0
        {
            return Err(PassiveCommitteeError::NoiseRate);
        }
        if max_samples == 0 {
            return Err(PassiveCommitteeError::Budget);
        }
        let count = candidates.len();
        Ok(Self {
            candidates,
            source_width,
            logweights: prior_weights.iter().map(|p| (p / sum).ln()).collect(),
            prediction_signatures: vec![Vec::new(); count],
            mismatches: vec![0usize; count],
            samples: 0,
            max_samples,
            assumed_label_noise,
        })
    }

    #[must_use]
    pub fn samples(&self) -> usize {
        self.samples
    }

    #[must_use]
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    #[must_use]
    pub fn fan_in(&self) -> usize {
        self.source_width
    }

    /// TRAIN-only update: evaluate every candidate before changing state.
    /// Returns an error without committing if width or budget fails.
    pub fn observe(&mut self, input: &[bool], label: bool) -> Result<(), PassiveCommitteeError> {
        if input.len() != self.fan_in() {
            return Err(PassiveCommitteeError::Width);
        }
        if self.samples >= self.max_samples {
            return Err(PassiveCommitteeError::Budget);
        }
        let predicted = self
            .candidates
            .iter()
            .map(|gate| gate.evaluate_scalar(input).map_err(Into::into))
            .collect::<Result<Vec<bool>, PassiveCommitteeError>>()?;
        let correct_log = (1.0 - self.assumed_label_noise).ln();
        let incorrect_log = self.assumed_label_noise.ln();
        for (i, &prediction) in predicted.iter().enumerate() {
            self.prediction_signatures[i].push(prediction);
            if prediction == label {
                self.logweights[i] += correct_log;
            } else {
                self.logweights[i] += incorrect_log;
                self.mismatches[i] += 1;
            }
        }
        self.samples += 1;
        Ok(())
    }

    /// No hash collisions: classes compare every candidate's FULL ordered
    /// passive prediction signature across all seen inputs, ignoring labels.
    #[must_use]
    pub fn observed_equivalence_classes(&self) -> usize {
        let mut representatives = Vec::<usize>::new();
        for i in 0..self.candidates.len() {
            if !representatives
                .iter()
                .any(|&j| self.prediction_signatures[i] == self.prediction_signatures[j])
            {
                representatives.push(i);
            }
        }
        representatives.len()
    }

    #[must_use]
    pub fn observed_signatures(&self) -> &[Vec<bool>] {
        &self.prediction_signatures
    }

    pub fn posterior(&self) -> Vec<f64> {
        let highest = self
            .logweights
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let likelihoods = self
            .logweights
            .iter()
            .map(|weight| (weight - highest).exp())
            .collect::<Vec<_>>();
        let total = likelihoods.iter().sum::<f64>();
        likelihoods.iter().map(|value| value / total).collect()
    }

    pub fn training_min_mismatch_rate(&self) -> Result<f64, PassiveCommitteeError> {
        if self.samples == 0 {
            return Err(PassiveCommitteeError::NoSamples);
        }
        let best = self.mismatches.iter().copied().min().unwrap_or(0);
        Ok(best as f64 / self.samples as f64)
    }

    pub fn bank_adequate(&self, mismatch_threshold: f64) -> Result<bool, PassiveCommitteeError> {
        if !mismatch_threshold.is_finite() || !(0.0..=1.0).contains(&mismatch_threshold) {
            return Err(PassiveCommitteeError::Confidence);
        }
        Ok(self.training_min_mismatch_rate()? <= mismatch_threshold)
    }

    /// A probability is not a proof: a unanimous BANK may still be wrong if
    /// a truly missing function adds a term absent from every candidate.
    pub fn predict(
        &self,
        input: &[bool],
        confidence_threshold: f64,
    ) -> Result<PassivePrediction, PassiveCommitteeError> {
        if !(0.5..=1.0).contains(&confidence_threshold) || !confidence_threshold.is_finite() {
            return Err(PassiveCommitteeError::Confidence);
        }
        if input.len() != self.fan_in() {
            return Err(PassiveCommitteeError::Width);
        }
        let bits = self
            .candidates
            .iter()
            .map(|candidate| candidate.evaluate_scalar(input).map_err(Into::into))
            .collect::<Result<Vec<bool>, PassiveCommitteeError>>()?;
        let posterior = self.posterior();
        let probability_true = posterior
            .iter()
            .zip(&bits)
            .filter(|(_, bit)| **bit)
            .map(|(p, _)| *p)
            .sum::<f64>()
            .clamp(0.0, 1.0);
        let confidence = probability_true.max(1.0 - probability_true);
        let max_candidate = posterior
            .iter()
            .enumerate()
            .max_by(|(ia, va), (ib, vb)| va.total_cmp(vb).then_with(|| ib.cmp(ia)))
            .map_or(0, |(i, _)| i);
        let map_prediction = bits[max_candidate];
        let unanimous = bits.iter().all(|&bit| bit == bits[0]);
        let selective_prediction = (confidence >= confidence_threshold).then_some(map_prediction);
        Ok(PassivePrediction {
            candidate_bits: bits,
            posterior,
            probability_true,
            confidence,
            map_prediction,
            selective_prediction,
            unanimous,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anf_wide_sparse::WideAnfLimits;

    fn limits() -> WideAnfLimits {
        WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
    }
    fn candidates() -> Vec<WideAnfGate> {
        [
            vec![vec![2], vec![3]],
            vec![vec![2], vec![3], vec![0, 1]],
            vec![vec![2], vec![3], vec![4, 5]],
            vec![vec![2], vec![3], vec![0, 1], vec![4, 5]],
        ]
        .into_iter()
        .map(|terms| WideAnfGate::from_monomials((0..8).collect(), terms, 8, limits()).unwrap())
        .collect()
    }
    fn model() -> PassiveCommittee {
        PassiveCommittee::new(candidates(), vec![4.0, 2.0, 2.0, 1.0], 1.0 / 16.0, 512).unwrap()
    }

    #[test]
    fn distinct_learned_sparse_input_supports_share_a_global_source() {
        let limits = limits();
        let low = WideAnfGate::from_monomials(vec![1, 3], vec![vec![0, 1]], 16, limits).unwrap();
        let high =
            WideAnfGate::from_monomials(vec![0, 2, 5], vec![vec![0, 1], vec![2]], 16, limits)
                .unwrap();
        assert_eq!(
            PassiveCommittee::new(
                vec![low.clone(), high.clone()],
                vec![1.0, 1.0],
                1.0 / 16.0,
                8
            )
            .unwrap_err(),
            PassiveCommitteeError::Width
        );
        let mut committee = PassiveCommittee::new_with_source_width(
            vec![low.clone(), high.clone()],
            vec![1.0, 1.0],
            1.0 / 16.0,
            8,
            16,
        )
        .unwrap();
        assert_eq!(committee.fan_in(), 16);
        let mut input = [false; 16];
        input[1] = true;
        input[3] = true;
        input[5] = true;
        committee.observe(&input, true).unwrap();
        let guess = committee.predict(&input, 0.5).unwrap();
        assert_eq!(guess.candidate_bits.len(), 2);
        assert_eq!(
            guess.candidate_bits[0],
            low.evaluate_scalar(&input).unwrap()
        );
        assert_eq!(
            guess.candidate_bits[1],
            high.evaluate_scalar(&input).unwrap()
        );
        assert_eq!(
            PassiveCommittee::new_with_source_width(
                vec![low, high],
                vec![1.0, 1.0],
                1.0 / 16.0,
                8,
                5
            )
            .unwrap_err(),
            PassiveCommitteeError::Width
        );
    }

    #[test]
    fn blind_history_is_exactly_equivalent_under_all_four_native_functions() {
        let mut model = model();
        for t in 0..512 {
            let mut inputs = (0..8).map(|j| (t >> (j % 7)) & 1 != 0).collect::<Vec<_>>();
            inputs[0] = false;
            inputs[4] = false;
            model.observe(&inputs, t % 3 == 0).unwrap();
        }
        assert_eq!(model.observed_equivalence_classes(), 1);
        assert!(model
            .observed_signatures()
            .windows(2)
            .all(|pair| pair[0] == pair[1]));
        let priors = model.posterior();
        for (p, prior) in priors
            .into_iter()
            .zip([4.0 / 9.0, 2.0 / 9.0, 2.0 / 9.0, 1.0 / 9.0])
        {
            assert!((p - prior).abs() < 1e-10);
        }
        let unseen = [true, true, false, false, false, true, false, false];
        let pred = model.predict(&unseen, 0.90).unwrap();
        assert!(!pred.unanimous);
        assert!(pred.selective_prediction.is_none());
        assert!(pred.confidence < 0.90);
    }

    #[test]
    fn partial_and_full_visibility_have_two_and_four_exact_classes() {
        for (first, second, expected) in [(true, false, 2), (true, true, 4)] {
            let mut model = model();
            for i in 0..32 {
                let q = [
                    first && (i & 1 != 0),
                    true,
                    i & 2 != 0,
                    i & 4 != 0,
                    second && (i & 8 != 0),
                    true,
                    false,
                    false,
                ];
                model.observe(&q, q[2] ^ q[3]).unwrap();
            }
            assert_eq!(model.observed_equivalence_classes(), expected);
        }
    }

    #[test]
    fn absent_teacher_can_make_all_consensus_predictions_false_on_unobserved_feature() {
        let model = model();
        let x = [false, false, false, false, false, false, true, true];
        let p = model.predict(&x, 0.9).unwrap();
        assert!(p.unanimous);
        assert_eq!(p.selective_prediction, Some(false));
        // True missing teacher h0 XOR x6*x7 would output TRUE.
        assert!(!p.map_prediction);
        assert!(x[6] && x[7]);
    }

    #[test]
    fn invalid_config_and_attempted_mutation_fail_closed() {
        let invalid =
            PassiveCommittee::new(candidates(), vec![4.0, -2.0, 2.0, 1.0], 1.0 / 16.0, 512);
        assert_eq!(invalid, Err(PassiveCommitteeError::Prior));
        assert_eq!(
            PassiveCommittee::new(candidates(), vec![1.0; 4], 0.0, 512),
            Err(PassiveCommitteeError::NoiseRate)
        );
        let mut model = model();
        let prior_state = model.clone();
        assert_eq!(
            model.observe(&[false; 7], false),
            Err(PassiveCommitteeError::Width)
        );
        assert_eq!(model, prior_state);
        assert_eq!(
            model.predict(&[false; 8], 0.2),
            Err(PassiveCommitteeError::Confidence)
        );
        assert_eq!(
            model.bank_adequate(0.125),
            Err(PassiveCommitteeError::NoSamples)
        );
        let mut duplicates = candidates();
        duplicates[3] = duplicates[0].clone();
        assert_eq!(
            PassiveCommittee::new(duplicates, vec![1.0; 4], 1.0 / 16.0, 10),
            Err(PassiveCommitteeError::DuplicateCandidate)
        );
    }
}
