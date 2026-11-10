# DEC-14 pre-registration: anchor-based question routing (2026-11-10)

## Why
DEC-13 (`docs/RESULTS-DEC-13-SCOPE.md`) tested the routers on 50 new wordings. The DEC-12 router scored 0.634 and the DEC-13 router 0.642. Both routed only 33–36 of the 50 wordings correctly. A router of hashed words and word pairs memorises its template wordings. Position words that never appear in the templates ("final", "back", "conclusion", "midsection") do not reach their intent. A scope grammar learned from templates did not generalise either: 4 of 9 contrastive wordings were routed correctly.

DEC-14 tests whether lexical knowledge from Open English WordNet (OEWN) carries new vocabulary to the right intent.

## Design (`sml_decision::dec14_anchor`, runner `examples/dec14_anchor_heads.rs`)
- **Anchors.** There are five anchors. Each has four seed words, fixed in `SEEDS`:

  | Anchor | Seed words |
  |---|---|
  | Opening | beginning, start, first, opening |
  | Middle | middle, center, centre, midst |
  | End | end, last, finish, ending |
  | Near (Continues) | follow, next, continue, after |
  | Away (NotContinues) | elsewhere, other, separate, gap |

- **Expansion.** A word hits an anchor if either holds in the pinned DEC-6 OEWN lexicon (sha `7f74b1d4…`):
  - it shares a synset with a seed word;
  - one of its direct hypernym synsets is a seed synset.

  An unknown word is retried after stripping one suffix: ing, es, ed, ly, s or d. The expansion runs on Thor, and nobody looked at its result before the reserved set was written.
- **Scope rule (fixed).** Only the main clause counts for anchors and their negation. The main clause is the text before the first " rather than ", " instead of " or " as opposed to ".
- **R14 router.**
  - Features: the DEC-12 features of the whole question, plus an anchor block. For each anchor, the block has a one-hot hit count (0, 1, or 2 and more) and a hit × main-clause-negation slot. The dimension is 2^16 + 32.
  - Training: the DEC-12 procedure (16 epochs, ±7) on the DEC-13 training wordings plus the spent DEC-13 reserved set. T1 has 371 wordings and T3 has 429.
- **ANCHOR_RULE.** A rule that uses the anchors alone:
  - T1: compare Near hits with Away hits, swapping them when the main clause is negated.
  - T3: take the unique largest of the Opening, Middle and End hits.
  - On a tie, or when nothing hits, ROUTER14_BOOL decides.
- **Content heads.** DEC-2 exactly.

## Reserved wordings (`dec14_anchor::reserved`, TEST only, read once)
- There are 10 per intent, 50 in all. They were written after the seed list and before any DEC-14 code or run.
- 7 of them are contrastive. Two put "opening" and "closing" on opposite sides of "rather than".
- `tests/dec14_anchor.rs` checks that none is in any training set or any earlier set. It compares case-folded text without punctuation. One first draft, "Does B fail to continue A?", was already spent. It was replaced before any run by "Would B be out of sequence if read right after A?".

**Stated limits.**
- The same author wrote the seeds and the reserved set. Many reserved wordings contain a seed word directly, because seeds are the natural words for each intent. Words reached only through OEWN, such as "initial", "concluding", "tail", "interior", "succeeds" and "consecutive", test the expansion.
- The smoke test ran on synthetic data with a toy lexicon. Its reserved rows were not read.

## Arms (TEST asked with `seen`, `dec13res` (spent) and `reserved`)
- ROUTER14_BOOL (primary)
- ROUTER14_INT
- ANCHOR_RULE
- ROUTER12_BOOL (DEC-12 unchanged)
- ORACLE_BOOL
- CONST

## Validity (all must hold, else DEC14_INVALID)
1. The two processes give identical rows.
2. CONST scores exactly 0.5 on every set.
3. The DEC-2 heads reproduce: 0.886 and 0.739.
4. ROUTER12_BOOL on `dec13res` reproduces DEC-13: 2537/4000.

## Verdict (ROUTER14_BOOL on `reserved`)
- **DEC14_GO** if accuracy ≥ 0.70 and accuracy ≥ ROUTER12_BOOL on `reserved` + 0.03.
- **DEC14_PARTIAL** if accuracy ≥ 0.65 but GO fails.
- **DEC14_FAIL** otherwise.

Reported, not gated:
- ANCHOR_RULE, both on the asked rows and per wording (decided / correct);
- router accuracy with a Wilson interval;
- the route and anchor counts of each reserved wording;
- per-intent accuracy;
- the number of anchor words.
