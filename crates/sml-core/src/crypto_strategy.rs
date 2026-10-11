//! A6 crypto benchmark: decision models and cost-aware backtest.
//!
//! Models (all fitted on one walk-forward training window):
//! * [`AnfDecision`]: static boolean decision circuit. Its inputs are chosen
//!   by greedy forward selection over the booleanized features, on the
//!   leave-one-out log-loss (at most [`ANF_MAX_FAN_IN`] bits). The readout
//!   truth table is fitted exactly (majority per address; unseen addresses get
//!   the training majority) and stored as a [`LocalAnfGate`] (Möbius / ANF
//!   form). No recurrence.
//! * [`Logistic`]: L2 logistic regression on train-standardized raw features.
//! * [`DecisionTree`]: CART (Gini) on raw features, depth <= 4, min leaf 500.
//! * Rules and constant exposures (no fitting, except the vol-filter
//!   threshold, which is a train quantile).
//!
//! Backtest: the position `p_t` is decided at the close of bar `t` and earns
//! the simple return of bar `t + 1`. Cost per unit of position change is
//! `bps / 10_000` (one side = 1 unit; a long/short flip is 2 units).

use crate::anf_local::LocalAnfGate;
use crate::anf_local_incremental::{loss_term, LossKind};
use crate::crypto_features::{BITS, FEATURES};
use crate::toy_tasks::SplitMix64;

/// Maximum number of input bits of the ANF decision circuit (pre-declared).
pub const ANF_MAX_FAN_IN: usize = 8;

/// Static ANF decision circuit over selected boolean features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnfDecision {
    /// Selected input bits, in selection order (address bit j = `selected[j]`).
    pub selected: Vec<usize>,
    /// Train `[count(0), count(1)]` per address.
    pub counts: Vec<[u32; 2]>,
    /// Decision for unseen addresses (training majority).
    pub default: bool,
    /// The fitted truth table as an ANF gate over source positions `0..BITS`.
    pub gate: LocalAnfGate,
}

fn address(bits: &[bool; BITS], selected: &[usize]) -> usize {
    selected
        .iter()
        .enumerate()
        .fold(0, |a, (j, &i)| a | (usize::from(bits[i]) << j))
}

impl AnfDecision {
    pub fn fit(x: &[[bool; BITS]], y: &[bool], max_fan_in: usize) -> Self {
        let max_fan_in = max_fan_in.clamp(1, ANF_MAX_FAN_IN);
        let count = |selected: &[usize]| -> Vec<[u32; 2]> {
            let mut counts = vec![[0u32; 2]; 1 << selected.len()];
            for (row, &label) in x.iter().zip(y) {
                counts[address(row, selected)][usize::from(label)] += 1;
            }
            counts
        };
        let loss = |counts: &[[u32; 2]]| -> i64 {
            counts
                .iter()
                .map(|&c| loss_term(LossKind::LeaveOneOutBits, c))
                .sum()
        };
        let mut selected: Vec<usize> = Vec::new();
        let mut best = loss(&count(&selected));
        while selected.len() < max_fan_in {
            let mut round: Option<(i64, usize)> = None;
            for c in (0..BITS).filter(|c| !selected.contains(c)) {
                let mut trial = selected.clone();
                trial.push(c);
                let l = loss(&count(&trial));
                if round.is_none_or(|r| l < r.0) {
                    round = Some((l, c));
                }
            }
            match round {
                Some((l, c)) if l < best => {
                    best = l;
                    selected.push(c);
                }
                _ => break,
            }
        }
        let ones = y.iter().filter(|&&v| v).count();
        let default = 2 * ones > y.len();
        let (selected, counts) = if selected.is_empty() {
            // Constant circuit: read bit 0 with an identical decision on both addresses.
            let c = count(&[]);
            (vec![0], vec![c[0], c[0]])
        } else {
            let c = count(&selected);
            (selected, c)
        };
        let table: Vec<bool> = counts
            .iter()
            .map(|c| {
                if c[0] + c[1] == 0 {
                    default
                } else {
                    c[1] > c[0]
                }
            })
            .collect();
        let gate = LocalAnfGate::from_truth_table(selected.clone(), &table, BITS)
            .expect("fan-in <= 8 and distinct bits");
        Self {
            selected,
            counts,
            default,
            gate,
        }
    }

