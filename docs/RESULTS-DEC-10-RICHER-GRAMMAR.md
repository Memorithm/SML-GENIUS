# DEC-10 results — question-reading heads on a richer wording grammar (2026-11-06)

Pre-registration: `docs/DEC-10-RICHER-GRAMMAR-PREREGISTRATION.md` (SML
`e23265d5`, SHA-256 checked). Run: RemoteOps #1279, Thor, job 127685733396.
Two processes, identical outputs.

## Verdict: **DEC10_GO** (reserved 2419/4000 = 0.605, bar 0.60; T3 0.623, bar 0.58)

Validity: all checks hold. SML_BOOL_GRAMMAR9 reproduces DEC-9 on `seen`
(3016/4000), and BLIND scores 0.5 on `reserved`. All 450 grammar wordings
appear in TRAIN.

| arm | seen (DEC-4) | **reserved (read once)** | T1 | T3 |
|---|---|---|---|---|
| **SML_BOOL_GRAMMAR10** (primary) | 0.681 | **0.605** | 0.587 | 0.623 |
| SML_INT_GRAMMAR10 | 0.706 | 0.628 | | |
| SML_BOOL_GRAMMAR9 (= DEC-9) | 0.754 | 0.574 | 0.515 | 0.633 |
| SML_INT_GRAMMAR9 | 0.756 | 0.570 | | |

Per intent on `reserved`, GRAMMAR10 against GRAMMAR9:

| intent | GRAMMAR10 | GRAMMAR9 |
|---|---|---|
| Continues | 0.536 | 0.479 |
| NotContinues | 0.638 | 0.550 |
| Opening | 0.696 | 0.684 |
| Middle | 0.659 | 0.667 |
| End | 0.508 | 0.543 |

## Reading (honest)
- **This is the first pass on wordings no run had seen**, but the margin is
  narrow: 19 rows above the bar, +0.031 over DEC-9.
- **The gain comes from T1, not from the new T3 frames.** T1 rose from 0.515
  to 0.587, driven by the noun frames and by the spent DEC-9 reserved
  wordings. T3 passes its bar, but the DEC-9 heads score as well on this T3
  set (0.633 against 0.623). The T3 bar is met because these reserved T3
  wordings are easier, not because of the T3 additions. End stays near chance.
- **There is a cost on familiar wordings.** On the DEC-4 wordings, accuracy is
  0.839 for DEC-7, 0.754 for DEC-9 and 0.681 for DEC-10. Each wider grammar
  spreads the same heads over more wordings. The integer heads hold up better
  (0.706 / 0.628). The ±1/±3/±7 quantization is starting to cost capacity.
- Both of these reserved sets were written by the author of the model, with
  the limits stated. A set written by the user would be the real test of
  generalization.
