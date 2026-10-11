# A6: crypto decision benchmark (ANF decision circuit vs baselines)

Diagnostic results only. **This is not a trading system and nothing here should
be traded.** Every number below is copied from the output of
`sml-crypto-bench` (the full output, including every per-fold ANF circuit, was
kept outside the repository).

## Summary

- Out of sample (20 quarterly walk-forward folds, 2021Q1..2025Q4, 43,823 hours
  per asset), **no model beats buy & hold after costs** on BTCUSDT or
  ETHUSDT. This includes the ANF decision circuit, logistic regression, a
  depth-4 tree and the simple rules.
- **A (risk-off filter)**: the label is "on" ~74–75% of the time. Logistic
  regression and the tree collapse to "always in" (= buy & hold). The ANF
  circuit stays in ~99% of the time and loses 55–182 points of total return to
  bad exits and costs. There is no exploitable timing signal in these features.
- **B (direction)**: at h=1h and h=4h, every learned model has a real but small
  classification edge (balanced accuracy 52.1–53.3% vs 50%), mostly short-term
  reversal. Gross of costs it is worth a lot (ANF BTC h=4: +555% vs +202% buy &
  hold). It turns over 1,600–3,400 times per year, though, so it goes to −100%
  at 10 bps per side. At h=24h the edge is ~0.2–1.5 points and also dies after
  costs.
- The ANF circuit is as accurate as logistic regression and the tree here, and
  its decisions can be read exactly. It is **not** better.
- The lowest random-signal p-values (0.007–0.017) do not survive multiple
  testing (~130 evaluated configurations).

## Data

- Source: Binance public data dumps (`data.binance.vision`), spot monthly 1h
  klines, BTCUSDT and ETHUSDT.
- Range: 2019-01 .. 2025-12, i.e. open times 2019-01-01 00:00 UTC ..
  2025-12-31 23:00 UTC (84 monthly files per symbol).
- Fetched by `scripts/fetch-crypto-data.sh`. The script:
  - checks each zip against its published `.CHECKSUM`;
  - normalizes the 2025 microsecond timestamps to milliseconds;
  - pins the SHA-256 of the merged CSV:
    - BTCUSDT `41c9fc96e7544734a478859003c98dafc855568811d75dfbafe32045c48e396d`;
    - ETHUSDT `5025660b7735fc163ad0d4d4e4557665e55056e0fb78ea0690893354e94cd025`.
- Output goes to `data/crypto/`, which is git-ignored. No market data is
  committed.
- 61,309 raw hourly bars per symbol, plus 59 missing hours (exchange
  outages). The missing hours are filled with flat, zero-volume bars so that
  one row is exactly one hour, giving 61,368 bars.

## Methodology (pre-declared configuration)

The configuration space was fixed before the first run and was not tuned.

- **Features** (13, past-only; row `t` uses bars `<= t` only; warm-up 169
  bars):
  - log returns over 1/4/24/72/168h;
  - realized volatility over 24h and 168h, and their ratio;
  - volume z-score over 168h;
  - close/SMA24 − 1, close/SMA168 − 1, SMA24/SMA168 − 1;
  - position in the 168h high-low range.
- **Tasks:**
  - **A (risk-off)**: y = 1 ("stay in") iff the forward 24h log return is
    >= −0.5 · vol_168h · √24.
  - **B (direction)**: y = 1 iff the forward h-hour return is > 0, for h ∈
    {1, 4, 24}.
- **Walk-forward**:
  - expanding train window;
  - quarterly test folds 2021Q1..2025Q4 (20 folds);
  - training rows end at `test_start − h`: an embargo equal to the label
    horizon, so no training label overlaps the test period.
  - Only the concatenated out-of-sample predictions are reported.
- **Booleanization** (ANF only): thermometer bits at the 20/40/60/80% quantiles
  of each feature, **fitted on the training rows of the fold only** (52 bits).
- **Models:**
  - buy & hold;
  - always flat;
  - majority class of the training set;
  - rules (no fitting, except the vol-filter quantile on train):
    - close > SMA168;
    - SMA24 > SMA168;
    - ret_24h > 0;
    - vol_24h <= train q80.
  - logistic regression: L2, gradient descent, standardized features;
  - CART decision tree: Gini, depth 4, min leaf 500;
  - **ANF decision circuit**:
    - greedy forward selection of at most 8 bits, using the leave-one-out
      log-loss of the exact count table;
    - exact majority truth table (unseen addresses take the train majority);
    - stored as a `LocalAnfGate`, so its algebraic normal form is printed.
- **Backtest:**
  - the position decided at the close of hour t earns the return of hour t+1;
  - cost = bps × |Δposition| per side, at 0 / 10 / 20 bps;
  - Sharpe annualized with √8760; turnover = Σ|Δposition| per year.
  - Default is long/flat. B also reports long/short (±1) at 10 bps.
- **Significance**:
  - 300 random two-state Markov signals with the same exposure and switching
    rate as the model;
  - p = share of random Sharpe ratios (10 bps) >= the model's Sharpe.
- **Post-hoc variant**:
  - added after seeing the first run, so explicitly **not pre-declared**;
  - decide every h hours from the fold start and hold for h hours. This
    removes the overlapping-label churn.
