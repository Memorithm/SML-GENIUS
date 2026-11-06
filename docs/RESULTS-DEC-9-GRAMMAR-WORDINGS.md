# DEC-9 results — question-reading heads trained on many wordings (2026-11-06)

Pre-registration: `docs/DEC-9-GRAMMAR-WORDINGS-PREREGISTRATION.md` (SML
`fa25011f`, SHA-256 checked). Run: RemoteOps #1278, Thor, job 127679006991.
Two processes, identical outputs.

## Verdict: **DEC9_FAIL** (reserved 2397/4000 = 0.5992, bar 0.60: 3 rows short)

Validity: all checks hold. SML_BOOL_BASE reproduces DEC-7 (seen 3356/4000),
and BLIND scores 2000/4000. All 333 grammar wordings appear in TRAIN.

| arm | seen (DEC-4) | **reserved (read once)** |
|---|---|---|
| **SML_BOOL_GRAMMAR** (primary) | 0.754 | **0.599** |
| SML_INT_GRAMMAR | 0.756 | 0.618 |
| SML_BOOL_BASE (= DEC-7) | 0.839 | 0.506 |
| SML_INT_BASE | 0.847 | 0.524 |
| BLIND | 0.500 | 0.500 |

The gain over BASE is **+0.094**, three times the pre-registered 0.03. The
verdict is still FAIL, because the absolute bar comes first.

Primary arm on `reserved`, against BASE:

| | GRAMMAR | BASE |
|---|---|---|
| T1 | 0.645 | 0.500 |
| T3 | 0.554 | 0.512 |
| Continues | 0.611 | 0.225 |
| NotContinues | 0.679 | 0.774 |
| Opening | 0.512 | 0.280 |
| Middle | 0.687 | 0.573 |
| End | 0.454 | 0.684 |

## Reading
- Training on many wordings is the lever DEC-8 pointed to. It removes the
  "unseen positive question → no" failure (Continues 0.225 → 0.611). T1 on
  reserved wordings reaches 0.645, and both questions of a pair are now
  answered correctly 0.376 of the time, against 0.155 before.
- T3 stays weak (0.554). The grammar only varies the position noun inside a
  fixed set of "from the {n} of the article" frames. The reserved wordings use
  verbs ("begin with", "finish with") and spatial words ("top", "bottom",
  "midway", "body"). End drops below chance, 0.454.
- On the DEC-4 wordings, accuracy falls from 0.839 to 0.754. The same capacity
  is now spread over 333 wordings instead of 13.
- This reserved set has now been read once. The next step needs a new one.
