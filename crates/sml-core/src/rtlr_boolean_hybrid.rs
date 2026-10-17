//! Experimental RTLR-ELASTIC / packed-ANF hybrid.
//!
//! A *learned, TRAIN-only* online selector mixes exact local RTRL
//! recurrence with several causally observed packed Boolean delays.
//! No attention, no future labels, no DEV-adaptive selector updates.
//! This is NOT full cross-cell RTRL or a trained 25B language model.
use crate::anf_delay_line::AnfDelayLine;
use crate::rtlr_elastic::RtlrElastic;

type Result<T> = std::result::Result<T, &'static str>;
const FLOOR: f64 = 1e-3;
const MAX_CANDIDATES: usize = 16;

#[derive(Clone, Debug)]
struct Pending {
    experts: Vec<f64>,
    weights: Vec<f64>,
    mixed: f64,
}

/// Online logloss-weighted mixture; score[0] refers to RTRL recurrence,
/// scores[1..] to bounded causal Boolean lag experts.
#[derive(Clone, Debug)]
pub struct RtlrBooleanHybrid {
    recurrent: RtlrElastic,
    history: AnfDelayLine,
    lags: Vec<usize>,
    scores: Vec<f64>,
    eta: f64,
    updates: u64,
    pending: Option<Pending>,
}

impl RtlrBooleanHybrid {
    pub fn new(
        recurrent: RtlrElastic,
        capacity: usize,
        lags: Vec<usize>,
        eta: f64,
    ) -> Result<Self> {
        if lags.is_empty()
            || lags.len() > MAX_CANDIDATES
            || !eta.is_finite()
            || !(0.0..=1.0).contains(&eta)
            || eta == 0.0
        {
            return Err("invalid RTLR Boolean hybrid policy");
        }
        if lags.iter().any(|&lag| lag == 0 || lag > capacity)
            || lags.windows(2).any(|w| w[0] >= w[1])
        {
            return Err("Boolean candidate delays must be sorted, unique and within capacity");
        }
        let history = AnfDelayLine::new(capacity).map_err(|_| "invalid bounded Boolean ring")?;
        Ok(Self {
            recurrent,
            history,
            scores: vec![0.0; lags.len() + 1],
            lags,
            eta,
            updates: 0,
            pending: None,
        })
    }

    #[must_use]
    pub fn delays(&self) -> &[usize] {
        &self.lags
    }

    #[must_use]
    pub fn train_updates(&self) -> u64 {
        self.updates
    }

    #[must_use]
    pub fn boolean_bytes(&self) -> usize {
        self.history.storage_bytes()
    }

    /// Storage accounting excludes Vec headers, allocation metadata,
    /// RTRL dormant banks and any generated-stream/serving buffer.
    #[must_use]
    pub fn logical_payload_bytes(&self) -> usize {
        self.recurrent.active_payload_bytes()
            + self.boolean_bytes()
            + self.scores.len() * size_of::<f64>()
    }

    /// Each returned score is the frozen TRAIN-derived weight at the
    /// moment of this query, not a learned or inferred delay identity.
    #[must_use]
    pub fn expert_weights(&self) -> Vec<f64> {
        let largest = self.scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let raw: Vec<f64> = self.scores.iter().map(|v| (v - largest).exp()).collect();
        let total: f64 = raw.iter().sum();
        raw.into_iter().map(|v| v / total).collect()
    }

    pub fn reset_state(&mut self) -> Result<()> {
        if self.pending.is_some() {
            return Err("cannot reset before committing TRAIN target");
        }
        self.history.reset();
        self.recurrent.reset_state();
        Ok(())
    }

    fn expert_predictions(&self, feature: f64) -> Vec<f64> {
        let mut probs = Vec::with_capacity(self.scores.len());
        probs.push((1.0 / (1.0 + (-4.0 * feature).exp())).clamp(FLOOR, 1.0 - FLOOR));
        for &lag in &self.lags {
            probs.push(match self.history.read_lag(lag) {
                Some(true) => 1.0 - FLOOR,
                Some(false) => FLOOR,
                None => 0.5,
            });
        }
        probs
    }

    fn mixture(&self, feature: f64) -> Pending {
        let experts = self.expert_predictions(feature);
        let weights = self.expert_weights();
        let mixed: f64 = experts.iter().zip(&weights).map(|(p, w)| p * w).sum();
        Pending {
            experts,
            weights,
            mixed: mixed.clamp(FLOOR, 1.0 - FLOOR),
        }
    }

    /// Advance once with past/current available inputs, *before* the
    /// current supervised target is revealed. May not be called twice
    /// without an intervening learn_target.
    pub fn advance_train(&mut self, older: u8, newer: u8) -> Result<f64> {
        if self.pending.is_some() {
            return Err("uncommitted previous TRAIN position");
        }
        let feature = self
            .recurrent
            .advance_train(older, newer)
            .map_err(|_| "RTLR train forward failed")?;
        self.history.observe_pair(older, newer);
        let prediction = self.mixture(feature);
        let score = prediction.mixed;
        self.pending = Some(prediction);
        Ok(score)
    }

