# DEC-7 — negation as a Boolean polarity bit (pre-registration)

Frozen 2026-11-05, after DEC-6 (`docs/RESULTS-DEC-6-LEXICON-CODES.md`,
`DEC6_SEEN_ONLY`: seen 0.836, new 0.571, new T1 0.491) and before any DEC-7
number on bench data. The owner approved follow-up steps in advance.

## Question
DEC-6 separates opposites but cannot read negation in new wordings. If each
question gets one Boolean polarity bit (on when it holds an odd number of
negation cues), does the head read the held-out wordings?

## Cue set (`sml-decision::dec7_polarity`, fixed mechanically)
- The closed list of English negators: no, not, never, none, nor, neither,
  without, cannot, nothing, nobody.
- Plus every word in the pinned DEC-6 lexicon (OEWN 2025, SHA-256 `7f74b1d4…`)
  that has an antonym synset of "true", "right" or "correct". Inflected forms
  are matched through the DEC-6 lemma rules.
- Polarity bit: on iff the number of cue words in the question is odd.

## Model
- Features: DEC-6 SML_BOOL_LEXICON features plus the polarity bit as one more
  tagged code. That code enters alone, with the kind tag, and ANDed with every
  content bit.
- Bench, wordings, heads, training, grid and DEV-only selection: unchanged.
- Arms:
  - **SML_BOOL_POLARITY** (primary): lexicon code and polarity bit;
  - SML_BOOL_POLARITY_CTX (descriptive): plus the DEC-5 context code;
  - integer heads (descriptive);
  - BLIND (control, must score 0.500).

## Criteria (SML_BOOL_POLARITY on TEST)
- `READS_SEEN`: accuracy on TEST `seen` ≥ 0.70.
- `READS_NEW`: accuracy on TEST `new` ≥ 0.60.
- **DEC7_GO**: both. **DEC7_SEEN_ONLY**, **DEC7_NEW_ONLY**, **DEC7_NO_GO**
  as in DEC-5/6.
- **DEC7_INVALID** if BLIND ≠ 0.500 on either set.
- **NO_GO_DETERMINISM** if two processes differ in `DEC7_CODES`/`DEC7_SCORE`/
  `DEC7_PRED`.
- Reported, not judged:
  - the other arms;
  - per-task and per-intent accuracy;
  - the cue list (`DEC7_CUES`);
  - the polarity and cues of every wording (`DEC7_QCODE`).

## Run
Thor, same pinned bench, TSV export and lexicon as DEC-6, two processes,
pinned to this SML commit (RemoteOps PR).

## Known limits (read before the result)
- **The held-out wordings were known when this rule was written.** The rule
  is general: a closed negator list plus antonyms of truth words, with nothing
  listed per wording. But it does cover the two held-out negations ("false",
  "wrong"), and that was foreseen. A `READS_NEW` here is weaker evidence than
  in DEC-4 to DEC-6. It shows that a Boolean polarity bit, once it fires, is
  used correctly; it does not show that the cue was discovered.
- Only one of the three seen `NotContinues` wordings carries a cue ("not").
  The other two ("other than", "out of place") do not, so the head must learn
  the polarity bit from a third of the negative T1 questions.
- The rule also marks antonyms of other senses of "right" (for example
  "left") as cues. That is harmless here, but wrong in general.
