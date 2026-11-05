# DEC-2 results — Boolean decision heads on the DEC-1 bench (2026-11-05)

Pre-registration: `docs/DEC-2-BOOLEAN-DECISION-HEADS-PREREGISTRATION.md` (SML
`adf96101`, SHA-256 checked). Run: RemoteOps #1251, Actions run 42007287997,
Thor, DEC-1 bench `fec8d3c5…` verified. Two processes; TEST predictions digests
identical (`DEC2_DETERMINISM identical`).

## Verdict: **DEC2_GO** (BEAT, BEAT, PAR)

| task | SML_BOOL (primary) | SML_INT | B1 lexical | Laya EN zero-shot | mark |
|---|---|---|---|---|---|
| T1 `noul` continuation | **0.886** | 0.896 | 0.813 | 0.631 | BEAT (+0.073) |
| T2 `choice` 1 of 4 | **0.902** | 0.909 | 0.835 | 0.457 | BEAT (+0.067) |
| T3 `score` position | **0.739** (MAE 0.298) | 0.750 (0.297) | 0.744 (0.302) | 0.359 | PAR (−0.005) |

## DEV-selected heads (SML_BOOL)
- T1: 4 epochs, 7 levels (3 bit planes), keep 1/4, 648 non-zero weights, DEV 898/1000.
- T2: 8 epochs, 7 levels, keep 1/2, 1786 non-zero weights, DEV 889/1000.
- T3: 8 epochs, 7 levels, keep 1/8, 20 556 non-zero weights (3 rows), DEV 740/1000.
- Fit 0.1–3.5 s per task on Thor CPU; 0.1–0.7 µs per decision (Laya EN: 230–900 ms).

## Reading
- A small Boolean head (hashed word/bigram bits, ≤ 3 bit planes of weights,
  popcount scoring, integer training) learns T1 and T2 clearly better than raw
  overlap, and about 1e6× faster per decision than Laya on the same CPU.
- T3 (where in the article) stays at the naive Bayes level: bag-of-words cues
  are exhausted; order and discourse cues are not in the features.
- Limits stand as pre-registered: lexical features, learned per task, no
  instruction reading, no SML language-model state yet (DEC-3).
