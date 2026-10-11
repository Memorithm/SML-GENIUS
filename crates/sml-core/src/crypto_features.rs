//! A6 crypto benchmark: hourly bars, past-only features, forward labels,
//! train-only booleanization and walk-forward folds.
//!
//! No-lookahead contract:
//! * the feature row of bar `t` is computed from bars `0..=t` only (tested by
//!   recomputing on truncated series);
//! * labels use bars `t+1..=t+h` and are only used for training rows whose
//!   label is fully realised before the test period starts (embargo `h`);
//! * booleanization thresholds are quantiles of the training rows only.

/// One hourly OHLCV bar (open time in milliseconds since the epoch).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    pub time_ms: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

pub const HOUR_MS: i64 = 3_600_000;

/// Parses `open_time_ms,open,high,low,close,volume` lines (header allowed).
pub fn parse_csv(text: &str) -> Result<Vec<Bar>, String> {
    let mut bars = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with("open_time") {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 6 {
            return Err(format!("line {}: expected 6 fields", line_no + 1));
        }
        let num = |i: usize| -> Result<f64, String> {
            fields[i]
                .parse::<f64>()
                .map_err(|e| format!("line {}: {e}", line_no + 1))
        };
        bars.push(Bar {
            time_ms: fields[0]
                .parse::<i64>()
                .map_err(|e| format!("line {}: {e}", line_no + 1))?,
            open: num(1)?,
            high: num(2)?,
            low: num(3)?,
            close: num(4)?,
            volume: num(5)?,
        });
    }
    Ok(bars)
}

/// Inserts a flat zero-volume bar for every missing hour (exchange outages), so
/// one bar is exactly one hour. Returns the filled series and the number of
/// inserted bars.
pub fn fill_gaps(bars: &[Bar]) -> (Vec<Bar>, usize) {
    let mut out: Vec<Bar> = Vec::with_capacity(bars.len());
    let mut filled = 0;
    for &bar in bars {
        if let Some(&last) = out.last() {
            let mut t = last.time_ms + HOUR_MS;
            while t < bar.time_ms {
                out.push(Bar {
                    time_ms: t,
                    open: last.close,
                    high: last.close,
                    low: last.close,
                    close: last.close,
                    volume: 0.0,
                });
                filled += 1;
                t += HOUR_MS;
            }
            if bar.time_ms < t {
                continue;
            }
        }
        out.push(bar);
    }
    (out, filled)
}

pub const FEATURES: usize = 13;
pub const FEATURE_NAMES: [&str; FEATURES] = [
    "ret_1h",
    "ret_4h",
    "ret_24h",
    "ret_72h",
    "ret_168h",
    "vol_24h",
    "vol_168h",
    "vol_ratio_24/168",
    "volume_z_168h",
    "close/sma24-1",
    "close/sma168-1",
    "sma24/sma168-1",
    "range_pos_168h",
];

/// First bar with a complete feature row (needs 168 past bars).
pub const WARMUP: usize = 169;

fn mean_std(values: impl Iterator<Item = f64> + Clone) -> (f64, f64) {
    let n = values.clone().count() as f64;
    let mean = values.clone().sum::<f64>() / n;
    let var = values.map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
    (mean, var.sqrt())
}

/// Feature row of bar `t`, computed from `bars[..=t]` only (`None` during warm-up).
pub fn feature_row(bars: &[Bar], t: usize) -> Option<[f64; FEATURES]> {
    if t < WARMUP - 1 || t >= bars.len() {
        return None;
    }
    let c = |i: usize| bars[i].close;
    let lr = |a: usize, b: usize| (c(b) / c(a)).ln();
    let r1 = |i: usize| lr(i - 1, i);
    let (_, vol24) = mean_std((t - 23..=t).map(r1));
    let (_, vol168) = mean_std((t - 167..=t).map(r1));
    let logv = |i: usize| (bars[i].volume + 1.0).ln();
    let (mv, sv) = mean_std((t - 167..=t).map(logv));
    let sma = |w: usize| (t + 1 - w..=t).map(c).sum::<f64>() / w as f64;
    let (sma24, sma168) = (sma(24), sma(168));
    let lo = (t - 167..=t)
        .map(|i| bars[i].low)
        .fold(f64::INFINITY, f64::min);
    let hi = (t - 167..=t)
        .map(|i| bars[i].high)
        .fold(f64::NEG_INFINITY, f64::max);
    Some([
        r1(t),
        lr(t - 4, t),
        lr(t - 24, t),
        lr(t - 72, t),
        lr(t - 168, t),
        vol24,
        vol168,
        if vol168 > 0.0 { vol24 / vol168 } else { 1.0 },
        if sv > 0.0 { (logv(t) - mv) / sv } else { 0.0 },
        c(t) / sma24 - 1.0,
        c(t) / sma168 - 1.0,
        sma24 / sma168 - 1.0,
        if hi > lo {
            (c(t) - lo) / (hi - lo)
        } else {
            0.5
        },
    ])
}