- Tests (in `crypto_features.rs` / `crypto_strategy.rs`):
  - no-lookahead of the features (future bars mutated → past rows unchanged);
  - fold embargo and non-overlap;
  - booleanizer fitted on train rows only;
  - cost accounting against a hand computation;
  - the ANF circuit recovers a known boolean rule and its ANF;
  - logistic/tree learn a threshold rule;
  - random-signal exposure matching.

Reproduce:

```
bash scripts/fetch-crypto-data.sh            # -> data/crypto/ (git-ignored)
cargo run --release -p sml-core --bin sml-crypto-bench -- data/crypto 300
```

## Results (out of sample, all 20 folds concatenated)

### BTCUSDT: 61368 bars (61309 raw + 59 filled), 1546300800000 .. 1767222000000 (open times, UTC ms)

#### BTCUSDT — A: risk-off filter (H=24h, drop < -0.5 sigma)

20 folds, 43823 out-of-sample hours, 43800 labelled; label base rate (y=1) 0.747.

| Model | acc % | bal. acc % | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | CAGR 10 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | folds beating B&H (10 bps) | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| buy & hold | 74.7 | 50.0 | 202.3 | 202.0 | 201.7 | 24.7 | 0.67 | 77.2 | 0 | 100.0 | 0/20 | - |
| always flat | 25.3 | 50.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.00 | 0.0 | 0 | 0.0 | 9/20 | - |
| majority class (train) | 74.7 | 50.0 | 202.3 | 202.0 | 201.7 | 24.7 | 0.67 | 77.2 | 0 | 100.0 | 0/20 | - |
| rule: close > SMA168 | 49.9 | 49.0 | 93.8 | -61.4 | -92.3 | -17.3 | -0.28 | 78.5 | 322 | 51.4 | 4/20 | 0.513 |
| rule: SMA24 > SMA168 | 50.9 | 50.2 | 118.9 | 43.6 | -5.8 | 7.5 | 0.38 | 67.9 | 84 | 51.6 | 7/20 | 0.357 |
| rule: momentum ret_24h > 0 | 50.3 | 49.5 | 1.9 | -98.8 | -100.0 | -58.9 | -1.99 | 99.2 | 893 | 51.4 | 2/20 | 0.923 |
| rule: vol filter vol_24h <= train q80 | 67.0 | 48.7 | 20.7 | -29.9 | -59.3 | -6.9 | 0.08 | 79.7 | 109 | 86.3 | 5/20 | 1.000 |
| logistic regression | 74.7 | 50.0 | 202.3 | 202.0 | 201.7 | 24.7 | 0.67 | 77.2 | 0 | 100.0 | 0/20 | - |
| decision tree (depth 4) | 74.7 | 50.0 | 202.3 | 202.0 | 201.7 | 24.7 | 0.67 | 77.2 | 0 | 100.0 | 0/20 | - |
| ANF decision circuit | 74.4 | 49.9 | 186.0 | 146.9 | 113.1 | 19.8 | 0.60 | 77.6 | 29 | 99.5 | 5/20 | 0.710 |

POST-HOC exploratory variant (added after the first run, not pre-declared): decision taken every 24 h from the fold start and held 24 h (non-overlapping horizon), long/flat:

| Model | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|
| logistic regression | 202.3 | 202.0 | 201.7 | 0.67 | 77.2 | 0 | 100.0 | - |
| decision tree (depth 4) | 202.3 | 202.0 | 201.7 | 0.67 | 77.2 | 0 | 100.0 | - |
| ANF decision circuit | 186.8 | 178.6 | 170.7 | 0.64 | 78.6 | 6 | 99.2 | 0.663 |

#### BTCUSDT — B: direction, h=1h

20 folds, 43823 out-of-sample hours, 43823 labelled; label base rate (y=1) 0.506.

| Model | acc % | bal. acc % | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | CAGR 10 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | folds beating B&H (10 bps) | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| buy & hold | 50.6 | 50.0 | 202.3 | 202.0 | 201.7 | 24.7 | 0.67 | 77.2 | 0 | 100.0 | 0/20 | - |
| always flat | 49.4 | 50.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.00 | 0.0 | 0 | 0.0 | 9/20 | - |
| majority class (train) | 50.6 | 50.0 | 202.3 | 202.0 | 201.7 | 24.7 | 0.67 | 77.2 | 0 | 100.0 | 0/20 | - |
| rule: close > SMA168 | 49.3 | 49.3 | 93.8 | -61.4 | -92.3 | -17.3 | -0.28 | 78.5 | 322 | 51.4 | 4/20 | 0.513 |
| rule: SMA24 > SMA168 | 50.2 | 50.1 | 118.9 | 43.6 | -5.8 | 7.5 | 0.38 | 67.9 | 84 | 51.6 | 7/20 | 0.357 |
| rule: momentum ret_24h > 0 | 47.7 | 47.6 | 1.9 | -98.8 | -100.0 | -58.9 | -1.99 | 99.2 | 893 | 51.4 | 2/20 | 0.923 |
| rule: vol filter vol_24h <= train q80 | 50.2 | 49.8 | 19.2 | -30.6 | -59.7 | -7.1 | 0.07 | 79.7 | 108 | 86.3 | 5/20 | 1.000 |
| logistic regression | 53.1 | 53.0 | 144.2 | -100.0 | -100.0 | -92.9 | -5.31 | 100.0 | 2819 | 60.9 | 0/20 | 0.240 |
| decision tree (depth 4) | 53.1 | 53.0 | 234.9 | -100.0 | -100.0 | -93.9 | -5.62 | 100.0 | 3036 | 58.4 | 0/20 | 0.047 |
| ANF decision circuit | 53.3 | 53.2 | 97.1 | -100.0 | -100.0 | -96.2 | -7.07 | 100.0 | 3414 | 51.3 | 0/20 | 0.123 |

