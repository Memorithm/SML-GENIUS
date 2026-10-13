# ENG-1c/VAL1-LANG-FR140-EN75 — exploratory language-specific thresholds

Measured 2026-10-13 on local x86_64 Linux with Rust 1.89.0.
Protocol frozen before implementation and scoring at
42c3c59bdeeb5e9d033b1b612340511b05446b88.
[Protocol](ENG-1C-VAL1-LANG-FR140-EN75-EXPLORATORY-PROTOCOL.md).

The user selected threshold 140 for FR and 75 for EN. The new fifteenth arm
uses the declared pinned corpus identity to select its threshold, while all
fourteen existing comparators remain unchanged. This is a further post-hoc
refinement on exposed Dumas/Austen VAL, not independent confirmation.
TEST is not scored or created as a file.

## Matched results

Lower bits/byte is better. Same immutable full TRAIN-only depth32 paged
Witten–Bell store, 64-lane VAL histories and denominator across all arms:
74,688 FR targets and 73,728 EN targets. Both TRAIN and VAL SHA256 were
independently rechecked before reuse.

| Language | Policy | Bits/byte | Accuracy |
|---|---|---:|---:|
| FR | FIXED_4 | 2.063036593417 | 56.5633% |
| FR | MIN_COUNT_128 | 2.061957644842 | 56.6169% |
| FR | MIN_COUNT_140 | 2.062213845512 | 56.5941% |
| EN | FIXED_4 | 1.876564702039 | 60.4180% |
| EN | MIN_COUNT_64 | 1.870807958960 | 60.5889% |
| EN | MIN_COUNT_75 | 1.871936565102 | 60.5767% |

Both new losses are slightly lower than FIXED_4, but higher than their
previously best exploratory thresholds. No refinement advantage is observed
versus FR128 or EN64. Differences below are new loss minus the prior best;
positive means the new threshold is worse.

| Language / refinement | New minus previous best bits/byte |
|---|---:|
| FR / 140 vs MIN_COUNT_128 | 0.000256200670 |
| EN / 75 vs MIN_COUNT_64 | 0.001128606142 |

## Descriptive uncertainty against FIXED_4

These paired intervals compare the new arm with FIXED_4, not the earlier
best adaptive arm. No paired uncertainty versus the earlier best was
computed. Use the unchanged 2048-draw paired 64-lane bootstrap, seed
0xE1C5_2026_1013_B001, order statistics 51/1996. Positive gain is fixed4 loss
minus candidate loss. On these exposed data the intervals are descriptive.

| Language / threshold | Gain bits/byte | Approx. 95% paired interval |
|---|---:|---|
| FR / 140 | 0.000822747904 | [-0.000021703956, 0.001562763957] |
| EN / 75 | 0.004628136937 | [0.002554426428, 0.006749495937] |

FR140's interval includes zero; EN75's interval is positive in this
descriptive bootstrap. Both gains are below the original 0.010000-bit/byte
effect-size criterion. Overall best policies remain FR128 and EN64, and
both printed verdicts remain EXPLORATORY_VAL_STOP_NOT_PROVEN.

## Actual selected depths

| Language / threshold | depth4 | depth7 | depth12 | depth32 |
|---|---:|---:|---:|---:|
| FR / 140 | 73165 | 1523 | 0 | 0 |
| EN / 75 | 68149 | 5487 | 92 | 0 |

Both refinements use deep contexts less often than FR128 or EN64.
The same full depth32 store remains retained: 565,374,640 counted bytes FR
and 551,748,904 EN. No physical storage/RAM-saving claim; process VmHWM
excludes filesystem-cache memory. Original 262144-KiB virtual-address limit
and 3000-second process deadline used; no native Thor or GPU timing.

## Verification and evidence

All fourteen earlier predictive rows reproduce exactly, including total
bits, accuracy, selected depths and bootstrap bounds. New language-specific
arms were scored once locally; dedicated CI is configured to repeat all
fifteen arms and check the explicit effective threshold metadata.

Strict Rust 1.89 fmt, warnings-as-errors Clippy and full workspace tests
passed through the versioned pre-push hook. All six evaluator modes preserve
the earlier arms. A separate real paged-store test creates a suffix with
exactly 128 TRAIN occurrences: EN75 selects depth7, FR140 falls back to
depth4; all fourteen ordinary arms stay identical between language inputs,
and an unknown corpus identity is rejected.

[Aggregate evidence, source/binary SHA256 and manifest](evidence/eng1c-val1-lang-fr140-en75/).
[Completed 256/512 then 1024 experiment](RESULTS-ENG-1C-VAL1-X256-512-1024.md).

No model default or SML25B promotion. Any confirmatory quality result requires
a frozen configuration and fresh independent text.