    /// Decision = ANF gate evaluated on the boolean feature vector.
    pub fn predict(&self, bits: &[bool; BITS]) -> bool {
        self.gate.evaluate(bits)
    }

    /// Human-readable circuit: inputs, truth table with train counts, ANF terms.
    pub fn describe(&self, bit_name: &dyn Fn(usize) -> String) -> String {
        let mut out = String::new();
        for (j, &i) in self.selected.iter().enumerate() {
            out.push_str(&format!("  u{j} = [{}]\n", bit_name(i)));
        }
        out.push_str(
            "  truth table (address u_{k-1}..u_0 -> decision, train counts [y=0, y=1]):\n",
        );
        let table = self.gate.truth_table();
        for (a, c) in self.counts.iter().enumerate() {
            let bits: String = (0..self.selected.len())
                .rev()
                .map(|j| if (a >> j) & 1 == 1 { '1' } else { '0' })
                .collect();
            out.push_str(&format!(
                "    {bits} -> {} [{}, {}]\n",
                u8::from(table[a]),
                c[0],
                c[1]
            ));
        }
        let terms: Vec<String> = (0..self.gate.monomial_count())
            .filter(|&m| self.gate.coefficient(m))
            .map(|m| {
                if m == 0 {
                    "1".to_string()
                } else {
                    (0..self.selected.len())
                        .filter(|j| (m >> j) & 1 == 1)
                        .map(|j| format!("u{j}"))
                        .collect::<Vec<_>>()
                        .join("·")
                }
            })
            .collect();
        out.push_str(&format!(
            "  ANF: decision = {}\n",
            if terms.is_empty() {
                "0".to_string()
            } else {
                terms.join(" ⊕ ")
            }
        ));
        out
    }
}

/// L2 logistic regression on standardized features (full-batch gradient descent).
#[derive(Debug, Clone, PartialEq)]
pub struct Logistic {
    mean: [f64; FEATURES],
    scale: [f64; FEATURES],
    weights: [f64; FEATURES],
    bias: f64,
}

impl Logistic {
    pub fn fit(x: &[[f64; FEATURES]], y: &[bool]) -> Self {
        let n = x.len() as f64;
        let mut mean = [0.0; FEATURES];
        let mut scale = [1.0; FEATURES];
        for f in 0..FEATURES {
            mean[f] = x.iter().map(|r| r[f]).sum::<f64>() / n;
            let var = x.iter().map(|r| (r[f] - mean[f]).powi(2)).sum::<f64>() / n;
            scale[f] = if var > 0.0 { var.sqrt() } else { 1.0 };
        }
        let mut model = Self {
            mean,
            scale,
            weights: [0.0; FEATURES],
            bias: 0.0,
        };
        let z: Vec<[f64; FEATURES]> = x.iter().map(|r| model.standardize(r)).collect();
        let (lr, l2) = (0.5, 1e-3);
        for _ in 0..300 {
            let mut gw = [0.0; FEATURES];
            let mut gb = 0.0;
            for (row, &label) in z.iter().zip(y) {
                let p = model.prob_standardized(row);
                let e = p - f64::from(u8::from(label));
                for (g, v) in gw.iter_mut().zip(row) {
                    *g += e * v;
                }
                gb += e;
            }
            for (w, g) in model.weights.iter_mut().zip(&gw) {
                *w -= lr * (g / n + l2 * *w);
            }
            model.bias -= lr * gb / n;
        }
        model
    }
    fn standardize(&self, row: &[f64; FEATURES]) -> [f64; FEATURES] {
        let mut z = [0.0; FEATURES];
        for f in 0..FEATURES {
            z[f] = ((row[f] - self.mean[f]) / self.scale[f]).clamp(-10.0, 10.0);
        }
        z
    }
    fn prob_standardized(&self, z: &[f64; FEATURES]) -> f64 {
        let s: f64 = self.bias + (0..FEATURES).map(|f| self.weights[f] * z[f]).sum::<f64>();
        1.0 / (1.0 + (-s).exp())
    }
    pub fn predict(&self, row: &[f64; FEATURES]) -> bool {
        self.prob_standardized(&self.standardize(row)) > 0.5
    }
}

