# ENG-1c/VAL1-GRID — every FR128..139 and EN65..74 threshold

Measured 2026-10-13, local x86_64 Linux, Rust 1.89.0.
Protocol frozen before grid implementation/scoring at
1eb7048a05c76cef2f75db3693e1eb1e94f2f5af.
[Protocol](ENG-1C-VAL1-GRID-FR128-139-EN65-74-PROTOCOL.md).

All requested thresholds were scored: FR128 through 139 inclusive (12),
EN65 through 74 inclusive (10). FR128 reuses its existing comparator row,
so there are 26 FR rows and 25 EN rows including all fifteen prior arms.
No skipped thresholds or extensions after scoring. TEST remains unscored
and is not materialized as a file.

This is a further post-hoc sweep on already exposed Dumas/Austen VAL.
Selecting a minimum on this grid is exploratory, not independent confirmation.

## Complete French grid

Lower bits/byte is better. Fixed4: 2.063036593417 bits/byte.
All arms use the same 74,688 target positions and immutable TRAIN counts.

| TRAIN threshold | Bits/byte | Accuracy | Selected depth7 | Selected depth12 |
|---:|---:|---:|---:|---:|
| 128 | 2.061957644842 | 56.6169% | 1856 | 0 |
| 129 | 2.061937590389 | 56.6169% | 1840 | 0 |
| 130 | 2.062023515488 | 56.6088% | 1831 | 0 |
| 131 | 2.062326835106 | 56.5941% | 1793 | 0 |
| 132 | 2.062325483575 | 56.5941% | 1753 | 0 |
| 133 | 2.062339035527 | 56.5941% | 1703 | 0 |
| 134 | 2.061985328266 | 56.5941% | 1664 | 0 |
| 135 | 2.062065558015 | 56.5941% | 1606 | 0 |
| 136 | 2.062149055225 | 56.5941% | 1592 | 0 |
| 137 | 2.062191376415 | 56.5941% | 1550 | 0 |
| 138 | 2.062213845512 | 56.5941% | 1523 | 0 |
| 139 | 2.062213845512 | 56.5941% | 1523 | 0 |

## Complete English grid

Fixed4: 1.876564702039 bits/byte. Same 73,728 target positions for all arms.

| TRAIN threshold | Bits/byte | Accuracy | Selected depth7 | Selected depth12 |
|---:|---:|---:|---:|---:|
| 65 | 1.871362182380 | 60.5835% | 6418 | 117 |
| 66 | 1.871632271508 | 60.5849% | 6253 | 117 |
| 67 | 1.871893140022 | 60.5781% | 6137 | 117 |
| 68 | 1.871927609679 | 60.5781% | 6048 | 117 |
| 69 | 1.872402161020 | 60.5767% | 5964 | 117 |
| 70 | 1.872351450400 | 60.5781% | 5936 | 117 |
| 71 | 1.872376881971 | 60.5767% | 5812 | 117 |
| 72 | 1.872595802422 | 60.5740% | 5694 | 117 |
| 73 | 1.872527519100 | 60.5754% | 5641 | 117 |
| 74 | 1.871997975172 | 60.5740% | 5511 | 102 |

## Observed minima and earlier comparators

Requested-grid minima: FR129 and EN65, each without a minimum-loss tie.
Ties use the f64 loss reconstructed from round-trip-safe raw total_bits
and the common token denominator. FR138/139 have identical observed loss;
their metrics also match the earlier FR140 arm on these VAL positions.

| Language | Earlier best threshold | Earlier bits/byte | Best requested threshold | Requested bits/byte | New loss minus earlier loss |
|---|---:|---:|---:|---:|---:|
| FR | 128 | 2.061957644842 | 129 | 2.061937590389 | -0.000020054453 |
| EN | 64 | 1.870807958960 | 65 | 1.871362182380 | 0.000554223420 |

FR129 improves the observed mean only slightly relative to FR128.
EN65 is best within the requested English range, but EN64 remains the best
across all retained candidates. Higher thresholds are not monotonically
beneficial; these are measured loss comparisons, not causal hypotheses.

## Descriptive uncertainty against FIXED_4

Same paired 64-lane bootstrap: 2048 draws, seed 0xE1C5_2026_1013_B001,
order statistics 51/1996. Positive gain is fixed4 loss minus candidate loss.
Bounds below compare against fixed4, not against the earlier adaptive best;
selection and multiple post-hoc sweeps preclude an independent significance
claim.

| Candidate | Gain bits/byte | Approx. 95% paired interval |
|---|---:|---|
| FR129 | 0.001099003027 | [-0.000016392205, 0.002101469415] |
| EN65 | 0.005202519659 | [0.002813433784, 0.007633712672] |
| EN64 | 0.005756743079 | [0.003228417525, 0.008231005498] |

FR129's interval includes zero. EN65's interval is descriptively positive,
while the previously best EN64 retains a larger observed gain.
No paired uncertainty versus earlier best thresholds was computed.
Both overall verdicts remain EXPLORATORY_VAL_STOP_NOT_PROVEN under the
original 0.010000-bit/byte effect-size condition. A confirmatory study needs
a configuration frozen before new independent text is exposed.

## Reproduction, implementation and resource boundaries

All fifteen earlier predictive rows match exactly: total bits, accuracy,
selected depths and bootstrap bounds. Typed policy specifications replace
fixed-size choice arrays so the bounded language-specific grid can retain
all earlier modes and append only the declared thresholds. The old six
modes keep their arm counts and output identities.

Strict Rust 1.89 fmt, warnings-as-errors Clippy and full workspace tests
passed through the versioned pre-push hook. A real paged-store fixture with
exactly 128 TRAIN occurrences checks every grid threshold, inclusive
endpoints, absence of duplicate names, FR128 reuse and preservation of all
fifteen earlier arms. Unknown language identity is rejected.
Dedicated CI is configured for 26 FR / 25 EN rows and exact grid metadata.

Same full TRAIN-only depth32 Witten–Bell stores (565,374,640 counted bytes FR;
551,748,904 EN), TRAIN/VAL SHA256, 64-lane reset histories and existing quotas.
Local TRAIN and VAL hashes were independently rechecked before reuse.
Original 262144-KiB virtual-address limit and 3000-second process deadline.
All grid policies retain the depth32 store; no physical storage or RAM saving,
GPU, serving latency, ElasticXxx actuation or SML25B promotion claim.

[All raw rows, source/binary SHA256 and manifest](evidence/eng1c-val1-grid-fr128-139-en65-74/).
[Previous FR140/EN75 report](RESULTS-ENG-1C-VAL1-LANG-FR140-EN75.md).
[Completed 256/512 then 1024 report](RESULTS-ENG-1C-VAL1-X256-512-1024.md).