Long/short variant (position +1 / -1), 10 bps per side:

| Model | total ret % | CAGR % | Sharpe | max DD % | turnover / yr |
|---|---|---|---|---|---|
| rule: SMA24 > SMA168 | -74.7 | -24.0 | -0.16 | 84.9 | 168 |
| rule: momentum ret_24h > 0 | -100.0 | -88.9 | -3.35 | 100.0 | 1786 |
| logistic regression | -100.0 | -99.6 | -9.00 | 100.0 | 5637 |
| decision tree (depth 4) | -100.0 | -99.7 | -9.49 | 100.0 | 6072 |
| ANF decision circuit | -100.0 | -99.9 | -11.12 | 100.0 | 6829 |

#### BTCUSDT — B: direction, h=4h

20 folds, 43823 out-of-sample hours, 43820 labelled; label base rate (y=1) 0.508.

| Model | acc % | bal. acc % | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | CAGR 10 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | folds beating B&H (10 bps) | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| buy & hold | 50.8 | 50.0 | 202.3 | 202.0 | 201.7 | 24.7 | 0.67 | 77.2 | 0 | 100.0 | 0/20 | - |
| always flat | 49.2 | 50.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.00 | 0.0 | 0 | 0.0 | 9/20 | - |
| majority class (train) | 50.8 | 50.0 | 202.3 | 202.0 | 201.7 | 24.7 | 0.67 | 77.2 | 0 | 100.0 | 0/20 | - |
| rule: close > SMA168 | 48.7 | 48.7 | 93.8 | -61.4 | -92.3 | -17.3 | -0.28 | 78.5 | 322 | 51.4 | 4/20 | 0.513 |
| rule: SMA24 > SMA168 | 49.6 | 49.6 | 118.9 | 43.6 | -5.8 | 7.5 | 0.38 | 67.9 | 84 | 51.6 | 7/20 | 0.357 |
| rule: momentum ret_24h > 0 | 46.7 | 46.7 | 1.9 | -98.8 | -100.0 | -58.9 | -1.99 | 99.2 | 893 | 51.4 | 2/20 | 0.923 |
| rule: vol filter vol_24h <= train q80 | 50.0 | 49.5 | 19.2 | -30.6 | -59.7 | -7.1 | 0.07 | 79.7 | 108 | 86.3 | 5/20 | 1.000 |
| logistic regression | 52.5 | 52.1 | 449.8 | -99.8 | -100.0 | -71.5 | -2.27 | 99.9 | 1596 | 71.6 | 2/20 | 0.100 |
| decision tree (depth 4) | 53.5 | 53.3 | 246.8 | -100.0 | -100.0 | -84.2 | -3.64 | 100.0 | 2094 | 61.6 | 1/20 | 0.177 |
| ANF decision circuit | 53.4 | 53.3 | 555.5 | -100.0 | -100.0 | -86.7 | -4.03 | 100.0 | 2389 | 58.5 | 0/20 | 0.007 |

POST-HOC exploratory variant (added after the first run, not pre-declared): decision taken every 4 h from the fold start and held 4 h (non-overlapping horizon), long/flat:

| Model | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|
| logistic regression | 606.7 | -76.7 | -99.2 | -0.33 | 90.4 | 682 | 72.5 | 0.043 |
| decision tree (depth 4) | 382.5 | -93.9 | -99.9 | -0.94 | 97.0 | 873 | 62.0 | 0.090 |
| ANF decision circuit | 444.6 | -94.6 | -99.9 | -1.02 | 95.9 | 923 | 59.0 | 0.047 |

Long/short variant (position +1 / -1), 10 bps per side:

| Model | total ret % | CAGR % | Sharpe | max DD % | turnover / yr |
|---|---|---|---|---|---|
| rule: SMA24 > SMA168 | -74.7 | -24.0 | -0.16 | 84.9 | 168 |
| rule: momentum ret_24h > 0 | -100.0 | -88.9 | -3.35 | 100.0 | 1786 |
| logistic regression | -100.0 | -94.2 | -4.41 | 100.0 | 3192 |
| decision tree (depth 4) | -100.0 | -98.3 | -6.38 | 100.0 | 4187 |
| ANF decision circuit | -100.0 | -98.8 | -6.93 | 100.0 | 4778 |

#### BTCUSDT — B: direction, h=24h

