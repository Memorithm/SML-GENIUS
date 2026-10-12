//! A6-E4: TRAIN-only, bounded joint feature-subset selection over supplied
//! Boolean/ANF outputs. Unlike single-feature forward selection, the pair
//! shortlist can capture XOR synergy with zero marginal information.
//!
//! This module learns which supplied gates to read, NOT new ANF coefficients.
//! VAL/TEST cannot influence fit: the fit API accepts TRAIN rows only.
//! Existing recurrent/C1 and A6-E3 implementations remain untouched.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JointRow {
    /// Bit i is the output of the caller's predeclared Boolean candidate i.
    pub features: u128,
    /// Class label in 0..classes (2..=256).
    pub label: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointConfig {
    pub candidates: usize,
    pub classes: usize,
    pub max_selected: usize,
    pub probe_rows: usize,
    pub pair_finalists: usize,
    /// Symmetric per-class Dirichlet pseudocount.
    pub alpha: f64,
}

impl Default for JointConfig {
    fn default() -> Self {
        Self {
            candidates: 128,
            classes: 256,
            max_selected: 8,
            probe_rows: 4096,
            pair_finalists: 32,
            alpha: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointError {
    InvalidConfig,
    EmptyTrain,
    TooManyRows,
    FeatureOutsideDomain,
    TargetOutsideDomain,
    InvalidPrediction,
}

impl std::fmt::Display for JointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for JointError {}

pub const MAX_JOINT_ROWS: usize = 500_000;
pub const MAX_JOINT_FEATURES: usize = 128;
pub const MAX_JOINT_SELECTED: usize = 8;
pub const MAX_JOINT_PROBE_ROWS: usize = 4096;
pub const MAX_JOINT_FINALISTS: usize = 32;
const EPSILON: f64 = 1.0e-12;

fn feature_valid(bits: u128, count: usize) -> bool {
    count == 128 || (bits >> count) == 0
}

fn signature(bits: u128, selected: &[usize]) -> usize {
    selected
        .iter()
        .enumerate()
        .fold(0usize, |acc, (slot, &index)| {
            acc | (((bits >> index) as usize & 1) << slot)
        })
}

fn build_counts(rows: &[JointRow], ids: &[usize], classes: usize) -> (Vec<u32>, Vec<u32>) {
    let addresses = 1usize << ids.len();
    let mut counts = vec![0u32; addresses * classes];
    let mut totals = vec![0u32; addresses];
    for row in rows {
        let address = signature(row.features, ids);
        totals[address] += 1;
        counts[address * classes + usize::from(row.label)] += 1;
    }
    (counts, totals)
}

/// Exact training leave-one-out multinomial log-loss, accumulated by observed
/// (signature,class) histograms; no held-out samples are accepted here.
/// A row removes itself from both its conditional total and its class count.
fn loo_bits(rows: &[JointRow], ids: &[usize], classes: usize, alpha: f64) -> f64 {
    let (counts, totals) = build_counts(rows, ids, classes);
    let mut loss = 0.0;
    for (address, &total) in totals.iter().enumerate() {
        if total == 0 {
            continue;
        }
        let denominator = f64::from(total - 1) + alpha * classes as f64;
        for &count in &counts[address * classes..(address + 1) * classes] {
            if count != 0 {
                let probability = (f64::from(count - 1) + alpha) / denominator;
                loss -= f64::from(count) * probability.log2();
            }
        }
    }
    loss / rows.len() as f64
}

/// Deterministic strictly-improving forward selection from an arbitrary seed.
/// A neutral one-feature step is not accepted, exposing the XOR blind spot.
fn forward(
    rows: &[JointRow],
    cfg: JointConfig,
    seed: &[usize],
    full_evaluations: &mut usize,
) -> (Vec<usize>, f64) {
    let mut chosen = seed.to_vec();
    let mut current = loo_bits(rows, &chosen, cfg.classes, cfg.alpha);
    *full_evaluations += 1;
    while chosen.len() < cfg.max_selected {
        let mut winner: Option<(usize, f64)> = None;
        for id in 0..cfg.candidates {
            if chosen.contains(&id) {
                continue;
            }
            let mut proposed = chosen.clone();
            proposed.push(id);
            proposed.sort_unstable();
            let loss = loo_bits(rows, &proposed, cfg.classes, cfg.alpha);
            *full_evaluations += 1;
            if loss + EPSILON < current
                && winner
                    .as_ref()
                    .is_none_or(|&(_, best_loss)| loss + EPSILON < best_loss)
            {
                winner = Some((id, loss));
            }
        }
        let Some((id, improved)) = winner else {
            break;
        };
        chosen.push(id);
        chosen.sort_unstable();
        current = improved;
    }
    (chosen, current)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointOrigin {
    Greedy,
    PairSeed,
}

/// Immutable TRAIN-frozen multinomial readout over the selected Boolean gates.
#[derive(Clone, Debug, PartialEq)]
pub struct JointModel {
    selected: Vec<usize>,
    candidate_count: usize,
    classes: usize,
    alpha: f64,
    counts: Vec<u32>,
    totals: Vec<u32>,
}

impl JointModel {
    fn from_train(rows: &[JointRow], selected: Vec<usize>, cfg: JointConfig) -> Self {
        let (counts, totals) = build_counts(rows, &selected, cfg.classes);
        Self {
            selected,
            candidate_count: cfg.candidates,
            classes: cfg.classes,
            alpha: cfg.alpha,
            counts,
            totals,
        }
    }

    pub fn selected_features(&self) -> &[usize] {
        &self.selected
    }

    pub fn training_samples(&self) -> u64 {
        self.totals.iter().map(|&n| u64::from(n)).sum()
    }

    /// Strictly positive and normalized even for a never-seen signature.
    pub fn distribution(&self, features: u128) -> Result<Vec<f64>, JointError> {
        if !feature_valid(features, self.candidate_count) {
            return Err(JointError::InvalidPrediction);
        }
        let address = signature(features, &self.selected);
        let n = f64::from(self.totals[address]);
        let denominator = n + self.alpha * self.classes as f64;
        Ok(self.counts[address * self.classes..(address + 1) * self.classes]
            .iter()
            .map(|&count| (f64::from(count) + self.alpha) / denominator)
            .collect())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct JointReport {
    pub greedy: JointModel,
    pub selected: JointModel,
    pub origin: JointOrigin,
    pub greedy_train_loo_bits: f64,
    pub selected_train_loo_bits: f64,
    pub probed_pairs: usize,
    pub full_train_scores: usize,
    pub probe_sample_count: usize,
}

/// Select Boolean features on TRAIN only. Every pair is ranked on a bounded
/// deterministic TRAIN subsample; only top finalists are rescored on all TRAIN.
/// The selection model cannot read validation/test labels through this API.
///
/// Work upper bound: C(candidates,2)*min(n,probe_rows) probe row visits, plus
/// (pair_finalists + max_selected*candidates + O(1))*n full TRAIN visits.
pub fn fit_joint(rows: &[JointRow], cfg: JointConfig) -> Result<JointReport, JointError> {
    if !(2..=MAX_JOINT_FEATURES).contains(&cfg.candidates)
        || !(2..=256).contains(&cfg.classes)
        || cfg.max_selected == 0
        || cfg.max_selected > MAX_JOINT_SELECTED
        || cfg.max_selected > cfg.candidates
        || cfg.probe_rows == 0
        || cfg.probe_rows > MAX_JOINT_PROBE_ROWS
        || cfg.pair_finalists == 0
        || cfg.pair_finalists > MAX_JOINT_FINALISTS
        || !cfg.alpha.is_finite()
        || cfg.alpha <= 0.0
    {
        return Err(JointError::InvalidConfig);
    }
    if rows.is_empty() {
        return Err(JointError::EmptyTrain);
    }
    if rows.len() > MAX_JOINT_ROWS {
        return Err(JointError::TooManyRows);
    }
    for row in rows {
        if !feature_valid(row.features, cfg.candidates) {
            return Err(JointError::FeatureOutsideDomain);
        }
        if usize::from(row.label) >= cfg.classes {
            return Err(JointError::TargetOutsideDomain);
        }
    }

    let mut full_train_scores = 0usize;
    let (greedy_ids, greedy_bits) = forward(rows, cfg, &[], &mut full_train_scores);
    let mut final_ids = greedy_ids.clone();
    let mut final_bits = greedy_bits;
    let mut origin = JointOrigin::Greedy;
    let mut probed_pairs = 0usize;
    let mut probe_sample_count = 0usize;

    if cfg.max_selected >= 2 {
        let probe_len = rows.len().min(cfg.probe_rows);
        // Distributed across all TRAIN positions, no dependence on TEST.
        let probe: Vec<JointRow> = (0..probe_len)
            .map(|j| rows[j * rows.len() / probe_len])
            .collect();
        probe_sample_count = probe.len();
        let mut pair_scores = Vec::with_capacity(cfg.candidates * (cfg.candidates - 1) / 2);
        for a in 0..cfg.candidates {
            for b in a + 1..cfg.candidates {
                let bits = loo_bits(&probe, &[a, b], cfg.classes, cfg.alpha);
                pair_scores.push((bits, a, b));
                probed_pairs += 1;
            }
        }
        pair_scores.sort_by(|l, r| {
            l.0.total_cmp(&r.0)
                .then_with(|| l.1.cmp(&r.1))
                .then_with(|| l.2.cmp(&r.2))
        });

        let mut winning_pair = None;
        let mut pair_bits = loo_bits(rows, &[], cfg.classes, cfg.alpha);
        full_train_scores += 1;
        for &(_, a, b) in pair_scores.iter().take(cfg.pair_finalists) {
            let candidate_bits = loo_bits(rows, &[a, b], cfg.classes, cfg.alpha);
            full_train_scores += 1;
            if candidate_bits + EPSILON < pair_bits {
                pair_bits = candidate_bits;
                winning_pair = Some([a, b]);
            }
        }
        if let Some(pair) = winning_pair {
            let (ids, loss) = forward(rows, cfg, &pair, &mut full_train_scores);
            if loss + EPSILON < final_bits {
                final_ids = ids;
                final_bits = loss;
                origin = JointOrigin::PairSeed;
            }
        }
    }

    Ok(JointReport {
        greedy: JointModel::from_train(rows, greedy_ids, cfg),
        selected: JointModel::from_train(rows, final_ids, cfg),
        origin,
        greedy_train_loo_bits: greedy_bits,
        selected_train_loo_bits: final_bits,
        probed_pairs,
        full_train_scores,
        probe_sample_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(candidates: usize, classes: usize, max_selected: usize) -> JointConfig {
        JointConfig {
            candidates,
            classes,
            max_selected,
            probe_rows: 256,
            pair_finalists: 32,
            alpha: 0.5,
        }
    }

    fn xor_training() -> Vec<JointRow> {
        let mut rows = Vec::new();
        for _ in 0..64 {
            for x in 0..8u128 {
                rows.push(JointRow {
                    features: x,
                    label: ((x & 1) ^ ((x >> 1) & 1)) as u8,
                });
            }
        }
        rows
    }

    #[test]
    fn zero_marginal_xor_is_recovered_by_joint_pair_not_greedy() {
        let report = fit_joint(&xor_training(), cfg(3, 2, 2)).unwrap();
        assert!(report.greedy.selected_features().is_empty());
        assert_eq!(report.origin, JointOrigin::PairSeed);
        assert_eq!(report.selected.selected_features(), &[0, 1]);
        assert!(report.selected_train_loo_bits < report.greedy_train_loo_bits);
        assert_eq!(report.probed_pairs, 3);
        assert_eq!(report.selected.training_samples(), 512);
        for x in 0..8u128 {
            let p = report.selected.distribution(x).unwrap();
            let y = ((x & 1) ^ ((x >> 1) & 1)) as usize;
            assert!(p[y] > 0.95);
            assert!((p[0] + p[1] - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn no_signal_does_not_force_any_feature_into_the_bank() {
        let mut rows = Vec::new();
        for _ in 0..32 {
            for x in 0..8u128 {
                for label in 0..2u8 {
                    rows.push(JointRow { features: x, label });
                }
            }
        }
        let report = fit_joint(&rows, cfg(3, 2, 3)).unwrap();
        assert_eq!(report.origin, JointOrigin::Greedy);
        assert!(report.greedy.selected_features().is_empty());
        assert!(report.selected.selected_features().is_empty());
    }

    #[test]
    fn full_byte_posterior_is_normalized_and_non_mutating() {
        let rows = [
            JointRow { features: 0, label: 0 },
            JointRow { features: 1, label: 16 },
            JointRow { features: 2, label: 255 },
            JointRow { features: 3, label: 16 },
        ];
        let report = fit_joint(&rows, cfg(2, 256, 2)).unwrap();
        let snapshot = report.clone();
        for bits in 0..4 {
            let p = report.selected.distribution(bits).unwrap();
            assert_eq!(p.len(), 256);
            assert!(p.iter().all(|v| *v > 0.0 && v.is_finite()));
            assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        }
        assert_eq!(report, snapshot);
    }

    #[test]
    fn rejects_bad_budgets_and_outside_values() {
        let train = xor_training();
        assert_eq!(fit_joint(&[], cfg(3, 2, 2)), Err(JointError::EmptyTrain));
        let mut invalid = cfg(3, 2, 2);
        invalid.alpha = f64::NAN;
        assert_eq!(fit_joint(&train, invalid), Err(JointError::InvalidConfig));
        let mut invalid = cfg(3, 2, 2);
        invalid.max_selected = 9;
        assert_eq!(fit_joint(&train, invalid), Err(JointError::InvalidConfig));
        assert_eq!(
            fit_joint(&[JointRow { features: 8, label: 0 }], cfg(3, 2, 2)),
            Err(JointError::FeatureOutsideDomain)
        );
        assert_eq!(
            fit_joint(&[JointRow { features: 0, label: 2 }], cfg(3, 2, 2)),
            Err(JointError::TargetOutsideDomain)
        );
        let fitted = fit_joint(&train, cfg(3, 2, 2)).unwrap();
        assert_eq!(
            fitted.selected.distribution(8),
            Err(JointError::InvalidPrediction)
        );
        assert_eq!(fit_joint(&train, cfg(3, 2, 2)), Ok(fitted));
    }
}
