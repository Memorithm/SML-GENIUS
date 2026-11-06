# DEC-12 results — two-stage instruction reader (2026-11-06)

Pre-registration: `docs/DEC-12-ROUTER-PREREGISTRATION.md` (SML `8da8a276`,
SHA-256 checked). Run: RemoteOps #1287, Thor, job 127799330178. Two
processes, identical outputs.

## Verdict: **DEC12_GO** (reserved 3183/4000 = 0.796, bar 0.70)

Validity: all checks hold. CONST scores exactly 2000/4000 on both sets. The
content heads reproduce DEC-2 exactly (T1 886/1000, T3 739/1000).

| arm | seen (DEC-4) | **reserved (read once)** | T1 | T3 |
|---|---|---|---|---|
| **ROUTER_BOOL** (primary) | 0.844 | **0.796** | 0.790 | 0.802 |
| ROUTER_INT | 0.854 | 0.705 | | |
| ORACLE_BOOL (true intent) | 0.844 | 0.844 | | |
| CONST | 0.500 | 0.500 | | |
| *previous best, one-stage (DEC-10 / DEC-11)* | *0.681 / 0.708* | *0.605 / 0.550* | | |

**Router, per wording (question only):**
- ROUTER_BOOL: 478/478 on train, 12/12 on seen, **13/14 on reserved**.
- ROUTER_INT: 11/14 on reserved.
- The one Boolean miss is "Does B skip ahead or come from elsewhere instead of
  following A?", routed to Continues. The model keys on "following A" and
  misses the scope of "instead of".
- Per intent on `reserved`: Continues 0.886, NotContinues 0.694, Opening
  0.885, Middle 0.771, End 0.749.

## Reading
- **The split is the lever that eight one-stage steps could not find.** On
  new wordings, accuracy rises from 0.55–0.60 to 0.796, within 0.05 of the
  ceiling set by the content heads (0.844). On familiar wordings it beats
  every earlier arm (0.844, against 0.839 for DEC-7). The router learns
  *what is asked* from about 480 question wordings alone. The content heads
  never have to relearn it for every passage feature.
- **Everything stays Boolean and small.** The router has 419 + 1106 non-zero
  weights at ±1..±7. The content heads are the DEC-2 heads.
- **Limits.**
  - There are only 14 reserved wordings, so one misroute costs about 0.05.
  - One author wrote both the training and the reserved wordings.
  - The smoke test on a toy lexicon printed 11/14 on this reserved set before
    the run. This was declared, and nothing was changed after it.
  - The quantized router generalized better than the integer one here (13/14
    against 11/14). With 14 wordings, that difference is not established.
- **Next.**
  - Measure the router on a much larger reserved set, to get a real error bar.
  - Teach negation scope ("instead of", "rather than").
  - Then extend the same router to more tasks: T2 (choice among 4) and new
    question types, so that SML answers instructions beyond yes/no.