20 folds, 43823 out-of-sample hours, 43800 labelled; label base rate (y=1) 0.514.

| Model | acc % | bal. acc % | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | CAGR 10 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | folds beating B&H (10 bps) | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| buy & hold | 51.4 | 50.0 | 202.3 | 202.0 | 201.7 | 24.7 | 0.67 | 77.2 | 0 | 100.0 | 0/20 | - |
| always flat | 48.6 | 50.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.00 | 0.0 | 0 | 0.0 | 9/20 | - |
| majority class (train) | 51.4 | 50.0 | 202.3 | 202.0 | 201.7 | 24.7 | 0.67 | 77.2 | 0 | 100.0 | 0/20 | - |
| rule: close > SMA168 | 47.9 | 47.9 | 93.8 | -61.4 | -92.3 | -17.3 | -0.28 | 78.5 | 322 | 51.4 | 4/20 | 0.513 |
| rule: SMA24 > SMA168 | 48.3 | 48.2 | 118.9 | 43.6 | -5.8 | 7.5 | 0.38 | 67.9 | 84 | 51.6 | 7/20 | 0.357 |
| rule: momentum ret_24h > 0 | 47.0 | 47.0 | 1.9 | -98.8 | -100.0 | -58.9 | -1.99 | 99.2 | 893 | 51.4 | 2/20 | 0.923 |
| rule: vol filter vol_24h <= train q80 | 49.6 | 48.6 | 20.7 | -29.9 | -59.3 | -6.9 | 0.08 | 79.7 | 109 | 86.3 | 5/20 | 1.000 |
| logistic regression | 51.1 | 50.2 | 75.1 | -91.4 | -99.6 | -38.7 | -0.63 | 95.4 | 601 | 82.2 | 1/20 | 0.757 |
| decision tree (depth 4) | 51.1 | 50.7 | 263.0 | -80.1 | -98.9 | -27.6 | -0.38 | 90.0 | 580 | 66.3 | 4/20 | 0.137 |
| ANF decision circuit | 50.9 | 50.7 | 329.0 | -95.6 | -100.0 | -46.4 | -1.07 | 97.4 | 914 | 58.4 | 3/20 | 0.093 |

POST-HOC exploratory variant (added after the first run, not pre-declared): decision taken every 24 h from the fold start and held 24 h (non-overlapping horizon), long/flat:

| Model | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|
| logistic regression | 136.5 | 43.6 | -12.9 | 0.41 | 75.8 | 100 | 82.6 | 0.623 |
| decision tree (depth 4) | 22.4 | -26.4 | -55.8 | 0.13 | 82.9 | 102 | 66.8 | 0.767 |
| ANF decision circuit | 189.1 | 31.5 | -40.3 | 0.35 | 77.2 | 158 | 58.5 | 0.247 |

Long/short variant (position +1 / -1), 10 bps per side:

| Model | total ret % | CAGR % | Sharpe | max DD % | turnover / yr |
|---|---|---|---|---|---|
| rule: SMA24 > SMA168 | -74.7 | -24.0 | -0.16 | 84.9 | 168 |
| rule: momentum ret_24h > 0 | -100.0 | -88.9 | -3.35 | 100.0 | 1786 |
| logistic regression | -99.8 | -71.9 | -1.81 | 99.9 | 1203 |
| decision tree (depth 4) | -99.2 | -62.0 | -1.31 | 99.4 | 1160 |
| ANF decision circuit | -100.0 | -79.8 | -2.35 | 100.0 | 1828 |

### ETHUSDT: 61368 bars (61309 raw + 59 filled), 1546300800000 .. 1767222000000 (open times, UTC ms)

#### ETHUSDT — A: risk-off filter (H=24h, drop < -0.5 sigma)

20 folds, 43823 out-of-sample hours, 43800 labelled; label base rate (y=1) 0.741.

| Model | acc % | bal. acc % | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | CAGR 10 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | folds beating B&H (10 bps) | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| buy & hold | 74.1 | 50.0 | 304.8 | 304.4 | 304.0 | 32.2 | 0.75 | 81.3 | 0 | 100.0 | 0/20 | - |
| always flat | 25.9 | 50.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.00 | 0.0 | 0 | 0.0 | 8/20 | - |
| majority class (train) | 74.1 | 50.0 | 304.8 | 304.4 | 304.0 | 32.2 | 0.75 | 81.3 | 0 | 100.0 | 0/20 | - |
| rule: close > SMA168 | 50.5 | 48.9 | 347.8 | -9.9 | -81.9 | -2.1 | 0.22 | 79.6 | 320 | 52.7 | 7/20 | 0.217 |
| rule: SMA24 > SMA168 | 51.0 | 49.6 | 393.4 | 228.3 | 118.4 | 26.8 | 0.72 | 55.4 | 81 | 52.8 | 10/20 | 0.173 |
| rule: momentum ret_24h > 0 | 50.4 | 49.6 | 7.4 | -98.5 | -100.0 | -57.0 | -1.35 | 99.5 | 857 | 51.4 | 2/20 | 0.913 |
| rule: vol filter vol_24h <= train q80 | 65.8 | 48.7 | -23.4 | -59.0 | -78.1 | -16.3 | -0.00 | 82.6 | 125 | 84.8 | 6/20 | 1.000 |
| logistic regression | 74.2 | 50.0 | 313.4 | 312.1 | 310.9 | 32.7 | 0.75 | 81.3 | 1 | 100.0 | 1/20 | 0.017 |
| decision tree (depth 4) | 74.1 | 50.0 | 304.8 | 304.4 | 304.0 | 32.2 | 0.75 | 81.3 | 0 | 100.0 | 0/20 | - |
| ANF decision circuit | 73.6 | 49.9 | 211.3 | 122.6 | 59.2 | 17.4 | 0.60 | 78.5 | 67 | 98.9 | 6/20 | 0.907 |

