//! A6: crypto decision benchmark (risk-off filter A vs direction signal B),
//! walk-forward out-of-sample, with costs.
//!
//! Usage: `sml-crypto-bench <data-dir> [random-trials]` (default 300). Fetch
//! data with `scripts/fetch-crypto-data.sh`. Prints markdown tables; the
//! configuration below is fixed in advance (no tuning on test results).

use std::collections::BTreeMap;

use sml_core::crypto_features::{
    compute_features, fill_gaps, parse_csv, quarterly_folds, Bar, Booleanizer, Task, BITS,
    FEATURES, FEATURE_NAMES,
};
use sml_core::crypto_strategy::{
    metrics, random_signal_p_value, strategy_returns, AnfDecision, DecisionTree, Logistic,
    ANF_MAX_FAN_IN,
};

const ASSETS: [&str; 2] = ["BTCUSDT", "ETHUSDT"];
const TASKS: [Task; 4] = [
    Task::RiskOff,
    Task::Direction(1),
    Task::Direction(4),
    Task::Direction(24),
];
const COSTS_BPS: [f64; 3] = [0.0, 10.0, 20.0];
const FIRST_TEST_YEAR: i64 = 2021;
const LAST_TEST_YEAR: i64 = 2025;
const MODELS: [&str; 10] = [
    "buy & hold",
    "always flat",
    "majority class (train)",
    "rule: close > SMA168",
    "rule: SMA24 > SMA168",
    "rule: momentum ret_24h > 0",
    "rule: vol filter vol_24h <= train q80",
    "logistic regression",
    "decision tree (depth 4)",
    "ANF decision circuit",
];

#[derive(Default)]
struct Series {
    positions: Vec<f64>,
    long_short: Vec<f64>,
    /// Post-hoc variant: decide every `h` hours (from the fold start) and hold.
    held: Vec<f64>,
    predictions: Vec<bool>,
    fold_returns: Vec<f64>,
}

