# DEC-13 pre-registration: a 50-wording reserved set and negation scope (2026-11-10)

## Why
DEC-12 (`docs/RESULTS-DEC-12-ROUTER.md`) reached 0.796 on its reserved set.
That set had only 14 wordings, and the router routed 13 of them correctly. One
wording more or less moves the score by several points, so 0.796 has no
usable error bar. The one miss was a scope error. In "Does B skip ahead or
come from elsewhere instead of following A?", the router read "following A"
as the intent, but "instead of" negates it.

DEC-13 asks two questions:
1. Does the DEC-12 result hold on a much larger reserved set?
2. Does a small "X rather than / instead of / as opposed to Y" grammar in the
   router training fix scope without hurting the rest?

## Design (`sml_decision::dec13_scope`, runner `examples/dec13_scope_heads.rs`)
- **R12 router.** The DEC-12 router, unchanged: same 478 training wordings,
  features, 16 epochs and ±7 quantization.
- **R13 router.** Same features and procedure as R12. Its training wordings
  are:
  - the DEC-12 training wordings;
  - the spent DEC-12 reserved set (14);
  - `scope_grammar`.
    - T1 Continues: "Does B {near} {link} {away-ing}?" (48).
    - T1 NotContinues: "Does B {away} {link} {near-ing}?" (48).
    - T3: "Is this from {place of the intent} {link} {place of another
      level}?" (54 per level).
    - Links: "rather than", "instead of", "as opposed to".
  - In total: T1 351, T3 399.
- **Content heads.** DEC-2 exactly (`select_on_dev`). They never see the
  question.

## Reserved wordings (`dec13_scope::reserved`, TEST only, read once)
- There are 10 per intent, 50 in all, written before the scope grammar and
  before any DEC-13 run.
- 9 of them are contrastive ("X rather than / instead of / as opposed to Y",
  true intent X). They form the scope subset. Their fillers are not those of
  the grammar ("another section", "sequel", "jumping somewhere else",
  "introduction", "conclusion", "core", "edges").
- `tests/dec13_scope.rs` checks that none of them is in any training set or
  in any earlier set. It compares case-folded text without punctuation. One
  first-draft wording ("Is B not the next passage after A?") was already
  spent and was replaced before any run by "Does A lead somewhere other than
  into B?".
- The full list is in the source. See `crates/sml-decision/src/dec13_scope.rs`.

**Stated limits.**
- One author wrote the training grammar and the reserved set, knowing the
  DEC-12 miss.
- The runner's smoke test ran once on synthetic passages with a toy lexicon.
  Its output was deleted without reading the reserved rows. Nothing was
  changed after that run.

## Arms (TEST asked with `seen`, `dec12res` and `reserved`)
- **ROUTER13_BOOL** (primary): the quantized R13 router with the quantized
  DEC-2 heads.
- **ROUTER13_INT**: the integer R13 router with the integer heads.
- **ROUTER12_BOOL**: the DEC-12 router with the quantized heads.
- **ORACLE_BOOL**: the true intent. This is the ceiling.
- **CONST**: router class 0. This scores exactly 0.5 by construction.

`dec12res` serves only to check that ROUTER12_BOOL reproduces DEC-12. R13
trains on it.

## Validity (all must hold, else DEC13_INVALID)
1. The two processes give identical rows.
2. CONST scores exactly 0.5 on every set.
3. The quantized content heads reproduce DEC-2: T1 0.886 and T3 0.739.
4. ROUTER12_BOOL on `dec12res` reproduces DEC-12 (0.796, 3183/4000).

## Verdict (ROUTER13_BOOL on `reserved`)
- **DEC13_GO** if both hold:
  - accuracy ≥ 0.75;
  - accuracy ≥ ROUTER12_BOOL on `reserved` − 0.01, so the scope grammar does
    not hurt.
- **DEC13_PARTIAL** if accuracy ≥ 0.65 but GO fails.
- **DEC13_FAIL** otherwise.

Reported, not gated:
- router accuracy per wording, with a Wilson 95% interval over the 50
  wordings, for R12 and R13;
- router accuracy on the scope subset;
- the route of each reserved wording;
- the ORACLE ceiling;
- T1, T3 and per-intent accuracy.
