# DEC-15 pre-registration: antonym polarity and a protected anchor block (2026-11-10)

## Why
DEC-14 (`docs/RESULTS-DEC-14-ANCHOR.md`) reached GO: ROUTER14_BOOL scored 0.728 on 50 new wordings.

Two gaps remain:
- **Quantization costs a lot.** The integer router routes 46/50 wordings correctly (0.793). The ±7 Boolean router gets only 41/50.
- **The misses are lexical.**
  - Middle wordings that hit no anchor: "intermediate", "partway", "mid-paper".
  - Negative wordings with no negation cue: "detached", "unconnected", "out of sequence".

## Changes from DEC-14 (`sml_decision::dec15_polarity`), fixed before the reserved set
1. **Seeds extended.**
   - Middle gets: intermediate, halfway, central.
   - Near gets: consecutive, successive, adjacent, connected.
   - Away gets: distant, remote.
   - Declared: these were chosen after reading the DEC-14 misses on a spent set.
2. **Antonym polarity.**
   - A word is an "anti" hit of an anchor when its OEWN antonym synsets meet that anchor's seed synsets.
   - In T1, anti-Near counts as Away and anti-Away counts as Near.
   - T3 ignores anti hits.
3. **Protected anchor block.**
   - Each anchor feature fires 4 slots (`REP`), so it keeps its weight after the ±7 quantization.
   - The dimension is 2^16 + 100.
   - The router procedure is unchanged: 16 epochs, ±7.
   - The training set is the DEC-14 training set plus the spent DEC-14 reserved set: T1 391, T3 459.

The scope rule, the content heads (DEC-2) and the OEWN lexicon (sha `7f74b1d4…`) are unchanged.

## Reserved wordings (`dec15_polarity::reserved`, TEST only, read once)
- There are 10 per intent, 50 in all, written after the changes above and before any DEC-15 code or run. 7 of them are contrastive.
- `tests/dec15_polarity.rs` checks that none is in any training set or any earlier set.
- Two first drafts were already spent. Before any run, they were replaced:
  - "Is B disconnected from A?" became "… from the passage A?".
  - "Is this from the midpoint of the paper?" became "Does this passage come from around the midpoint of the article?".
- Stated limit: one author wrote the seeds and the reserved set. Some reserved wordings use the new seeds ("distant", "remote", "adjacent", "halfway"), and some use their antonyms ("disconnected").

## Arms (TEST asked with `seen`, `dec14res` (spent) and `reserved`)
- **ROUTER15_BOOL** (primary)
- **ROUTER15_INT**
- **ANCHOR_RULE**: DEC-15 anchors, with ROUTER15_BOOL deciding ties.
- **ROUTER14_BOOL**: DEC-14, unchanged.
- **ORACLE_BOOL**
- **CONST**

## Validity (all must hold, else DEC15_INVALID)
1. The two processes give identical results.
2. CONST scores 0.5 on every set.
3. The DEC-2 heads reproduce: 0.886 and 0.739.
4. ROUTER14_BOOL on `dec14res` reproduces DEC-14: 2912/4000.

## Verdict (ROUTER15_BOOL on `reserved`)
- **DEC15_GO** if accuracy ≥ 0.75 and accuracy ≥ ROUTER14_BOOL on `reserved` + 0.02.
- **DEC15_PARTIAL** if accuracy ≥ 0.70 but GO fails.
- **DEC15_FAIL** otherwise.

Reported, not gated:
- routing accuracy with a Wilson interval;
- the routes and anchor counts of each reserved wording;
- per-intent accuracy;
- the number of anchor words.
