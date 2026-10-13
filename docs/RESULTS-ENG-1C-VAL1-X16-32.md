# ENG-1c/VAL1-X16-32 — exploratory adaptive occurrence thresholds

Measured 2026-10-13 on local x86_64 Linux with Rust 1.89.0.
User-requested thresholds 16 and 32 were frozen in commit
`f18c666ec72cf9ad9f563008fc4e4cde6b3f3686` before implementation/scoring.
[Protocol](ENG-1C-VAL1-X16-32-EXPLORATORY-PROTOCOL.md).

These Dumas/Austen validation sets were already exposed by VAL1. This is
post-hoc exploration, not independent confirmation. The original seven-arm
experiment and its report remain intact. TEST is not scored or created as a file.

## Predictive comparison

Lower bits/byte is better. Exact same TRAIN-only depth32 store, histories,
64 VAL lanes and target positions for all nine arms; counts do not update on VAL.
FR: 74,688 targets. EN: 73,728 targets.

| Policy | FR bits/byte | FR accuracy | EN bits/byte | EN accuracy |
|---|---:|---:|---:|---:|
| FIXED_4 | 2.063036593 | 56.5633% | 1.876564702 | 60.4180% |
| MIN_COUNT_8 | 2.089344543 | 58.5181% | 1.913076323 | 62.0294% |
| MIN_COUNT_16 | 2.071065428 | 57.6277% | 1.882945951 | 61.4407% |
| MIN_COUNT_32 | 2.065172325 | 56.9610% | 1.876043659 | 60.9172% |

Threshold32 has the lowest loss among the five adaptive thresholds in both
languages. Against FIXED_4 it is slightly worse in FR and slightly better
in EN. Neither comparison demonstrates a robust gain.

Positive gain below means FIXED_4 loss minus adaptive loss. Intervals are
**descriptive**, using 2048 paired 64-lane bootstrap draws with the unchanged
VAL1 seed; they are not independent significance tests on these exposed data.

| Language / threshold | Gain bits/byte | Approx. 95% paired interval |
|---|---:|---|
| FR / MIN_COUNT_16 | -0.008028834399 | [-0.015282398277, -0.001072514589] |
| FR / MIN_COUNT_32 | -0.002135731571 | [-0.007023169275, 0.002438576034] |
| EN / MIN_COUNT_16 | -0.006381249252 | [-0.012857872999, 0.000057518131] |
| EN / MIN_COUNT_32 | 0.000521043202 | [-0.003599320095, 0.004684449703] |

## Actual selected depths

| Language / threshold | depth4 | depth7 | depth12 | depth32 |
|---|---:|---:|---:|---:|
| FR / MIN_COUNT_16 | 56714 | 17226 | 748 | 0 |
| FR / MIN_COUNT_32 | 64035 | 10402 | 251 | 0 |
| EN / MIN_COUNT_16 | 52877 | 19706 | 1133 | 12 |
| EN / MIN_COUNT_32 | 60793 | 12528 | 407 | 0 |

The higher thresholds use depth4 more often and reduce the loss gap observed
at threshold8. This does not establish a causal explanation of the remaining
errors. Threshold32 never selected depth32 in either language.

## Evidence and limits

All nine predictive rows (loss, accuracy, selections and bootstrap intervals)
reproduced exactly in a second local execution. The original seven rows match
the previously retained VAL1 values. Final source and release binary SHA256,
source split identities, both aggregate outputs and their checksum manifest:
[raw evidence](evidence/eng1c-val1-x16-32/).

Both runs used the existing 262144-KiB virtual-address limit and 3000-second
deadline. These are not standalone production latency experiments. All arms
still retain the common depth32 store (565,374,640 counted bytes FR;
551,748,904 EN), so higher thresholds do not demonstrate storage savings.
Process VmHWM is not filesystem-cache memory or full system RAM.

Verdict in both languages: **EXPLORATORY_VAL_STOP_NOT_PROVEN**.
No runtime default, ElasticXxx physical actuation or SML25B promotion.
A further adaptive-quality claim requires a separately frozen fresh-data study.