POST-HOC exploratory variant (added after the first run, not pre-declared): decision taken every 24 h from the fold start and held 24 h (non-overlapping horizon), long/flat:

| Model | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|
| logistic regression | 304.8 | 304.4 | 304.0 | 0.75 | 81.3 | 0 | 100.0 | - |
| decision tree (depth 4) | 304.8 | 304.4 | 304.0 | 0.75 | 81.3 | 0 | 100.0 | - |
| ANF decision circuit | 170.2 | 159.3 | 148.8 | 0.63 | 79.2 | 8 | 98.8 | 0.980 |

#### ETHUSDT — B: direction, h=1h

20 folds, 43823 out-of-sample hours, 43823 labelled; label base rate (y=1) 0.506.

| Model | acc % | bal. acc % | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | CAGR 10 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | folds beating B&H (10 bps) | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| buy & hold | 50.6 | 50.0 | 304.8 | 304.4 | 304.0 | 32.2 | 0.75 | 81.3 | 0 | 100.0 | 0/20 | - |
| always flat | 49.4 | 50.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.00 | 0.0 | 0 | 0.0 | 8/20 | - |
| majority class (train) | 50.6 | 50.0 | 304.8 | 304.4 | 304.0 | 32.2 | 0.75 | 81.3 | 0 | 100.0 | 0/20 | - |
| rule: close > SMA168 | 49.4 | 49.4 | 347.8 | -9.9 | -81.9 | -2.1 | 0.22 | 79.6 | 320 | 52.7 | 7/20 | 0.217 |
| rule: SMA24 > SMA168 | 50.3 | 50.3 | 393.4 | 228.3 | 118.4 | 26.8 | 0.72 | 55.4 | 81 | 52.8 | 10/20 | 0.173 |
| rule: momentum ret_24h > 0 | 47.6 | 47.6 | 7.4 | -98.5 | -100.0 | -57.0 | -1.35 | 99.5 | 857 | 51.4 | 2/20 | 0.913 |
| rule: vol filter vol_24h <= train q80 | 50.0 | 49.6 | -23.4 | -59.0 | -78.1 | -16.3 | -0.00 | 82.6 | 125 | 84.8 | 6/20 | 1.000 |
| logistic regression | 53.2 | 53.2 | -15.4 | -100.0 | -100.0 | -93.3 | -4.37 | 100.0 | 2670 | 47.4 | 0/20 | 0.523 |
| decision tree (depth 4) | 53.2 | 53.1 | 37.9 | -100.0 | -100.0 | -95.6 | -4.90 | 100.0 | 3187 | 52.6 | 0/20 | 0.290 |
| ANF decision circuit | 53.2 | 53.2 | 130.0 | -100.0 | -100.0 | -95.3 | -4.81 | 100.0 | 3233 | 51.1 | 0/20 | 0.103 |

Long/short variant (position +1 / -1), 10 bps per side:

| Model | total ret % | CAGR % | Sharpe | max DD % | turnover / yr |
|---|---|---|---|---|---|
| rule: SMA24 > SMA168 | -51.1 | -13.3 | 0.21 | 80.4 | 163 |
| rule: momentum ret_24h > 0 | -100.0 | -90.1 | -2.55 | 100.0 | 1715 |
| logistic regression | -100.0 | -99.7 | -7.17 | 100.0 | 5339 |
| decision tree (depth 4) | -100.0 | -99.9 | -8.20 | 100.0 | 6374 |
| ANF decision circuit | -100.0 | -99.9 | -8.06 | 100.0 | 6466 |

#### ETHUSDT — B: direction, h=4h

20 folds, 43823 out-of-sample hours, 43820 labelled; label base rate (y=1) 0.511.

