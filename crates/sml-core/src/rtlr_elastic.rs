//! RTLR-ELASTIC: demand-allocated, uncoupled recurrent cells with exact local
//! forward eligibility (RTRL) and a learned bounded residual readout.
//!
//! This is a research algorithm: independent cells are exact w.r.t. their own
//! local parameters for frozen weights; they do not implement full cross-cell
//! RTRL. Structural decisions use TRAIN loss only, at document boundaries.
//! The original paged MOR/ANF experts remain completely outside this module.
use crate::elastic_ngram_paged_sha::{hex, Sha256};
use std::{
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

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
    pub bank_parks: u64,
    pub bank_restores: u64,
    pub peak_parked: usize,
    pub train_shadow_documents: u64,
}

/// TRAIN-only V2 width scheduler. All rates are predetermined, not tuned on
/// the already inspected SEC DEV set. Costs are in bits/byte/cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtlrV2Policy {
    pub ewma_alpha: f64,
    pub plateau_tolerance_bpc: f64,
    pub marginal_cost_bpc_per_cell: f64,
}

impl RtlrV2Policy {
    pub fn validate(self) -> Result<Self> {
        if !self.ewma_alpha.is_finite()
            || !(0.0..=1.0).contains(&self.ewma_alpha)
            || self.ewma_alpha == 0.0
            || !self.plateau_tolerance_bpc.is_finite()
            || !(0.0..=1.0).contains(&self.plateau_tolerance_bpc)
            || !self.marginal_cost_bpc_per_cell.is_finite()
            || !(0.0..=0.5).contains(&self.marginal_cost_bpc_per_cell)
        {
            return Err("invalid RTLR-V2 TRAIN-only marginal policy".into());
        }
        Ok(self)
    }
}

