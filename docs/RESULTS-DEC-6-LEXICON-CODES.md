# DEC-6 results — Boolean word codes from Open English WordNet (2026-11-05)

Pre-registration: `docs/DEC-6-LEXICON-CODES-PREREGISTRATION.md` (SML
`876ba566`, SHA-256 checked). Lexicon: OEWN 2025 (CC-BY 4.0), lexicon TSV
SHA-256 `7f74b1d4…` checked (RemoteOps #1260). Run: RemoteOps #1263, Thor.
Two processes, identical codes (lexicon fingerprint `22f62b52…`, 64 889
words), scores and predictions.

## Verdict: **DEC6_SEEN_ONLY** (READS_SEEN yes, READS_NEW no)

BLIND control: exactly 2000/4000 on both sets (valid).

| arm | TEST seen | TEST new |
|---|---|---|
| **SML_BOOL_LEXICON** (primary) | **0.836** (bar 0.70) | 0.571 (bar 0.60) |
| SML_BOOL_LEXICON_CTX | 0.841 | 0.575 |
| DEC-5 SML_BOOL_MEANING (reference) | 0.828 | 0.525 |
| DEC-4 SML_BOOL_INSTR (reference) | 0.774 | 0.515 |
| BLIND | 0.500 | 0.500 |

SML_BOOL_LEXICON per task: seen T1 0.868 (both right 0.830), seen T3 0.804;
new T1 0.491, **new T3 0.651**. Per intent on `new`: Continues 0.159,
NotContinues 0.822, Opening 0.693, Middle 0.581, **End 0.682** (DEC-5: 0.360).

## Reading
- The lexicon fixes what it was meant to fix. Opposites no longer collapse
  (beginning/end 34 bits apart, against 2 in DEC-5; end/final 20). New T3
  wordings rise from 0.578 to 0.651, and "final part" is now read as the end
  (0.68 against 0.36).
- Negation is still not read. New T1 stays at chance (0.491): every new T1
  wording is answered "does not continue". WordNet has no link from "not" to
  "false" or "wrong", and "would ... come directly after" shares no synset
  with the seen continuation wordings.
- The overall new score (0.571) misses the 0.60 bar because of T1. The seen
  score is the best so far (0.836).
- The next step is negation as its own Boolean feature. A polarity bit (a
  negation cue flips the answer) needs labelled negation words, so it must
  be pre-registered with its word list fixed before any run.
