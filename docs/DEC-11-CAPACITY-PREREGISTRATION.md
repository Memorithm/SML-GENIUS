# DEC-11 pre-registration: more training capacity for the DEC-10 heads (2026-11-06)

## Why
DEC-10 (`docs/RESULTS-DEC-10-RICHER-GRAMMAR.md`) passed narrowly on
reserved wordings (0.605). Accuracy on the familiar DEC-4 wordings fell as
the grammar grew: 0.839 (DEC-7), 0.754 (DEC-9), 0.681 (DEC-10). In DEC-9 and
DEC-10, DEV picked the largest epoch count in the grid (8) for every
question arm, which suggests the heads are under-trained. DEC-11 changes only
the DEV grid. The grammar, features, bench and lexicon are those of DEC-10.

The user asked (6 Nov, 08:28) that I work autonomously and no longer ask
them. So this reserved set is again written by me, with the same limit
stated as before.

## Change
- Epoch grid {1, 2, 4, 8} → {1, 2, 4, 8, 16, 32}.
- Level grid {1, 3, 7} → {1, 3, 7, 15}.
- The keep_div grid and the tie rules are unchanged
  (`dec2_text::select_on_dev_grid`). `select_on_dev_dim` now calls it with
  the DEC-2 grids, so every earlier arm is reproduced unchanged.

## Reserved wordings (`sml_decision::dec11_capacity::reserved`, TEST only)
Written before any DEC-11 run. `tests/dec11_capacity.rs` checks that none of
them appears in the DEC-10 grammar or in any earlier set.
- Continues: "Does the passage A run on into B?" / "Is B what you would read
  once A is over?" / "Could B be the next lines after A?" / "Is B a natural
  follow-on to A?"
- NotContinues: "Is B a jump away from where A ends?" / "Would B feel out of
  sequence after A?" / "Is B not what follows A?" / "Is there a gap between A
  and B in the text?"
- Opening: "Is this taken from the very first page of the paper?" / "Does this
  passage appear before most of the article?"
- Middle: "Is this passage drawn from the center of the paper?" / "Does this
  text come from neither the opening nor the closing of the article?"
- End: "Is this taken from the final page of the paper?" / "Does this passage
  appear after most of the article?"

## Arms (runner `examples/dec11_capacity_heads.rs`, two processes on Thor)
- **SML_BOOL_LONG** (primary): the DEC-10 grammar and features, with the
  DEC-11 grid.
- **SML_BOOL_GRAMMAR10**: the DEC-10 primary exactly.
- The integer heads of both, and BLIND.
- TEST is asked twice: with the DEC-4 `seen` wordings and with `reserved`.

## Validity (all must hold, else DEC11_INVALID)
1. The two processes give identical score and prediction rows.
2. SML_BOOL_GRAMMAR10 reproduces DEC-10 on `seen`: 2722/4000.
3. BLIND scores exactly 0.5 on `reserved`.

## Verdict
Let r be SML_BOOL_LONG on `reserved`, read once, and s be SML_BOOL_LONG on
`seen`.
- **DEC11_GO** if r ≥ 0.60 and s ≥ 0.72 (capacity recovers on familiar
  wordings without losing new ones).
- **DEC11_NEW_ONLY** if r ≥ 0.60 and s < 0.72.
- **DEC11_FAIL** if r < 0.60.

Reported, not gated: the epochs and levels DEV picks, SML_BOOL_GRAMMAR10 on
`reserved` (the effect of the grid alone), T1 and T3, and per-intent
accuracy.
