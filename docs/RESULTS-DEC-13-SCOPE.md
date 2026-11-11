# DEC-13 results: 50-wording reserved set and negation scope (2026-11-10)

Pre-registration: `docs/DEC-13-SCOPE-PREREGISTRATION.md` (sha256 `3530b6ab…`, SML `4ee3887a`).
Run: RemoteOps #1486, workflow run 42902662203, job 130132438286 (Thor). The two processes gave identical rows.

## Verdict: **DEC13_FAIL** (ROUTER13_BOOL reserved 2566/4000 = 0.6415; PARTIAL bar 0.65, GO bar 0.75)

All validity checks hold:
- CONST scores 2000/4000 on every set.
- The content heads reproduce DEC-2: 886/1000 and 739/1000.
- ROUTER12_BOOL reproduces DEC-12 on the DEC-12 reserved set: 3183/4000.

| Arm | seen | DEC-12 reserved | **DEC-13 reserved (50 wordings)** | T1 | T3 |
|---|---|---|---|---|---|
| **ROUTER13_BOOL** (primary) | 0.844 | 0.844 (trained on) | **0.642** | 0.621 | 0.663 |
| ROUTER13_INT | 0.854 | 0.854 (trained on) | 0.673 | 0.666 | 0.680 |
| ROUTER12_BOOL (DEC-12 unchanged) | 0.844 | 0.796 | 0.634 | 0.591 | 0.678 |
| ORACLE_BOOL (ceiling) | 0.844 | 0.844 | 0.844 | 0.886 | 0.802 |
| CONST | 0.500 | 0.500 | 0.500 | 0.500 | 0.500 |

The T1 and T3 columns are for the DEC-13 reserved set.

Router accuracy per wording on the 50 reserved wordings, with Wilson 95% intervals:

| Router | Accuracy | Wilson 95% |
|---|---|---|
| R12_BOOL | 33/50 | [0.52, 0.78] |
| R12_INT | 36/50 | [0.58, 0.83] |
| R13_BOOL | 34/50 | [0.54, 0.79] |
| R13_INT | 36/50 | [0.58, 0.83] |

On the 9 contrastive (scope) wordings, both Boolean routers got 4/9 and R13_INT got 5/9.

## What this says
1. **DEC-12's 0.796 was an overestimate from a small set.**
   - On 50 new wordings, the same DEC-12 router scores 0.634.
   - Its routing accuracy per wording falls from 13/14 to 33/50.
   - The two-stage design still beats the one-stage heads, which plateaued at 0.55–0.60.
   - It is not at the 0.70 bar on truly varied wordings.
2. **The scope grammar did not generalise.**
   - R13 learned its own templates: 14/14 on the spent DEC-12 set, which it was trained on.
   - On new contrastive wordings it scored 4/9, the same as R12.
   - Overall it gained +0.007, and the Boolean gain is within noise.
3. **Where it fails.** The misses are mostly position vocabulary absent from the templates, which the router cannot connect to an intent:
   - "final few paragraphs", "back of the article", "after most of the paper", "conclusion", "midsection", "core … edges" (End/Middle routed as Opening or Middle);
   - NotContinues phrasings with no negation cue ("gap between", "another part", "lead somewhere other than").
   - A router of hashed words and pairs, trained on about 750 template wordings, memorises the templates. The 64-bit lexicon code does not carry "final", "last" or "back" to End.

## Next
- **DEC-14: anchor-based routing.** Each intent gets a few anchor concepts (end/last/close/final…, beginning/first/start…, middle/body/centre…, follow/next/adjacent…, gap/elsewhere/other…). Question words reach an anchor through OEWN synonymy and hypernymy instead of hashed templates.
- **Clause-scope rule.** In "X rather than / instead of / as opposed to Y", only X is read. This rule will be written and frozen in the pre-registration, before a new reserved set is written by someone other than the grammar author, if possible.
- **The DEC-13 reserved set is spent.**