fn pct(v: f64) -> String {
    format!("{:.1}", v * 100.0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let dir = args
        .get(1)
        .ok_or("usage: sml-crypto-bench <data-dir> [random-trials]")?;
    let trials: usize = args.get(2).map_or(Ok(300), |s| s.parse())?;
    println!(
        "Walk-forward: expanding train window from the first complete feature row, quarterly test folds {FIRST_TEST_YEAR}Q1..{LAST_TEST_YEAR}Q4, embargo = label horizon. Costs per side: 0 / 10 / 20 bps. Random-signal check: {trials} Markov signals with the same exposure and switching rate, Sharpe at 10 bps."
    );
    println!(
        "ANF: static decision circuit, greedy forward selection (leave-one-out log-loss) over {BITS} thermometer bits (13 features x 4 train quantiles), fan-in <= {ANF_MAX_FAN_IN}, exact truth table, no recurrence.\n"
    );
    let mut circuits = String::new();
    for asset in ASSETS {
        let text = std::fs::read_to_string(format!("{dir}/{asset}-1h.csv"))?;
        let raw = parse_csv(&text)?;
        let (bars, filled) = fill_gaps(&raw);
        let features = compute_features(&bars);
        let returns: Vec<f64> = (0..bars.len())
            .map(|t| {
                bars.get(t + 1)
                    .map_or(0.0, |b| b.close / bars[t].close - 1.0)
            })
            .collect();
        println!(
            "## {asset}: {} bars ({} raw + {filled} filled), {} .. {} (open times, UTC ms)\n",
            bars.len(),
            raw.len(),
            bars[0].time_ms,
            bars[bars.len() - 1].time_ms
        );
        for task in TASKS {
            run_task(
                asset,
                task,
                &bars,
                &features,
                &returns,
                trials,
                &mut circuits,
            )?;
        }
    }
    println!("## ANF decision circuits (last fold) and selection frequency\n");
    println!("```\n{circuits}```");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_task(
    asset: &str,
    task: Task,
    bars: &[Bar],
    features: &[Option<[f64; FEATURES]>],
    returns: &[f64],
    trials: usize,
    circuits: &mut String,
) -> Result<(), Box<dyn std::error::Error>> {
    let h = task.horizon();
    let folds = quarterly_folds(bars, FIRST_TEST_YEAR, LAST_TEST_YEAR, h);
    let mut series: Vec<Series> = MODELS.iter().map(|_| Series::default()).collect();
    let mut labels: Vec<Option<bool>> = Vec::new();
    let mut bar_returns: Vec<f64> = Vec::new();
    let mut selection: BTreeMap<String, usize> = BTreeMap::new();
    let mut last_circuit = String::new();
    for fold in &folds {
        let train_rows: Vec<usize> = fold
            .train
            .clone()
            .filter(|&t| features[t].is_some())
            .collect();
        let xr: Vec<[f64; FEATURES]> = train_rows.iter().map(|&t| features[t].unwrap()).collect();
        let y: Vec<bool> = train_rows
            .iter()
            .map(|&t| {
                task.label(bars, &features[t].unwrap(), t)
                    .expect("realised")
            })
            .collect();
        let booleanizer = Booleanizer::fit(&xr);
        let xb: Vec<[bool; BITS]> = xr.iter().map(|r| booleanizer.bits(r)).collect();
        let majority = 2 * y.iter().filter(|&&v| v).count() > y.len();
        let mut vols: Vec<f64> = xr.iter().map(|r| r[5]).collect();
        vols.sort_by(f64::total_cmp);
        let vol_q80 = vols[(vols.len() - 1) * 4 / 5];
        let logistic = Logistic::fit(&xr, &y);
        let tree = DecisionTree::fit(&xr, &y);
        let anf = AnfDecision::fit(&xb, &y, ANF_MAX_FAN_IN);
        for &i in &anf.selected {
            *selection
                .entry(FEATURE_NAMES[i / 4].to_string())
                .or_default() += 1;
        }
        last_circuit = format!(
            "{asset} / {} / fold {} (train rows {}, train base rate {:.3}):\n{}",
            task.name(),
            fold.name,
            y.len(),
            y.iter().filter(|&&v| v).count() as f64 / y.len() as f64,
            anf.describe(&|i| booleanizer.bit_name(i))
        );
        let start = bar_returns.len();
        for (k, t) in fold.test.clone().enumerate() {
            let row = features[t].expect("test rows are past warm-up");
            let bits = booleanizer.bits(&row);
            let decisions = [
                true,
                false,
                majority,
                row[10] > 0.0,
                row[11] > 0.0,
                row[2] > 0.0,
                row[5] <= vol_q80,
                logistic.predict(&row),
                tree.predict(&row),
                anf.predict(&bits),
            ];
            for (s, &d) in series.iter_mut().zip(&decisions) {
                let held = if k % h == 0 {
                    f64::from(u8::from(d))
                } else {
                    *s.held.last().expect("k > 0")
                };
                s.held.push(held);
                s.positions.push(f64::from(u8::from(d)));
                s.long_short.push(if d { 1.0 } else { -1.0 });
                s.predictions.push(d);
            }
            labels.push(task.label(bars, &row, t));
            bar_returns.push(returns[t]);
        }
        // Per-fold net return at 10 bps (position before the fold = previous fold's last).
        for s in series.iter_mut() {
            let prev = if start == 0 {
                0.0
            } else {
                s.positions[start - 1]
            };
            let net = strategy_returns(&s.positions[start..], &bar_returns[start..], 10.0, prev);
            s.fold_returns
                .push(net.iter().map(|r| 1.0 + r).product::<f64>() - 1.0);
        }
    }
    println!("### {asset} — {}\n", task.name());
    let labeled: Vec<(usize, bool)> = labels
        .iter()
        .enumerate()
        .filter_map(|(i, l)| l.map(|v| (i, v)))
        .collect();
    let positives = labeled.iter().filter(|(_, v)| *v).count();
    println!(
        "{} folds, {} out-of-sample hours, {} labelled; label base rate (y=1) {:.3}.\n",
        folds.len(),
        bar_returns.len(),
        labeled.len(),
        positives as f64 / labeled.len() as f64
    );
    println!("| Model | acc % | bal. acc % | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | CAGR 10 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | folds beating B&H (10 bps) | random-signal p (10 bps) |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    let bh_folds = series[0].fold_returns.clone();
    for (m, s) in series.iter().enumerate() {
        let (mut tp, mut tn, mut fp, mut f_n) = (0usize, 0usize, 0usize, 0usize);
        for &(i, v) in &labeled {
            match (s.predictions[i], v) {
                (true, true) => tp += 1,
                (false, false) => tn += 1,
                (true, false) => fp += 1,
                (false, true) => f_n += 1,
            }
        }
        let acc = (tp + tn) as f64 / labeled.len() as f64;
        let tpr = if tp + f_n > 0 {
            tp as f64 / (tp + f_n) as f64
        } else {
            0.0
        };
        let tnr = if tn + fp > 0 {
            tn as f64 / (tn + fp) as f64
        } else {
            0.0
        };
        let by_cost: Vec<_> = COSTS_BPS
            .iter()
            .map(|&c| metrics(&s.positions, &bar_returns, c))
            .collect();
        let m10 = by_cost[1];
        let wins = s
            .fold_returns
            .iter()
            .zip(&bh_folds)
            .filter(|(a, b)| a > b)
            .count();
        let p = random_signal_p_value(&s.positions, &bar_returns, 10.0, trials, 0xa6 + m as u64)
            .map_or("-".to_string(), |p| format!("{p:.3}"));
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {:.2} | {} | {:.0} | {} | {wins}/{} | {p} |",
            MODELS[m],
            pct(acc),
            pct((tpr + tnr) / 2.0),
            pct(by_cost[0].total_return),
            pct(by_cost[1].total_return),
            pct(by_cost[2].total_return),
            pct(m10.cagr),
            m10.sharpe,
            pct(m10.max_drawdown),
            m10.turnover_per_year,
            pct(m10.time_in_market),
            folds.len()
        );
    }
    if h > 1 {
        println!("\nPOST-HOC exploratory variant (added after the first run, not pre-declared): decision taken every {h} h from the fold start and held {h} h (non-overlapping horizon), long/flat:\n");
        println!("| Model | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | random-signal p (10 bps) |");
        println!("|---|---|---|---|---|---|---|---|---|");
        for m in [7usize, 8, 9] {
            let held = &series[m].held;
            let r: Vec<_> = COSTS_BPS
                .iter()
                .map(|&c| metrics(held, &bar_returns, c))
                .collect();
            let p = random_signal_p_value(held, &bar_returns, 10.0, trials, 0xb6 + m as u64)
                .map_or("-".to_string(), |p| format!("{p:.3}"));
            println!(
                "| {} | {} | {} | {} | {:.2} | {} | {:.0} | {} | {p} |",
                MODELS[m],
                pct(r[0].total_return),
                pct(r[1].total_return),
                pct(r[2].total_return),
                r[1].sharpe,
                pct(r[1].max_drawdown),
                r[1].turnover_per_year,
                pct(r[1].time_in_market)
            );
        }
    }
    if let Task::Direction(_) = task {
        println!("\nLong/short variant (position +1 / -1), 10 bps per side:\n");
        println!("| Model | total ret % | CAGR % | Sharpe | max DD % | turnover / yr |");
        println!("|---|---|---|---|---|---|");
        for m in [4usize, 5, 7, 8, 9] {
            let r = metrics(&series[m].long_short, &bar_returns, 10.0);
            println!(
                "| {} | {} | {} | {:.2} | {} | {:.0} |",
                MODELS[m],
                pct(r.total_return),
                pct(r.cagr),
                r.sharpe,
                pct(r.max_drawdown),
                r.turnover_per_year
            );
        }
    }
    println!();
    let mut freq: Vec<(String, usize)> = selection.into_iter().collect();
    freq.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    circuits.push_str(&last_circuit);
    circuits.push_str(&format!(
        "  selected-bit feature frequency over {} folds: {}\n\n",
        folds.len(),
        freq.iter()
            .map(|(f, c)| format!("{f}:{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    Ok(())
}
