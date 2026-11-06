# DEC-8 results — the DEC-7 heads on a fresh reserved wording set (2026-11-05)

Pre-registration: `docs/DEC-8-FRESH-WORDINGS-PREREGISTRATION.md` (SML
`bf84110e`, SHA-256 checked). Run: RemoteOps #1274, Thor, job 127575713936.
Two processes, identical outputs.

## Verdict: **DEC8_FRESH_FAIL** (fresh 0.559, bar 0.60)

Validity: every check held. The two processes match, and SML_BOOL_POLARITY
reproduces DEC-7 exactly (seen 3356/4000 = 0.839, new 2368/4000 = 0.592).
BLIND scores 2000/4000 on all three sets.

| arm | seen | new (read 5×) | **fresh (read once)** |
|---|---|---|---|
| **SML_BOOL_POLARITY** (primary) | 0.839 | 0.592 | **0.559** |
| SML_INT_POLARITY | 0.847 | 0.572 | 0.579 |
| SML_BOOL_POLARITY_CTX | 0.833 | 0.566 | 0.509 |
| SML_INT_POLARITY_CTX | 0.854 | 0.583 | 0.513 |
| BLIND | 0.500 | 0.500 | 0.500 |

Primary arm on `fresh`:
- By task: T1 0.561, T3 0.556.
- By intent: Continues 0.408, NotContinues 0.714, Opening 0.492, Middle 0.484, End 0.698.

The polarity bit fired on 2 of the 4 fresh negative wordings ("incorrect" and
"not"). It did not fire on "fail to" or "unrelated". "neither … nor" has even
parity, as stated before the run.

## Reading
- On wordings nobody had seen, the head is above chance (0.559 against 0.500)
  but below the bar. The `new` score of 0.592 was slightly optimistic, which
  is consistent with that set having been read many times.
- The pattern is the same as in DEC-7. A positive question that looks unlike
  the 3 seen positive wordings tends to be read as negative (Continues
  0.408). Opening and Middle drop to chance on new verbs ("open", "sit
  halfway"). End holds ("last", "conclude" sit close to "end" in the lexicon).
- **Diagnosis.** The heads learn from only 13 wordings (3 + 3 + 2 + 2 + 2).
  Word codes and the polarity bit help them reach a little past those 13, but
  not far enough. The next lever is the number of wordings seen in training,
  not another word-level feature.
- TEST `fresh` has now been read once. No further change will be judged on it.