| Model | acc % | bal. acc % | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | CAGR 10 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | folds beating B&H (10 bps) | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| buy & hold | 51.1 | 50.0 | 304.8 | 304.4 | 304.0 | 32.2 | 0.75 | 81.3 | 0 | 100.0 | 0/20 | - |
| always flat | 48.9 | 50.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.00 | 0.0 | 0 | 0.0 | 8/20 | - |
| majority class (train) | 51.1 | 50.0 | 304.8 | 304.4 | 304.0 | 32.2 | 0.75 | 81.3 | 0 | 100.0 | 0/20 | - |
| rule: close > SMA168 | 48.8 | 48.8 | 347.8 | -9.9 | -81.9 | -2.1 | 0.22 | 79.6 | 320 | 52.7 | 7/20 | 0.217 |
| rule: SMA24 > SMA168 | 49.6 | 49.6 | 393.4 | 228.3 | 118.4 | 26.8 | 0.72 | 55.4 | 81 | 52.8 | 10/20 | 0.173 |
| rule: momentum ret_24h > 0 | 46.6 | 46.6 | 7.4 | -98.5 | -100.0 | -57.0 | -1.35 | 99.5 | 857 | 51.4 | 2/20 | 0.913 |
| rule: vol filter vol_24h <= train q80 | 49.9 | 49.1 | -23.6 | -59.1 | -78.2 | -16.4 | -0.00 | 82.6 | 125 | 84.8 | 6/20 | 1.000 |
| logistic regression | 53.2 | 53.0 | 727.1 | -99.9 | -100.0 | -72.9 | -1.78 | 99.9 | 1726 | 57.9 | 3/20 | 0.033 |
| decision tree (depth 4) | 53.4 | 53.3 | 173.5 | -99.9 | -100.0 | -75.6 | -1.99 | 99.9 | 1610 | 52.0 | 1/20 | 0.157 |
| ANF decision circuit | 52.6 | 52.5 | 710.5 | -99.9 | -100.0 | -75.9 | -1.99 | 99.9 | 1841 | 53.0 | 2/20 | 0.010 |

POST-HOC exploratory variant (added after the first run, not pre-declared): decision taken every 4 h from the fold start and held 4 h (non-overlapping horizon), long/flat:

| Model | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|
| logistic regression | 1396.2 | -69.3 | -99.4 | -0.07 | 95.0 | 776 | 58.6 | 0.020 |
| decision tree (depth 4) | 342.1 | -86.8 | -99.6 | -0.35 | 96.0 | 702 | 52.6 | 0.137 |
| ANF decision circuit | 429.0 | -89.4 | -99.8 | -0.42 | 94.1 | 781 | 53.2 | 0.083 |

Long/short variant (position +1 / -1), 10 bps per side:

| Model | total ret % | CAGR % | Sharpe | max DD % | turnover / yr |
|---|---|---|---|---|---|
| rule: SMA24 > SMA168 | -51.1 | -13.3 | 0.21 | 80.4 | 163 |
| rule: momentum ret_24h > 0 | -100.0 | -90.1 | -2.55 | 100.0 | 1715 |
| logistic regression | -100.0 | -95.6 | -3.56 | 100.0 | 3452 |
| decision tree (depth 4) | -100.0 | -96.5 | -3.84 | 100.0 | 3219 |
| ANF decision circuit | -100.0 | -96.5 | -3.87 | 100.0 | 3682 |

#### ETHUSDT — B: direction, h=24h

20 folds, 43823 out-of-sample hours, 43800 labelled; label base rate (y=1) 0.514.

| Model | acc % | bal. acc % | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | CAGR 10 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | folds beating B&H (10 bps) | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| buy & hold | 51.4 | 50.0 | 304.8 | 304.4 | 304.0 | 32.2 | 0.75 | 81.3 | 0 | 100.0 | 0/20 | - |
| always flat | 48.6 | 50.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.00 | 0.0 | 0 | 0.0 | 8/20 | - |
| majority class (train) | 51.4 | 50.0 | 304.8 | 304.4 | 304.0 | 32.2 | 0.75 | 81.3 | 0 | 100.0 | 0/20 | - |
| rule: close > SMA168 | 48.2 | 48.1 | 347.8 | -9.9 | -81.9 | -2.1 | 0.22 | 79.6 | 320 | 52.7 | 7/20 | 0.217 |
| rule: SMA24 > SMA168 | 48.9 | 48.9 | 393.4 | 228.3 | 118.4 | 26.8 | 0.72 | 55.4 | 81 | 52.8 | 10/20 | 0.173 |
| rule: momentum ret_24h > 0 | 47.4 | 47.4 | 7.4 | -98.5 | -100.0 | -57.0 | -1.35 | 99.5 | 857 | 51.4 | 2/20 | 0.913 |
| rule: vol filter vol_24h <= train q80 | 49.5 | 48.5 | -23.4 | -59.0 | -78.1 | -16.3 | -0.00 | 82.6 | 125 | 84.8 | 6/20 | 1.000 |
| logistic regression | 52.0 | 51.3 | 203.5 | -91.2 | -99.7 | -38.5 | -0.36 | 96.8 | 708 | 73.7 | 1/20 | 0.423 |
| decision tree (depth 4) | 51.2 | 50.9 | -17.0 | -97.2 | -99.9 | -51.0 | -0.78 | 97.9 | 675 | 58.3 | 3/20 | 0.777 |
| ANF decision circuit | 51.6 | 51.5 | 11.0 | -98.4 | -100.0 | -56.2 | -1.04 | 98.7 | 845 | 54.6 | 1/20 | 0.677 |

POST-HOC exploratory variant (added after the first run, not pre-declared): decision taken every 24 h from the fold start and held 24 h (non-overlapping horizon), long/flat:

| Model | total ret 0 bps % | total ret 10 bps % | total ret 20 bps % | Sharpe 10 bps | max DD 10 bps % | turnover / yr | in market % | random-signal p (10 bps) |
|---|---|---|---|---|---|---|---|---|
| logistic regression | 448.7 | 200.7 | 64.7 | 0.66 | 77.7 | 120 | 74.2 | 0.213 |
| decision tree (depth 4) | 101.8 | 2.8 | -47.6 | 0.32 | 81.9 | 135 | 57.4 | 0.543 |
| ANF decision circuit | -4.4 | -54.6 | -78.4 | 0.06 | 89.9 | 149 | 54.7 | 0.787 |

Long/short variant (position +1 / -1), 10 bps per side:

| Model | total ret % | CAGR % | Sharpe | max DD % | turnover / yr |
|---|---|---|---|---|---|
| rule: SMA24 > SMA168 | -51.1 | -13.3 | 0.21 | 80.4 | 163 |
| rule: momentum ret_24h > 0 | -100.0 | -90.1 | -2.55 | 100.0 | 1715 |
| logistic regression | -99.9 | -75.2 | -1.38 | 99.9 | 1416 |
| decision tree (depth 4) | -100.0 | -85.2 | -2.03 | 100.0 | 1350 |
| ANF decision circuit | -100.0 | -88.6 | -2.36 | 100.0 | 1690 |


## A vs B

| | A (risk-off, H=24h) | B h=1 | B h=4 | B h=24 |
|---|---|---|---|---|
| best balanced acc. (learned), BTC / ETH | 50.0 / 50.0 | 53.2 / 53.2 | 53.3 / 53.3 | 50.7 / 51.5 |
| ANF total ret 0 bps, BTC / ETH % | 186.0 / 211.3 | 97.1 / 130.0 | 555.5 / 710.5 | 329.0 / 11.0 |
| ANF total ret 10 bps, BTC / ETH % | 146.9 / 122.6 | −100.0 / −100.0 | −100.0 / −99.9 | −95.6 / −98.4 |
| ANF turnover / yr, BTC / ETH | 29 / 67 | 3414 / 3233 | 2389 / 1841 | 914 / 845 |
| buy & hold 10 bps, BTC / ETH % | 202.0 / 304.4 | same | same | same |

- **A** is the "right" decision problem for a long-only holder: few
  decisions, so costs barely matter. But the features carry no information
  about a coming large drop. Every learned model either stays always-in or
  loses to buy & hold through bad exits.
- **B** carries real short-horizon information: reversal after strong
  1h/4h/24h moves, consistent across both assets and all three learned models.
  The information is far smaller than the cost of acting on it hourly. In the
  post-hoc hold-h variant, which cuts turnover by ~3–4×, B4 is still deeply
  negative at 10 bps. Only a daily decision (B24 hold) is positive:
  - BTC logistic +43.6%, ANF +31.5%;
  - ETH logistic +200.7%.
  All of these are still below buy & hold, with p >= 0.21.
- The only configuration with a better Sharpe ratio than buy & hold is not a
  learned model. It is the SMA24 > SMA168 rule on ETH: Sharpe 0.72 vs 0.75,
  max DD 55.4% vs 81.3%, but total return 228.3% vs 304.4%, p = 0.173. This is
  a drawdown reduction, not alpha.

## Example readable ANF circuit

BTCUSDT, B h=1, last fold (2025Q4). Unseen addresses (count `[0, 0]`) are
omitted from the table; they take the train majority (1).

```
BTCUSDT / B: direction, h=1h / fold 2025Q4 (train rows 58991, train base rate 0.509):
  u0 = [ret_1h > -0.00077 (train q40)]
  u1 = [ret_4h > -0.00131 (train q40)]
  u2 = [ret_24h > 0.00553 (train q60)]
  u3 = [ret_1h > -0.00307 (train q20)]
  u4 = [ret_4h > 0.00177 (train q60)]
  u5 = [ret_1h > 0.00092 (train q60)]
  truth table (address u_{k-1}..u_0 -> decision, train counts [y=0, y=1]):
    000000 -> 1 [2526, 3517]
    000010 -> 1 [319, 402]
    000100 -> 1 [870, 1231]
    000110 -> 1 [238, 255]
    001000 -> 1 [1571, 2138]
    001001 -> 1 [1268, 1486]
    001010 -> 1 [851, 1008]
    001011 -> 1 [1263, 1291]
    001100 -> 1 [772, 879]
    001101 -> 1 [567, 626]
    001110 -> 0 [496, 475]
    001111 -> 0 [557, 514]
    010010 -> 1 [490, 589]
    010110 -> 1 [610, 752]
    011010 -> 1 [827, 905]
    011011 -> 0 [1175, 1095]
    011110 -> 0 [992, 884]
    011111 -> 0 [1070, 886]
    101001 -> 1 [2038, 2210]
    101011 -> 0 [1292, 1252]
    101101 -> 0 [993, 905]
    101111 -> 0 [846, 739]
    111011 -> 0 [3183, 2699]
    111111 -> 0 [4166, 3273]
  ANF: decision = 1 ⊕ u1·u2·u3 ⊕ u0·u1·u3·u4 ⊕ u0·u1·u2·u3·u4 ⊕ u0·u1·u3·u5 ⊕ u0·u2·u3·u5 ⊕ u1·u2·u3·u5 ⊕ u0·u1·u2·u3·u5 ⊕ u0·u1·u3·u4·u5 ⊕ u0·u2·u3·u4·u5
  selected-bit feature frequency over 20 folds: ret_1h:54, ret_4h:27, ret_24h:9, close/sma24-1:7, volume_z_168h:6, sma24/sma168-1:4, vol_168h:3, vol_24h:2, close/sma168-1:1, ret_168h:1
```