    /// Commit target only on TRAIN. Logloss experts compete using a
    /// bounded multiplicative-weights update; RTRL exact-local credit is
    /// chain-rule scaled by the *previous* recurrent mixture weight.
    pub fn learn_target(&mut self, positive: bool) -> Result<()> {
        let pending = self.pending.take().ok_or("missing TRAIN forward position")?;
        let target = f64::from(u8::from(positive));
        let recurrent_p = pending.experts[0];
        let grad_mixed = (pending.mixed - target) / (pending.mixed * (1.0 - pending.mixed));
        let credit = grad_mixed * pending.weights[0] * 4.0 * recurrent_p * (1.0 - recurrent_p);
        self.recurrent
            .learn_credit(credit)
            .map_err(|_| "RTLR exact-local credit failed")?;

        // Scores are renormalized after each update to avoid exponent
        // underflow even for arbitrarily long causal TRAIN streams.
        for (score, &p) in self.scores.iter_mut().zip(&pending.experts) {
            let likelihood = if positive { p } else { 1.0 - p };
            *score += self.eta * likelihood.ln();
        }
        let max_score = self.scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        for score in &mut self.scores {
            *score = (*score - max_score).max(-64.0);
        }
        self.updates += 1;
        Ok(())
    }

    /// Read-only selector and learned-parameter path. Volatile recurrent
    /// and Boolean history continue causally, but no TRAIN gate/weight
    /// update may occur during DEV/inference.
    pub fn advance_inference(&mut self, older: u8, newer: u8) -> Result<f64> {
        if self.pending.is_some() {
            return Err("pending TRAIN update when entering inference");
        }
        let feature = self
            .recurrent
            .advance_inference(older, newer)
            .map_err(|_| "RTLR inference forward failed")?;
        self.history.observe_pair(older, newer);
        Ok(self.mixture(feature).mixed)
    }
}

#[cfg(test)]
mod tests {
    use super::RtlrBooleanHybrid;
    use crate::rtlr_elastic::{RtlrElastic, RtlrElasticConfig};

    fn new_hybrid(lags: Vec<usize>) -> RtlrBooleanHybrid {
        let config = RtlrElasticConfig {
            max_active: 1,
            learning_rate: 0.005,
            grow_above_bpc: 4.1,
            shrink_below_bpc: 3.8,
            patience: 2,
            warmup_documents: 2,
            seed: 17,
        };
        RtlrBooleanHybrid::new(RtlrElastic::new(config).unwrap(), 1024, lags, 0.25).unwrap()
    }

    #[test]
    fn sorted_unique_bounded_selector_validation() {
        let cfg = RtlrElasticConfig {
            max_active: 1,
            learning_rate: 0.005,
            grow_above_bpc: 4.1,
            shrink_below_bpc: 3.8,
            patience: 2,
            warmup_documents: 2,
            seed: 17,
        };
        for delays in [vec![], vec![0], vec![16, 16], vec![128, 16], vec![1025]] {
            assert!(RtlrBooleanHybrid::new(
                RtlrElastic::new(cfg).unwrap(),
                1024,
                delays,
                0.25
            )
            .is_err());
        }
        let h = new_hybrid(vec![1, 16, 128, 1023]);
        assert_eq!(h.boolean_bytes(), 128);
        assert_eq!(h.logical_payload_bytes(), 64 + 128 + 5 * 8);
        assert_eq!(h.train_updates(), 0);
        assert_eq!(h.delays(), &[1, 16, 128, 1023]);
    }

    #[test]
    fn causal_train_target_separation_and_dev_read_only() {
        let mut h = new_hybrid(vec![1, 16]);
        assert!(h.learn_target(true).is_err());
        assert!(h.advance_train(0xaa, 0x55).unwrap().is_finite());
        assert!(h.advance_train(0xaa, 0x55).is_err());
        assert!(h.advance_inference(0xaa, 0x55).is_err());
        assert!(h.reset_state().is_err());
        h.learn_target(false).unwrap();
        assert_eq!(h.train_updates(), 1);
        let before = h.expert_weights();
        h.reset_state().unwrap();
        for i in 0..500 {
            let p = h.advance_inference(i as u8, (i * 43) as u8).unwrap();
            assert!((0.0..=1.0).contains(&p));
        }
        assert_eq!(h.expert_weights(), before);
        assert_eq!(h.train_updates(), 1);
    }

    #[test]
    fn learned_lag_selector_recovers_synthetic_delay_sixteen() {
        let mut h = new_hybrid(vec![1, 16, 128]);
        let bytes: Vec<u8> = (0..6000)
            .scan(0x1636_49be_u32, |state, _| {
                *state ^= *state << 13;
                *state ^= *state >> 17;
                *state ^= *state << 5;
                Some(*state as u8)
            })
            .collect();
        // Causal burn-in with no gradient, followed by TRAIN-only labels.
        for i in 2..1024 {
            h.advance_inference(bytes[i - 2], bytes[i - 1]).unwrap();
        }
        for i in 1024..5000 {
            h.advance_train(bytes[i - 2], bytes[i - 1]).unwrap();
            let y = ((bytes[i - 17] & bytes[i - 16]).count_ones() & 1) != 0;
            h.learn_target(y).unwrap();
        }
        let selector = h.expert_weights();
        // Index 2 corresponds to the predeclared lag16 candidate.
        assert!(selector[2] > 0.98, "selector did not learn lag16: {selector:?}");
        assert_eq!(h.train_updates(), 3976);
        for i in 5000..6000 {
            let p = h.advance_inference(bytes[i - 2], bytes[i - 1]).unwrap();
            let target = ((bytes[i - 17] & bytes[i - 16]).count_ones() & 1) != 0;
            assert_eq!(p > 0.5, target);
        }
        assert_eq!(h.expert_weights(), selector);
    }
}
