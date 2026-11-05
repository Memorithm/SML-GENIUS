# DEC-8 pre-registration: the DEC-7 heads on a fresh reserved wording set (2026-11-05)

## Why
DEC-4 to DEC-7 judged every change on the same held-out wordings (TEST
`new`), which were read four times; the DEC-7 negation rule was written while
they were known. DEC-7 missed the `new` bar by 0.008. DEC-8 asks whether the
DEC-7 result holds on wordings that no run has seen. Nothing in the model
changes: the only addition is a third TEST variant, `fresh`.

The user chose (decision card) to write these wordings themselves, then asked
to continue without waiting. They were therefore written by the model's
author, after DEC-7, and frozen here before any run. **Stated limit:** the
author knows the cue list (the 10 closed negators plus lexicon antonyms of
true/right/correct). The wordings were written as natural questions, not
checked against the cue list or the word codes before this commit. Some
negative wordings carry no cue ("unrelated", "fail to"), and one Middle
wording carries two ("neither ... nor", even parity, so not negated). This is
left as written.

## Fresh wordings (frozen, `sml_decision::dec8_fresh::fresh`)
- Continues: "Is B the continuation of A?" / "Does A lead straight into B?" /
  "Could B be the sentence that comes after A in the original text?" /
  "Is B the passage that immediately follows A?"
- NotContinues: "Is B unrelated to what comes after A?" / "Does B fail to
  follow A?" / "Is it incorrect to say that B comes after A?" / "Was B not
  written right after A?"
- Opening: "Is this the first part of the paper?" / "Does this passage open
  the article?"
- Middle: "Is this passage from neither the start nor the end of the
  article?" / "Does this text sit halfway through the paper?"
- End: "Is this the last part of the article?" / "Does this passage conclude
  the paper?"

The test `tests/dec8_fresh.rs` checks that none of the 14 appears in `seen`
or `held_out`.

## Protocol
Identical to DEC-7 (same bench, lexicon sha 7f74b1d4…, features, arms, DEV
grid, row pairing and wording choice by split/id/intent). Runner
`examples/dec8_fresh_heads.rs`, two processes on Thor.

## Validity checks (all must hold, else DEC8_INVALID)
1. The two processes print identical `DEC8_PRED` digests.
2. SML_BOOL_POLARITY reproduces DEC-7: TEST seen 0.839 and TEST new 0.592
   (to 3 decimals).
3. BLIND scores exactly 0.5000 on `fresh`.

## Verdict (primary arm SML_BOOL_POLARITY, TEST `fresh`)
- **DEC8_FRESH_PASS** if accuracy ≥ 0.60.
- **DEC8_FRESH_FAIL** otherwise.

Reported, not gated: the other arms, fresh T1 and T3, per-intent accuracy,
the cues found in each fresh wording.

## After the run
TEST `fresh` is then read once. Whatever the verdict, no change to the model
will be judged on it again; a later step needs another fresh set.
