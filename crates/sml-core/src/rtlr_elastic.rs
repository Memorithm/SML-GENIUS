//! RTLR-ELASTIC: demand-allocated, uncoupled recurrent cells with exact local
//! forward eligibility (RTRL) and a learned bounded residual readout.
//!
//! This is a research algorithm: independent cells are exact w.r.t. their own
//! local parameters for frozen weights; they do not implement full cross-cell
//! RTRL. Structural decisions use TRAIN loss only, at document boundaries.
//! The original paged MOR/ANF experts remain completely outside this module.
use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const LOCAL: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtlrElasticConfig {
    pub max_active: usize,
    pub learning_rate: f64,
    pub grow_above_bpc: f64,
    pub shrink_below_bpc: f64,
    pub patience: u64,
    pub warmup_documents: u64,
    pub seed: u64,
}

impl RtlrElasticConfig {
    pub fn validate(self) -> Result<Self> {
        if !(1..=64).contains(&self.max_active)
            || !self.learning_rate.is_finite()
            || !(0.0..=0.05).contains(&self.learning_rate)
            || self.learning_rate == 0.0
            || !self.grow_above_bpc.is_finite()
            || !self.shrink_below_bpc.is_finite()
            || self.shrink_below_bpc < 0.0
            || self.shrink_below_bpc >= self.grow_above_bpc
            || self.grow_above_bpc > 8.0
            || !(1..=64).contains(&self.patience)
            || self.warmup_documents > 1024
            || self.seed == 0
        {
            return Err("invalid RTLR-ELASTIC bounded training policy".into());
        }
        Ok(self)
    }
}

