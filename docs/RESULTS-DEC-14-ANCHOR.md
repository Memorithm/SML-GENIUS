# DEC-14 results: anchor-based question routing (2026-11-10)

- Pre-registration: `docs/DEC-14-ANCHOR-PREREGISTRATION.md` (sha256 `27ad39e8…`, SML `94c8069b`).
- Run: RemoteOps #1487, workflow run 42903680864, job 130136001937 (Thor).
- The two processes gave identical results.

## Verdict: **DEC14_GO**

ROUTER14_BOOL scored 2912/4000 = **0.728** on the reserved set. That clears the 0.70 bar. It is also +0.080 above the DEC-12 router, against a required margin of +0.03.

Validity checks, all of which hold:
- CONST scores 2000/4000 on every set.
- The DEC-2 heads reproduce: 886/1000 and 739/1000.
- ROUTER12_BOOL reproduces DEC-13: 2537/4000.
- The OEWN expansion gave 689 anchor words.

| Arm | Reserved (50 new wordings) | T1 | T3 | Scope rows |
|---|---|---|---|---|
| **ROUTER14_BOOL** (primary) | **0.728** | 0.737 | 0.720 | 0.874 |
| ROUTER14_INT | 0.793 | 0.821 | 0.766 | 0.896 |
| ANCHOR_RULE (backoff R14_BOOL) | 0.725 | 0.731 | 0.720 | 0.874 |
| ROUTER12_BOOL (DEC-12 unchanged) | 0.648 | 0.666 | 0.631 | 0.635 |
| ORACLE_BOOL (ceiling) | 0.844 | 0.886 | 0.802 | 0.874 |
| CONST | 0.500 | 0.500 | 0.500 | 0.488 |

Routing accuracy per wording on the 50 reserved wordings, with Wilson 95% intervals:

| Router | Correct | Wilson 95% | Contrastive wordings |
|---|---|---|---|
| R14_BOOL | 41/50 | [0.69, 0.90] | 7/7 |
| R14_INT | 46/50 | [0.81, 0.97] | 7/7 |
| R12_BOOL | 33/50 | [0.52, 0.78] | 4/7 |

The anchor rule alone decided 32 of the 50 wordings and was right on 29.

## What this says
1. **Lexical knowledge carries.** OEWN anchors take the router from 33/50 to 41/50 Boolean and 46/50 integer, on wordings never seen in training. This is the first unseen-wording result whose lower Wilson bound is above 0.69.
2. **The fixed scope rule works where the learned scope grammar did not.**
   - Contrastive wordings: 7/7 with the rule, against 4/7 for DEC-12 and 4/9 for DEC-13.
   - This includes the opening/closing swaps on either side of "rather than".
3. **Quantization costs here.**
   - The integer router reaches 0.793, within 0.05 of the ceiling.
   - The ±7 Boolean router loses 5 wordings.
   - The anchor block is small and gets swamped after quantization.
4. **Remaining misses.**
   - Middle phrasings with no anchor hit: "mid-paper", "partway through", "intermediate", "neither … nor".
   - Continues phrasings where a seed is missing or misleading: "consecutive", "resume", "kept reading after".
   - Negative phrasings without a negation cue: "detached", "unconnected", "out of sequence".
   - All three gaps are lexical coverage or polarity, not architecture.

## Limits
- One author wrote the seeds and the reserved set, and many reserved wordings contain a seed word.
- The words reached only through OEWN were partly carried: "initial", "concluding" and "tail" were routed correctly. Others were not: "intermediate" and "partway".

## Next
- Keep the anchor block at full precision, or with more levels, inside the Boolean router.
- Add polarity-bearing adjectives through OEWN antonyms of the Near seeds: "connected" → "unconnected".
- Extend the router to T2 and to other question types.
- Write the next reserved set before any change. The DEC-14 reserved set is spent.
