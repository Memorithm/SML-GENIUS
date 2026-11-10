# DEC-11 results — the DEC-10 heads with a wider DEV grid (2026-11-06)

Pre-registration: `docs/DEC-11-CAPACITY-PREREGISTRATION.md` (SML
`d740f558`, SHA-256 checked). Run: RemoteOps #1286, Thor, job 127794030879.
Two processes, identical outputs.

## Verdict: **DEC11_FAIL** (reserved 2199/4000 = 0.550, bar 0.60)

Validity holds. SML_BOOL_GRAMMAR10 reproduces DEC-10 on `seen` (2722/4000),
and BLIND scores 0.5.

| arm | DEV pick | seen (DEC-4) | **reserved (read once)** | T1 | T3 |
|---|---|---|---|---|---|
| **SML_BOOL_LONG** (primary) | 8 epochs, ±15 | 0.708 | **0.550** | 0.625 | 0.475 |
| SML_INT_LONG | 32 epochs | 0.715 | 0.550 | | |
| SML_BOOL_GRAMMAR10 (= DEC-10) | 4 epochs, ±7 | 0.681 | 0.544 | 0.628 | 0.460 |

Primary arm per intent on `reserved`: Continues 0.647, NotContinues 0.603,
Opening 0.585, Middle 0.361, End 0.483.

## Reading
- **DEC-10's pass does not carry over.** On this new reserved set, the
  unchanged DEC-10 heads score 0.544, not 0.605. T1 holds across sets
  (0.645, 0.587 and 0.628 on the DEC-9, DEC-10 and DEC-11 sets). T3 swings
  with the wording set (0.554, 0.623, 0.460), and here falls below chance.
  "Neither the opening nor the closing" and "after most of the article" are
  read by their words, not their meaning.
- **Capacity alone does little.** The wider grid recovers some of the
  familiar wordings (0.681 → 0.708) and adds nothing on new ones (+0.006).
- **Diagnosis.** One linear head over (question word × content bit)
  conjunctions has to learn each new phrasing for every content bit at once.
  The question is never reduced to *what is being asked*. The next step is
  structural: first map the question to an intent (a small Boolean
  classifier over question words only), then answer with the DEC-2 content
  heads, which already reach 0.886 (T1) and 0.739 (T3) when the intent is
  known.