/// CART decision tree (Gini), thresholds at 5 % train quantiles.
#[derive(Debug, Clone, PartialEq)]
pub enum DecisionTree {
    Leaf(bool),
    Split {
        feature: usize,
        threshold: f64,
        below: Box<DecisionTree>,
        above: Box<DecisionTree>,
    },
}

pub const TREE_MAX_DEPTH: usize = 4;
pub const TREE_MIN_LEAF: usize = 500;

impl DecisionTree {
    pub fn fit(x: &[[f64; FEATURES]], y: &[bool]) -> Self {
        let mut candidates = Vec::with_capacity(FEATURES);
        for f in 0..FEATURES {
            let mut values: Vec<f64> = x.iter().map(|r| r[f]).collect();
            values.sort_by(f64::total_cmp);
            let mut cuts: Vec<f64> = (1..20)
                .map(|q| values[(q * (values.len() - 1)) / 20])
                .collect();
            cuts.dedup();
            candidates.push(cuts);
        }
        let rows: Vec<usize> = (0..x.len()).collect();
        Self::grow(x, y, &rows, &candidates, 0)
    }
    fn grow(
        x: &[[f64; FEATURES]],
        y: &[bool],
        rows: &[usize],
        candidates: &[Vec<f64>],
        depth: usize,
    ) -> Self {
        let ones = rows.iter().filter(|&&i| y[i]).count();
        let majority = 2 * ones > rows.len();
        if depth >= TREE_MAX_DEPTH
            || rows.len() < 2 * TREE_MIN_LEAF
            || ones == 0
            || ones == rows.len()
        {
            return Self::Leaf(majority);
        }
        let gini = |n: usize, k: usize| -> f64 {
            if n == 0 {
                return 0.0;
            }
            let p = k as f64 / n as f64;
            n as f64 * 2.0 * p * (1.0 - p)
        };
        let parent = gini(rows.len(), ones);
        let mut best: Option<(f64, usize, f64)> = None;
        for (f, cuts) in candidates.iter().enumerate() {
            for &cut in cuts {
                let (mut n_below, mut k_below) = (0usize, 0usize);
                for &i in rows {
                    if x[i][f] <= cut {
                        n_below += 1;
                        k_below += usize::from(y[i]);
                    }
                }
                let n_above = rows.len() - n_below;
                if n_below < TREE_MIN_LEAF || n_above < TREE_MIN_LEAF {
                    continue;
                }
                let impurity = gini(n_below, k_below) + gini(n_above, ones - k_below);
                if best.is_none_or(|b| impurity < b.0) {
                    best = Some((impurity, f, cut));
                }
            }
        }
        match best {
            Some((impurity, feature, threshold)) if impurity < parent - 1e-9 => {
                let (below, above): (Vec<usize>, Vec<usize>) =
                    rows.iter().partition(|&&i| x[i][feature] <= threshold);
                let below = Self::grow(x, y, &below, candidates, depth + 1);
                let above = Self::grow(x, y, &above, candidates, depth + 1);
                if below == above {
                    return below;
                }
                Self::Split {
                    feature,
                    threshold,
                    below: Box::new(below),
                    above: Box::new(above),
                }
            }
            _ => Self::Leaf(majority),
        }
    }
    pub fn predict(&self, row: &[f64; FEATURES]) -> bool {
        match self {
            Self::Leaf(v) => *v,
            Self::Split {
                feature,
                threshold,
                below,
                above,
            } => {
                if row[*feature] <= *threshold {
                    below.predict(row)
                } else {
                    above.predict(row)
                }
            }
        }
    }
}

