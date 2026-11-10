# DEC-12 pre-registration: a two-stage instruction reader (2026-11-06)

## Why
DEC-4 to DEC-11 used one linear head over (question word × content bit)
conjunctions. On wordings it had never seen, it plateaued at 0.55–0.60.
T1 was stable (about 0.63), and T3 swung from one wording set to the next
(`docs/RESULTS-DEC-11-CAPACITY.md`). Each new phrasing had to be learned for
every content bit at once. DEC-12 splits the job in two:
1. A **router** reads only the question and outputs an intent.
2. The **DEC-2 content heads** answer the passage task. They never see the
   question.

The yes/no answer is `answer(intent, content prediction)`.

## Design (`sml_decision::dec12_router`, runner `examples/dec12_router_heads.rs`)
- **The row's shape gives the task, as in DEC-4.** A T1 row has two passages
  and a T3 row has one. The T1 router is binary (Continues or NotContinues).
  The T3 router has three classes (Opening, Middle, End).
- **Router features.** Hashed question words and word pairs, the 64-bit
  lexicon question code (OEWN, as in DEC-6, two-sided), and the DEC-7
  polarity bit, in a space of 2^16 features.
- **Router training.**
  - Data: one example per wording, from the DEC-10 grammar plus the spent
    DEC-10 and DEC-11 reserved sets (478 wordings). No passage is used.
  - Settings, fixed in advance because the router has no DEV: an averaged
    perceptron for 16 epochs, then quantization to ±1..±7 with no pruning
    (ROUTER_BOOL). The unquantized average is ROUTER_INT.
- **Content heads.** DEC-2 exactly: `select_on_dev` on TRAIN/DEV with DEC-2
  features. T1 is binary; T3 is ordinal over 3 levels.

## Reserved wordings (`dec12_router::reserved`, TEST only)
These were written before any DEC-12 run. `tests/dec12_router.rs` checks
that none of them is in the router training set or in any earlier set.
- Continues: "Is B the passage the author put directly after A?" / "Does
  reading A then B follow the original order?" / "Was B placed immediately
  after A in the paper?" / "Is the text A followed by B?"
- NotContinues: "Is B taken out of order relative to A?" / "Would the author
  not have written B right after A?" / "Is B somewhere else in the text, not
  just after A?" / "Does B skip ahead or come from elsewhere instead of
  following A?"
- Opening: "Is this part of the article's first lines?" / "Does this text come
  at the outset of the paper?"
- Middle: "Is this passage from the middle portion of the paper?" / "Is this
  text located mid-article?"
- End: "Is this part of the article's last lines?" / "Does this text come at
  the close of the paper?"

**Stated limits.**
- One author wrote the router training set and this reserved set.
- The runner's smoke test ran once on synthetic passages with a 4-word toy
  lexicon. That run printed router accuracy on these reserved wordings
  (11/14 Boolean, 10/14 integer). With the real lexicon, the router will
  differ. Nothing was changed after that run.

## Arms (TEST asked with the DEC-4 `seen` wordings and with `reserved`)
- **ROUTER_BOOL** (primary): the quantized router with the quantized DEC-2
  heads.
- **ROUTER_INT**: the integer router with the integer heads.
- **ORACLE_BOOL**: the true intent with the quantized heads. This is the
  ceiling.
- **CONST**: the router always outputs class 0. This scores exactly 0.5 by
  construction.

## Validity (all must hold, else DEC12_INVALID)
1. The two processes give identical score and prediction rows.
2. CONST scores exactly 0.5 on both sets.
3. The quantized content heads reproduce DEC-2 on TEST: T1 0.886 and T3
   0.739, to 3 decimals.

## Verdict (ROUTER_BOOL on `reserved`, read once)
- **DEC12_GO** if accuracy ≥ 0.70, the DEC-4 bar for familiar wordings.
- **DEC12_PARTIAL** if 0.60 ≤ accuracy < 0.70.
- **DEC12_FAIL** if accuracy < 0.60.

Reported, not gated:
- router accuracy per wording (train, seen, reserved) and the route taken by
  each reserved wording;
- the ORACLE ceiling;
- T1, T3 and per-intent accuracy.