#[derive(Clone, Debug)]
struct Cell {
    weights: [f64; LOCAL],
    h: f64,
    sensitivity: [f64; LOCAL],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RtlrElasticStats {
    pub training_documents: u64,
    pub grown: u64,
    pub pruned: u64,
    pub peak_active: usize,
    pub recurrent_parameter_changes: u64,
    pub gate_parameter_changes: u64,
    pub recurrent_steps: u64,
}

#[derive(Clone, Debug)]
pub struct RtlrElastic {
    config: RtlrElasticConfig,
    cells: Vec<Cell>,
    residual_gate: f64,
    grow_streak: u64,
    shrink_streak: u64,
    stats: RtlrElasticStats,
}

fn signal(b: u8) -> f64 {
    (f64::from(b.count_ones()) - 4.0) / 4.0
}

fn anf(a: u8, b: u8) -> f64 {
    if (a & b).count_ones() & 1 == 0 {
        -1.0
    } else {
        1.0
    }
}

fn new_cell(index: usize, seed: u64) -> Cell {
    let weights = if index == 0 {
        [0.65, 0.3, 0.2]
    } else {
        // Deterministically break exact state symmetry at the moment a new
        // cell is allocated. No RNG stream or external initialization file.
        let mut x = seed ^ (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        x ^= x >> 30;
        x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x ^= x >> 27;
        x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^= x >> 31;
        let offset = (x % 1024) as f64 / 1024.0 - 0.5;
        [0.65 + 0.25 * offset, 0.3 - 0.20 * offset, 0.2 + 0.30 * offset]
    };
    Cell {
        weights,
        h: 0.0,
        sensitivity: [0.0; LOCAL],
    }
}

impl RtlrElastic {
    pub fn new(config: RtlrElasticConfig) -> Result<Self> {
        let config = config.validate()?;
        Ok(Self {
            config,
            cells: vec![new_cell(0, config.seed)],
            residual_gate: 0.125,
            grow_streak: 0,
            shrink_streak: 0,
            stats: RtlrElasticStats {
                peak_active: 1,
                ..RtlrElasticStats::default()
            },
        })
    }

    pub fn active(&self) -> usize {
        self.cells.len()
    }

    pub fn capacity(&self) -> usize {
        self.config.max_active
    }

    pub fn stats(&self) -> RtlrElasticStats {
        self.stats
    }

    pub fn trained_local_parameters(&self) -> usize {
        LOCAL * self.cells.len() + usize::from(self.cells.len() > 1)
    }

    /// Logical f64 cell payload: 3 weights, 1 hidden scalar and 3 derivatives
    /// per active cell, plus one residual gate. This excludes Vec overhead.
    pub fn active_payload_bytes(&self) -> usize {
        (7 * self.cells.len() + 1) * size_of::<f64>()
    }

    pub fn reset_state(&mut self) {
        for cell in &mut self.cells {
            cell.h = 0.0;
            cell.sensitivity.fill(0.0);
        }
    }

    fn readout(&self) -> f64 {
        let anchor = self.cells[0].h;
        if self.cells.len() == 1 {
            return anchor;
        }
        let extra_mean = self.cells[1..].iter().map(|c| c.h).sum::<f64>()
            / (self.cells.len() - 1) as f64;
        anchor + self.residual_gate * (extra_mean - anchor)
    }

    fn advance(&mut self, a: u8, b: u8, derivatives: bool) -> Result<f64> {
        let x = signal(b);
        let phi = anf(a, b);
        for cell in &mut self.cells {
            let prior = cell.h;
            cell.h = (cell.weights[0] * prior
                + cell.weights[1] * x
                + cell.weights[2] * phi)
                .tanh();
            if derivatives {
                let d = 1.0 - cell.h * cell.h;
                cell.sensitivity = [
                    d * (prior + cell.weights[0] * cell.sensitivity[0]),
                    d * (x + cell.weights[0] * cell.sensitivity[1]),
                    d * (phi + cell.weights[0] * cell.sensitivity[2]),
                ];
            }
            if !cell.h.is_finite()
                || (derivatives && cell.sensitivity.iter().any(|v| !v.is_finite()))
            {
                return Err("RTLR-ELASTIC nonfinite recurrent state".into());
            }
        }
        let feature = self.readout();
        if !feature.is_finite() {
            return Err("RTLR-ELASTIC nonfinite readout".into());
        }
        Ok(feature)
    }

    /// Call exactly once per predicted byte, before consuming target labels.
    pub fn advance_train(&mut self, a: u8, b: u8) -> Result<f64> {
        self.advance(a, b, true)
    }

    /// Read-only recurrent state evolution (on a disposable clone); no
    /// sensitivity or weight changes, not even UORO random-sign evolution.
    pub fn advance_inference(&mut self, a: u8, b: u8) -> Result<f64> {
        self.advance(a, b, false)
    }

    /// Train local exact eligibility and the bounded residual readout using
    /// the unchanged MOR gradient credit from all 8 target bits.
    pub fn learn_credit(&mut self, credit: f64) -> Result<u64> {
        if !credit.is_finite() {
            return Err("RTLR-ELASTIC nonfinite MOR gradient credit".into());
        }
        let extra = self.cells.len() - 1;
        let anchor = self.cells[0].h;
        let extra_mean = if extra > 0 {
            self.cells[1..].iter().map(|c| c.h).sum::<f64>() / extra as f64
        } else {
            0.0
        };
        let mut changed = 0u64;
        for (index, cell) in self.cells.iter_mut().enumerate() {
            let readout_weight = if extra == 0 {
                1.0
            } else if index == 0 {
                1.0 - self.residual_gate
            } else {
                self.residual_gate / extra as f64
            };
            for p in 0..LOCAL {
                let before = cell.weights[p];
                let after = if extra == 0 {
                    // Preserve the exact left-associative RTRL-2 SGD path.
                    (before - self.config.learning_rate * credit * cell.sensitivity[p])
                        .clamp(-2.0, 2.0)
                } else {
                    (before
                        - self.config.learning_rate
                            * credit
                            * readout_weight
                            * cell.sensitivity[p])
                        .clamp(-2.0, 2.0)
                };
                if !after.is_finite() {
                    return Err("RTLR-ELASTIC nonfinite recurrent update".into());
                }
                if after != before {
                    changed += 1;
                }
                cell.weights[p] = after;
            }
        }
        if extra > 0 {
            let old = self.residual_gate;
            self.residual_gate = (old - self.config.learning_rate * credit
                * (extra_mean - anchor))
                .clamp(0.0, 0.5);
            if !self.residual_gate.is_finite() {
                return Err("RTLR-ELASTIC nonfinite residual gate".into());
            }
            if self.residual_gate != old {
                changed += 1;
                self.stats.gate_parameter_changes += 1;
            }
        }
        self.stats.recurrent_steps += 1;
        self.stats.recurrent_parameter_changes += changed;
        Ok(changed)
    }

    /// TRAIN-only structural scheduler, called after a whole training
    /// document. Expansion and contraction occur at document boundaries,
    /// never by inspecting read-only DEV loss. Patience prevents oscillation.
    pub fn observe_train_document(&mut self, loss_bpc: f64) -> Result<()> {
        if !loss_bpc.is_finite() || !(0.0..=8.0).contains(&loss_bpc) {
            return Err("invalid TRAIN-only document bit loss".into());
        }
        self.stats.training_documents += 1;
        if self.stats.training_documents <= self.config.warmup_documents {
            return Ok(());
        }
        if loss_bpc > self.config.grow_above_bpc && self.active() < self.config.max_active {
            self.grow_streak += 1;
            self.shrink_streak = 0;
            if self.grow_streak >= self.config.patience {
                self.cells.push(new_cell(self.cells.len(), self.config.seed));
                self.stats.grown += 1;
                self.stats.peak_active = self.stats.peak_active.max(self.active());
                self.grow_streak = 0;
            }
        } else if loss_bpc < self.config.shrink_below_bpc && self.active() > 1 {
            self.shrink_streak += 1;
            self.grow_streak = 0;
            if self.shrink_streak >= self.config.patience {
                self.cells.pop();
                self.stats.pruned += 1;
                self.shrink_streak = 0;
            }
        } else {
            self.grow_streak = 0;
            self.shrink_streak = 0;
        }
        Ok(())
    }

    /// Fails closed on incomplete or malformed sidecar paths.
    /// A full snapshot loader including the physical atlas is not implemented.
    pub fn write_evidence(&self, path: &std::path::Path) -> Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        writeln!(file, "RTLR_ELASTIC_EXACT_LOCAL_V1\tactive={}\tmax={}\tgate={:.17e}\tseed={}",
            self.active(), self.capacity(), self.residual_gate, self.config.seed)?;
        writeln!(file, "index\ta\tb\tc")?;
        for (index, cell) in self.cells.iter().enumerate() {
            writeln!(file, "{index}\t{:.17e}\t{:.17e}\t{:.17e}",
                cell.weights[0], cell.weights[1], cell.weights[2])?;
        }
        file.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(max_active: usize) -> RtlrElasticConfig {
        RtlrElasticConfig {
            max_active,
            learning_rate: 0.005,
            grow_above_bpc: 4.1,
            shrink_below_bpc: 3.8,
            patience: 2,
            warmup_documents: 2,
            seed: 17,
        }
    }

    #[test]
    fn scalar_exact_trace_and_update_match_original_rtrl2() {
        let mut rt = RtlrElastic::new(config(1)).unwrap();
        let mut w = [0.65, 0.3, 0.2];
        let mut state = 0.0;
        let mut sens = [0.0; LOCAL];
        for (a, b) in [(b'a', b'b'), (b'c', b'd'), (b'e', b'f'), (b'g', b'h')] {
            let old = state;
            state = (w[0] * state + w[1] * signal(b) + w[2] * anf(a, b)).tanh();
            let d = 1.0 - state * state;
            sens = [
                d * (old + w[0] * sens[0]),
                d * (signal(b) + w[0] * sens[1]),
                d * (anf(a, b) + w[0] * sens[2]),
            ];
            assert_eq!(rt.advance_train(a, b).unwrap(), state);
            let credit = 0.017;
            let changed = rt.learn_credit(credit).unwrap();
            assert!(changed > 0);
            for (p, weight) in w.iter_mut().enumerate() {
                *weight = (*weight - 0.005 * credit * sens[p]).clamp(-2.0, 2.0);
            }
            assert_eq!(rt.cells[0].weights, w);
        }
        assert_eq!(rt.active(), 1);
        assert_eq!(rt.stats().grown, 0);
        assert_eq!(rt.trained_local_parameters(), 3);
    }

    #[test]
    fn training_only_demand_growth_and_retraction_are_bounded() {
        let mut rt = RtlrElastic::new(config(4)).unwrap();
        assert_eq!(rt.active(), 1);
        rt.observe_train_document(7.0).unwrap();
        rt.observe_train_document(7.0).unwrap();
        assert_eq!(rt.active(), 1);
        for _ in 0..12 {
            rt.observe_train_document(7.0).unwrap();
        }
        assert_eq!(rt.active(), 4);
        assert_eq!(rt.stats().grown, 3);
        assert_eq!(rt.stats().peak_active, 4);
        for _ in 0..8 {
            rt.observe_train_document(2.0).unwrap();
        }
        assert_eq!(rt.active(), 1);
        assert_eq!(rt.stats().pruned, 3);
        assert_eq!(rt.active_payload_bytes(), 64);
    }

    #[test]
    fn multi_state_training_is_finite_and_read_only_deterministic() {
        let mut rt = RtlrElastic::new(config(8)).unwrap();
        for _ in 0..12 {
            rt.observe_train_document(6.0).unwrap();
        }
        assert!(rt.active() > 1);
        for (a, b) in [(b'a', b'b'), (b'b', b'c'), (b'c', b'd')] {
            assert!(rt.advance_train(a, b).unwrap().is_finite());
            rt.learn_credit(0.25).unwrap();
        }
        let before = rt.stats();
        let weights: Vec<_> = rt.cells.iter().map(|c| c.weights).collect();
        let mut first = rt.clone();
        first.reset_state();
        let mut second = rt.clone();
        second.reset_state();
        for &(a, b) in &[(b'a', b'z'), (b'c', b'd'), (b'e', b'f')] {
            assert_eq!(first.advance_inference(a, b).unwrap(),
                       second.advance_inference(a, b).unwrap());
        }
        assert_eq!(rt.stats(), before);
        assert_eq!(rt.cells.iter().map(|c| c.weights).collect::<Vec<_>>(), weights);
    }

    #[test]
    fn validation_rejects_invalid_policy_and_nan_credit() {
        let mut invalid = config(0);
        assert!(RtlrElastic::new(invalid).is_err());
        invalid.max_active = 8;
        invalid.grow_above_bpc = 3.0;
        assert!(RtlrElastic::new(invalid).is_err());
        let mut rt = RtlrElastic::new(config(2)).unwrap();
        assert!(rt.learn_credit(f64::NAN).is_err());
        assert!(rt.observe_train_document(f64::INFINITY).is_err());
        assert_eq!(rt.active(), 1);
        assert_eq!(rt.stats().training_documents, 0);
    }
}