/// Strategy metrics of a position series.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrategyMetrics {
    pub hours: usize,
    pub total_return: f64,
    pub cagr: f64,
    pub sharpe: f64,
    pub max_drawdown: f64,
    /// Position change units per year.
    pub turnover_per_year: f64,
    /// Fraction of hours with a non-zero position.
    pub time_in_market: f64,
}

/// Net hourly strategy returns: `positions[i]` is held over `bar_returns[i]`;
/// `previous` is the position before the first one.
pub fn strategy_returns(
    positions: &[f64],
    bar_returns: &[f64],
    bps: f64,
    previous: f64,
) -> Vec<f64> {
    let cost = bps / 10_000.0;
    let mut prev = previous;
    positions
        .iter()
        .zip(bar_returns)
        .map(|(&p, &r)| {
            let net = p * r - cost * (p - prev).abs();
            prev = p;
            net
        })
        .collect()
}

pub fn metrics(positions: &[f64], bar_returns: &[f64], bps: f64) -> StrategyMetrics {
    let returns = strategy_returns(positions, bar_returns, bps, 0.0);
    let hours = returns.len();
    let n = hours as f64;
    let mut equity = 1.0f64;
    let mut peak = 1.0f64;
    let mut max_drawdown = 0.0f64;
    for &r in &returns {
        equity *= 1.0 + r;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max(1.0 - equity / peak);
    }
    let mean = returns.iter().sum::<f64>() / n;
    let var = returns.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / n;
    let mut prev = 0.0;
    let mut turnover = 0.0;
    for &p in positions {
        turnover += (p - prev).abs();
        prev = p;
    }
    StrategyMetrics {
        hours,
        total_return: equity - 1.0,
        cagr: equity.powf(8760.0 / n) - 1.0,
        sharpe: if var > 0.0 {
            mean / var.sqrt() * 8760f64.sqrt()
        } else {
            0.0
        },
        max_drawdown,
        turnover_per_year: turnover * 8760.0 / n,
        time_in_market: positions.iter().filter(|&&p| p != 0.0).count() as f64 / n,
    }
}

/// Annualized Sharpe of net returns.
pub fn sharpe(returns: &[f64]) -> f64 {
    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let var = returns.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / n;
    if var > 0.0 {
        mean / var.sqrt() * 8760f64.sqrt()
    } else {
        0.0
    }
}

