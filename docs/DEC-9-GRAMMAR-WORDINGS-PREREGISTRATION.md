# DEC-9 pre-registration: question-reading heads trained on many wordings (2026-11-06)

## Why
DEC-8 (`docs/RESULTS-DEC-8-FRESH-WORDINGS.md`): the DEC-7 heads score 0.559
on 14 wordings nobody had seen, below the 0.60 bar. They learn from only 13
wordings. DEC-9 changes only that: TRAIN and DEV are asked with a few hundred
wordings. Features, heads, DEV grid and bench are unchanged.

## Wordings (`sml_decision::dec9_grammar`)
- **Reserved** (`reserved`), 14 wordings written first, before the grammar.
  They are used only on TEST:
  - Continues: "Is B the next bit of A?" / "Does the text go on from A to B?" /
    "Is B where A carries on?" / "Would a reader see B right after A?"
  - NotContinues: "Is B misplaced after A?" / "Does B break the flow from A?" /
    "Is B not the next passage after A?" / "Would it be a mistake to put B
    after A?"
  - Opening: "Is this passage near the top of the paper?" / "Does the article
    begin with this text?"
  - Middle: "Is this from the body of the article, away from both ends?" /
    "Does this passage come midway through the paper?"
  - End: "Is this near the bottom of the paper?" / "Does the article finish
    with this text?"
- **Grammar** (`grammar`), used for TRAIN and DEV, 333 wordings in all
  (80 / 120 / 44 / 39 / 50):
  - T1 positive: 6 frames × 12 "continue" phrases.
  - T1 negative: 8 negated frames × 12 phrases, plus 3 frames × 6 "break"
    phrases.
  - T3: 6 frames × 6–8 position nouns.
  - Plus every wording already spent as a test in DEC-4 to DEC-8.
  - Exact copies of a reserved wording are removed (one: "Is B misplaced after
    A?"). `tests/dec9_grammar.rs` checks that no reserved wording reaches
    training.

**Stated limit.** One author wrote both sets, the reserved one first. Words
are shared on purpose, as they would be in any language ("after", "end",
"not", "misplaced"). The reserved set tests new sentence frames and new
phrasings ("next bit", "carries on", "top", "bottom", "body", "finish",
"break the flow", "a mistake to"), not unseen words. The user's own
wordings would make a stronger test. They will be used for a later step if
the user sends them.

## Arms (runner `examples/dec9_grammar_heads.rs`, two processes on Thor)
- **SML_BOOL_GRAMMAR** (primary): DEC-7 features (lexicon code and polarity
  bit). TRAIN and DEV are asked with grammar wordings, and the question coder
  is fitted on those TRAIN questions.
- **SML_BOOL_BASE**: DEC-7 SML_BOOL_POLARITY exactly (TRAIN and DEV with
  `dec4_instr::seen`).
- Integer heads of both arms, and BLIND.
- TEST is asked twice: `seen` (the DEC-4 wordings) and `reserved`.

## Validity (all must hold, else DEC9_INVALID)
1. The two processes give identical score and prediction rows.
2. SML_BOOL_BASE reproduces DEC-7 on `seen`: 3356/4000.
3. BLIND scores exactly 0.5 on `reserved`.

## Verdict (TEST `reserved`, read once)
Let g be the SML_BOOL_GRAMMAR accuracy and b the SML_BOOL_BASE accuracy.
- **DEC9_GO** if g ≥ 0.60 and g ≥ b + 0.03.
- **DEC9_PASS_NO_GAIN** if g ≥ 0.60 and g < b + 0.03.
- **DEC9_FAIL** if g < 0.60.

Reported, not gated: `seen` accuracy, T1 and T3, per-intent accuracy, and
the cues found in each reserved wording.
