# DEC-10 pre-registration: a richer wording grammar, new reserved set (2026-11-06)

## Why
DEC-9 (`docs/RESULTS-DEC-9-GRAMMAR-WORDINGS.md`) trained on 333 grammar
wordings. It scored 0.599 on its reserved set, three rows under the bar, and
+0.094 over DEC-7. T1 reached 0.645, but T3 stayed at 0.554: its frames only
varied the position noun. DEC-10 changes only the grammar. Features, heads,
DEV grid and bench are unchanged.

## Wordings (`sml_decision::dec10_grammar`)
- **Reserved** (`reserved`), 14 wordings written before the additions below.
  They are used only on TEST:
  - Continues: "Does B pick up right where A stops?" / "Is B the following part
    of A's text?" / "Would B be next in line after A?" / "Does B come straight
    on from A?"
  - NotContinues: "Is B from a different place than right after A?" / "Does B
    not belong right after A?" / "Would B be an odd thing to read after A?" /
    "Is it a stretch to say B comes after A?"
  - Opening: "Is this one of the first things the article says?" / "Does this
    passage start the paper off?"
  - Middle: "Is this passage somewhere in the middle stretch of the article?" /
    "Does this text fall between the start and the end of the paper?"
  - End: "Is this one of the last things the article says?" / "Does this
    passage wrap up the paper?"
- **Grammar** (`grammar`), 450 wordings (95 / 136 / 76 / 59 / 84). It is made
  of the DEC-9 grammar, plus the 14 spent DEC-9 reserved wordings, plus these
  frames:
  - T1 noun frames ("Is B the {continuation, next part, …} of A?" and their
    negations).
  - T3 document-verb frames ("Does the article {begin with, end with, …} this
    text?").
  - T3 passage-verb frames ("Does this passage {open, conclude, …} the
    article?").
  - T3 place frames ("Is this {near the top of the paper, halfway through the
    article, at the very end of the article, …}?").
  - `tests/dec10_grammar.rs` checks that no reserved wording reaches training.

**Stated limits.**
- One author wrote both sets, the reserved set first.
- While writing the grammar, I left out two passage verbs ("start", "wrap
  up"). Each would have produced a wording one word away from a reserved one.
- I also changed one reserved wording before freezing it, from "Is it untrue
  that B comes next after A?" to "Is it a stretch to say B comes after A?".
  The grammar already holds "Is it untrue that B can come next after A?".
- Both steps were taken to keep the test honest. They also show that the
  author saw both sets at once. The user's own wordings remain the better
  test.

## Arms (runner `examples/dec10_grammar_heads.rs`, two processes on Thor)
- **SML_BOOL_GRAMMAR10** (primary): DEC-7 features, with TRAIN and DEV asked
  using `dec10_grammar::grammar`.
- **SML_BOOL_GRAMMAR9**: the DEC-9 primary arm, exactly.
- Integer heads of both arms, and BLIND.
- TEST is asked twice: `seen` (the DEC-4 wordings) and `reserved`.

## Validity (all must hold, else DEC10_INVALID)
1. The two processes give identical score and prediction rows.
2. SML_BOOL_GRAMMAR9 reproduces DEC-9 on `seen`: 3016/4000.
3. BLIND scores exactly 0.5 on `reserved`.

## Verdict (TEST `reserved`, read once)
Let g be the SML_BOOL_GRAMMAR10 accuracy and b the SML_BOOL_GRAMMAR9
accuracy.
- **DEC10_GO** if g ≥ 0.60 and T3 accuracy ≥ 0.58.
- **DEC10_T1_ONLY** if g ≥ 0.60 and T3 < 0.58.
- **DEC10_FAIL** if g < 0.60.

Reported, not gated: g − b, `seen`, T1, per-intent accuracy, and the cues of
each reserved wording.
