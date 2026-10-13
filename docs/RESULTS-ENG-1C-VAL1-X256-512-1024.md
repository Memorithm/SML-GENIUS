# ENG-1c/VAL1-X256-512-1024 — sequential exploratory threshold escalation

Measured 2026-10-13 on local x86_64 Linux with Rust 1.89.0.
All three thresholds were frozen before code/scoring in protocol commit
c414e814a7c4edd978519ef7cde590883c4c7021.
[Protocol](ENG-1C-VAL1-X256-512-1024-EXPLORATORY-PROTOCOL.md).

Stage 1 scored the thirteen-arm 256/512 mode in both languages.
Only after both evaluations completed, stage 2 scored the fourteen-arm
1024 mode. Thresholds were not altered after stage 1.
These are previously exposed Dumas/Austen VAL sets: exploratory evidence,
not independent confirmation. TEST is not scored or materialized as a file.

## Matched predictive results

Lower bits/byte is better. All fourteen arms use the same immutable
TRAIN-only depth32 Witten–Bell store and identical 64-lane VAL targets:
74,688 FR and 73,728 EN. Local TRAIN and VAL SHA256 were independently
rechecked before reuse; no counts update on VAL.

| Policy | FR bits/byte | FR accuracy | EN bits/byte | EN accuracy |
|---|---:|---:|---:|---:|
| FIXED_4 | 2.063036593417 | 56.5633% | 1.876564702039 | 60.4180% |
| MIN_COUNT_64 | 2.065420144531 | 56.6624% | 1.870807958960 | 60.5889% |
| MIN_COUNT_128 | 2.061957644842 | 56.6169% | 1.874853319727 | 60.4641% |
| MIN_COUNT_256 | 2.063106741450 | 56.5794% | 1.876934633220 | 60.4058% |
| MIN_COUNT_512 | 2.063105146280 | 56.5633% | 1.876559133352 | 60.4180% |
| MIN_COUNT_1024 | 2.063036593417 | 56.5633% | 1.876564702039 | 60.4180% |

The best observed policy remains MIN_COUNT_128 in FR and MIN_COUNT_64 in EN.
None of the three new thresholds beats those earlier best losses.
Threshold 256 is slightly worse than FIXED_4 in both texts. Threshold 512 is
slightly worse in FR and almost identical in EN.

At threshold 1024, **every VAL event selects depth4 in both languages**:
the predictor and all scored metrics coincide with the fixed4 control.
This is a complete fallback on these observed histories; it does not prove
that no TRAIN depth7 context anywhere has 1024 occurrences.

## Descriptive paired intervals versus FIXED_4

Positive gain is fixed4 loss minus adaptive loss. Same 2048-draw paired
64-lane bootstrap, seed 0xE1C5_2026_1013_B001, order statistics 51/1996.
Intervals are descriptive on exposed VAL; no independent significance claim.

| Language / threshold | Gain bits/byte | Approx. 95% paired interval |
|---|---:|---|
| FR / MIN_COUNT_256 | -0.000070148034 | [-0.000537335821, 0.000298856568] |
| FR / MIN_COUNT_512 | -0.000068552864 | [-0.000193641686, 0.000064845519] |
| FR / MIN_COUNT_1024 | 0.000000000000 | [0.000000000000, 0.000000000000] |
| EN / MIN_COUNT_256 | -0.000369931181 | [-0.001208663236, 0.000326879628] |
| EN / MIN_COUNT_512 | 0.000005568686 | [-0.000064853362, 0.000074702875] |
| EN / MIN_COUNT_1024 | 0.000000000000 | [0.000000000000, 0.000000000000] |

All nonzero new-arm gain intervals include zero. Threshold 1024 equals the
control, so its paired interval is exactly [0,0]. Both overall exploratory
verdicts remain **EXPLORATORY_VAL_STOP_NOT_PROVEN** under the unchanged
0.010000-bit/byte effect-size criterion.

## Actual selected depths

| Language / threshold | depth4 | depth7 | depth12 | depth32 |
|---|---:|---:|---:|---:|
| FR / MIN_COUNT_256 | 74115 | 573 | 0 | 0 |
| FR / MIN_COUNT_512 | 74561 | 127 | 0 | 0 |
| FR / MIN_COUNT_1024 | 74688 | 0 | 0 | 0 |
| EN / MIN_COUNT_256 | 72866 | 862 | 0 | 0 |
| EN / MIN_COUNT_512 | 73609 | 119 | 0 | 0 |
| EN / MIN_COUNT_1024 | 73728 | 0 | 0 | 0 |

The new gates never select depth12 or depth32. The common depth32 store is
still retained: 565,374,640 counted bytes FR and 551,748,904 EN. Returning
fixed4 predictions does not free this storage. Process VmHWM excludes
filesystem-cache memory; no physical RAM-saving or production-latency claim.
Both stages use the original 262144-KiB virtual-address limit and 3000-second
process deadline. No Thor, GPU or live ElasticXxx actuation.

## Reproduction and validation

The thirteen common predictive rows reproduce exactly between stage 1 and
stage 2 in both languages, including total bits, accuracy, selection counts
and descriptive bootstrap intervals. The original eleven rows also match
the previously published evidence exactly. Thus 256/512 were scored twice
locally; 1024 once. Dedicated CI is configured to repeat all fourteen arms.

Strict Rust 1.89 fmt, warnings-as-errors Clippy and full workspace tests
passed through the versioned pre-push hook. A real paged-store integration
test executes all five evaluator modes and compares earlier arms exactly.
Synthetic TRAIN repetitions now supply enough evidence to activate the
1024-occurrence arm, while separate threshold tests exercise inclusive
boundaries through 1024 and fallback below each threshold.

[Aggregate evidence, source/binary SHA256 and manifests](evidence/eng1c-val1-x256-512-1024/).
Earlier [64/128 report](RESULTS-ENG-1C-VAL1-X64-128.md) remains unchanged.

No model default or SML25B promotion. This exploration does not establish
general superiority of an adaptive n-gram. Any confirmatory quality study
requires a frozen policy and fresh independent text.