Reading: the six bits are thermometer levels of ret_1h, ret_4h and ret_24h.
The circuit goes **flat** when the recent returns are all high. For example,
`111111` (1h, 4h and 24h returns all above their 60% quantile) → 0, counts
[4166, 3273]. It stays **long** otherwise; for example, `000000` (all low) → 1,
counts [2526, 3517]. That is a short-term reversal rule, and every cell can be
audited against its train counts.

Not every circuit is this readable. With 8 inputs the ANF often has dozens of
monomials, and some decisions sit on tiny cells (counts like [1, 0]), which is
overfitting the fan-in budget does not prevent.

Bits selected over the 20 folds, by feature:

- BTCUSDT A: vol_ratio_24/168:27, ret_72h:26, vol_168h:22, ret_168h:17, range_pos_168h:15, vol_24h:15, ret_24h:11, close/sma24-1:9, volume_z_168h:9, sma24/sma168-1:8, close/sma168-1:1
- BTCUSDT B h=1: ret_1h:54, ret_4h:27, ret_24h:9, close/sma24-1:7, volume_z_168h:6, sma24/sma168-1:4, vol_168h:3, vol_24h:2, close/sma168-1:1, ret_168h:1
- BTCUSDT B h=4: ret_4h:39, ret_24h:38, volume_z_168h:21, ret_1h:10, ret_168h:9, ret_72h:7, vol_168h:5, close/sma168-1:3, close/sma24-1:2, vol_ratio_24/168:2, range_pos_168h:1, sma24/sma168-1:1, vol_24h:1
- BTCUSDT B h=24: ret_168h:26, vol_168h:26, close/sma24-1:20, ret_72h:17, vol_ratio_24/168:17, range_pos_168h:16, sma24/sma168-1:15, close/sma168-1:9, vol_24h:9, ret_24h:5
- ETHUSDT A: vol_168h:31, ret_168h:23, vol_24h:19, ret_72h:17, vol_ratio_24/168:16, range_pos_168h:15, sma24/sma168-1:13, ret_24h:10, close/sma168-1:8, close/sma24-1:8
- ETHUSDT B h=1: ret_1h:39, ret_4h:30, volume_z_168h:16, ret_24h:14, close/sma24-1:8, ret_72h:3, vol_168h:2, ret_168h:1, vol_24h:1
- ETHUSDT B h=4: ret_24h:30, ret_4h:24, ret_168h:21, ret_72h:18, sma24/sma168-1:13, vol_168h:10, volume_z_168h:9, vol_ratio_24/168:8, range_pos_168h:6, ret_1h:5, vol_24h:5, close/sma24-1:3, close/sma168-1:2
- ETHUSDT B h=24: ret_168h:22, vol_168h:22, vol_ratio_24/168:20, close/sma24-1:19, vol_24h:18, ret_72h:16, close/sma168-1:15, range_pos_168h:11, sma24/sma168-1:11, ret_24h:6

## Interpretation

- The boolean decision circuit is a usable, auditable classifier on this kind
  of data. It matches logistic regression and a tree on accuracy, and it says
  exactly which conditions trigger a decision.
- The bottleneck is not the model class but the **objective**. All learned
  models maximize classification accuracy on a label that ignores costs, so
  they flip position whenever the probability crosses 0.5. With a ~53% edge on
  1–4h moves of ~0.5%, the expected gross edge per trade is well under 10 bps.
- For A, the hourly feature set (returns, vol, SMA, volume) does not predict
  24h drawdowns beyond the base rate.

## Limitations

- Two assets, one exchange, spot only; 2021–2025 is a single strongly trending
  regime (buy & hold +202% / +304%).
- No funding, order book, on-chain or cross-asset features.
- Fills at the hourly close; no slippage model beyond the flat bps cost; no
  shorting cost.
- About 130 model/task/variant evaluations: the smallest p-value (0.007) is far
  above a Bonferroni threshold (~0.0004). Individual "significant" results
  should be treated as noise.
- The post-hoc hold-h variant was added after seeing the results.
- The random-signal test conditions on exposure and switching rate, not on
  the market regime.

## Recommendation and next step

- **Do not trade any of these models.**
- If crypto stays the first specialty, the next step should change the
  **decision objective**, not the circuit:
  - cost-aware labels: trade only if |forward return| > 2 × cost;
  - a no-trade band or hysteresis on the decision (two thresholds);
  - decisions at 24h+ horizons;
  - a maker-fee sensitivity (2–5 bps).
  Then add information that the features lack: funding rates, BTC→ETH
  cross-asset lags.
- Keep the ANF circuit as the readable model. Its value here is
  auditability, not accuracy.
