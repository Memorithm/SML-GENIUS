# DEC-3 results — SML P1 language-model features for the Boolean heads (2026-11-05)

Pre-registration: `docs/DEC-3-LM-FEATURES-PREREGISTRATION.md` (SML `af40f545`,
SHA-256 checked). Run: RemoteOps #1252, Actions run 42008266856, Thor, DEC-1
bench `fec8d3c5…`. Two processes: identical scores, predictions and P1 weights.

## Verdict: **DEC3_NO_GO** (PAR, PAR, PAR)

| task | SML_BOOL_LM | DEC-2 SML_BOOL | delta | SML_INT_LM |
|---|---|---|---|---|
| T1 continuation | 0.879 | 0.886 | −0.007 (PAR) | 0.882 |
| T2 1 of 4 | 0.908 | 0.902 | +0.006 (PAR) | 0.914 |
| T3 position | 0.739 (MAE 0.309) | 0.739 | 0.000 (PAR) | 0.738 |

- P1 model: 92 629 unique TRAIN texts, 23.2 MB, prequential TRAIN 2.664 bpc,
  weights SHA-256 `00443306…`; training 16–28 s, features 13–21 s on Thor.
- DEV moved for T3 (740 → 769/1000) but TEST did not: the extra codes did not
  generalise beyond DEV selection.

## Reading
- A small order-≤6 SML byte model trained on 23 MB adds nothing measurable to
  the lexical Boolean heads on these tasks. Its A/B boundary signal is either
  weak at this model size or already implied by the shared-word bits.
- The negative result is kept. DEC-2 heads remain the best SML decision
  heads. A stronger language-model signal would need the S5h/S6 model
  (larger, trained on GBs), not a bench-sized one.