/// Feature rows of every bar.
pub fn compute_features(bars: &[Bar]) -> Vec<Option<[f64; FEATURES]>> {
    (0..bars.len()).map(|t| feature_row(bars, t)).collect()
}

/// Forward simple return `close[t+h] / close[t] - 1` (`None` past the end).
pub fn forward_return(bars: &[Bar], t: usize, h: usize) -> Option<f64> {
    bars.get(t + h).map(|b| b.close / bars[t].close - 1.0)
}

/// Decision task: which label is predicted (1 = be exposed / long).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    /// (A) risk-on unless the forward 24 h return is below `-0.5 * vol_168h * sqrt(24)`.
    RiskOff,
    /// (B) sign of the forward return over `h` hours.
    Direction(usize),
}

impl Task {
    pub fn horizon(self) -> usize {
        match self {
            Self::RiskOff => 24,
            Self::Direction(h) => h,
        }
    }
    pub fn name(self) -> String {
        match self {
            Self::RiskOff => "A: risk-off filter (H=24h, drop < -0.5 sigma)".into(),
            Self::Direction(h) => format!("B: direction, h={h}h"),
        }
    }
    /// Label of bar `t` (needs bars up to `t + horizon`).
    pub fn label(self, bars: &[Bar], features: &[f64; FEATURES], t: usize) -> Option<bool> {
        let fwd = forward_return(bars, t, self.horizon())?;
        Some(match self {
            Self::RiskOff => fwd >= -0.5 * features[6] * (24f64).sqrt(),
            Self::Direction(_) => fwd > 0.0,
        })
    }
}

/// One walk-forward fold: train rows `train`, test rows `test` (bar indices).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fold {
    pub name: String,
    pub train: core::ops::Range<usize>,
    pub test: core::ops::Range<usize>,
}

/// Civil date from days since 1970-01-01 (proleptic Gregorian).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Days since 1970-01-01 of a civil date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Expanding-window quarterly folds. Test quarter `[start, end)` of bars; train
/// rows are `WARMUP-1 .. start - horizon` so every training label is realised
/// before the test period (embargo = label horizon). The last test row keeps
/// one following bar for its return.
pub fn quarterly_folds(bars: &[Bar], first_year: i64, last_year: i64, horizon: usize) -> Vec<Fold> {
    let index_of = |ms: i64| bars.partition_point(|b| b.time_ms < ms);
    let mut folds = Vec::new();
    for year in first_year..=last_year {
        for q in 0..4u32 {
            let start_ms = days_from_civil(year, 3 * q + 1, 1) * 86_400_000;
            let end_ms = if q == 3 {
                days_from_civil(year + 1, 1, 1)
            } else {
                days_from_civil(year, 3 * q + 4, 1)
            } * 86_400_000;
            let start = index_of(start_ms);
            let end = index_of(end_ms).min(bars.len().saturating_sub(1));
            if start >= end || start < WARMUP + horizon {
                continue;
            }
            folds.push(Fold {
                name: format!("{year}Q{}", q + 1),
                train: WARMUP - 1..start - horizon,
                test: start..end,
            });
        }
    }
    folds
}

/// Thermometer booleanization: bit `4f + j` is `feature_f > threshold_{f,j}`,
/// thresholds at the 20/40/60/80 % quantiles of the training rows.
#[derive(Debug, Clone, PartialEq)]
pub struct Booleanizer {
    pub thresholds: Vec<[f64; 4]>,
}

pub const QUANTILES: [f64; 4] = [0.2, 0.4, 0.6, 0.8];
pub const BITS: usize = FEATURES * 4;

