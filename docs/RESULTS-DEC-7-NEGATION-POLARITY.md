# DEC-7 results — negation as a Boolean polarity bit (2026-11-05)

Pre-registration: `docs/DEC-7-NEGATION-POLARITY-PREREGISTRATION.md` (SML
`ebad2605`, SHA-256 checked). Run: RemoteOps #1265, Thor, same pinned bench
and lexicon as DEC-6. Two processes, identical outputs.

## Verdict: **DEC7_SEEN_ONLY** (READS_SEEN yes, READS_NEW no, by 0.008)

BLIND control: exactly 2000/4000 on both sets (valid). Cue set: 22 words
(the 10 closed negators plus false, falsify, incorrect, incorrectly,
improperly, untruthful, wrong, wrongly, wrongfulness, left, center, partly).

| arm | TEST seen | TEST new |
|---|---|---|
| **SML_BOOL_POLARITY** (primary) | **0.839** (bar 0.70) | **0.592** (bar 0.60) |
| SML_BOOL_POLARITY_CTX | 0.833 | 0.566 |
| SML_INT_POLARITY | 0.847 | 0.572 |
| DEC-6 SML_BOOL_LEXICON (reference) | 0.836 | 0.571 |
| BLIND | 0.500 | 0.500 |

SML_BOOL_POLARITY per task: new T1 0.521 (DEC-6 0.491), new T3 0.664. Per
intent on `new`: Continues 0.184, **NotContinues 0.857**, Opening 0.701,
Middle 0.610, End 0.682.

## Reading
- The polarity bit fires as designed: both held-out negations ("false",
  "wrong") are marked, and the seen "not" wording is marked. New NotContinues
  rises to 0.857. As stated before the run, this is weak evidence, because
  the rule was written with the held-out wordings known.
- The remaining failure is the held-out **positive** T1 wordings ("Would B
  come directly after A?", "Is B what the author wrote next after A?"): 0.184.
  Two of the three seen negative wordings carry no cue ("other than", "out of
  place"), so the head cannot treat "no cue" as "positive". It still keys on
  word codes, and the new positive wordings look like the seen negative ones.
- **TEST `new` has now been read four times (DEC-4 to DEC-7).** Further
  changes judged on it would amount to tuning on it. Any next step must use a
  fresh held-out wording set, written before the run and ideally not by the
  author of the model.
