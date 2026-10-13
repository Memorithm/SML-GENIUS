# ENG-1c/VAL1-X64-128 — exploratory adaptive occurrence thresholds

Measured 2026-10-13, local x86_64 Linux, Rust1.89.0.
Protocol frozen before implementation/scoring at
8c48115e7369de1bee48b59d45e3c179527714dd.
[Protocol](ENG-1C-VAL1-X64-128-EXPLORATORY-PROTOCOL.md).

User-requested continuation of the exposed Dumas/Austen VAL1 experiment:
new thresholds64 and128, original nine controls retained. This is post-hoc
exploration, not independent confirmation. TEST is not scored or created.

## Matched predictive comparison

Lower bits/byte is better. The same immutable TRAIN-only depth32 paged
Witten–Bell store, 64-lane histories and target positions serve all eleven
arms: 74,688 FR targets, 73,728 EN targets. Both TRAIN and VAL SHA256 were
independently rechecked before reusing the previously downloaded splits.

| Policy | FR bits/byte | FR accuracy | EN bits/byte | EN accuracy |
|---|---:|---:|---:|---:|
| FIXED_4 | 2.063036593 | 56.5633% | 1.876564702 | 60.4180% |
| MIN_COUNT_16 | 2.071065428 | 57.6277% | 1.882945951 | 61.4407% |
| MIN_COUNT_32 | 2.065172325 | 56.9610% | 1.876043659 | 60.9172% |
| MIN_COUNT_64 | 2.065420145 | 56.6624% | 1.870807959 | 60.5889% |
| MIN_COUNT_128 | 2.061957645 | 56.6169% | 1.874853320 | 60.4641% |

Threshold128 has the lowest observed loss in FR across the eleven arms.
Threshold64 has the lowest observed loss in EN. Increasing the threshold
is not monotonically beneficial: FR64 is worse than FR32, and EN128 is worse
than EN64. These are observed comparisons, not causal explanations.

## Descriptive paired uncertainty versus FIXED_4

Positive gain = fixed4 bits/byte minus adaptive bits/byte. Same VAL1
2048 paired 64-lane bootstrap draws, seed and order statistics. Since these
VAL sets were already exposed and thresholds explored in succession,
intervals are descriptive and do not establish independent significance.

| Language / threshold | Gain bits/byte | Approx. 95% paired interval |
|---|---:|---|
| FR / MIN_COUNT_64 | -0.002383551115 | [-0.005657439871, 0.000799460545] |
| FR / MIN_COUNT_128 | 0.001078948574 | [-0.000029277102, 0.002079807088] |
| EN / MIN_COUNT_64 | 0.005756743079 | [0.003228417525, 0.008231005498] |
| EN / MIN_COUNT_128 | 0.001711382311 | [0.000269293783, 0.003095980173] |

Both English intervals are positive in this descriptive bootstrap.
The FR128 gain interval includes zero. The best observed gains
(FR128: 0.001078948574; EN64: 0.005756743079 bits/byte) remain below
VAL1's frozen 0.010000-bit/byte effect-size criterion. Therefore both
printed verdicts remain **EXPLORATORY_VAL_STOP_NOT_PROVEN**.
There is an English exploratory signal, not a confirmatory result.

## Selected depths and resource boundary

| Language / threshold | depth4 | depth7 | depth12 | depth32 |
|---|---:|---:|---:|---:|
| FR / MIN_COUNT_64 | 69687 | 4908 | 93 | 0 |
| FR / MIN_COUNT_128 | 72832 | 1856 | 0 | 0 |
| EN / MIN_COUNT_64 | 67037 | 6568 | 123 | 0 |
| EN / MIN_COUNT_128 | 70772 | 2912 | 44 | 0 |

Neither new policy selected depth32. All eleven arms still share the full
depth32 store: 565,374,640 counted bytes FR; 551,748,904 EN. Rare selection of
long contexts does not demonstrate physical storage or RAM savings.
Process VmHWM excludes filesystem-cache memory and is not model-only RAM.
The 262144-KiB virtual-address limit and 3000-second deadline were used.
No native Thor timing, GPU, serving latency or resource-controller claim.

## Reproducibility and software validation

All nine original predictive rows match the previously retained evidence
exactly, including total bits, accuracy, selections and bootstrap bounds.
The two new rows were scored once locally. Dedicated CI is configured to
repeat all eleven arms. Strict Rust1.89 fmt, warnings-as-errors Clippy and
full workspace tests passed through the versioned pre-push hook.
Integration tests execute seven/nine/eleven-arm modes on a real paged store
and compare the earlier arms exactly; threshold boundaries16/32/64/128
are inclusive and choose the deepest qualifying TRAIN context.

[Raw aggregate evidence and source/binary SHA256](evidence/eng1c-val1-x64-128/).
Earlier [16/32 report](RESULTS-ENG-1C-VAL1-X16-32.md) remains unchanged.

No independent-data, production default, ElasticXxx actuation or SML25B
promotion is established. A new quality claim needs a frozen fresh-data
confirmation rather than continued threshold selection on this VAL.