/// Random long/flat signals with the same exposure and switching rate as
/// `positions` (two-state Markov chain); returns the fraction of `trials`
/// random Sharpe ratios (at `bps`) >= the strategy's Sharpe (one-sided p-value).
pub fn random_signal_p_value(
    positions: &[f64],
    bar_returns: &[f64],
    bps: f64,
    trials: usize,
    seed: u64,
) -> Option<f64> {
    let n = positions.len() as f64;
    let exposure = positions.iter().filter(|&&p| p != 0.0).count() as f64 / n;
    let switches = positions.windows(2).filter(|w| w[0] != w[1]).count() as f64 / n;
    if exposure <= 0.0 || exposure >= 1.0 || switches <= 0.0 {
        return None;
    }
    let total = (switches / (2.0 * exposure * (1.0 - exposure))).min(1.0);
    let (up, down) = (exposure * total, (1.0 - exposure) * total);
    let target = sharpe(&strategy_returns(positions, bar_returns, bps, 0.0));
    let mut rng = SplitMix64::new(seed);
    let unit = |rng: &mut SplitMix64| (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    let mut at_least = 0;
    let mut random = vec![0.0; positions.len()];
    for _ in 0..trials {
        let mut on = unit(&mut rng) < exposure;
        for p in random.iter_mut() {
            *p = f64::from(u8::from(on));
            let u = unit(&mut rng);
            on = if on { u >= down } else { u < up };
        }
        if sharpe(&strategy_returns(&random, bar_returns, bps, 0.0)) >= target {
            at_least += 1;
        }
    }
    Some(at_least as f64 / trials as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_accounting_matches_hand_computation() {
        let positions = [1.0, 1.0, 0.0, -1.0, 1.0];
        let returns = [0.01, -0.02, 0.03, 0.01, 0.02];
        let net = strategy_returns(&positions, &returns, 10.0, 0.0);
        let c = 0.001;
        let expected = [0.01 - c, -0.02, -c, -0.01 - c, 0.02 - 2.0 * c];
        for (a, b) in net.iter().zip(expected) {
            assert!((a - b).abs() < 1e-15, "{a} vs {b}");
        }
        let m = metrics(&positions, &returns, 10.0);
        // Turnover: 1 (enter) + 1 (exit) + 1 (short) + 2 (flip) = 5 units over 5 hours.
        assert!((m.turnover_per_year - 5.0 * 8760.0 / 5.0).abs() < 1e-9);
        assert!((m.time_in_market - 0.8).abs() < 1e-12);
        let equity: f64 = expected.iter().map(|r| 1.0 + r).product();
        assert!((m.total_return - (equity - 1.0)).abs() < 1e-12);
        // Zero cost: buy & hold equals the compounded asset return.
        let bh = metrics(&[1.0; 5], &returns, 0.0);
        let asset: f64 = returns.iter().map(|r| 1.0 + r).product();
        assert!((bh.total_return - (asset - 1.0)).abs() < 1e-12);
        assert!(bh.max_drawdown > 0.019 && bh.max_drawdown < 0.021);
    }

    #[test]
    fn anf_decision_recovers_a_boolean_rule_and_is_its_anf() {
        let mut rng = SplitMix64::new(5);
        let x: Vec<[bool; BITS]> = (0..4000)
            .map(|_| {
                let mut row = [false; BITS];
                row.iter_mut().for_each(|b| *b = rng.next_bool());
                row
            })
            .collect();
        // y = b3 AND NOT b17, with 5 % label noise.
        let y: Vec<bool> = x
            .iter()
            .map(|r| (r[3] && !r[17]) ^ (rng.below(20) == 0))
            .collect();
        let model = AnfDecision::fit(&x, &y, 8);
        let mut first_two = model.selected[..2].to_vec();
        first_two.sort_unstable();
        assert_eq!(first_two, vec![3, 17]);
        for row in x.iter().take(500) {
            assert_eq!(model.predict(row), row[3] && !row[17]);
        }
        let text = model.describe(&|i| format!("bit{i}"));
        assert!(text.contains("ANF: decision ="));
    }

    #[test]
    fn logistic_and_tree_learn_a_threshold_rule() {
        let mut rng = SplitMix64::new(6);
        let x: Vec<[f64; FEATURES]> = (0..3000)
            .map(|_| {
                let mut r = [0.0; FEATURES];
                r.iter_mut()
                    .for_each(|v| *v = f64::from(rng.next_signed_unit()));
                r
            })
            .collect();
        let y: Vec<bool> = x.iter().map(|r| r[2] > 0.1).collect();
        let logistic = Logistic::fit(&x, &y);
        let tree = DecisionTree::fit(&x, &y);
        let acc = |f: &dyn Fn(&[f64; FEATURES]) -> bool| {
            x.iter().zip(&y).filter(|(r, &l)| f(r) == l).count() as f64 / x.len() as f64
        };
        assert!(acc(&|r| logistic.predict(r)) > 0.95);
        assert!(acc(&|r| tree.predict(r)) > 0.95);
    }

    #[test]
    fn random_signals_match_exposure_and_give_a_valid_p_value() {
        let returns: Vec<f64> = (0..5000)
            .map(|i| if i % 2 == 0 { 0.01 } else { -0.009 })
            .collect();
        // Perfect foresight long/flat: p-value 0.
        let oracle: Vec<f64> = returns
            .iter()
            .map(|&r| f64::from(u8::from(r > 0.0)))
            .collect();
        assert_eq!(
            random_signal_p_value(&oracle, &returns, 0.0, 50, 1),
            Some(0.0)
        );
        // Always-in has no random counterpart.
        assert_eq!(
            random_signal_p_value(&[1.0; 5000], &returns, 0.0, 50, 1),
            None
        );
    }
}