fn quantile(sorted: &[f64], q: f64) -> f64 {
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

impl Booleanizer {
    /// Fits thresholds on `rows` (the training rows only).
    pub fn fit(rows: &[[f64; FEATURES]]) -> Self {
        let thresholds = (0..FEATURES)
            .map(|f| {
                let mut values: Vec<f64> = rows.iter().map(|r| r[f]).collect();
                values.sort_by(f64::total_cmp);
                QUANTILES.map(|q| quantile(&values, q))
            })
            .collect();
        Self { thresholds }
    }
    pub fn bits(&self, row: &[f64; FEATURES]) -> [bool; BITS] {
        let mut out = [false; BITS];
        for f in 0..FEATURES {
            for j in 0..4 {
                out[4 * f + j] = row[f] > self.thresholds[f][j];
            }
        }
        out
    }
    /// Human-readable name of bit `i`.
    pub fn bit_name(&self, i: usize) -> String {
        let (f, j) = (i / 4, i % 4);
        format!(
            "{} > {:.5} (train q{:.0})",
            FEATURE_NAMES[f],
            self.thresholds[f][j],
            QUANTILES[j] * 100.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toy_tasks::SplitMix64;

    pub(crate) fn synthetic_bars(n: usize, seed: u64) -> Vec<Bar> {
        let mut rng = SplitMix64::new(seed);
        let mut price = 100.0;
        (0..n)
            .map(|i| {
                let r = 0.01 * f64::from(rng.next_signed_unit());
                let open = price;
                price *= (r).exp();
                Bar {
                    time_ms: 1_546_300_800_000 + i as i64 * HOUR_MS,
                    open,
                    high: open.max(price) * 1.001,
                    low: open.min(price) * 0.999,
                    close: price,
                    volume: 10.0 + 5.0 * f64::from(rng.next_signed_unit()),
                }
            })
            .collect()
    }

    #[test]
    fn features_use_only_past_bars() {
        let bars = synthetic_bars(600, 1);
        let full = compute_features(&bars);
        for t in [WARMUP - 2, WARMUP - 1, 300, 599] {
            // Recompute on the series truncated at t: identical row.
            assert_eq!(feature_row(&bars[..=t], t), full[t]);
            // Changing every future bar cannot change the row.
            let mut altered = bars.clone();
            for b in altered.iter_mut().skip(t + 1) {
                b.close *= 3.0;
                b.volume = 1e9;
            }
            assert_eq!(feature_row(&altered, t), full[t]);
        }
        assert!(full[WARMUP - 2].is_none() && full[WARMUP - 1].is_some());
    }

    #[test]
    fn folds_embargo_the_label_horizon_and_do_not_overlap() {
        let bars = synthetic_bars(24 * 900, 2);
        for h in [1usize, 4, 24] {
            let folds = quarterly_folds(&bars, 2019, 2021, h);
            assert!(folds.len() >= 6);
            let mut last_end = 0;
            for fold in &folds {
                assert!(fold.test.start >= last_end);
                last_end = fold.test.end;
                // Every training label (t + h) is strictly before the test start.
                assert!(fold.train.end - 1 + h < fold.test.start);
                assert_eq!(fold.train.start, WARMUP - 1);
                let (y, m, d) =
                    civil_from_days(bars[fold.test.start].time_ms.div_euclid(86_400_000));
                assert!(d == 1 && [1, 4, 7, 10].contains(&m) && y >= 2019);
            }
        }
    }

    #[test]
    fn booleanizer_is_fitted_on_training_rows_only() {
        let bars = synthetic_bars(2000, 3);
        let rows: Vec<[f64; FEATURES]> = compute_features(&bars).into_iter().flatten().collect();
        let (train, test) = rows.split_at(1000);
        let fitted = Booleanizer::fit(train);
        let mut wild_test = test.to_vec();
        wild_test
            .iter_mut()
            .for_each(|r| r.iter_mut().for_each(|v| *v *= 100.0));
        // Thresholds depend on train rows only.
        assert_eq!(fitted, Booleanizer::fit(train));
        let combined: Vec<[f64; FEATURES]> = train.iter().chain(&wild_test).copied().collect();
        assert_ne!(fitted, Booleanizer::fit(&combined));
        // About 20 % of train rows are above the 80 % threshold.
        let above = train.iter().filter(|r| fitted.bits(r)[4 * 2 + 3]).count();
        assert!((180..=220).contains(&above), "{above}");
    }

    #[test]
    fn gaps_are_filled_with_flat_bars_and_calendar_round_trips() {
        let mut bars = synthetic_bars(10, 4);
        bars.remove(5);
        bars.remove(5);
        let (filled, count) = fill_gaps(&bars);
        assert_eq!((filled.len(), count), (10, 2));
        assert_eq!(filled[5].close, filled[4].close);
        assert_eq!(filled[6].volume, 0.0);
        for days in [0i64, 17_897, 20_454, -1] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(civil_from_days(17_897), (2019, 1, 1));
    }
}
