# DEC-15 results: antonym polarity and a protected anchor block (2026-11-10)

Pre-registration: `docs/DEC-15-POLARITY-PREREGISTRATION.md` (sha256 `b301a082…`, SML `b6a5e5d5`).
Run: RemoteOps #1488, workflow run 42904533638, job 130138952046 (Thor). Both processes gave identical results.

## Verdict: **DEC15_FAIL**

ROUTER15_BOOL scored 2684/4000 = 0.671 on the reserved set. The PARTIAL bar was 0.70 and the GO bar 0.75.

All validity checks hold:
- CONST scores 2000/4000 on every set.
- The DEC-2 heads reproduce 886/1000 and 739/1000.
- ROUTER14_BOOL reproduces DEC-14 with 2912/4000.
- The lexicon yields 726 anchor words, up from 689 in DEC-14.

| Arm | DEC-15 reserved (50 new wordings) | T1 | T3 | scope rows |
|---|---|---|---|---|
| **ROUTER15_BOOL** (primary) | **0.671** | 0.696 | 0.647 | 0.876 |
| ROUTER15_INT | 0.698 | | | 0.887 |
| ANCHOR_RULE | 0.676 | | | 0.876 |
| ROUTER14_BOOL (DEC-14 unchanged) | 0.627 | 0.617 | 0.637 | 0.737 |
| ORACLE_BOOL (ceiling) | 0.844 | | | |
| CONST | 0.500 | | | |

Routing accuracy per wording, with Wilson 95% intervals:

| Router | Correct | Wilson 95% | Contrastive |
|---|---|---|---|
| R15_BOOL | 37/50 | [0.60, 0.84] | 7/7 |
| R15_INT | 39/50 | [0.65, 0.87] | 7/7 |
| R14_BOOL | 35/50 | [0.56, 0.81] | 6/7 |
| R14_INT | 31/50 | [0.48, 0.74] | 6/7 |

## What this says
1. **The new set is harder, and set-to-set variance is large.**
   - The unchanged DEC-14 router falls from 0.728 on the DEC-14 set to 0.627 on this one, a drop of about 0.10.
   - With 50 wordings written by one author, that variance is larger than the effects being tested.
2. **The relative gain holds.** On the same set, DEC-15 beats DEC-14 by +0.045, and the scope rule stays at 7/7.
3. **The remaining misses** fall into four groups:
   - Opening wordings with no seed, routed to End: "earliest", "leading", "very top".
   - End wordings routed to Opening: "wrap up", "very bottom".
   - Continues wordings with "joined", "extend" or "neighbours".
   - NotContinues wordings with "missing between" or "misplaced".

   Each fix would mean adding seeds after reading a spent set, and that loop is what produces optimistic numbers.

## Next
- Stop tuning seeds against a single new set.
- **DEC-16: leave-one-set-out evaluation.** The spent reserved sets of DEC-12 to DEC-15 hold 164 wordings. For each set, train the router without that set, then test on it. This gives a stable estimate with a real error bar and needs no new reading.
- After DEC-16, the next fresh set should come from a source independent of the author.