#[derive(Clone, Debug)]
struct RtlrV2State {
    policy: RtlrV2Policy,
    bank: PathBuf,
    train_ewma: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct RtlrElastic {
    config: RtlrElasticConfig,
    cells: Vec<Cell>,
    residual_gate: f64,
    grow_streak: u64,
    shrink_streak: u64,
    stats: RtlrElasticStats,
    v2: Option<RtlrV2State>,
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
        [
            0.65 + 0.25 * offset,
            0.3 - 0.20 * offset,
            0.2 + 0.30 * offset,
        ]
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
            v2: None,
            stats: RtlrElasticStats {
                peak_active: 1,
                ..RtlrElasticStats::default()
            },
        })
    }

    /// Create a new private non-symlink bank. Its files persist parked
    /// *weights*, while volatile recurrence state and eligibility are reset
    /// on reactivation. This is not a complete resumable model checkpoint.
    pub fn new_v2(
        config: RtlrElasticConfig,
        policy: RtlrV2Policy,
        bank_directory: &Path,
    ) -> Result<Self> {
        let policy = policy.validate()?;
        let metadata = bank_directory.symlink_metadata();
        if metadata.is_ok() {
            return Err("RTLR-V2 bank path already exists".into());
        }
        let mut model = Self::new(config)?;
        fs::create_dir(bank_directory)?;
        model.v2 = Some(RtlrV2State {
            policy,
            bank: bank_directory.to_path_buf(),
            train_ewma: None,
        });
        Ok(model)
    }

    pub fn anchor_state(&self) -> f64 {
        self.cells[0].h
    }

    pub fn is_v2(&self) -> bool {
        self.v2.is_some()
    }

    fn bank_path(&self, index: usize) -> Result<PathBuf> {
        let v2 = self.v2.as_ref().ok_or("RTLR-V2 bank not enabled")?;
        if index == 0 || index >= self.capacity() {
            return Err("RTLR-V2 invalid dormant cell index".into());
        }
        let meta = fs::symlink_metadata(&v2.bank)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err("RTLR-V2 bank directory replaced or linked".into());
        }
        Ok(v2.bank.join(format!("cell-{index:03}.tsv")))
    }

    fn park_last_cell(&mut self) -> Result<()> {
        let index = self.cells.len().checked_sub(1).ok_or("empty cell set")?;
        let path = self.bank_path(index)?;
        let cell = &self.cells[index];
        // Hex IEEE 754 roundtrip, plus explicit SHA256 to reject bit-flips.
        let record = format!(
            "RTLR_ELASTIC_PARKED_V2\t{index}\t{:016x}\t{:016x}\t{:016x}\n",
            cell.weights[0].to_bits(),
            cell.weights[1].to_bits(),
            cell.weights[2].to_bits()
        );
        let mut hash = Sha256::default();
        hash.update(record.as_bytes());
        let digest = hex(&hash.finish());
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        output.write_all(record.as_bytes())?;
        writeln!(output, "sha256\t{digest}")?;
        output.sync_all()?;
        self.cells.pop();
        self.stats.pruned += 1;
        self.stats.bank_parks += 1;
        self.stats.peak_parked = self
            .stats
            .peak_parked
            .max((self.stats.bank_parks - self.stats.bank_restores) as usize);
        Ok(())
    }

    fn restore_or_allocate(&mut self) -> Result<()> {
        if self.active() >= self.capacity() {
            return Err("RTLR-V2 capacity exceeded".into());
        }
        let index = self.active();
        let path = self.bank_path(index)?;
        let cell = match fs::symlink_metadata(&path) {
            Ok(meta) => {
                if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 512 {
                    return Err("RTLR-V2 unsafe parked cell file".into());
                }
                let contents = fs::read_to_string(&path)?;
                let (record, digest_line) = contents
                    .split_once("sha256\t")
                    .ok_or("RTLR-V2 missing cell digest")?;
                let digest = digest_line
                    .strip_suffix('\n')
                    .ok_or("bad cell digest suffix")?;
                let mut hash = Sha256::default();
                hash.update(record.as_bytes());
                if hex(&hash.finish()) != digest {
                    return Err("RTLR-V2 parked cell SHA256 mismatch".into());
                }
                let parts: Vec<_> = record.trim_end_matches('\n').split('\t').collect();
                if parts.len() != 5
                    || parts[0] != "RTLR_ELASTIC_PARKED_V2"
                    || parts[1] != index.to_string()
                {
                    return Err("RTLR-V2 invalid cell schema or index".into());
                }
                let mut weights = [0.0; LOCAL];
                for (slot, string) in weights.iter_mut().zip(&parts[2..]) {
                    *slot = f64::from_bits(u64::from_str_radix(string, 16)?);
                    if !slot.is_finite() || !(-2.0..=2.0).contains(slot) {
                        return Err("RTLR-V2 invalid parked weight".into());
                    }
                }
                fs::remove_file(&path)?;
                self.stats.bank_restores += 1;
                Cell {
                    weights,
                    h: 0.0,
                    sensitivity: [0.0; LOCAL],
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => new_cell(index, self.config.seed),
            Err(e) => return Err(e.into()),
        };
        self.cells.push(cell);
        self.stats.grown += 1;
        self.stats.peak_active = self.stats.peak_active.max(self.active());
        Ok(())
    }

    /// A paired *TRAIN-only* comparison between the active recurrent
    /// feature and an anchor-feature shadow prediction under the same
    /// then-current experts/router weights. This is NOT an independently
    /// trained counterfactual RTRL-2 model and must not be described as one.
    pub fn observe_train_document_v2(
        &mut self,
        live_bpc: f64,
        shadow_anchor_bpc: f64,
    ) -> Result<()> {
        if self.v2.is_none() {
            return Err("RTLR-V2 policy disabled".into());
        }
        if !live_bpc.is_finite()
            || !shadow_anchor_bpc.is_finite()
            || !(0.0..=16.0).contains(&live_bpc)
            || !(0.0..=16.0).contains(&shadow_anchor_bpc)
        {
            return Err("invalid RTLR-V2 TRAIN paired loss".into());
        }
        let previous = self.v2.as_ref().and_then(|v| v.train_ewma);
        let policy = self.v2.as_ref().ok_or("missing v2")?.policy;
        self.stats.training_documents += 1;
        self.stats.train_shadow_documents += 1;
        let stagnated =
            previous.is_some_and(|base| live_bpc >= base - policy.plateau_tolerance_bpc);
        let active = self.active();
        let benefit = shadow_anchor_bpc - live_bpc;
        let net_benefit = benefit - policy.marginal_cost_bpc_per_cell * (active - 1) as f64;
        if self.stats.training_documents > self.config.warmup_documents {
            if active > 1 && net_benefit <= 0.0 {
                self.shrink_streak += 1;
                self.grow_streak = 0;
                if self.shrink_streak >= self.config.patience {
                    self.park_last_cell()?;
                    self.shrink_streak = 0;
                }
            } else if stagnated && active < self.capacity() {
                self.grow_streak += 1;
                self.shrink_streak = 0;
                if self.grow_streak >= self.config.patience {
                    self.restore_or_allocate()?;
                    self.grow_streak = 0;
                }
            } else {
                self.grow_streak = 0;
                self.shrink_streak = 0;
            }
        }
        self.v2.as_mut().ok_or("v2 disappeared")?.train_ewma =
            Some(previous.map_or(live_bpc, |mean| {
                policy.ewma_alpha * live_bpc + (1.0 - policy.ewma_alpha) * mean
            }));
        Ok(())
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
        let extra_mean =
            self.cells[1..].iter().map(|c| c.h).sum::<f64>() / (self.cells.len() - 1) as f64;
        anchor + self.residual_gate * (extra_mean - anchor)
    }

    fn advance(&mut self, a: u8, b: u8, derivatives: bool) -> Result<f64> {
        let x = signal(b);
        let phi = anf(a, b);
        for cell in &mut self.cells {
            let prior = cell.h;
            cell.h = (cell.weights[0] * prior + cell.weights[1] * x + cell.weights[2] * phi).tanh();
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
                        - self.config.learning_rate * credit * readout_weight * cell.sensitivity[p])
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
            self.residual_gate =
                (old - self.config.learning_rate * credit * (extra_mean - anchor)).clamp(0.0, 0.5);
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
                self.cells
                    .push(new_cell(self.cells.len(), self.config.seed));
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
        writeln!(
            file,
            "RTLR_ELASTIC_EXACT_LOCAL_V1\tactive={}\tmax={}\tgate={:.17e}\tseed={}",
            self.active(),
            self.capacity(),
            self.residual_gate,
            self.config.seed
        )?;
        writeln!(file, "index\ta\tb\tc")?;
        for (index, cell) in self.cells.iter().enumerate() {
            writeln!(
                file,
                "{index}\t{:.17e}\t{:.17e}\t{:.17e}",
                cell.weights[0], cell.weights[1], cell.weights[2]
            )?;
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
            assert_eq!(
                first.advance_inference(a, b).unwrap(),
                second.advance_inference(a, b).unwrap()
            );
        }
        assert_eq!(rt.stats(), before);
        assert_eq!(
            rt.cells.iter().map(|c| c.weights).collect::<Vec<_>>(),
            weights
        );
    }

    #[test]
    fn v2_parks_restores_identical_trained_weights_without_resident_dormant_cells() {
        let unique = format!(
            "rtlr-v2-bank-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let mut cfg = config(4);
        cfg.patience = 1;
        cfg.warmup_documents = 1;
        let policy = RtlrV2Policy {
            ewma_alpha: 0.5,
            plateau_tolerance_bpc: 0.1,
            marginal_cost_bpc_per_cell: 0.001,
        };
        let mut engine = RtlrElastic::new_v2(cfg, policy, &root).unwrap();
        engine.observe_train_document_v2(4.5, 4.5).unwrap();
        engine.observe_train_document_v2(4.5, 4.5).unwrap();
        assert_eq!(engine.active(), 2);
        engine.advance_train(b'a', b'z').unwrap();
        engine.learn_credit(0.17).unwrap();
        let saved = engine.cells[1].weights;
        engine.observe_train_document_v2(5.5, 5.4).unwrap();
        assert_eq!(engine.active(), 1);
        assert_eq!(engine.stats().bank_parks, 1);
        assert!(root.join("cell-001.tsv").exists());
        assert_eq!(engine.active_payload_bytes(), 64);
        engine.observe_train_document_v2(5.5, 5.5).unwrap();
        assert_eq!(engine.active(), 2);
        assert_eq!(engine.stats().bank_restores, 1);
        assert_eq!(engine.cells[1].weights, saved);
        assert_eq!(engine.cells[1].h, 0.0);
        assert!(!root.join("cell-001.tsv").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v2_fails_closed_on_corrupted_parked_weights_and_preserves_active_state() {
        let root = std::env::temp_dir().join(format!(
            "rtlr-v2-corrupt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut cfg = config(3);
        cfg.patience = 1;
        cfg.warmup_documents = 1;
        let policy = RtlrV2Policy {
            ewma_alpha: 0.25,
            plateau_tolerance_bpc: 0.1,
            marginal_cost_bpc_per_cell: 0.001,
        };
        let mut engine = RtlrElastic::new_v2(cfg, policy, &root).unwrap();
        engine.observe_train_document_v2(4.0, 4.0).unwrap();
        engine.observe_train_document_v2(4.0, 4.0).unwrap();
        engine.observe_train_document_v2(4.5, 4.4).unwrap();
        assert_eq!(engine.active(), 1);
        fs::write(root.join("cell-001.tsv"), "tampered").unwrap();
        assert!(engine.observe_train_document_v2(4.5, 4.5).is_err());
        assert_eq!(engine.active(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v2_rejects_insecure_existing_bank_and_invalid_policy() {
        let root = std::env::temp_dir().join(format!(
            "rtlr-v2-existing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        assert!(RtlrElastic::new_v2(
            config(3),
            RtlrV2Policy {
                ewma_alpha: 0.2,
                plateau_tolerance_bpc: 0.1,
                marginal_cost_bpc_per_cell: 0.001,
            },
            &root
        )
        .is_err());
        fs::remove_dir_all(root).unwrap();
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
