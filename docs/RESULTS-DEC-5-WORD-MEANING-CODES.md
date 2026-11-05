# DEC-5 results — Boolean word-meaning codes (2026-11-05)

Pre-registration: `docs/DEC-5-WORD-MEANING-CODES-PREREGISTRATION.md` (SML
`241efc8a`, SHA-256 checked). Run: RemoteOps #1255, Actions run 42018861559,
Thor. Two processes: identical codes (fingerprint `a5e2de65…`, 27 477 words
from 92 629 TRAIN texts), scores and predictions.

## Verdict: **DEC5_SEEN_ONLY** (READS_SEEN yes, READS_NEW no)

BLIND control: exactly 2000/4000 on both sets (valid).

| arm | TEST seen | TEST new |
|---|---|---|
| **SML_BOOL_MEANING** (primary) | **0.828** (bar 0.70) | 0.525 (bar 0.60) |
| SML_INT_MEANING | 0.840 | 0.534 |
| SML_BOOL_MEANING_ID | 0.829 | 0.551 |
| DEC-4 SML_BOOL_INSTR (reference) | 0.774 | 0.515 |
| BLIND | 0.500 | 0.500 |

SML_BOOL_MEANING per task: seen T1 0.865 (both right 0.816), seen T3 0.792;
new T1 0.473, new T3 0.578. Per intent on `new`: Continues 0.151,
NotContinues 0.794, Opening 0.787, Middle 0.581, End 0.360.

## Reading
- Coding questions through learned word codes makes the Boolean head **read
  seen questions much better** than word identities: 0.828 against 0.774
  (T1 both-right 0.82 against 0.66), with a 3-plane head of 32 252 weights.
- New wordings still fail. The code distances explain it:
  - antonyms collapse: beginning/end 2 bits apart, opening/end 5, end/final 5
    (out of 64). The ±2 word window sees "the ___ of the", so position words
    share a code whatever their meaning;
  - negation is not learned: not/false 25 bits and not/wrong 28, near the
    ~32 of unrelated words.
  - So on new wordings every T1 question is read as "not continue" (0.79 on
    NotContinues, 0.15 on Continues), and "final part" reads as "opening".
- Context codes from 23 MB with a small window carry topic, not polarity or
  opposition. Reading new wordings needs a signal that separates opposites,
  for example codes trained on the decision itself (supervised word codes,
  from TRAIN questions only, which a held-out word cannot use), or a much
  larger corpus with wider and ordered contexts. This is not pre-registered yet.
